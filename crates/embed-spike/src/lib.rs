pub mod host;
pub mod render;

use std::ffi::{c_char, CStr};
use std::time::Duration;

use carapace::engine::{Engine, PointerEvent};
use carapace::render::Renderer;
use carapace::scene::Pt;
// io-surface 0.16 is deprecated in favour of objc2-io-surface; we knowingly use it here.
#[allow(deprecated)]
use io_surface::IOSurfaceRef;

use crate::host::{CarapaceHostVTable, FfiHost};
use crate::render::{
    blit, copy_into_iosurface, init_gpu, new_offscreen, readback_rgba, render_frame, try_shared,
    GpuCtx, OffscreenTarget, Tier,
};

/// How a rendered frame reaches the caller's IOSurface.
enum Present {
    /// Tier 2 (zero CPU copy): vello renders into an `Rgba8` storage offscreen, then a GPU
    /// blit copies+reorders it into the `Bgra8` texture that aliases the IOSurface. Nothing
    /// touches the CPU. (Blit variant — chosen for robustness; see task-6-report.md.)
    Shared {
        off: OffscreenTarget,
        // Held only to keep the imported wgpu texture (and the MTLTexture aliasing the
        // IOSurface) alive for the engine's lifetime; we render through `iosurface_view`.
        #[allow(dead_code)]
        iosurface_tex: wgpu::Texture,
        iosurface_view: wgpu::TextureView,
        blitter: wgpu::util::TextureBlitter,
    },
    /// Tier 1 (fallback): render into the offscreen, read it back to CPU, swizzle-copy into
    /// the IOSurface.
    Readback { off: OffscreenTarget },
}

/// Opaque handle handed across the C ABI.
#[allow(deprecated)]
pub struct CarapaceEngine {
    gpu: GpuCtx,
    renderer: Renderer,
    engine: Engine,
    present: Present,
    surface: IOSurfaceRef,
    tier: Tier,
    w: u32,
    h: u32,
}

// SAFETY: the spike runs entirely on one thread; the IOSurfaceRef is only touched in tick()
// which is called from that same thread.
unsafe impl Send for CarapaceEngine {}

/// # Safety
/// `skin_dir` must be a valid NUL-terminated UTF-8 path. `vtable` function pointers must
/// outlive the returned engine. `surface` must be a valid IOSurface of size w×h, BGRA format,
/// that outlives the engine. Returns null on failure.
#[no_mangle]
#[allow(deprecated)]
pub unsafe extern "C" fn carapace_create(
    skin_dir: *const c_char,
    vtable: CarapaceHostVTable,
    surface: IOSurfaceRef,
    w: u32,
    h: u32,
) -> *mut CarapaceEngine {
    if skin_dir.is_null() {
        return std::ptr::null_mut();
    }
    let dir = match unsafe { CStr::from_ptr(skin_dir) }.to_str() {
        Ok(s) => std::path::PathBuf::from(s),
        Err(_) => return std::ptr::null_mut(),
    };
    let (_m, source) = match carapace::skin::load_dir(&dir) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    let engine = match Engine::new(
        Box::new(FfiHost::new(vtable)),
        carapace::vocab::VocabRegistry::base(),
        source,
    ) {
        Ok(e) => e,
        Err(_) => return std::ptr::null_mut(),
    };
    let gpu = init_gpu();
    let renderer = Renderer::new(&gpu.device);

    // Try Tier 2 (zero-copy IOSurface import) first; fall back to Tier 1 readback on any
    // failure. The IOSurface texture only needs RENDER_ATTACHMENT — the blitter renders into
    // it, so no BGRA storage feature is required.
    let (present, tier) = match unsafe {
        try_shared(&gpu.device, surface, w, h, wgpu::TextureUsages::RENDER_ATTACHMENT)
    } {
        Some(iosurface_tex) => {
            let iosurface_view =
                iosurface_tex.create_view(&wgpu::TextureViewDescriptor::default());
            let blitter =
                wgpu::util::TextureBlitter::new(&gpu.device, wgpu::TextureFormat::Bgra8Unorm);
            let off = new_offscreen(&gpu.device, w, h);
            (
                Present::Shared { off, iosurface_tex, iosurface_view, blitter },
                Tier::Shared,
            )
        }
        None => (Present::Readback { off: new_offscreen(&gpu.device, w, h) }, Tier::Readback),
    };
    Box::into_raw(Box::new(CarapaceEngine { gpu, renderer, engine, present, surface, tier, w, h }))
}

/// Tick + render one frame into the engine's target surface.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be destroyed.
#[no_mangle]
pub unsafe extern "C" fn carapace_tick(ptr: *mut CarapaceEngine, dt_seconds: f64) {
    let Some(e) = (unsafe { ptr.as_mut() }) else { return };
    let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
    // Split the borrows: render_frame needs &mut engine/renderer while the present path needs
    // &present — both live under `e`, so destructure into disjoint field borrows.
    let CarapaceEngine { gpu, renderer, engine, present, surface, w, h, .. } = e;
    let (w, h) = (*w, *h);
    match present {
        // Tier 2: render into the Rgba8 offscreen, then GPU-blit it into the IOSurface texture.
        // No CPU readback, no swizzle copy — the blitter reorders RGBA→BGRA on the GPU.
        // wait=false: blit() is the single poll on this path; skipping the render_frame stall
        // removes a redundant GPU wait and reduces Tier-2 latency.
        Present::Shared { off, iosurface_view, blitter, .. } => {
            render_frame(engine, renderer, gpu, &off.view, w, h, dt, false);
            blit(gpu, blitter, &off.view, iosurface_view);
        }
        // Tier 1: render, read back, swizzle-copy into the IOSurface.
        // wait=true: readback_rgba must see completed GPU work before it maps the buffer.
        Present::Readback { off } => {
            render_frame(engine, renderer, gpu, &off.view, w, h, dt, true);
            let rgba = readback_rgba(gpu, &off.tex, w, h);
            unsafe { copy_into_iosurface(*surface, &rgba, w, h) };
        }
    }
}

/// Forward a pointer event in canvas coordinates. kind: 0 = press (others ignored in spike).
///
/// # Safety
/// `ptr` must come from `carapace_create`.
#[no_mangle]
pub unsafe extern "C" fn carapace_pointer(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    kind: i32,
) {
    let Some(e) = (unsafe { ptr.as_mut() }) else { return };
    if kind == 0 {
        e.engine.handle_pointer_resolved(
            e.w as f32,
            e.h as f32,
            Pt { x: x as f32, y: y as f32 },
            PointerEvent::Press,
        );
    }
}

/// Returns the active tier: 1 = Readback (CPU copy), 2 = Shared (zero-copy Metal texture).
///
/// # Safety
/// `ptr` must come from `carapace_create`.
#[no_mangle]
pub unsafe extern "C" fn carapace_active_tier(ptr: *mut CarapaceEngine) -> i32 {
    match unsafe { ptr.as_ref() } {
        Some(e) => e.tier as i32,
        None => 0,
    }
}

/// # Safety
/// `ptr` must come from `carapace_create`; do not use it afterward.
#[no_mangle]
pub unsafe extern "C" fn carapace_destroy(ptr: *mut CarapaceEngine) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

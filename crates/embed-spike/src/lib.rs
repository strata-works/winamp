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
    copy_into_iosurface, init_gpu, new_offscreen, readback_rgba, render_frame, GpuCtx,
    OffscreenTarget, Tier,
};

/// Opaque handle handed across the C ABI.
#[allow(deprecated)]
pub struct CarapaceEngine {
    gpu: GpuCtx,
    renderer: Renderer,
    engine: Engine,
    off: OffscreenTarget,
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
    let off = new_offscreen(&gpu.device, w, h);
    // Task 6 will try Tier::Shared first and fall back to Readback.
    let tier = Tier::Readback;
    Box::into_raw(Box::new(CarapaceEngine { gpu, renderer, engine, off, surface, tier, w, h }))
}

/// Tick + render one frame into the engine's target surface.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be destroyed.
#[no_mangle]
pub unsafe extern "C" fn carapace_tick(ptr: *mut CarapaceEngine, dt_seconds: f64) {
    let Some(e) = (unsafe { ptr.as_mut() }) else { return };
    let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
    render_frame(&mut e.engine, &mut e.renderer, &e.gpu, &e.off.view, e.w, e.h, dt);
    if e.tier == Tier::Readback {
        let rgba = readback_rgba(&e.gpu, &e.off.tex, e.w, e.h);
        unsafe { copy_into_iosurface(e.surface, &rgba, e.w, e.h) };
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

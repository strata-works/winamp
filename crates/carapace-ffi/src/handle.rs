//! The opaque engine handle handed across the C ABI, plus create/destroy/tick.

use std::ffi::{CStr, c_char};
use std::time::Duration;

use carapace::engine::{Engine, PointerEvent};
use carapace::render::Renderer;
use carapace::scene::Pt;

use crate::guard::{
    CarapaceStatus, ffi_guard, ffi_guard_no_handle, install_panic_hook, set_last_error,
};
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::render::{
    GpuCtx, IOSurfaceGetHeight, IOSurfaceGetWidth, IOSurfaceRef, OffscreenTarget, Tier, blit,
    copy_into_iosurface, init_gpu, make_content_texture, new_offscreen, readback_rgba,
    render_frame, try_shared, upload_iosurface_to_texture,
};

/// How a rendered frame reaches the caller's IOSurface.
pub enum Present {
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

/// Host-supplied live content for a skin `view{}` cutout. We hold a NORMAL wgpu `Bgra8Unorm`
/// texture (`tex`/`view`) plus the caller-owned content `surface`. Each tick we re-read the
/// surface's current bytes and upload them into `tex` (see `carapace_tick`), so the engine
/// composites THIS frame's host content into the matching `view{ id = "host" }` rect — fixing
/// the frozen-content bug an IOSurface-aliased import causes (the GPU caches the first frame
/// and never re-reads the CPU's per-frame writes).
#[allow(deprecated)]
pub struct ContentTex {
    pub surface: IOSurfaceRef,
    #[allow(dead_code)]
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

/// Opaque handle handed across the C ABI. `poisoned` is set by `ffi_guard!` after a caught panic;
/// every subsequent call short-circuits with `ErrPoisoned`.
#[allow(deprecated)]
pub struct CarapaceEngine {
    pub gpu: GpuCtx,
    pub renderer: Renderer,
    pub engine: Engine,
    pub present: Present,
    pub surface: IOSurfaceRef,
    pub content: Option<ContentTex>,
    pub tier: Tier,
    pub w: u32,
    pub h: u32,
    pub cw: u32,
    pub ch: u32,
    pub poisoned: bool,
}

// SAFETY: single-threaded handle; the IOSurfaceRef is only touched on the calling thread.
unsafe impl Send for CarapaceEngine {}

/// Parameters for `carapace_create`. Grouped in a struct so create can grow additively.
#[repr(C)]
pub struct CarapaceCreateDesc {
    /// NUL-terminated UTF-8 skin directory path (borrowed for the call).
    pub skin_dir: *const c_char,
    /// Host callbacks (fn pointers must outlive the engine).
    pub vtable: CarapaceHostVTable,
    /// Caller-owned BGRA IOSurface of size `w`x`h` that outlives the engine.
    pub surface: IOSurfaceRef,
    /// Optional live host content for a `view{ id = "host" }` cutout; null = none.
    pub content_surface: IOSurfaceRef,
    pub w: u32,
    pub h: u32,
}

/// Try Tier 2 (zero-copy IOSurface import) first; fall back to Tier 1 readback on any failure.
/// The IOSurface texture only needs RENDER_ATTACHMENT — the blitter renders into it, so no BGRA
/// storage feature is required. Lifted verbatim from the spike's `carapace_create`.
fn build_present(gpu: &GpuCtx, surface: IOSurfaceRef, w: u32, h: u32) -> (Present, Tier) {
    match unsafe {
        try_shared(
            &gpu.device,
            surface,
            w,
            h,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
    } {
        Some(iosurface_tex) => {
            let iosurface_view = iosurface_tex.create_view(&wgpu::TextureViewDescriptor::default());
            let blitter =
                wgpu::util::TextureBlitter::new(&gpu.device, wgpu::TextureFormat::Bgra8Unorm);
            let off = new_offscreen(&gpu.device, w, h);
            (
                Present::Shared {
                    off,
                    iosurface_tex,
                    iosurface_view,
                    blitter,
                },
                Tier::Shared,
            )
        }
        None => (
            Present::Readback {
                off: new_offscreen(&gpu.device, w, h),
            },
            Tier::Readback,
        ),
    }
}

/// Optionally import the host's content IOSurface as a sampled texture for the skin's
/// `view{ id = "host" }` cutout. Null surface, a failed import, or zero dimensions all yield
/// None (the cutout simply shows nothing). NEVER panic. Lifted verbatim from the spike's
/// `carapace_create`.
fn build_content(gpu: &GpuCtx, content_surface: IOSurfaceRef) -> Option<ContentTex> {
    if content_surface.is_null() {
        None
    } else {
        let (cw, ch) = unsafe {
            (
                IOSurfaceGetWidth(content_surface) as u32,
                IOSurfaceGetHeight(content_surface) as u32,
            )
        };
        if cw == 0 || ch == 0 {
            None
        } else {
            // A NORMAL wgpu texture we re-upload the content surface into every tick
            // (CPU→GPU coherency). No IOSurface aliasing here — that's what froze the
            // content before.
            let (tex, view) = make_content_texture(&gpu.device, cw, ch);
            Some(ContentTex {
                surface: content_surface,
                tex,
                view,
                w: cw,
                h: ch,
            })
        }
    }
}

/// Create an engine. Returns a status; on `Ok`, `*out` receives the handle (else stays null).
///
/// # Safety
/// `desc` must be a valid pointer; its `skin_dir` a valid NUL-terminated UTF-8 path; `surface` a
/// live `w`x`h` BGRA IOSurface outliving the engine; `vtable` fn pointers outliving the engine.
/// `out` must be a valid pointer to a `*mut CarapaceEngine`.
#[unsafe(no_mangle)]
#[allow(deprecated)]
pub unsafe extern "C" fn carapace_create(
    desc: *const CarapaceCreateDesc,
    out: *mut *mut CarapaceEngine,
) -> CarapaceStatus {
    install_panic_hook();
    if out.is_null() {
        return CarapaceStatus::ErrNullArg;
    }
    unsafe { *out = std::ptr::null_mut() };
    ffi_guard_no_handle!({
        let Some(desc) = (unsafe { desc.as_ref() }) else {
            set_last_error("carapace_create: null desc");
            return CarapaceStatus::ErrNullArg;
        };
        if desc.skin_dir.is_null() {
            set_last_error("carapace_create: null skin_dir");
            return CarapaceStatus::ErrNullArg;
        }
        let dir = match unsafe { CStr::from_ptr(desc.skin_dir) }.to_str() {
            Ok(s) => std::path::PathBuf::from(s),
            Err(_) => {
                set_last_error("carapace_create: skin_dir is not valid UTF-8");
                return CarapaceStatus::ErrNullArg;
            }
        };
        let (_m, source) = match carapace::skin::load_dir(&dir) {
            Ok(v) => v,
            Err(e) => {
                set_last_error(&format!("carapace_create: skin load failed: {e:?}"));
                return CarapaceStatus::ErrBadSkin;
            }
        };
        let engine = match Engine::new(
            Box::new(FfiHost::new(desc.vtable)),
            carapace::vocab::VocabRegistry::base(),
            source,
        ) {
            Ok(e) => e,
            Err(e) => {
                set_last_error(&format!("carapace_create: engine init failed: {e:?}"));
                return CarapaceStatus::ErrBadSkin;
            }
        };
        let (cw, ch) = engine.scene().canvas;

        let gpu = match init_gpu() {
            Ok(g) => g,
            Err(msg) => {
                set_last_error(&format!("carapace_create: {msg}"));
                return CarapaceStatus::ErrGpuInit;
            }
        };
        let renderer = Renderer::new(&gpu.device);

        // Tier 2 (zero-copy) with Tier 1 fallback — see `build_present`.
        let (present, tier) = build_present(&gpu, desc.surface, desc.w, desc.h);

        // Optional host content view — see `build_content`.
        let content = build_content(&gpu, desc.content_surface);

        let handle = Box::into_raw(Box::new(CarapaceEngine {
            gpu,
            renderer,
            engine,
            present,
            surface: desc.surface,
            content,
            tier,
            w: desc.w,
            h: desc.h,
            cw,
            ch,
            poisoned: false,
        }));
        unsafe { *out = handle };
        CarapaceStatus::Ok
    })
}

/// Destroy an engine created by `carapace_create`. Null-safe; valid on a poisoned handle.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be used afterward.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_destroy(ptr: *mut CarapaceEngine) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

/// The present path the engine resolved to. Mirrors `render::Tier`.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapaceTier {
    Readback = 1,
    Shared = 2,
}

/// Tick + render one frame, lifted verbatim from the spike's `carapace_tick` body: split the
/// borrows so `render_frame` can hold `&mut engine`/`&mut renderer` while the present path holds
/// `&present`, upload this frame's host content (CPU→GPU coherency fix), then present via the
/// Shared blit path or the Readback CPU-copy path.
fn tick_inner(e: &mut CarapaceEngine, dt: Duration) {
    let CarapaceEngine {
        gpu,
        renderer,
        engine,
        present,
        surface,
        content,
        w,
        h,
        ..
    } = e;
    let (w, h) = (*w, *h);
    // Upload THIS frame's host content into the content texture before rendering, so the
    // engine samples fresh bytes (the CPU→GPU coherency fix). Then supply that texture for
    // the skin's `view{ id = "host" }` cutout.
    if let Some(c) = content.as_ref() {
        unsafe { upload_iosurface_to_texture(&gpu.queue, c.surface, &c.tex, c.w, c.h) };
    }
    let host_view = content.as_ref().map(|c| ("host", &c.view));
    match present {
        // Tier 2: render into the Rgba8 offscreen, then GPU-blit it into the IOSurface
        // texture. No CPU readback, no swizzle copy — the blitter reorders RGBA→BGRA on
        // the GPU. wait=false: blit() is the single poll on this path; skipping the
        // render_frame stall removes a redundant GPU wait and reduces Tier-2 latency.
        Present::Shared {
            off,
            iosurface_view,
            blitter,
            ..
        } => {
            render_frame(engine, renderer, gpu, &off.view, w, h, dt, false, host_view);
            blit(gpu, blitter, &off.view, iosurface_view);
        }
        // Tier 1: render, read back, swizzle-copy into the IOSurface.
        // wait=true: readback_rgba must see completed GPU work before it maps the buffer.
        Present::Readback { off } => {
            render_frame(engine, renderer, gpu, &off.view, w, h, dt, true, host_view);
            let rgba = readback_rgba(gpu, &off.tex, w, h);
            unsafe { copy_into_iosurface(*surface, &rgba, w, h) };
        }
    }
}

/// Tick + render one frame into the engine's surface. `dt_seconds` is host wall-clock time.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_tick(
    ptr: *mut CarapaceEngine,
    dt_seconds: f64,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    ffi_guard!(ptr, {
        let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
        tick_inner(e, dt);
        CarapaceStatus::Ok
    })
}

/// Report the active present tier.
///
/// # Safety
/// `ptr` must come from `carapace_create`; `out` must be a valid pointer to a `CarapaceTier`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_active_tier(
    ptr: *mut CarapaceEngine,
    out: *mut CarapaceTier,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if out.is_null() {
        return CarapaceStatus::ErrNullArg;
    }
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    let tier = match e.tier {
        Tier::Readback => CarapaceTier::Readback,
        Tier::Shared => CarapaceTier::Shared,
    };
    unsafe { *out = tier };
    CarapaceStatus::Ok
}

/// Pointer event kinds. v1 forwards all; the engine currently acts on `Press`, the rest are
/// plumbed for hover/drag semantics and forward-compat (additive).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapacePointerKind {
    Press = 0,
    Release = 1,
    Move = 2,
    Enter = 3,
    Leave = 4,
}

/// Forward a pointer event in DESIGN-CANVAS coordinates.
///
/// # Safety
/// `ptr` must come from `carapace_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_pointer(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    kind: CarapacePointerKind,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    ffi_guard!(ptr, {
        let event = match kind {
            CarapacePointerKind::Press => Some(PointerEvent::Press),
            // The engine models Press today; map the rest to the nearest existing event it accepts.
            // Until the engine grows release/move/enter/leave, forward only Press and treat the
            // others as no-ops (still validated + guarded). This stays additive: when the engine
            // gains those events, extend this match — no ABI change.
            _ => None,
        };
        if let Some(ev) = event {
            e.engine.handle_pointer_resolved(
                e.cw as f32,
                e.ch as f32,
                Pt {
                    x: x as f32,
                    y: y as f32,
                },
                ev,
            );
        }
        CarapaceStatus::Ok
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::host::CarapaceHostVTable;

    fn empty_vtable() -> CarapaceHostVTable {
        CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
        }
    }

    #[test]
    fn create_rejects_null_out_and_null_skin_dir() {
        // null out
        let desc = CarapaceCreateDesc {
            skin_dir: std::ptr::null(),
            vtable: empty_vtable(),
            surface: std::ptr::null_mut(),
            content_surface: std::ptr::null_mut(),
            w: 4,
            h: 4,
        };
        let status = unsafe { carapace_create(&desc, std::ptr::null_mut()) };
        assert_eq!(status, CarapaceStatus::ErrNullArg);
        // null skin_dir, valid out
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        let status = unsafe { carapace_create(&desc, &mut handle) };
        assert_eq!(status, CarapaceStatus::ErrNullArg);
        assert!(handle.is_null());
    }

    #[test]
    fn create_reports_bad_skin_for_missing_dir() {
        let path = std::ffi::CString::new("/no/such/skin/dir").unwrap();
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(),
            vtable: empty_vtable(),
            surface: std::ptr::null_mut(),
            content_surface: std::ptr::null_mut(),
            w: 4,
            h: 4,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        let status = unsafe { carapace_create(&desc, &mut handle) };
        assert_eq!(status, CarapaceStatus::ErrBadSkin);
        assert!(handle.is_null());
    }
}

/// End-to-end: create an engine against a real skin fixture, tick it once, and confirm the
/// engine actually painted non-zero pixels into the caller's IOSurface.
#[cfg(all(test, target_os = "macos"))]
mod tick_tests {
    use super::*;
    use crate::host::CarapaceHostVTable;
    use crate::render::{
        IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceUnlock,
    };

    // A workspace-sibling demo skin (300x140 canvas, visible content) — kept independent of the
    // frozen embed-spike crate.
    const SKIN_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../carapace-demo/skins/classic"
    );

    /// Build a caller-owned BGRA8 IOSurface of size `w`x`h` via the `io-surface` crate.
    ///
    /// `io_surface::new` returns an owning `IOSurface` wrapper (drop => `CFRelease`); we must NOT
    /// let that wrapper drop before the test is done with the raw ref, so we `mem::forget` it and
    /// intentionally leak the +1 Core Foundation reference for the lifetime of the test process.
    #[allow(deprecated)] // `io_surface` (test-only dev-dep) is deprecated upstream in favor of
    // `objc2-io-surface`; kept here only for its convenient IOSurface-creation + lock API.
    fn make_bgra_iosurface(w: usize, h: usize) -> IOSurfaceRef {
        use core_foundation::base::TCFType;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::number::CFNumber;
        use core_foundation::string::CFString;
        let props = CFDictionary::from_CFType_pairs(&[
            (
                CFString::new("IOSurfaceWidth"),
                CFNumber::from(w as i64).as_CFType(),
            ),
            (
                CFString::new("IOSurfaceHeight"),
                CFNumber::from(h as i64).as_CFType(),
            ),
            (
                CFString::new("IOSurfaceBytesPerElement"),
                CFNumber::from(4i64).as_CFType(),
            ),
            (
                CFString::new("IOSurfacePixelFormat"),
                CFNumber::from(0x42475241i64 /* 'BGRA' */).as_CFType(),
            ),
        ]);
        let owned = io_surface::new(&props);
        let raw = owned.as_concrete_TypeRef();
        std::mem::forget(owned); // keep the surface alive; the test owns it for its whole run
        raw as IOSurfaceRef
    }

    /// Lock `surface` read-only and scan its `w`x`h` BGRA8 bytes for any non-zero byte.
    ///
    /// # Safety
    /// `surface` must be a live, lockable IOSurface of at least `w`x`h` BGRA8 pixels.
    unsafe fn iosurface_has_nonzero_pixels(surface: IOSurfaceRef, w: u32, h: u32) -> bool {
        let mut seed: u32 = 0;
        unsafe {
            IOSurfaceLock(surface, 0x1 /* kIOSurfaceLockReadOnly */, &mut seed)
        };
        let base = unsafe { IOSurfaceGetBaseAddress(surface) } as *const u8;
        let stride = unsafe { IOSurfaceGetBytesPerRow(surface) };
        let row_bytes = (w * 4) as usize;
        let mut nonzero = false;
        for y in 0..h as usize {
            let row = unsafe { std::slice::from_raw_parts(base.add(y * stride), row_bytes) };
            if row.iter().any(|&b| b != 0) {
                nonzero = true;
                break;
            }
        }
        unsafe { IOSurfaceUnlock(surface, 0x1, &mut seed) };
        nonzero
    }

    #[test]
    fn create_tick_destroy_renders_nonblank() {
        let (w, h) = (300u32, 140u32);
        let surface = make_bgra_iosurface(w as usize, h as usize);
        let path = std::ffi::CString::new(SKIN_DIR).unwrap();
        let vtable = CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
        };
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(),
            vtable,
            surface,
            content_surface: std::ptr::null_mut(),
            w,
            h,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        assert_eq!(
            unsafe { carapace_create(&desc, &mut handle) },
            CarapaceStatus::Ok
        );
        assert!(!handle.is_null());

        assert_eq!(unsafe { carapace_tick(handle, 0.016) }, CarapaceStatus::Ok);

        let mut tier = CarapaceTier::Readback;
        assert_eq!(
            unsafe { carapace_active_tier(handle, &mut tier) },
            CarapaceStatus::Ok
        );
        assert!(matches!(
            tier,
            CarapaceTier::Readback | CarapaceTier::Shared
        ));

        // The skin should have drawn something — the surface can't still be all zero bytes.
        let nonzero = unsafe { iosurface_has_nonzero_pixels(surface, w, h) };
        assert!(nonzero, "expected the skin to render visible pixels");

        unsafe { carapace_destroy(handle) };
    }

    #[test]
    fn pointer_press_returns_ok_and_null_is_rejected() {
        // null handle
        assert_eq!(
            unsafe { carapace_pointer(std::ptr::null_mut(), 1.0, 1.0, CarapacePointerKind::Press) },
            CarapaceStatus::ErrNullArg
        );
    }

    // Records whether `host.toggle_play()` (the classic skin's play-button handler) fired.
    // A plain static bool is fine here: it's touched only by this single test.
    static PRESSED_TOGGLE_PLAY: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    extern "C" fn record_invoke(_ctx: *mut std::ffi::c_void, action: *const std::ffi::c_char) {
        let name = unsafe { std::ffi::CStr::from_ptr(action) }.to_string_lossy();
        if name == "toggle_play" {
            PRESSED_TOGGLE_PLAY.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn pointer_press_over_hotspot_dispatches_through_tick() {
        PRESSED_TOGGLE_PLAY.store(false, std::sync::atomic::Ordering::SeqCst);

        let (w, h) = (300u32, 140u32);
        let surface = make_bgra_iosurface(w as usize, h as usize);
        let path = std::ffi::CString::new(SKIN_DIR).unwrap();
        let vtable = CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: Some(record_invoke),
        };
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(),
            vtable,
            surface,
            content_surface: std::ptr::null_mut(),
            w,
            h,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        assert_eq!(
            unsafe { carapace_create(&desc, &mut handle) },
            CarapaceStatus::Ok
        );
        assert!(!handle.is_null());

        // The play button hotspot is `fill{ path = rect{x=20,y=20,w=70,h=70}, ... on_press =
        // function() host.toggle_play() end }` in skin.lua, in DESIGN-CANVAS (300x140) coords.
        // (55, 55) sits well inside it.
        assert_eq!(
            unsafe { carapace_pointer(handle, 55.0, 55.0, CarapacePointerKind::Press) },
            CarapaceStatus::Ok
        );
        // The press only enqueues the handler; `carapace_tick` drains the queue and invokes it
        // through the host vtable.
        assert_eq!(unsafe { carapace_tick(handle, 0.016) }, CarapaceStatus::Ok);

        assert!(
            PRESSED_TOGGLE_PLAY.load(std::sync::atomic::Ordering::SeqCst),
            "expected a press over the play button to fire host.toggle_play()"
        );

        unsafe { carapace_destroy(handle) };
    }
}

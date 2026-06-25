pub mod host;
pub mod render;

use std::ffi::{c_char, CStr};
use std::time::Duration;

use carapace::engine::{Engine, PointerEvent};
use carapace::render::Renderer;
use carapace::scene::Pt;

use crate::host::{CarapaceHostVTable, FfiHost};
use crate::render::{init_gpu, new_offscreen, render_frame, GpuCtx, OffscreenTarget};

/// Opaque handle handed across the C ABI.
pub struct CarapaceEngine {
    gpu: GpuCtx,
    renderer: Renderer,
    engine: Engine,
    target: OffscreenTarget, // Task 4 swaps this for a Present enum (offscreen | iosurface)
    w: u32,
    h: u32,
}

/// # Safety
/// `skin_dir` must be a valid NUL-terminated UTF-8 path. `vtable` function pointers must
/// outlive the returned engine. Returns null on failure.
#[no_mangle]
pub unsafe extern "C" fn carapace_create(
    skin_dir: *const c_char,
    vtable: CarapaceHostVTable,
    w: u32,
    h: u32,
) -> *mut CarapaceEngine {
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
    let target = new_offscreen(&gpu.device, w, h);
    Box::into_raw(Box::new(CarapaceEngine { gpu, renderer, engine, target, w, h }))
}

/// Tick + render one frame into the engine's target.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be destroyed.
#[no_mangle]
pub unsafe extern "C" fn carapace_tick(ptr: *mut CarapaceEngine, dt_seconds: f64) {
    let Some(e) = (unsafe { ptr.as_mut() }) else { return };
    let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
    render_frame(&mut e.engine, &mut e.renderer, &e.gpu, &e.target.view, e.w, e.h, dt);
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

/// # Safety
/// `ptr` must come from `carapace_create`; do not use it afterward.
#[no_mangle]
pub unsafe extern "C" fn carapace_destroy(ptr: *mut CarapaceEngine) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

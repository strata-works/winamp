//! The opaque engine handle handed across the C ABI, plus create/destroy.

use std::ffi::{CStr, c_char};

use carapace::engine::Engine;
use carapace::render::Renderer;

use crate::guard::{CarapaceStatus, ffi_guard_no_handle, install_panic_hook, set_last_error};
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::render::{
    GpuCtx, IOSurfaceGetHeight, IOSurfaceGetWidth, IOSurfaceRef, OffscreenTarget, Tier, init_gpu,
    make_content_texture, new_offscreen, try_shared,
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

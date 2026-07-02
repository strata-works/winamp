//! The opaque engine handle handed across the C ABI, plus create/destroy.
//!
//! In v2 the handle is a thread-safe FRONT-END: `carapace_create` spawns a dedicated render thread
//! that constructs and owns the `!Send` engine + GPU (see `render_thread`); this handle only holds
//! the command sender, the published-snapshot cell, an atomic poison flag, and the thread's join
//! handle. `carapace_destroy` signals shutdown and joins.

use std::ffi::{CStr, c_char, c_void};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;

use crate::guard::{CarapaceStatus, ffi_guard_no_handle, install_panic_hook, set_last_error};
use crate::host::CarapaceHostVTable;
use crate::queue::{Command, CommandTx};
use crate::render::{IOSurfaceRef, Tier};
use crate::render_thread::{self, SendSurfaces};
use crate::snapshot::{self, SnapshotCell};

/// Host-supplied live content for a skin `view{}` cutout. We hold a NORMAL wgpu `Bgra8Unorm`
/// texture (`tex`/`view`) plus the caller-owned content `surface`. Each frame the render thread
/// re-reads the surface's current bytes and uploads them into `tex`, so the engine composites THIS
/// frame's host content into the matching `view{ id = "host" }` rect — fixing the frozen-content
/// bug an IOSurface-aliased import causes (the GPU caches the first frame and never re-reads the
/// CPU's per-frame writes). Constructed on the render thread (`render::build_content`).
// SDD v2: the render thread's present path reads these every frame (Tasks 5/6); until then only
// `render::build_content` constructs a `ContentTex`, so allow the interim "constructed but not read".
#[allow(deprecated)]
#[allow(dead_code)]
pub(crate) struct ContentTex {
    pub surface: IOSurfaceRef,
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

/// Opaque handle handed across the C ABI — a thread-safe front-end for the render thread.
///
/// `poisoned` is set (with `Release`) by the render thread / guard after a caught panic; front-end
/// query exports read it with `Acquire` and short-circuit with `ErrPoisoned`.
pub struct CarapaceEngine {
    /// Enqueues commands (pointer, invalidate, frame-rate, shutdown) onto the render thread.
    pub tx: CommandTx,
    /// The render thread's latest published scene + tier; read lock-free-ish by query exports.
    pub snapshot: SnapshotCell,
    /// Set after a caught panic; every subsequent handle call short-circuits with `ErrPoisoned`.
    pub poisoned: Arc<AtomicBool>,
    /// Joined by `carapace_destroy`. `Option` so destroy can `take` + join exactly once.
    pub join: Option<JoinHandle<()>>,
    /// The present tier resolved at create time, for an immediate `active_tier` answer.
    pub tier: CarapaceTier,
}

/// Parameters for `carapace_create`. Grouped in a struct so create can grow additively.
#[repr(C)]
pub struct CarapaceCreateDesc {
    /// NUL-terminated UTF-8 skin directory path (borrowed for the call).
    pub skin_dir: *const c_char,
    /// Host callbacks (fn pointers must outlive the engine).
    pub vtable: CarapaceHostVTable,
    /// Pointer to a caller-owned array of `surface_count` BGRA IOSurfaces, each of size `w`x`h`,
    /// that outlive the engine. The render thread rotates through this pool.
    pub surfaces: *const IOSurfaceRef,
    /// Number of surfaces in `surfaces`; must be >= 1.
    pub surface_count: u32,
    /// Optional live host content for a `view{ id = "host" }` cutout; null = none.
    pub content_surface: IOSurfaceRef,
    pub w: u32,
    pub h: u32,
}

/// The present path the engine resolved to. Mirrors `render::Tier`.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapaceTier {
    Readback = 1,
    Shared = 2,
}

/// Create an engine. Returns a status; on `Ok`, `*out` receives the handle (else stays null).
///
/// Spawns a dedicated render thread that constructs and owns the engine + GPU, then BLOCKS on the
/// thread's init handshake so this call still reports `ErrBadSkin`/`ErrGpuInit`/`Ok` synchronously.
///
/// # Safety
/// `desc` must be a valid pointer; its `skin_dir` a valid NUL-terminated UTF-8 path; `surfaces` a
/// valid array of `surface_count` live `w`x`h` BGRA IOSurfaces outliving the engine; `vtable` fn
/// pointers outliving the engine. `out` must be a valid pointer to a `*mut CarapaceEngine`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_create(
    desc: *const CarapaceCreateDesc,
    out: *mut *mut CarapaceEngine,
) -> CarapaceStatus {
    install_panic_hook();
    if out.is_null() {
        set_last_error("carapace_create: null out pointer");
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

        let count = desc.surface_count as usize;
        if count == 0 {
            set_last_error("carapace_create: surface_count must be >= 1");
            return CarapaceStatus::ErrNullArg;
        }
        if desc.surfaces.is_null() {
            set_last_error("carapace_create: null surfaces");
            return CarapaceStatus::ErrNullArg;
        }
        let surfaces: Vec<*const c_void> = (0..count)
            .map(|i| unsafe { *desc.surfaces.add(i) } as *const c_void)
            .collect();
        let send_surfaces = SendSurfaces {
            surfaces,
            content: desc.content_surface as *const c_void,
            vtable: desc.vtable,
        };

        let (tx, rx) = std::sync::mpsc::channel::<Command>();
        let poisoned = Arc::new(AtomicBool::new(false));
        // Provisional tier; refined below once the render thread reports what it resolved to.
        let cell = snapshot::new_cell(snapshot::SnapshotTier::Shared);
        let (init_tx, init_rx) = std::sync::mpsc::channel::<render_thread::InitResult>();
        let join = render_thread::spawn(
            dir,
            send_surfaces,
            desc.w,
            desc.h,
            rx,
            cell.clone(),
            poisoned.clone(),
            init_tx,
        );

        // Block on the init handshake so create returns a synchronous status.
        match init_rx.recv() {
            Ok(render_thread::InitResult::Ok { tier }) => {
                let (ctier, stier) = match tier {
                    Tier::Readback => (CarapaceTier::Readback, snapshot::SnapshotTier::Readback),
                    Tier::Shared => (CarapaceTier::Shared, snapshot::SnapshotTier::Shared),
                };
                // Seed the snapshot's tier so `active_tier` is correct before frame 1.
                snapshot::publish_tier_only(&cell, stier);
                let handle = Box::into_raw(Box::new(CarapaceEngine {
                    tx,
                    snapshot: cell,
                    poisoned,
                    join: Some(join),
                    tier: ctier,
                }));
                unsafe { *out = handle };
                CarapaceStatus::Ok
            }
            Ok(render_thread::InitResult::Err(status, msg)) => {
                set_last_error(&msg);
                let _ = join.join();
                status
            }
            Err(_) => {
                set_last_error("carapace_create: render thread died during init");
                let _ = join.join();
                CarapaceStatus::ErrPanic
            }
        }
    })
}

/// Destroy an engine created by `carapace_create`. Null-safe; valid on a poisoned/exited handle.
/// Signals the render thread to shut down and joins it.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be used afterward.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_destroy(ptr: *mut CarapaceEngine) {
    if ptr.is_null() {
        return;
    }
    let mut engine = unsafe { Box::from_raw(ptr) };
    // The thread may already be gone (init failure re-homed, panic); ignore a send error.
    let _ = engine.tx.send(Command::Shutdown);
    if let Some(join) = engine.join.take() {
        let _ = join.join();
    }
}

/// Enqueue a render of exactly one frame now (wakes a paused engine — see `carapace_set_frame_rate`).
/// Real pacing (free-run at `fps`, coalescing) lands in Task 6; for now this is a thin `tx.send`.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_invalidate(ptr: *mut CarapaceEngine) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return CarapaceStatus::ErrPoisoned;
    }
    let _ = e.tx.send(Command::Invalidate);
    CarapaceStatus::Ok
}

/// Set the free-run target frame rate; `0` pauses the render thread (it then only renders on
/// `carapace_invalidate`/pointer events). Real pacing behavior lands in Task 6; for now this is a
/// thin `tx.send`.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_set_frame_rate(
    ptr: *mut CarapaceEngine,
    fps: u32,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return CarapaceStatus::ErrPoisoned;
    }
    let _ = e.tx.send(Command::SetFrameRate(fps));
    CarapaceStatus::Ok
}

/// Tell the render thread the host is done displaying `surfaces[index]`; it may be rendered into
/// again.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_release_surface(
    ptr: *mut CarapaceEngine,
    index: u32,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return CarapaceStatus::ErrPoisoned;
    }
    let _ = e.tx.send(Command::ReleaseSurface(index));
    CarapaceStatus::Ok
}

/// Pointer event kind, mirrored 1:1 by the Rust `queue::PointerKind` the render thread consumes.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapacePointerKind {
    Press = 0,
    Release = 1,
    Move = 2,
    Enter = 3,
    Leave = 4,
}

/// Forward a pointer event, in DESIGN-CANVAS coordinates, to the render thread. Non-blocking: this
/// enqueues `Command::Pointer` and returns immediately — the render thread applies it (and renders a
/// frame) the next time it drains the queue. Thin enough it doesn't need a panic guard of its own;
/// any panic it could cause happens later, on the render thread, inside `render_guarded`.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_pointer(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    kind: CarapacePointerKind,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return CarapaceStatus::ErrPoisoned;
    }
    let k = match kind {
        CarapacePointerKind::Press => crate::queue::PointerKind::Press,
        CarapacePointerKind::Release => crate::queue::PointerKind::Release,
        CarapacePointerKind::Move => crate::queue::PointerKind::Move,
        CarapacePointerKind::Enter => crate::queue::PointerKind::Enter,
        CarapacePointerKind::Leave => crate::queue::PointerKind::Leave,
    };
    let _ = e.tx.send(Command::Pointer { x, y, kind: k });
    CarapaceStatus::Ok
}

/// Test-only: enqueue a forced panic on the render thread, to prove the panic→poison→`ErrPoisoned`
/// contract end-to-end (see `render_thread::render_guarded` and `Command::ForcePanic`). Non-blocking,
/// like `carapace_pointer`. Never compiled into a shipping build — `cbindgen.toml` also excludes it
/// defensively, since cbindgen parses source statically and won't evaluate `#[cfg(test)]`.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`.
#[cfg(all(test, target_os = "macos"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_force_panic(ptr: *mut CarapaceEngine) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return CarapaceStatus::ErrPoisoned;
    }
    let _ = e.tx.send(Command::ForcePanic);
    CarapaceStatus::Ok
}

/// Report the present tier the engine is currently using, read from the render thread's published
/// snapshot (seeded at create time via `publish_tier_only`, so this is a valid answer immediately
/// after `carapace_create` returns and after every subsequent frame). Panic-free (a lock read + a
/// match), so no panic guard is needed.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`. `out`
/// must be a valid pointer to a `CarapaceTier`.
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
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return CarapaceStatus::ErrPoisoned;
    }
    let tier = match snapshot::tier_of(&e.snapshot) {
        snapshot::SnapshotTier::Readback => CarapaceTier::Readback,
        snapshot::SnapshotTier::Shared => CarapaceTier::Shared,
    };
    unsafe { *out = tier };
    CarapaceStatus::Ok
}

/// Test helpers shared by the lifecycle suite: a real skin fixture + a pool of IOSurfaces, so each
/// suite doesn't hand-roll its own `carapace_create` call.
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod test_support {
    use super::*;
    use crate::host::CarapaceHostVTable;
    use crate::render::IOSurfaceRef;

    /// A workspace-sibling demo skin (300x140 canvas, visible content) — kept independent of the
    /// frozen embed-spike crate.
    pub(crate) const SKIN_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../carapace-demo/skins/classic"
    );

    /// Poll `cond` every 5ms until it returns `true` or `timeout` elapses; returns whether the
    /// condition was met. Used by the render-thread tests to wait on the asynchronously-produced
    /// first frame instead of a fixed sleep — the first GPU frame's one-time pipeline-compile cost
    /// is highly variable under parallel GPU-test load, so a fixed sleep is inherently flaky.
    pub(crate) fn wait_for(timeout: std::time::Duration, mut cond: impl FnMut() -> bool) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if cond() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        cond()
    }

    pub(crate) fn empty_vtable() -> CarapaceHostVTable {
        CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: None,
        }
    }

    /// Build a caller-owned BGRA8 IOSurface of size `w`x`h` via the `io-surface` crate.
    ///
    /// `io_surface::new` returns an owning `IOSurface` wrapper (drop => `CFRelease`); we must NOT
    /// let that wrapper drop before the test is done with the raw ref, so we `mem::forget` it and
    /// intentionally leak the +1 Core Foundation reference for the lifetime of the test process.
    #[allow(deprecated)] // `io_surface` (test-only dev-dep) is deprecated upstream in favor of
    // `objc2-io-surface`; kept here only for its convenient IOSurface-creation + lock API.
    pub(crate) fn make_bgra_iosurface(w: usize, h: usize) -> IOSurfaceRef {
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

    /// Create a handle against the shared classic-skin fixture with a POOL of `count` surfaces and
    /// an empty (no-op) vtable. Returns the handle plus the surfaces (kept alive by the caller).
    pub(crate) fn create_test_handle_pool(
        w: u32,
        h: u32,
        count: usize,
    ) -> (*mut CarapaceEngine, Vec<IOSurfaceRef>) {
        create_test_handle_pool_vt(w, h, count, empty_vtable())
    }

    /// Like `create_test_handle_pool`, but with a caller-supplied vtable (e.g. to observe
    /// `frame_ready`). Returns the handle plus the surfaces (kept alive by the caller).
    pub(crate) fn create_test_handle_pool_vt(
        w: u32,
        h: u32,
        count: usize,
        vtable: CarapaceHostVTable,
    ) -> (*mut CarapaceEngine, Vec<IOSurfaceRef>) {
        let surfaces: Vec<IOSurfaceRef> = (0..count)
            .map(|_| make_bgra_iosurface(w as usize, h as usize))
            .collect();
        let path = std::ffi::CString::new(SKIN_DIR).unwrap();
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(),
            vtable,
            surfaces: surfaces.as_ptr(),
            surface_count: count as u32,
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
        (handle, surfaces)
    }

    /// Lock a caller-owned BGRA8 IOSurface and check whether ANY byte in its `w`x`h` extent is
    /// non-zero — a cheap "did something actually render" probe for the render-thread tests.
    ///
    /// # Safety
    /// `surface` must be a valid, live IOSurface of at least `w`x`h` BGRA8 pixels.
    pub(crate) unsafe fn iosurface_has_nonzero_pixels(
        surface: IOSurfaceRef,
        w: u32,
        h: u32,
    ) -> bool {
        use crate::render::{
            IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceUnlock,
        };
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
}

#[cfg(all(test, target_os = "macos"))]
mod lifecycle_tests {
    use super::test_support::create_test_handle_pool;
    use super::*;

    #[test]
    fn create_spawns_thread_and_destroy_joins_cleanly() {
        let (handle, _surfaces) = create_test_handle_pool(300, 140, 3);
        // No tick call exists anymore; create alone must produce a live handle.
        unsafe { carapace_destroy(handle) }; // must join the render thread without hanging/crashing
    }

    #[test]
    fn create_reports_bad_skin_for_missing_dir() {
        let surfaces = [super::test_support::make_bgra_iosurface(4, 4)];
        let path = std::ffi::CString::new("/no/such/skin/dir").unwrap();
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(),
            vtable: super::test_support::empty_vtable(),
            surfaces: surfaces.as_ptr(),
            surface_count: 1,
            content_surface: std::ptr::null_mut(),
            w: 4,
            h: 4,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        assert_eq!(
            unsafe { carapace_create(&desc, &mut handle) },
            CarapaceStatus::ErrBadSkin
        );
        assert!(handle.is_null());
    }

    #[test]
    fn create_rejects_null_out_and_null_skin_dir() {
        // null out
        let surfaces = [super::test_support::make_bgra_iosurface(4, 4)];
        let desc = CarapaceCreateDesc {
            skin_dir: std::ptr::null(),
            vtable: super::test_support::empty_vtable(),
            surfaces: surfaces.as_ptr(),
            surface_count: 1,
            content_surface: std::ptr::null_mut(),
            w: 4,
            h: 4,
        };
        assert_eq!(
            unsafe { carapace_create(&desc, std::ptr::null_mut()) },
            CarapaceStatus::ErrNullArg
        );
        // null skin_dir, valid out
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        assert_eq!(
            unsafe { carapace_create(&desc, &mut handle) },
            CarapaceStatus::ErrNullArg
        );
        assert!(handle.is_null());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod active_tier_tests {
    use super::test_support::create_test_handle_pool;
    use super::*;

    #[test]
    fn active_tier_is_valid_before_and_after_first_frame() {
        let (handle, _s) = create_test_handle_pool(300, 140, 2);
        let mut tier = CarapaceTier::Readback;
        assert_eq!(
            unsafe { carapace_active_tier(handle, &mut tier) },
            CarapaceStatus::Ok
        );
        assert!(matches!(
            tier,
            CarapaceTier::Readback | CarapaceTier::Shared
        ));

        // Force a frame; the tier should still (trivially) agree.
        unsafe {
            let _ = carapace_set_frame_rate(handle, 0);
        }
        unsafe {
            let _ = carapace_invalidate(handle);
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        let mut tier2 = CarapaceTier::Readback;
        assert_eq!(
            unsafe { carapace_active_tier(handle, &mut tier2) },
            CarapaceStatus::Ok
        );
        assert!(matches!(
            tier2,
            CarapaceTier::Readback | CarapaceTier::Shared
        ));

        unsafe { carapace_destroy(handle) };
    }

    #[test]
    fn active_tier_rejects_null_handle_and_null_out() {
        let mut tier = CarapaceTier::Readback;
        assert_eq!(
            unsafe { carapace_active_tier(std::ptr::null_mut(), &mut tier) },
            CarapaceStatus::ErrNullArg
        );

        let (handle, _s) = create_test_handle_pool(300, 140, 2);
        assert_eq!(
            unsafe { carapace_active_tier(handle, std::ptr::null_mut()) },
            CarapaceStatus::ErrNullArg
        );
        unsafe { carapace_destroy(handle) };
    }
}

#[cfg(all(test, target_os = "macos"))]
mod v2_pointer_poison_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    static TOGGLED: AtomicBool = AtomicBool::new(false);
    extern "C" fn rec(_c: *mut std::ffi::c_void, action: *const std::ffi::c_char) {
        if unsafe { std::ffi::CStr::from_ptr(action) }.to_string_lossy() == "toggle_play" {
            TOGGLED.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn press_over_button_fires_action_through_the_loop() {
        TOGGLED.store(false, Ordering::SeqCst);
        let vt = crate::host::CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: Some(rec),
            frame_ready: None,
        };
        let (h, _s) = test_support::create_test_handle_pool_vt(300, 140, 2, vt);
        assert_eq!(
            unsafe { carapace_pointer(h, 55.0, 55.0, CarapacePointerKind::Press) },
            CarapaceStatus::Ok
        );
        // Poll instead of a fixed sleep: the loop must drain the pointer command, render (paying the
        // first-frame GPU pipeline-compile cost under parallel test load), and invoke through the
        // host vtable before the assertion below can pass.
        test_support::wait_for(std::time::Duration::from_secs(10), || {
            TOGGLED.load(Ordering::SeqCst)
        });
        assert!(
            TOGGLED.load(Ordering::SeqCst),
            "press should fire host.toggle_play via the loop"
        );
        unsafe { carapace_destroy(h) };
    }

    #[test]
    fn render_thread_panic_poisons_and_subsequent_calls_are_poisoned() {
        let (h, _s) = test_support::create_test_handle_pool(300, 140, 2);
        assert_eq!(
            unsafe { carapace_force_panic(h) },
            CarapaceStatus::Ok // enqueues; returns immediately
        );
        // Poll instead of a fixed sleep: wait for the loop to drain, panic inside
        // `render_guarded`'s `catch_unwind`, and set `poisoned` before asserting on it.
        test_support::wait_for(std::time::Duration::from_secs(10), || {
            (unsafe { carapace_invalidate(h) }) == CarapaceStatus::ErrPoisoned
        });
        assert_eq!(
            unsafe { carapace_invalidate(h) },
            CarapaceStatus::ErrPoisoned
        );
        assert_eq!(
            unsafe { carapace_pointer(h, 0.0, 0.0, CarapacePointerKind::Press) },
            CarapaceStatus::ErrPoisoned
        );
        unsafe { carapace_destroy(h) }; // must still join a poisoned/exited thread
    }
}

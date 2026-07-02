//! The engine→host hit-test channel: classify a point without firing Lua, so a host can decide to
//! move the window, let the skin consume the event, or pass it through.

use carapace::scene::{HitKind, Pt};

use crate::guard::{CarapaceStatus, ffi_guard};
use crate::handle::CarapaceEngine;

/// Classification of a point for a host embedder. Additive enum.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapaceHitKind {
    /// Event should fall through the skin (transparent / passthrough region).
    Passthrough = 0,
    /// Skin consumes the event (a control, or opaque non-interactive skin).
    Control = 1,
    /// Host should move the window (a drag region).
    Drag = 2,
}

/// Classify the point `(x, y)` (DESIGN-CANVAS coords) without side effects. Writes `*out`.
///
/// # Safety
/// `ptr` must come from `carapace_create`; `out` must be non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_hit_test(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    out: *mut CarapaceHitKind,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if out.is_null() {
        return CarapaceStatus::ErrNullArg;
    }
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    ffi_guard!(ptr, {
        // Lay out at the design canvas (as the pointer path does), classify, map.
        let scene = e.engine.layout(e.cw as f32, e.ch as f32);
        let kind = match scene.hit_kind(Pt {
            x: x as f32,
            y: y as f32,
        }) {
            HitKind::Passthrough => CarapaceHitKind::Passthrough,
            HitKind::Control => CarapaceHitKind::Control,
            HitKind::Drag => CarapaceHitKind::Drag,
        };
        unsafe { *out = kind };
        CarapaceStatus::Ok
    })
}

#[cfg(all(test, target_os = "macos"))]
mod hit_tests {
    use super::*;
    use crate::handle::{CarapaceCreateDesc, carapace_create, carapace_destroy};
    use crate::host::CarapaceHostVTable;
    use crate::render::IOSurfaceRef;

    // Same workspace-sibling demo skin used by handle.rs's tick tests (300x140 canvas, visible
    // content) — kept independent of the frozen embed-spike crate.
    const SKIN_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../carapace-demo/skins/classic"
    );

    /// Build a caller-owned BGRA8 IOSurface of size `w`x`h` via the `io-surface` crate. Mirrors
    /// `handle.rs`'s test helper of the same name (kept local here to avoid widening hit.rs's
    /// surface beyond the brief's create+hit_test files).
    ///
    /// `io_surface::new` returns an owning `IOSurface` wrapper (drop => `CFRelease`); we must NOT
    /// let that wrapper drop before the test is done with the raw ref, so we `mem::forget` it and
    /// intentionally leak the +1 Core Foundation reference for the lifetime of the test process.
    #[allow(deprecated)] // `io_surface` (test-only dev-dep) is deprecated upstream in favor of
    // `objc2-io-surface`; kept here only for its convenient IOSurface-creation API.
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

    fn create_classic_handle(w: u32, h: u32) -> *mut CarapaceEngine {
        let surface = make_bgra_iosurface(w as usize, h as usize);
        let path = std::ffi::CString::new(SKIN_DIR).unwrap();
        let vtable = CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: None,
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
        handle
    }

    #[test]
    fn hit_test_rejects_null_out_and_handle() {
        // null handle
        assert_eq!(
            unsafe { carapace_hit_test(std::ptr::null_mut(), 0.0, 0.0, std::ptr::null_mut()) },
            CarapaceStatus::ErrNullArg
        );

        // null out, real handle
        let handle = create_classic_handle(300, 140);
        assert_eq!(
            unsafe { carapace_hit_test(handle, 0.0, 0.0, std::ptr::null_mut()) },
            CarapaceStatus::ErrNullArg
        );
        unsafe { carapace_destroy(handle) };
    }

    #[test]
    fn hit_test_classifies_outside_as_passthrough_and_inside_as_control() {
        let handle = create_classic_handle(300, 140);

        // Far outside the 300x140 canvas — nothing the skin declares can cover this point.
        let mut kind = CarapaceHitKind::Control;
        assert_eq!(
            unsafe { carapace_hit_test(handle, -100.0, -100.0, &mut kind) },
            CarapaceStatus::Ok
        );
        assert_eq!(kind as i32, CarapaceHitKind::Passthrough as i32);

        // Over the play button (`fill{ path = rect{x=20,y=20,w=70,h=70}, ... on_press = ... }` in
        // skin.lua, DESIGN-CANVAS coords) — an opaque, interactive control.
        let mut kind = CarapaceHitKind::Passthrough;
        assert_eq!(
            unsafe { carapace_hit_test(handle, 55.0, 55.0, &mut kind) },
            CarapaceStatus::Ok
        );
        assert_eq!(kind as i32, CarapaceHitKind::Control as i32);

        // Note: this skin's whole-backdrop `region{}` (skin.lua line 4) omits `role = "drag"`, so
        // it defaults to `HotspotRole::Control` (see `vocab::parse_role`) rather than `Drag` —
        // there is no point in this fixture that yields `HitKind::Drag`, so a drag assertion isn't
        // included here (see task-9-report.md).

        unsafe { carapace_destroy(handle) };
    }
}

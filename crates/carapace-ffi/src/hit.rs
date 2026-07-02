//! The engine→host hit-test channel: classify a point without firing Lua, so a host can decide to
//! move the window, let the skin consume the event, or pass it through.
//!
//! In v2 `carapace_hit_test` reads the render thread's published `SnapshotCell` on the CALLER's
//! thread (no engine access, no queue round-trip) — sub-millisecond and never blocks the render
//! thread. The snapshot is at most one frame stale.

use carapace::scene::{HitKind, Pt};

use crate::guard::CarapaceStatus;
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

/// Classify a point `(x, y)` in skin-local coordinates against the latest published scene.
/// Panic-free (a lock read + a match), so no panic guard is needed.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`. `out`
/// must be a valid pointer to a `CarapaceHitKind`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_hit_test(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    out: *mut CarapaceHitKind,
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
    let kind = match crate::snapshot::hit_kind_of(
        &e.snapshot,
        Pt {
            x: x as f32,
            y: y as f32,
        },
    ) {
        HitKind::Passthrough => CarapaceHitKind::Passthrough,
        HitKind::Control => CarapaceHitKind::Control,
        HitKind::Drag => CarapaceHitKind::Drag,
    };
    unsafe { *out = kind };
    CarapaceStatus::Ok
}

#[cfg(all(test, target_os = "macos"))]
mod hit_tests {
    use super::*;

    #[test]
    fn hit_test_after_a_frame_classifies_outside_passthrough_and_control_inside() {
        let (handle, _s) = crate::handle::test_support::create_test_handle_pool(300, 140, 2);
        // Force one frame so the snapshot is populated, then release + let it publish.
        unsafe {
            let _ = crate::handle::carapace_set_frame_rate(handle, 0);
        }
        unsafe {
            let _ = crate::handle::carapace_invalidate(handle);
        }
        // The first GPU frame pays a one-time pipeline/shader-compile cost that can exceed a fixed
        // short sleep under parallel GPU-test load (see render_thread.rs's render_tests); poll up
        // to a generous ceiling for the play-button hit to classify as Control instead.
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || {
            let mut kind = CarapaceHitKind::Passthrough;
            let _ = unsafe { carapace_hit_test(handle, 55.0, 55.0, &mut kind) };
            matches!(kind, CarapaceHitKind::Control)
        });

        let mut kind = CarapaceHitKind::Control;
        assert_eq!(
            unsafe { carapace_hit_test(handle, -100.0, -100.0, &mut kind) },
            CarapaceStatus::Ok
        );
        assert_eq!(kind as i32, CarapaceHitKind::Passthrough as i32);

        let mut kind = CarapaceHitKind::Passthrough;
        assert_eq!(
            unsafe { carapace_hit_test(handle, 55.0, 55.0, &mut kind) },
            CarapaceStatus::Ok
        );
        assert_eq!(kind as i32, CarapaceHitKind::Control as i32);

        unsafe { crate::handle::carapace_destroy(handle) };
    }

    #[test]
    fn hit_test_rejects_null_handle_and_null_out() {
        let mut kind = CarapaceHitKind::Passthrough;
        assert_eq!(
            unsafe { carapace_hit_test(std::ptr::null_mut(), 0.0, 0.0, &mut kind) },
            CarapaceStatus::ErrNullArg
        );

        let (handle, _s) = crate::handle::test_support::create_test_handle_pool(300, 140, 2);
        assert_eq!(
            unsafe { carapace_hit_test(handle, 0.0, 0.0, std::ptr::null_mut()) },
            CarapaceStatus::ErrNullArg
        );
        unsafe { crate::handle::carapace_destroy(handle) };
    }
}

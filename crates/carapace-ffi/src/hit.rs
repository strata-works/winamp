//! The engine→host hit-test channel: classify a point without firing Lua, so a host can decide to
//! move the window, let the skin consume the event, or pass it through.
//!
//! NOTE (SDD v2): `carapace_hit_test` and its `hit_tests` module were REMOVED here in Task 4 (the
//! render-thread rewrite). In v2 the classification reads the render thread's published
//! `SnapshotCell` on the caller's thread (no engine access) — re-added in Task 7. Only the ABI enum
//! `CarapaceHitKind` remains, so the public type is stable across the interim.

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

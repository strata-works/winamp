//! The dedicated render thread: owns the `!Send` Engine + GPU, runs the pacing loop. Apple-only.
#![cfg(any(target_os = "macos", target_os = "ios"))]
// `SendSurfaces` isn't wired into a `RenderThread` yet (later task); allow it to sit unused in
// the meantime, matching `queue.rs`/`snapshot.rs`'s precedent for staged-ahead modules.
#![allow(dead_code)]

use std::ffi::c_void;

/// The raw host-owned pointers that must cross onto the spawned render thread at construction.
///
/// # Safety contract
/// The IOSurface pointers are caller-owned and guaranteed (by the C ABI contract, see
/// `carapace.h`) to (1) be valid BGRA surfaces of the create-time size and (2) outlive the engine.
/// They are only ever touched by the render thread after this move. The engine itself is built on
/// the render thread and never crosses, so the ONLY thing this wrapper makes `Send` is opaque host
/// memory the host promised is thread-safe to use from our render thread. This is the single
/// load-bearing `Send` assertion in the crate.
pub(crate) struct SendSurfaces {
    pub surfaces: Vec<*const c_void>,
    pub content: *const c_void,
}

// SAFETY: see the struct's safety contract above.
unsafe impl Send for SendSurfaces {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_surfaces_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SendSurfaces>();
    }
}

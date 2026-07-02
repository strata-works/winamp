//! carapace-ffi — the stable C ABI that lets a host app embed the carapace engine as custom UI.
//! Apple (macOS/iOS) only in this version; see docs/superpowers/specs/2026-07-01-carapace-ffi-design.md.

mod guard;
pub mod host;
mod queue;
mod snapshot;

pub use guard::{CARAPACE_ABI_MAJOR, CARAPACE_ABI_MINOR, CarapaceStatus, carapace_last_error};

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod render;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod handle;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use handle::*;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod hit;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use hit::*;

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod render_thread;

/// The ABI version this library implements: `MAJOR << 16 | MINOR`. Additive changes bump MINOR;
/// breaking changes bump MAJOR. A host compares this against the header's constants at load time.
#[unsafe(no_mangle)]
pub extern "C" fn carapace_abi_version() -> u32 {
    (CARAPACE_ABI_MAJOR << 16) | CARAPACE_ABI_MINOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_version_is_v2() {
        // v2.0: MAJOR=2, MINOR=0 → 2 << 16. Must match the header's CARAPACE_ABI_MAJOR/MINOR.
        assert_eq!(carapace_abi_version(), 2 << 16);
        assert_eq!(CARAPACE_ABI_MAJOR, 2);
        assert_eq!(CARAPACE_ABI_MINOR, 0);
    }
}

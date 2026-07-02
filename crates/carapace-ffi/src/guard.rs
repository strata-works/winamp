//! Panic-safety, status codes, and the thread-local error channel shared by every export.
//!
//! Boundary policy: every `#[unsafe(no_mangle)]` export wraps its body in `ffi_guard!`, which catches any
//! panic (so nothing unwinds into the host's foreign frames) and turns it into `ErrPanic`. Handle-
//! bearing calls additionally *poison* the handle. We NEVER `abort()`: carapace runs inside the
//! host's process.

use std::cell::RefCell;
use std::ffi::c_char;
use std::sync::Once;

/// Result of every fallible export. Additive: append new variants, never reorder.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarapaceStatus {
    Ok = 0,
    ErrNullArg = 1,
    ErrBadSkin = 2,
    ErrGpuInit = 3,
    ErrPoisoned = 4,
    ErrPanic = 5,
}

pub const CARAPACE_ABI_MAJOR: u32 = 0;
pub const CARAPACE_ABI_MINOR: u32 = 1;

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Record a human-readable error for the current thread; retrievable via `carapace_last_error`.
// On non-Apple targets `carapace-ffi` compiles as a shell (handle/hit/render are cfg'd out), and
// this fn's only non-test caller is `install_panic_hook`/`carapace_create`, both Apple-gated —
// dead on that target. Genuinely used on Apple, so the allow is cfg'd to non-Apple only.
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| *e.borrow_mut() = msg.to_string());
}

/// Install (once per process) a panic hook that captures the panic message + location into the
/// thread-local BEFORE the unwind reaches `catch_unwind` (whose payload is opaque). Chains the
/// previous hook. Call this at the top of `carapace_create`.
// Only caller is `carapace_create`, which is Apple-gated — dead on non-Apple.
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub fn install_panic_hook() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            set_last_error(&info.to_string());
            prev(info);
        }));
    });
}

/// Wrap a handle-less export body. On panic: record `ErrPanic`, return it.
// Its only non-test call site is `carapace_create` (Apple-gated); dead on non-Apple.
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_macros))]
macro_rules! ffi_guard_no_handle {
    ($body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(status) => status,
            Err(_) => $crate::guard::CarapaceStatus::ErrPanic,
        }
    };
}

/// Wrap a handle-bearing export body. On panic: poison the handle, return `ErrPanic`.
/// `$ptr` is the `*mut CarapaceEngine` passed to the export.
// SDD v2: its consumers (`carapace_pointer`/`carapace_hit_test`/`carapace_force_panic`) were
// removed in Task 4 and are re-added in Tasks 7/8; the macro (and its re-export below) stay so those
// tasks don't have to reintroduce them. Allow them to sit unused in the interim.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[allow(unused_macros)]
macro_rules! ffi_guard {
    ($ptr:expr, $body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(status) => status,
            Err(_) => {
                if let Some(h) = unsafe { ($ptr).as_mut() } {
                    h.poisoned = true;
                }
                $crate::guard::CarapaceStatus::ErrPanic
            }
        }
    };
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[allow(unused_imports)] // re-consumed in Tasks 7/8 (see the macro's note above).
pub(crate) use ffi_guard;
// Only imported via this path by `handle.rs` (Apple-gated); dead on non-Apple (the crate's own
// tests below reach the macro directly, without going through this re-export).
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios")),
    allow(unused_imports)
)]
pub(crate) use ffi_guard_no_handle;

/// Copy the current thread's last error into `buf` (NUL-terminated, truncated to `cap`). Returns
/// the number of bytes the message needs (excluding NUL), so a caller can size a retry buffer.
/// Passing a null `buf` or `cap == 0` just returns that length.
///
/// # Safety
/// `buf` must be null or point to at least `cap` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_last_error(buf: *mut c_char, cap: usize) -> usize {
    LAST_ERROR.with(|e| {
        let s = e.borrow();
        let bytes = s.as_bytes();
        let needed = bytes.len();
        if !buf.is_null() && cap > 0 {
            let n = needed.min(cap - 1);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
                *buf.add(n) = 0;
            }
        }
        needed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn last_error_roundtrips_and_truncates() {
        set_last_error("boom");
        let mut buf = [0i8; 16];
        let needed = unsafe { carapace_last_error(buf.as_mut_ptr(), buf.len()) };
        assert_eq!(needed, 4);
        assert_eq!(
            unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap(),
            "boom"
        );

        // Truncation: cap smaller than the message still NUL-terminates.
        set_last_error("abcdefgh");
        let mut small = [0i8; 4]; // room for 3 chars + NUL
        let needed = unsafe { carapace_last_error(small.as_mut_ptr(), small.len()) };
        assert_eq!(needed, 8);
        assert_eq!(
            unsafe { CStr::from_ptr(small.as_ptr()) }.to_str().unwrap(),
            "abc"
        );
    }

    #[test]
    fn no_handle_guard_maps_panic_to_err_panic() {
        let ok = ffi_guard_no_handle!(CarapaceStatus::Ok);
        assert_eq!(ok, CarapaceStatus::Ok);
        let panicked = ffi_guard_no_handle!({
            panic!("kaboom");
            #[allow(unreachable_code)]
            CarapaceStatus::Ok
        });
        assert_eq!(panicked, CarapaceStatus::ErrPanic);
    }
}

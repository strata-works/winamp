//! Panic-safety, status codes, and the thread-local error channel shared by every export.
//!
//! Boundary policy: `carapace_create` wraps its synchronous init body in `ffi_guard_no_handle!`,
//! catching any panic (so nothing unwinds into the host's foreign frames) and turning it into
//! `ErrPanic`. Once a handle exists, panics are caught on the RENDER THREAD instead
//! (`render_thread::render_guarded`'s `catch_unwind`), which sets the shared `poisoned` flag and
//! lets the thread exit; every front-end export then short-circuits with `ErrPoisoned` by reading
//! that atomic directly — no per-call guard needed for the thin, genuinely panic-free front-end
//! functions. `carapace_hit_test` is the one exception: it runs engine geometry code on the
//! CALLER's thread (not the render thread), so it carries its own `catch_unwind` and reports a
//! caught panic there as `ErrPanic` without poisoning the handle. We NEVER `abort()`: carapace runs
//! inside the host's process.

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

pub const CARAPACE_ABI_MAJOR: u32 = 2;
pub const CARAPACE_ABI_MINOR: u32 = 0;

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

/// Read the current thread's last error as an owned `String` (a clone; the TLS is left intact).
/// The render thread uses this to lift the panic message the process-wide panic hook just wrote
/// into ITS thread-local — captured on the render thread — up into the shared poison slot, so a
/// host calling `carapace_last_error` on its OWN thread can retrieve it via the poison path.
// Only non-test caller is `render_guarded` (Apple-gated); dead on non-Apple.
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub fn last_error_string() -> String {
    LAST_ERROR.with(|e| e.borrow().clone())
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

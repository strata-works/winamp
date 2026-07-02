//! The host callback vtable a host app registers, and `FfiHost` bridging it to `carapace::host::Host`.
//! Ported from embed-spike; string lifetimes are borrowed-per-call (see the zero-free contract).

use std::ffi::{CStr, CString, c_char, c_void};
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::state::StateValue;

/// C function table the Swift app registers. Swift IS the host.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CarapaceHostVTable {
    pub ctx: *mut c_void,
    pub get_num: Option<extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool>,
    pub get_str: Option<extern "C" fn(*mut c_void, *const c_char, *mut c_char, usize) -> bool>,
    pub invoke: Option<extern "C" fn(*mut c_void, *const c_char)>,
    /// v2: fired on the render thread when a frame lands in `surfaces[index]`. `frame_id` is a
    /// monotonic counter starting at 1. Must be thread-safe, non-blocking, and MUST NOT call any
    /// `carapace_*` function (that reenters the queue/loop and can deadlock).
    pub frame_ready: Option<extern "C" fn(*mut c_void, u32, u64)>,
}

// The vtable now legitimately crosses onto the dedicated render thread at construction (it is
// moved into the `RenderThread` state along with the `SendSurfaces` wrapper). Its raw `ctx`
// pointer, and every function pointer here, are host-guaranteed thread-safe to invoke from
// whichever thread calls them (§ callback contract in the spec) — the host promises `ctx` may be
// touched off the calling thread. Send/Sync are asserted to satisfy the engine's `Box<dyn Host>`
// and the render thread's `'static + Send` bound.
unsafe impl Send for CarapaceHostVTable {}
unsafe impl Sync for CarapaceHostVTable {}

const ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "toggle" },
    ActionSpec {
        name: "toggle_play",
    },
    ActionSpec { name: "stop" },
    ActionSpec { name: "prev" },
    ActionSpec { name: "next" },
    ActionSpec { name: "seek" },
    ActionSpec { name: "play_index" },
    ActionSpec { name: "begin_drag" },
    ActionSpec { name: "minimize" },
    ActionSpec { name: "close" },
];

pub struct FfiHost {
    vtable: CarapaceHostVTable,
}

impl FfiHost {
    pub fn new(vtable: CarapaceHostVTable) -> Self {
        Self { vtable }
    }
}

impl Host for FfiHost {
    fn name(&self) -> &str {
        "ffi"
    }

    fn tick(&mut self, _dt: Duration) {
        // Swift owns its own clock/state; nothing to advance Rust-side.
    }

    fn get(&self, key: &str) -> Option<StateValue> {
        let ckey = CString::new(key).ok()?;
        // Try numeric first.
        if let Some(get_num) = self.vtable.get_num {
            let mut out = 0.0_f64;
            if get_num(self.vtable.ctx, ckey.as_ptr(), &mut out as *mut f64) {
                return Some(StateValue::Scalar(out as f32));
            }
        }
        // Then string.
        if let Some(get_str) = self.vtable.get_str {
            let mut buf = vec![0_u8; 256];
            if get_str(
                self.vtable.ctx,
                ckey.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
            ) {
                // Defensive: ensure NUL termination even if callee fills all 256 bytes
                let last = buf.len() - 1;
                buf[last] = 0;
                let s = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
                    .to_string_lossy()
                    .into_owned();
                return Some(StateValue::Str(std::sync::Arc::from(s.as_str())));
            }
        }
        None
    }

    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }

    fn invoke(&mut self, action: &str, _args: &[Value]) {
        if let (Some(invoke), Ok(caction)) = (self.vtable.invoke, CString::new(action)) {
            invoke(self.vtable.ctx, caction.as_ptr());
        }
    }

    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new() // collections out of scope for the spike
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static INVOKED: AtomicU32 = AtomicU32::new(0);

    extern "C" fn fake_get_num(_ctx: *mut c_void, key: *const c_char, out: *mut f64) -> bool {
        let k = unsafe { CStr::from_ptr(key) }.to_str().unwrap();
        if k == "level" {
            unsafe { *out = 0.42 };
            true
        } else {
            false
        }
    }

    extern "C" fn fake_invoke(_ctx: *mut c_void, action: *const c_char) {
        let a = unsafe { CStr::from_ptr(action) }.to_str().unwrap();
        if a == "toggle" {
            INVOKED.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn vtable() -> CarapaceHostVTable {
        CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: Some(fake_get_num),
            get_str: None,
            invoke: Some(fake_invoke),
            frame_ready: None,
        }
    }

    #[test]
    fn vtable_has_frame_ready_slot() {
        let vt = CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: None,
        };
        assert!(vt.frame_ready.is_none());
    }

    #[test]
    fn get_maps_numeric_state_through_the_vtable() {
        let host = FfiHost::new(vtable());
        assert_eq!(host.get("level"), Some(StateValue::Scalar(0.42_f32)));
        assert_eq!(host.get("missing"), None);
    }

    #[test]
    fn invoke_routes_to_the_callback_and_action_is_advertised() {
        INVOKED.store(0, Ordering::SeqCst);
        let mut host = FfiHost::new(vtable());
        assert!(host.actions().iter().any(|a| a.name == "toggle"));
        host.invoke("toggle", &[]);
        assert_eq!(INVOKED.load(Ordering::SeqCst), 1);
    }
}

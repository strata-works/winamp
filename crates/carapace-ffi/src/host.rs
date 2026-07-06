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
    /// Opaque host context, passed unchanged as the first argument to every callback below.
    pub ctx: *mut c_void,
    /// Numeric state read: given a NUL-terminated key, write the value through `out` and return
    /// `true` if the host has it, else return `false` and leave `out` untouched.
    pub get_num: Option<extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool>,
    /// String state read: given a NUL-terminated key, the host writes a NUL-terminated string into
    /// `buf` (capacity `cap`) and returns `true` if it has the key, else returns `false`.
    pub get_str: Option<extern "C" fn(*mut c_void, *const c_char, *mut c_char, usize) -> bool>,
    /// Perform a host action identified by a NUL-terminated action name (the C mirror of
    /// `Host::invoke`).
    pub invoke: Option<extern "C" fn(*mut c_void, *const c_char)>,
    /// v2: fired on the render thread when a frame lands in `surfaces[index]`. `frame_id` is a
    /// monotonic counter starting at 1. Must be thread-safe, non-blocking, and MUST NOT call any
    /// `carapace_*` function (that reenters the queue/loop and can deadlock).
    pub frame_ready: Option<extern "C" fn(*mut c_void, u32, u64)>,
    /// v3: number of rows in `collection` (NUL-terminated). Null = no collections.
    pub row_count: Option<extern "C" fn(*mut c_void, *const c_char) -> u32>,
    /// v3: write row `index`'s string `field` into `buf` (cap `cap`), NUL-terminated; return
    /// `true` if present. Tried after `get_row_num` (mirrors `get`, which reads numeric first).
    pub get_row_str:
        Option<extern "C" fn(*mut c_void, *const c_char, u32, *const c_char, *mut c_char, usize) -> bool>,
    /// v3: write row `index`'s numeric `field` through `out`; return `true` if present.
    pub get_row_num:
        Option<extern "C" fn(*mut c_void, *const c_char, u32, *const c_char, *mut f64) -> bool>,
    /// v3: perform a host action carrying a single numeric argument (the C mirror of
    /// `Host::invoke` with one `Value::Num`, e.g. `seek`, `set_volume`, `play_index`).
    pub invoke_arg: Option<extern "C" fn(*mut c_void, *const c_char, f64)>,
}

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

/// Bridges a host-supplied [`CarapaceHostVTable`] to the engine's Rust [`Host`] trait, so the
/// render thread can drive the vtable's callbacks as if it were a native Rust host.
pub struct FfiHost {
    vtable: CarapaceHostVTable,
}

impl FfiHost {
    /// Wrap a host-supplied vtable as an `FfiHost`.
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

    fn invoke(&mut self, action: &str, args: &[Value]) {
        let Ok(caction) = CString::new(action) else { return };
        if let (Some(invoke_arg), Some(Value::Num(n))) = (self.vtable.invoke_arg, args.first()) {
            invoke_arg(self.vtable.ctx, caction.as_ptr(), *n);
            return;
        }
        if let Some(invoke) = self.vtable.invoke {
            invoke(self.vtable.ctx, caction.as_ptr());
        }
    }

    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new() // field-agnostic path unused for FFI; see rows_for
    }

    fn rows_for(&self, collection: &str, fields: &[&str]) -> Vec<Row> {
        let (Some(count_fn), Ok(ccol)) = (self.vtable.row_count, CString::new(collection)) else {
            return Vec::new();
        };
        let n = count_fn(self.vtable.ctx, ccol.as_ptr());
        (0..n)
            .map(|i| {
                let mut row = Row::new();
                for &field in fields {
                    let Ok(cfield) = CString::new(field) else { continue };
                    if let Some(gn) = self.vtable.get_row_num {
                        let mut out = 0.0_f64;
                        if gn(self.vtable.ctx, ccol.as_ptr(), i, cfield.as_ptr(), &mut out) {
                            row = row.set(field, StateValue::Scalar(out as f32));
                            continue;
                        }
                    }
                    if let Some(gs) = self.vtable.get_row_str {
                        let mut buf = vec![0_u8; 256];
                        if gs(self.vtable.ctx, ccol.as_ptr(), i, cfield.as_ptr(),
                              buf.as_mut_ptr() as *mut c_char, buf.len()) {
                            let last = buf.len() - 1;
                            buf[last] = 0;
                            let s = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
                                .to_string_lossy()
                                .into_owned();
                            row = row.set(field, StateValue::Str(std::sync::Arc::from(s.as_str())));
                        }
                    }
                }
                row
            })
            .collect()
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
            row_count: None,
            get_row_str: None,
            get_row_num: None,
            invoke_arg: None,
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
            row_count: None,
            get_row_str: None,
            get_row_num: None,
            invoke_arg: None,
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

    use std::sync::atomic::{AtomicU64, Ordering as O2};
    static LAST_ARG_BITS: AtomicU64 = AtomicU64::new(0);

    extern "C" fn fake_row_count(_c: *mut c_void, coll: *const c_char) -> u32 {
        let c = unsafe { CStr::from_ptr(coll) }.to_str().unwrap();
        if c == "playlist" { 2 } else { 0 }
    }
    extern "C" fn fake_get_row_str(
        _c: *mut c_void, _coll: *const c_char, index: u32, field: *const c_char,
        buf: *mut c_char, cap: usize,
    ) -> bool {
        let f = unsafe { CStr::from_ptr(field) }.to_str().unwrap();
        let val = match (index, f) {
            (0, "title") => "one", (1, "title") => "two",
            (0, "dur") => "0:10", (1, "dur") => "0:20",
            _ => return false,
        };
        let bytes = val.as_bytes();
        let n = bytes.len().min(cap.saturating_sub(1));
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
            *buf.add(n) = 0;
        }
        true
    }
    extern "C" fn fake_invoke_arg(_c: *mut c_void, action: *const c_char, arg: f64) {
        let a = unsafe { CStr::from_ptr(action) }.to_str().unwrap();
        if a == "seek" { LAST_ARG_BITS.store(arg.to_bits(), O2::SeqCst); }
    }

    fn vtable_v3() -> CarapaceHostVTable {
        CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: Some(fake_invoke),
            frame_ready: None,
            row_count: Some(fake_row_count),
            get_row_str: Some(fake_get_row_str),
            get_row_num: None,
            invoke_arg: Some(fake_invoke_arg),
        }
    }

    #[test]
    fn rows_for_materializes_requested_fields() {
        let host = FfiHost::new(vtable_v3());
        let rows = host.rows_for("playlist", &["title", "dur"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("title"), Some(&StateValue::Str("one".into())));
        assert_eq!(rows[1].get("dur"), Some(&StateValue::Str("0:20".into())));
        assert!(host.rows_for("unknown", &["title"]).is_empty());
    }

    #[test]
    fn invoke_forwards_numeric_arg_else_falls_back() {
        LAST_ARG_BITS.store(0, O2::SeqCst);
        INVOKED.store(0, O2::SeqCst);
        let mut host = FfiHost::new(vtable_v3());
        host.invoke("seek", &[Value::Num(0.5)]);
        assert_eq!(f64::from_bits(LAST_ARG_BITS.load(O2::SeqCst)), 0.5);
        host.invoke("toggle", &[]); // parameterless → plain invoke (fake_invoke bumps INVOKED)
        assert_eq!(INVOKED.load(O2::SeqCst), 1);
    }
}

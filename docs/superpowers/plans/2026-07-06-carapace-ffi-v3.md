# carapace-ffi v3 (ABI 3.0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the carapace C ABI to 3.0 so a native host can drive a multi-skin, list-and-argument music player: live skin hot-swap, host-provided list rows, and numeric action arguments.

**Architecture:** Three additive capabilities. (1) An engine seam `Host::rows_for(collection, fields)` that defaults to the existing `rows()` — so only the FFI host overrides it and native hosts are untouched. (2) Four new nullable callbacks on `CarapaceHostVTable` (`row_count`, `get_row_str`, `get_row_num`, `invoke_arg`) plus their `FfiHost` implementations. (3) A `carapace_swap_skin` export that loads on the render thread (SkinSource is `!Send`) and applies `Command::Swap`, keeping the host + GPU + render thread.

**Tech Stack:** Rust, `carapace` (engine) + `carapace-ffi` (C ABI), cbindgen, mpsc command queue, IOSurface render thread (macOS gpu tests).

## Global Constraints

- Git identity for all commits: `Daniel Agbemava <danagbemava@gmail.com>` (use `git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit ...`).
- Work on branch `showcase-three-skins` (already checked out).
- `CARAPACE_ABI_MAJOR` = 3, `CARAPACE_ABI_MINOR` = 0. `carapace_abi_version()` returns `3 << 16`.
- The committed `crates/carapace-ffi/include/carapace.h` MUST stay in sync: after any change to the vtable struct or `extern "C"` exports, regenerate with `cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact` and commit the result. `tests/header.rs::header_is_fresh` gates this (macOS only).
- Before declaring done / pushing: `cargo test --workspace` AND `cargo clippy --workspace --all-targets -- -D warnings` must pass. The FFI render/swap tests are `#[cfg(all(test, target_os = "macos"))]` and run in the macOS test lane.
- No "Generated with Claude Code" attribution in commits or PRs.
- Crate is `#![deny(missing_docs)]` — every new public item and C export needs a `///` doc comment.

---

### Task 1: Engine `rows_for` seam (field-aware rows)

**Files:**
- Modify: `crates/carapace/src/host.rs:65` (add `rows_for` to the `Host` trait)
- Modify: `crates/carapace/src/engine.rs:240` (call `rows_for` with the template binds)
- Test: `crates/carapace/tests/list_layout.rs` (append a test)

**Interfaces:**
- Produces: `Host::rows_for(&self, collection: &str, fields: &[&str]) -> Vec<Row>` — default impl delegates to `self.rows(collection)`. The engine's list layout calls this instead of `rows()`. Task 2's `FfiHost` overrides it.

- [ ] **Step 1: Write the failing test**

Append to `crates/carapace/tests/list_layout.rs`:

```rust
#[test]
fn engine_passes_template_binds_to_rows_for() {
    use std::cell::RefCell;
    struct RecHost {
        seen: RefCell<Vec<String>>,
    }
    impl Host for RecHost {
        fn name(&self) -> &str { "rec" }
        fn tick(&mut self, _dt: Duration) {}
        fn get(&self, _k: &str) -> Option<StateValue> { None }
        fn actions(&self) -> &[ActionSpec] { &[] }
        fn invoke(&mut self, _a: &str, _args: &[Value]) {}
        fn rows_for(&self, _collection: &str, fields: &[&str]) -> Vec<Row> {
            *self.seen.borrow_mut() = fields.iter().map(|s| s.to_string()).collect();
            vec![Row::new()
                .set("title", StateValue::Str("t".into()))
                .set("dur", StateValue::Str("d".into()))]
        }
    }
    const SK: &str = "list{ collection='playlist', x=0, y=0, w=100, h=40, row_height=20, \
        template={ { bind='title', x=2, y=2, size=12, color={r=1,g=2,b=3} }, \
                   { bind='dur', right=4, y=2, size=12, color={r=1,g=2,b=3} } } }";
    let host = RecHost { seen: RefCell::new(vec![]) };
    let engine = Engine::new(
        Box::new(host),
        VocabRegistry::base(),
        SkinSource::inline(SK, (100, 100)),
    )
    .unwrap();
    // The engine must have asked rows_for for exactly the two template binds; assert via the
    // rendered text (the moved host is unreadable). `RowCell::to_node` emits `TextContent::Static`.
    use carapace::scene::TextContent;
    let scene = engine.layout(100.0, 100.0);
    let texts: Vec<String> = scene
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text { content: TextContent::Static(s), .. } => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"t".to_string()), "title cell rendered from rows_for");
    assert!(texts.contains(&"d".to_string()), "dur cell rendered from rows_for");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace --test list_layout engine_passes_template_binds_to_rows_for`
Expected: FAIL to compile — `rows_for` is not a member of `Host` (and the engine still calls `rows`, so the override is never reached).

- [ ] **Step 3: Add `rows_for` to the `Host` trait**

In `crates/carapace/src/host.rs`, immediately after the existing `rows` default (line 65-67), add:

```rust
    /// Field-aware variant of [`Host::rows`]: `fields` are the template `bind` names the current
    /// `list{}` needs. Defaults to [`Host::rows`] (field-agnostic); a host that fetches cells lazily
    /// (e.g. the FFI host) overrides this to populate only `fields`.
    fn rows_for(&self, collection: &str, _fields: &[&str]) -> Vec<Row> {
        self.rows(collection)
    }
```

- [ ] **Step 4: Make the engine call `rows_for` with the template binds**

In `crates/carapace/src/engine.rs`, replace line 240 (`let rows = host.rows(&collection);`) with:

```rust
        let fields: Vec<&str> = template.iter().map(|c| c.bind.as_str()).collect();
        let rows = host.rows_for(&collection, &fields);
```

(`template` is already destructured from the `Node::List` at this point — see the match around `engine.rs:203-240`.)

- [ ] **Step 5: Run the new test + the whole list-layout suite**

Run: `cargo test -p carapace --test list_layout`
Expected: PASS — the new test and all existing tests (they implement `rows`; `rows_for` delegates to it, so behavior is unchanged).

- [ ] **Step 6: Commit**

```bash
git add crates/carapace/src/host.rs crates/carapace/src/engine.rs crates/carapace/tests/list_layout.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(engine): field-aware Host::rows_for seam for list rows"
```

---

### Task 2: FFI vtable v3 — collections + action args, ABI bump to 3.0

**Files:**
- Modify: `crates/carapace-ffi/src/guard.rs` (bump `CARAPACE_ABI_MAJOR` to 3)
- Modify: `crates/carapace-ffi/src/lib.rs:40-44` (update `abi_version` test)
- Modify: `crates/carapace-ffi/src/host.rs` (add 4 vtable fields; implement `rows_for` + arg-aware `invoke`)
- Modify: `crates/carapace-ffi/include/carapace.h` (regenerated)
- Test: `crates/carapace-ffi/src/host.rs` (unit tests, same file)

**Interfaces:**
- Consumes: `Host::rows_for` (Task 1).
- Produces (C ABI): four new nullable fields on `CarapaceHostVTable`:
  - `row_count: Option<extern "C" fn(*mut c_void, *const c_char) -> u32>`
  - `get_row_str: Option<extern "C" fn(*mut c_void, *const c_char, u32, *const c_char, *mut c_char, usize) -> bool>`
  - `get_row_num: Option<extern "C" fn(*mut c_void, *const c_char, u32, *const c_char, *mut f64) -> bool>`
  - `invoke_arg: Option<extern "C" fn(*mut c_void, *const c_char, f64)>`

- [ ] **Step 1: Write the failing tests**

In `crates/carapace-ffi/src/host.rs` `mod tests`, add fakes + tests:

```rust
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
```

Also update the existing `vtable()` helper and the two `CarapaceHostVTable { .. }` literals in `host.rs` tests to include the four new fields set to `None` (so they still compile).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-ffi --lib host::`
Expected: FAIL to compile — `CarapaceHostVTable` has no field `row_count` (etc.).

- [ ] **Step 3: Add the four fields to the vtable struct**

In `crates/carapace-ffi/src/host.rs`, append inside `pub struct CarapaceHostVTable { ... }` (after `frame_ready`):

```rust
    /// v3: number of rows in `collection` (NUL-terminated). Null = no collections.
    pub row_count: Option<extern "C" fn(*mut c_void, *const c_char) -> u32>,
    /// v3: write row `index`'s string `field` into `buf` (cap `cap`), NUL-terminated; return
    /// `true` if present. Tried before `get_row_num` (mirrors `get`).
    pub get_row_str:
        Option<extern "C" fn(*mut c_void, *const c_char, u32, *const c_char, *mut c_char, usize) -> bool>,
    /// v3: write row `index`'s numeric `field` through `out`; return `true` if present.
    pub get_row_num:
        Option<extern "C" fn(*mut c_void, *const c_char, u32, *const c_char, *mut f64) -> bool>,
    /// v3: perform a host action carrying a single numeric argument (the C mirror of
    /// `Host::invoke` with one `Value::Num`, e.g. `seek`, `set_volume`, `play_index`).
    pub invoke_arg: Option<extern "C" fn(*mut c_void, *const c_char, f64)>,
```

- [ ] **Step 4: Implement `rows_for` and arg-aware `invoke` on `FfiHost`**

In `crates/carapace-ffi/src/host.rs`, replace `FfiHost`'s `invoke` (currently host.rs:102) and `rows` (host.rs:108) with:

```rust
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
```

- [ ] **Step 5: Fix every other `CarapaceHostVTable { .. }` literal**

The struct gained fields, so every literal in the crate must set them. Find and update each:

Run: `grep -rn 'CarapaceHostVTable {' crates/carapace-ffi/src`
Add `row_count: None, get_row_str: None, get_row_num: None, invoke_arg: None,` to each literal in `render_thread.rs` (the `render_tests` + `pacing_tests` modules) and any in `handle.rs`/`lib.rs`. (The `host.rs` test literals were handled in Step 1.)

- [ ] **Step 6: Bump the ABI major + its test**

In `crates/carapace-ffi/src/guard.rs`, change `CARAPACE_ABI_MAJOR` from `2` to `3`.
In `crates/carapace-ffi/src/lib.rs`, update the test (lines 40-44):

```rust
    fn abi_version_is_v3() {
        assert_eq!(carapace_abi_version(), 3 << 16);
        assert_eq!(CARAPACE_ABI_MAJOR, 3);
        assert_eq!(CARAPACE_ABI_MINOR, 0);
    }
```

- [ ] **Step 7: Run the unit tests**

Run: `cargo test -p carapace-ffi --lib`
Expected: PASS (`rows_for_materializes_requested_fields`, `invoke_forwards_numeric_arg_else_falls_back`, `abi_version_is_v3`, and existing host tests).

- [ ] **Step 8: Regenerate + verify the header**

Run:
```bash
cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact
cargo test -p carapace-ffi --test header header_is_fresh
```
Expected: `header_is_fresh` PASSES. Confirm `include/carapace.h` now shows `#define CARAPACE_ABI_MAJOR 3`, the four new vtable fields, and no unrelated diff.

- [ ] **Step 9: Commit**

```bash
git add crates/carapace-ffi/src/guard.rs crates/carapace-ffi/src/lib.rs \
        crates/carapace-ffi/src/host.rs crates/carapace-ffi/src/render_thread.rs \
        crates/carapace-ffi/include/carapace.h
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(ffi): ABI 3.0 vtable — collections (rows) + numeric action args"
```

---

### Task 3: `carapace_swap_skin` — live skin hot-swap

**Files:**
- Modify: `crates/carapace-ffi/src/queue.rs` (add `Command::SwapSkin`; drop `Copy`)
- Modify: `crates/carapace-ffi/src/render_thread.rs` (handle `SwapSkin` in `apply`)
- Modify: `crates/carapace-ffi/src/handle.rs` (add `carapace_swap_skin` export)
- Modify: `crates/carapace-ffi/include/carapace.h` (regenerated)
- Test: `crates/carapace-ffi/src/queue.rs` (ordering) + `crates/carapace-ffi/src/render_thread.rs` (macOS gpu swap test)

**Interfaces:**
- Consumes: `carapace::skin::load_dir`, `carapace::command::Command::Swap`, `Engine::handle_command`, `Engine::scene().canvas`, the `CarapaceEngine` front-end (`e.tx`, `e.poisoned`, `e.enter_poisoned()`).
- Produces (C ABI): `CarapaceStatus carapace_swap_skin(CarapaceEngine *ptr, const char *skin_dir);`

- [ ] **Step 1: Write the failing queue-ordering test**

In `crates/carapace-ffi/src/queue.rs` `mod tests`, add:

```rust
    #[test]
    fn swap_skin_is_preserved_in_order_and_not_coalesced() {
        let (tx, rx) = channel::<Command>();
        let (rtx, _rrx) = channel::<crate::guard::CarapaceStatus>();
        tx.send(Command::SwapSkin { dir: "/tmp/a".into(), reply: rtx.clone() }).unwrap();
        tx.send(Command::Invalidate).unwrap();
        let mut out = Vec::new();
        drain_coalescing(&rx, Command::SetFrameRate(30), &mut out);
        assert!(matches!(out[0], Command::SetFrameRate(30)));
        assert!(matches!(out[1], Command::SwapSkin { .. }));
        assert!(matches!(out[2], Command::Invalidate));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace-ffi --lib queue::`
Expected: FAIL to compile — no variant `SwapSkin`.

- [ ] **Step 3: Add the `SwapSkin` command variant**

In `crates/carapace-ffi/src/queue.rs`: change the enum's derive from `#[derive(Clone, Copy, Debug)]` to `#[derive(Clone, Debug)]`, add the import `use crate::guard::CarapaceStatus;` at the top, and add the variant (before the `#[cfg(test)] ForcePanic`):

```rust
    /// Load the skin at `dir` and swap it in on the render thread, keeping the host. `reply`
    /// receives the outcome so `carapace_swap_skin` can report `ErrBadSkin` synchronously.
    SwapSkin {
        dir: std::path::PathBuf,
        reply: std::sync::mpsc::Sender<CarapaceStatus>,
    },
```

(`drain_coalescing`/`push_coalesced` move values and never require `Copy`; `SwapSkin` is not a `Move`, so it is never coalesced.)

- [ ] **Step 4: Run the queue test**

Run: `cargo test -p carapace-ffi --lib queue::`
Expected: PASS.

- [ ] **Step 5: Handle `SwapSkin` on the render thread**

In `crates/carapace-ffi/src/render_thread.rs`, in `RenderThread::apply`'s `match cmd { ... }`, add an arm (before the `#[cfg(test)] Command::ForcePanic`):

```rust
            Command::SwapSkin { dir, reply } => {
                let status = match carapace::skin::load_dir(&dir) {
                    Ok((_m, source)) => {
                        self.engine
                            .handle_command(carapace::command::Command::Swap(source));
                        let (cw, ch) = self.engine.scene().canvas;
                        self.cw = cw;
                        self.ch = ch;
                        *invalidated = true; // draw the swapped-in skin immediately
                        CarapaceStatus::Ok
                    }
                    Err(e) => {
                        set_last_error(&format!("swap_skin: load failed: {e:?}"));
                        CarapaceStatus::ErrBadSkin
                    }
                };
                let _ = reply.send(status);
            }
```

(`set_last_error` and `CarapaceStatus` are already imported at the top of `render_thread.rs`; `PathBuf` too.)

- [ ] **Step 6: Add the `carapace_swap_skin` export**

In `crates/carapace-ffi/src/handle.rs`, after `carapace_release_surface`, add (match the surrounding `#[unsafe(no_mangle)]` + cfg gating of the other exports in this file):

```rust
/// Swap the running skin to the one at `skin_dir`, keeping the host, GPU, and render thread.
/// Synchronous: blocks until the render thread has loaded + applied the new skin (~<=1 frame), so
/// a bad skin dir is reported as `ErrBadSkin` on the caller's thread. On failure the current skin is
/// kept. The IOSurface pool and window are unchanged — author skins to a shared design canvas.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`;
/// `skin_dir` must be a valid NUL-terminated UTF-8 path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_swap_skin(
    ptr: *mut CarapaceEngine,
    skin_dir: *const std::os::raw::c_char,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if skin_dir.is_null() {
        set_last_error("carapace_swap_skin: null skin_dir");
        return CarapaceStatus::ErrNullArg;
    }
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) {
        return e.enter_poisoned();
    }
    let dir = match unsafe { std::ffi::CStr::from_ptr(skin_dir) }.to_str() {
        Ok(s) => std::path::PathBuf::from(s),
        Err(_) => {
            set_last_error("carapace_swap_skin: skin_dir is not valid UTF-8");
            return CarapaceStatus::ErrNullArg;
        }
    };
    let (reply_tx, reply_rx) = std::sync::mpsc::channel::<CarapaceStatus>();
    if e.tx.send(Command::SwapSkin { dir, reply: reply_tx }).is_err() {
        return e.enter_poisoned();
    }
    // The render thread always replies unless it died (dropping the sender → recv Err).
    reply_rx.recv().unwrap_or_else(|_| e.enter_poisoned())
}
```

Confirm `set_last_error` is imported in `handle.rs` (it is used by `carapace_create`); add the import if the linter flags it.

- [ ] **Step 7: Write the macOS gpu swap test**

In `crates/carapace-ffi/src/render_thread.rs`, inside `#[cfg(all(test, target_os = "macos"))] mod render_tests`, add. It swaps from the default test skin to a second on-disk skin dir and asserts a frame still renders. Use the demo skins as the swap targets:

```rust
    #[test]
    fn swap_skin_applies_and_bad_dir_is_rejected() {
        let (w, h) = (300u32, 140u32);
        let vt = crate::host::CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None, get_str: None, invoke: None, frame_ready: None,
            row_count: None, get_row_str: None, get_row_num: None, invoke_arg: None,
        };
        let (handle, surfaces) =
            crate::handle::test_support::create_test_handle_pool_vt(w, h, 2, vt);
        assert_eq!(
            unsafe { crate::handle::carapace_set_frame_rate(handle, 0) },
            crate::guard::CarapaceStatus::Ok
        );
        // A valid skin dir → Ok, and a following invalidate renders a non-blank frame. The test
        // fixture loads `skins/classic` by default, so swap to a DIFFERENT base-vocab skin
        // (`minimal`) to prove a real content swap. Both load under `VocabRegistry::base()`.
        let good = std::ffi::CString::new(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../carapace-demo/skins/minimal")
        ).unwrap();
        assert_eq!(
            unsafe { crate::handle::carapace_swap_skin(handle, good.as_ptr()) },
            crate::guard::CarapaceStatus::Ok
        );
        assert_eq!(
            unsafe { crate::handle::carapace_invalidate(handle) },
            crate::guard::CarapaceStatus::Ok
        );
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || {
            unsafe { crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h) }
        });
        assert!(unsafe {
            crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h)
        });
        // A bad dir → ErrBadSkin, engine intact.
        let bad = std::ffi::CString::new("/no/such/skin/dir").unwrap();
        assert_eq!(
            unsafe { crate::handle::carapace_swap_skin(handle, bad.as_ptr()) },
            crate::guard::CarapaceStatus::ErrBadSkin
        );
        unsafe { crate::handle::carapace_destroy(handle) };
    }
```

(Verified: `create_test_handle_pool_vt` loads `/../carapace-demo/skins/classic`; `skins/minimal` is a distinct skin that uses only base vocab, so it swaps in cleanly under `VocabRegistry::base()`.)

- [ ] **Step 8: Run the tests**

Run: `cargo test -p carapace-ffi --lib`
Expected: PASS, including `swap_skin_applies_and_bad_dir_is_rejected` (macOS).

- [ ] **Step 9: Regenerate + verify the header**

Run:
```bash
cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact
cargo test -p carapace-ffi --test header header_is_fresh
```
Expected: `header_is_fresh` PASSES; `include/carapace.h` now declares `carapace_swap_skin` (under the `CARAPACE_APPLE` guard like the other exports). `Command::SwapSkin` is internal (not `#[repr(C)]`) so it does NOT appear in the header.

- [ ] **Step 10: Commit**

```bash
git add crates/carapace-ffi/src/queue.rs crates/carapace-ffi/src/render_thread.rs \
        crates/carapace-ffi/src/handle.rs crates/carapace-ffi/include/carapace.h
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(ffi): carapace_swap_skin — live skin hot-swap keeping host + GPU"
```

---

### Final verification (after all tasks)

- [ ] **Step 1: Full workspace test + clippy**

Run:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: both PASS. (If the headless `check` lane can't build wgpu/GPU tests, the FFI render/swap tests are macOS-gated and run in the macOS lane; the ungated queue/host/abi tests run everywhere.)

- [ ] **Step 2: Confirm ABI surface**

Run: `grep -n 'CARAPACE_ABI_MAJOR\|carapace_swap_skin\|row_count\|invoke_arg' crates/carapace-ffi/include/carapace.h`
Expected: `#define CARAPACE_ABI_MAJOR 3`, the `carapace_swap_skin` declaration, and the four new vtable fields all present.

## Self-review notes (already reconciled)

- **Spec refinement:** the spec's Change 2 proposed changing `Host::rows`'s signature (12 sites);
  this plan instead ADDS `rows_for` with a delegating default (Task 1), so native/test hosts are
  untouched. Functionally identical, less churn. (Update the spec's Change 2 wording to match if desired.)
- **Type consistency:** `rows_for(collection, fields)`, the four vtable field names, and
  `Command::SwapSkin { dir, reply }` are used identically across tasks.
- **Header freshness** is regenerated in both Task 2 (vtable fields + const) and Task 3 (export).

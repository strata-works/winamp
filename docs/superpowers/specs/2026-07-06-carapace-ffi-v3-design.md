# carapace-ffi v3 Design (ABI 3.0)

**Date:** 2026-07-06
**Status:** Design — pending spec review
**Parent program:** `2026-07-06-showcase-three-skins-design.md` (Sub-project A, build first)
**Depends on:** nothing new — extends `crates/carapace-ffi` (ABI 2.0) and `crates/carapace`.

## Goal

Extend the carapace C ABI so a native host (the SwiftUI music app) can drive a **multi-skin,
list-and-argument-driven** application. Three additive capabilities, shipped together as **ABI 3.0**:

1. **Skin hot-swap** — `carapace_swap_skin` swaps the skin on the live render thread, keeping the
   host (and GPU/render thread), instead of destroy+recreate.
2. **Collections/rows** — a vtable path so `list{ collection=... }` renders host-provided rows.
3. **Action arguments** — a vtable path so parameterized actions (`seek(f)`, `set_volume(f)`,
   `play_index(i)`) convey their numeric value.

Non-goals (deferred): string/bool action args (these skins need only `Num`); per-row numeric-only
optimizations; Windows/Linux/Android; the SwiftUI app and skins (Sub-projects B and C).

## ABI versioning

Bump `CARAPACE_ABI_MAJOR` **2 → 3** (`CARAPACE_ABI_MINOR` 0). Appending fields to the
host-allocated `CarapaceHostVTable` changes its binary layout, so this is a major bump even though
every new callback is a nullable `Option<fn>`. `carapace_abi_version()` returns `3 << 16 | 0`.
The header test (`tests/header.rs`) regenerates `include/carapace.h` via cbindgen.

---

## Change 1 — Skin hot-swap (`carapace_swap_skin`)

### C ABI

```c
/// Swap the running skin to the one at `skin_dir`, keeping the host, GPU, and render thread.
/// Synchronous: blocks until the render thread has loaded + applied the new skin (~<=1 frame),
/// so a bad skin dir is reported as `ErrBadSkin` on the caller's thread. On failure the current
/// skin is kept (never leaves the engine skinless). The IOSurface pool and window are unchanged;
/// authors should target a shared design canvas so no resize is needed (see program design).
///
/// # Safety
/// `ptr` must come from `carapace_create` and not have been passed to `carapace_destroy`;
/// `skin_dir` a valid NUL-terminated UTF-8 path.
CarapaceStatus carapace_swap_skin(CarapaceEngine *ptr, const char *skin_dir);
```

### Rust implementation

- **`queue.rs`:** `Command` currently `#[derive(Clone, Copy)]`. Add a variant carrying an owned path
  and a reply channel; drop `Copy` (keep `Clone` — `PathBuf` and `mpsc::Sender` are `Clone`).
  `drain_coalescing` moves values (never requires `Copy`); `SwapSkin` is not a `Move` so it is never
  coalesced.

  ```rust
  Command::SwapSkin {
      dir: std::path::PathBuf,
      reply: std::sync::mpsc::Sender<CarapaceStatus>,
  }
  ```

- **`render_thread.rs` `apply`:** load on the render thread (SkinSource is `!Send`), keep the host:

  ```rust
  Command::SwapSkin { dir, reply } => {
      let status = match carapace::skin::load_dir(&dir) {
          Ok((_m, source)) => {
              self.engine.handle_command(carapace::command::Command::Swap(source));
              let (cw, ch) = self.engine.scene().canvas; // stays constant for shared-canvas skins
              self.cw = cw;
              self.ch = ch;
              *invalidated = true; // draw the new skin immediately
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

  (`engine.apply_swap` already logs + keeps the current skin if the *rebuild* fails; the `load_dir`
  guard above covers a missing/invalid dir before rebuild.)

- **`handle.rs` export:** mirror the create-time handshake — enqueue `SwapSkin` with a fresh reply
  channel, block on `reply.recv()`, translate to status. Guarded by the standard poison/null checks
  used by the other exports. Null/invalid path → `ErrNullArg`; render thread gone → `ErrPoisoned`.

### Tests

- `queue.rs`: `SwapSkin` is preserved in order through `drain_coalescing` (not coalesced).
- `render_thread.rs` (macos gpu-test): create paused; `carapace_swap_skin` to a second valid skin
  dir returns `Ok`; a following `carapace_invalidate` renders a frame whose canvas matches the new
  skin. Swapping to a non-existent dir returns `ErrBadSkin` and a subsequent frame still renders the
  original skin (host + engine intact).

---

## Change 2 — Collections / rows

### Engine seam change (`crates/carapace`)

`Host::rows` cannot currently tell the FFI host *which* fields to fetch. Change its signature to
pass the template's bound field names (known at the call site, `engine.rs:240`). This is an internal
trait change (4 impls: `MusicPlayerHost`, `SysmonHost`, `FileBrowserHost`, `FfiHost`, plus tests);
not part of the C ABI.

```rust
// carapace/src/host.rs — Host trait
fn rows(&self, _collection: &str, _fields: &[&str]) -> Vec<Row> {
    Vec::new()
}
```

- **`engine.rs` `expand_lists`:** collect the binds and pass them:

  ```rust
  let fields: Vec<&str> = template.iter().map(|c| c.bind.as_str()).collect();
  let rows = host.rows(&collection, &fields);
  ```

- Native hosts (`carapace-demo`) ignore `_fields` and keep building full rows exactly as today
  (they already populate every field). Only their method signature changes.

### C ABI — three new nullable vtable callbacks

Append to `CarapaceHostVTable` (all `Option`, so a host that doesn't use lists leaves them null):

```c
typedef struct {
  void *ctx;
  bool (*get_num)(void*, const char*, double*);
  bool (*get_str)(void*, const char*, char*, uintptr_t);
  void (*invoke)(void*, const char*);
  void (*frame_ready)(void*, uint32_t, uint64_t);
  /* v3: collections */
  uint32_t (*row_count)(void* ctx, const char* collection);
  bool (*get_row_str)(void* ctx, const char* collection, uint32_t index,
                      const char* field, char* buf, uintptr_t cap);
  bool (*get_row_num)(void* ctx, const char* collection, uint32_t index,
                      const char* field, double* out);
  /* v3: action args (Change 3) */
  void (*invoke_arg)(void* ctx, const char* action, double arg);
} CarapaceHostVTable;
```

### Rust implementation (`host.rs` `FfiHost::rows`)

```rust
fn rows(&self, collection: &str, fields: &[&str]) -> Vec<Row> {
    let (Some(count_fn), Ok(ccol)) = (self.vtable.row_count, CString::new(collection))
        else { return Vec::new() };
    let n = count_fn(self.vtable.ctx, ccol.as_ptr());
    (0..n).map(|i| {
        let mut row = Row::new();
        for &field in fields {
            let Ok(cfield) = CString::new(field) else { continue };
            // numeric first, then string (mirrors `get`)
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
                    let last = buf.len() - 1; buf[last] = 0;
                    let s = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
                        .to_string_lossy().into_owned();
                    row = row.set(field, StateValue::Str(std::sync::Arc::from(s.as_str())));
                }
            }
        }
        row
    }).collect()
}
```

### Tests

- `host.rs`: a fake vtable exposing a 2-row `playlist` with `title` (str) + `duration` (str);
  `FfiHost::rows("playlist", &["title","duration"])` returns 2 rows with the right cell values;
  `row_count` null → empty; unknown collection (count 0) → empty.

---

## Change 3 — Action arguments (`invoke_arg`)

The `invoke_arg` field is added to the vtable in Change 2's struct. `FfiHost::invoke` forwards a
single numeric arg when present, falling back to the parameterless `invoke`:

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
```

(Non-`Num` first args, or a null `invoke_arg`, fall through to the parameterless callback — matching
today's behavior. String/bool args are out of scope; note in the doc comment.)

### Tests

- `host.rs`: a fake vtable recording `(action, arg)`; `FfiHost::invoke("seek", &[Value::Num(0.5)])`
  calls `invoke_arg` with `0.5`; `FfiHost::invoke("toggle_play", &[])` calls the parameterless
  `invoke`; with `invoke_arg == None`, `invoke("seek", &[Num(0.5)])` still calls plain `invoke`.

---

## Header + cross-cutting

- Regenerate `include/carapace.h` (cbindgen) and update `tests/header.rs` expectations; bump the
  `CARAPACE_ABI_MAJOR` assertion to 3.
- Update the mirrored headers used by embed samples only if a sample is rebuilt in Sub-project B
  (out of scope here; note it).
- `docs/api` rustdoc: the new exports/callbacks carry `///` docs (crate is `#![deny(missing_docs)]`).
- Run `cargo clippy -D warnings` and the `gpu-tests` feature lane before finishing (CI gates on
  both; the swap/render tests belong behind `#[cfg(feature = "gpu-tests")]` or the headless lane).

## Migration impact

- `crates/carapace-demo` Host impls (`MusicPlayerHost`, `SysmonHost`, `FileBrowserHost`) and any
  test hosts get the one-line `rows(&self, _collection, _fields)` signature change — behavior
  unchanged.
- Any external ABI 2.0 host must recompile against the new header (major bump); the create desc,
  existing callbacks, and existing exports are unchanged in meaning.

## Definition of done

- `carapace_abi_version()` returns `3<<16`; header regenerated; `tests/header.rs` green.
- `carapace_swap_skin` swaps live (gpu-test proves swap + bad-dir rejection).
- `FfiHost::rows` and `FfiHost::invoke` covered by unit tests above.
- Engine `rows(collection, fields)` change compiles across all impls; existing engine tests green.
- `cargo test --workspace` + `clippy -D warnings` + gpu-tests lane pass.

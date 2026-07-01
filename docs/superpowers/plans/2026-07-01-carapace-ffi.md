# carapace-ffi v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote the throwaway `embed-spike` into a real, safe, versioned C ABI crate `carapace-ffi` for Apple (macOS/iOS): panic-guarded, opaque-handle, cbindgen header, full pointer input, and an engine→host hit-test channel.

**Architecture:** A thin `#[no_mangle] extern "C"` layer wraps an opaque `CarapaceEngine` handle. Every export runs inside a `catch_unwind` guard that, on panic, records a thread-local message and *poisons* the handle (never `abort()`s the host). GPU/present internals port verbatim from the spike except `init_gpu` now returns `Result`. Two small additive changes to the `carapace` engine crate (a hotspot `role` + non-firing `Scene::hit_kind`/`covers`) back the hit-test channel.

**Tech Stack:** Rust (edition 2021), `wgpu`/`wgpu-hal` `=29.0.3`, Metal via `objc2`/`objc2-metal`/`objc2-io-surface`, IOSurface.framework, `cbindgen` for the C header, `mlua` (engine), `pollster`.

## Global Constraints

- **Platform:** Apple only. All GPU/handle/ABI code is `#[cfg(any(target_os = "macos", target_os = "ios"))]`-gated; the crate still compiles (as a near-empty shell exporting only `carapace_abi_version`/`carapace_last_error`) on other targets so Linux CI stays green. Windows/Linux/Android are future work.
- **Never `abort()`** at the boundary. Panics are caught; the host process must survive.
- **wgpu pinned** at `=29.0.3` and `wgpu-hal = "=29.0.3"` (the versions the spike proved). Do not bump.
- **Zero `free()` contract:** strings out are copied into caller buffers; carapace never returns a pointer the caller must free, and never frees a caller pointer.
- **Additive-only ABI:** exports and enum variants are appended, never reordered/removed. `carapace_abi_version()` = `MAJOR<<16 | MINOR`, currently MAJOR=0 MINOR=1.
- **Coordinates:** `carapace_pointer` / `carapace_hit_test` take design-canvas coords; descriptor `w`/`h` are surface pixels.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>`.
- **Dependency fetch policy:** the first fetch of any new third-party crate (here: `cbindgen`) must run through Socket Firewall — `sfw cargo add ...` (or `sfw cargo build` on first pull).
- **Lint gate:** `cargo clippy --all-targets -- -D warnings` must pass (CI gates on it, plus a `gpu-tests` variant). Run `cargo fmt` before every commit.
- **No Claude attribution** in commit messages or PR descriptions.

---

## File map

Engine crate (additive):
- Modify `crates/carapace/src/scene.rs` — add `HotspotRole`, `role` field on `Node::Hotspot`, `HitKind`, `Scene::hit_kind`, `Scene::covers`; update in-file matches/constructors.
- Modify `crates/carapace/src/vocab.rs` — parse `role` in the two hotspot builders.
- Modify `crates/carapace/src/script.rs` — set `role` on its programmatic `Node::Hotspot`.

New crate `crates/carapace-ffi/`:
- `Cargo.toml`, `src/lib.rs` (module gating + `carapace_abi_version`)
- `src/guard.rs` — `CarapaceStatus`, thread-local `last_error`, panic hook, `ffi_guard!`, `carapace_last_error`
- `src/host.rs` — `CarapaceHostVTable` + `FfiHost` (ported)
- `src/render.rs` — GPU/present internals (ported; `init_gpu -> Result`)
- `src/handle.rs` — opaque `CarapaceEngine`, `CarapaceCreateDesc`, create/destroy/tick/pointer/active_tier
- `src/hit.rs` — `CarapaceHitKind` + `carapace_hit_test`
- `cbindgen.toml`, `include/carapace.h` (committed), `tests/header.rs` (freshness)

Workspace:
- Modify root `Cargo.toml` — add `crates/carapace-ffi` member.

---

## Task 1: Engine — hotspot `role` (additive)

**Files:**
- Modify: `crates/carapace/src/scene.rs` (add `HotspotRole`; add `role` to `Node::Hotspot` at line ~169; fix explicit matches at `scene.rs:424,439`)
- Modify: `crates/carapace/src/vocab.rs` (parse `role` at `:155` and `:223`; fix test match at `:830`)
- Modify: `crates/carapace/src/script.rs` (set `role` at `:435`)
- Test: inline `#[cfg(test)]` in `crates/carapace/src/vocab.rs`

**Interfaces:**
- Produces: `pub enum HotspotRole { Control, Drag, Passthrough }` (in `scene.rs`); `Node::Hotspot { region, on_press, role: HotspotRole }`.

- [ ] **Step 1: Write the failing test** — add to `crates/carapace/src/vocab.rs` `#[cfg(test)] mod tests`:

```rust
#[test]
fn hotspot_role_parses_control_default_drag_and_passthrough() {
    use crate::scene::HotspotRole;
    // default (no role) = Control
    let t = table("return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, on_press = function() end }");
    match one(FillPrim.build(&t, &mut NoHandlers)) {
        Node::Hotspot { role, .. } => assert_eq!(role, HotspotRole::Control),
        other => panic!("expected Hotspot, got {other:?}"),
    }
    // explicit drag
    let t = table("return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, role='drag', on_press = function() end }");
    match one(FillPrim.build(&t, &mut NoHandlers)) {
        Node::Hotspot { role, .. } => assert_eq!(role, HotspotRole::Drag),
        other => panic!("expected Hotspot, got {other:?}"),
    }
    // explicit passthrough
    let t = table("return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, role='passthrough', on_press = function() end }");
    match one(FillPrim.build(&t, &mut NoHandlers)) {
        Node::Hotspot { role, .. } => assert_eq!(role, HotspotRole::Passthrough),
        other => panic!("expected Hotspot, got {other:?}"),
    }
}
```

Note: reuse whatever the existing tests use to build a Lua `Table` (see the sibling tests near `vocab.rs:667` — mirror their `table(...)`/`one(...)`/`NoHandlers` helpers exactly; if the helper is named differently, match the existing name).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace hotspot_role_parses -- --nocapture`
Expected: FAIL — `no field 'role'` / `HotspotRole` unresolved.

- [ ] **Step 3a: Add the enum + field** in `crates/carapace/src/scene.rs`. After the `Node` enum (or near `HandlerId`), add:

```rust
/// Author-declared interaction role for a `hotspot{}`, reported by [`Scene::hit_kind`] so a host
/// can classify an OS event without firing the hotspot's Lua handler. Default is `Control`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotspotRole {
    /// Skin consumes the event (a button/control). Default.
    Control,
    /// Host should treat the region as window chrome (move the window).
    Drag,
    /// Event falls through to whatever is behind the skin (a deliberate hole).
    Passthrough,
}
```

Then change the `Node::Hotspot` variant (around line 169) to:

```rust
    Hotspot {
        region: Region,
        on_press: HandlerId,
        role: HotspotRole,
    },
```

- [ ] **Step 3b: Fix the two explicit-field matches** in `scene.rs`. At `:424` (`fn hit`) change the pattern `Node::Hotspot { region, on_press }` → `Node::Hotspot { region, on_press, .. }`. At `:439` (`fn hit_any`) change `Node::Hotspot { region, on_press }` → `Node::Hotspot { region, on_press, .. }`. (The `summary()` match at `:259` already uses `..` — leave it.)

- [ ] **Step 3c: Parse `role` in `vocab.rs`.** Add this helper near `maybe_hotspot` (after line ~161):

```rust
fn parse_role(args: &Table) -> Result<crate::scene::HotspotRole, BuildError> {
    use crate::scene::HotspotRole;
    // Lenient + additive: unknown/absent → Control (never rejects an existing skin).
    Ok(match args.get::<Option<String>>("role")?.as_deref() {
        Some("drag") => HotspotRole::Drag,
        Some("passthrough") => HotspotRole::Passthrough,
        _ => HotspotRole::Control,
    })
}
```

In `maybe_hotspot` (`:155`) set the field:

```rust
        Some(f) => Ok(Some(Node::Hotspot {
            region,
            on_press: ctx.register_handler(f),
            role: parse_role(args)?,
        })),
```

In `RegionPrim::build` (`:223`):

```rust
        Ok(vec![Node::Hotspot {
            region: crate::scene::region_of(&path),
            on_press: id,
            role: parse_role(args)?,
        }])
```

- [ ] **Step 3d: Fix remaining constructors/matches.** In `crates/carapace/src/script.rs:435`, add `role: crate::scene::HotspotRole::Control,` to the `Node::Hotspot { .. }` literal. In `crates/carapace/src/vocab.rs:830`, change the test match `Node::Hotspot { on_press, region }` → `Node::Hotspot { on_press, region, .. }`. Then build to surface any other constructor the compiler flags: `cargo build -p carapace` and add `role: HotspotRole::Control` / `..` to each site it reports (expected: the three `#[cfg(test)]` constructors in `scene.rs` around `:513,:562,:600` — add `role: HotspotRole::Control,`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace`
Expected: PASS (new role test + all existing engine tests still green).

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add crates/carapace/src/scene.rs crates/carapace/src/vocab.rs crates/carapace/src/script.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(engine): declarative hotspot role (control/drag/passthrough) for FFI hit-test"
```

---

## Task 2: Engine — `Scene::hit_kind` + `Scene::covers` (additive)

**Files:**
- Modify: `crates/carapace/src/scene.rs` (add `HitKind`, `covers`, `hit_kind`)
- Test: inline `#[cfg(test)]` in `crates/carapace/src/scene.rs`

**Interfaces:**
- Consumes: `HotspotRole`, `Node`, `Pt`, `region_of`, `hittest::{Point, Region}`.
- Produces: `pub enum HitKind { Passthrough, Control, Drag }`; `Scene::hit_kind(&self, p: Pt) -> HitKind`; `Scene::covers(&self, p: Pt) -> bool`.

Semantics (elaborates the spec's C1): topmost interactive node decides — a `role=drag` hotspot → `Drag`; a `role=passthrough` hotspot → `Passthrough`; any control (button/list-row/scrub or `role=control` hotspot) → `Control`. With no interactive node under `p`: `Control` if the point is inside the skin's opaque coverage (visible skin swallows the event), else `Passthrough` (transparent → falls through). No Lua is fired.

- [ ] **Step 1: Write the failing test** — add to the `#[cfg(test)] mod tests` in `scene.rs`:

```rust
#[test]
fn hit_kind_classifies_drag_control_and_passthrough() {
    use crate::scene::{HitKind, HotspotRole, ImageDest, Node, Scene};
    use hittest::region_rect; // if absent, build the Region inline as the other tests do

    // A 100x100 canvas: a drag hotspot over the left half, a control image over the right half.
    let drag = Node::Hotspot {
        region: crate::scene::region_of(&[
            Pt { x: 0.0, y: 0.0 }, Pt { x: 50.0, y: 0.0 },
            Pt { x: 50.0, y: 100.0 }, Pt { x: 0.0, y: 100.0 },
        ]),
        on_press: 0,
        role: HotspotRole::Drag,
    };
    let img = Node::Image {
        image: std::sync::Arc::new(crate::asset::DecodedImage::solid_test(10, 10)),
        dest: ImageDest { x: 50.0, y: 0.0, w: 50.0, h: 100.0 },
    };
    let scene = Scene { nodes: vec![drag, img], canvas: (100, 100) };

    assert_eq!(scene.hit_kind(Pt { x: 10.0, y: 50.0 }), HitKind::Drag);      // over drag hotspot
    assert_eq!(scene.hit_kind(Pt { x: 75.0, y: 50.0 }), HitKind::Control);   // over opaque image
    assert_eq!(scene.hit_kind(Pt { x: 200.0, y: 200.0 }), HitKind::Passthrough); // outside all nodes
    assert!(scene.covers(Pt { x: 75.0, y: 50.0 }));
    assert!(!scene.covers(Pt { x: 200.0, y: 200.0 }));
}
```

Note: use whatever constructor the codebase already provides for a test `DecodedImage`. If none exists, replace `img` with a `Node::Fill` covering the right half (`Fill { path: <right-half rect>, paint: <any solid> }`) — `covers` treats any `Fill`'s `region_of(path)` as opaque, so the assertions hold identically. Pick the variant that compiles against the real `asset`/`scene` API.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace hit_kind_classifies -- --nocapture`
Expected: FAIL — `no method named 'hit_kind'` / `HitKind` unresolved.

- [ ] **Step 3: Implement `HitKind`, `covers`, `hit_kind`** in `scene.rs`. Add the enum near `Hit`:

```rust
/// Coarse interaction classification of a point for a host embedder — see [`Scene::hit_kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    /// Event should fall through the skin (transparent, or a `role=passthrough` region).
    Passthrough,
    /// The skin consumes the event (a control, or opaque non-interactive skin).
    Control,
    /// Host should move the window (a `role=drag` region).
    Drag,
}
```

Add both methods in the `impl Scene` block (near `hit_any`):

```rust
    /// True if `p` falls inside any drawn node's bounds — the skin's opaque coverage geometry.
    /// Rect-bounded nodes use their dest/region; polygon nodes use `region_of(path)`. `Text` is
    /// ignored (no reliable glyph bounds). This is the geometry a host uses for a shaped-window /
    /// click-through mask.
    pub fn covers(&self, p: Pt) -> bool {
        let inside_rect = |x: f32, y: f32, w: f32, h: f32| p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h;
        self.nodes.iter().any(|node| match node {
            Node::Hotspot { region, .. } => region.contains(Point { x: p.x, y: p.y }),
            Node::Fill { path, .. } | Node::ValueFill { path, .. } => {
                region_of(path).contains(Point { x: p.x, y: p.y })
            }
            Node::Image { dest, .. } | Node::Frame { dest, .. } | Node::View { dest, .. } => {
                inside_rect(dest.x, dest.y, dest.w, dest.h)
            }
            Node::List { region, .. } | Node::Scrub { region, .. } => {
                inside_rect(region.x, region.y, region.w, region.h)
            }
            Node::Text { .. } => false,
        })
    }

    /// Classify `p` for a host embedder WITHOUT firing any Lua handler (unlike
    /// `handle_pointer_resolved`). Topmost interactive node decides; otherwise opaque coverage vs.
    /// transparent. See [`HitKind`].
    pub fn hit_kind(&self, p: Pt) -> HitKind {
        let pt = Point { x: p.x, y: p.y };
        for node in self.nodes.iter().rev() {
            match node {
                Node::Hotspot { region, role, .. } if region.contains(pt) => {
                    return match role {
                        HotspotRole::Drag => HitKind::Drag,
                        HotspotRole::Passthrough => HitKind::Passthrough,
                        HotspotRole::Control => HitKind::Control,
                    };
                }
                Node::List { region, row_height, on_select: Some(_), count, .. }
                    if *row_height > 0.0
                        && *count > 0
                        && p.x >= region.x
                        && p.x <= region.x + region.w
                        && p.y >= region.y
                        && ((p.y - region.y) / row_height).floor() < *count as f32 =>
                {
                    return HitKind::Control;
                }
                Node::Scrub { region, .. }
                    if p.x >= region.x
                        && p.x <= region.x + region.w
                        && p.y >= region.y
                        && p.y <= region.y + region.h =>
                {
                    return HitKind::Control;
                }
                _ => {}
            }
        }
        if self.covers(p) { HitKind::Control } else { HitKind::Passthrough }
    }
```

Ensure `HotspotRole` is in scope (add to the existing `use` or reference as `crate::scene::HotspotRole`; it is defined in this same file).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace hit_kind_classifies` then `cargo test -p carapace`
Expected: PASS (new test + full engine suite green).

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add crates/carapace/src/scene.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(engine): Scene::hit_kind + covers for the FFI engine→host hit-test channel"
```

---

## Task 3: FFI crate scaffold + status + last_error

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `crates/carapace-ffi/Cargo.toml`
- Create: `crates/carapace-ffi/src/lib.rs`
- Create: `crates/carapace-ffi/src/guard.rs`
- Test: inline `#[cfg(test)]` in `guard.rs`

**Interfaces:**
- Produces: `#[repr(i32)] pub enum CarapaceStatus { Ok=0, ErrNullArg=1, ErrBadSkin=2, ErrGpuInit=3, ErrPoisoned=4, ErrPanic=5 }`; `pub fn set_last_error(&str)`; `extern "C" fn carapace_last_error(*mut c_char, usize) -> usize`; `extern "C" fn carapace_abi_version() -> u32`; consts `CARAPACE_ABI_MAJOR=0`, `CARAPACE_ABI_MINOR=1`.

- [ ] **Step 1: Add the workspace member** — edit root `Cargo.toml`:

```toml
[workspace]
members = ["crates/hittest", "crates/carapace", "crates/carapace-demo", "crates/window-spike", "crates/embed-spike", "crates/carapace-ffi"]
resolver = "2"
```

- [ ] **Step 2: Create `crates/carapace-ffi/Cargo.toml`**

```toml
[package]
name = "carapace-ffi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "staticlib", "rlib"]   # cdylib+staticlib: host apps link one or the other

[dependencies]
carapace = { path = "../carapace" }
libc = "0.2.186"

# GPU/Metal/IOSurface deps are Apple-only; on other targets the crate is a near-empty shell.
[target.'cfg(any(target_os = "macos", target_os = "ios"))'.dependencies]
pollster = "0.4.0"
wgpu = "29.0.3"
wgpu-hal = "=29.0.3"
core-foundation = "0.10.1"
objc2 = "0.6"
objc2-io-surface = "0.3.2"
objc2-metal = { version = "0.3.2", features = ["objc2-io-surface"] }

# Only tests create an IOSurface in Rust (macOS-only; transitively links OpenGL via cgl).
[target.'cfg(target_os = "macos")'.dev-dependencies]
io-surface = "0.16.1"

[dev-dependencies]
cbindgen = "0.29"
```

- [ ] **Step 3: Create `crates/carapace-ffi/src/guard.rs`**

```rust
//! Panic-safety, status codes, and the thread-local error channel shared by every export.
//!
//! Boundary policy: every `#[no_mangle]` export wraps its body in `ffi_guard!`, which catches any
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
pub fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| *e.borrow_mut() = msg.to_string());
}

/// Install (once per process) a panic hook that captures the panic message + location into the
/// thread-local BEFORE the unwind reaches `catch_unwind` (whose payload is opaque). Chains the
/// previous hook. Call this at the top of `carapace_create`.
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
#[cfg(any(target_os = "macos", target_os = "ios"))]
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

pub(crate) use ffi_guard_no_handle;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) use ffi_guard;

/// Copy the current thread's last error into `buf` (NUL-terminated, truncated to `cap`). Returns
/// the number of bytes the message needs (excluding NUL), so a caller can size a retry buffer.
/// Passing a null `buf` or `cap == 0` just returns that length.
///
/// # Safety
/// `buf` must be null or point to at least `cap` writable bytes.
#[no_mangle]
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
        assert_eq!(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap(), "boom");

        // Truncation: cap smaller than the message still NUL-terminates.
        set_last_error("abcdefgh");
        let mut small = [0i8; 4]; // room for 3 chars + NUL
        let needed = unsafe { carapace_last_error(small.as_mut_ptr(), small.len()) };
        assert_eq!(needed, 8);
        assert_eq!(unsafe { CStr::from_ptr(small.as_ptr()) }.to_str().unwrap(), "abc");
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
```

- [ ] **Step 4: Create `crates/carapace-ffi/src/lib.rs`**

```rust
//! carapace-ffi — the stable C ABI that lets a host app embed the carapace engine as custom UI.
//! Apple (macOS/iOS) only in this version; see docs/superpowers/specs/2026-07-01-carapace-ffi-design.md.

#[macro_use]
mod guard;
pub mod host;

pub use guard::{carapace_last_error, CarapaceStatus, CARAPACE_ABI_MAJOR, CARAPACE_ABI_MINOR};

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod render;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod handle;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod hit;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use handle::*;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use hit::*;

/// The ABI version this library implements: `MAJOR << 16 | MINOR`. Additive changes bump MINOR;
/// breaking changes bump MAJOR. A host compares this against the header's constants at load time.
#[no_mangle]
pub extern "C" fn carapace_abi_version() -> u32 {
    (CARAPACE_ABI_MAJOR << 16) | CARAPACE_ABI_MINOR
}
```

Create a placeholder `crates/carapace-ffi/src/host.rs` with `// filled in Task 5` and an empty body for now — OR sequence Task 5 before building. To keep this task self-contained, create `host.rs` as an empty module stub:

```rust
//! Host vtable + `FfiHost` — implemented in Task 5.
```

(The `handle`/`render`/`hit` modules are Apple-gated and added in later tasks; on non-Apple they don't exist yet, which is fine. On macOS this task will not yet compile `handle`/`hit` — so for THIS task only, temporarily comment out the three Apple-gated `mod`/`pub use` lines, then restore them in the task that introduces each module. Simpler: leave them commented here and uncomment per later task. Track via the checkboxes in Tasks 6/7/10.)

To avoid churn, use this lib.rs body for Task 3 (Apple modules commented, uncommented as they land):

```rust
#[macro_use]
mod guard;
pub mod host;
pub use guard::{carapace_last_error, CarapaceStatus, CARAPACE_ABI_MAJOR, CARAPACE_ABI_MINOR};

// Apple-gated modules are added in later tasks:
// #[cfg(any(target_os = "macos", target_os = "ios"))] mod render;   // Task 6
// #[cfg(any(target_os = "macos", target_os = "ios"))] mod handle;   // Task 7
// #[cfg(any(target_os = "macos", target_os = "ios"))] mod hit;      // Task 10
// #[cfg(any(target_os = "macos", target_os = "ios"))] pub use handle::*;
// #[cfg(any(target_os = "macos", target_os = "ios"))] pub use hit::*;

#[no_mangle]
pub extern "C" fn carapace_abi_version() -> u32 {
    (CARAPACE_ABI_MAJOR << 16) | CARAPACE_ABI_MINOR
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `sfw cargo test -p carapace-ffi` (first build fetches `cbindgen` etc. through Socket Firewall)
Expected: PASS — `last_error_roundtrips_and_truncates`, `no_handle_guard_maps_panic_to_err_panic`.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add Cargo.toml Cargo.lock crates/carapace-ffi/Cargo.toml crates/carapace-ffi/src/lib.rs crates/carapace-ffi/src/guard.rs crates/carapace-ffi/src/host.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): scaffold carapace-ffi crate (status codes, last_error, panic guard, abi_version)"
```

---

## Task 4: Port the host vtable (`host.rs`)

**Files:**
- Modify: `crates/carapace-ffi/src/host.rs` (replace the stub with the ported vtable)
- Test: inline `#[cfg(test)]` (ported from the spike)

**Interfaces:**
- Produces: `#[repr(C)] pub struct CarapaceHostVTable { ctx, get_num, get_str, invoke }`; `pub struct FfiHost` impl `carapace::host::Host`.

- [ ] **Step 1: Copy the spike's host module.** Replace `crates/carapace-ffi/src/host.rs` with the full contents of `crates/embed-spike/src/host.rs` (the `CarapaceHostVTable`, `FfiHost`, `ACTIONS`, `impl Host`, and the `#[cfg(test)]` tests — all portable, no GPU). Keep it byte-for-byte except the top doc comment:

```rust
//! The host callback vtable a host app registers, and `FfiHost` bridging it to `carapace::host::Host`.
//! Ported from embed-spike; string lifetimes are borrowed-per-call (see the zero-free contract).
```

- [ ] **Step 2: Run test to verify it fails (compile) then passes.** Because the code is proven, this is a port-verification, not new TDD.

Run: `cargo test -p carapace-ffi host::tests`
Expected: PASS — `get_maps_numeric_state_through_the_vtable`, `invoke_routes_to_the_callback_and_action_is_advertised`.

If it fails to compile, the only likely cause is a missing `use`; the module is self-contained in the spike.

- [ ] **Step 3: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/host.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): port host vtable + FfiHost from embed-spike"
```

---

## Task 5: Port GPU/present internals (`render.rs`) with `init_gpu -> Result`

**Files:**
- Create: `crates/carapace-ffi/src/render.rs` (ported; one signature change)
- Modify: `crates/carapace-ffi/src/lib.rs` (uncomment the `render` mod line)
- Test: Apple-gated inline `#[cfg(test)]`

**Interfaces:**
- Produces: `pub struct GpuCtx { device, queue }`; `pub fn init_gpu() -> Result<GpuCtx, String>`; plus the ported `OffscreenTarget`, `new_offscreen`, `render_frame`, `readback_rgba`, `try_shared`, `make_content_texture`, `upload_iosurface_to_texture`, `blit`, `copy_into_iosurface`, `Tier`, `IOSurfaceRef`, `IOSurfaceGetWidth/Height`.

- [ ] **Step 1: Copy `crates/embed-spike/src/render.rs` verbatim** to `crates/carapace-ffi/src/render.rs`.

- [ ] **Step 2: Change `init_gpu` to return `Result`.** Replace the `pub fn init_gpu() -> GpuCtx { ... }` body's two `.expect(...)` calls so the function becomes:

```rust
/// Headless Metal device — no surface, we render into our own textures. Returns `Err(msg)` instead
/// of panicking so `carapace_create` can surface `ErrGpuInit` (the spike's `.expect()` holes).
pub fn init_gpu() -> Result<GpuCtx, String> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok_or_else(|| "no Metal adapter available".to_string())?;

    let mut required_features = wgpu::Features::empty();
    if adapter.features().contains(wgpu::Features::BGRA8UNORM_STORAGE) {
        required_features |= wgpu::Features::BGRA8UNORM_STORAGE;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .map_err(|e| format!("wgpu device request failed: {e}"))?;
    Ok(GpuCtx { device, queue })
}
```

(Keep the explanatory comments about `BGRA8UNORM_STORAGE` and `adapter.limits()` from the spike.)

- [ ] **Step 3: Uncomment the render module** in `lib.rs`:

```rust
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod render;
```

- [ ] **Step 4: Add an Apple-gated smoke test** at the bottom of `render.rs`:

```rust
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn init_gpu_succeeds_and_offscreen_allocates() {
        let gpu = init_gpu().expect("Metal device on a macOS test host");
        let off = new_offscreen(&gpu.device, 8, 8);
        assert_eq!((off.w, off.h), (8, 8));
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi --features '' render`  (on the macOS dev host)
Expected: PASS — `init_gpu_succeeds_and_offscreen_allocates`.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/render.rs crates/carapace-ffi/src/lib.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): port GPU/present internals; init_gpu returns Result (no expect)"
```

---

## Task 6: Opaque handle + `carapace_create` / `carapace_destroy`

**Files:**
- Create: `crates/carapace-ffi/src/handle.rs`
- Modify: `crates/carapace-ffi/src/lib.rs` (uncomment `handle` mod + `pub use handle::*`)
- Test: inline tests (host-portable null-arg + Apple-gated create/destroy/poison)

**Interfaces:**
- Consumes: `render::{GpuCtx, Renderer, Present, OffscreenTarget, Tier, ContentTex ...}`, `host::CarapaceHostVTable`, `guard::{CarapaceStatus, set_last_error, install_panic_hook, ffi_guard}`, `carapace::{engine::Engine, render::Renderer, skin, vocab}`.
- Produces: opaque `pub struct CarapaceEngine { ... poisoned: bool }`; `#[repr(C)] pub struct CarapaceCreateDesc { skin_dir, vtable, surface, content_surface, w, h }`; `carapace_create`, `carapace_destroy`.

- [ ] **Step 1: Write the handle + create/destroy.** Create `crates/carapace-ffi/src/handle.rs`. Port the spike's `CarapaceEngine` struct and `carapace_create`/`carapace_destroy` from `crates/embed-spike/src/lib.rs`, with these changes: (a) add `pub poisoned: bool`; (b) take a `*const CarapaceCreateDesc` instead of loose args and return `CarapaceStatus` with an out-param; (c) wrap the body in `ffi_guard_no_handle!` and install the panic hook; (d) map errors to status codes + `set_last_error`.

```rust
//! The opaque engine handle handed across the C ABI, plus create/destroy.

use std::ffi::{c_char, CStr};
use std::time::Duration;

use carapace::engine::{Engine, PointerEvent};
use carapace::render::Renderer;
use carapace::scene::Pt;

use crate::guard::{set_last_error, install_panic_hook, CarapaceStatus};
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::render::{
    blit, copy_into_iosurface, init_gpu, make_content_texture, new_offscreen, readback_rgba,
    render_frame, try_shared, upload_iosurface_to_texture, GpuCtx, IOSurfaceRef, OffscreenTarget,
    Tier, IOSurfaceGetHeight, IOSurfaceGetWidth,
};

// --- Present + ContentTex: copy verbatim from crates/embed-spike/src/lib.rs (the `Present` enum
//     and `ContentTex` struct, including their doc comments). They are unchanged. ---
// pub enum Present { Shared { .. }, Readback { .. } }
// pub struct ContentTex { .. }

/// Opaque handle handed across the C ABI. `poisoned` is set by `ffi_guard!` after a caught panic;
/// every subsequent call short-circuits with `ErrPoisoned`.
#[allow(deprecated)]
pub struct CarapaceEngine {
    pub gpu: GpuCtx,
    pub renderer: Renderer,
    pub engine: Engine,
    pub present: Present,
    pub surface: IOSurfaceRef,
    pub content: Option<ContentTex>,
    pub tier: Tier,
    pub w: u32,
    pub h: u32,
    pub cw: u32,
    pub ch: u32,
    pub poisoned: bool,
}

// SAFETY: single-threaded handle; the IOSurfaceRef is only touched on the calling thread.
unsafe impl Send for CarapaceEngine {}

/// Parameters for `carapace_create`. Grouped in a struct so create can grow additively.
#[repr(C)]
pub struct CarapaceCreateDesc {
    /// NUL-terminated UTF-8 skin directory path (borrowed for the call).
    pub skin_dir: *const c_char,
    /// Host callbacks (fn pointers must outlive the engine).
    pub vtable: CarapaceHostVTable,
    /// Caller-owned BGRA IOSurface of size `w`x`h` that outlives the engine.
    pub surface: IOSurfaceRef,
    /// Optional live host content for a `view{ id = "host" }` cutout; null = none.
    pub content_surface: IOSurfaceRef,
    pub w: u32,
    pub h: u32,
}

/// Create an engine. Returns a status; on `Ok`, `*out` receives the handle (else stays null).
///
/// # Safety
/// `desc` must be a valid pointer; its `skin_dir` a valid NUL-terminated UTF-8 path; `surface` a
/// live `w`x`h` BGRA IOSurface outliving the engine; `vtable` fn pointers outliving the engine.
/// `out` must be a valid pointer to a `*mut CarapaceEngine`.
#[no_mangle]
#[allow(deprecated)]
pub unsafe extern "C" fn carapace_create(
    desc: *const CarapaceCreateDesc,
    out: *mut *mut CarapaceEngine,
) -> CarapaceStatus {
    install_panic_hook();
    if out.is_null() {
        return CarapaceStatus::ErrNullArg;
    }
    unsafe { *out = std::ptr::null_mut() };
    ffi_guard_no_handle!({
        let Some(desc) = (unsafe { desc.as_ref() }) else {
            set_last_error("carapace_create: null desc");
            return CarapaceStatus::ErrNullArg;
        };
        if desc.skin_dir.is_null() {
            set_last_error("carapace_create: null skin_dir");
            return CarapaceStatus::ErrNullArg;
        }
        let dir = match unsafe { CStr::from_ptr(desc.skin_dir) }.to_str() {
            Ok(s) => std::path::PathBuf::from(s),
            Err(_) => {
                set_last_error("carapace_create: skin_dir is not valid UTF-8");
                return CarapaceStatus::ErrNullArg;
            }
        };
        let (_m, source) = match carapace::skin::load_dir(&dir) {
            Ok(v) => v,
            Err(e) => {
                set_last_error(&format!("carapace_create: skin load failed: {e:?}"));
                return CarapaceStatus::ErrBadSkin;
            }
        };
        let engine = match Engine::new(
            Box::new(FfiHost::new(desc.vtable)),
            carapace::vocab::VocabRegistry::base(),
            source,
        ) {
            Ok(e) => e,
            Err(e) => {
                set_last_error(&format!("carapace_create: engine init failed: {e:?}"));
                return CarapaceStatus::ErrBadSkin;
            }
        };
        let (cw, ch) = engine.scene().canvas;

        let gpu = match init_gpu() {
            Ok(g) => g,
            Err(msg) => {
                set_last_error(&format!("carapace_create: {msg}"));
                return CarapaceStatus::ErrGpuInit;
            }
        };
        let renderer = Renderer::new(&gpu.device);

        // Tier 2 (zero-copy) with Tier 1 fallback — copy this block verbatim from the spike's
        // `carapace_create` (the `try_shared`/`Present::Shared`/`Present::Readback` match), it is
        // unchanged. Assign `(present, tier)`.
        let (present, tier) = build_present(&gpu, desc.surface, desc.w, desc.h);

        // Optional host content view — copy verbatim from the spike (null/zero-dim → None).
        let content = build_content(&gpu, desc.content_surface);

        let handle = Box::into_raw(Box::new(CarapaceEngine {
            gpu, renderer, engine, present, surface: desc.surface, content, tier,
            w: desc.w, h: desc.h, cw, ch, poisoned: false,
        }));
        unsafe { *out = handle };
        CarapaceStatus::Ok
    })
}

/// Destroy an engine created by `carapace_create`. Null-safe; valid on a poisoned handle.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be used afterward.
#[no_mangle]
pub unsafe extern "C" fn carapace_destroy(ptr: *mut CarapaceEngine) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}
```

Add the two private helpers `build_present` and `build_content` in `handle.rs`, lifting the exact bodies from the spike's `carapace_create` (the Tier-2/Tier-1 match and the content-import block respectively), each returning the value assigned above. This keeps `carapace_create` readable while reusing proven code verbatim.

- [ ] **Step 2: Uncomment the handle module** in `lib.rs`:

```rust
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod handle;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use handle::*;
```

- [ ] **Step 3: Write tests.** Host-portable null-arg test needs no GPU but `carapace_create` is Apple-gated; put the null-`out` test under macOS too. Add to `handle.rs`:

```rust
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::host::CarapaceHostVTable;

    fn empty_vtable() -> CarapaceHostVTable {
        CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: None }
    }

    #[test]
    fn create_rejects_null_out_and_null_skin_dir() {
        // null out
        let desc = CarapaceCreateDesc {
            skin_dir: std::ptr::null(), vtable: empty_vtable(),
            surface: std::ptr::null_mut(), content_surface: std::ptr::null_mut(), w: 4, h: 4,
        };
        let status = unsafe { carapace_create(&desc, std::ptr::null_mut()) };
        assert_eq!(status, CarapaceStatus::ErrNullArg);
        // null skin_dir, valid out
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        let status = unsafe { carapace_create(&desc, &mut handle) };
        assert_eq!(status, CarapaceStatus::ErrNullArg);
        assert!(handle.is_null());
    }

    #[test]
    fn create_reports_bad_skin_for_missing_dir() {
        let path = std::ffi::CString::new("/no/such/skin/dir").unwrap();
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(), vtable: empty_vtable(),
            surface: std::ptr::null_mut(), content_surface: std::ptr::null_mut(), w: 4, h: 4,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        let status = unsafe { carapace_create(&desc, &mut handle) };
        assert_eq!(status, CarapaceStatus::ErrBadSkin);
        assert!(handle.is_null());
    }
}
```

(A full create→destroy against a real skin + IOSurface is exercised in Task 7's tick test, which needs a live surface anyway.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi handle`
Expected: PASS — `create_rejects_null_out_and_null_skin_dir`, `create_reports_bad_skin_for_missing_dir`.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/handle.rs crates/carapace-ffi/src/lib.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): opaque handle + carapace_create/destroy over a descriptor, error-mapped"
```

---

## Task 7: `carapace_tick` + `carapace_active_tier`

**Files:**
- Modify: `crates/carapace-ffi/src/handle.rs` (add `carapace_tick`, `carapace_active_tier`)
- Test: Apple-gated create→tick→readback pixel test

**Interfaces:**
- Consumes: `CarapaceEngine`, `render::{render_frame, blit, readback_rgba, copy_into_iosurface, upload_iosurface_to_texture}`, `ffi_guard!`.
- Produces: `#[repr(i32)] pub enum CarapaceTier { Readback=1, Shared=2 }`; `carapace_tick(*mut CarapaceEngine, f64) -> CarapaceStatus`; `carapace_active_tier(*mut CarapaceEngine, *mut CarapaceTier) -> CarapaceStatus`.

- [ ] **Step 1: Add tick + active_tier + the poison short-circuit.** In `handle.rs`:

```rust
/// The present path the engine resolved to. Mirrors `render::Tier`.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapaceTier {
    Readback = 1,
    Shared = 2,
}

/// Tick + render one frame into the engine's surface. `dt_seconds` is host wall-clock time.
///
/// # Safety
/// `ptr` must come from `carapace_create` and not be destroyed.
#[no_mangle]
pub unsafe extern "C" fn carapace_tick(ptr: *mut CarapaceEngine, dt_seconds: f64) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    ffi_guard!(ptr, {
        let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
        // Copy the spike's `carapace_tick` body verbatim here (the field destructure + the
        // Present::Shared / Present::Readback match), then evaluate to CarapaceStatus::Ok.
        tick_inner(e, dt);
        CarapaceStatus::Ok
    })
}

/// Report the active present tier. `# Safety`: `ptr` from `carapace_create`; `out` non-null.
#[no_mangle]
pub unsafe extern "C" fn carapace_active_tier(
    ptr: *mut CarapaceEngine,
    out: *mut CarapaceTier,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if out.is_null() {
        return CarapaceStatus::ErrNullArg;
    }
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    let tier = match e.tier {
        Tier::Readback => CarapaceTier::Readback,
        Tier::Shared => CarapaceTier::Shared,
    };
    unsafe { *out = tier };
    CarapaceStatus::Ok
}
```

Add the private `tick_inner(e: &mut CarapaceEngine, dt: Duration)` helper containing the spike's exact `carapace_tick` rendering body (content upload → `render_frame` → blit/readback present). This isolates the proven render sequence and keeps the export a thin guard wrapper.

- [ ] **Step 2: Write the pixel test.** This needs a real skin and a live IOSurface. Use a fixture skin from the repo (find one under `crates/` used by existing GPU tests — e.g. the demo/headspace skin dir; grep for `load_dir` in tests) and create a BGRA IOSurface via the macOS `io-surface` crate. Add to `handle.rs`:

```rust
#[cfg(all(test, target_os = "macos"))]
mod tick_tests {
    use super::*;
    use crate::host::CarapaceHostVTable;

    // Path to a known-good skin directory checked into the repo. Replace with the actual fixture
    // path discovered via `grep -rn "load_dir" crates/*/tests crates/*/src` (e.g. a Headspace or
    // clock skin the engine tests already load).
    const SKIN_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../carapace-demo/skins/<fixture>");

    fn make_bgra_iosurface(w: usize, h: usize) -> io_surface::IOSurfaceRef {
        use core_foundation::base::TCFType;
        use core_foundation::number::CFNumber;
        use core_foundation::string::CFString;
        use core_foundation::dictionary::CFDictionary;
        let props = CFDictionary::from_CFType_pairs(&[
            (CFString::new("IOSurfaceWidth"), CFNumber::from(w as i64).as_CFType()),
            (CFString::new("IOSurfaceHeight"), CFNumber::from(h as i64).as_CFType()),
            (CFString::new("IOSurfaceBytesPerElement"), CFNumber::from(4i64).as_CFType()),
            (CFString::new("IOSurfacePixelFormat"), CFNumber::from(0x42475241i64 /* 'BGRA' */).as_CFType()),
        ]);
        io_surface::new(&props).as_concrete_TypeRef()
    }

    #[test]
    fn create_tick_destroy_renders_nonblank() {
        let (w, h) = (64u32, 64u32);
        let surface = make_bgra_iosurface(w as usize, h as usize) as IOSurfaceRef;
        let path = std::ffi::CString::new(SKIN_DIR).unwrap();
        let vtable = CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: None };
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(), vtable, surface, content_surface: std::ptr::null_mut(), w, h,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        assert_eq!(unsafe { carapace_create(&desc, &mut handle) }, CarapaceStatus::Ok);
        assert!(!handle.is_null());

        assert_eq!(unsafe { carapace_tick(handle, 0.016) }, CarapaceStatus::Ok);

        let mut tier = CarapaceTier::Readback;
        assert_eq!(unsafe { carapace_active_tier(handle, &mut tier) }, CarapaceStatus::Ok);

        // Surface should now contain non-zero pixels (the skin drew something).
        let nonzero = unsafe { iosurface_has_nonzero_pixels(surface, w, h) };
        assert!(nonzero, "expected the skin to render visible pixels");

        unsafe { carapace_destroy(handle) };
    }
}
```

Add a small `#[cfg(all(test, target_os = "macos"))]` helper `iosurface_has_nonzero_pixels` that locks the surface (reuse `render::IOSurfaceGetBaseAddress`/stride via a thin test-only accessor, or the `io-surface` crate's lock API) and returns whether any byte is non-zero. If exposing the framework accessors to tests is awkward, instead assert on `carapace_active_tier` returning a valid tier and that `carapace_tick` returns `Ok` twice in a row (create+tick prove the pipeline; pixel inspection is a bonus). Prefer the pixel check if the accessor is readily reusable.

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi tick`
Expected: PASS — `create_tick_destroy_renders_nonblank`.

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/handle.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): carapace_tick + active_tier (poison-checked, guarded present path)"
```

---

## Task 8: `carapace_pointer` (full pointer input)

**Files:**
- Modify: `crates/carapace-ffi/src/handle.rs` (add `carapace_pointer`, `CarapacePointerKind`)
- Test: host-portable mapping test + Apple-gated end-to-end press

**Interfaces:**
- Produces: `#[repr(i32)] pub enum CarapacePointerKind { Press=0, Release=1, Move=2, Enter=3, Leave=4 }`; `carapace_pointer(*mut CarapaceEngine, f64, f64, CarapacePointerKind) -> CarapaceStatus`.

- [ ] **Step 1: Add the pointer export.** In `handle.rs`:

```rust
/// Pointer event kinds. v1 forwards all; the engine currently acts on `Press`, the rest are
/// plumbed for hover/drag semantics and forward-compat (additive).
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarapacePointerKind {
    Press = 0,
    Release = 1,
    Move = 2,
    Enter = 3,
    Leave = 4,
}

/// Forward a pointer event in DESIGN-CANVAS coordinates.
///
/// # Safety
/// `ptr` must come from `carapace_create`.
#[no_mangle]
pub unsafe extern "C" fn carapace_pointer(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    kind: CarapacePointerKind,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    ffi_guard!(ptr, {
        let event = match kind {
            CarapacePointerKind::Press => Some(PointerEvent::Press),
            // The engine models Press today; map the rest to the nearest existing event it accepts.
            // Until the engine grows release/move/enter/leave, forward only Press and treat the
            // others as no-ops (still validated + guarded). This stays additive: when the engine
            // gains those events, extend this match — no ABI change.
            _ => None,
        };
        if let Some(ev) = event {
            e.engine.handle_pointer_resolved(e.cw as f32, e.ch as f32, Pt { x: x as f32, y: y as f32 }, ev);
        }
        CarapaceStatus::Ok
    })
}
```

Note: check `carapace::engine::PointerEvent`'s actual variants before finalizing the match. If it already has `Release`/`Move` variants, map them through instead of `None`. The ABI (five kinds) is fixed regardless; only the internal mapping tracks engine capability.

- [ ] **Step 2: Write tests.** Add to the Apple-gated `tick_tests` (reuses the created handle pattern) — verify a press over a known hotspot enqueues its action, and that a null/poison path returns the right status:

```rust
#[test]
fn pointer_press_returns_ok_and_null_is_rejected() {
    // null handle
    assert_eq!(
        unsafe { carapace_pointer(std::ptr::null_mut(), 1.0, 1.0, CarapacePointerKind::Press) },
        CarapaceStatus::ErrNullArg
    );
    // (Optional end-to-end: create as in create_tick_destroy_renders_nonblank, then
    // carapace_pointer(handle, x, y, Press) == Ok, then carapace_tick to drain, and assert the
    // host vtable's invoke callback fired for the expected action.)
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi pointer`
Expected: PASS — `pointer_press_returns_ok_and_null_is_rejected`.

- [ ] **Step 4: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/handle.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): carapace_pointer with full pointer-kind ABI (press wired, rest plumbed)"
```

---

## Task 9: `carapace_hit_test` (engine→host channel)

**Files:**
- Create: `crates/carapace-ffi/src/hit.rs`
- Modify: `crates/carapace-ffi/src/lib.rs` (uncomment `hit` mod + `pub use hit::*`)
- Test: Apple-gated end-to-end

**Interfaces:**
- Consumes: `CarapaceEngine`, `carapace::scene::HitKind`, `ffi_guard!`.
- Produces: `#[repr(i32)] pub enum CarapaceHitKind { Passthrough=0, Control=1, Drag=2 }`; `carapace_hit_test(*mut CarapaceEngine, f64, f64, *mut CarapaceHitKind) -> CarapaceStatus`.

- [ ] **Step 1: Implement the hit-test export.** Create `crates/carapace-ffi/src/hit.rs`:

```rust
//! The engine→host hit-test channel: classify a point without firing Lua, so a host can decide to
//! move the window, let the skin consume the event, or pass it through.

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

/// Classify the point `(x, y)` (DESIGN-CANVAS coords) without side effects. Writes `*out`.
///
/// # Safety
/// `ptr` must come from `carapace_create`; `out` must be non-null.
#[no_mangle]
pub unsafe extern "C" fn carapace_hit_test(
    ptr: *mut CarapaceEngine,
    x: f64,
    y: f64,
    out: *mut CarapaceHitKind,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else {
        return CarapaceStatus::ErrNullArg;
    };
    if out.is_null() {
        return CarapaceStatus::ErrNullArg;
    }
    if e.poisoned {
        return CarapaceStatus::ErrPoisoned;
    }
    ffi_guard!(ptr, {
        // Lay out at the design canvas (as the pointer path does), classify, map.
        let scene = e.engine.layout(e.cw as f32, e.ch as f32);
        let kind = match scene.hit_kind(Pt { x: x as f32, y: y as f32 }) {
            HitKind::Passthrough => CarapaceHitKind::Passthrough,
            HitKind::Control => CarapaceHitKind::Control,
            HitKind::Drag => CarapaceHitKind::Drag,
        };
        unsafe { *out = kind };
        CarapaceStatus::Ok
    })
}
```

- [ ] **Step 2: Uncomment the hit module** in `lib.rs`:

```rust
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod hit;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use hit::*;
```

- [ ] **Step 3: Write the end-to-end test.** In an Apple-gated test (reuse the create pattern), assert that a point over the skin returns `Control`/`Drag` and a far-outside point returns `Passthrough`:

```rust
#[cfg(all(test, target_os = "macos"))]
mod hit_tests {
    use super::*;
    // create a handle exactly as in handle.rs's tick test (same SKIN_DIR + IOSurface helper),
    // then:
    // let mut kind = CarapaceHitKind::Passthrough;
    // assert_eq!(unsafe { carapace_hit_test(handle, -100.0, -100.0, &mut kind) }, CarapaceStatus::Ok);
    // assert_eq!(kind, CarapaceHitKind::Passthrough); // far outside the canvas
    // null-out rejection:
    #[test]
    fn hit_test_rejects_null_out_and_handle() {
        assert_eq!(
            unsafe { carapace_hit_test(std::ptr::null_mut(), 0.0, 0.0, std::ptr::null_mut()) },
            CarapaceStatus::ErrNullArg
        );
    }
}
```

Flesh out the create-based assertions using the same helper as Task 7 (factor the surface+create setup into a shared `#[cfg(test)]` helper if convenient).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi hit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/hit.rs crates/carapace-ffi/src/lib.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): carapace_hit_test engine→host channel (control/drag/passthrough)"
```

---

## Task 10: cbindgen header + freshness test

**Files:**
- Create: `crates/carapace-ffi/cbindgen.toml`
- Create: `crates/carapace-ffi/include/carapace.h` (generated, committed)
- Create: `crates/carapace-ffi/tests/header.rs`

**Interfaces:**
- Consumes: the whole public `extern "C"` surface.
- Produces: a committed C header + a test that fails if it drifts.

- [ ] **Step 1: Write `crates/carapace-ffi/cbindgen.toml`**

```toml
language = "C"
include_guard = "CARAPACE_FFI_H"
pragma_once = true
cpp_compat = true
autogen_warning = "/* Generated by cbindgen. Do not edit by hand. Regenerate: cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact */"
tab_width = 2
style = "type"

[parse]
parse_deps = false

[defines]
"target_os = macos" = "CARAPACE_APPLE"
"target_os = ios" = "CARAPACE_APPLE"

[enum]
prefix_with_name = false
```

Note: because the ABI symbols are `#[cfg]`-gated to Apple, the freshness test (Step 3) runs on macOS so cbindgen sees them. The `[defines]` mapping lets cbindgen resolve the cfg.

- [ ] **Step 2: Write `crates/carapace-ffi/tests/header.rs`**

```rust
//! Guards that the committed C header matches the current ABI. On macOS (where the ABI symbols are
//! active) it regenerates in memory and diffs. Run the ignored `regenerate_header` to update.

#[cfg(target_os = "macos")]
fn generate() -> String {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let config = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml")).unwrap();
    let mut out = Vec::new();
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(config)
        .generate()
        .expect("cbindgen generate")
        .write(&mut out);
    String::from_utf8(out).unwrap()
}

#[cfg(target_os = "macos")]
#[test]
fn header_is_fresh() {
    let generated = generate();
    let committed = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/include/carapace.h"))
        .expect("committed include/carapace.h must exist");
    assert_eq!(
        generated, committed,
        "carapace.h is stale — regenerate with: cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact"
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn regenerate_header() {
    let generated = generate();
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/include/carapace.h"), generated).unwrap();
}
```

- [ ] **Step 3: Generate the committed header**

Run: `cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact`
Then inspect `crates/carapace-ffi/include/carapace.h` — it must contain `CarapaceStatus`, `CarapaceCreateDesc`, `CarapaceHostVTable`, `CarapacePointerKind`, `CarapaceHitKind`, `CarapaceTier`, and all `carapace_*` prototypes.

- [ ] **Step 4: Verify the freshness test passes**

Run: `cargo test -p carapace-ffi --test header header_is_fresh`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/cbindgen.toml crates/carapace-ffi/include/carapace.h crates/carapace-ffi/tests/header.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "feat(ffi): cbindgen C header + freshness test"
```

---

## Task 11: Poison end-to-end test + full lint gate + README

**Files:**
- Modify: `crates/carapace-ffi/src/handle.rs` (a test-only forced-panic path to prove poisoning)
- Modify: `README.md` (document the new crate per repo policy)
- Test: Apple-gated poison test

**Interfaces:**
- Consumes: the full ABI.

- [ ] **Step 1: Prove poisoning end-to-end.** Add an Apple-gated test that drives a real panic through a guarded call and asserts the handle poisons. The cleanest trigger without adding production hooks: create a handle, then corrupt an invariant only reachable in tests — OR add a hidden `#[cfg(test)]`-only export `carapace_force_panic(ptr)` that panics inside `ffi_guard!`. Use the latter:

```rust
#[cfg(all(test, target_os = "macos"))]
#[no_mangle]
pub unsafe extern "C" fn carapace_force_panic(ptr: *mut CarapaceEngine) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_mut() }) else { return CarapaceStatus::ErrNullArg; };
    if e.poisoned { return CarapaceStatus::ErrPoisoned; }
    ffi_guard!(ptr, {
        panic!("forced panic for poison test");
        #[allow(unreachable_code)]
        CarapaceStatus::Ok
    })
}

#[cfg(all(test, target_os = "macos"))]
mod poison_tests {
    use super::*;
    // Build a handle via the shared create helper (same SKIN_DIR + IOSurface as Task 7), then:
    #[test]
    fn caught_panic_poisons_the_handle_and_last_error_is_set() {
        // let handle = create_test_handle();
        // assert_eq!(unsafe { carapace_force_panic(handle) }, CarapaceStatus::ErrPanic);
        // subsequent calls short-circuit:
        // assert_eq!(unsafe { carapace_tick(handle, 0.016) }, CarapaceStatus::ErrPoisoned);
        // last_error populated by the hook:
        // let mut buf = [0i8; 128];
        // let n = unsafe { crate::guard::carapace_last_error(buf.as_mut_ptr(), buf.len()) };
        // assert!(n > 0);
        // unsafe { carapace_destroy(handle) }; // still frees a poisoned handle
    }
}
```

Fill in the commented lines using the shared create helper from Task 7 (factor `create_test_handle()` into a `#[cfg(all(test, target_os="macos"))]` helper used by the tick/hit/poison tests).

- [ ] **Step 2: Run the poison test**

Run: `cargo test -p carapace-ffi poison`
Expected: PASS — panic → `ErrPanic`, next call → `ErrPoisoned`, `last_error` non-empty, destroy succeeds.

- [ ] **Step 3: Update the README.** Add a `carapace-ffi` entry to `README.md` alongside the other crates: one paragraph — "The production C ABI (Apple/macOS+iOS) for embedding the engine as host UI: opaque handle, panic-safe boundary, cbindgen header at `crates/carapace-ffi/include/carapace.h`, engine→host hit-test. Windows/Linux/Android are future work." Match the README's existing crate-list format (read the current file first and follow its structure).

- [ ] **Step 4: Full workspace lint + test gate**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo clippy -p carapace --features gpu-tests --all-targets -- -D warnings
cargo test -p carapace
cargo test -p carapace-ffi
```
Expected: all green. Fix any clippy findings inline (common: `needless_return` in the early-return guards — restructure to expression form if flagged; missing `# Safety` docs — every `unsafe extern "C"` already has one, keep it).

- [ ] **Step 5: Commit**

```bash
cargo fmt
git add crates/carapace-ffi/src/handle.rs README.md
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit -m "test(ffi): prove panic→poison→ErrPoisoned end-to-end; document crate; lint gate"
```

---

## Task 12: Open the PR

**Files:** none (git/PR only)

- [ ] **Step 1: Push the branch and open a PR**

```bash
git push -u origin carapace-ffi
gh pr create --title "carapace-ffi v1: safe Apple C ABI (panic-guarded handle, engine→host hit-test)" \
  --body "$(cat <<'EOF'
Promotes the embed-spike into a real, versioned C ABI crate `carapace-ffi` (Apple/macOS+iOS).

## What
- Panic-safe boundary: catch-unwind + poison + thread-local `last_error` on every export (never aborts the host). Closes the spike's `init_gpu().expect()` holes.
- Opaque `CarapaceEngine` handle; `carapace_create` over a `CarapaceCreateDesc`; `tick`/`pointer`/`active_tier`/`hit_test`/`destroy`.
- Versioned: `carapace_abi_version()`, additive-only exports, cbindgen-generated committed `include/carapace.h` with a freshness test.
- Engine→host hit-test channel (control/drag/passthrough) backed by two additive engine changes: hotspot `role` + `Scene::hit_kind`/`covers`.
- `embed-spike` left frozen as reference; its samples untouched.

## Scope
Apple only, single-threaded. Windows/Linux/Android backends, a render thread, per-pixel GPU-alpha masking, and sample ports are named future work in the spec.

Spec: `docs/superpowers/specs/2026-07-01-carapace-ffi-design.md`
Plan: `docs/superpowers/plans/2026-07-01-carapace-ffi.md`
EOF
)"
```

Expected: PR created against `main`. (No Claude attribution in the body.)

---

## Self-review notes

- **Spec coverage:** crate scaffold + version (T3) · panic guard/poison/last_error (T3, T7–T9, T11) · opaque handle + descriptor + zero-free strings (T6) · `init_gpu -> Result` error taxonomy (T5, T6) · 2-tier present + host-view + host vtable parity (T4, T6, T7) · full pointer ABI (T8) · engine→host hit-test + role + covers (T1, T2, T9) · cbindgen header + freshness (T10) · Apple-gating / non-goals (Global Constraints, T3) · README per-phase policy (T11) · tests at every layer (each task). All spec sections map to a task.
- **Non-placeholders:** ported-verbatim blocks (Present/ContentTex, the Tier match, the tick body) point to exact spike source and are wrapped by named helpers (`build_present`, `build_content`, `tick_inner`) rather than restated — this is deliberate reuse of proven code, with the exact source location given, not a "similar to" hand-wave. Two engine-API checks are called out explicitly (the `PointerEvent` variant set in T8; the test skin fixture path in T7) because they must be read from the live tree at implementation time.
- **Type consistency:** `CarapaceStatus`, `CarapaceCreateDesc`, `CarapaceEngine`, `CarapaceTier`, `CarapacePointerKind`, `CarapaceHitKind`, `HotspotRole`, `HitKind`, `Scene::hit_kind`/`covers`, `init_gpu -> Result<GpuCtx, String>` are named identically across every task that produces or consumes them.

# carapace-ffi v2 — Render Thread + Command Queue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the carapace engine off the caller's thread onto a dedicated render thread that carapace owns, driven by a command queue, so host API calls are non-blocking and animation free-runs without host pumping.

**Architecture:** `carapace_create` spawns one `std::thread` that builds and owns the `!Send` `Engine` + GPU context and runs a pacing loop (free-run at a target fps, wall-clock `dt`). The C-ABI handle becomes a thread-safe front-end: an `mpsc` command sender (input), an `RwLock<Arc<SceneSnapshot>>` (queries), a shared `AtomicBool` poison flag, and the `JoinHandle`. Finished frames land in one of a host-provided IOSurface pool and are announced via a new `frame_ready` callback.

**Tech Stack:** Rust (edition 2024), `wgpu`/`wgpu-hal` `=29.0.3` (Metal, pinned), IOSurface via `objc2-io-surface`/`objc2-metal`, `std::sync::mpsc` + `std::sync::RwLock` + `std::sync::atomic` (no new runtime deps), `cbindgen` 0.29 (dev-dep) for the header.

## Global Constraints

- **Platform:** Apple (macOS/iOS) only. The whole render/handle/hit path is `#[cfg(any(target_os = "macos", target_os = "ios"))]`-gated; on other targets the crate stays a near-empty shell so Linux CI + clippy stay green. Copy the existing cfg attributes verbatim.
- **Edition:** 2024 (repo standard). `#[unsafe(no_mangle)]` and `unsafe extern "C"` syntax as in the existing v1 files.
- **wgpu pinned:** `wgpu = "29.0.3"`, `wgpu-hal = "=29.0.3"`. Do NOT bump; zero-copy `try_shared` is verified only against this version.
- **No new runtime dependencies.** Use `std` (`mpsc`, `RwLock`, `atomic`, `thread`, `Instant`). `cbindgen` stays a dev-dep. If any 3rd-party crate is ever added, its first fetch MUST run as `sfw cargo add ...` (Socket Firewall) — but this plan adds none.
- **Panic policy:** catch + poison + error code. NEVER `abort()` — carapace runs in the host's process. Every C export wraps its body in `ffi_guard!`/`ffi_guard_no_handle!`; the render-loop body has its own `catch_unwind`.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>` (`git -c user.name=... -c user.email=... commit`). Work on branch `carapace-ffi-render-thread` (already created; the design spec is committed there).
- **Lint gate:** `cargo clippy -D warnings` (and the `gpu-tests` variant) must pass — CI gates on it. Run `cargo fmt` before every commit.
- **ABI:** breaking v2 bump. `carapace_abi_version()` returns `2 << 16`. No external consumer links this ABI yet (samples link the frozen `embed-spike`), so breaking changes are allowed once, in Task 9.
- **Spec:** `docs/superpowers/specs/2026-07-02-carapace-ffi-render-thread-design.md`.

## File Structure

- `crates/carapace-ffi/src/queue.rs` — **new.** `Command` enum, the mpsc channel type alias, and the coalescing drain helper. Host-portable (no GPU, no cfg gate). Unit-tested.
- `crates/carapace-ffi/src/snapshot.rs` — **new.** `SceneSnapshot` (owned `Scene` + `Tier`), the shared `RwLock<Arc<SceneSnapshot>>` cell, publish + query readers (`hit_kind_of`, `tier_of`). Host-portable. Unit-tested with a synthetic scene.
- `crates/carapace-ffi/src/host.rs` — **modify.** Add `frame_ready` fn-ptr to `CarapaceHostVTable`. Keep the existing `Send`/`Sync` impls (needed so the whole `FfiHost` can move to the render thread).
- `crates/carapace-ffi/src/render_thread.rs` — **new, Apple-gated.** The render thread's owned `RenderThread` state + `spawn()` (construct-on-thread + init handshake) + the pacing loop + per-surface present pool + poison guard.
- `crates/carapace-ffi/src/handle.rs` — **modify (major).** `CarapaceEngine` becomes the thread-safe front-end. `CarapaceCreateDesc` gains the surface pool + drops the single `surface`. `carapace_create`/`carapace_destroy`/`carapace_pointer` rewritten; add `carapace_invalidate`/`carapace_set_frame_rate`/`carapace_release_surface`; **remove `carapace_tick`/`carapace_active_tier`** (tier moves to snapshot reader; active_tier re-added in Task 7 reading the snapshot).
- `crates/carapace-ffi/src/hit.rs` — **modify.** `carapace_hit_test` reads the snapshot instead of touching the engine.
- `crates/carapace-ffi/src/render.rs` — **modify (small).** `Present` stays; add a helper to build a `Present` per surface (extract from `build_present`, which lives in `handle.rs` today — move it to `render.rs`).
- `crates/carapace-ffi/src/lib.rs` — **modify.** Register `queue`, `snapshot`, `render_thread` modules; re-export the new exports.
- `crates/carapace-ffi/include/carapace.h`, `cbindgen.toml` — **modify.** Regenerated for v2; freshness test updated.
- `crates/carapace-ffi/README.md` (or the repo README section for carapace-ffi) — **modify** in Task 10.

---

### Task 1: Command queue (`queue.rs`)

**Files:**
- Create: `crates/carapace-ffi/src/queue.rs`
- Modify: `crates/carapace-ffi/src/lib.rs` (add `mod queue;`)
- Test: inline `#[cfg(test)] mod tests` in `queue.rs`

**Interfaces:**
- Produces:
  - `pub enum Command { Pointer { x: f64, y: f64, kind: PointerKind }, Invalidate, SetFrameRate(u32), ReleaseSurface(u32), Shutdown }`
  - `pub enum PointerKind { Press, Release, Move, Enter, Leave }` (plain, `Copy`; the C `CarapacePointerKind` maps onto it)
  - `pub type CommandTx = std::sync::mpsc::Sender<Command>;`
  - `pub type CommandRx = std::sync::mpsc::Receiver<Command>;`
  - `pub fn drain_coalescing(rx: &CommandRx, first: Command, out: &mut Vec<Command>)` — pushes `first`, then non-blockingly drains all currently-queued commands into `out`, collapsing a run of consecutive `Pointer{kind: Move}` into only the latest Move (press/release/enter/leave and all non-pointer commands are preserved in order).

- [ ] **Step 1: Write the failing test**

In `crates/carapace-ffi/src/queue.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn mv(x: f64) -> Command {
        Command::Pointer { x, y: 0.0, kind: PointerKind::Move }
    }

    #[test]
    fn coalesces_consecutive_moves_keeping_the_latest() {
        let (tx, rx) = channel::<Command>();
        // queue: Move(1), Move(2), Press, Move(3) — drain starting from an initial Move(0)
        tx.send(mv(1.0)).unwrap();
        tx.send(mv(2.0)).unwrap();
        tx.send(Command::Pointer { x: 9.0, y: 9.0, kind: PointerKind::Press }).unwrap();
        tx.send(mv(3.0)).unwrap();
        let mut out = Vec::new();
        drain_coalescing(&rx, mv(0.0), &mut out);
        // Expect: Move(2) [latest of the leading run 0,1,2], Press, Move(3)
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Command::Pointer { x, kind: PointerKind::Move, .. } if x == 2.0));
        assert!(matches!(out[1], Command::Pointer { kind: PointerKind::Press, .. }));
        assert!(matches!(out[2], Command::Pointer { x, kind: PointerKind::Move, .. } if x == 3.0));
    }

    #[test]
    fn preserves_non_move_order_and_shutdown() {
        let (tx, rx) = channel::<Command>();
        tx.send(Command::SetFrameRate(30)).unwrap();
        tx.send(Command::ReleaseSurface(1)).unwrap();
        tx.send(Command::Shutdown).unwrap();
        let mut out = Vec::new();
        drain_coalescing(&rx, Command::Invalidate, &mut out);
        assert!(matches!(out[0], Command::Invalidate));
        assert!(matches!(out[1], Command::SetFrameRate(30)));
        assert!(matches!(out[2], Command::ReleaseSurface(1)));
        assert!(matches!(out[3], Command::Shutdown));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --lib queue`
Expected: FAIL — `queue` module / `Command` not found (won't compile).

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/carapace-ffi/src/queue.rs`:

```rust
//! The render thread's command channel. Host API calls enqueue `Command`s; the render loop drains
//! them each frame. Host-portable (no GPU): kept ungated so its logic is unit-tested on all CI.

/// Pointer event kind, mirrored 1:1 by the C `CarapacePointerKind` (see `handle.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Press,
    Release,
    Move,
    Enter,
    Leave,
}

/// A message from a host API call to the render thread. Additive: append new variants.
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Pointer { x: f64, y: f64, kind: PointerKind },
    /// Render exactly one frame now (wakes a paused engine).
    Invalidate,
    /// Set the free-run target frame rate; 0 = paused (render only on Invalidate/Pointer).
    SetFrameRate(u32),
    /// Host is done displaying `surfaces[index]`; it may be rendered into again.
    ReleaseSurface(u32),
    /// Stop the loop and let the thread exit (sent by `carapace_destroy`).
    Shutdown,
}

pub type CommandTx = std::sync::mpsc::Sender<Command>;
pub type CommandRx = std::sync::mpsc::Receiver<Command>;

/// Push `first`, then drain everything currently queued into `out`, collapsing a run of consecutive
/// `Pointer{Move}` into only the most recent one (stale positions are worthless; the latest wins).
/// All other commands — and Moves separated by a non-Move — keep their order.
pub fn drain_coalescing(rx: &CommandRx, first: Command, out: &mut Vec<Command>) {
    push_coalesced(out, first);
    while let Ok(cmd) = rx.try_recv() {
        push_coalesced(out, cmd);
    }
}

fn is_move(c: &Command) -> bool {
    matches!(c, Command::Pointer { kind: PointerKind::Move, .. })
}

fn push_coalesced(out: &mut Vec<Command>, cmd: Command) {
    if is_move(&cmd) && out.last().is_some_and(is_move) {
        *out.last_mut().unwrap() = cmd; // replace the previous trailing Move
    } else {
        out.push(cmd);
    }
}
```

Add to `crates/carapace-ffi/src/lib.rs` after `mod guard;`:

```rust
mod queue;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi --lib queue`
Expected: PASS (2 tests).

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt -p carapace-ffi
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/queue.rs crates/carapace-ffi/src/lib.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): command queue with move-coalescing drain (v2 render thread)"
```

---

### Task 2: Scene snapshot (`snapshot.rs`)

**Files:**
- Create: `crates/carapace-ffi/src/snapshot.rs`
- Modify: `crates/carapace-ffi/src/lib.rs` (add `mod snapshot;`)
- Test: inline `#[cfg(test)] mod tests` in `snapshot.rs`

**Interfaces:**
- Consumes: `carapace::scene::{Scene, HitKind, Pt}` (`Scene::hit_kind(Pt) -> HitKind`, `Scene::canvas: (u32,u32)`), `crate::hit::CarapaceHitKind` (Task exists in v1), and a local `Tier` mirror.
- Produces:
  - `pub struct SceneSnapshot { pub scene: Option<Scene>, pub tier: SnapshotTier }`
  - `pub enum SnapshotTier { Readback, Shared }` (plain mirror so `snapshot.rs` stays GPU-free; the render `Tier` maps onto it)
  - `pub type SnapshotCell = std::sync::Arc<std::sync::RwLock<std::sync::Arc<SceneSnapshot>>>;`
  - `pub fn new_cell(initial_tier: SnapshotTier) -> SnapshotCell`
  - `pub fn publish(cell: &SnapshotCell, scene: Scene, tier: SnapshotTier)`
  - `pub fn hit_kind_of(cell: &SnapshotCell, p: Pt) -> HitKind` — reads the current snapshot; returns `HitKind::Passthrough` when no frame has been published yet.
  - `pub fn tier_of(cell: &SnapshotCell) -> SnapshotTier`

- [ ] **Step 1: Write the failing test**

In `crates/carapace-ffi/src/snapshot.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use carapace::scene::{HitKind, Pt};

    #[test]
    fn before_first_publish_hit_is_passthrough_and_tier_is_initial() {
        let cell = new_cell(SnapshotTier::Shared);
        assert!(matches!(tier_of(&cell), SnapshotTier::Shared));
        let k = hit_kind_of(&cell, Pt { x: 5.0, y: 5.0 });
        assert!(matches!(k, HitKind::Passthrough));
    }

    #[test]
    fn publish_then_query_reads_the_published_scene() {
        let cell = new_cell(SnapshotTier::Readback);
        // An empty scene covers nothing → hit_kind is Passthrough everywhere, but tier updates.
        let scene = carapace::scene::Scene { canvas: (100, 50), ..empty_scene() };
        publish(&cell, scene, SnapshotTier::Shared);
        assert!(matches!(tier_of(&cell), SnapshotTier::Shared));
        // reading does not panic and returns a defined classification
        let _ = hit_kind_of(&cell, Pt { x: 5.0, y: 5.0 });
    }

    // Build a minimal empty Scene using its public constructor/Default if available.
    // NOTE (implementer): replace with the real minimal-scene builder from `carapace::scene`.
    // If `Scene` has no `Default`, construct via `carapace::skin`/`Engine::layout` on a trivial
    // fixture instead; the assertion only needs a valid empty Scene.
    fn empty_scene() -> carapace::scene::Scene {
        carapace::scene::Scene::default()
    }

    #[test]
    fn snapshot_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SceneSnapshot>();
        assert_send_sync::<SnapshotCell>();
    }
}
```

- [ ] **Step 2: Verify `Scene` supports the test's construction, then run the failing test**

First confirm how to build an empty `Scene`: `grep -n "impl Default for Scene\|pub struct Scene" crates/carapace/src/scene.rs`. If `Scene: Default` does not exist, either (a) add `#[derive(Default)]`/manual `Default` to `Scene` in `carapace` if all fields are `Default` (small additive engine change, allowed — see spec precedent for additive engine changes), or (b) change `empty_scene()` to lay out a trivial fixture via `carapace::skin::load_dir` + `Engine::layout`. Pick (a) if the fields allow it; it keeps the test GPU-free.

Run: `cargo test -p carapace-ffi --lib snapshot`
Expected: FAIL — `snapshot` module not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/carapace-ffi/src/snapshot.rs`:

```rust
//! The render thread's published, read-only view of the world. After each frame the loop calls
//! `publish`; the C query exports (`hit_test`, `active_tier`) read it on the CALLER's thread with a
//! short read-lock and no engine access — so classification is sub-millisecond and never blocks the
//! render thread. The snapshot is ≤1 frame stale, which is fine for chrome hit-testing.
//!
//! Host-portable (no GPU): `SnapshotTier` mirrors `render::Tier` so this module needs no Apple gate.

use std::sync::{Arc, RwLock};

use carapace::scene::{HitKind, Pt, Scene};

/// Present tier, mirrored so `snapshot.rs` stays GPU-free. Maps to/from `render::Tier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotTier {
    Readback,
    Shared,
}

/// The latest laid-out scene + tier. `scene` is `None` until the first frame is published.
pub struct SceneSnapshot {
    pub scene: Option<Scene>,
    pub tier: SnapshotTier,
}

/// Shared, atomically-swappable snapshot. Readers take a read-lock and clone the inner `Arc` out
/// (cheap); the writer takes a write-lock only to swap the `Arc` (never holds it across a render).
pub type SnapshotCell = Arc<RwLock<Arc<SceneSnapshot>>>;

pub fn new_cell(initial_tier: SnapshotTier) -> SnapshotCell {
    Arc::new(RwLock::new(Arc::new(SceneSnapshot {
        scene: None,
        tier: initial_tier,
    })))
}

pub fn publish(cell: &SnapshotCell, scene: Scene, tier: SnapshotTier) {
    let next = Arc::new(SceneSnapshot { scene: Some(scene), tier });
    // A poisoned lock (a reader panicked mid-read) must not wedge the render thread: recover it.
    let mut guard = cell.write().unwrap_or_else(|e| e.into_inner());
    *guard = next;
}

fn load(cell: &SnapshotCell) -> Arc<SceneSnapshot> {
    cell.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn hit_kind_of(cell: &SnapshotCell, p: Pt) -> HitKind {
    match &load(cell).scene {
        Some(scene) => scene.hit_kind(p),
        None => HitKind::Passthrough,
    }
}

pub fn tier_of(cell: &SnapshotCell) -> SnapshotTier {
    load(cell).tier
}
```

Add to `crates/carapace-ffi/src/lib.rs`:

```rust
mod snapshot;
```

If Step 2 chose option (a), also add `Default` to `Scene` in `crates/carapace/src/scene.rs` (derive if fields allow) and commit that as part of this task.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi --lib snapshot`
Expected: PASS (3 tests).

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/snapshot.rs crates/carapace-ffi/src/lib.rs crates/carapace/src/scene.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): read-only scene snapshot cell for lock-free queries (v2)"
```

---

### Task 3: `frame_ready` callback + surface `Send` wrapper

**Files:**
- Modify: `crates/carapace-ffi/src/host.rs:11-23` (add `frame_ready` to the vtable)
- Create: `crates/carapace-ffi/src/render_thread.rs` (start it with just the `Send` surface wrapper + a compile assertion; the loop comes in later tasks) — Apple-gated
- Modify: `crates/carapace-ffi/src/lib.rs` (Apple-gated `mod render_thread;`)
- Test: inline in `host.rs` (vtable shape) + inline Apple-gated in `render_thread.rs` (Send wrapper)

**Interfaces:**
- Consumes: `CarapaceHostVTable` (v1 shape).
- Produces:
  - `CarapaceHostVTable` gains `pub frame_ready: Option<extern "C" fn(*mut c_void, u32, u64)>` (ctx, surface_index, frame_id).
  - In `render_thread.rs`: `pub(crate) struct SendSurfaces { pub surfaces: Vec<*const c_void>, pub content: *const c_void }` with `unsafe impl Send for SendSurfaces {}`.

- [ ] **Step 1: Write the failing test**

Add to `host.rs` `#[cfg(test)] mod tests`:

```rust
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
```

Create `crates/carapace-ffi/src/render_thread.rs`:

```rust
//! The dedicated render thread: owns the `!Send` Engine + GPU, runs the pacing loop. Apple-only.
#![cfg(any(target_os = "macos", target_os = "ios"))]

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-ffi` (on macOS)
Expected: FAIL to compile — `frame_ready` not a field of `CarapaceHostVTable`; `render_thread` module not declared.

- [ ] **Step 3: Write minimal implementation**

In `host.rs`, extend the struct (add the field after `invoke`):

```rust
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
```

Update the comment on the existing `unsafe impl Send/Sync for CarapaceHostVTable` (host.rs:20-23) to note the vtable now legitimately crosses to the render thread at construction and its pointers are host-guaranteed thread-safe (§ callback contract in the spec).

Every place in the crate that constructs a `CarapaceHostVTable` literal must add `frame_ready: None` (or a real fn). Find them: `grep -rn "get_str: None" crates/carapace-ffi/src` and the `host.rs` test `vtable()` helper — update each.

Add to `lib.rs`:

```rust
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod render_thread;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi`
Expected: PASS — vtable + Send tests green; existing host tests still green.

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/host.rs crates/carapace-ffi/src/render_thread.rs crates/carapace-ffi/src/lib.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): frame_ready vtable slot + narrow Send surface wrapper (v2)"
```

---

### Task 4: Render-thread lifecycle — spawn, construct-on-thread, init handshake, shutdown/join

**Files:**
- Modify: `crates/carapace-ffi/src/render.rs` (move `build_present` here from `handle.rs`; make it `pub(crate)`)
- Modify: `crates/carapace-ffi/src/render_thread.rs` (add `RenderThread` state, `spawn`, the loop skeleton with shutdown — NO pacing/present yet)
- Modify: `crates/carapace-ffi/src/handle.rs` (rewrite `CarapaceEngine` as the front-end; rewrite `carapace_create`/`carapace_destroy`; `CarapaceCreateDesc` gains the surface pool)
- Test: Apple-gated in `handle.rs` (`create` returns Ok + `destroy` joins; bad skin → ErrBadSkin)

**Interfaces:**
- Consumes: `carapace::skin::load_dir`, `carapace::engine::Engine::new`, `carapace::vocab::VocabRegistry::base`, `render::{init_gpu, build_present, Tier, GpuCtx, Present}`, `host::{FfiHost, CarapaceHostVTable}`, `queue::{Command, CommandTx, CommandRx, drain_coalescing, PointerKind}`, `snapshot::{SnapshotCell, SnapshotTier, new_cell}`, `SendSurfaces`.
- Produces:
  - `pub(crate) struct RenderThread { engine, renderer, gpu, presents: Vec<Present>, surfaces: Vec<IOSurfaceRef>, held: Vec<bool>, content: Option<ContentTex>, tier: Tier, w, h, cw, ch, fps: u32, next_surface: usize, frame_id: u64, last_render: Instant }`
  - `pub(crate) enum InitResult { Ok { cw: u32, ch: u32, tier: Tier }, Err(CarapaceStatus, String) }`
  - `pub(crate) fn spawn(dir: PathBuf, vtable: CarapaceHostVTable, surfaces: SendSurfaces, w: u32, h: u32, rx: CommandRx, cell: SnapshotCell, poisoned: Arc<AtomicBool>, init_tx: mpsc::Sender<InitResult>) -> JoinHandle<()>`
  - New `CarapaceEngine` (front-end) fields: `tx: CommandTx`, `snapshot: SnapshotCell`, `poisoned: Arc<AtomicBool>`, `join: Option<JoinHandle<()>>`, `tier: CarapaceTier` (create-time, for immediate `active_tier`).
  - New `CarapaceCreateDesc { skin_dir, vtable, surfaces: *const IOSurfaceRef, surface_count: u32, content_surface: IOSurfaceRef, w, h }`.

- [ ] **Step 1: Write the failing test**

Replace the v1 create/tick tests in `handle.rs` that construct the old single-surface desc with a pool-based helper, and add the lifecycle test. In `handle.rs` `test_support` (Apple-gated), add a multi-surface creator:

```rust
pub(crate) fn create_test_handle_pool(w: u32, h: u32, count: usize) -> (*mut CarapaceEngine, Vec<IOSurfaceRef>) {
    let surfaces: Vec<IOSurfaceRef> =
        (0..count).map(|_| make_bgra_iosurface(w as usize, h as usize)).collect();
    let path = std::ffi::CString::new(SKIN_DIR).unwrap();
    let desc = CarapaceCreateDesc {
        skin_dir: path.as_ptr(),
        vtable: empty_vtable(),
        surfaces: surfaces.as_ptr(),
        surface_count: count as u32,
        content_surface: std::ptr::null_mut(),
        w,
        h,
    };
    let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
    assert_eq!(unsafe { carapace_create(&desc, &mut handle) }, CarapaceStatus::Ok);
    assert!(!handle.is_null());
    (handle, surfaces)
}
```

Add a lifecycle test module:

```rust
#[cfg(all(test, target_os = "macos"))]
mod lifecycle_tests {
    use super::test_support::create_test_handle_pool;
    use super::*;

    #[test]
    fn create_spawns_thread_and_destroy_joins_cleanly() {
        let (handle, _surfaces) = create_test_handle_pool(300, 140, 3);
        // No tick call exists anymore; create alone must produce a live handle.
        unsafe { carapace_destroy(handle) }; // must join the render thread without hanging/crashing
    }

    #[test]
    fn create_reports_bad_skin_for_missing_dir() {
        let surfaces = [super::test_support::make_bgra_iosurface(4, 4)];
        let path = std::ffi::CString::new("/no/such/skin/dir").unwrap();
        let desc = CarapaceCreateDesc {
            skin_dir: path.as_ptr(),
            vtable: super::test_support::empty_vtable(),
            surfaces: surfaces.as_ptr(),
            surface_count: 1,
            content_surface: std::ptr::null_mut(),
            w: 4,
            h: 4,
        };
        let mut handle: *mut CarapaceEngine = std::ptr::null_mut();
        assert_eq!(unsafe { carapace_create(&desc, &mut handle) }, CarapaceStatus::ErrBadSkin);
        assert!(handle.is_null());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --lib lifecycle`
Expected: FAIL to compile — `CarapaceCreateDesc` has no `surfaces`/`surface_count`; `create_test_handle_pool` undefined.

- [ ] **Step 3: Write the implementation**

**3a.** In `render.rs`, move `build_present` from `handle.rs` and make it `pub(crate) fn build_present(gpu: &GpuCtx, surface: IOSurfaceRef, w: u32, h: u32) -> (Present, Tier)` (body unchanged from `handle.rs:91-123`).

**3b.** In `render_thread.rs`, add the state + spawn + skeleton loop:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use carapace::engine::Engine;
use carapace::render::Renderer;

use crate::guard::set_last_error;
use crate::handle::{CarapaceTier, ContentTex};
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::queue::{Command, CommandRx};
use crate::render::{GpuCtx, IOSurfaceRef, Present, Tier, build_content, build_present, init_gpu};
use crate::snapshot::{SnapshotCell, SnapshotTier};

pub(crate) enum InitResult {
    Ok { cw: u32, ch: u32, tier: Tier },
    Err(crate::guard::CarapaceStatus, String),
}

struct RenderThread {
    engine: Engine,
    renderer: Renderer,
    gpu: GpuCtx,
    presents: Vec<Present>,
    surfaces: Vec<IOSurfaceRef>,
    held: Vec<bool>,
    content: Option<ContentTex>,
    tier: Tier,
    w: u32,
    h: u32,
    cw: u32,
    ch: u32,
    fps: u32,
    next_surface: usize,
    frame_id: u64,
    last_render: Instant,
}

const DEFAULT_FPS: u32 = 60;

/// Build the engine + GPU + per-surface present pool ON the render thread. Returns the state, or a
/// status + message on failure (reported back so `carapace_create` can return synchronously).
fn build(
    dir: &PathBuf,
    vtable: CarapaceHostVTable,
    surfaces: Vec<IOSurfaceRef>,
    content_surface: IOSurfaceRef,
    w: u32,
    h: u32,
) -> Result<RenderThread, (crate::guard::CarapaceStatus, String)> {
    use crate::guard::CarapaceStatus;
    let (_m, source) = carapace::skin::load_dir(dir)
        .map_err(|e| (CarapaceStatus::ErrBadSkin, format!("skin load failed: {e:?}")))?;
    let engine = Engine::new(
        Box::new(FfiHost::new(vtable)),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .map_err(|e| (CarapaceStatus::ErrBadSkin, format!("engine init failed: {e:?}")))?;
    let (cw, ch) = engine.scene().canvas;
    let gpu = init_gpu().map_err(|m| (CarapaceStatus::ErrGpuInit, m))?;
    let renderer = Renderer::new(&gpu.device);

    // One Present per pooled surface. Tier is the WEAKEST any surface resolved to (if any fell back
    // to Readback, report Readback) so `active_tier` never over-promises.
    let mut presents = Vec::with_capacity(surfaces.len());
    let mut tier = Tier::Shared;
    for &s in &surfaces {
        let (p, t) = build_present(&gpu, s, w, h);
        if t == Tier::Readback {
            tier = Tier::Readback;
        }
        presents.push(p);
    }
    let content = build_content(&gpu, content_surface);
    let held = vec![false; surfaces.len()];
    Ok(RenderThread {
        engine,
        renderer,
        gpu,
        presents,
        surfaces,
        held,
        content,
        tier,
        w,
        h,
        cw,
        ch,
        fps: DEFAULT_FPS,
        next_surface: 0,
        frame_id: 0,
        last_render: Instant::now(),
    })
}

pub(crate) fn spawn(
    dir: PathBuf,
    vtable: CarapaceHostVTable,
    send_surfaces: super::render_thread::SendSurfaces,
    w: u32,
    h: u32,
    rx: CommandRx,
    cell: SnapshotCell,
    poisoned: Arc<AtomicBool>,
    init_tx: mpsc::Sender<InitResult>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("carapace-render".into())
        .spawn(move || {
            let SendSurfaces { surfaces, content } = send_surfaces;
            let surfaces: Vec<IOSurfaceRef> = surfaces.into_iter().map(|p| p as IOSurfaceRef).collect();
            let mut rt = match build(&dir, vtable, surfaces, content as IOSurfaceRef, w, h) {
                Ok(rt) => {
                    let _ = init_tx.send(InitResult::Ok { cw: rt.cw, ch: rt.ch, tier: rt.tier });
                    rt
                }
                Err((status, msg)) => {
                    set_last_error(&msg);
                    let _ = init_tx.send(InitResult::Err(status, msg));
                    return;
                }
            };
            run_loop(&mut rt, &rx, &cell, &poisoned);
        })
        .expect("spawn carapace render thread")
}

/// Skeleton loop: block for a command, handle Shutdown, ignore the rest for now (pacing/present land
/// in Task 6/5). Task 6 replaces this with `recv_timeout`-based free-run pacing.
fn run_loop(_rt: &mut RenderThread, rx: &CommandRx, _cell: &SnapshotCell, _poisoned: &Arc<AtomicBool>) {
    while let Ok(cmd) = rx.recv() {
        if matches!(cmd, Command::Shutdown) {
            break;
        }
    }
}

use super::render_thread::SendSurfaces;
```

(Adjust the `use super::render_thread::SendSurfaces;` — since this IS `render_thread`, reference `SendSurfaces` directly; the snippet shows intent. Also map `SnapshotTier`↔`Tier` where needed.)

**3c.** In `render.rs`, make `build_content` `pub(crate)` too (currently in `handle.rs:129`) — move it alongside `build_present`, or re-export. Keep `ContentTex` in `handle.rs` but `pub(crate)`.

**3d.** Rewrite `handle.rs`:
- New `CarapaceCreateDesc` (surface pool). New `CarapaceEngine` front-end (fields listed in Interfaces). Keep `CarapaceTier` enum + its `Tier`→`CarapaceTier` mapping.
- `carapace_create`: validate `out`/`desc`/`skin_dir`/`surface_count >= 1`; read the `surfaces` array into a `Vec<*const c_void>`; build `SendSurfaces`; create the channel, snapshot cell (`new_cell` seeded with a provisional tier), poison flag, and a one-shot `init` channel; call `render_thread::spawn(...)`; **block on `init.recv()`**; on `InitResult::Ok` box the front-end handle (store `tx`, `snapshot`, `poisoned`, `join`, `tier`) into `*out` and return `Ok`; on `Err` set `last_error`, join the thread, leave `*out` null, return the status. Whole body under `ffi_guard_no_handle!`.
- `carapace_destroy`: if non-null, `Box::from_raw`; send `Command::Shutdown` (ignore error — thread may already be gone); `join.take().map(|j| j.join())`. Null-safe; valid on poisoned/exited.

```rust
// carapace_create core (inside ffi_guard_no_handle!, after arg validation):
let count = desc.surface_count as usize;
if count == 0 { set_last_error("carapace_create: surface_count must be >= 1"); return CarapaceStatus::ErrNullArg; }
if desc.surfaces.is_null() { set_last_error("carapace_create: null surfaces"); return CarapaceStatus::ErrNullArg; }
let surfaces: Vec<*const c_void> =
    (0..count).map(|i| unsafe { *desc.surfaces.add(i) } as *const c_void).collect();
let send_surfaces = SendSurfaces { surfaces, content: desc.content_surface as *const c_void };

let (tx, rx) = std::sync::mpsc::channel::<Command>();
let poisoned = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
let cell = snapshot::new_cell(snapshot::SnapshotTier::Shared); // refined below once init reports tier
let (init_tx, init_rx) = std::sync::mpsc::channel::<render_thread::InitResult>();
let join = render_thread::spawn(dir, desc.vtable, send_surfaces, desc.w, desc.h, rx, cell.clone(), poisoned.clone(), init_tx);

match init_rx.recv() {
    Ok(render_thread::InitResult::Ok { tier, .. }) => {
        let ctier = match tier { Tier::Readback => CarapaceTier::Readback, Tier::Shared => CarapaceTier::Shared };
        // seed the snapshot's tier so active_tier is correct before frame 1
        snapshot::publish_tier_only(&cell, match tier { Tier::Readback => snapshot::SnapshotTier::Readback, Tier::Shared => snapshot::SnapshotTier::Shared });
        let handle = Box::into_raw(Box::new(CarapaceEngine {
            tx, snapshot: cell, poisoned, join: Some(join), tier: ctier,
        }));
        unsafe { *out = handle };
        CarapaceStatus::Ok
    }
    Ok(render_thread::InitResult::Err(status, msg)) => { set_last_error(&msg); let _ = join.join(); status }
    Err(_) => { set_last_error("carapace_create: render thread died during init"); let _ = join.join(); CarapaceStatus::ErrPanic }
}
```

Add `snapshot::publish_tier_only(cell, tier)` to `snapshot.rs` (swaps tier, keeps `scene: None`). (Small addition to Task 2's module — add it here with a one-line test appended to `snapshot`'s tests.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi --lib lifecycle` then `cargo test -p carapace-ffi`
Expected: PASS — `create_spawns_thread_and_destroy_joins_cleanly` and `create_reports_bad_skin_for_missing_dir` green. (The old v1 `create_tick_destroy_renders_nonblank` / pointer / poison tests will be re-homed in Tasks 5/6/8 — comment them out with a `// re-enabled in Task N` note if they block compilation now, and track that in the task's report.)

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/{render.rs,render_thread.rs,handle.rs,snapshot.rs}
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): spawn render thread, construct engine on it, sync init handshake + join (v2)"
```

---

### Task 5: Render one frame into a pooled surface + `frame_ready`

**Files:**
- Modify: `crates/carapace-ffi/src/render_thread.rs` (add `render_one`, surface selection, `frame_ready` dispatch)
- Test: Apple-gated in `render_thread.rs` — via `carapace_invalidate` (added minimally here) a single frame renders non-blank and fires `frame_ready`

**Interfaces:**
- Consumes: `render::{render_frame, blit, readback_rgba, copy_into_iosurface, upload_iosurface_to_texture, Present}`, the vtable's `frame_ready`.
- Produces:
  - `impl RenderThread { fn pick_free_surface(&self) -> Option<usize>; fn render_one(&mut self, dt: Duration, vtable: &CarapaceHostVTable); }`
  - The `RenderThread` also stores the `vtable: CarapaceHostVTable` (add the field; it's `Copy` + `Send`) so `render_one` can fire `frame_ready`.

- [ ] **Step 1: Write the failing test**

In `render_thread.rs`, Apple-gated tests. Reuse a small IOSurface creator (factor `make_bgra_iosurface` into a shared `#[cfg(all(test, target_os="macos"))]` test util module, e.g. `crate::testutil`, so `handle.rs`, `hit.rs`, and `render_thread.rs` share one copy — DRY; currently duplicated in handle.rs and hit.rs).

```rust
#[cfg(all(test, target_os = "macos"))]
mod render_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    static FRAME_READY_COUNT: AtomicU32 = AtomicU32::new(0);
    static LAST_FRAME_ID: AtomicU64 = AtomicU64::new(0);

    extern "C" fn on_frame_ready(_ctx: *mut std::ffi::c_void, _idx: u32, frame_id: u64) {
        FRAME_READY_COUNT.fetch_add(1, Ordering::SeqCst);
        LAST_FRAME_ID.store(frame_id, Ordering::SeqCst);
    }

    #[test]
    fn one_invalidate_renders_nonblank_and_fires_frame_ready_once() {
        FRAME_READY_COUNT.store(0, Ordering::SeqCst);
        // Build via carapace_create with fps set to 0 (paused) so only our invalidate renders.
        let (w, h) = (300u32, 140u32);
        let vt = crate::host::CarapaceHostVTable {
            ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: None,
            frame_ready: Some(on_frame_ready),
        };
        let (handle, surfaces) = crate::handle::test_support::create_test_handle_pool_vt(w, h, 2, vt);
        assert_eq!(unsafe { crate::handle::carapace_set_frame_rate(handle, 0) }, crate::guard::CarapaceStatus::Ok);
        assert_eq!(unsafe { crate::handle::carapace_invalidate(handle) }, crate::guard::CarapaceStatus::Ok);
        // Give the render thread a moment to process the single frame.
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(FRAME_READY_COUNT.load(Ordering::SeqCst), 1, "exactly one frame");
        assert_eq!(LAST_FRAME_ID.load(Ordering::SeqCst), 1, "frame_id starts at 1");
        // The surface handed to frame_ready must be non-blank.
        // (Reuse handle.rs's iosurface_has_nonzero_pixels via a pub(crate) test helper.)
        unsafe { crate::handle::carapace_destroy(handle) };
        let _ = surfaces;
    }
}
```

(This test depends on `carapace_invalidate`/`carapace_set_frame_rate` existing minimally — add them in this task as thin `tx.send` wrappers; Task 6 gives the loop its real pacing behavior. `create_test_handle_pool_vt` = the pool creator with a supplied vtable.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --lib render_tests`
Expected: FAIL — `carapace_invalidate`/`carapace_set_frame_rate`/`render_one` not defined.

- [ ] **Step 3: Write the implementation**

Add `vtable: CarapaceHostVTable` to `RenderThread` (set it in `build`). Implement:

```rust
impl RenderThread {
    fn pick_free_surface(&self) -> Option<usize> {
        // Round-robin from next_surface, skipping surfaces the host still holds.
        let n = self.surfaces.len();
        (0..n).map(|i| (self.next_surface + i) % n).find(|&i| !self.held[i])
    }

    fn render_one(&mut self, dt: Duration) {
        let Some(idx) = self.pick_free_surface() else {
            // Backpressure: host holds every surface. Skip this frame (never block, never tear).
            return;
        };
        // Upload this frame's host content (CPU->GPU coherency), then render into the chosen surface.
        if let Some(c) = self.content.as_ref() {
            unsafe { crate::render::upload_iosurface_to_texture(&self.gpu.queue, c.surface, &c.tex, c.w, c.h) };
        }
        let host_view = self.content.as_ref().map(|c| ("host", &c.view));
        let (w, h) = (self.w, self.h);
        match &self.presents[idx] {
            Present::Shared { off, iosurface_view, blitter, .. } => {
                crate::render::render_frame(&mut self.engine, &mut self.renderer, &self.gpu, &off.view, w, h, dt, false, host_view);
                crate::render::blit(&self.gpu, blitter, &off.view, iosurface_view);
            }
            Present::Readback { off } => {
                crate::render::render_frame(&mut self.engine, &mut self.renderer, &self.gpu, &off.view, w, h, dt, true, host_view);
                let rgba = crate::render::readback_rgba(&self.gpu, &off.tex, w, h);
                unsafe { crate::render::copy_into_iosurface(self.surfaces[idx], &rgba, w, h) };
            }
        }
        self.held[idx] = true;
        self.next_surface = (idx + 1) % self.surfaces.len();
        self.frame_id += 1;
        // Announce readiness to the host (on THIS render thread).
        if let Some(cb) = self.vtable.frame_ready {
            cb(self.vtable.ctx, idx as u32, self.frame_id);
        }
    }
}
```

Note the borrow split: `render_frame` needs `&mut self.engine`/`&mut self.renderer` while reading `&self.presents[idx]`. If the borrow checker objects, destructure the fields into locals at the top of `render_one` (mirror the v1 `tick_inner` destructuring pattern in the old `handle.rs:268`). Prefer that pattern.

Add thin exports in `handle.rs` (real pacing behavior in Task 6):

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_invalidate(ptr: *mut CarapaceEngine) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else { return CarapaceStatus::ErrNullArg; };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) { return CarapaceStatus::ErrPoisoned; }
    let _ = e.tx.send(Command::Invalidate);
    CarapaceStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_set_frame_rate(ptr: *mut CarapaceEngine, fps: u32) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else { return CarapaceStatus::ErrNullArg; };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) { return CarapaceStatus::ErrPoisoned; }
    let _ = e.tx.send(Command::SetFrameRate(fps));
    CarapaceStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_release_surface(ptr: *mut CarapaceEngine, index: u32) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else { return CarapaceStatus::ErrNullArg; };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) { return CarapaceStatus::ErrPoisoned; }
    let _ = e.tx.send(Command::ReleaseSurface(index));
    CarapaceStatus::Ok
}
```

For this task the skeleton `run_loop` must at least handle `Invalidate` → `render_one`, `SetFrameRate` → set `rt.fps`, `ReleaseSurface(i)` → `rt.held[i as usize] = false`, `Shutdown` → break. (Full pacing = Task 6.) Compute `dt` as `now - last_render` for the invalidate path.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi --lib render_tests`
Expected: PASS — one `frame_ready` with `frame_id == 1`, surface non-blank.

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/{render_thread.rs,handle.rs}
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): render into a pooled surface + frame_ready dispatch (v2)"
```

---

### Task 6: Free-run pacing loop (wall-clock dt, park/wake, fps control)

**Files:**
- Modify: `crates/carapace-ffi/src/render_thread.rs` (replace the skeleton `run_loop` with the real pacing loop)
- Test: Apple-gated — running at fps>0 produces multiple frames over a short window; fps=0 stays paused until invalidate; `dt` clamp caps a long idle gap

**Interfaces:**
- Consumes: `queue::drain_coalescing`, `queue::{Command, PointerKind}`, `Instant`, `Engine::handle_pointer_resolved`.
- Produces: final `run_loop` behavior; a `RenderThread::apply(&mut self, cmd) -> bool` returning `false` on Shutdown.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(all(test, target_os = "macos"))]
mod pacing_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNT: AtomicU32 = AtomicU32::new(0);
    extern "C" fn count_ready(_c: *mut std::ffi::c_void, _i: u32, _f: u64) { COUNT.fetch_add(1, Ordering::SeqCst); }

    fn make(fps_vt: crate::host::CarapaceHostVTable) -> *mut crate::handle::CarapaceEngine {
        let (h, _s) = crate::handle::test_support::create_test_handle_pool_vt(300, 140, 3, fps_vt);
        h
    }

    #[test]
    fn free_run_at_60_produces_many_frames_in_300ms() {
        COUNT.store(0, Ordering::SeqCst);
        let vt = crate::host::CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: None, frame_ready: Some(count_ready) };
        let h = make(vt); // default fps = 60, running immediately
        // Release surfaces as they come so the loop never backpressures. Simplest: 3 surfaces + a
        // releaser thread is overkill for the test; instead poll-release all indices periodically.
        for _ in 0..30 { for i in 0..3 { unsafe { let _ = crate::handle::carapace_release_surface(h, i); } } std::thread::sleep(std::time::Duration::from_millis(10)); }
        let n = COUNT.load(Ordering::SeqCst);
        assert!(n >= 5, "expected several frames at 60fps in ~300ms, got {n}");
        unsafe { crate::handle::carapace_destroy(h) };
    }

    #[test]
    fn paused_engine_renders_only_on_invalidate() {
        COUNT.store(0, Ordering::SeqCst);
        let vt = crate::host::CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: None, frame_ready: Some(count_ready) };
        let h = make(vt);
        unsafe { let _ = crate::handle::carapace_set_frame_rate(h, 0); }
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(COUNT.load(Ordering::SeqCst), 0, "paused: no frames without invalidate");
        unsafe { let _ = crate::handle::carapace_invalidate(h); }
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
        unsafe { crate::handle::carapace_destroy(h) };
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --lib pacing_tests`
Expected: FAIL — skeleton loop doesn't free-run (COUNT stays 0 in the 60fps test).

- [ ] **Step 3: Write the implementation**

Replace `run_loop`:

```rust
fn run_loop(rt: &mut RenderThread, rx: &CommandRx, cell: &SnapshotCell, poisoned: &Arc<AtomicBool>) {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut pending: Vec<Command> = Vec::new();
    loop {
        // Decide how to wait: running → wake at the frame deadline OR on a command; paused → block.
        let wait = if rt.fps > 0 {
            let interval = Duration::from_secs_f64(1.0 / rt.fps as f64);
            let since = rt.last_render.elapsed();
            interval.saturating_sub(since)
        } else {
            Duration::from_secs(3600) // effectively "block until a command"
        };

        let woke = rx.recv_timeout(wait);
        match woke {
            Ok(first) => {
                drain_coalescing(rx, first, &mut pending);
                let mut invalidated = false;
                for cmd in pending.drain(..) {
                    match cmd {
                        Command::Shutdown => return,
                        Command::SetFrameRate(f) => rt.fps = f,
                        Command::ReleaseSurface(i) => { if let Some(h) = rt.held.get_mut(i as usize) { *h = false; } }
                        Command::Invalidate => invalidated = true,
                        Command::Pointer { x, y, kind } => {
                            if let Some(ev) = map_pointer(kind) {
                                rt.engine.handle_pointer_resolved(rt.cw as f32, rt.ch as f32,
                                    carapace::scene::Pt { x: x as f32, y: y as f32 }, ev);
                            }
                            invalidated = true; // input should show a frame even when paused
                        }
                    }
                }
                if invalidated {
                    render_guarded(rt, cell, poisoned, &mut catch_unwind_render);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Frame deadline while running: render one paced frame.
                if rt.fps > 0 {
                    render_guarded(rt, cell, poisoned, &mut catch_unwind_render);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return, // handle dropped
        }
        if poisoned.load(Ordering::Acquire) { return; }
    }
}

fn render_guarded(rt: &mut RenderThread, cell: &SnapshotCell, poisoned: &Arc<AtomicBool>, _f: &mut ()) {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let now = Instant::now();
    let interval = if rt.fps > 0 { Duration::from_secs_f64(1.0 / rt.fps as f64) } else { Duration::from_millis(16) };
    let raw = now.saturating_duration_since(rt.last_render);
    let dt = raw.min(interval * 4); // clamp a huge idle/after-park gap
    let result = catch_unwind(AssertUnwindSafe(|| {
        rt.render_one(dt);
        // Publish the just-laid-out scene for hit_test/active_tier. Engine::layout is cheap + pure.
        let scene = rt.engine.layout(rt.cw as f32, rt.ch as f32);
        let tier = match rt.tier { Tier::Readback => SnapshotTier::Readback, Tier::Shared => SnapshotTier::Shared };
        crate::snapshot::publish(cell, scene, tier);
    }));
    rt.last_render = now;
    if result.is_err() {
        poisoned.store(true, Ordering::Release);
        // last_error already set by the process-wide panic hook installed in carapace_create.
    }
}

fn map_pointer(kind: crate::queue::PointerKind) -> Option<carapace::engine::PointerEvent> {
    use crate::queue::PointerKind::*;
    match kind {
        Press => Some(carapace::engine::PointerEvent::Press),
        _ => None, // engine models Press today; others are plumbed, no-op for now (additive)
    }
}
```

(Simplify the `_f: &mut ()` placeholder away — `render_guarded` needs only `rt`, `cell`, `poisoned`. The snippet shows the structure; drop the unused param. The `pacing_tests` `dt` clamp is covered by `raw.min(interval * 4)`.)

Publishing note: `render_one` already laid out inside `render_frame`; re-laying out for the snapshot double-computes layout. To avoid that, have `render_frame`/`render_one` return the `Scene` it laid out, or lay out once in `render_guarded` and pass the scene into a `render_into(scene, ...)`. **Choose one:** refactor `render_one` to `layout → render_scene(&scene) → return scene`, then publish that exact scene. Implement this to avoid the redundant layout (matches the spec's "publish the laid-out scene").

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi --lib pacing_tests`
Expected: PASS — free-run yields ≥5 frames; paused yields 0 then exactly 1 after invalidate.

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/render_thread.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): free-run pacing loop with wall-clock dt + park/wake + fps control (v2)"
```

---

### Task 7: Wire queries to the snapshot (`hit_test`, `active_tier`)

**Files:**
- Modify: `crates/carapace-ffi/src/hit.rs` (`carapace_hit_test` reads the snapshot; no engine access)
- Modify: `crates/carapace-ffi/src/handle.rs` (re-add `carapace_active_tier` reading the snapshot/front-end)
- Test: Apple-gated — after a frame lands, `hit_test` classifies outside=Passthrough, control=Control; `active_tier` returns a valid tier immediately and after a frame

**Interfaces:**
- Consumes: `snapshot::{hit_kind_of, tier_of}`, `CarapaceEngine.snapshot`, `CarapaceEngine.poisoned`.
- Produces: thread-safe `carapace_hit_test` + `carapace_active_tier` that never touch the render thread.

- [ ] **Step 1: Write the failing test**

Rewrite `hit.rs`'s `hit_tests` module to use the pooled create helper and to render a frame before asserting (so the snapshot is populated):

```rust
#[test]
fn hit_test_after_a_frame_classifies_outside_passthrough_and_control_inside() {
    let (handle, _s) = crate::handle::test_support::create_test_handle_pool(300, 140, 2);
    // Force one frame so the snapshot is populated, then release + let it publish.
    unsafe { let _ = crate::handle::carapace_set_frame_rate(handle, 0); }
    unsafe { let _ = crate::handle::carapace_invalidate(handle); }
    std::thread::sleep(std::time::Duration::from_millis(150));

    let mut kind = CarapaceHitKind::Control;
    assert_eq!(unsafe { carapace_hit_test(handle, -100.0, -100.0, &mut kind) }, CarapaceStatus::Ok);
    assert_eq!(kind as i32, CarapaceHitKind::Passthrough as i32);

    let mut kind = CarapaceHitKind::Passthrough;
    assert_eq!(unsafe { carapace_hit_test(handle, 55.0, 55.0, &mut kind) }, CarapaceStatus::Ok);
    assert_eq!(kind as i32, CarapaceHitKind::Control as i32);

    unsafe { crate::handle::carapace_destroy(handle) };
}

#[test]
fn active_tier_is_valid_before_and_after_first_frame() {
    let (handle, _s) = crate::handle::test_support::create_test_handle_pool(300, 140, 2);
    let mut tier = CarapaceTier::Readback;
    assert_eq!(unsafe { crate::handle::carapace_active_tier(handle, &mut tier) }, CarapaceStatus::Ok);
    assert!(matches!(tier, CarapaceTier::Readback | CarapaceTier::Shared));
    unsafe { crate::handle::carapace_destroy(handle) };
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --lib hit_tests active_tier`
Expected: FAIL to compile — `carapace_hit_test` still references `e.engine`; `carapace_active_tier` was removed in Task 4.

- [ ] **Step 3: Write the implementation**

Rewrite `carapace_hit_test` (hit.rs):

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_hit_test(
    ptr: *mut CarapaceEngine, x: f64, y: f64, out: *mut CarapaceHitKind,
) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else { return CarapaceStatus::ErrNullArg; };
    if out.is_null() { return CarapaceStatus::ErrNullArg; }
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) { return CarapaceStatus::ErrPoisoned; }
    let kind = match crate::snapshot::hit_kind_of(&e.snapshot, Pt { x: x as f32, y: y as f32 }) {
        HitKind::Passthrough => CarapaceHitKind::Passthrough,
        HitKind::Control => CarapaceHitKind::Control,
        HitKind::Drag => CarapaceHitKind::Drag,
    };
    unsafe { *out = kind };
    CarapaceStatus::Ok
}
```

(No `ffi_guard!` needed — the body is panic-free reads; but keep the poison check. If you prefer symmetry, a guard is fine, but there's no handle mutation to poison.)

Re-add `carapace_active_tier` in `handle.rs`, reading the snapshot (falls back to the create-time `e.tier` — they agree):

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_active_tier(ptr: *mut CarapaceEngine, out: *mut CarapaceTier) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else { return CarapaceStatus::ErrNullArg; };
    if out.is_null() { return CarapaceStatus::ErrNullArg; }
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) { return CarapaceStatus::ErrPoisoned; }
    let tier = match crate::snapshot::tier_of(&e.snapshot) {
        crate::snapshot::SnapshotTier::Readback => CarapaceTier::Readback,
        crate::snapshot::SnapshotTier::Shared => CarapaceTier::Shared,
    };
    unsafe { *out = tier };
    CarapaceStatus::Ok
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi`
Expected: PASS — hit_test + active_tier tests green; whole crate compiles.

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/{hit.rs,handle.rs}
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): serve hit_test/active_tier from the lock-free scene snapshot (v2)"
```

---

### Task 8: Pointer enqueue + poison-across-thread contract

**Files:**
- Modify: `crates/carapace-ffi/src/handle.rs` (`carapace_pointer` → enqueue; add the test-only `carapace_force_panic` as a queued command)
- Modify: `crates/carapace-ffi/src/render_thread.rs` (a test-only forced-panic command path; confirm poison-on-render-panic)
- Test: Apple-gated — a press over the play button fires `host.toggle_play` through the loop; a forced render-thread panic sets poison and every later call returns `ErrPoisoned`; `destroy` still joins

**Interfaces:**
- Consumes: `queue::Command`, front-end `tx`/`poisoned`.
- Produces: `carapace_pointer` enqueues `Command::Pointer`; a `#[cfg(test)]`-only `Command::ForcePanic` variant + export.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(all(test, target_os = "macos"))]
mod v2_pointer_poison_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    static TOGGLED: AtomicBool = AtomicBool::new(false);
    extern "C" fn rec(_c: *mut std::ffi::c_void, action: *const std::ffi::c_char) {
        if unsafe { std::ffi::CStr::from_ptr(action) }.to_string_lossy() == "toggle_play" {
            TOGGLED.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn press_over_button_fires_action_through_the_loop() {
        TOGGLED.store(false, Ordering::SeqCst);
        let vt = crate::host::CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: Some(rec), frame_ready: None };
        let (h, _s) = test_support::create_test_handle_pool_vt(300, 140, 2, vt);
        assert_eq!(unsafe { carapace_pointer(h, 55.0, 55.0, CarapacePointerKind::Press) }, CarapaceStatus::Ok);
        std::thread::sleep(std::time::Duration::from_millis(150)); // loop drains + renders + invokes
        assert!(TOGGLED.load(Ordering::SeqCst), "press should fire host.toggle_play via the loop");
        unsafe { carapace_destroy(h) };
    }

    #[test]
    fn render_thread_panic_poisons_and_subsequent_calls_are_poisoned() {
        let (h, _s) = test_support::create_test_handle_pool(300, 140, 2);
        assert_eq!(unsafe { carapace_force_panic(h) }, CarapaceStatus::Ok); // enqueues; returns immediately
        std::thread::sleep(std::time::Duration::from_millis(150)); // loop panics + sets poison + exits
        assert_eq!(unsafe { carapace_invalidate(h) }, CarapaceStatus::ErrPoisoned);
        assert_eq!(unsafe { carapace_pointer(h, 0.0, 0.0, CarapacePointerKind::Press) }, CarapaceStatus::ErrPoisoned);
        unsafe { carapace_destroy(h) }; // must still join a poisoned/exited thread
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --lib v2_pointer_poison`
Expected: FAIL — `carapace_pointer` still calls the engine directly / `carapace_force_panic` undefined.

- [ ] **Step 3: Write the implementation**

Rewrite `carapace_pointer` to enqueue:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_pointer(ptr: *mut CarapaceEngine, x: f64, y: f64, kind: CarapacePointerKind) -> CarapaceStatus {
    let Some(e) = (unsafe { ptr.as_ref() }) else { return CarapaceStatus::ErrNullArg; };
    if e.poisoned.load(std::sync::atomic::Ordering::Acquire) { return CarapaceStatus::ErrPoisoned; }
    let k = match kind {
        CarapacePointerKind::Press => crate::queue::PointerKind::Press,
        CarapacePointerKind::Release => crate::queue::PointerKind::Release,
        CarapacePointerKind::Move => crate::queue::PointerKind::Move,
        CarapacePointerKind::Enter => crate::queue::PointerKind::Enter,
        CarapacePointerKind::Leave => crate::queue::PointerKind::Leave,
    };
    let _ = e.tx.send(Command::Pointer { x, y, kind: k });
    CarapaceStatus::Ok
}
```

Add a `#[cfg(test)]`-only forced-panic path: a `Command::ForcePanic` variant behind `#[cfg(test)]` (or a debug-only field), handled in the loop by `panic!("forced render-thread panic")` inside `render_guarded`'s `catch_unwind`, and a `#[cfg(all(test, target_os="macos"))] carapace_force_panic` export that enqueues it. Because the panic happens inside `render_guarded`'s `catch_unwind`, it sets `poisoned` and the loop returns — exactly the contract.

Confirm `carapace_destroy` joins a thread that has already exited (join on a finished thread returns immediately) — no change needed beyond Task 4's implementation; the test asserts it.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi`
Expected: PASS — action fires through the loop; render-thread panic poisons; later calls `ErrPoisoned`; destroy joins.

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/{handle.rs,render_thread.rs,queue.rs}
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi): pointer enqueue + render-thread panic poison contract (v2)"
```

---

### Task 9: ABI finalization — remove `tick`, bump version, regenerate + freshness-test the header

**Files:**
- Modify: `crates/carapace-ffi/src/guard.rs:24-25` (`CARAPACE_ABI_MAJOR = 2`, `CARAPACE_ABI_MINOR = 0`)
- Modify: `crates/carapace-ffi/src/lib.rs` (ensure all v2 exports are reachable/re-exported; confirm `carapace_tick` is gone)
- Modify: `crates/carapace-ffi/include/carapace.h` (regenerate)
- Modify/confirm: `crates/carapace-ffi/cbindgen.toml`
- Test: the existing header-freshness test (regenerate in-memory, assert byte-equality) + an `abi_version == 2<<16` test

**Interfaces:**
- Consumes: cbindgen, the freshness test harness already in the crate (find it: `grep -rn "cbindgen\|freshness\|carapace.h" crates/carapace-ffi`).
- Produces: committed v2 `carapace.h`; `carapace_abi_version()` returns `2 << 16`.

- [ ] **Step 1: Write/adjust the failing test**

Add (or adjust the existing abi test) in `lib.rs` tests or wherever v1 tested it:

```rust
#[test]
fn abi_version_is_v2() {
    assert_eq!(carapace_abi_version(), 2 << 16);
}
```

Locate the header-freshness test (v1 had one per the spec). It will now fail because the header is stale (no `frame_ready`, wrong signatures, still has `carapace_tick`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-ffi --lib abi` and the freshness test by name.
Expected: FAIL — abi_version still `0<<16|1`; header freshness mismatch.

- [ ] **Step 3: Implement**

- Set `CARAPACE_ABI_MAJOR = 2; CARAPACE_ABI_MINOR = 0;` in `guard.rs`.
- Confirm `carapace_tick` is fully removed (grep the crate; also remove any lingering v1 tick tests).
- Regenerate the header. Use the crate's existing mechanism (the freshness test typically shells `cbindgen` or calls it in-memory). If it's manual: `cbindgen --config crates/carapace-ffi/cbindgen.toml --crate carapace-ffi --output crates/carapace-ffi/include/carapace.h` (run from repo root; cbindgen is a dev-dep — `cargo run` the freshness test's generator or use the installed binary). Verify the header now contains: `carapace_abi_version`, `carapace_create` (pool desc), `carapace_pointer`, `carapace_hit_test`, `carapace_active_tier`, `carapace_invalidate`, `carapace_set_frame_rate`, `carapace_release_surface`, `carapace_last_error`, `carapace_destroy`, the `frame_ready` fn-ptr in the vtable, `surfaces`/`surface_count` in the desc — and NOT `carapace_tick`.
- Ensure `cbindgen.toml` excludes the `#[cfg(test)]` `carapace_force_panic` (it's test-gated, so cbindgen won't see it — confirm).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-ffi`
Expected: PASS — abi_version v2; header freshness matches the regenerated file.

- [ ] **Step 5: Lint + format + commit**

```bash
cargo fmt
cargo clippy -p carapace-ffi --all-targets -- -D warnings
git add crates/carapace-ffi/src/{guard.rs,lib.rs} crates/carapace-ffi/include/carapace.h crates/carapace-ffi/cbindgen.toml
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(ffi)!: v2 ABI — surface pool + frame_ready + invalidate/set_frame_rate/release_surface, drop tick, bump abi_version to 2.0"
```

---

### Task 10: Docs — README + full-crate verification

**Files:**
- Modify: `crates/carapace-ffi/README.md` (or the carapace-ffi section of the repo README — check both; memory: "keep README current per phase")
- Test: full workspace test + clippy + fmt as the final gate

**Interfaces:** none (docs + verification).

- [ ] **Step 1: Update the README**

Document the v2 model in the carapace-ffi README: carapace owns the render thread; host provides a 2–3 IOSurface pool + a `frame_ready` callback; free-runs at 60fps by default (`set_frame_rate(0)` to pause, `invalidate` to render on demand); handle is now thread-safe; `carapace_tick` removed. Include a minimal host usage sketch (create with a pool → on `frame_ready` swap the displayed surface + `release_surface` the previous → feed `pointer`; call `set_frame_rate`/`invalidate` only for battery/on-demand). Note the callback contract (render-thread, non-blocking, no reentrancy). State abi_version 2.0.

- [ ] **Step 2: Full verification gate**

Run each and confirm green:

```bash
cargo fmt --check
cargo clippy -p carapace-ffi --all-targets -- -D warnings
cargo test -p carapace-ffi                       # host-portable + Apple GPU tests (on macOS)
cargo build -p carapace-ffi --target x86_64-unknown-linux-gnu 2>/dev/null || cargo check -p carapace-ffi  # confirm the non-Apple shell still builds
```

Expected: fmt clean; clippy clean; all tests pass; non-Apple shell compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/carapace-ffi/README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "docs(ffi): document v2 render-thread model + host usage (surface pool, frame_ready, fps)"
```

- [ ] **Step 4: Open the PR** (only when the user asks)

```bash
git push -u origin carapace-ffi-render-thread
gh pr create --title "carapace-ffi v2: render thread + command queue" --body "<summary of the increment, linking the spec>"
```

(No "Generated with Claude Code" footer — per repo convention.)

---

## Self-Review

**Spec coverage:**
- §1 carapace owns thread → Tasks 4 (spawn/construct-on-thread), 6 (loop). ✓
- §2 surface pool + frame_ready → Tasks 3 (vtable slot + Send wrapper), 5 (render into pool + fire). ✓
- §3 free-run 60fps default, wall-clock dt, invalidate/set_frame_rate opt-in, skip-not-block backpressure → Tasks 5 (thin exports + pick_free skip), 6 (pacing + dt clamp). ✓
- §4 snapshot queries → Tasks 2 (cell), 6 (publish), 7 (readers). ✓
- §5 callback contract (render thread, no reentrancy) → documented in Task 3 vtable comment + Task 10 README. ✓
- §6 panic/poison across thread → Tasks 6 (`render_guarded` catch_unwind + poison flag), 8 (forced-panic test + poison short-circuit). ✓
- §7 unsafe crux (SendSurfaces) → Task 3 with explicit safety contract + Send test. ✓
- §8 ABI v2 (remove tick, add exports, MAJOR bump) → Task 9. ✓
- §9 testing (host-portable + Apple-gated + freshness) → distributed; queue/snapshot host-portable (Tasks 1–2), GPU end-to-end (Tasks 5–8), freshness (Task 9). ✓
- §10 non-goals (Windows/Linux/Android, next_wake, per-pixel mask, sample port, resize) → untouched by design; `Command` is additive for later `Resize`. ✓

**Placeholder scan:** The plan flags three implementer decisions that are genuine forks, not vague hand-waves, each with the concrete options spelled out: (a) `Scene::Default` vs fixture-layout for the snapshot test (Task 2 Step 2); (b) borrow-split style in `render_one` (Task 5 Step 3, with the v1 `tick_inner` pattern named); (c) avoiding the redundant layout by returning the scene from `render_one` (Task 6 Step 3). No "TODO/handle errors appropriately" placeholders remain. The `render_guarded` snippet's `_f: &mut ()` placeholder is explicitly called out to be dropped.

**Type consistency:** `Command`/`PointerKind` (queue.rs) used consistently in handle/render_thread; `SnapshotCell`/`SnapshotTier`/`publish`/`hit_kind_of`/`tier_of` consistent across snapshot/handle/hit; `CarapaceEngine` front-end fields (`tx`, `snapshot`, `poisoned`, `join`, `tier`) referenced identically in every export; `Tier`↔`CarapaceTier`↔`SnapshotTier` mappings are the three-way spelled out in Tasks 4/6/7. `frame_ready` signature `(*mut c_void, u32, u64)` identical in host.rs and render_one. `create_test_handle_pool`/`create_test_handle_pool_vt` naming consistent across Tasks 4–8.

One coupling to watch during execution: Task 4 comments out v1's tick/pointer/poison tests, and Tasks 5–8 re-home them; the per-task reports must confirm none were silently dropped (the coverage above lists their replacements).

# Phase 3a — Headless Core — Implementation Design

**Date:** 2026-06-17
**Status:** Approved design; the first half of Phase 3 (the build phase).
**Project:** carapace (repo codename `winamp`)
**Implements:** `2026-06-17-phase2-engine-architecture.md` (§1–§4, §6) — the spine, minus `render`.
**Informed by:** `2026-06-17-phase1-lessons.md`.

## Purpose

Build the **real engine** as a new crate `crates/carapace`, headless: the Phase 2 spine
modules and the `Engine` frame-loop API up to (but not including) rendering. Everything
here is provable without a GPU or window. The render-direct-to-surface path and a
winit/wgpu host app are **Phase 3b** (the next cycle).

Phase 3 is split (decided in brainstorming) into **3a (this doc): headless core** and
**3b: render + host app**, along the Phase 2 testing boundary (most of the engine is
headless; only `render` needs a GPU).

## Scope

**In scope:** `state`, `host`, `scene`, `vocab` (seam + stub primitives), `script`
(sandbox + command queue), `skin` (loader), `swap`, and `engine` (the input→drain→tick
phases of the frame loop). All headless, fully unit- + integration-tested.

**Out of scope (→ Phase 3b):** `render` and the `RenderTarget`/surface integration; the
winit/wgpu host application; any vello/wgpu/winit dependency.

**Out of scope (→ Phase 5):** the real base vocabulary and host-extension primitives;
richer value-fill semantics and shared draw+hotspot geometry (Phase 1 lessons #2–#3). 3a
ships only **stub** primitives.

## Crate structure

- New `crates/carapace`, depends on `hittest` (path), `mlua` (lua54 + vendored), `toml`,
  `serde`. **No** vello/wgpu/winit in 3a.
- Reuses `crates/hittest` as-is.
- `crates/proto` (Phase 1 throwaway) and `crates/spike-render` (Phase 0 spike) **stay for
  now** — reference material for the 3b host app + render integration. Both are removed at
  the **end of Phase 3** (after 3b), once `carapace` supersedes them.

```
crates/carapace/src/
  state.rs    # StateValue; host-owned source of truth
  host.rs     # Host trait (get/actions/invoke/tick) + Command enum + ActionSpec
  scene.rs    # Node + Scene; cached hittest::Region per hotspot; Scene::hit
  vocab.rs    # Primitive + VocabRegistry traits + stub base primitives
  script.rs   # mlua sandbox + command queue; builds a Scene via the registry
  skin.rs     # skin.toml loader (schema/engine/canvas/entry) + compat check
  swap.rs     # transactional rebuild-from-state; host preserved
  engine.rs   # Engine: new / handle_pointer / handle_command / update (drain→tick)
  lib.rs
```

## Engine API in 3a

3a implements the input→**drain**→tick phases (Phase 2 §2); `render` is 3b.

```
Engine::new(host, registry, initial_skin) -> Result<Engine, EngineError>
Engine::handle_pointer(&mut self, p: Point, kind: PointerEvent)  // resolve hotspot → enqueue
Engine::handle_command(&mut self, cmd: Command)                  // swap / switch-host enqueue
Engine::update(&mut self, dt: Duration)                          // drain (&mut host) → tick
Engine::scene(&self) -> &Scene                                   // for tests + 3b's renderer
Engine::state(&self, key: &str) -> Option<StateValue>            // read-through to host
```

`update` is the single mutation point: it drains the command queue FIFO with exclusive
`&mut host` (host actions, swap, switch-host), then calls `host.tick(dt)`. No host
mutation happens during `handle_pointer` — handlers only enqueue. This makes the Phase 1
re-entrancy hazard structurally impossible.

## The vocabulary seam (the one real refinement over the prototype)

In the prototype, `fill`/`region`/`value_fill` were hardcoded Lua constructors. Here they
are **data-driven through the registry**:

- `vocab.rs`: `trait Primitive { fn id(&self) -> &str; fn build(&self, args: Table) ->
  Result<Node, BuildError>; }` and `VocabRegistry` holding `Vec<Box<dyn Primitive>>`.
- The engine populates the registry with the **stub base set** — `FillPrim`, `RegionPrim`,
  `ValueFillPrim` (the prototype's three, reimplemented as `Primitive` impls). 3a ships
  only these; Phase 5 adds the real set + host extensions.
- `script.rs`, on skin load, iterates the registry and installs **one Lua constructor per
  `primitive.id()`** into the sandbox `_ENV`, each shim calling `primitive.build(table)`
  and pushing the `Node` into the scene builder. The `host` table (allowlisted actions →
  command-enqueue shims) is added alongside (Phase 2 §3).
- The sandbox surface is therefore **`{ <registry primitive ids…>, host }`** — when Phase
  5 adds primitives or a host registers extensions, **no engine code changes**.

**`Node`** (engine-owned, domain-neutral) is what primitives produce: geometry (path) +
style (color) + optional **binding key** (value-driven nodes) + optional **hotspot handler
id**. 3a keeps the stub primitives' behavior identical to the prototype so the *seam* is
what's exercised, not new vocabulary. Lessons #2–#3 remain Phase 5 concerns.

## Command queue (Phase 2 §3, implemented)

`enum Command { HostAction { action, args }, Swap(SkinHandle), SwitchHost(HostHandle) }`.
Drained at `update`: **FIFO, every occurrence applied, no dedup; reads synchronous against
pre-drain state (no read-after-write within a handler); commands carry args; non-recursive
(commands may not enqueue commands).** A `SwitchHost` replaces the host; `HostAction`s
queued after it the same frame are validated against the new allowlist and dropped + logged
if absent. Per-action coalescing is **not** built (future opt-in).

## Error handling (Phase 2 §6, the headless subset)

- Manifest invalid / schema or engine-compat mismatch → `load` returns `Err`; no panic.
- Skin build error (bad Lua, unregistered action, unknown primitive id) → `Err`; **swap is
  transactional** — the prior scene stays active on a failed rebuild.
- Handler error at input resolution → caught; offending command(s) dropped + logged; frame
  continues.
- Engine code returns `Result` and does not panic on skin/host faults; `unwrap` only on
  genuine engine invariants.

## Testing

The engine ships **no** host, so tests use a **`FixtureHost`** (test-only, never shipped;
domain-neutral): a `toggle` action (flips a `Bool` `on`), a `bump(amount)` action (proves
args), and a `Scalar` `level` that advances on `tick`. No media/sysmon names.

Coverage, all headless (no GPU, no window):
- **Per module:** `state`; `scene` (cached-region hit resolution, concave-notch miss);
  `skin` (manifest parse/validate; schema + engine-compat rejection); `vocab` (each stub
  primitive `build`); `swap`.
- **Command-queue drain — highest value:** FIFO; every-occurrence/no-dedup; **no
  read-after-write within a handler**; commands-don't-enqueue-commands; post-`SwitchHost`
  invalid-action drop.
- **Sandbox negatives:** `io`/`os`/`require` absent; unregistered action errors; unknown
  primitive id errors.
- **Transactional swap:** a failing skin build leaves the prior scene intact.
- **Frame-loop integration (the 3a acceptance test):** drive `handle_pointer → update(dt)`
  against `FixtureHost`; assert resulting `state()` + `scene()`. End-to-end
  command→state→scene with no window.

## Definition of done (3a)

`crates/carapace` builds; the full headless suite passes; the `Engine` exposes
input→drain→tick + `scene()`/`state()` accessors ready for 3b's renderer to consume;
`hittest` and Phase 0/1 crates untouched.

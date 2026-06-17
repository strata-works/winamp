# Phase 2 — Formal Engine Architecture

**Date:** 2026-06-17
**Status:** Approved design; the architecture spec the Phase 3 core engine implements.
**Project:** carapace (repo codename `winamp`)
**Depends on / supersedes detail in:** `2026-06-17-skinning-engine-design.md` (roadmap, decisions 1–8),
informed by `2026-06-17-phase1-lessons.md`.

## Purpose & scope

Phase 2 formalizes the load-bearing contracts of the engine — the "spine" — so Phase 3
can implement a clean core and later phases build on stable interfaces. It is a
**document**, not code.

**In scope (formalized here):** the engine/host boundary, the frame loop + timestep, the
host capability boundary + command queue, and the `scene` / `state` / `swap` / `render` /
`skin` (artifact) contracts. The vocabulary/host-extension **seam** is fixed as an
interface; its concrete primitives are **out of scope** (Phase 5).

**Settled by prior work, assumed here:**
- Decisions 1–8 from the roadmap (coupled skin artifact; free-form not slot-based; live
  swap with state surviving; embedded Lua; host-extensible base vocabulary; desktop-first
  Rust + vello; prototype-first; capability sandbox).
- Phase 0: vello rendering backend; the `hittest` even-odd kernel.
- Phase 1 lessons, especially: render **direct-to-surface** (no offscreen readback);
  **wall-clock `dt`** (no fixed 1/60); cache hit-test regions; transactional/borrow-safe
  host boundary.

## 1. Engine boundary & module map

**The engine is a library; it does not own the window or the event loop.** The host
application owns the windowing system (e.g. winit), the GPU surface, and the frame loop.
It feeds the engine input + a wall-clock `dt`, and hands it a render target. This is what
makes the engine embeddable in arbitrary host projects, and it is what enables the Phase 1
performance fix: the host provides a GPU surface and the engine renders **directly to it**,
with no offscreen render-to-texture + readback (the Phase 1 dead end).

Public engine surface (the contract a host app codes against):

```
Engine::new(host: impl Host, initial_skin: SkinHandle) -> Result<Engine, EngineError>
Engine::handle_pointer(&mut self, p: Point, kind: PointerEvent)  // resolve hotspots → enqueue
Engine::handle_command(&mut self, cmd: Command)                  // meta commands (swap, switch)
Engine::update(&mut self, dt: Duration)                          // drain queue (&mut host) → tick
Engine::render(&mut self, target: &mut RenderTarget)             // draw scene → host surface
```

Module map (spine formalized now; `vocab` is interface-only):

| Module    | Formal responsibility | Depends on |
|-----------|----------------------|------------|
| `hittest` | Free-form geometry + point resolution (Phase 0, carried as-is; zero render deps). | — |
| `scene`   | Retained node graph; a **disposable projection** of state; caches each node's `hittest` region. | `hittest` |
| `state`   | Host-owned state store; the **only** source of truth. | — |
| `host`    | The capability trait: synchronous state reads + action allowlist + `tick`. | `state` |
| `script`  | Sandboxed Lua runtime + the **command queue** (the host boundary). | `host`, `scene`, `vocab` |
| `skin`    | Artifact format + loader (manifest + assets + entry script). | `scene`, `script` |
| `swap`    | Teardown + rebuild scene from state; host preserved. | `scene`, `state`, `skin` |
| `render`  | Draw a scene **directly to a host-provided surface**; paced by the host. | `scene` |
| `vocab`   | **(seam only)** trait boundary for base primitives + host extensions. | — |

Inversion from the Phase 1 prototype: the engine renders into a target the host owns,
rather than spinning its own window + readback loop.

## 2. Frame loop, timestep, and the command drain

The host owns the loop; the engine defines the **ordered phases** of a frame and where the
command queue drains. This ordering is the backbone that makes the model deterministic and
resolves the Phase 1 timestep/re-entrancy findings. Per frame, the host calls, in this
fixed order:

1. **Input.** Host forwards pointer/key events. `handle_pointer` resolves the point against
   the scene's hotspots (topmost wins, via cached `hittest` regions) and runs the matching
   skin handler, which **enqueues** commands. No host mutation yet. Meta-events (skin swap,
   host switch) enqueue engine commands via `handle_command`.
2. **Drain.** `update(dt)` applies the entire command queue **FIFO, with exclusive
   `&mut host`** (host actions mutate state; a swap rebuilds the scene from the now-current
   state; a host-switch replaces the host). The queue then resets to empty. This is the
   **single point** where host state changes — the re-entrancy hazard is structurally
   impossible because scripts ran only in phase 1 and only enqueued.
3. **Tick.** `host.tick(dt)` advances time-based state, with **`dt` a real `Duration` from
   the host's clock** (Phase 1 fix — never a hardcoded 1/60).
4. **Render.** `render(target)` reads current host state, projects the scene onto the
   surface, and draws **directly to the host's GPU surface**. Frame pacing (vsync / cap) is
   the host's choice, since it owns the surface.

Ordering decisions (load-bearing):
- **Drain before tick:** a click's command takes effect this frame, before time advances —
  responsive.
- **Swap drains in-band (phase 2), not deferred:** a skin swap is atomic within the frame
  that requested it; the next render shows the new skin with preserved state.
- **Commands may not enqueue commands.** Actions are pure state mutations + engine effects;
  the drain is a single bounded FIFO pass with no recursion.

## 3. The host boundary & command queue

The crux of the spec. Reads are synchronous and immutable; mutations occur only at the
drain.

**The `Host` trait:**

```
trait Host {
    fn name(&self) -> &str;
    fn tick(&mut self, dt: Duration);
    fn get(&self, key: &str) -> Option<StateValue>;       // synchronous read (bindings)
    fn actions(&self) -> &[ActionSpec];                   // allowlist: name + arg arity/types
    fn invoke(&mut self, action: &str, args: &[Value]);   // applied ONLY at drain, with &mut
}
```

`get` is the read side (bindings, evaluated in the render phase against current state).
`invoke` is the write side, reachable only through the queue.

**The command queue** — what a skin handler produces:

```
enum Command {
    HostAction { action: String, args: Vec<Value> },  // → host.invoke at drain
    Swap(SkinHandle),                                  // → swap module at drain
    SwitchHost(HostHandle),                            // → replace host at drain
}
```

Semantics (binding):
- **FIFO; every occurrence applied; no dedup.** `toggle` twice = two toggles; `stop` twice
  is idempotent by the action's own semantics. The queue faithfully replays what was issued.
- **No read-after-write within a handler.** All of a handler's commands are queued before
  any apply, so reads (bindings, `get`) observe **pre-drain** state. Skins express intent;
  imperative read-modify-write belongs to the host, not the skin.
- **Commands carry args** (`seek(0.5)`-style actions fit without a model change).
- **Per-action coalescing is deferred** (a future opt-in: a high-frequency action could
  declare "only the last this drain matters"). Not built now (YAGNI).
- **Non-recursive:** commands may not enqueue commands (see §2).

**The sandbox (decision 8, formalized):** the skin's Lua `_ENV` contains only the
vocabulary constructors + a `host` table whose fields are **exactly `host.actions()`**, each
a thin shim that pushes a `HostAction`. Calling an unregistered action is a load/run error.
`io`/`os`/`require` and base globals are absent (set via the chunk environment, as validated
in Phase 1). A script may *name* state keys (strings, for bindings) but cannot read or write
state directly: reads flow through the engine at render, writes through the queue.

**Cross-host validity:** a `SwitchHost` at drain replaces the host; any `HostAction` queued
*after* it in the same frame is validated against the **new** host's allowlist and dropped
(with a logged warning) if absent. This makes chaining across a switch safe rather than
undefined; it is discouraged in skins.

## 4. Scene, state, and swap contracts

Formalizes decision 3 (state outside the graph; scene disposable/rebuildable).

**`state`** — the single source of truth, owned by the host. The engine stores no
authoritative values elsewhere. `StateValue` is a closed scalar set — `Bool(bool)` and
`Scalar(f32)` in `0..=1` — widened later only if a concrete need appears. Reads are pure;
the only writer is `host.invoke` at the drain.

**`scene`** — a flat list of retained nodes, each a **pure projection** of state. A node
holds geometry + style + (for value-driven nodes) a **binding key string**, never a resolved
value. Invariant the spec asserts: *the scene is a deterministic function of (skin, state)* —
rebuilding from the same skin + state yields an equivalent scene. Each hotspot node caches
its `hittest::Region` at build time (Phase 1 lesson: do not rebuild per click). Nodes are
addressed by handle; handler ids are scene-local.

**`swap`** — `swap(skin)` runs at the drain point: drop the current scene entirely, run the
new skin's entry script against the **unchanged** host to build a fresh scene, re-establish
bindings. The host/state instance is untouched, so bound values carry across — the
state-survives-swap guarantee is now a **spec invariant**, not an emergent property.

**Host switch** is the distinct operation: replace the host instance (fresh state) and load
its default skin. The asymmetry is explicit and load-bearing: **skin swap preserves state;
host switch resets it.**

## 5. Skin artifact format & the vocabulary seam

**Artifact** — a directory or zip:

```
<skin>/
  skin.toml     # manifest (declarative, no logic)
  skin.lua      # entry script
  assets/       # optional: bitmaps, vector/path data
```

`skin.toml`:

```toml
schema = 1                    # artifact schema version — engine rejects unknown majors
id = "media-classic"
name = "Media Classic"
engine = "^0.1"               # engine semver the skin targets
canvas = { width = 300, height = 120 }
entry = "skin.lua"
# assets = [...]              # optional; declared for validation
```

Two fields Phase 1 motivated: **`schema`** (the loader can reject/migrate future formats)
and **`engine`** compat range (a skin built for a later vocabulary fails loudly, not
mysteriously). The loader validates the manifest, checks compat, then hands `entry` + the
asset table to `script`. **Asset paths are sandboxed to the artifact** — traversal outside
the skin directory is rejected.

**The vocabulary seam (interface only — contents are Phase 5).** Phase 2 fixes the *shape*
of the boundary so Phase 3 builds against it and Phase 5 fills it:

```
trait Primitive {                                   // a vocabulary entry skins can construct
    fn id(&self) -> &str;                            // e.g. "fill", "region", "value_fill"
    fn build(&self, args: Table) -> Result<Node, BuildError>;   // table → scene node
}
trait VocabRegistry {                               // what a host extends
    fn register(&mut self, prim: Box<dyn Primitive>);
}
```

The engine ships the base set behind this trait; a host registers domain primitives (e.g. a
media host's "visualizer") and they appear in the skin env exactly like built-ins. **Phase 2
commits only these two traits.** The concrete base primitives, their arg schemas, and the
media/sysmon extensions are Phase 5. Phase 1 lessons #2–#3 (richer value-fill with explicit
fill direction / actual-region fill; shared draw+hotspot geometry so a clickable visible
control declares geometry once) are recorded here as **requirements on Phase 5's
primitives**, not resolved now.

## 6. Error handling & testing

**Error handling** — classified by *when* a failure occurs and the rule that the engine,
embedded in someone else's app, must never take the host process down on skin/host/GPU
faults:

| Failure | Policy |
|---------|--------|
| Manifest invalid / schema or engine-compat mismatch | Load-time, recoverable. `load_skin` returns `Err`; host decides (keep current, fallback). No panic. |
| Skin script error at build (bad Lua; unregistered action; unknown primitive) | Load-time, recoverable. **Swap is transactional** — the old scene stays active if the new skin fails to build. A broken skin never blanks the window. |
| Skin handler error at input resolution | Caught; offending command(s) dropped with a logged diagnostic; the frame continues. |
| Command targets an invalid action post-switch | Dropped + warned (§3). |
| Render / GPU error (device loss, etc.) | Surfaced to the host (it owns the surface); no `unwrap`-panic. |

Principle: skin artifacts are untrusted-ish input (decision 8's framing); engine code returns
`Result` and does not panic on skin/host/GPU faults. `unwrap` is allowed only on genuine
engine invariants.

**Testing strategy** — what Phase 3 must be able to test, by module:
- `hittest`, `scene`, `state`, `swap`, `skin` (manifest parse/validate), and the **command
  queue drain semantics** (FIFO, no-dedup, no read-after-write, post-switch drop) — all
  **headless unit tests**, no GPU. The drain-ordering tests are the highest-value: they lock
  the §2/§3 contracts.
- `script` sandbox — negative tests (globals absent; unregistered action errors), as Phase 1.
- **Transactional swap** — a test that a failing skin build leaves the prior scene intact.
- `render` — requires a GPU; gated/separated so the rest of the suite runs GPU-less
  (Phase 1 lesson #6). A software-adapter fallback is the recommended CI path.
- **Frame-loop integration** — a headless harness driving `handle_pointer → update → tick`
  (skipping `render`) to assert end-to-end command → state → scene behavior without a window.

## Out of scope (deferred)

- Concrete base vocabulary primitives, their arg schemas, and host extensions → **Phase 5**.
- Richer value-fill semantics and shared draw+hotspot geometry → **Phase 5** (requirements
  recorded in §5).
- Asset/bitmap decoding and vector-path import formats beyond the manifest's declared list.
- Web portability (non-binding stretch goal per decision 6).
- Per-action command coalescing (future opt-in, §3).

## What Phase 3 builds

The core engine implementing §1–§4 and §6: `scene`, `state`, `host`, `script` (+ command
queue), `skin` (loader), `swap`, `render` (direct-to-surface), wired through the `Engine`
frame-loop API — driven from Rust/tests, against the `vocab` seam (§5) with a minimal stub
primitive set, before Phase 5 fills the real vocabulary.

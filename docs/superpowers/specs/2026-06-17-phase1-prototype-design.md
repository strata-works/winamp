# Phase 1 — Throwaway Prototype — Design

**Date:** 2026-06-17
**Status:** Approved design, pre-implementation
**Project:** carapace (repo codename `winamp`)
**Depends on:** Phase 0 (`hittest` kernel, vello rendering decision)
**Source:** roadmap in `2026-06-17-skinning-engine-design.md` (Phase 1) + decision 7

## Purpose

Build a **throwaway minimal slice** to surface real problems before formalizing the
engine (decision 7). It exists to teach, not to be reused. It must exercise the three
risk areas the design names:

1. **Free-form hit-testing** — clicks resolved against arbitrary skin-defined geometry.
2. **The Lua ↔ host call boundary** — skins bind to host actions/state through a
   sandboxed scripting context (decision 8's allowlist, in miniature).
3. **State-survives-swap** — hot-swap a skin mid-activity with no loss of host state.

A **lessons-learned note** is a named output of this phase; it feeds the Phase 2 spec.

Everything not in service of those three risks is kept deliberately scrappy.

## Scope decisions (settled in brainstorming)

- **Real `mlua`.** Skins are real Lua scripts; the script↔host boundary is exercised for
  real (not faked with Rust closures).
- **Real `skin.toml`.** A skin is an on-disk directory (`skin.toml` + `skin.lua`), so
  loading/parsing and swapping are genuine. Manifest is minimal: `id`, `name`, canvas
  `width`/`height`, `entry` (the lua file). No asset list (the prototype draws shapes
  from geometry, ships no bitmaps).
- **No text rendering.** Shapes and colors only — no glyph/font work (a Phase 3 concern).
  State is shown via color and a proportional fill bar.
- **Two hosts, two skins each (4 tiny skins).** Proves the three risks *and*
  host-agnosticism (the engine carries zero domain knowledge) — the latter was a hard
  requirement (Phase 0, Q4).

## Architecture

A new **throwaway** crate `crates/proto`, marked disposable. Small, single-purpose
modules; reuses what Phase 0 validated.

```
crates/proto/
  Cargo.toml
  skins/
    media-classic/{skin.toml, skin.lua}    media-minimal/{skin.toml, skin.lua}
    sysmon-bars/{skin.toml, skin.lua}       sysmon-dial/{skin.toml, skin.lua}
  src/
    scene.rs       # node types built by Lua
    host.rs        # Host trait + capability registry; MediaHost, SysmonHost; tick(dt)
    lua_bridge.rs  # mlua runtime + capability sandbox; runs skin.lua once -> Scene
    skin.rs        # load a skin dir: parse skin.toml + read skin.lua
    swap.rs        # holds current Host (state) + current Scene; swap rebuilds Scene
    app.rs         # winit + vello frame loop: tick, render, click -> hittest -> action
    main.rs
```

### Reuse from Phase 0

- **`hittest`** — used directly. A hotspot's path becomes a `hittest::Region`; clicks
  resolve via `Region::contains`. This is the free-form hit-testing, now driven by
  skin-defined geometry.
- **vello** — the *approach* carries, not `spike-render`'s single-region `Renderer`. The
  prototype renders a **multi-node scene** (many shapes, colors, a clipped progress
  fill), so it builds a `vello::Scene` directly. Expected: Phase 0's renderer was a
  spike; this is the first real multi-shape render.

### The capability sandbox (decision 8, concretely)

`lua_bridge` hands `skin.lua` an environment table containing **only**:

- the three scene constructors (`fill`, `region`, `value_fill`), and
- a `host` table whose fields are **exactly** the actions the current host registered.

No `io`, `os`, `require`, or other globals. A skin can reach nothing the host did not
deliberately expose.

## The Lua vocabulary

Three primitives, covering all three risk areas:

| Lua primitive | Purpose | Risk area |
|---|---|---|
| `fill{ path, color }` | a static filled shape | appearance (baseline) |
| `region{ path, on_press = fn }` | a free-form hotspot with a press handler that calls a host action | hit-testing + script↔host boundary |
| `value_fill{ path, value = "position", color }` | a region whose fill **extent** is driven by a named host value | state display + survives-swap |

- `path` is a list of `{x, y}` points (one contour; closed implicitly). For the
  prototype, `value_fill` paths are treated as axis-aligned bars: the rendered fill
  spans `x_min .. x_min + value × (x_max − x_min)`.
- `color` is `{r, g, b}` (0–255), alpha implied opaque.
- `on_press` is a Lua function; the engine stores it by id and calls it on a hit.

### Update model

- `skin.lua` runs **once** per load/swap — it builds the scene and declares bindings. It
  does **not** run per frame.
- Each frame the engine reads the host state that `value_fill` nodes are bound to and
  updates visuals. **Scene nodes hold binding keys (`"position"`), never values** — the
  scene is a pure projection of host state (design decision 3).

## Host capability model

A `Host` exposes a generic capability surface; the **engine knows none of the names**:

- **State:** named scalar values readable/bindable by skins. Each is a `bool` or an
  `f32` in `0..=1`.
- **Actions:** named zero-arg callables (the allowlist surfaced into the Lua `host`
  table).
- **`tick(dt)`:** advances the host's own state over time.

Two implementations:

**MediaHost**
- State: `playing` (bool), `position` (f32 0→1, advances while `playing`).
- Actions: `toggle_play()`, `stop()` (sets `position = 0`, `playing = false`).

**SysmonHost**
- State: `cpu` (f32 0→1, drifts over time while sampling), `sampling` (bool).
- Actions: `toggle_sampling()`.

The capability registry mechanism (named state reads + named action calls = the Lua
env) is **identical** across both hosts — that is the host-agnosticism proof.

## Data flow

**Load / swap a skin:**
1. `skin.rs` reads the dir → parses `skin.toml` → reads `skin.lua`.
2. `lua_bridge` builds the sandboxed env (3 constructors + the current host's actions),
   runs `skin.lua` once; each constructor call pushes a node into a fresh `Scene`;
   `on_press` handlers stored as Lua refs keyed by id.
3. Result: a `Scene` + press-handler table. **Host state untouched.**

**Per-frame loop (`app.rs`):**
1. `host.tick(dt)` advances host state.
2. Walk the `Scene` → build a `vello::Scene`: `fill`/`region` draw their path in their
   color; `value_fill` reads its bound host value now and fills its bar to
   `value × width` (clip). Present via the vello surface.

**Click:**
1. Cursor → region space → test against each `Hotspot`'s `hittest::Region`; topmost hit
   wins.
2. Invoke that hotspot's stored Lua handler → it calls an allowlisted `host.*` action →
   host mutates its own state → reflected next frame.

**State-survives-swap:** while `position` advances, `Tab` triggers a swap. `swap.rs`
drops the old `Scene`, re-runs the new skin's `skin.lua` (rebuilding nodes + bindings),
leaves the host untouched. The new skin's progress bar shows the same advancing
`position` — visibly continuous.

## Interaction & driving

A `winit` window with a vello surface (same presenter style as the Phase 0 viewer).

- **Left-click** → hit-test current skin's hotspots → fire the bound host action. The
  app has no hardcoded buttons; the skin decides what is clickable.
- **`Tab`** → cycle the **skin** within the current host (A ↔ B). The state-survives-swap
  trigger.
- **`H`** → switch the **host** (media ↔ sysmon), loading that host's first skin.
  Switching host = a fresh host instance (own state); swapping *skins* preserves state.
  The distinction is deliberate and visible.
- **`Space` is not special** — pausing is a skin hotspot bound to `toggle_play`, proving
  control lives in the skin, not the app.
- On each swap/host-switch, print a status line to **stdout**
  (`[media-classic] position=0.42 playing=true`) so state continuity is legible in the
  terminal as well as on screen.

Host `tick` uses the frame loop's delta time.

## Testing

Same split as Phase 0: TDD the logic; verify the visual/interactive layer by running the
app.

**Automated (no GUI):**
- **`host`** — registering named state/actions; `tick(dt)` advances `position`/`cpu`;
  an action mutates only its host's state.
- **`lua_bridge`** (the high-value tests — this is the script↔host boundary + sandbox):
  - run a small `skin.lua` string → assert it produces the expected `Scene` nodes
    (a `region`, a `value_fill` with the right binding key).
  - **sandbox negative tests:** `io`, `os`, `require` are `nil` in the skin env; calling
    a host action the host did not register raises a Lua error.
- **`swap`** — load skin A, `tick` so `position` advances, swap to skin B, assert host
  state is identical and the `Scene` was rebuilt from it. Headless.
- **hotspot resolution** — a concave skin-defined path → `hittest::Region`: a point in
  the notch resolves to no hotspot; a point inside fires the right handler.

**Manual (run the app):** render fidelity; live progress-survives-swap across `Tab`;
host switch via `H`; click→action. Verified by launching and screenshotting.

## Output: lessons-learned note

A short note (e.g. `docs/superpowers/specs/2026-06-17-phase1-lessons.md`) capturing the
real problems the prototype surfaced in the three risk areas, written after the prototype
runs. This is the bridge to the Phase 2 formal spec. The prototype code is **not**
expected to carry forward.

## Out of scope (deliberately)

- The formal skin artifact spec, asset/bitmap loading, vector path import formats.
- Text/glyph rendering.
- The full base-vocabulary API and host-extension registration mechanism (Phase 5).
- Production module boundaries (Phase 3). The prototype's modules are scrappy on purpose.

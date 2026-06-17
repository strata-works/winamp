# Skinning Engine — Design & Roadmap

**Date:** 2026-06-17
**Status:** Approved design, pre-implementation
**Source:** `skinning-engine-decisions.md` (decisions log) + brainstorming session 2026-06-17

## Purpose

A general-purpose, **host-agnostic** skinnable UI engine, inspired by the *concept*
of WMP-style skins (total window replacement, free-form hotspots, live swapping) —
not tied to media players. Other projects embed it and define their own skinnable
surface. The engine carries **zero domain-specific knowledge**; anything
media-flavored (transport, audio visualizer) enters only as a host extension.

This document is the **roadmap-level design**. The detailed per-module engine spec
is itself a deliverable of Phase 2, written only after the throwaway prototype
(Phase 1) has surfaced real problems.

## Decisions carried in from the log

These are settled and constrain everything below:

1. **Skin scope:** authentic WMP-style coupled artifact — layout + appearance +
   hotspot behavior ship together as one unit, not three separable layers.
2. **Layout model:** free-form, not slot-based. Skins define their own canvas and
   arbitrary-shaped hotspots (modernized: vector paths/SDF, not WMP bitmap masks).
   The engine cannot rely on native widget layout; it needs its own retained-mode
   scene graph and its own hit-testing.
3. **Runtime swapping:** live hot-swap with **no loss of app state**. Application
   state lives entirely **outside** the scene graph. The scene graph is
   disposable/rebuildable from state — never the reverse.
4. **Host binding model:** embedded scripting via **Lua (`mlua`)** — chosen over
   JS. Not a typed capability schema, not stringly-typed dynamic binding.
5. **Base vocabulary:** ships with a base set, host-extensible. **Reframed (see
   below):** the base set is domain-neutral primitives only.
6. **Target runtime:** desktop-first via Tauri/Rust, consistent with the existing
   shader-explorer stack. Web portability is a stretch goal, not a constraint.
7. **Process:** prototype before formalizing. Throwaway minimal slice first.
8. **Script sandboxing:** capability sandbox. The script gets no raw host/io/os
   access; the host exposes only an explicit allowlisted API surface (base
   vocabulary + host extensions). This is a guardrail independent of author trust
   (bugs, compromised upstreams, supply-chain), not a hedge against untrusted
   authors. Audience is developers embedding the engine, not end users installing
   arbitrary downloaded skins.

### Reframing of the base vocabulary (decision 5)

The log listed "transport controls, sliders, labels, visualizer slots" as the base
vocabulary. But "transport controls" and "visualizer slots" are **media-specific**.
Since the engine has **no media-specific knowledge**, those cannot be base
vocabulary — they are **host extensions** registered by a media-player host.

- **Base vocabulary (engine-owned, domain-neutral):** button, scalar/slider
  control, text/label, image, generic **region** (the free-form hotspot), generic
  **custom draw-slot**, and a generic **value binding** (named host state a control
  reads/writes).
- **Host extensions (registered by the host):** a media player registers
  "transport control", "visualizer-as-audio", "seek bar as time"; a system-monitor
  host registers "gauge", "meter". Their *meaning* comes from the host; the engine
  only sees primitives.

The two prototype host kinds (media player + system monitor) are the proof that the
**same engine, with zero built-in domain concepts**, supports both purely through
the host-extension mechanism.

## Module architecture

The engine is a set of small, independently-testable modules. The critical boundary:
**hotspot geometry/hit-testing is its own module that knows nothing about
rendering**, and **rendering is a swappable backend behind a trait**.

```
┌─────────────────────────────────────────────────────────┐
│ HOST (the embedder — fake media player / fake sysmon)     │
│  • registers vocabulary extensions (decision 5)           │
│  • owns application state                                 │
│  • exposes allowlisted actions/state = the capability set │
└───────────────▲───────────────────────────┬──────────────┘
                │ host binding API           │ state reads
   ┌────────────┴────────────┐               ▼
   │ Scripting host (mlua)    │      ┌──────────────────┐
   │ • capability sandbox     │◄────►│ State store       │
   │ • env = base vocab +     │      │ (lives OUTSIDE    │
   │   host extensions only   │      │  scene graph)     │
   └────────────┬────────────┘       └────────┬─────────┘
                │ builds/binds                 │ rebuild source
                ▼                              ▼
   ┌─────────────────────────┐       ┌──────────────────┐
   │ Scene graph (retained)   │◄──────│ Swap controller   │
   │ • DISPOSABLE/rebuildable │       │ teardown + rebuild│
   │ • nodes: region, control,│       │ from state        │
   │   text, image, draw-slot │       └──────────────────┘
   └──────┬───────────┬───────┘
          │           │
          ▼           ▼
 ┌────────────────┐ ┌──────────────────────┐
 │ Hit-test module│ │ Render backend (trait)│
 │ free-form geo  │ │ wgpu | vello |        │
 │ (paths/SDF)    │ │ tiny-skia (TBD spike) │
 │ NO render dep  │ └──────────────────────┘
 └────────────────┘
```

| Module     | Responsibility                                                                                   | Depends on |
|------------|--------------------------------------------------------------------------------------------------|------------|
| `scene`    | Retained-mode scene graph; disposable, always rebuilt from state, never a source of truth (d3).  | —          |
| `hittest`  | Free-form hotspot geometry + hit resolution (vector paths / SDF). **No rendering dependency.**   | `scene`    |
| `render`   | Trait + chosen backend; draws a scene. Swappable.                                                | `scene`    |
| `skin`     | Skin artifact format + loader (manifest + assets + Lua).                                         | `scene`, `script` |
| `script`   | `mlua` runtime + capability sandbox (d8): the script's environment *is* the allowlist.           | `vocab`, `state` |
| `vocab`    | Domain-neutral base vocabulary primitives + host-extension registration mechanism (d5).          | —          |
| `state`    | Host-owned state store, external to the scene graph.                                             | —          |
| `swap`     | Hot-swap controller: tear down scene graph, rebuild from new skin + current state (d3).          | `scene`, `state`, `skin` |

Each unit answers: *what does it do, how do you use it, what does it depend on?* —
and can be tested independently. `hittest` and `render` both consume `scene` but
not each other, keeping the rendering backend a swappable detail.

## Skin artifact format

A skin is a **directory (or zip)** with three parts, mirroring decision 1 (layout +
appearance + behavior as one coupled artifact):

```
my-skin/
  skin.toml          # manifest: id, name, engine-version, canvas size, asset + entry refs
  assets/            # appearance: bitmaps, vector path defs for regions/hotspots
    bg.png
    play.svg
    knob.png
  skin.lua           # behavior + layout: builds the scene graph, defines hotspot
                     # geometry, binds controls to host capabilities
```

- **`skin.toml`** — declarative metadata only (no logic): identity, target engine
  version, canvas dimensions, declared asset list, entry script. Cheap to validate
  on load.
- **`assets/`** — the appearance layer: bitmaps and vector/path definitions for
  free-form hotspot geometry (d2).
- **`skin.lua`** — runs inside the sandbox (d8). Constructs layout (places
  regions/controls on the canvas), attaches hotspot geometry, and binds each
  control to host capabilities (e.g. `onPress → host.transport.toggle()` for the
  media host; `host.metrics.cpu` for the sysmon host). It can name **only** what
  the engine + host put in its environment.

**Coupling check (d1):** all three travel together as one unit; the host never
dictates layout. ✓
**Hot-swap implication (d3):** on swap, the old `skin.lua`-built scene graph is torn
down entirely and the new skin's `skin.lua` runs fresh, rebuilding from current host
state. State never lives in the graph. ✓

## Roadmap

Phases **0 and 1 are throwaway**. Phases 2–6 are the real engine.

### Phase 0 — Rendering/hit-test spike (de-risk the one open decision)
Throwaway. One irregular hotspot (concave — e.g. L-shaped or a ring) rendered + hit-
tested under each candidate: raw `wgpu`, `vello`, `tiny-skia`. **Success criterion:**
clean per-path/per-pixel hit resolution on the concave shape, with the `hittest`
module already decoupled from the backend.
**Output:** committed rendering backend + a proven `hittest`↔`render` boundary.
Everything downstream depends on this.

### Phase 1 — Throwaway prototype (decision 7)
Fake host + two fake skins, built on the Phase-0 winner. Deliberately scrappy code,
meant to be discarded. Surfaces real problems in the three risk areas the log names:
- free-form **hit-testing** against real irregular hotspots,
- the **script ↔ host call boundary** (Lua calling allowlisted host actions),
- **state-survives-swap** (swap skin mid-"playback", state intact).

**Output:** a written "lessons learned" note feeding the spec. No reuse expectation.

### Phase 2 — Formalize the spec
With prototype problems surfaced, write the real architecture spec for the 8 modules,
the skin artifact format, and the host-extension API. (Distinct from this roadmap
doc.)

### Phase 3 — Core engine, built clean & modular
`scene`, `hittest`, `render`, `state`, `swap` as real tested modules. No scripting,
no Lua yet — drive from Rust directly to validate the retained-graph + rebuild-from-
state mechanics in isolation.

### Phase 4 — Scripting + capability sandbox
`mlua` integration; `script` module; the sandbox where the Lua environment table *is*
the allowlist (d8). `skin` loader parses manifest + assets + Lua.

### Phase 5 — Base vocabulary + host-extension mechanism
`vocab`: domain-neutral primitives (button, scalar/slider, text, image, region,
custom draw-slot, value-binding) + the registration API hosts use to add their own
concepts.

### Phase 6 — Validation against both host kinds
Reuse the *concepts* from Phase 1 against the real engine: a **media-player host** and
a **non-media host (system monitor)**, two skins each — proving the engine carries
zero media knowledge; every media concept enters via host extension only.

## Sequencing rationale

- **Scripting (Phase 4) comes after the core engine (Phase 3)** so the hard
  graph/swap mechanics are proven before Lua is layered on, even though the prototype
  already touched Lua.
- **Only Phases 0 and 1 are throwaway.** The lessons-learned note is the bridge from
  throwaway work to the real spec.

## Open items deferred by design

- **Exact rendering backend** — resolved by the Phase 0 spike, not pre-committed.
- **Detailed per-module API surfaces** — resolved by the Phase 2 spec, informed by
  Phase 1 lessons.

# Skinnable UI Engine (working name)

A general-purpose, **host-agnostic** skinnable UI engine for desktop apps — written in
Rust. It lets an application hand its entire surface over to a *skin* that defines its
own layout, appearance, and interactive hotspots, and lets users hot-swap skins at
runtime without losing app state.

> **Status: early spike.** Only Phase 0 is built — a hit-testing kernel and a rendering-
> backend bake-off (see [Current status](#current-status)). The full engine does not
> exist yet. This repo is being built phase by phase against a written design.

## Motivation

The inspiration is the *concept* of a Windows Media Player / Winamp–style skin: a **total
window replacement** with free-form, bitmap/region-based hotspots that could be swapped
live, mid-playback, without interrupting the app. That model made those skins
distinctive — a skin wasn't a theme layered over a fixed app chrome; it *was* the
interface.

Almost every modern "theming" system is the safer, weaker version of this: a stylesheet
or a set of named slots the app still owns and lays out. That's deliberately **not** what
this is. The goal here is to rebuild the powerful version of the idea — total reskin,
arbitrary geometry, live swap — as a **reusable engine** that any project can embed and
point at its own state and actions. It is **not** tied to media players; a skin for a
media player and a skin for a system monitor must run on the exact same engine, because
the engine carries **zero** domain-specific knowledge.

## Design at a glance

The full design and rationale live in
[`docs/superpowers/specs/2026-06-17-skinning-engine-design.md`](docs/superpowers/specs/2026-06-17-skinning-engine-design.md).
The load-bearing decisions:

- **Coupled artifact.** A skin replaces layout + appearance + hotspot behavior together
  as one unit — the "total reskin" model, not a restyling layer.
- **Free-form, not slot-based.** Skins define their own canvas and arbitrary-shaped
  hotspots (vector paths). The engine therefore owns its own retained-mode scene graph
  and its own hit-testing, independent of any native widget layout.
- **Live swap, state survives.** Skins hot-swap while running with no loss of app state.
  Application state lives entirely **outside** the scene graph; the scene graph is
  disposable and always rebuilt from state — never the reverse.
- **Embedded Lua scripting in a capability sandbox.** Skins bind to host actions/state
  through a Lua script that can only reach an explicit allowlisted API surface (the base
  vocabulary plus host extensions). No raw host/io/os access.
- **Domain-neutral base vocabulary, host-extensible.** The engine ships only generic
  primitives (button, slider, text, image, region, custom draw-slot, value-binding).
  Anything domain-flavored — "transport control", "audio visualizer" — is registered by
  the host as an extension.
- **Desktop-first, Rust + vello.** Targets desktop (consistent with an existing
  shader/GPU stack). The rendering backend is **vello** (chosen in Phase 0); web
  portability is a non-binding stretch goal.

## Current status

**Phase 0 — rendering / hit-test spike: complete.** Phase 0 de-risked the one open
design decision (the 2D rendering backend) by building three candidates behind a common
trait and holding each to an objective gate.

What exists today:

| Crate | What it is |
|-------|------------|
| [`crates/hittest`](crates/hittest) | Dependency-free even-odd point-in-region kernel for concave + holed shapes. The decoupled hit-testing module; has **no** rendering/GPU dependency. |
| [`crates/spike-render`](crates/spike-render) | A `Renderer` trait, an RGBA8 `Pixmap`, and a `parity_check` gate that asserts every unambiguous pixel a backend fills agrees with `hittest`'s independent verdict. Contains the chosen **vello** backend and a live viewer. |

**Decision: vello.** All three candidates (`tiny-skia`, `wgpu`+`lyon`, `vello`) passed the
correctness gate — free-form hit-testing is satisfied by the decoupled `hittest` module
regardless of backend. vello was chosen for fit with the GPU/shader direction: it renders
vector paths directly (no tessellator) while leaving raw `wgpu` available underneath for
visualizer shaders. Full rationale:
[`docs/superpowers/specs/2026-06-17-phase0-backend-decision.md`](docs/superpowers/specs/2026-06-17-phase0-backend-decision.md).

## Building & running

Requires a recent Rust toolchain (edition 2024; built against Rust 1.95). On macOS the
GPU paths use Metal.

```sh
# Run the full test suite (hit-test kernel + the parity gate against vello)
cargo test

# Launch the live viewer: shows a shape, click to test inside/outside,
# Space toggles the concave L-shape / holed ring, Esc quits.
cargo run -p spike-render --example viewer
```

In the viewer the shape starts gray; a left-click turns it **green** if the click landed
inside the shape and **red** if it missed — the free-form hit-testing, live.

## Roadmap

Phases **0–1 are throwaway** (spikes/prototype); **2–6** build the real engine.

- **Phase 0 — rendering / hit-test spike.** ✅ Done. Backend chosen: vello.
- **Phase 1 — throwaway prototype.** Fake media-player and system-monitor hosts, two
  skins each, exercising hit-testing, the Lua↔host boundary, and state-survives-swap.
- **Phase 2 — formalize the spec** from what the prototype surfaces.
- **Phase 3 — core engine** (scene graph, hit-testing, render, state, swap), driven from
  Rust before scripting is layered on.
- **Phase 4 — scripting + capability sandbox** (`mlua`, skin artifact loader).
- **Phase 5 — base vocabulary + host-extension mechanism.**
- **Phase 6 — validation** against both host kinds, proving zero media-specific
  knowledge in the engine.

## Repository layout

```
crates/hittest/         # decoupled hit-testing kernel (no render deps)
crates/spike-render/     # Renderer trait + parity gate + vello backend + viewer example
docs/superpowers/specs/  # design doc + backend decision
docs/superpowers/plans/  # phase implementation plans
```

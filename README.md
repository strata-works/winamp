# carapace

> A swappable shell over any host. (Repo codename: `winamp`.)

A general-purpose, **host-agnostic** skinnable UI engine for desktop apps — written in
Rust. It lets an application hand its entire surface over to a *skin* that defines its
own layout, appearance, and interactive hotspots, and lets users hot-swap skins at
runtime without losing app state.

> **Status: working engine, built phase by phase — Phases 0–6 complete, plus the live
> host-view region (`view{}` primitive).** The demo is a borderless, transparent, draggable
> window where the skin *is* the interface — vector skins self-shape into rounded silhouettes
> (the Headspace bitmap floats as a borderless rectangle); the **H** key live-switches the whole
> window between a media player and a real `sysinfo` system monitor on one engine, proving total
> window replacement and zero domain knowledge (the only engine change was a transparent render
> base color); and the Headspace skin hosts a live CPU / MEM / SWP system monitor painted into a
> declared `view{}` region each frame. See [Current status](#current-status).

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
[`docs/superpowers/specs/2026-06-17-skinning-engine-design.md`](docs/superpowers/specs/2026-06-17-skinning-engine-design.md)
and the engine architecture in
[`docs/superpowers/specs/2026-06-17-phase2-engine-architecture.md`](docs/superpowers/specs/2026-06-17-phase2-engine-architecture.md).
The load-bearing decisions:

- **Coupled artifact.** A skin replaces layout + appearance + hotspot behavior together
  as one unit — the "total reskin" model, not a restyling layer.
- **Free-form, not slot-based.** Skins define their own canvas and arbitrary-shaped
  hotspots (vector paths). The engine owns its own retained-mode scene graph and its own
  hit-testing, independent of any native widget layout.
- **Live swap, state survives.** Skins hot-swap while running with no loss of app state.
  Application state lives entirely **outside** the scene graph; the scene is a pure
  projection of state (it binds *keys*, never values) and is always rebuilt — never the
  reverse. Swap is transactional: a skin that fails to load leaves the prior one running.
- **Embedded Lua scripting in a capability sandbox.** Skins bind to host actions/state
  through a Lua script whose `_ENV` is *only* the vocabulary constructors plus an
  allowlisted set of host actions — no raw `host`/`io`/`os`/filesystem access.
- **Domain-neutral base vocabulary, host-extensible.** The engine ships six generic
  base primitives: `fill` (background), `region` hotspots, value-bound `value_fill`,
  `image`, `text` — laid-out, value-bindable, `Paint`-filled — and `view` (live host-content
  region; see below). Anything domain-flavored — "transport control", "audio visualizer" — is
  registered by the host as an extension.
  A host registers its own domain primitives through `VocabRegistry::register`; they appear in the
  skin env exactly like built-ins and can bind the host's allowlisted actions directly (e.g. the
  demo's `transport{}`). `carapace` re-exports `mlua` so an extension crate needs no direct `mlua`
  dependency.
  Shapes (`circle`/`rect`/`rounded_rect`) are composable path-helpers, and any drawable can take
  an `on_press` to be both drawn and hit-testable from one declaration.
- **Desktop-first, Rust + vello.** The host owns the window, event loop, and surface; the
  engine renders **direct-to-surface** (no readback) on a wall-clock delta. The 2D backend
  is **vello** (chosen in Phase 0) over `wgpu`, leaving raw `wgpu` available underneath for
  future visualizer shaders.

## Current status

The engine works end-to-end. Run `cargo run -p carapace-demo` and a borderless, draggable GPU
window opens running a skin (vector skins self-shape into rounded silhouettes; the Headspace
bitmap floats as a rectangle); **Tab** cycles through the bundled skins, **H**
live-switches the whole window between the media-player host and a real `sysinfo` system
monitor, clicks hit free-form hotspots, a value-bound seek bar advances on wall-clock time,
and a skin swap preserves playback state.

What exists today:

| Crate | What it is |
|-------|------------|
| [`crates/hittest`](crates/hittest) | Dependency-free even-odd point-in-region kernel for concave + holed shapes. The decoupled hit-testing module; **no** rendering/GPU dependency. |
| [`crates/carapace`](crates/carapace) | The engine: scene graph + hit-testing, host command queue, external state + value bindings, transactional skin swap, Lua scripting in a capability sandbox, the base vocabulary, sandboxed asset loading + image decode, and a vello/`wgpu` direct-to-surface renderer. |
| [`crates/carapace-demo`](crates/carapace-demo) | A borderless dual-domain embedder (`winit` + `wgpu`): a media-player host and a real `sysinfo` system-monitor host share one engine; **H** live-switches between them. Three bundled skins, including the real Headspace bitmap. |

**Phase 5a — asset loading + the `image` primitive: complete.** A type-agnostic,
sandboxed `AssetResolver` scans a skin's `assets/` directory (Flutter-style: resolved =
usable; `..` and symlinks can't escape the skin dir) and decodes PNG/JPEG/GIF/BMP to sRGB
RGBA8 on first use. The new `image{ asset = "…", x, y }` primitive draws that bitmap
through vello. The demo `reference` skin is the genuine `headspace.png` faceplate with two
invisible hotspots (play/stop) and a live seek bar layered on top — exactly how real WMP
skins are built. See
[`docs/superpowers/specs/2026-06-18-phase5a-asset-loading-design.md`](docs/superpowers/specs/2026-06-18-phase5a-asset-loading-design.md).

## A skin, end to end

A skin is a directory: a `skin.toml` manifest (canvas size, entry script, asset dir) plus
a Lua entry script that calls vocabulary constructors. The demo's `reference` skin:

```lua
image{ asset = "headspace.png", x = 0, y = 0 }                       -- the bitmap faceplate
region{ path = { ... play button ... }, on_press = function() host.toggle_play() end }
region{ path = { ... stop button ... }, on_press = function() host.stop() end }
value_fill{ path = { ... seek bar ... }, value = "position",         -- bound to host state
            color = { r = 120, g = 230, b = 80 } }
```

The bitmap supplies the look; Lua supplies placement and interactivity; the host supplies
state (`position`) and actions (`toggle_play`, `stop`). The engine knows nothing about
"playback" — those are just an allowlisted action name and a bound state key.

### The `view{}` primitive — live host-content region

A skin can declare one or more host-content regions with `view{}`:

```lua
view{ id = "display", x = 78, y = 50, w = 186, h = 150 }
```

`view{}` declares a named rectangular cutout inside the skin canvas. The engine
exposes the collected rects via `Scene::views()`. The embedder renders its own
pixels — a video frame, a visualizer, a system monitor, anything — into a
`wgpu::TextureView` on the **same wgpu device** (zero-copy; no CPU readback),
and passes a lookup closure to `Renderer::draw`. The engine composites the
supplied texture into the rect, framing it with the surrounding skin chrome. If
the embedder supplies no texture for a view, the rect is left transparent.

The Headspace skin in the demo declares a `view{ id = "display", … }` and the
demo embedder paints a live CPU / MEM / SWP system-monitor readout into it each
frame — the same seam by which any host embeds carapace and "wears" a skin around
its own live content.

**What this is not.** `view{}` is a same-process, same-device GPU-texture
transport. It does not embed foreign-process apps (no OS-window reparenting).
Responsive / resizable layout inside a view is not part of this primitive; the
view rect is a fixed declared geometry, exactly like the other base primitives.

## Building & running

Requires a recent Rust toolchain (edition 2024; built against Rust 1.96). Dependency
versions are pinned via a committed `Cargo.lock`; CI builds `--locked`. On macOS the GPU
paths use Metal; on Linux, Vulkan. **Linux build dependency:** the text layer (parley/fontique)
links system `fontconfig` for font fallback, so a Linux build needs the dev package
(`libfontconfig1-dev` + `pkg-config`); macOS uses Core Text and needs nothing extra.

```sh
# Full workspace test suite (hit-test kernel, engine, headless skin/scene tests).
cargo test --workspace

# Launch the live demo: a borderless, draggable GPU window — the skin is the window.
#   Tab   cycle skins (classic / minimal / reference=real Headspace bitmap)
#   H     live-switch between the media-player host and a real sysinfo system monitor
#   click free-form hotspots fire host actions; the seek bar advances live
#   state survives a skin swap
cargo run -p carapace-demo
```

The GPU render-correctness test is gated behind the `gpu-tests` feature (it needs a real
adapter) and runs separately:

```sh
cargo test -p carapace --features gpu-tests --test render_offscreen
```

## Roadmap

The engine is built phase by phase against written specs (in `docs/superpowers/`). Phases
0–1 were throwaway (spike + prototype) and have been removed; 2 onward build the real
engine. Phase 5 was decomposed into sub-projects (5a–5e).

- **Phase 0 — rendering / hit-test spike.** ✅ Backend chosen: vello.
- **Phase 1 — throwaway prototype.** ✅ Surfaced the Lua↔host boundary and swap lessons.
- **Phase 2 — formal engine architecture spec.** ✅
- **Phase 3 — core engine + live host.** ✅ Headless core (scene graph, hit-testing,
  state, transactional swap, Lua scripting + capability sandbox, skin loader) then
  direct-to-surface render and the live `winit`/`wgpu` host app. CI + a software-render
  regression harness landed alongside.
- **Phase 5a — asset loading + `image` primitive.** ✅ Real bitmap skins.
- **Phase 5b — gradient fills.** ✅ `Paint` (solid + linear/radial/sweep) + color alpha.
- **Phase 5c — text + fonts.** ✅ `text{}` primitive: parley layout, fonts via the asset
  resolver (system fallback), value-bound strings, multi-line wrap, 2-D (halign × valign)
  anchoring, `Paint`-filled (chrome numerals).
- **Phase 5d — vocab ergonomics.** ✅ Shape path-helpers (`circle`/`rect`/`rounded_rect`);
  `on_press` on drawables (a control is drawn + clickable from one declaration);
  `value_fill` direction (right/left/up/down) + clip-to-path.
- **Phase 5e — host-extension mechanism.** ✅ A host registers a domain primitive
  (`VocabRegistry::register`) that binds its own actions via a Rust-side `host_action` handler —
  no Lua glue. The demo's `transport{}` (defined in the demo crate, not the engine) proves the
  seam. **Phase 5 is complete.**
- **Phase 6 — skin-as-window + cross-domain validation.** ✅ The demo renders the skin *as* a
  borderless, transparent, draggable window — vector skins self-shape into rounded silhouettes
  (the Headspace bitmap floats as a rectangle), with skin-drawn minimize/close — and the **H** key
  live-switches the whole window between a media player and a real `sysinfo` system monitor on one
  engine — proving total window replacement **and** zero domain knowledge (the only engine change
  is a transparent render base color). **Phases 0–6 complete.**
- **Live host-view region (`view{}` primitive).** ✅ A skin declares a named rectangular
  cutout; the embedder supplies a `wgpu::TextureView` (same device, zero-copy) and carapace
  composites it into the rect, framing it with skin chrome. `Scene::views()` exposes the
  rects; `Renderer::draw` accepts an embedder-provided texture lookup. The Headspace skin in
  the demo hosts a live CPU / MEM / SWP system-monitor painted into the `"display"` view each
  frame. GPU render-correctness covered by dedicated offscreen tests.

## Repository layout

```
crates/hittest/          # decoupled hit-testing kernel (no render deps)
crates/carapace/          # the engine (scene, state, swap, scripting, vocab, assets, render)
crates/carapace-demo/     # live winit/wgpu host app + bundled skins
docs/superpowers/specs/   # design docs, per-phase specs, decisions
docs/superpowers/plans/   # per-phase implementation plans
```

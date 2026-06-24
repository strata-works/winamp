# carapace

> A swappable shell over any host. (Repo codename: `winamp`.)

A general-purpose, **host-agnostic** skinnable UI engine for desktop apps — written in
Rust. It lets an application hand its entire surface over to a *skin* that defines its
own layout, appearance, and interactive hotspots, and lets users hot-swap skins at
runtime without losing app state.

> **Status: working engine, built phase by phase — Phases 0–6 complete, plus the live
> host-view region (`view{}` primitive), frame-skin support (resizable themed windows), and
> the Headspace music player (real audio, playlist, click-to-seek).**
> The demo is a borderless, transparent, draggable window where the skin *is* the interface —
> vector skins self-shape into rounded silhouettes; the **H** key live-switches the whole
> window between a functioning music player and a real `sysinfo` system monitor on one engine,
> proving total window replacement and zero domain knowledge (the only engine change was a
> transparent render base color); the Headspace skin is a full music player (real audio via
> `rodio`, clickable `list{}` playlist, `scrub{}` click-to-seek, next/prev, auto-advance,
> elapsed/total time readout) and also hosts a live CPU / MEM / SWP system monitor in its
> `view{}` region. A separate `frame` skin demonstrates resizable themed windows with 9-slice
> chrome — the window resizes, corners stay fixed, edges stretch.
> See [Current status](#current-status).

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
- **Domain-neutral base vocabulary, host-extensible.** The engine ships nine generic
  base primitives: `fill` (background), `region` hotspots, value-bound `value_fill`,
  `image`, `frame` (9-slice stretchable chrome), `text`, `view` (live host-content
  region; see below), `list` (dynamic host-driven list; see below), and `scrub`
  (click-to-seek progress bar; see below). Anything domain-flavored — "transport control", "audio visualizer" — is
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
monitor, clicks hit free-form hotspots, the Headspace skin is a functioning music player
(real audio via `rodio`, a clickable playlist, click-to-seek, next/prev, auto-advance),
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
through vello. The demo `reference` skin uses the genuine `headspace.png` faceplate as its
visual chrome — exactly how real WMP skins are built. See
[`docs/superpowers/specs/2026-06-18-phase5a-asset-loading-design.md`](docs/superpowers/specs/2026-06-18-phase5a-asset-loading-design.md).

## A skin, end to end

A skin is a directory: a `skin.toml` manifest (canvas size, entry script, asset dir) plus
a Lua entry script that calls vocabulary constructors. The demo's `reference` (Headspace)
skin is a functioning music player:

```lua
image{ asset = "headspace.png", x = 0, y = 0 }                       -- the bitmap faceplate
region{ path = { ... play button ... }, on_press = function() host.toggle_play() end }
region{ path = { ... next ... },        on_press = function() host.next() end }
scrub{ x = 16, y = 72, w = 220, h = 8,                               -- click-to-seek bar
       value = "position", on_seek = "seek", color = { r=120, g=230, b=80 } }
text{ x = 16, y = 84, value = "time",   size = 10, color = { r=200, g=200, b=200 } }
list{ collection = "playlist", x = 16, y = 100, w = 220, h = 120,    -- clickable playlist
      row_height = 20, on_select = "play_index",
      template = { { bind = "title", x = 4, y = 3, size = 11,
                     color = { r=220, g=220, b=220 } } } }
```

The bitmap supplies the look; Lua supplies placement and interactivity; the host supplies
state (`position`, `time`, `track_title`, `playing`) and actions (`toggle_play`, `stop`,
`next`, `prev`, `seek`, `play_index`). The engine knows nothing about "playback" — those
are just allowlisted action names and bound state keys. (No volume, shuffle, repeat,
drag-scrub, or playlist scrolling; bundled demo clips are generated public-domain tones.)

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

### Frame skins — resizable themed windows

A second skin archetype sits alongside the classic gadget skin: **frame skins** define
resizable windows rather than fixed-canvas widgets.

```toml
# skin.toml
resizable = true
min_size  = { w = 320, h = 240 }
```

Setting `resizable = true` in the manifest switches the engine from uniform-zoom mode
(gadget) to anchor-resolved layout mode (frame). A gadget skin sets a fixed `canvas`
size and the engine scales the whole scene uniformly to the output surface — every pixel
lands exactly where the design says, at any DPI. A frame skin omits a fixed canvas and
instead resolves each primitive's position from anchors before the GPU sees the scene; the
layout pass is CPU-only and produces a fully-resolved scene that the renderer handles just
like any other scene.

**Positioned primitives and anchors.** Every primitive that takes a position can carry an
optional `anchor` table:

```lua
fill{ path = rect(0, 0, 1, 1),   -- normalized or absolute coords in the resolved rect
      anchor = { "left", "right", "top", "bottom" },   -- all four → fills the whole window
      paint = { r=30, g=30, b=30 } }

image{ asset = "close.png", x = 0, y = 0, w = 16, h = 16,
       anchor = { "right", "top" } }   -- pinned to top-right corner
```

Anchoring rules:
- Specifying **both** sides of an axis (`"left"` + `"right"`, or `"top"` + `"bottom"`)
  makes that dimension **stretch** to fill the available space.
- Specifying **one** side pins the edge to that side of the window; the dimension is fixed.
- Default (no `anchor`) is **top-left fixed** — identical behaviour to a gadget-skin primitive.
- An optional `min = { w = …, h = … }` sub-table sets a minimum logical size for the
  resolved rectangle; the engine clamps to it before layout finalises.

**The `frame{}` 9-slice primitive.** `frame{}` draws stretchable bitmap chrome. It
splits a source image into the classic 3×3 grid using four inset distances and composites
each cell into the matching region of the destination rect:

```lua
frame{ asset   = "chrome.png",
       x = 0, y = 0, w = 1, h = 1,   -- destination (resolved via anchor)
       anchor  = { "left", "right", "top", "bottom" },
       slice   = { left = 12, right = 12, top = 28, bottom = 12 },
       center  = "stretch" }          -- or "hollow" to leave the center transparent
```

- `slice` — pixel distances from each edge of the source image to the slice boundaries.
  Corners are fixed (1:1 pixel copy). Edge cells stretch along the axis perpendicular to
  the edge. The `center` cell either stretches to fill the interior or is left transparent
  (`"hollow"`) for a frame-only effect. The engine clamps insets so they never exceed half
  the source dimension in either axis.
- `center = "stretch"` — draws all nine cells including the interior.
- `center = "hollow"` — draws only the eight border cells, leaving the interior untouched.

**Gadget skins are unchanged.** A skin that does not set `resizable = true` renders
identically to before: the engine applies a single uniform scale from the declared canvas
to the output surface. No GPU paths changed; the gadget pixel-identical guarantee is
covered by a dedicated offscreen regression test (`gadget_path_still_uniform_scales`).

**What the frame demo shows.** The bundled `frame` skin demonstrates a resizable window
with a 9-slice bitmap title bar and border, anchor-resolved controls (close button pinned
to top-right), and a stretching interior. A `view{}` region in the interior hosts a live,
two-pane read-only **file browser**: a shortcuts column and a directory listing, both driven
by `list{}` over a `FileBrowserHost`. Clicks inside the `view{}` region are translated into
the nested shell engine's coordinate space by the demo and forwarded to it; the engine
reuses scene hit-testing and host actions — no new input primitive was required.

**What the frame demo does not do.** The file browser is read-only: there is no scroll,
no selection highlight, no file opening, and no filesystem writes. The engine does not
embed foreign-process windows; it carries no audio subsystem (audio is in the demo host).

### The `list{}` primitive — dynamic host-driven lists

A skin can render a dynamic, variable-length list of rows with `list{}`:

```lua
list{
  collection = "entries",          -- collection name; host answers Host::rows("entries")
  x = 160, y = 30, w = 280, h = 340,
  row_height = 22,
  on_select  = "open_entry",       -- host action invoked with the row index on click
  template   = {                   -- one or more cells per row (plain data tables)
    { bind = "name", x = 4, y = 2, size = 13, color = { r=220, g=220, b=220 } },
  },
}
```

The engine calls `Host::rows(collection)` each frame and expands the `template` — a list of
`{ bind, x|right, y, size, color, halign }` cells — into one row per item, clamped to the
visible region. Clicking a row invokes the `on_select` host action with the row index.
Template cells are plain data tables, **not** `text{}` calls; constructors emit scene nodes
as a side effect and cannot be used inside a template.

`list{}` carries no scrolling, selection highlight, or multi-column sort — those remain
out of scope. It is the base seam by which a host exposes a flat, read-only collection to
a skin.

### The `scrub{}` primitive — click-to-seek progress bar

A skin can render a click-to-seek progress bar with `scrub{}`:

```lua
scrub{ x = 16, y = 72, w = 220, h = 8,
       value    = "position",    -- host state key (0.0–1.0 fill fraction)
       on_seek  = "seek",        -- host action invoked with the 0..1 click fraction
       color    = { r=120, g=230, b=80 },
       direction = "right" }     -- optional; "right" (default) | "left" | "up" | "down"
```

`scrub{}` renders a proportional fill from host state `value` (like `value_fill`) but is
hittable: clicking it invokes the `on_seek` host action with the click's 0..1 fraction,
via `Scene::hit_scrub` — the seek-bar analogue of `list{}`'s `hit_row`. Gadget skins route
through `Engine::layout()` so `scrub{}` (and `list{}`) work in them; the uniform-scale
gadget path is unchanged and pixel-identical.

`scrub{}` supports click-to-seek only; drag-scrub is out of scope.

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
- **Frame skins — resizable themed windows.** ✅ A second skin archetype: `resizable = true`
  + `min_size` in the manifest switches to anchor-resolved layout. Primitives take an optional
  `anchor` table (`"left"` / `"right"` / `"top"` / `"bottom"`; both sides of an axis →
  stretch; default = top-left = fixed) plus an optional `min` size. The `frame{}` 9-slice
  primitive splits a source bitmap along `slice = { left, right, top, bottom }` insets and
  composites corners (fixed), edges (stretched), and center (`"stretch"` | `"hollow"`). The
  engine resolves anchors to logical rects in a GPU-free layout pass; gadget skins render
  pixel-identically (uniform scale path unchanged). The bundled `frame` demo skin hosts a
  live, two-pane read-only file browser through the `view{}` seam — powered by the new
  `list{}` primitive, input routing into the nested shell engine, and a `FileBrowserHost`
  (read-only; no scroll, no selection highlight, no file opening).
- **Headspace music player.** ✅ The Headspace gadget skin is now a functioning music
  player. New `scrub{ x, y, w, h, value, on_seek, color, direction? }` base primitive: a
  click-to-seek bar that renders a proportional fill from host state `value` and invokes the
  `on_seek` host action with the click's 0..1 fraction (`Scene::hit_scrub`). Real audio
  playback via `rodio` (behind a mockable `AudioBackend` trait; `RodioBackend` for the live
  demo, `MockAudio` for tests, `NullAudio` fallback) drives `MusicPlayerHost`
  (play/pause/stop/next/prev/seek/play_index, auto-advance on track end). The skin exposes
  a clickable `list{}` playlist, a `scrub{}` seek bar, and an elapsed/total time readout.
  Gadget skins now route through `Engine::layout()` so `list{}`/`scrub{}` work in them;
  the uniform-scale pixel-identical guarantee is preserved (verified by
  `gadget_path_still_uniform_scales`). Base vocab is now **nine** primitives. Limitations:
  no volume/shuffle/repeat, click-to-seek only (no drag), no playlist scrolling; bundled
  demo clips are generated public-domain tones, not a library scan.

## Repository layout

```
crates/hittest/          # decoupled hit-testing kernel (no render deps)
crates/carapace/          # the engine (scene, state, swap, scripting, vocab, assets, render)
crates/carapace-demo/     # live winit/wgpu host app + bundled skins
docs/superpowers/specs/   # design docs, per-phase specs, decisions
docs/superpowers/plans/   # per-phase implementation plans
```

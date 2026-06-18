# Phase 3b — Render-to-Surface + Live Host App — Design

**Date:** 2026-06-18
**Status:** Approved design; the second half of Phase 3 (the build phase).
**Project:** carapace (repo codename `winamp`)
**Implements:** `2026-06-17-phase2-engine-architecture.md` §1–2 (render to a host-owned surface)
and resolves `2026-06-17-phase1-lessons.md` #8–#9 (direct-to-surface, wall-clock `dt`).

## Purpose

Make the engine render **live, on screen** for the first time, and fix the Phase 1
performance dead end. The headless core (3a) stops at "the scene is a correct projection
of state"; 3b draws that scene **directly to a host-provided GPU surface** (no offscreen
readback, no CPU blit) at vsync, driven by a real host application with a wall-clock
timestep. This is the on-screen test of the real engine — and it should be visibly faster
than the ~31 fps `proto` prototype.

Phase 3 was split (brainstorming) into 3a (headless core, merged) and **3b (this doc)**.

## Scope

**In scope:** the `render` module in `carapace` (vello + wgpu, host-owned `Renderer`);
a new `crates/carapace-demo` host-app binary (winit loop, a demo host, skins); an offscreen
render-parity + pixel-golden test runnable under a software adapter; a `lavapipe` CI job
for it; Criterion perf benches (local); and the Headspace reference skin.

**Out of scope (→ Phase 5):** richer vocabulary, **asset/bitmap loading**, text, gradients,
visualizer — and therefore the full-fidelity Headspace (the 3b reference skin is a flat
vector homage). **Skin generation** is a separate future phase (this reference skin is its
ground-truth). Widening the CI clippy/test scope to `--workspace` waits until the throwaway
`proto`/`spike-render` crates are removed at the end of Phase 3.

## 1. Architecture & crate layout

The `render` module lives **in** the engine (Phase 2 §1); the host app owns winit + the
wgpu surface + the loop.

```
crates/carapace/src/render.rs        # NEW: vello + wgpu; host-owned Renderer + RenderTarget
crates/carapace/benches/engine.rs    # NEW: Criterion benches (local)
crates/carapace-demo/                # NEW binary: the live host app
  Cargo.toml                         # carapace + winit + wgpu + pollster
  src/main.rs                        # winit loop: surface, drive engine, present at vsync
  src/demo_host.rs                   # DemoHost: impl carapace::Host (playing/position)
  skins/classic/{skin.toml, skin.lua}
  skins/minimal/{skin.toml, skin.lua}
  skins/reference/{skin.toml, skin.lua, headspace-source.png}
```

**The headless boundary holds.** Only `render.rs` touches vello/wgpu, as a **host-owned
`Renderer`** (not a method on `Engine`), so `Engine::new` never needs a device and every
existing headless test/CI path is unaffected. `carapace` *builds* the GPU stack but the
GPU-*running* render test is gated to the `lavapipe` job. `winit` lives only in
`carapace-demo` — the engine never depends on a windowing library.

**Per-frame data flow (host app owns the loop):**
1. winit input → `engine.handle_pointer(...)` (enqueues).
2. `engine.update(dt)` with **wall-clock `dt`** (from an `Instant` delta) → drains + ticks.
3. acquire the surface's current frame → `renderer.draw(engine.scene(), |k| engine.state(k),
   &target)` → vello renders the scene **directly into the surface view** → present at
   **vsync** (`PresentMode::Fifo`).

No offscreen texture, no readback, no CPU blit, no fixed `1/60` — the Phase 1 dead end,
fixed.

## 2. The `render` API

Host-owned; the engine stays headless.

```rust
pub struct RenderTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView, // the surface frame's view (or an offscreen view in tests)
    pub width: u32,
    pub height: u32,
}

pub struct Renderer { /* holds vello::Renderer */ }

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self;
    pub fn draw(
        &mut self,
        scene: &Scene,
        read_value: impl Fn(&str) -> Option<StateValue>,
        target: &RenderTarget,
    );
}
```

`draw` builds a `vello::Scene` from the carapace `Scene` (the node→vello mapping proven in
Phase 0/1) and renders into `target.view` via vello's `render_to_texture`:
- **`Fill`** → fill the path in its color.
- **`Hotspot`** → invisible (skins add a `Fill` to draw it — 3a stub semantics).
- **`ValueFill`** → read `read_value(value_key)` *now*; fill the path's bbox to
  `value × width`. `read_value` is the only channel into host state — render **reads, never
  writes** (pure projection, Phase 2 invariant). The demo passes `|k| engine.state(k)`.

`read_value: impl Fn` keeps `render` decoupled from `Engine`. `RenderTarget`'s borrows mean
the host hands over wgpu handles per frame; `Renderer` owns only the persistent
`vello::Renderer`.

**Canvas→surface scaling:** skins declare `scene.canvas`; the surface may differ. `draw`
applies a **non-uniform** scale transform (canvas→surface, stretch-to-fill) so the skin
fills the window; the demo maps clicks back through the **matching inverse** ratio
(`canvas/physical`), so clicks stay correct at any window aspect (DPI-correct, carried from
the Phase 1 viewer). *(As built: non-uniform stretch rather than letterbox — the two
mappings are exact inverses, so hit-testing stays consistent.)*

## 3. Render testing & perf benches

**Offscreen render-parity + pixel-golden test (the testable render coverage).** `draw`
targets any `TextureView`, so the test renders to an **offscreen** `Rgba8Unorm` texture
(no window), reads pixels back (the proven Phase 0 vello readback), and asserts:
- **sentinel-pixel assertions** (primary gate, *as built*): assert specific known pixels
  equal expected colors (inside a fill = its color; outside = base black; a `value_fill`
  half-filled at value 0.5). Deterministic across the `lavapipe` software adapter and robust
  to AA — the robust subset of the parity idea.
- a full committed **pixel-golden PNG** (Flutter-Gold analog) is **deferred** — fragile to
  AA/driver differences; the sentinel-pixel gate is what we ship.

These need a GPU adapter, so they are gated (a `gpu-tests` feature or `#[ignore]`) and run
only in the `lavapipe` job — the fast headless gate skips them.

**CI: a second `render` job.** Alongside the fast `check` job, an `ubuntu-latest` `render`
job `apt install`s Mesa (`mesa-vulkan-drivers`, `libvulkan1`, `vulkan-tools`), exports the
`lavapipe` ICD, and runs the gated render tests. Its own job so `check` stays fast; the
`render` job may be slower. (Industry-validated: this mirrors Flutter's software-rendered
golden tests — software rasterization is the *deterministic* path; real-GPU output is
left to local/manual.)

**Perf benches (Criterion, local).** `crates/carapace/benches/engine.rs`:
- headless hot paths — `Scene::hit` over a many-node scene (the reference skin), the command
  **drain**, **scene rebuild** (skin load);
- **render frame-time** — `Renderer::draw` to an offscreen target (the path behind the
  ~31 fps finding).
Run via `cargo bench` **locally** (real GPU) with Criterion baselines. NOT in the gating CI
(GPU perf is meaningless on `lavapipe`). This is where ~31 fps → vsync gets quantified.

## 4. Demo host, skins, interaction

**`DemoHost`** (in `carapace-demo`, impl `carapace::Host` — domain knowledge lives in the
demo, never the engine): media-style state `playing` (bool), `position` (0→1, advances
while playing); actions `toggle_play`, `stop`.

**Three skins** (`skin.toml` + `skin.lua`, stub vocabulary only):
- `classic`, `minimal` — simple skins to prove swap + basic layout.
- `reference` — the **Headspace homage** (see §5).

**Interaction (winit):** left-click → `handle_pointer`; **Tab** → enqueue `Swap` (state
preserved — the live state-survives-swap proof); **Esc/close** → quit. Window sized from
`scene.canvas × scale`; DPI-correct click mapping; wall-clock `Instant` delta feeds
`update(dt)`.

**The live test proves:** the real engine + real skins rendering **direct-to-surface at
vsync**, `position` advancing in **wall-clock time**, free-form hit-testing on click, and
state surviving a `Tab` swap — all on the production engine, visibly faster than `proto`.

## 5. The Headspace reference skin

Target: `https://wmpskinsarchive.neocities.org/images/Headspace.png` (342×394) — a green
organic alien-head media player: black display screen, 6 round speaker grilles on side
wings, a top transport row + sunburst options button, side arrows, a horizontal seek bar
with EQ/balance icons, a center logo, and a photographic face filling the bottom half.

`skins/reference/` is a **flat-color vector homage** hand-traced from the image (canvas
342×394), using only the stub vocabulary (~15–20 nodes — a genuine render/perf stress test):

**Reproduced now:**
- green organic **body silhouette** (free-form filled path);
- **black display** (dark filled rect);
- **6 speaker grilles** (filled circle-approximating polygons);
- **transport hotspots** — play→`toggle_play`, stop→`stop` (region + drawn fill each);
  prev/next, options, side arrows as fills/hotspots;
- **seek bar** — `value_fill` bound to `position`.

**Deferred to Phase 5** (noted in the skin + here): the photographic **face** (bitmap), the
metallic **gradients/shading**, **text/numerals**, and the **visualizer** in the black
screen. Until then the face is a flat placeholder fill and the screen is solid. When Phase 5
adds asset loading + richer vocabulary, the reference skin is upgraded toward true fidelity.

Coordinates are hand-estimated from the image, so it is a recognizable **stylized** Headspace,
not pixel-exact. The source PNG is committed at `skins/reference/headspace-source.png` as the
comparison reference. This skin is: the render stress-test, the perf-bench "busy scene," the
render-golden subject, and the **ground-truth for future skin generation**.

## 6. Decomposition & error handling

**Decomposition** (one phase, two independently-testable halves; the plan sequences them):
1. **render module + offscreen parity/golden test + `lavapipe` CI job + perf benches** —
   verifiable without a window (locally + the CI job).
2. **`carapace-demo` app** (winit loop, `DemoHost`, 3 skins incl. Headspace) — verified by
   `cargo build` + the human live run (the on-screen test), as Phase 0/1 GUI work was.

**Error handling:** the demo loop surfaces surface-acquisition / device-loss via `Result`,
avoiding `unwrap` panics in the loop where practical (lost/outdated surface frames are
reconfigured and skipped, not fatal). A skin that fails to load falls back via the engine's
transactional swap. `render::draw` of a malformed scene draws what it can and never panics
the host.

## Out of scope / deferred (recorded)

- Asset/bitmap loading, text, gradients, visualizer, richer vocabulary → **Phase 5** (the
  Headspace reference skin's fidelity upgrade rides on this).
- **Skin generation** (auto/AI-generated skins) → a future phase of its own; the Headspace
  reference skin is its ground-truth.
- Widening CI clippy/test to `--workspace` + removing `proto`/`spike-render` → end of Phase 3.
- The `lavapipe` `render` job staying best-effort vs required → revisit once it has run a few
  times and its flakiness/speed are known.

## Definition of done (3b)

`carapace` has a working `render::Renderer`; the offscreen parity + golden test passes under
`lavapipe` (and the CI `render` job is green); `cargo bench` runs the hot-path + render
benches locally; `carapace-demo` builds and runs a live window where `classic`/`minimal`/
`reference` skins render at vsync, clicks hit, `position` advances in real time, and `Tab`
swaps preserve state; the engine's headless boundary + the fast `check` CI job are unchanged.

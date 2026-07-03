# Paper.design Mesh-Gradient — Phase 2 (macOS host) Findings

**Date:** 2026-07-02 → 07-03
**Design:** `2026-07-02-paper-shader-phase2-design.md`
**Plan:** `../plans/2026-07-02-paper-shader-phase2.md`
**Branch:** `worktree-paper-shader-transpile-spike` (unmerged worktree)

## Verdict: ✅ GO

**Paper.design's transpiled shaders run live and animated inside a real macOS window, as the
surround framing a genuine native macOS media player — with real audio, on the proven
IOSurface Tier-2 path.** Phase 1 proved the transpile; Phase 2 proves it on screen in the
host, and goes well beyond the original spec into a polished, interactive demo.

**One deliberate scope change:** the spike is **no longer zero-engine-diff**. A single, small,
backward-compatible `carapace` engine change (the view-compositor now alpha-blends) was made
**at the user's direction** to let host content reveal the live gradient behind it. This is the
first engine change spun off this spike — the same pattern as the math-table PR (#23) off the
Flutter spike. See "Engine change" below.

## What the demo does (live, on real macOS, tier 2)

- Paper's transpiled mesh-gradient **animates as the full-window surround** (a living border),
  rendered by a wgpu pass from **pre-baked** WGSL (no glslang/naga at runtime except naga
  `wgsl-in` once per pipeline build for uniform offsets), driven by wall-clock `dt`.
- A **real native macOS player** is framed inside it: album art, "Cascade" / artist, a scrubber
  with a macOS slider **thumb**, transport as **SF Symbols** (`backward`/`play`·`pause`/`forward`
  `.fill`), and **real `AVAudioPlayer` playback** (plays on ▶, scrub seeks).
- **Translucent frosted card**: the card is ~62% white, so the soft gradient shines through the
  whole panel (macOS-vibrancy feel; no real blur — the content pass has no access to the paper
  pixels, but the gradient is soft enough to read as frosted glass).
- **Shader switcher**: press `s` or **click the album art** to cycle the surround shader —
  **mesh → neuro noise → swirl → waves → dithering** — each rendered distinctly.
- **Window chrome**: borderless, macOS-radius rounded corners; a floating `● paper_mesh` label +
  a `– ▢ ✕` control pill over the gradient titlebar strip; draggable anywhere on the gradient.

## Gate results

| Gate | Result |
|---|---|
| **P1 — builds & accepts** | ✅ cdylib + Swift host build clean; `PaperView` pipeline accepted by the engine device (no "paper shader disabled"); `active tier: 2 (Shared/Metal)`. |
| **P2 — live in-window, framed** | ✅ the window shows paper's gradient as the surround framing the real player. Evidence: `macos-sample/evidence/*.png`. |
| **P3 — animated + real audio** | ✅ gradient animates on wall-clock time; ▶ plays the audio (`player loaded=true playing=true`, `currentTime` advances). |
| **P4 — perf + engine-diff** | ⚠️ full-window shader renders every frame though only the border/frosted-card show it (content overdraws the interior); smooth at tier 2. Engine diff is now **1 intentional file** (see below), not zero. |

## Key engineering

- **`PaperView`** (`crates/embed-spike/src/paper_view.rs`): builds a wgpu pipeline from the baked
  WGSL, fills uniforms by name at naga-computed offsets, renders one frame per tick into an owned
  `TEXTURE_BINDING` texture. Generalized to a `SHADERS` list with `cycle()` rebuilding the
  pipeline; `member_values` broadened to the switchable shaders' uniforms.
- **N-cutout compositing**: the engine already composites every `view{}` node by id; embed-spike's
  `render_frame` was widened from one `Option` host_view to a `&[(id, view)]` slice. Two cutouts:
  full-bleed `paper` (fed the wgpu texture) behind, inset `content` (fed the host player IOSurface)
  in front. **z-order comes from the skin's `view{}` declaration order**, not the slice order.
- **Retina sharpness**: the content IOSurface is allocated at the backing scale (2×) and the draw
  context scaled to logical units — the player was blurry when the surface was 1× upscaled into 2×.

## Runtime bugs the live run exposed (each fixed)

1. **Content cutout id mismatch** — engine pushed content under id `"host"` but the new skin
   declares `view{ id="content" }`; the player never composited (only the gradient showed).
2. **Transport hotspots misaligned** with the Swift-drawn glyphs → play clicks fell through to the
   drag region and audio never started; realigned the skin hotspots to the glyph positions.
3. **Black card corners** → the compositor overwrote host content (no alpha blend); fixed by the
   engine change below (after an interim static-gradient workaround).

## Engine change (zero-engine-diff intentionally broken)

`crates/carapace/src/render.rs`: the view-composite pipeline's `blend: None` →
`Some(PREMULTIPLIED_ALPHA_BLENDING)` (one target-state; 8 insertions). Host content is
premultiplied (CGContext `premultipliedFirst`), so this blends supplied `view{}` content over
whatever is already in the target (vello scene + earlier view layers). **Backward-compatible:**
opaque content (alpha=1) composites identically to before. This is what makes the card's
transparent corners and translucent frosted body reveal the **live animated gradient**.

**Limitation it does NOT solve:** carapace *vector* (Lua `fill{}`/`text{}`) still cannot paint
over the full-bleed shader — vello renders **under** the view-composite layer, so an opaque paper
`view{}` covers it. That's why the window controls are **AppKit overlays** (real `NSButton`s over
the CALayer), not skin vector. A true skin-vector-over-shader control would need the paper layer
to carry alpha holes or the controls to be their own late-composited `view{}`.

## Breadth of the other paper shaders ("a few + switcher")

paper ships **29** `*FragmentShader` exports, **all `#version 300 es`**, all resolved. Ran a
representative 6 through the Phase-1 ladder (diagnostic `transpile_more_shaders`):

- **4 transpile clean** via the `spirv` rung: dithering, neuro noise, swirl, waves — vendored +
  wired into the switcher (with mesh gradient = 5 total).
- **2 fail** at naga `spv-in` with `InvalidId`: **metaballs, voronoi** — glslang emits valid
  SPIR-V that naga's importer can't consume.

**Conclusion:** "trivial by reuse" is *mostly* true but **NOT 100%** — a subset of shaders hit
naga `spv-in` edge cases needing per-shader attention (different glslang flags, a naga workaround,
or SPIRV-Cross). The switcher ships the 4 clean ones, not all ~20.

## Scope / caveats

- **Copyrighted audio not committed.** The demo plays a user-supplied song converted to
  `Resources/sample.m4a` (working-tree only); the committed placeholder is a synth tone.
- **Perf:** the surround renders the whole window every frame though only the border + frosted
  card reveal it. Border-only / dirty-region optimization is out of scope (a measured P4 note).
- **Out of scope (unchanged):** a real `shader{}` Lua primitive, sandbox/trust, general uniform
  mapping, build-time transpile pipeline, the metaballs/voronoi `spv-in` fixes.

## Recommendation (productization)

1. **`shader{}` primitive**: expose paper-style shaders as a first-class skin node. Prefer a
   **build-time** GLSL→WGSL transpile (glslang isn't pure-Rust) that bakes WGSL + a uniform
   schema; the runtime just binds uniforms and renders (as `PaperView` does).
2. **Compositor alpha-blend**: the engine change here is a genuine improvement (hosts can supply
   transparent content) — land it as its own reviewed PR, like the math-table PR.
3. **Shader coverage**: to cover all ~20 paper shaders, resolve the naga `spv-in` `InvalidId`
   cases (metaballs/voronoi) — try SPIRV-Cross for MSL/WGSL or a newer naga.
4. **Skin-vector-over-shader** (optional): if controls should be skin-drawn over the shader,
   composite the vello vector layer *after* the view layer, or give the paper view alpha holes.

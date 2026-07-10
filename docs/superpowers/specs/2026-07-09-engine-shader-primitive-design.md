# Engine `shader{}` Primitive — Design

**Status:** Approved (brainstorming) — ready for implementation plan.
**Date:** 2026-07-09

## Goal

Add a first-class `shader{}` primitive to the carapace engine: a skin declares a rectangle that the
engine fills each frame by running author-supplied WGSL, driven by reactive uniforms (engine clock,
resolution, literals, and live host-data bindings). Any skin can use it; nothing here is
weather-specific.

## Context & motivation

This is **sub-project 1 of 2**. The north-star request is a weather-app showcase whose skin is a
shader that reflects the current day's weather condition/season. That decomposes into:

1. **This spec — the engine `shader{}` primitive** (reusable capability).
2. **The weather showcase** (sub-project 2, its own spec later): Open-Meteo data + mock fallback, six
   parameterized condition shaders, condition→uniform mapping, weather-UI skin — all *consuming* this
   primitive.

Prior art this builds on:
- **Paper-shader spike** (`crates/embed-spike/`, `docs/.../2026-07-02-paper-shader-*`): proved
  paper.design WGSL transpiles + renders animated on wgpu (naga path). But it ran WGSL **host-side**
  via raw wgpu — never composited a custom WGSL pass *inside* the engine's vello scene.
- **Host-view content registry** (shipped, PR #40): the engine already composites external textures
  into `view{}` rects via a raw-wgpu blit pass (`render.rs` `composite_pipeline` + `composite.wgsl`),
  proving vello and a raw-wgpu compositing stage already coexist in the renderer.

## Current renderer model (verified)

`crates/carapace/src/render.rs` `Renderer::draw(scene, state_fn, view_tex, target)`:
1. **Vello pass** — iterates `scene.nodes` (draw order) into a vello `Scene`, then
   `render_to_texture(target.view)` which **clears** the target to `base_color`. `Node::View` is a
   **no-op** in this pass.
2. **View composite pass** — iterates nodes again; for each `Node::View` with a resolved texture,
   blits it into the view's surface-space rect **on top of** the vello output.

So the renderer already has exactly two layers: vello 2D (bottom) then composited textures (top),
with no per-node z-order between them. Nodes live in `crates/carapace/src/scene.rs` (`enum Node`);
primitives are registered in `VocabRegistry::base()` (`vocab.rs`) and wired data-drivenly in
`script.rs`. Host-data bindings (`text{ value="k" }`, `value_fill`) resolve `host.get(key)` **every
frame** in `render.rs`.

## Architecture: compositing (the crux)

A weather **background** shader needs pixels *under* the UI, but vello's clear erases anything drawn
first. So the frame becomes **4 stages** (from 2):

1. **Shader pass** — for each `Node::Shader`, run its WGSL into the target (the background layer).
2. **Vello pass** — render 2D nodes into a **transparent offscreen** texture (not the target).
3. **Composite vello** — alpha-blend the vello offscreen **over** the shader background (into target).
4. **View composite** — blit `view{}` host content on top (unchanged).

**v1 scope: `shader{}` is a background layer only** — rendered in stage 1, beneath the 2D UI. This is
the 80% case (weather/ambient/gradient backdrops). Foreground shader effects (e.g. rain *over* the
UI) are an explicit future extension, not v1.

**Performance is first-class** for carapace, and this adds two full-frame passes (a vello offscreen +
a full-frame composite) plus the shader pass. The spike (below) **measures the added per-frame ms**
and gates the design on it.

### Step 1 is a spike (GO/NO-GO)

Before building the real primitive, a throwaway spike hacks `render.rs` into the 4-stage pipeline
with a single trivial shader, proving the background-under-UI order composites correctly and
**reporting the added per-frame cost**. `FINDINGS → GO/NO-GO`, matching the paper-shader and
host-embedding spike pattern. If perf is unacceptable, the design is revisited before more work.

## The `shader{}` primitive

### Lua interface

```lua
shader{ src = "clear.wgsl", x = 0, y = 0, w = 720, h = 480,
        uniforms = {
          season    = 2,          -- number literal → constant uniform
          temp      = "wx_temp",  -- string → host binding key, resolved each frame → f32
          tOfDay    = "wx_time",
          intensity = "wx_rain",
        } }
```

- `src` — a `.wgsl` file resolved in the skin directory (loaded at skin load, like assets).
- `x, y, w, h` — the rect, in design-canvas coordinates (scaled to surface space like other nodes).
- `uniforms` — a table of `name = number | string`. A **number** is a constant. A **string** is a
  host binding key resolved via `host.get(key)` **every frame** (reusing the existing binding path);
  a missing/non-numeric key resolves to `0.0`.

### Author contract: fragment only

The author writes **only a fragment shader** body referencing a uniform struct `u`. The engine:
- provides the **vertex stage** (a fullscreen triangle clipped to the rect) and passes the fragment a
  `uv` in `0.0..1.0` across the rect (top-left origin); pixel coordinates derive from `uv *
  u.resolution`;
- **generates and prepends the uniform struct + `@group @binding`** from the declared uniform names,
  so the author references `u.time`, `u.resolution`, `u.season`, `u.temp`, … directly — no manual
  binding or packing. The engine owns std140 layout.

### Uniform ABI (v1)

- **Types:** scalar `f32` only for user uniforms. Vectors/colors are computed *inside* the shader
  from scalars — this keeps the host interface a simple `key → f32`.
- **Standard uniforms (always present):** `time` (f32, seconds — the engine's accumulated animation
  clock, the same `dt`-driven time that `engine.update(dt)` advances, so shader animation stays in
  lockstep with the rest of the scene) and `resolution` (`vec2<f32>`, the shader rect's pixel size).
- **Packing:** the engine generates the WGSL struct and the matching CPU-side buffer with a single,
  engine-owned std140 layout; the exact packing is an implementation detail of the plan. Uniform
  values are written to the buffer each frame (literals unchanged, host-bound keys re-read).

### Compilation & errors

WGSL (engine prelude + author fragment) is compiled via wgpu/naga at **skin load**. A compile error →
`ErrBadSkin` carrying the naga message — the same failure surface as a malformed skin today. No
first-frame or runtime compile surprises.

## Safety & limitations (stated, not hidden)

Arbitrary WGSL runs on the GPU. A pathological shader (e.g. a huge loop) can stall the GPU; the
fragment-only contract limits the blast radius but does **not** sandbox it. For trusted, local skins
this is acceptable. This primitive is **not** a safe surface for untrusted third-party skins without
further work (GPU watchdog/timeouts, complexity limits) — explicitly out of scope for v1.

## Testing

- **GPU test** (behind the existing `gpu-tests` feature, real adapter): a `shader{}` renders
  **non-blank** *and* **reacts to a uniform** — render the same shader with `intensity = 0` vs
  `intensity = 1` and assert the pixel outputs differ. This proves the reactive uniform path, not
  merely that something drew.
- **Compositing test:** a `shader{}` background plus a 2D primitive (e.g. `fill{}`/`text{}`) on top —
  assert the 2D is visible over the shader (background-under-UI order) and the shader shows where the
  2D doesn't cover.
- **Error path:** a `shader{ src = <malformed.wgsl> }` load returns `ErrBadSkin` with a message.
- Existing skin/render tests stay green (the 4-stage pipeline must not regress skins with no
  `shader{}` — when no shader nodes exist, the pipeline must be equivalent to today's output).

## Cross-platform

The engine core (`crates/carapace`) is cross-platform wgpu/vello; the `shader{}` primitive is too.
GPU tests run under the `gpu-tests` feature / the CI lavapipe `render` lane. (The Apple-only gating
lives in `carapace-ffi`, not the engine core.)

## Documentation

- `docs/api/skin-authoring.md` — a `### shader{}` section: the Lua interface, the fragment-only
  author contract, the standard + literal + host-bound uniform model, `f32`-only, the background-layer
  semantics, and the safety note.

## Sub-project 2 preview (not designed here)

The weather showcase will: fetch Open-Meteo (current/hourly/daily) with a bundled mock fallback and a
demo condition-cycle; map WMO weather codes + month + local time → a base shader (`clear`, `cloud`,
`rain`, `snow`, `storm`, `fog`) plus `season`/`tOfDay`/`temp`/`intensity` uniform values published as
`wx_*` host keys; and draw the weather UI (current, hourly, daily) as engine 2D primitives over the
`shader{}` background. Validating this primitive against that consumer is why the uniform model is
reactive and `f32`-keyed.

## Open questions / deferred

- Exact std140 packing scheme for generated uniforms — implementation detail for the plan.
- Foreground-layer shaders (over the UI) — future extension.
- GPU-hang protection for untrusted skins — out of scope for v1.
- Whether the vello-offscreen introduced here should be reused by other future effects — revisit
  after the spike.

# Paper.design Mesh-Gradient → macOS Skin — Feasibility Spike

**Date:** 2026-07-02
**Status:** Design (approved for spec review)
**Type:** Feasibility spike (Phase 1: zero engine-crate diff, deletable)

## North star

A **native macOS app whose skin is paper.design's live, animated mesh gradient**. The user
wants the flowing paper look as the window backdrop — not a static image, and not a
carapace-native gradient approximation, but paper's actual `meshGradientFragmentShader`
running live.

Most of the stack for this already exists and is proven:

- **Native macOS host** — host-embedding spike (merged #21): Swift host + C-ABI cdylib,
  zero-copy IOSurface, total window replacement.
- **C ABI to drive the engine** — carapace-ffi v1 (merged #25), Apple-only.
- **External-texture compositing** — the engine's `view{}` primitive already composites a
  host-supplied wgpu/IOSurface texture into a rect. A shader-rendered gradient can ride
  that exact seam with potentially **zero engine change**.

The **one unproven link** is turning paper's WebGL2 GLSL into something wgpu/Metal can run.
This spike retires exactly that — narrowed to the single shader the user wants.

## Two phases

- **Phase 1 (this spec + its plan): transpile + animated render, offscreen.** Prove
  paper's mesh-gradient can be transpiled GLSL→WGSL (fragment *and* the shared vertex
  shader it depends on) and rendered as a correct, flowing animation offscreen. Zero engine
  diff. If this is green, the real risk is dead.
- **Phase 2 (gated on Phase 1, its own future spec/plan): macOS integration.** Render the
  gradient per-frame into an IOSurface and composite it through `view{}` in the Swift host
  (#21). One design decision deferred to then: render via **wgpu/WGSL** (reuse Phase 1's
  output) or **Metal/MSL** (naga also has `msl-out`, letting the Swift host render it
  natively with no wgpu). We decide with Phase 1's evidence in hand.

The rest of this document specifies **Phase 1**.

## Why this shader is not trivial (what Phase 1 must actually prove)

`meshGradientFragmentShader` (WebGL2, `#version 300 es`):

- **Fragment uniforms:** `u_time` (float), `u_colors[10]` (vec4 array), `u_colorsCount`
  (float), `u_distortion`, `u_swirl`, `u_grainMixer`, `u_grainOverlay` (floats). These are
  **freestanding WebGL uniforms** — WGSL has no default uniform block, so how naga maps them
  (individual bindings? merged block? push-constants?) is a real unknown.
- **Reads a varying:** `in vec2 v_objectUV;` — the fragment is **not** self-contained. That
  varying is produced by paper's shared **vertex shader** (`vertexShaderSource`), which does
  all the sizing math from ~13 of its own uniforms. So Phase 1 must transpile **both stages**
  and link them.
- **Cross-stage linkage:** `v_objectUV` is the vertex shader's *first* `out` and the
  fragment's *only* `in`, so both should land at `@location(0)` under naga's
  declaration-order assignment. This is the expected happy path; confirming it (and the 6
  unused extra vertex outputs don't break linkage) is part of the proof.
- **Snippet interpolation:** the GLSL string interpolates shared snippets (`${declarePI}`,
  `${rotation2}`, `${proceduralHash21}`) and the `${maxColorCount}` constant at JS runtime —
  so the source must be extracted by *evaluating* the package, not copied raw.

## Goal & success criteria (Phase 1)

Take paper's mesh-gradient (fragment + vertex) unmodified and prove:

- **G1 — Transpile:** both stages GLSL→valid WGSL, **no edits to effect logic** (mechanical,
  documented preprocessing allowed — see ladder). Record which ladder rung each stage needed.
- **G2 — Accept:** wgpu `create_shader_module` accepts both WGSL outputs, and a render
  pipeline linking vertex+fragment builds without validation error.
- **G3 — Render:** the linked pipeline renders an **animated** frame sequence whose output
  **visually matches paper's reference** (flowing mesh gradient) by eyeball, with frames at
  different `u_time` values visibly differing.

**Deliverable:** a go/no-go verdict, a per-stage transpile-rung record, an **animated PNG
sequence** of the mesh gradient (the artifact you actually look at), and a findings doc.

The verdict we're buying:
> "Can we run paper's mesh gradient live, via transpilation, without hand-porting it — and
> via which toolchain path (naga-direct / preprocess / SPIR-V)?"

## Transpile approach — fallback ladder (the ladder is itself a finding)

Attempt each stage in order; record the lowest rung that worked:

1. **naga `glsl-in` direct** — add `naga` (v29, matching wgpu 29's resolved version) as a
   direct spike dependency with `glsl-in` + `wgsl-out` and feed the raw GLSL per stage.
2. **Light preprocess → naga** — mechanical normalization only (version/precision/WebGL-isms),
   documented per rule, never a rewrite of effect logic.
3. **glslang → SPIR-V → naga `spv-in` → WGSL** — heavier frontend, for GLSL naga's parser
   rejects outright. Gracefully "unavailable" if `glslangValidator` isn't installed.

**Version-decoupling:** the pipeline is GLSL → (naga) → **WGSL string** → handed to wgpu,
which compiles it with its own bundled naga. So the spike's naga copy need not match wgpu's
internal one.

**Uniforms:** we set the vertex sizing uniforms to sensible defaults (scale=1, fit=none,
worldWidth/Height=0 → use resolution, rotation=0, offsets=0, originX/Y=0.5, pixelRatio=1),
give the fragment a small palette (`u_colorsCount`≈4, a few `u_colors`, low distortion/swirl/
grain), and animate `u_time`. Enough to confirm G3 flow; not a general uniform-mapping API.

## Harness & isolation

A **standalone example** at `crates/carapace-demo/examples/paper_mesh_spike.rs` (directory
example with focused modules), **zero diff to the `carapace` engine crate**. Offscreen wgpu
+ PNG readback (same pattern as `examples/shoot.rs`), rendering the linked vertex+fragment
pipeline to a PNG sequence across `u_time` values. Deletable in one commit if no-go.

## Explicitly out of scope (Phase 1)

- The `shader{}` Lua primitive and any Lua-facing API.
- Phase 2 macOS integration (IOSurface + `view{}` compositing, wgpu-vs-MSL decision).
- Any shader other than mesh-gradient (+ its required vertex shader).
- General/faithful uniform mapping beyond the defaults above.
- Sandbox / trust hardening, audio-reactivity, performance tuning.
- Any change to the `carapace` engine crate.

## Expected findings shape

`2026-07-02-paper-shader-transpile-spike-findings.md`: per-stage transpile rung (or failure +
first error), G2 accept/link result, G3 render result, the animated PNG artifact paths, any
preprocessing rules added, `glslangValidator` availability, and the go/no-go verdict with a
one-paragraph Phase-2 recommendation (e.g. "naga-direct handles both stages, WGSL renders
flowing gradient → Phase 2 can reuse WGSL in wgpu" vs "vertex stage needs SPIR-V rung →
prefer a build-time transpile / consider MSL-out for the native host").

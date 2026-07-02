# Paper.design GLSL → WGSL Feasibility Spike

**Date:** 2026-07-02
**Status:** Design (approved for spec review)
**Type:** Feasibility spike (zero engine-crate diff, deletable)

## Motivation

We want carapace skins to be able to use the animated fragment-shader effects from
[shaders.paper.design](https://shaders.paper.design/) (mesh gradients, warp, dithering,
voronoi, metaballs, neuro-noise, etc.). The eventual shape is a `shader{}` Lua primitive
that renders a paper effect into a rect and composites it under the hosted app — the
engine's existing `view{}` primitive already composites an external wgpu texture, so the
integration seam exists.

Before committing to that primitive, we retire the **scariest single unknown**: paper's
shaders are **WebGL2 GLSL ES 3.0**, and carapace renders through **vello on wgpu/WGSL**.
Can real, unmodified paper GLSL be transpiled to WGSL and run on wgpu at all? If yes, ~any
of the ~30 paper effects becomes reachable without hand-porting each one. If no, the whole
"use paper shaders" premise needs a different plan (hand-port, or a different renderer).

This spike proves (or disproves) exactly that, and nothing more.

## Goal & success criteria

Push a curated spread of paper shaders through a GLSL→WGSL transpile pipeline and run the
result. For **each** shader, record three gates:

- **G1 — Transpile:** GLSL → *valid WGSL* with **no hand-editing of the shader body**
  (light, mechanical, documented preprocessing is allowed — see Transpile Ladder).
- **G2 — Compile:** the produced WGSL passes `wgpu` shader-module creation +
  `create_render_pipeline` (i.e. wgpu's own naga validation accepts it).
- **G3 — Render:** produces animated output that **visually matches paper's reference**
  by eyeball (not pixel-perfect).

**Deliverable:** a **go/no-go verdict** plus a **pass/fail table** (one row per shader,
columns G1/G2/G3), a **failure note** for each miss, and — importantly — **which rung of
the transpile ladder each shader needed**. Plus one dumped **PNG frame per shader** as an
artifact, and a short findings doc.

The verdict we're buying:
> "Can we run paper's shaders at all, without hand-porting each one — and if so, via which
> toolchain path?"

## Test set (complexity ladder)

Chosen to expose where a transpiler breaks, not to cherry-pick easy wins:

| Shader | Complexity | Why it's in the set |
|---|---|---|
| `radial_gradient` | trivial | baseline; if this fails, the harness is wrong |
| `mesh_gradient` | easy | multiple uniforms, color mixing |
| `warp` | medium | domain warping, nested function calls |
| `dithering` | medium | branching, integer/threshold ops |
| `voronoi` | hard | loops over a neighborhood |
| `metaballs` | hard | noise functions, accumulation |

Source GLSL is pulled from the **open-source `@paper-design/shaders` core package** (the
raw fragment-shader strings), **not** the `@paper-design/shaders-react` wrappers. Fetching
that package counts as a third-party dependency fetch and must go through Socket Firewall
per repo policy.

If a listed effect isn't shipped as extractable GLSL in the core package, substitute the
nearest-complexity neighbor from the same package and note the swap. The ladder shape
(trivial→easy→medium→hard×2) is what matters, not the exact six names.

## Transpile approach — a fallback ladder (the ladder is itself a finding)

Paper's shaders are `#version 300 es` fragment shaders with `precision` qualifiers, `uniform`
declarations, and an `out vec4` output. Attempt each shader in order and **record the lowest
rung that worked**:

1. **naga `glsl-in` direct.** Add `naga` as a direct spike dependency with the `glsl-in` +
   `wgsl-out` features and feed the raw GLSL (fragment stage) straight in.
2. **Light preprocess → naga.** Mechanical normalization only (version line, precision
   qualifiers, WebGL-isms, uniform layout). Any preprocessing is documented and applied by
   rule, never a hand-rewrite of the effect logic.
3. **glslang → SPIR-V → naga `spv-in` → WGSL.** Heavier but robust frontend, for shaders
   naga's GLSL parser rejects outright.

**Version-decoupling note:** the pipeline is GLSL → (naga) → **WGSL string** → handed to
`wgpu` as WGSL source, which wgpu compiles with *its own* bundled naga. So the spike's
`naga` dependency does **not** need to match wgpu 29's internal naga version — we exchange a
string, not naga IR. This keeps the dependency graph clean.

**Uniforms:** full fidelity is out of scope. We drive `time` + `resolution` (+ up to a
couple of per-effect params where needed to see motion) through a single uniform buffer
filled each frame. Enough to confirm G3 animation; not a general uniform-mapping solution.

## Harness & isolation

A **standalone example** at `crates/carapace-demo/examples/paper_shader_spike.rs`:

- Reuses carapace-demo's existing winit + wgpu bootstrap (same pattern as
  `examples/shoot.rs`), so minimal scaffolding.
- **Zero diff to the `carapace` engine crate.** All new code lives in the example plus a
  `naga`/`glslang` dev-dependency on `carapace-demo`. `image` (png) is already a
  carapace-demo dev-dependency for the PNG dumps.
- For each shader: transpile (recording the ladder rung), build a full-screen-quad
  pipeline with the produced WGSL, render animated to a winit window (eyeball the motion),
  and dump one PNG frame to disk.
- Prints the pass/fail table to stdout at the end.

Deletable in one commit if the answer is no-go.

## Explicitly out of scope (YAGNI for this spike)

- The `shader{}` Lua primitive and any Lua-facing API.
- Compositing into a real skin via `view{}` (noted as a trivial follow-on / the *next*
  spike, not built here).
- Sandbox / trust hardening for skin-supplied GPU code.
- General/faithful uniform mapping beyond time + resolution + a couple of params.
- Audio / music-reactivity.
- Performance tuning.
- More than the ~6 shaders in the test set.
- Any change to the `carapace` engine crate.

## Expected findings shape

A `2026-07-02-paper-shader-transpile-spike-findings.md` doc containing: the pass/fail
table, per-shader failure notes, the transpile-ladder rung each shader needed, the go/no-go
verdict, and a one-paragraph recommendation for the follow-on (e.g. "naga direct handles
simple+medium; hard shaders need the glslang→SPIR-V rung → the `shader{}` primitive should
ship a build-time transpile step rather than a runtime one").

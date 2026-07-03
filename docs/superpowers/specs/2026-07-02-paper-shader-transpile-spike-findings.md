# Paper.design Mesh-Gradient → macOS Skin — Phase 1 Findings

**Date:** 2026-07-02
**Spec:** `2026-07-02-paper-shader-transpile-spike-design.md`
**Plan:** `../plans/2026-07-02-paper-shader-transpile-spike.md`
**Branch:** `worktree-paper-shader-transpile-spike`

## Verdict: ✅ GO

**Paper.design's live, animated mesh gradient can be run as a carapace skin via
transpilation, with no hand-porting of shader logic.** The unproven link is now proven
end-to-end: paper's real `#version 300 es` mesh-gradient shaders transpile to WGSL and render
a correct, flowing, animated gradient on wgpu — with **zero diff to the carapace engine
crate**.

The one caveat: the transpile does **not** run through pure-Rust naga alone. It requires a
`glslang` step (`glslang → SPIR-V → naga spv-in → WGSL`). See the recommendation for what
that means for Phase 2.

## Gate results (all against the REAL vendored paper shaders)

| Stage | G1 Transpile | Rung | G2 wgpu-accept | G3 Render |
|---|---|---|---|---|
| `vertex.vert` (paper shared vertex) | ✅ | `SpirV` | ✅ pass | — |
| `mesh_gradient.frag` | ✅ | `SpirV` | ✅ pass | ✅ non-blank + animated |

Driver output (`cargo run -p carapace-demo --example paper_mesh_spike`):
```
G1 vertex:   spirv
G1 fragment: spirv
G2 vertex:   pass
G2 fragment: pass
G3 render: ok -> /tmp/paper-mesh-spike
```

**G3 evidence:** rendered 512×512 frames at t=0.0/1.3/2.6. Pixel variance ≈3774 (not blank);
t=0 vs t=3 differ in 74.6% of bytes (genuinely animated). Frames independently eyeballed
(controller + reviewer) — authentic paper mesh gradient: smoothly blended pink/blue/gold/green
color spots with organic swirl/distortion. Artifacts: `/tmp/paper-mesh-spike/mesh_t{0,1,2}.png`.

## What the transpile ladder actually needed (rung-by-rung)

1. **naga `glsl-in` direct (rung 1) — FAILS on paper's ES shaders.** Confirmed against
   naga 29.0.3 source: the GLSL frontend accepts only `#version 440|450|460` (rejects
   `300 es` outright) and rejects bare uniforms lacking `layout(binding=X)`. Paper's shaders
   are `#version 300 es` with freestanding uniforms, so rung 1 cannot touch them. (Rung 1
   *does* work for desktop-dialect GLSL — proven by the `trivial_fragment` test.)
2. **Light preprocess (rung 2) — not sufficient alone**, left as a no-op scaffold. Making
   naga-direct accept ES would require injecting `layout(binding=N)` on every uniform and a
   dialect bump — more brittle than rung 3, and unnecessary given rung 3 works.
3. **glslang → SPIR-V → naga `spv-in` (rung 3) — WORKS.** This is the path both stages take.

### The winning glslang invocation (found empirically)

```
glslang -V --target-env vulkan1.0 -R --amb --aml -S <stage>
```
plus a **mechanical `#version 300 es` → `#version 310 es` bump** (directive line only; no
effect-logic edit). Dead-ends encountered and ruled out:
- **strict Vulkan** (no `-R`): rejects paper's freestanding (non-block) uniforms.
- **`-G` OpenGL SPIR-V**: naga's `spv-in` chokes on `OriginLowerLeft`
  (`UnsupportedExecutionMode(8)`).
- **`-R` relaxed Vulkan**: unlocks freestanding uniforms *and* keeps the naga-friendly
  `OriginUpperLeft`. This was the unlock.

## Integration facts discovered (these shape Phase 2)

- **Uniform layout:** glslang merges all freestanding ES uniforms into a single
  `gl_DefaultUniformBlock` struct per stage (vertex 56 B, fragment 208 B). The harness fills
  members **by name at naga-computed offsets** (`u_time`←time, `u_colors`←palette, sizing
  defaults) — reliable, not declaration-order guessing. Phase 2 must do the same: read the
  emitted WGSL `@group/@binding` + struct members, don't assume ES uniform order.
- **Binding collision:** both stages emit `@group(0) @binding(0)`. The harness relocates the
  fragment block to `@group(1)` via a WGSL group-number shift. Phase 2 needs a deliberate
  group/binding allocation across stages.
- **Cross-stage linkage:** paper's `v_objectUV` is the vertex shader's first `out` and the
  fragment's only `in`; both land at `@location(0)` and link cleanly. The vertex shader's 6
  other outputs are unused by the fragment (WGSL permits this).
- **Vertex input:** paper's vertex shader consumes `a_position` at `@location(0)` — supplied
  as a 4-corner clip-space quad vertex buffer.

## Toolchain / reproducibility

- naga **29** (matches wgpu 29; the pipeline exchanges a WGSL *string*, so the spike's naga
  is decoupled from wgpu's internal copy). Features used: `glsl-in`, `wgsl-out`, `spv-in`,
  and `wgsl-in` (harness re-parses WGSL for exact member offsets). All on the **carapace-demo
  crate only** — zero engine-crate diff throughout.
- **glslang 16.3.0** (installed via `sfw brew install glslang`). The binary may be named
  `glslang` or `glslangValidator`; the code probes both.
- Paper GLSL vendored by evaluating `@paper-design/shaders` in Node (the shader strings
  interpolate shared snippets at JS runtime; the shared vertex shader is not root-exported and
  is imported from the package's `dist/vertex-shader.js`).

## Recommendation for Phase 2 (macOS integration)

**Proceed.** The remaining work rides already-proven machinery (host-embedding spike #21:
Swift host, IOSurface, `view{}` external-texture compositing; carapace-ffi v1 #25).

Decide the host render path with this evidence in hand:

- **Option A — WGSL in wgpu (reuse Phase 1 output).** Feed the transpiled WGSL into a wgpu
  pass in the host, render to an IOSurface, composite via `view{}`. Straightforward reuse of
  everything above.
- **Option B — Metal/MSL native.** Since the transpile already goes through SPIR-V, emit MSL
  (naga has `msl-out`, or SPIRV-Cross) and render natively in the Swift host with no wgpu
  dependency. Often the cleaner macOS path.

Either way, note the **build-time vs runtime** decision: transpilation needs `glslang` (not
pure Rust). For a shipping `shader{}` primitive, prefer a **build-time transpile step**
(GLSL→WGSL/MSL baked at build) over shipping glslang in the runtime. Phase 1's ladder is fine
for a spike/dev tool; production should pre-transpile.

## Scope notes

- This was Phase 1 only (offscreen transpile + animated render). Out of scope and untouched:
  the `shader{}` Lua primitive, macOS integration, sandbox/trust hardening, general uniform
  mapping, audio-reactivity, perf tuning.
- Minor code-quality items logged for the eventual productionization (not blocking a spike):
  `via_spirv` uses non-unique temp filenames (concurrency race) with no cleanup; the harness's
  `shift_groups`/`entry_point_name` are string scanners assuming well-formed naga `wgsl-out`;
  `render_mesh`'s `frag_fields` param is currently unused.

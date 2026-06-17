# Phase 0 — Rendering Backend Decision

**Date:** 2026-06-17
**Outcome:** **vello** (0.9.0)

## Method

All three candidates (`tiny-skia`, `wgpu`+`lyon`, `vello`) were implemented behind a
single `Renderer` trait and held to one objective gate: every unambiguous
(non-antialiased) pixel a backend fills must agree with the `hittest` kernel's
independent inside/outside verdict, on both a concave L-shape and a holed ring. The
kernel has zero rendering dependency, proving the hit-test ↔ render decoupling holds
under each backend. A live `winit`/`softbuffer` viewer was then built so the backends
could be compared visually and the free-form hit-testing felt interactively before
committing.

## Result

All three **PASSED** the correctness gate with zero mismatches. The decision was made
on secondary criteria plus the visual comparison.

| Backend   | Integration LOC | Transitive crates | Cold build | Tessellator | API adjustments | API maturity |
|-----------|-----------------|-------------------|------------|-------------|-----------------|--------------|
| tiny-skia | 43              | ~3                | <1 s       | No          | 0               | Stable (0.12) |
| wgpu      | 266             | ~85               | ~25 s      | Yes (lyon)  | 6               | Churny (v29) |
| vello     | 147             | ~85 (shares wgpu) | ~27 s      | No          | 2 + 3 minor     | Pre-1.0      |

## Decision & rationale

**vello** was chosen. The spike's stated primary constraint — arbitrary free-form
hit-testing — is satisfied by all three and lives in the decoupled `hittest` module
regardless, so it was explicitly *not* the deciding factor (that was the point of the
decoupling). The decision therefore weighed the engine's broader direction:

- The engine is **GPU/shader-oriented** by decision 6 (desktop-first, consistent with
  the existing shader-explorer stack), and the "visualizer slot" host-extension
  concept is squarely shader territory. A CPU-only backend (tiny-skia) would force a
  separate GPU layer to be bolted on later for that work.
- Among the GPU options, **vello dominates raw wgpu** for this engine: it fills vector
  paths directly with no tessellator (the 266-LOC `lyon` burden disappears), while
  still exposing raw `wgpu` underneath for custom visualizer shaders. It needed far
  fewer API adjustments than raw wgpu (2 structural vs 6) and shares wgpu's dependency
  tree rather than adding a wholly separate one.
- The accepted costs are the full GPU dependency stack (~85 crates, ~27 s cold build)
  and vello's pre-1.0 API churn (it inherits wgpu's churn). These are acceptable for a
  desktop-first engine whose rendering performance is explicitly not the concern.

tiny-skia remains the natural fallback if the GPU/shader ambitions are later dropped
or if dependency weight / build time become painful — its simplicity advantage is
large (6× less code, ~3 deps, sub-second build, zero churn).

## What carries forward vs. what was thrown away

- **Carries forward:** the `hittest` kernel and the `Renderer` / `Pixmap` /
  `parity_check` contract (these seed the real `hittest` and `render` modules, design
  doc Phase 3); the chosen `vello_backend`; and the live `viewer` example (now
  vello-only), useful for feeling hit-testing in the Phase 1 prototype.
- **Thrown away:** the `tiny-skia` and `wgpu` backend implementations and their parity
  tests, plus the now-unused `tiny-skia`, `lyon`, and `bytemuck` dependencies
  (`wgpu` and `pollster` are retained because vello uses them).

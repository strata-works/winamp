# Responsive Frame Skins — Design (Spec 1 of 3)

**Date:** 2026-06-23
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** the post–live-host-view-region demo-apps program (3 sequenced specs). This is **Spec 1**.
Builds on the base vocabulary (5a–5d), the host-extension seam (5e), skin-as-window (Phase 6), and
the live host view region (`view{}`).

## The program (context, not all built here)

The user wants carapace to host real, resizable, semi-functional apps inside a skin. To keep each
artifact coherent and reviewable, the work is split into three sequenced specs, each its own
spec→plan→build:

1. **Responsive frame skins (THIS spec):** the engine capability — per-element anchors, a GPU-free
   layout pass, a `frame{}` 9-slice primitive, the logical-resize vs DPI-scale split, and a
   resizable demo window. Example: a file-browser **shell** (reflowing chrome + a static row list,
   not yet navigable).
2. **Interactive-app foundation (future):** a dynamic `list{}`/`repeat{}` primitive + demo-side
   pointer-input routing into `view{}` regions + `FileBrowserHost` → turns the shell into a live,
   navigable file browser.
3. **Music player (future):** audio playback + `MusicPlayerHost` + a playlist (reusing `list{}`) on
   the existing Headspace **gadget** skin → Headspace plays real audio.

Specs 2–3 are **out of scope here** and named only so this spec's boundaries are clear.

## Purpose

Add the **frame skin**: a second skin archetype that is a **resizable themed window**. Where a
**gadget skin** (Headspace, classic Winamp) is a fixed canvas of free-form bitmap art that can only
**scale uniformly** (resize = zoom the whole thing), a **frame skin** has chrome that docks to
window edges and a content region that **stretches** to fill — the prerequisite for hosting a real
resizable application's UI inside a skin. This spec delivers the responsive engine and proves it
with a reflowing example; it does not make the hosted content interactive (that is Spec 2).

The two archetypes are **additive**: gadget skins keep absolute free-form positioning and uniform
zoom; frame skins opt into anchors + stretch + 9-slice. Every existing skin renders identically.

## Scope

**In scope:**
- **Per-element anchors** — an optional `anchor` attribute on positioned primitives naming pinned
  window edges `{left, right, top, bottom}`, resolving to fixed/stretch behavior per axis.
- **A GPU-free layout pass** (`layout.rs`) — a pure function resolving design-space rects + anchors
  → concrete logical rects for the current window size, clamped to per-axis `min`.
- **The `frame{}` 9-slice primitive** — stretchable bitmap chrome (4 fixed corners, edges stretch,
  center `stretch|hollow`).
- **The logical-resize vs DPI-scale split** in `render.rs` — separate the window-resize factor
  (drives layout) from the display-density factor (drives crisp rendering), which today's single
  `sx = target.width / canvas.width` conflates.
- **Manifest archetype switch** — `resizable = true` + `min_size` (optional `max_size`) marks a
  frame skin (anchor reflow); its absence = gadget skin (uniform zoom), unchanged.
- **A resizable demo window** + an example frame skin (`skins/frame`) hosting a nested carapace
  **app-shell** skin (reflowing chrome + nav rail + a static file-row list).

**Out of scope (future specs / efforts):**
- **Dynamic lists** (`list{}`/`repeat{}`) and **pointer input routing into `view{}` regions** — Spec 2.
- **The file-browser behavior** (`FileBrowserHost`, real FS navigation) — Spec 2.
- **Audio / the music player** — Spec 3.
- **Container/flex layout** (rows/columns box tree) — rejected; it abandons carapace's free-form
  ethos. Anchors stay absolute-positioned.
- **Embedder-computed layout** — rejected; it denies skin *authors* responsive control.
- **Edge-tiling for 9-slice** — v1 stretches edges; tiling is a later option if art needs it.

## 1. Architecture & invariants

The headless/GPU split holds and is reinforced: **layout is pure geometry**, so the new `layout.rs`
joins `scene.rs`/`vocab.rs` on the GPU-free side; `render.rs` stays the only file that touches the
GPU. The engine carries no domain knowledge — anchors and 9-slice are geometry, not app meaning.

```
layout.rs (NEW)  # GPU-free. Anchors type; resolve(design_size, logical_size, design_rect, anchors)
                 #   -> logical_rect. Pure, branch-only, no constraint solver.
scene.rs         # positioned nodes carry design_rect + Anchors; new Node::Frame { image, dest,
                 #   slice, center }; Scene gains a resolve(logical) -> Scene producing concrete rects.
vocab.rs         # `anchor` attribute parsed on positioned prims; FramePrim (`frame{}`); manifest
                 #   `resizable`/`min_size`/`max_size`.
skin.rs          # manifest: resizable (bool), min_size, max_size.
render.rs        # draws the RESOLVED scene under a DPI-only xform; 9-slice = 9 image quads;
                 #   the view{} composite rect = resolved_logical_rect x dpi_scale.
engine           # owns current logical window size; runs the layout pass on resize; hands render
                 #   the resolved scene.
crates/carapace-demo  # resizable window from manifest archetype; skins/frame + a nested app-shell engine.
```

**Invariants preserved:** scene-as-projection, sandbox, transactional swap. Backward compatibility
is a hard invariant — gadget skins must render pixel-identical (golden-snapshot enforced, §4/§7).

## 2. Anchor semantics (`scene.rs`, `vocab.rs`, `layout.rs`)

The manifest `canvas` is the **design size**; authors position/size every element in those logical
units as today. Each positioned primitive gains an optional `anchor` = a subset of
`{left, right, top, bottom}`. The gap between the element's edge and the window's matching edge —
measured at design size — is held constant as the window resizes. Resolved per axis:

| Horizontal anchor | Behavior on width change |
|---|---|
| `left` only *(default)* | fixed width, fixed x — today's behavior |
| `right` only | fixed width, rides the right edge (fixed right gap) |
| `left` + `right` | both gaps fixed → **width stretches** |
| neither | fixed width, x re-centers proportionally |

Vertical is identical with `top`/`bottom`. Examples: title bar `{top,left,right}` (full-width,
fixed height); content region `{top,left,right,bottom}` (stretches both axes); status bar
`{bottom,left,right}`; fixed nav rail `{top,left,bottom}` (fixed width, full height).

```lua
-- a stretchable content region, never narrower than 240x160
view{ id = "app", x = 8, y = 32, w = 624, h = 380,
      anchor = { "left", "right", "top", "bottom" }, min = { w = 240, h = 160 } }
```

```
design 640x420            resized 900x560
┌───────────────┐         ┌─────────────────────┐
│ title  T,L,R  │         │ title               │   full-width, same height
├───────────────┤         ├─────────────────────┤
│ content        │        │ content             │   stretches both axes
│ T,L,R,B        │        │                     │
├───────────────┤         │                     │
│ status B,L,R   │        ├─────────────────────┤
└───────────────┘         │ status              │   full-width, pinned bottom
                          └─────────────────────┘
```

**Min/max.** The manifest declares `min_size` (optional `max_size`); the window enforces it. A
stretch region carries an optional per-axis `min` so it never collapses.

**Default & compatibility.** No `anchor` → `{top, left}` = fixed size, fixed top-left (today). A skin
without `resizable` ignores anchors entirely and uses uniform zoom. Existing skins are unaffected.

`Anchors` is plain data (`{left,right,top,bottom}: bool` + optional `min`), so the seam stays
FFI-friendly (consistent with the LHVR forward-compat goal).

## 3. The layout pass (`layout.rs`, GPU-free)

A pure resolution function, no GPU, no iteration:

```rust
// design_size, logical_size: (f64, f64); rect: design-space; anchors: per-element spec.
fn resolve(design: Size, logical: Size, rect: DesignRect, anchors: Anchors) -> LogicalRect
```

Per axis, independently, apply the §2 table: from which edges are pinned and the constant
design-space gaps, compute the logical origin and extent; clamp a stretched extent to the element's
`min`. A handful of arithmetic branches.

**Integration.** The engine owns a *current logical window size*. On a resize (and at startup) it
produces a **resolved `Scene`** with concrete logical rects, which `render.rs` consumes. The
resolution differs by archetype but the output shape is the same: a frame skin resolves per-element
anchors (above); a gadget skin applies a single uniform zoom (`logical = design × zoom`) — no
anchors. **Either way the engine emits a resolved logical scene and render applies DPI only (§4)** —
there is no zoom math left in render. This unification is what keeps render a dumb DPI rasterizer.

```
design Scene (design rects + anchors)
  │  engine.layout(logical_w, logical_h)     # GPU-free, on resize
  ▼
resolved Scene (concrete logical rects)
  │  renderer.draw(resolved, …, dpi_scale)   # GPU, DPI-only scale (§4)
  ▼
pixels + view{} regions composited (LHVR)
```

The property that makes this tractable: **anchors resolve once per resize in pure code**; render
stays a dumb rasterizer of concrete rects, independently unit-testable without a GPU.

## 4. The logical-resize vs DPI-scale split (`render.rs`)

Today `render.rs` computes one scale, `sx = target.width / canvas.width`, applied to everything.
That single number conflates two unrelated factors — how much bigger the window is than the design
(resize), and the display-density factor (DPI/retina). For a gadget skin both mean "bigger"; for a
frame skin they must come apart.

| | Logical resize | DPI scale |
|---|---|---|
| Source | user drags window → logical points | display density (retina = 2×) |
| Frame skin | drives the anchor **layout pass** (§3) | uniform render scale only |
| Gadget skin | drives uniform **zoom** | uniform render scale only |

**New render contract.** `draw` takes the resolved Scene (already in **logical** units) and a single
`dpi_scale = physical ÷ logical`. Its `xform` applies *only* `dpi_scale`. No layout math in render.

- **Frame skin:** the layout pass already stretched rects to the logical window size; render only
  sharpens by DPI. A 1-logical-px chrome line is 2 physical px on retina and does **not** grow when
  the window enlarges — correct app-window behavior.
- **Gadget skin:** the engine feeds render a scene whose rects are `design × zoom` (logical = canvas
  × user zoom); render applies DPI on top. Net scale = zoom × DPI — **identical pixels to today**,
  expressed as two factors instead of one. Enforced by golden snapshots (§7).

**LHVR composite folds in.** The `view{}` composite rect was `design × sx`; it becomes
`resolved_logical_rect × dpi_scale`. The embedder sizes its texture to the *resolved* view rect
(read via `Scene::views()`), as it already does. The monitor fit-fix (an authoring change) is
orthogonal and stays correct.

This is the riskiest change — a load-bearing number every skin flows through — so the golden-snapshot
net (gadget skins byte-identical) is the gate that makes the refactor provably behavior-preserving.

## 5. The `frame{}` 9-slice primitive (`scene.rs`, `vocab.rs`, `render.rs`)

A stretched chrome bitmap must keep crisp, fixed corners while edges stretch and the center fills or
stays hollow — 9-slice (nine-patch).

```lua
frame{ asset = "window.png", x=0, y=0, w=640, h=420,
       slice = { left=24, right=24, top=48, bottom=20 },  -- corner insets in the SOURCE bitmap
       center = "stretch",                                 -- "stretch" (default) | "hollow"
       anchor = { "left","right","top","bottom" } }        -- §2 anchors apply
```

`Node::Frame { image, dest, slice, center }` — `dest` is the resolved logical rect (§3), `slice` and
`center` are plain data. The image is cut into 9 cells: 4 corners (native size, never scaled), 4
edges (stretched along their long axis only), 1 center (stretched both axes, or skipped when
`hollow` so a `view{}` shows through). `render.rs` computes the 9 source/dest sub-rect pairs and
draws 9 image quads via the existing image path (corners at 1:1, edges/center scaled). Insets
exceeding the resolved rect clamp so opposing corners never overlap.

**A distinct primitive, not an `image{}` flag:** `image{}` stays the whole-bitmap blit (gadget art);
`frame{}` is the stretchable-chrome case with its own required `slice` field. Vector chrome
(`fill`/`rounded_rect` with anchors) remains available for skins that don't need bitmap borders.

## 6. The resizable demo (`crates/carapace-demo`)

**Window.** Resizable: `with_resizable(true)`, `min_inner_size` from manifest `min_size`, a
`WindowEvent::Resized` handler that reconfigures the surface, updates the engine's logical size,
re-runs the layout pass (§3), redraws. Drag/minimize/close reuse the Phase 6 `WindowOutbox`. The
demo selects resizable-vs-fixed from the manifest archetype, so gadget skins keep a fixed window and
Tab still cycles archetypes live (gadget zoom ↔ frame reflow).

**Example frame skin** (`skins/frame`): a title bar `{top,left,right}` (title + close/min hotspots),
hollow-center `frame{}` 9-slice borders anchored to all four edges, and a content
`view{ id="app" }` anchored `{top,left,right,bottom}` with a `min` (per §2).

**Hosted content (shell only, this spec).** A nested carapace engine (the live-monitor pattern)
renders an **app-shell skin that is itself a frame skin**: a header/toolbar `{top,left,right}`, a
left nav rail `{top,left,bottom}` (fixed width), and a content pane `{top,left,right,bottom}` holding
a **hand-authored static list of ~8 file-style rows** (plain `text` rows — no dynamic count). On
resize, the inner engine re-lays-out at the view's resolved size and repaints. **No clicking or
navigation** — that is Spec 2.

**Proves end to end:** drag the window → 9-slice borders stay crisp; title/status keep height and
span width; nav rail keeps width; content pane + rows reflow (not zoom). The anchor engine is
dogfooded twice (outer chrome + inner shell).

## 7. Error handling

- Invalid `anchor` token, malformed `slice`, or bad `min_size` → `BuildError` → transactional swap
  keeps the prior good scene.
- Window below `min_size` → prevented by winit `min_inner_size`; the layout pass never sees a
  sub-min size.
- A stretch region squeezed below its `min` → clamped; competing mins that can't all be honored
  clip the region at its edge — never a negative extent, never a panic.
- 9-slice `slice` insets exceeding the resolved rect → insets clamp so corners don't overlap.
- No `resizable` flag → gadget path runs unchanged; anchors ignored (not an error).
- Skin/embedder faults degrade; `unwrap` only on engine invariants.

## 8. Testing

- **Headless layout units (the bulk):** `resolve()` for every anchor combo per axis (left-only /
  right-only / both / neither; top/bottom analog), min-clamp, and a multi-element frame asserted at
  two sizes → exact rects. Pure, fast, no GPU.
- **9-slice geometry:** given `dest` + `slice`, assert the 9 source/dest sub-rect pairs, including
  the inset-clamp case.
- **Manifest parse:** `resizable` + `min`/`max` and defaults.
- **Backward-compat goldens:** every existing gadget skin renders **pixel-identical** after the
  logical/DPI refactor (the §4 safety net), via the snapshot/GPU harness.
- **GPU (gated `gpu-tests`):** a frame skin rendered at two sizes → a corner pixel block byte-identical
  (fixed) while edge/center spans grow; the `view{}` composites at the resolved rect.
- **Human:** resize the window — borders crisp, content reflows (not zooms); Tab cycles gadget(zoom)
  ↔ frame(reflow).

## Definition of done

Per-element anchors, the GPU-free layout pass, the `frame{}` 9-slice primitive, and the
logical-resize vs DPI-scale split all land; a resizable frame skin reflows its chrome and content
while gadget skins render pixel-identical; the demo window resizes with crisp 9-slice borders around
a reflowing app-shell; the headless boundary holds; and CI — `clippy -D warnings` (both feature
sets), `fmt`, the snapshot/GPU harnesses — is green. The second skin archetype exists end to end,
ready for Spec 2 to make the hosted content interactive.

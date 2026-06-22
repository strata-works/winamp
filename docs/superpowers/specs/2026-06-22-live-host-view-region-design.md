# Live Host View Region — Design

**Date:** 2026-06-22
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Post–Phase-6 — the capstone capability the whole stack was built to enable. Builds on
the base vocabulary (5a–5d), the host-extension seam (5e), and skin-as-window (Phase 6).

## Purpose

Add the **live host view region**: a skin declares a region the *embedding application* fills with
its own live pixels, and carapace composites that content into the rect and draws the skin chrome
around it. This is the seam by which **an app embeds carapace as a library and "wears" a skin** —
the app's UI (or a visualizer, video, monitor) renders *inside* the skin, framed by it. It is the
difference between a skin engine that draws chrome *next to* content and one that hosts content
*inside* the skin.

**North star (kept in mind, not built here):** carapace as a cross-framework skinning SDK — a
stable C ABI plus per-framework adapters (SwiftUI, Flutter, web, native). This seam is the
prerequisite for all of it. The design keeps the view API plain-data and FFI-friendly (rects are
values; the embedder's content is an opaque texture handle) so a future C ABI slots on without
reshaping it. **No C ABI or framework adapter is built in this spec.**

## Scope

**In scope:**
- A **`view{ id, x, y, w, h }`** base-vocab primitive → `Node::View { id, dest }` (plain data,
  headless). The skin owns *where* host content goes and how big.
- **`Scene::views() -> Vec<(String, ImageDest)>`** — the engine exposes declared regions to the embedder
  (so it can render content at the right place/size, and so an *external* compositor can position
  its own content over the hole).
- **Render:** a `Node::View` draws **nothing** — a transparent hole in the skin render.
- **Optional engine composite:** `Renderer::draw` gains a per-frame `view_tex` source; for each
  `Node::View` whose id resolves to an embedder-supplied **wgpu texture**, the engine composites
  that texture into the view's rect (zero-copy, shared device) via a textured-quad pass. If no
  texture is supplied for a view, the hole is left as-is for an external compositor to fill.
- **Demo:** the Headspace skin declares `view{ id="display" }` over its (currently black) screen;
  the demo App paints a **live CPU/system monitor** into that view's texture each frame — the black
  screen becomes a live monitor *inside* the floating skin.
- A geometry-neutral `summary()` line.

**Out of scope (separate efforts / future):**
- **Hosting foreign, separate-process apps** (reparenting OS windows). Brutally platform-specific;
  a separate feasibility spike, likely unnecessary given the embed-carapace model.
- **CPU-pixel transport** (readback per frame). GPU texture only — required for real-time content.
- **View-under-translucent-skin-overlays** (a glass sheen *over* the content) — needs a two-pass
  render; v1 composites the view into its hole with the skin framing *around* it.
- **The C ABI and per-framework adapters** — the north-star program; each its own brainstorm/spike.
- **Multiple textures blended per view, view input/hit-testing inside the region** — the region's
  *content* handles its own input (the embedder owns it); the skin's hotspots are unchanged.

## 1. Architecture & invariants

The headless/GPU split holds: `scene.rs`/`vocab.rs` stay GPU-free; only `render.rs` touches the GPU.
The engine composites a **generic texture** into a **generic rect** — zero knowledge of what it is
(app UI / video / monitor). The neutrality thesis holds; this is a larger engine change than Phase 6
(a wgpu composite pass) but carries no domain or app meaning.

- **The hole + rect is the foundation; the texture composite is an opt-in.** `view{}` always leaves
  a transparent hole and exposes its rect via `Scene::views()`. A Rust/wgpu embedder hands carapace
  a texture and gets the composite for free (the demo path). A non-wgpu embedder (SwiftUI/Flutter)
  ignores the composite and overlays its own content at the exposed rect with its own renderer (the
  external-compositor path). One primitive, both modes.
- **Embedder owns content; domain `Host` stays clean.** Painting the region is the *embedder's* job
  (a GPU/app concern), exactly as window control became the embedder's job in Phase 6. The domain
  `Host` trait (state/actions) is unchanged.
- **Forward-compatible for the future C ABI:** rects are plain `(f32,f32,f32,f32)` data; the
  embedder's content crosses as an opaque GPU-texture handle. No Rust-only types leak into the seam
  shape.
- **Scene-as-projection, sandbox, transactional swap** — unchanged. A `view{}` binds nothing
  (its content is external); it is static layout.

```
scene.rs   # new Node::View { id: String, dest: ImageDest }; Scene::views() accessor; summary() line.
           #   (reuses the existing ImageDest {x,y,w,h} from the image primitive.)
vocab.rs   # ViewPrim: view{ id, x, y, w, h } -> Node::View (required fields -> MissingField).
render.rs  # Node::View draws nothing; Renderer::draw gains a `view_tex` source + a textured-quad
           #   composite pass that draws each supplied texture into its view rect (canvas->surface).
crates/carapace-demo/...  # Headspace skin gains view{id="display"}; the App renders a live monitor
           #   to a texture each frame and supplies it; the screen shows the monitor in the skin.
```

## 2. The `view{}` primitive (`scene.rs`, `vocab.rs`)

```rust
// scene.rs — reuse the existing rect shape (ImageDest) or a dedicated ViewDest with the same fields.
Node::View {
    id: String,                 // matches the key the embedder supplies content under
    dest: ImageDest,            // { x, y, w, h } in canvas coords (authoring space)
}
```

`vocab.rs` `ViewPrim` (id `"view"`): reads `id` (string), `x`, `y`, `w`, `h` (all required →
`BuildError::MissingField`); emits `vec![Node::View { id, dest }]`. Registered in
`VocabRegistry::base()` (now 6 base primitives). Author usage:

```lua
view{ id = "display", x = 78, y = 50, w = 186, h = 150 }
```

`Scene::views() -> Vec<(String, ImageDest)>` collects every `Node::View`'s `(id, dest)` (canvas
coords) — the embedder uses these to know what content to render and where. (Canvas coords; the
embedder scales to surface resolution using the same canvas→window ratio it already owns for sizing
its content texture crisply.)

## 3. Render & composite (`render.rs`)

- **Skin render (unchanged path):** vello draws the scene to `target.view` (the intermediate). A
  `Node::View` arm draws **nothing** — leaving the rect transparent (a hole), so the composite (or
  an external compositor) can fill it.
- **Composite pass (new):** `Renderer::draw` gains a content source:
  ```rust
  pub fn draw(
      &mut self,
      scene: &Scene,
      read_value: impl Fn(&str) -> Option<StateValue>,
      view_tex: impl Fn(&str) -> Option<&wgpu::TextureView>,   // NEW: per-view content
      target: &RenderTarget,
  )
  ```
  After the vello pass, for each `Node::View { id, dest }` where `view_tex(id)` is `Some(tex)`, the
  engine composites `tex` into the view's **surface-space** rect (`dest` × the canvas→surface scale)
  with a small textured-quad render pass (`LoadOp::Load`, no clear, a hardware sampler so the content
  scales to the rect). A view with no supplied texture is left transparent (external-compositor mode).
- **Transport:** the embedder's `wgpu::TextureView` is on the **same device** as the engine
  (zero-copy). GPU only; no CPU readback.
- **Target requirement:** because the composite is a render pass into `target.view`, the embedder
  creates that texture with `RENDER_ATTACHMENT` usage **in addition to** what vello needs
  (`STORAGE_BINDING`) and the blit needs (`TEXTURE_BINDING`). Documented as an embedder requirement;
  the demo's intermediate adds it. (Implementation may instead use a compute composite to avoid the
  `RENDER_ATTACHMENT` requirement — the plan picks; the contract is "engine composites the texture
  into the rect.")
- The composite uses the **same canvas→surface `xform`** as the skin, so the content tracks the view
  rect when the window scales.

## 4. `scene::summary()`

Geometry-neutral, deterministic — node kind + id only, no coordinates:

```
view id=<id>
```

## 5. The demo — a live monitor inside the Headspace screen

- The Headspace skin (`reference`) declares `view{ id="display", … }` over the faceplate's display
  rect. The bitmap's display is opaque black; the composited monitor texture (opaque) covers it, so
  **no further bitmap edit is needed**.
- The demo App, each frame: reads `engine.scene().views()`, renders a **live CPU/system monitor**
  (e.g. bars + a scrolling history graph) into a wgpu texture sized to the view rect (at surface
  resolution for crispness), and passes a `view_tex` closure returning that texture for `"display"`.
  The App draws the monitor with **its own wgpu** (arbitrary pixels — demonstrating the region is not
  vocab-limited); it reads metrics from the existing `sysinfo` path (`SysmonHost`/a small sampler).
- Result: the black Headspace screen becomes a live monitor *inside* the floating, draggable skin —
  the embed-carapace pattern proven end to end (the monitor is the stand-in for "an app's own UI").
- The intermediate texture gains `RENDER_ATTACHMENT` usage so the engine can composite.

## 6. Testing

**Headless (no GPU):**
- `view{}` parse: required `id`/`x`/`y`/`w`/`h`; missing any → `MissingField`; emits `Node::View`.
- `Scene::views()` returns the declared `(id, dest)`s; a scene with no views → empty.
- `summary()`: the `view id=<id>` line (geometry-free), snapshot-stable.
- `base()` registry count grows by one (now 6).

**Gated GPU (`gpu-tests`):**
- Compose check: a scene with a `view{}` over a sub-rect; supply a **known solid-color** texture for
  that id; assert a pixel **inside** the view rect reads that color and a pixel **outside** (on a
  skin fill) reads the skin's — proving the composite lands in the rect and the frame survives.
- No-texture check: the same scene with `view_tex` returning `None` leaves the view rect at the base
  color (the hole) — proving the optional/external-compositor path.

**Human:** `cargo run -p carapace-demo` → the Headspace skin's screen shows a **live CPU monitor**
inside the floating window; drag/close still work; Tab/H still cycle skins/domains.

## 7. Error handling

- Malformed `view{}` (missing field) → `BuildError` → transactional swap keeps the prior scene.
- A `Node::View` whose id has no supplied texture → transparent hole, no panic (external-compositor
  mode or simply empty).
- A target texture lacking the composite usage when a texture is supplied → the embedder's setup
  error (documented requirement); the engine does not silently mis-render — pipeline creation fails
  loudly at the embedder, not at runtime per frame.
- No panics on a skin/embedder fault; the engine returns/degrades. `unwrap` only on engine invariants.

## Definition of done

`view{}` declares a region (`Node::View`), `Scene::views()` exposes the rects, and `Renderer::draw`
composites an optional embedder-supplied wgpu texture into each view rect (leaving a transparent hole
when none is supplied); the demo's Headspace screen hosts a **live, embedder-painted CPU monitor**
inside the floating skin; the headless boundary, the fast `check` CI job (incl. `clippy -D warnings`,
both feature sets), `fmt`, and the snapshot/GPU harnesses are all green. The seam that lets an app
embed carapace and render its own UI inside a skin exists end to end.

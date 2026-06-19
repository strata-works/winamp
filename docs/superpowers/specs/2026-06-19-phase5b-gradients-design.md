# Phase 5b — Gradient Fills — Design

**Date:** 2026-06-19
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Phase 5 (base vocabulary + host extensions + assets), decomposed — **5b is the
second sub-project**, after 5a (asset loading + `image`). Builds on the Phase 2 vocab seam,
the Phase 3 engine + render, and the 5a vocabulary set.

## Purpose

Add **gradient fills** to the engine: any `fill{}` can be painted with a solid color **or**
a linear/radial/sweep gradient, and colors gain an **alpha channel** for translucency. This
is the Y2K chrome/sheen layer — the glassy highlights and metallic bars that, laid over the
5a bitmap faceplate, give skins their period look.

Gradients are introduced through a small **`Paint` abstraction** (`Solid | Gradient`) so the
existing `fill{}` primitive is generalized rather than duplicated — one primitive paints any
path with any paint.

### Phase 5 decomposition (recorded; 5b is second)

| Sub-project | Adds | Status |
|---|---|---|
| 5a | asset resolver + `image` primitive | done |
| **5b (this doc)** | `Paint` (solid + linear/radial/sweep gradient) + color alpha | this spec |
| 5b-mesh (next) | mesh gradient (`Paint += Mesh`) via CPU bake → texture | deferred, own spec |
| 5c | text + fonts (reuses the 5a asset resolver) | later |
| 5d | vocab ergonomics: shape helper, shared draw+hotspot geometry, value-fill direction | later |
| 5e | host-extension mechanism | later |

## Scope

**In scope:** a `Paint = Solid(Color) | Gradient` abstraction; `Color` gains an optional
alpha channel; three **native** gradient kinds (linear, radial, sweep) rendered via peniko
brushes; generalizing `fill{}` to accept `color={…}` (solid, unchanged) or `gradient={…}`;
render of gradient paints through vello; a domain-neutral `summary()` line; and gradient
accents in the demo skins.

**Out of scope (later 5x / phases):**
- **Mesh gradients** — peniko/vello have no native mesh primitive; mesh needs a separate
  CPU-bake-to-texture path (Coons/bilinear interpolation → RGBA8 → the 5a image draw path).
  That is the **next sub-project** (its own spec), not 5b.
- **Gradient-filled `value_fill`** — the value-clipped sub-rect interacts with gradient
  geometry in ways that need their own reasoning; `value_fill` stays solid in 5b (it still
  benefits from the shared `Color` alpha).
- **Value-bound / animated gradients** — gradients are static style in 5b.
- **Extend modes** beyond Pad (repeat/reflect) and the **two-circle** radial form.

## 1. Architecture & invariants

`Paint`, `Color`, and `Gradient` are **plain data** in `scene.rs`. Parsing happens in
`vocab.rs` (headless, no GPU). Only `render.rs` turns a `Paint` into a peniko brush and
touches the GPU. This preserves the established split:

- **Headless boundary intact.** A `Node::Fill` carries a `Paint` (plain data); existing
  headless skin-build/scene tests construct gradient fills with no GPU.
- **Scene = pure projection of state.** Gradients are **static** in 5b — they bind no state
  key — so the "scene binds keys, never values; rebuilt from state" rule is untouched.
- **Transactional swap, capability sandbox, zero domain knowledge** — all unaffected; a
  gradient is generic style, no media meaning.

```
scene.rs   # Color gains `a`; new ColorStop, Gradient (Linear/Radial/Sweep), Paint;
           #   Node::Fill { path, color } -> Node::Fill { path, paint: Paint }; summary() lines
vocab.rs   # parse_color gains optional alpha; new parse_gradient + parse_paint;
           #   FillPrim::build uses parse_paint (color= OR gradient=)
render.rs  # paint -> peniko brush (Solid -> VColor w/ real alpha; Gradient -> peniko::Gradient);
           #   vcolor's hardcoded alpha=255 removed
crates/carapace-demo/skins/   # gradient accents: sheen+glossy over reference; sweep on a vector skin
```

## 2. Data model (`scene.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }   // a defaults to 255 at parse time

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop { pub at: f32, pub color: Color }            // at in [0,1]

#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    Linear { from: Pt, to: Pt, stops: Vec<ColorStop> },
    Radial { center: Pt, radius: f32, stops: Vec<ColorStop> },    // single circle: start_radius 0, end_radius = radius
    Sweep  { center: Pt, start_deg: f32, end_deg: f32, stops: Vec<ColorStop> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint { Solid(Color), Gradient(Gradient) }

pub enum Node {
    Fill { path: Vec<Pt>, paint: Paint },   // CHANGED from { path, color: Color }
    Hotspot { region: Region, on_press: HandlerId },
    ValueFill { path: Vec<Pt>, value_key: String, color: Color },  // stays Solid in 5b
    Image { image: Arc<DecodedImage>, dest: ImageDest },
}
```

Stops are stored already validated and sorted (see §3). Angles stored in **degrees** in the
scene (author-facing unit); converted to radians only at render.

## 3. Lua / vocab API (`vocab.rs`)

- **`parse_color`** gains an optional `a` field: `c.get("a").unwrap_or(255)`. Solid skins are
  fully back-compat (no `a` → opaque).
- **`parse_gradient(t: &Table)`** reads:
  - `type`: `"linear" | "radial" | "sweep"` (any other → `BuildError::BadType`).
  - geometry by type: linear → `from{x,y}`, `to{x,y}`; radial → `center{x,y}`, `radius`;
    sweep → `center{x,y}`, optional `start_deg` (default 0), `end_deg` (default 360).
  - `stops`: a sequence of `{ at = <0..1>, color = {…} }`. **Requires ≥ 2 stops**
    (`BuildError::BadType` otherwise). Each `at` is clamped to [0,1]; stops are **sorted by
    `at`** (stable) so authors can list them in any order.
- **`parse_paint(args: &Table)`**: if a `gradient` table is present → `Paint::Gradient(parse_gradient)`;
  else → `Paint::Solid(parse_color)`. Missing both → the existing `MissingField("color")`.
- **`FillPrim::build`** → `Node::Fill { path: parse_path(args)?, paint: parse_paint(args)? }`.

```lua
-- solid (unchanged)
fill{ path = {...}, color = {r=20, g=30, b=40} }

-- linear glass sheen (translucent white, top-down)
fill{ path = {...}, gradient = {
  type = "linear", from = {x=0, y=0}, to = {x=0, y=40},
  stops = { {at=0, color={r=255,g=255,b=255, a=120}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }

-- radial glossy highlight
fill{ path = {...}, gradient = {
  type = "radial", center = {x=20, y=20}, radius = 20,
  stops = { {at=0, color={r=255,g=255,b=255, a=200}},
            {at=1, color={r=120,g=160,b=255, a=0}} } } }

-- sweep (angles in degrees; default 0..360)
fill{ path = {...}, gradient = {
  type = "sweep", center = {x=20, y=20}, start_deg = 0, end_deg = 360,
  stops = { {at=0, color={r=255,g=0,b=0}}, {at=0.5, color={r=0,g=255,b=0}},
            {at=1, color={r=255,g=0,b=0}} } } }
```

A malformed gradient (bad `type`, missing geometry, <2 stops) → `BuildError` → caught by the
**transactional swap** (skin fails to load; prior scene stays).

## 4. Render (`render.rs`)

A `paint_brush(paint: &Paint) -> peniko::Brush` (or equivalent `BrushRef`) helper:

- `Paint::Solid(c)` → `VColor::from_rgba8(c.r, c.g, c.b, c.a)` — alpha now real (the old
  `vcolor` that hardcoded `255` is removed).
- `Paint::Gradient(g)` → a `peniko::Gradient`:
  - Linear → `Gradient::new_linear(from, to)`
  - Radial → `Gradient::new_radial(center, radius)` (single circle)
  - Sweep → `Gradient::new_sweep(center, start_deg.to_radians(), end_deg.to_radians())`
  - `.with_stops(&[ColorStop { offset: at, color: from_rgba8(...) }, …])` (peniko
    `ColorStop`/`ColorStops`), extend = Pad (peniko default).

Gradient coordinates are **canvas-space** (same coordinate system as paths) and drawn under
the same canvas→surface `xform` as every other node, so a gradient scales with the skin.
`Node::Fill` draws with `vs.fill(Fill::NonZero, xform, &brush, None, &bez(path))`.

## 5. `scene::summary()`

Domain-neutral, deterministic, **no float geometry** (so snapshots stay stable):

- Solid fill → `fill rgba=<r>,<g>,<b>,<a>`  (the existing `fill rgb=…` line changes to
  `rgba=`; the existing snapshot test updates to match.)
- Gradient fill → `fill gradient=<kind> stops=<n>`  (e.g. `fill gradient=linear stops=2`).
- `value_fill` → `value_fill key=<k> rgba=<r>,<g>,<b>,<a>` (also gains alpha via shared `Color`).
- `image` line unchanged.

## 6. Demo payoff

The `reference` (Headspace) skin gains the authentic Y2K touch, gradients laid **over** the
bitmap:
- a **translucent linear "glass sheen"** across the header band (white, `a` 120→0),
- a **radial glossy highlight** near the play hotspot.

**Sweep** reads poorly over a photo, so one vector skin (`classic` or `minimal`) gets a
**sweep-filled disc** — so all three gradient kinds render live in `cargo run -p carapace-demo`.

## 7. Testing

**Headless (no GPU):**
- `parse_color`: missing `a` → 255; explicit `a` honored.
- `parse_gradient`: each kind parses geometry + stops correctly; `type` other than the three
  → `BadType`; **<2 stops → `BadType`**; out-of-order stops are sorted; `at` clamped to [0,1];
  sweep `start_deg`/`end_deg` default to 0/360.
- `parse_paint` / `FillPrim`: `gradient=` → `Paint::Gradient`; `color=` → `Paint::Solid`;
  neither → `MissingField("color")`.
- `summary()`: the `rgba=` solid line, the `gradient=<kind> stops=<n>` line, and `value_fill`
  alpha line are stable (snapshot updated).

**Gated GPU (`gpu-tests` feature; lavapipe CI / local Metal):**
- `render_offscreen` gains a **linear-gradient** case: draw a known 2-stop horizontal
  gradient over the canvas, sample an interior midpoint, assert the interpolated color within
  a tolerance (catches stop/geometry/gamma regressions).
- a **translucent-fill** case: a 50%-alpha solid drawn over a known opaque background blends
  to the predicted value within tolerance (proves alpha is wired through, not dropped).

**Snapshot harness:** continues via the `summary()` lines (no RGBA hashing).

**Human:** `cargo run -p carapace-demo` → the `reference` skin shows the sheen + glossy
highlight over the Headspace photo; the vector skin shows the sweep disc; Tab-swap survives.

## Error handling

- Malformed gradient/paint at build → `BuildError` → transactional swap keeps the prior scene.
- No panics on a skin/asset/paint fault; engine returns `Result` / degrades. `unwrap` only on
  engine invariants (as today).

## Definition of done (5b)

`Paint` exists; `fill{}` paints solid or linear/radial/sweep gradients; `Color` carries
alpha and translucent fills/sheen render correctly (the GPU color/alpha sentinels pass); the
demo `reference` skin wears a glass sheen + glossy highlight over the Headspace bitmap and a
vector skin shows a sweep disc; the headless boundary, the fast `check` CI job, and the
snapshot harness are all unchanged/green.

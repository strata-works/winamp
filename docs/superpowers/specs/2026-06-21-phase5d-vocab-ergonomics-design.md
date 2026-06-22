# Phase 5d — Vocab Ergonomics — Design

**Date:** 2026-06-21
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Phase 5 (base vocabulary + host extensions + assets), decomposed — **5d is the
fourth sub-project**, after 5a (assets/`image`), 5b (`Paint`/gradients), and 5c (text/fonts).
Builds on the Phase 2 vocab seam and the Phase 3 engine + render.

## Purpose

Resolve the author-DX debts the prototype surfaced (Phase 1 lessons #2 and #3) plus the
shape-authoring papercut, in one cohesive **vocabulary ergonomics** pass:

1. **Shape helpers** — `circle`, `rect`, `rounded_rect` as composable path-generators, so authors
   stop hand-writing polygon points for the shapes every skin needs.
2. **Shared draw+hotspot geometry** — a drawable (`fill`/`image`) gains an optional `on_press`, so
   a clickable control is declared **once** instead of as a duplicated `region{}` + `fill{}` pair
   (Phase 1 lesson #3 — geometry declared twice, can silently drift).
3. **`value_fill` direction + clip-to-path** — an explicit fill `direction` and filling the
   **actual free-form region** (clip to the path), not just its bounding box (Phase 1 lesson #2).

These are domain-neutral ergonomics on the existing vocabulary; no new domain knowledge enters
the engine.

### Phase 5 decomposition (recorded; 5d is fourth)

| Sub-project | Adds | Status |
|---|---|---|
| 5a | asset resolver + `image` primitive | done |
| 5b | `Paint` (solid + linear/radial/sweep gradient) + color alpha | done |
| 5c | text + fonts (`text{}`, parley, value-bound strings) | done |
| **5d (this doc)** | shape helpers; `on_press` on drawables; `value_fill` direction + path clip | this spec |
| 5e | host-extension mechanism (`VocabRegistry::register` flow + a demo extension) | later |

## Scope

**In scope:**
- A headless **`shape.rs`** with pure `circle`/`rect`/`rounded_rect` → `Vec<Pt>` generators, and
  Lua wrappers injected into the skin sandbox that return point-tables usable in any `path=`.
- An optional **`on_press`** on `fill{}` and `image{}` that emits a `Hotspot` from the drawn
  geometry alongside the draw node — powered by changing `Primitive::build` to emit **`Vec<Node>`**.
- **`FillDir`** (`Right`|`Left`|`Up`|`Down`) on `Node::ValueFill`, parsed from a `direction` field
  (default `"right"`), and a render that **clips the fill to the path** (path ∩ value-extent).
- A geometry-neutral `summary()` update (`value_fill … dir=<d> …`).
- Demo payoff: the `classic` skin refactored to use all three ergonomics live.

**Out of scope (later 5x / phases):**
- Diagonal/angled fills (only the four cardinal directions).
- **Clickable `value_fill`** (click-to-seek needs a value-from-click mapping + a computed host-action
  arg) — `value_fill` stays display-only.
- More shapes (ellipse, arbitrary N-gon, star, polyline) and per-shape stroke/outline.
- Non-rect hit regions for `image{ on_press }` (image hotspots derive from the dest rect).
- Region acceleration structures (Phase 1 lesson #5 — region caching — was already addressed in
  Phase 3: `Node::Hotspot` stores its `Region`, and `Scene::hit` uses it).

## 1. Architecture & invariants

The headless/GPU split holds exactly as in 5a–5c. **Geometry and parsing are headless**
(`shape.rs`, `vocab.rs`, `scene.rs`); only `render.rs` touches the GPU.

- **Headless boundary intact.** `shape.rs` is pure math (no Lua, no GPU) and unit-tested directly.
  Shape *injection* (Lua wrappers) lives in `script.rs` but calls only `shape.rs`. `Node::ValueFill`
  carries a plain `FillDir`; the clip is a render-only concern.
- **Scene = pure projection of state.** `value_fill` still binds a key and resolves at render; the
  new `direction` is static style. Shapes and `on_press` are build-time geometry, not state.
- **Domain-neutral.** Shapes, directions, and shared hotspots carry no media meaning. A `circle` is
  geometry; `on_press` is still an allowlisted host action; `direction` is generic style.
- **Sandbox unchanged in spirit.** Shape helpers are new *allowlisted* names injected into the env
  (like the primitive constructors); they expose only pure geometry, no capability. `io`/`os`/
  `require`/`load` stay absent.
- **Transactional swap** unaffected — a malformed shape/`on_press`/`direction` is a `BuildError`,
  caught by the swap (prior skin stays).

```
shape.rs    (new) # pure circle/rect/rounded_rect -> Vec<Pt> (headless, GPU-free, Lua-free)
scene.rs          # FillDir enum; Node::ValueFill gains `direction`; summary() line gains dir=
vocab.rs          # Primitive::build -> Result<Vec<Node>, BuildError>; fill/image emit optional
                  #   Hotspot from on_press; parse_direction; ValueFillPrim reads direction
script.rs         # build loop extends with Vec<Node>; injects circle/rect/rounded_rect Lua helpers
render.rs         # value_fill: clip to path (push_layer) + direction-aware value-extent rect
crates/carapace-demo/skins/classic  # refactored to use shapes + on_press + a vertical meter
```

## 2. Shape helpers (`shape.rs`, `script.rs`)

```rust
// shape.rs — pure, headless. Angles tessellated to polygons (even-odd / polyline consistent).
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Pt>;            // 4 corners, CW from top-left
pub fn circle(cx: f32, cy: f32, r: f32, segments: u32) -> Vec<Pt>; // `segments` points on the radius
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, segments: u32) -> Vec<Pt>;
//   ^ 4 corner arcs of `segments` points each => 4*segments vertices total; the straight sides are
//     the polygon edges between adjacent arc endpoints (no extra vertices). `radius` clamped to
//     min(w,h)/2.
```

Lua wrappers in `script.rs` (Rust closures injected into the env), each reading its arg table and
returning a Lua sequence of `{x=…, y=…}`:

- `circle{ cx, cy, r, segments = 48 }`
- `rect{ x, y, w, h }`
- `rounded_rect{ x, y, w, h, radius, segments = 8 }`  (`segments` per corner)

Missing required fields → a Lua error from the wrapper (surfaced as a `BuildError::Lua` when the
enclosing primitive consumes the bad path, or directly if the helper is called with garbage). The
helpers return **plain paths**, so they compose with every path consumer:

```lua
fill{ path = rect{x=20,y=20,w=70,h=70}, color = C, on_press = function() host.toggle_play() end }
fill{ path = rounded_rect{x=20,y=20,w=70,h=70,radius=8}, color = C }
region{ path = circle{cx=50,cy=50,r=30}, on_press = function() host.stop() end }
value_fill{ path = rect{x=20,y=110,w=260,h=16}, value = "position", color = C }
```

Shapes are **base sugar**, always injected (not host-extensible vocab). They emit no nodes.

## 3. Shared draw+hotspot geometry (`vocab.rs`, `script.rs`)

`Primitive::build` changes shape so one declaration can contribute several nodes:

```rust
pub trait Primitive {
    fn id(&self) -> &str;
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError>;
}
```

- Existing prims (`region`, `value_fill`, `text`) return `Ok(vec![node])` (mechanical wrap).
- **`fill{}`** and **`image{}`** read an optional `on_press`. With it present:
  - register the handler via the existing `ctx.register_handler(f) -> HandlerId`,
  - build the draw node **and** a `Node::Hotspot { region, on_press }` where `region` =
    `region_of(path)` for `fill` (and `region_of` of the dest rect's 4 corners for `image`),
  - return `vec![draw_node, hotspot]`.
  - Absent `on_press` → `vec![draw_node]` (unchanged behavior).
- **`script.rs`** build-loop: `b.nodes.extend(prim.build(...)?)` instead of `push`.

`region{}` is unchanged — the way to declare an **invisible** hotspot (e.g. over a bitmap, as the
`reference` skin does). The draw node is emitted before its hotspot; `Scene::hit` already iterates
`rev()` and returns the first containing hotspot, so ordering is correct.

A helper `fn maybe_hotspot(args, region, ctx) -> Result<Option<Node>, BuildError>` keeps the
`on_press` logic DRY across `FillPrim` and `ImagePrim`.

## 4. `value_fill` direction + clip-to-path (`scene.rs`, `vocab.rs`, `render.rs`)

```rust
// scene.rs
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillDir { Right, Left, Up, Down }

Node::ValueFill {
    path: Vec<Pt>,
    value_key: String,
    color: Color,
    direction: FillDir,   // NEW; default Right at parse time
}
```

- **`vocab.rs`:** `parse_direction(args)` reads `direction`: absent | `"right"` → `Right`;
  `"left"` → `Left`; `"up"` → `Up`; `"down"` → `Down`; anything else → `BuildError::BadType`.
- **`render.rs`:** the fill is **path ∩ value-extent**, drawn under the canvas→surface `xform`:
  1. Compute the path bbox `(x0,y0,x1,y1)` and the resolved value `v ∈ [0,1]` (existing `value_of`).
  2. Value-extent rect by direction:
     - `Right`: `Rect(x0, y0, x0 + (x1-x0)*v, y1)`  (today's behavior)
     - `Left` : `Rect(x1 - (x1-x0)*v, y0, x1, y1)`
     - `Up`   : `Rect(x0, y1 - (y1-y0)*v, x1, y1)`  (grows from the bottom upward)
     - `Down` : `Rect(x0, y0, x1, y0 + (y1-y0)*v)`
  3. Clip to the path: `vs.push_layer(Mix::Normal/Compose::SrcOver, 1.0, xform, &bez(path))`, then
     `vs.fill(NonZero, xform, color, None, &extent_rect)`, then `vs.pop_layer()`.
  - For a rectangular `value_fill` (bbox == path), the clip is a no-op and `Right` reproduces the
    current output exactly — **no visual regression** for existing seek bars.

`value_fill` remains solid-color (the 5b note: gradient value_fill is still deferred).

## 5. `scene::summary()`

Geometry-neutral, deterministic. The `value_fill` line gains `dir=`:

```
value_fill key=<k> dir=<d> rgba=<r>,<g>,<b>,<a>     # <d> in right|left|up|down
```

`fill`/`image` summaries are unchanged; an emitted hotspot prints its existing
`hotspot handler=<id>` line (so a `fill{ on_press }` now contributes both a `fill …` and a
`hotspot …` line — visible, deterministic). Shapes leave no summary trace (they are just paths fed
to a primitive). The existing snapshot tests update for the `dir=` field.

## 6. Demo payoff

The **`classic`** skin is refactored to exercise all three ergonomics in one file:

- each `region{}` + `fill{}` button-pair collapses into a single
  `fill{ path = rect{…}, color = …, on_press = … }` (kills the lesson-#3 duplication live),
- a `rounded_rect` chrome button and a `circle` knob (shape helpers),
- a **vertical** `value_fill{ direction = "up" }` meter bound to `position` alongside the existing
  horizontal seek bar (fill direction).

The refactor is behavior-preserving (same hotspots fire the same host actions); the demo's
`skins_build` test asserts the rebuilt scene still has the expected node kinds.

## 7. Testing

**Headless (no GPU):**
- `shape.rs`: `rect` → 4 corners at the right coordinates; `circle(cx,cy,r,n)` → `n` points each at
  distance `r` from `(cx,cy)` (within tolerance); `rounded_rect` → `4 + 4*segments` points, corners
  within `radius` of the box, total `4*segments` vertices, `radius` clamped to `min(w,h)/2`.
- `vocab`: `fill{ on_press }` → `[Fill, Hotspot]` with `region_of(path)` matching; `fill` without
  `on_press` → `[Fill]`; `image{ on_press }` → `[Image, Hotspot]` from the dest rect; existing prims
  return single-element vecs.
- `parse_direction`: default `Right`; each string; bad → `BadType`.
- `summary()`: `dir=` snapshot; a `fill{ on_press }` produces both `fill` and `hotspot` lines.
- Shape-injected skin builds (a `circle`/`rect`/`rounded_rect` path feeds `fill`/`region`).
- `Scene::hit` returns the handler for a point inside a `fill{ on_press }`'s shape.

**Gated GPU (`gpu-tests`):**
- `direction="up"`: a vertical `value_fill` with `v=0.5` fills the **bottom** half (sentinel pixel
  low = color, high = background).
- **clip-to-path**: a non-rectangular `value_fill` (e.g. a `circle` path) at `v=1.0` leaves a pixel
  that is inside the bbox but **outside** the circle as background (proves the clip, not bbox).

**Snapshot harness / fast `check` CI:** unchanged in shape; snapshots updated for `dir=` and the
extra `hotspot` lines.

**Human:** `cargo run -p carapace-demo` → the refactored `classic` skin clicks correctly (single
declarations), shows the rounded/circle shapes, and the vertical meter advances.

## Error handling

- Malformed shape args, bad `direction`, or a bad `on_press` → `BuildError` (`Lua`/`BadType`) →
  transactional swap keeps the prior scene.
- No panics on a skin fault; the engine returns `Result` / degrades. `unwrap` only on engine
  invariants (as today).

## Definition of done (5d)

`shape.rs` ships `circle`/`rect`/`rounded_rect` and they are injectable, composable path-helpers;
`fill{}`/`image{}` accept `on_press` and emit a hotspot from the drawn geometry (via
`Primitive::build -> Vec<Node>`); `value_fill` honours a `direction` and clips to its path with no
regression on rectangular bars; the `classic` skin is refactored to use all three live; the headless
boundary, the fast `check` CI job (incl. `clippy -D warnings`), and the snapshot harness are all
green.

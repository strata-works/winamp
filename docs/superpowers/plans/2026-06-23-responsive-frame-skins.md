# Responsive Frame Skins Implementation Plan (Spec 1 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the responsive **frame skin** archetype — resizable themed windows whose chrome docks to edges and whose content region stretches — to carapace, proven by a resizable demo hosting a reflowing app-shell.

**Architecture:** Per-element anchors are parsed as a common authoring attribute in the skin build loop and stored as a parallel `Vec<Anchors>` (no `Node`/`Primitive` change). A new GPU-free `layout.rs` resolves anchors → concrete logical rects for the current window size, returning a resolved `Scene` whose `canvas` field is set to the logical size — so the existing `render.rs` scale formula (`target.width / scene.canvas`) yields the DPI factor for frame skins while gadget skins keep their unchanged (pixel-identical) path. A new `frame{}` 9-slice primitive draws stretchable bitmap chrome.

**Tech Stack:** Rust, vello 0.9, wgpu 29.0.3, mlua 0.11 (lua54), parley 0.10, winit 0.30, serde/toml. Tests: insta (snapshots), pollster (GPU offscreen, `gpu-tests` feature).

## Global Constraints

- **Headless/GPU split:** `scene.rs`, `vocab.rs`, `layout.rs`, `script.rs`, `skin.rs` stay GPU-free; only `render.rs` touches the GPU. The layout pass is pure geometry.
- **Backward compatibility is a hard invariant:** every existing gadget skin renders **pixel-identical**. Gadget skins take the unchanged render path (design `Scene`, `canvas` = design size). Enforced by a GPU sentinel-pixel test (Task 7).
- **Domain neutrality:** anchors and 9-slice are geometry; the engine carries no app meaning.
- **Default anchor = `{top, left}`** (fixed size, fixed top-left) — identical to today's behavior. Anchors are an optional attribute; absence = default.
- **Archetype switch:** manifest `resizable = true` + `min_size` marks a frame skin. Absent = gadget skin (uniform zoom), unchanged.
- **CI gates:** `cargo clippy --locked --workspace --all-targets -- -D warnings` (and the `-p carapace --features gpu-tests` variant), `cargo fmt --check`, `cargo test --workspace`, the snapshot + GPU harnesses. Run all before every commit.
- **Git identity:** `Daniel Agbemava <danagbemava@gmail.com>`. No Claude attribution in commits.
- **New deps:** none expected. If any is added, first fetch via `sfw cargo ...` (Socket Firewall).

## File Structure

- **Create `crates/carapace/src/layout.rs`** — the `Anchors` type, `Rect`, `resolve_bbox` (pure per-axis anchor resolution), and `resolve_scene` (apply anchors to every node, return a logical `Scene`). GPU-free.
- **Modify `crates/carapace/src/lib.rs`** — `pub mod layout;`.
- **Modify `crates/carapace/src/script.rs`** — parse the common `anchor`/`min` attributes in the build loop; store `Vec<Anchors>` in `SceneBuilder` and `LoadedSkin`.
- **Modify `crates/carapace/src/engine.rs`** — `Engine::layout(logical) -> Scene` delegating to `layout::resolve_scene`.
- **Modify `crates/carapace/src/scene.rs`** — add `Node::Frame { image, dest, slice, center }`, its `summary()` arm, and a `Slice`/`FrameCenter` type.
- **Modify `crates/carapace/src/vocab.rs`** — `FramePrim` (`frame{}`); register in `base()` (now 7).
- **Modify `crates/carapace/src/render.rs`** — draw `Node::Frame` as 9 image quads. (No scale-formula change.)
- **Modify `crates/carapace/src/skin.rs`** — `Manifest` gains `resizable`, `min_size`, `max_size`.
- **Modify `crates/carapace-demo/src/main.rs`** — resizable window from manifest, logical/DPI sizing, call `engine.layout` for frame skins, the nested app-shell engine, pointer mapping in logical space.
- **Create `crates/carapace-demo/skins/frame/`** — the example frame skin (`skin.toml`, `skin.lua`, `assets/`).
- **Create `crates/carapace/tests/layout.rs`** — headless layout unit tests.

## Interfaces (cross-task contract)

```rust
// layout.rs
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchors {
    pub left: bool, pub right: bool, pub top: bool, pub bottom: bool,
    pub min: Option<(f32, f32)>,
}
impl Anchors { pub const TOP_LEFT: Anchors; pub fn from_edges(edges: &[&str]) -> Anchors; }

pub fn resolve_bbox(design: (f32, f32), logical: (f32, f32), bbox: Rect, a: Anchors) -> Rect;
pub fn resolve_scene(design: &crate::scene::Scene, anchors: &[Anchors], logical: (f32, f32))
    -> crate::scene::Scene;

// scene.rs
pub enum FrameCenter { Stretch, Hollow }
pub struct Slice { pub left: f32, pub right: f32, pub top: f32, pub bottom: f32 }
// Node::Frame { image: Arc<DecodedImage>, dest: ImageDest, slice: Slice, center: FrameCenter }

// engine.rs
impl Engine { pub fn layout(&self, logical_w: f32, logical_h: f32) -> Scene; }
```

---

### Task 1: `layout.rs` — `Anchors` + `resolve_bbox` (pure geometry)

**Files:**
- Create: `crates/carapace/src/layout.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod layout;` near the other `pub mod` lines)
- Test: `crates/carapace/tests/layout.rs`

**Interfaces:**
- Produces: `Rect`, `Anchors` (+ `TOP_LEFT`, `from_edges`), `resolve_bbox(design, logical, bbox, anchors) -> Rect`.
- Consumes: nothing.

**Anchor resolution math (per axis, independent):** given a design-space span `[p, p+e]` (origin `p`, extent `e`) inside design length `D`, resolving to logical length `L`:
- left+right (both): `p' = p` (left gap fixed), `e' = e + (L - D)` (right gap fixed); clamp `e'` to `min`.
- left only: `p' = p`, `e' = e`.
- right only: `p' = p + (L - D)` (fixed right gap), `e' = e`.
- neither: `e' = e`, `p' = p * (L / D)` (center rides proportionally; with `e` fixed this keeps the proportional offset).

- [ ] **Step 1: Write the failing tests**

Create `crates/carapace/tests/layout.rs`:
```rust
use carapace::layout::{resolve_bbox, Anchors, Rect};

const DESIGN: (f32, f32) = (100.0, 100.0);
const BIG: (f32, f32) = (200.0, 140.0);

fn a(left: bool, right: bool, top: bool, bottom: bool) -> Anchors {
    Anchors { left, right, top, bottom, min: None }
}

#[test]
fn left_only_is_fixed_position_and_size() {
    let r = resolve_bbox(DESIGN, BIG, Rect { x: 10.0, y: 10.0, w: 30.0, h: 20.0 }, a(true, false, true, false));
    assert_eq!(r, Rect { x: 10.0, y: 10.0, w: 30.0, h: 20.0 });
}

#[test]
fn right_only_rides_the_right_edge() {
    // width grows 100->200 (+100); a right-anchored element keeps width, x shifts by +100.
    let r = resolve_bbox(DESIGN, BIG, Rect { x: 60.0, y: 10.0, w: 30.0, h: 20.0 }, a(false, true, true, false));
    assert_eq!(r.x, 160.0);
    assert_eq!(r.w, 30.0);
}

#[test]
fn left_and_right_stretches_width() {
    // gaps: left=10, right=100-(10+80)=10. At width 200: w = 200-10-10 = 180.
    let r = resolve_bbox(DESIGN, BIG, Rect { x: 10.0, y: 10.0, w: 80.0, h: 20.0 }, a(true, true, true, false));
    assert_eq!(r.x, 10.0);
    assert_eq!(r.w, 180.0);
}

#[test]
fn top_and_bottom_stretches_height() {
    // height grows 100->140 (+40); top=10,bottom=10 gaps -> h = 140-20 = 120.
    let r = resolve_bbox(DESIGN, BIG, Rect { x: 10.0, y: 10.0, w: 30.0, h: 80.0 }, a(true, false, true, true));
    assert_eq!(r.y, 10.0);
    assert_eq!(r.h, 120.0);
}

#[test]
fn stretch_clamps_to_min() {
    let mut an = a(true, true, true, false);
    an.min = Some((40.0, 0.0)); // never narrower than 40 even when window shrinks
    let small = (50.0, 100.0);
    // design width 100, shrink to 50: unclamped w = 80 + (50-100) = 30 -> clamp to 40.
    let r = resolve_bbox(DESIGN, small, Rect { x: 10.0, y: 10.0, w: 80.0, h: 20.0 }, an);
    assert_eq!(r.w, 40.0);
}

#[test]
fn from_edges_parses_named_anchors() {
    assert_eq!(Anchors::from_edges(&["left", "right", "top"]), a(true, true, true, false));
    assert_eq!(Anchors::from_edges(&[]), a(false, false, false, false));
}

#[test]
fn top_left_default_is_fixed() {
    assert_eq!(Anchors::TOP_LEFT, a(true, false, true, false));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p carapace --test layout`
Expected: FAIL — `carapace::layout` unresolved.

- [ ] **Step 3: Implement `layout.rs`**

Create `crates/carapace/src/layout.rs`:
```rust
//! GPU-free layout resolution for frame skins. Resolves per-element anchors against the current
//! window size, producing concrete logical rects. Pure geometry — no GPU, no engine state.

use crate::scene::{ImageDest, Node, Pt, Scene};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Which window edges an element is pinned to (gap held constant), plus an optional stretch floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchors {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    /// Minimum (w, h) a stretched element collapses to. 0 on an axis = no floor.
    pub min: Option<(f32, f32)>,
}

impl Anchors {
    /// The default: fixed size, pinned to the top-left — identical to pre-anchor behavior.
    pub const TOP_LEFT: Anchors = Anchors {
        left: true,
        right: false,
        top: true,
        bottom: false,
        min: None,
    };

    /// Build from a list of edge names (`"left"`, `"right"`, `"top"`, `"bottom"`); unknown ignored.
    pub fn from_edges(edges: &[&str]) -> Anchors {
        Anchors {
            left: edges.contains(&"left"),
            right: edges.contains(&"right"),
            top: edges.contains(&"top"),
            bottom: edges.contains(&"bottom"),
            min: None,
        }
    }
}

/// Resolve one axis: origin `p`, extent `e`, design length `d`, logical length `l`, pins
/// `(near, far)`, floor `min_e`. Returns `(p', e')`.
fn resolve_axis(p: f32, e: f32, d: f32, l: f32, near: bool, far: bool, min_e: f32) -> (f32, f32) {
    let delta = l - d;
    let (mut np, mut ne) = match (near, far) {
        (true, true) => (p, e + delta),       // both gaps fixed -> stretch
        (true, false) => (p, e),              // near gap fixed
        (false, true) => (p + delta, e),      // far gap fixed -> rides far edge
        (false, false) => (p * (l / d.max(1.0)), e), // proportional re-center
    };
    if ne < min_e {
        ne = min_e;
    }
    if ne < 0.0 {
        ne = 0.0;
    }
    if !np.is_finite() {
        np = p;
    }
    (np, ne)
}

/// Resolve a design-space bounding box to a logical bounding box under its anchors.
pub fn resolve_bbox(design: (f32, f32), logical: (f32, f32), bbox: Rect, a: Anchors) -> Rect {
    let (min_w, min_h) = a.min.unwrap_or((0.0, 0.0));
    let (x, w) = resolve_axis(bbox.x, bbox.w, design.0, logical.0, a.left, a.right, min_w);
    let (y, h) = resolve_axis(bbox.y, bbox.h, design.1, logical.1, a.top, a.bottom, min_h);
    Rect { x, y, w, h }
}

/// The design-space bounding box of a node (rect for rect-nodes; point-bbox for text; path bbox
/// otherwise). Returns `None` for nodes without geometry to resolve.
fn node_bbox(node: &Node) -> Option<Rect> {
    fn path_bbox(path: &[Pt]) -> Option<Rect> {
        let xs = path.iter().map(|p| p.x);
        let ys = path.iter().map(|p| p.y);
        let x0 = xs.clone().fold(f32::INFINITY, f32::min);
        let x1 = xs.fold(f32::NEG_INFINITY, f32::max);
        let y0 = ys.clone().fold(f32::INFINITY, f32::min);
        let y1 = ys.fold(f32::NEG_INFINITY, f32::max);
        if x0.is_finite() && x1.is_finite() {
            Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
        } else {
            None
        }
    }
    match node {
        Node::Image { dest, .. } | Node::View { dest, .. } => {
            Some(Rect { x: dest.x, y: dest.y, w: dest.w, h: dest.h })
        }
        Node::Fill { path, .. } | Node::ValueFill { path, .. } => path_bbox(path),
        Node::Hotspot { region, .. } => {
            let pts: Vec<Pt> = region
                .contours
                .iter()
                .flat_map(|c| c.points.iter().map(|p| Pt { x: p.x, y: p.y }))
                .collect();
            path_bbox(&pts)
        }
        Node::Text { pos, .. } => Some(Rect { x: pos.x, y: pos.y, w: 0.0, h: 0.0 }),
    }
}

/// Apply a design->logical (translate + per-axis scale) transform to a node's geometry.
fn transform_node(node: &Node, from: Rect, to: Rect) -> Node {
    let sx = if from.w.abs() > f32::EPSILON { to.w / from.w } else { 1.0 };
    let sy = if from.h.abs() > f32::EPSILON { to.h / from.h } else { 1.0 };
    let map = |p: Pt| Pt {
        x: to.x + (p.x - from.x) * sx,
        y: to.y + (p.y - from.y) * sy,
    };
    let map_path = |path: &[Pt]| path.iter().map(|p| map(*p)).collect::<Vec<_>>();
    let mut n = node.clone();
    match &mut n {
        Node::Image { dest, .. } | Node::View { dest, .. } => {
            *dest = ImageDest { x: to.x, y: to.y, w: to.w, h: to.h };
        }
        Node::Fill { path, .. } | Node::ValueFill { path, .. } => {
            *path = map_path(path);
        }
        Node::Hotspot { region, .. } => {
            for c in &mut region.contours {
                for p in &mut c.points {
                    let m = map(Pt { x: p.x, y: p.y });
                    p.x = m.x;
                    p.y = m.y;
                }
            }
        }
        Node::Text { pos, .. } => {
            *pos = map(*pos);
        }
    }
    n
}

/// Resolve a design scene to a logical scene: each node's geometry is reflowed by its anchors,
/// and the result's `canvas` is set to the logical size (so the renderer scales it by DPI only).
pub fn resolve_scene(design: &Scene, anchors: &[Anchors], logical: (f32, f32)) -> Scene {
    let d = (design.canvas.0 as f32, design.canvas.1 as f32);
    let nodes = design
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let a = anchors.get(i).copied().unwrap_or(Anchors::TOP_LEFT);
            match node_bbox(node) {
                Some(bbox) => {
                    let to = resolve_bbox(d, logical, bbox, a);
                    transform_node(node, bbox, to)
                }
                None => node.clone(),
            }
        })
        .collect();
    Scene {
        nodes,
        canvas: (logical.0.round().max(1.0) as u32, logical.1.round().max(1.0) as u32),
    }
}
```

Add to `crates/carapace/src/lib.rs` alongside the other module declarations:
```rust
pub mod layout;
```

NOTE: `Node::Frame` does not exist yet (Task 4 adds it). The `node_bbox`/`transform_node` matches are exhaustive over the *current* `Node` variants; Task 4 extends both with a `Frame` arm. Do not add a wildcard arm — keep the match exhaustive so Task 4's compiler error flags the spot.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p carapace --test layout`
Expected: PASS (7 tests).

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked -p carapace --all-targets -- -D warnings
git add crates/carapace/src/layout.rs crates/carapace/src/lib.rs crates/carapace/tests/layout.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(layout): GPU-free anchor resolution (Anchors + resolve_bbox/resolve_scene)"
```

---

### Task 2: Parse `anchor`/`min` in the build loop; carry `Vec<Anchors>` to the engine

**Files:**
- Modify: `crates/carapace/src/script.rs` (`SceneBuilder`, `LoadedSkin`, the ctor closure, `load`)
- Modify: `crates/carapace/src/engine.rs` (`Engine::anchors` accessor for tests)
- Test: `crates/carapace/tests/anchors_build.rs`

**Interfaces:**
- Consumes: `Anchors`, `Anchors::from_edges` (Task 1).
- Produces: `LoadedSkin.anchors: Vec<Anchors>` (parallel to `scene.nodes`); `Engine::scene_anchors(&self) -> &[Anchors]`.

The `anchor` attribute is a Lua array of edge-name strings; `min` is `{ w=, h= }`. Both optional. Parsed once per primitive call in the build loop and replicated for each node that call emitted (so multi-node prims like `image{}`+hotspot share the element's anchors).

- [ ] **Step 1: Write the failing test**

Create `crates/carapace/tests/anchors_build.rs`:
```rust
use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::host::Host;
use carapace::layout::Anchors;
use carapace::vocab::VocabRegistry;

struct NoHost;
impl Host for NoHost {
    fn get(&self, _k: &str) -> Option<carapace::host::StateValue> { None }
    fn actions(&self) -> Vec<carapace::host::ActionSpec> { vec![] }
    fn invoke(&mut self, _a: &str, _args: &[carapace::host::Value]) {}
    fn tick(&mut self, _dt: std::time::Duration) {}
}

const SKIN: &str = "\
    view{ id='a', x=0, y=0, w=10, h=10, anchor = { 'left', 'right', 'top', 'bottom' } }\n\
    view{ id='b', x=0, y=0, w=10, h=10 }\n";

#[test]
fn anchors_parsed_parallel_to_nodes() {
    let e = Engine::new(Box::new(NoHost), VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100))).unwrap();
    let anchors = e.scene_anchors();
    assert_eq!(anchors.len(), e.scene().nodes.len());
    assert_eq!(anchors[0], Anchors { left: true, right: true, top: true, bottom: true, min: None });
    assert_eq!(anchors[1], Anchors::TOP_LEFT); // no anchor attr -> default
}
```

(If `carapace::host` item names differ, match the real ones from `host.rs` — the test only needs a no-op host; reuse an existing test host if one is exported.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace --test anchors_build`
Expected: FAIL — `scene_anchors` not found.

- [ ] **Step 3: Implement parsing + storage**

In `crates/carapace/src/script.rs`, add an `anchors` field to `SceneBuilder` and `LoadedSkin`:
```rust
pub struct LoadedSkin {
    pub scene: Scene,
    pub anchors: Vec<crate::layout::Anchors>,
    lua: Lua,
    handlers: Vec<Handler>,
    queue: Queue,
}

struct SceneBuilder {
    nodes: Vec<Node>,
    anchors: Vec<crate::layout::Anchors>,
    handlers: Vec<HandlerSpec>,
    assets: std::rc::Rc<crate::asset::AssetResolver>,
}
```

Add a free helper in `script.rs` to parse the common attributes from a primitive's `args` table:
```rust
fn parse_anchors(args: &Table) -> mlua::Result<crate::layout::Anchors> {
    use crate::layout::Anchors;
    let edges: Vec<String> = match args.get::<Option<Table>>("anchor")? {
        Some(t) => t.sequence_values::<String>().filter_map(|v| v.ok()).collect(),
        None => return Ok(Anchors::TOP_LEFT),
    };
    let refs: Vec<&str> = edges.iter().map(|s| s.as_str()).collect();
    let mut a = Anchors::from_edges(&refs);
    if let Some(m) = args.get::<Option<Table>>("min")? {
        let w: f32 = m.get::<Option<f32>>("w")?.unwrap_or(0.0);
        let h: f32 = m.get::<Option<f32>>("h")?.unwrap_or(0.0);
        a.min = Some((w, h));
    }
    Ok(a)
}
```

In the ctor closure (currently `b.nodes.extend(nodes);`), parse anchors and push one per emitted node:
```rust
let nodes = prim
    .build(&args, &mut *b)
    .map_err(|e| mlua::Error::external(format!("{e:?}")))?;
let anchors = parse_anchors(&args)?;
for _ in &nodes {
    b.anchors.push(anchors);
}
b.nodes.extend(nodes);
```

Initialize `anchors: Vec::new()` where `SceneBuilder` is constructed in `load`, and thread it into `LoadedSkin { scene, anchors: builder_anchors, ... }`. Find where `LoadedSkin` is assembled at the end of `load` (it moves `builder.nodes` into `Scene`); move `builder.anchors` into the new field alongside.

In `crates/carapace/src/engine.rs`, add the accessor:
```rust
/// The per-node anchors parallel to `scene().nodes`, for the layout pass.
pub fn scene_anchors(&self) -> &[crate::layout::Anchors] {
    &self.skin.anchors
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p carapace --test anchors_build`
Expected: PASS.

- [ ] **Step 5: Full test sweep + fmt + clippy + commit**

```bash
cargo test -p carapace
cargo fmt
cargo clippy --locked -p carapace --all-targets -- -D warnings
git add crates/carapace/src/script.rs crates/carapace/src/engine.rs crates/carapace/tests/anchors_build.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(skin): parse per-element anchor/min attributes into a parallel Vec<Anchors>"
```

---

### Task 3: `Engine::layout` — resolve a design scene to a logical scene

**Files:**
- Modify: `crates/carapace/src/engine.rs` (`Engine::layout`)
- Test: `crates/carapace/tests/engine_layout.rs`

**Interfaces:**
- Consumes: `layout::resolve_scene` (Task 1), `Engine::scene_anchors` (Task 2).
- Produces: `Engine::layout(&self, logical_w: f32, logical_h: f32) -> Scene`.

- [ ] **Step 1: Write the failing test**

Create `crates/carapace/tests/engine_layout.rs`:
```rust
use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::scene::Node;
use carapace::vocab::VocabRegistry;

struct NoHost;
impl carapace::host::Host for NoHost {
    fn get(&self, _k: &str) -> Option<carapace::host::StateValue> { None }
    fn actions(&self) -> Vec<carapace::host::ActionSpec> { vec![] }
    fn invoke(&mut self, _a: &str, _args: &[carapace::host::Value]) {}
    fn tick(&mut self, _dt: std::time::Duration) {}
}

// A full-bleed content view anchored to all four edges, in a 100x100 design.
const SKIN: &str = "view{ id='app', x=10, y=10, w=80, h=80, \
    anchor = { 'left','right','top','bottom' } }\n";

#[test]
fn layout_stretches_view_and_sets_canvas_to_logical() {
    let e = Engine::new(Box::new(NoHost), VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100))).unwrap();
    let resolved = e.layout(200.0, 150.0);
    assert_eq!(resolved.canvas, (200, 150)); // canvas = logical size -> render scales by DPI only
    match &resolved.nodes[0] {
        Node::View { dest, .. } => {
            // gaps left/top=10, right=100-90=10, bottom=10. -> x=10,y=10,w=180,h=130.
            assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 180.0, 130.0));
        }
        _ => panic!("expected a View node"),
    }
}

#[test]
fn layout_at_design_size_is_identity() {
    let e = Engine::new(Box::new(NoHost), VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100))).unwrap();
    let resolved = e.layout(100.0, 100.0);
    match &resolved.nodes[0] {
        Node::View { dest, .. } => assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 80.0, 80.0)),
        _ => panic!(),
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace --test engine_layout`
Expected: FAIL — `Engine::layout` not found.

- [ ] **Step 3: Implement `Engine::layout`**

In `crates/carapace/src/engine.rs`:
```rust
/// Resolve the design scene to a logical scene for the given window logical size, using the
/// skin's per-element anchors. The result's `canvas` equals the logical size, so the renderer
/// applies only the DPI scale. Frame skins call this on resize; gadget skins render the design
/// scene directly.
pub fn layout(&self, logical_w: f32, logical_h: f32) -> Scene {
    crate::layout::resolve_scene(&self.skin.scene, &self.skin.anchors, (logical_w, logical_h))
}
```

Ensure `Scene` is imported in `engine.rs` (it returns `Scene`; add `use crate::scene::Scene;` if not already present).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p carapace --test engine_layout`
Expected: PASS (2 tests).

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked -p carapace --all-targets -- -D warnings
git add crates/carapace/src/engine.rs crates/carapace/tests/engine_layout.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(engine): Engine::layout resolves anchors to a logical scene on resize"
```

---

### Task 4: `frame{}` 9-slice primitive

**Files:**
- Modify: `crates/carapace/src/scene.rs` (`Slice`, `FrameCenter`, `Node::Frame`, `summary()` arm)
- Modify: `crates/carapace/src/layout.rs` (`node_bbox`/`transform_node` `Frame` arms)
- Modify: `crates/carapace/src/vocab.rs` (`FramePrim`, register in `base()`)
- Modify: `crates/carapace/src/render.rs` (draw `Node::Frame` as 9 quads)
- Test: `crates/carapace/tests/frame_prim.rs`, and a GPU case appended to `crates/carapace/tests/render_offscreen.rs`

**Interfaces:**
- Consumes: `ImageDest`, `DecodedImage`, the image draw path in `render.rs`.
- Produces: `Node::Frame { image, dest, slice, center }`, `Slice`, `FrameCenter`; `frame{}` Lua primitive; `base()` now registers 7 primitives.

**9-slice geometry:** source image is `iw x ih`; `slice = {l,r,t,b}` insets. Destination rect `dest`. The 9 cells map source sub-rects → dest sub-rects: corners 1:1 (source corner size → same dest size), edges stretch along their long axis, center fills `dest` interior (skipped when `Hollow`). Clamp insets so `l+r <= dest.w` and `t+b <= dest.h` (and `<= iw/ih`) — when a window is tiny, shrink insets proportionally so corners never overlap.

- [ ] **Step 1: Write the failing headless test**

Create `crates/carapace/tests/frame_prim.rs`:
```rust
use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::scene::Node;
use carapace::vocab::VocabRegistry;

struct NoHost;
impl carapace::host::Host for NoHost {
    fn get(&self, _k: &str) -> Option<carapace::host::StateValue> { None }
    fn actions(&self) -> Vec<carapace::host::ActionSpec> { vec![] }
    fn invoke(&mut self, _a: &str, _args: &[carapace::host::Value]) {}
    fn tick(&mut self, _dt: std::time::Duration) {}
}

#[test]
fn base_registry_now_has_seven() {
    assert_eq!(VocabRegistry::base().iter().count(), 7);
}

#[test]
fn frame_builds_a_frame_node_with_slice_and_center() {
    // Uses the reference skin's headspace.png asset dir via an inline source pointing at it would
    // require assets; instead assert the summary line shape through a skin that ships an asset.
    // (Implementer: load `skins/reference` which ships headspace.png, append a frame{} line.)
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../carapace-demo/skins/reference");
    let (_m, mut src) = carapace::skin::load_dir(&dir).unwrap();
    src.lua_src = "frame{ asset='headspace.png', x=0, y=0, w=100, h=80, \
        slice={left=10,right=10,top=10,bottom=10}, center='hollow' }\n".to_string();
    let e = Engine::new(Box::new(NoHost), VocabRegistry::base(), src).unwrap();
    match &e.scene().nodes[0] {
        Node::Frame { dest, slice, .. } => {
            assert_eq!((dest.w, dest.h), (100.0, 80.0));
            assert_eq!((slice.left, slice.right, slice.top, slice.bottom), (10.0, 10.0, 10.0, 10.0));
        }
        _ => panic!("expected Frame node"),
    }
    assert!(e.scene().summary().contains("frame"));
}
```

(`SkinSource.lua_src` and `.canvas` are public fields per `command.rs`; if `lua_src` is private, the implementer adds a tiny inline constructor or uses `SkinSource::inline` with the asset dir wired — keep the asset `headspace.png` reachable.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace --test frame_prim`
Expected: FAIL — `Node::Frame` / count 6≠7.

- [ ] **Step 3: Add the scene types + node**

In `crates/carapace/src/scene.rs`, add near `ImageDest`:
```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameCenter {
    Stretch,
    Hollow,
}
```
Add a `Node` variant (place it after `Image`):
```rust
    Frame {
        image: std::sync::Arc<crate::asset::DecodedImage>,
        dest: ImageDest,
        slice: Slice,
        center: FrameCenter,
    },
```
Add the `summary()` arm (geometry-neutral, like `Image`):
```rust
    Node::Frame { image, slice, center, .. } => format!(
        "frame {}x{} slice {},{},{},{} center={}",
        image.width, image.height,
        slice.left as i64, slice.right as i64, slice.top as i64, slice.bottom as i64,
        match center { FrameCenter::Stretch => "stretch", FrameCenter::Hollow => "hollow" }
    ),
```

- [ ] **Step 4: Extend the layout match arms for `Frame`**

In `crates/carapace/src/layout.rs`, add `Frame` to both matches (it behaves like `Image`/`View` — a rect):
```rust
// node_bbox:
Node::Image { dest, .. } | Node::View { dest, .. } | Node::Frame { dest, .. } => {
    Some(Rect { x: dest.x, y: dest.y, w: dest.w, h: dest.h })
}
// transform_node:
Node::Image { dest, .. } | Node::View { dest, .. } | Node::Frame { dest, .. } => {
    *dest = ImageDest { x: to.x, y: to.y, w: to.w, h: to.h };
}
```

- [ ] **Step 5: Add the `FramePrim` vocab + register it**

In `crates/carapace/src/vocab.rs`, add the primitive and register it in `base()` after `ImagePrim` (so the order is fill, region, value_fill, image, frame, text, view — or append; order only affects draw order, keep `frame` near `image`):
```rust
struct FramePrim;
impl Primitive for FramePrim {
    fn id(&self) -> &str { "frame" }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let name: String = args.get("asset").map_err(|_| BuildError::MissingField("asset"))?;
        let image = ctx.image(&name).map_err(BuildError::Asset)?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").map_err(|_| BuildError::MissingField("w"))?;
        let h: f32 = args.get("h").map_err(|_| BuildError::MissingField("h"))?;
        let st: Table = args.get("slice").map_err(|_| BuildError::MissingField("slice"))?;
        let slice = crate::scene::Slice {
            left: st.get("left").map_err(|_| BuildError::MissingField("slice.left"))?,
            right: st.get("right").map_err(|_| BuildError::MissingField("slice.right"))?,
            top: st.get("top").map_err(|_| BuildError::MissingField("slice.top"))?,
            bottom: st.get("bottom").map_err(|_| BuildError::MissingField("slice.bottom"))?,
        };
        let center = match args.get::<Option<String>>("center").ok().flatten().as_deref() {
            Some("hollow") => crate::scene::FrameCenter::Hollow,
            _ => crate::scene::FrameCenter::Stretch,
        };
        Ok(vec![Node::Frame {
            image,
            dest: crate::scene::ImageDest { x, y, w, h },
            slice,
            center,
        }])
    }
}
```
In `base()`:
```rust
r.register(Box::new(ImagePrim));
r.register(Box::new(FramePrim));
```

- [ ] **Step 6: Draw `Node::Frame` in render.rs (9 quads)**

In `crates/carapace/src/render.rs`, the draw loop matches each node under the existing `xform` (canvas→surface). Add a `Node::Frame` arm modeled on the `Node::Image` arm (which builds an `ImageData` blob and an `Affine` placing it). Replicate the image placement 9 times, once per cell, clamping insets:
```rust
Node::Frame { image, dest, slice, center } => {
    let blob = Blob::new(std::sync::Arc::new(image.rgba.clone()));
    let img_data = vello::peniko::ImageData {
        data: blob,
        format: vello::peniko::ImageFormat::Rgba8,
        width: image.width,
        height: image.height,
        // match the alpha/quality fields used by the existing Node::Image arm
        ..image_data_defaults()
    };
    let (iw, ih) = (image.width as f64, image.height as f64);
    // Clamp insets so opposing corners never overlap in source or dest.
    let mut sl = slice.left as f64;
    let mut sr = slice.right as f64;
    let mut st = slice.top as f64;
    let mut sb = slice.bottom as f64;
    let fit = |a: &mut f64, b: &mut f64, limit: f64| {
        if *a + *b > limit && *a + *b > 0.0 {
            let k = limit / (*a + *b);
            *a *= k; *b *= k;
        }
    };
    fit(&mut sl, &mut sr, iw.min(dest.w as f64));
    fit(&mut st, &mut sb, ih.min(dest.h as f64));
    // Columns: (src_x0,src_w, dst_x0,dst_w) for left|center|right; rows likewise.
    let cols = [
        (0.0, sl, dest.x as f64, sl),
        (sl, iw - sl - sr, dest.x as f64 + sl, dest.w as f64 - sl - sr),
        (iw - sr, sr, dest.x as f64 + dest.w as f64 - sr, sr),
    ];
    let rows = [
        (0.0, st, dest.y as f64, st),
        (st, ih - st - sb, dest.y as f64 + st, dest.h as f64 - st - sb),
        (ih - sb, sb, dest.y as f64 + dest.h as f64 - sb, sb),
    ];
    for (ri, &(srcy, srch, dsty, dsth)) in rows.iter().enumerate() {
        for (ci, &(srcx, srcw, dstx, dstw)) in cols.iter().enumerate() {
            let is_center = ri == 1 && ci == 1;
            if is_center && matches!(center, FrameCenter::Hollow) {
                continue;
            }
            if srcw <= 0.0 || srch <= 0.0 || dstw <= 0.0 || dsth <= 0.0 {
                continue;
            }
            // Place the source sub-rect (srcx,srcy,srcw,srch) into dest sub-rect (dstx,dsty,dstw,dsth):
            // translate source origin to 0, scale to dest size, translate to dest origin — all under xform.
            let place = Affine::translate((dstx, dsty))
                * Affine::scale_non_uniform(dstw / srcw, dsth / srch)
                * Affine::translate((-srcx, -srcy));
            vs.push_clip_layer(
                Fill::NonZero,
                xform * Affine::translate((dstx, dsty)),
                &bez(&rect_path(dstx, dsty, dstw, dsth)),
            );
            vs.draw_image(&img_data, xform * place);
            vs.pop_layer();
        }
    }
}
```
IMPLEMENTER NOTE: mirror the EXACT vello image-draw calls the existing `Node::Image` arm uses (the real method names for building `ImageData` and drawing — `draw_image` vs `vs.fill` with an image brush, the alpha/quality fields, and how it clips). The snippet shows the geometry; copy the established image-draw idiom from the `Node::Image` arm verbatim for the per-cell draw, including a clip to the dest sub-rect so the scaled source doesn't bleed. Add a small `rect_path(x,y,w,h) -> Vec<KPoint>`/`bez` helper if one isn't already present (the value_fill arm builds rects — reuse it).

- [ ] **Step 7: Run the headless test + add a GPU test**

Run: `cargo test -p carapace --test frame_prim`
Expected: PASS.

Append to `crates/carapace/tests/render_offscreen.rs` a two-size test asserting corner pixels are fixed and an edge span grew. Use a small solid-bordered test asset, or the reference `headspace.png`. Skeleton:
```rust
#[test]
fn frame_keeps_corners_fixed_and_stretches_edges() {
    // Render a frame{} at 120x120 and at 200x120; a top-left corner block of the chrome should be
    // byte-identical between the two (corners never scale), while a top-edge sample differs.
    // (Implementer: build two Scenes with a single Node::Frame differing only in dest.w, render
    //  each offscreen, compare px() at a corner vs an edge.)
}
```
Run: `cargo test -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS (existing + new).

- [ ] **Step 8: Fix dependent count tests, full sweep, fmt, clippy, commit**

The demo's `gauge.rs`/`transport.rs` base-count asserts (currently 7) become 8 — update them. Run the whole sweep:
```bash
cargo test --workspace
cargo test -p carapace --features gpu-tests
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace --features gpu-tests --all-targets -- -D warnings
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(vocab): frame{} 9-slice primitive (Node::Frame + render as 9 quads)"
```

---

### Task 5: Manifest archetype + resizable demo window (logical/DPI sizing)

**Files:**
- Modify: `crates/carapace/src/skin.rs` (`Manifest` gains `resizable`, `min_size`, `max_size`)
- Modify: `crates/carapace-demo/src/main.rs` (window resizable from manifest; logical/DPI; `engine.layout` for frame skins; pointer mapping)
- Test: `crates/carapace/tests/manifest_resizable.rs`

**Interfaces:**
- Consumes: `Engine::layout` (Task 3), `Manifest` (extended here).
- Produces: a resizable window path for frame skins; gadget skins unchanged.

- [ ] **Step 1: Write the failing manifest test**

Create `crates/carapace/tests/manifest_resizable.rs`:
```rust
#[test]
fn manifest_defaults_to_non_resizable() {
    let toml = r#"
schema = 1
id = "x"
name = "X"
engine = "carapace"
entry = "skin.lua"
canvas = { width = 100, height = 80 }
"#;
    let m: carapace::skin::Manifest = toml::from_str(toml).unwrap();
    assert!(!m.resizable);
    assert_eq!(m.min_size, None);
}

#[test]
fn manifest_parses_resizable_and_min_size() {
    let toml = r#"
schema = 1
id = "x"
name = "X"
engine = "carapace"
entry = "skin.lua"
canvas = { width = 480, height = 320 }
resizable = true
min_size = [320, 220]
"#;
    let m: carapace::skin::Manifest = toml::from_str(toml).unwrap();
    assert!(m.resizable);
    assert_eq!(m.min_size, Some((320, 220)));
}
```
(Verify `SUPPORTED_SCHEMA` = 1; if different, use the real value. These tests deserialize the `Manifest` directly, so they don't hit the schema/engine checks in `load_dir`.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace --test manifest_resizable`
Expected: FAIL — fields missing.

- [ ] **Step 3: Extend the `Manifest`**

In `crates/carapace/src/skin.rs`, add to `Manifest`:
```rust
    #[serde(default)]
    pub resizable: bool,
    #[serde(default)]
    pub min_size: Option<(u32, u32)>,
    #[serde(default)]
    pub max_size: Option<(u32, u32)>,
```
`load_dir` returns the `Manifest`; the demo already receives it via `load_dir` (it currently discards it in `load_source_from`). Change `load_source_from` to also return the manifest fields the demo needs (see Step 4).

- [ ] **Step 4: Make the demo resize frame skins**

In `crates/carapace-demo/src/main.rs`:

1. Carry the archetype. Change `load_source_from` to surface `resizable`/`min_size`:
```rust
struct SkinMeta { resizable: bool, min_size: Option<(u32, u32)> }

fn load_source_from(list: &[&str], i: usize) -> (SkinSource, (u32, u32), SkinMeta) {
    let (m, src) = carapace::skin::load_dir(&skin_root().join(list[i])).expect("load skin");
    let canvas = src.canvas;
    (src, canvas, SkinMeta { resizable: m.resizable, min_size: m.min_size })
}
```
Store the current `SkinMeta` on `App`. Update the two other call sites (Tab/H skin switches) to capture it.

2. Window creation: a frame skin opens at its design (canvas) logical size and is resizable with `min_inner_size`; a gadget skin keeps `canvas * INIT_SCALE` and is fixed:
```rust
let (cw, ch) = self.engine.scene().canvas;
let mut attrs = Window::default_attributes()
    .with_decorations(false)
    .with_transparent(true);
if self.meta.resizable {
    attrs = attrs
        .with_resizable(true)
        .with_inner_size(winit::dpi::LogicalSize::new(cw, ch));
    if let Some((mw, mh)) = self.meta.min_size {
        attrs = attrs.with_min_inner_size(winit::dpi::LogicalSize::new(mw, mh));
    }
} else {
    attrs = attrs
        .with_resizable(false)
        .with_inner_size(winit::dpi::LogicalSize::new(cw * INIT_SCALE, ch * INIT_SCALE));
}
```

3. The draw call: for a frame skin, resolve at the window's LOGICAL size and pass the resolved scene; for a gadget skin, pass the design scene unchanged. Compute logical from physical and the window scale factor:
```rust
let scale_factor = window.scale_factor() as f32; // physical / logical (retina = 2.0)
let logical = (gpu.config.width as f32 / scale_factor, gpu.config.height as f32 / scale_factor);
let resolved;
let scene: &Scene = if self.meta.resizable {
    resolved = self.engine.layout(logical.0, logical.1);
    &resolved
} else {
    self.engine.scene()
};
// renderer.draw(scene, read_value, view_tex, &RenderTarget { width: gpu.config.width, height: gpu.config.height, .. })
```
For a frame skin `scene.canvas == logical`, so `render`'s `sx = config.width / logical = scale_factor` (DPI only). For a gadget skin nothing changed.

4. Pointer mapping: map the physical cursor into the scene's coordinate space. Today it maps physical→canvas. For a frame skin the hit-test scene is the *resolved* one (canvas = logical), so map physical→logical; for a gadget skin keep physical→design-canvas. Since `engine.handle_pointer` hit-tests `engine.scene()` (the DESIGN scene), and frame-skin hotspots are in design coords, map the physical cursor back to DESIGN coords for both: `cx = cursor.x * design_canvas.w / physical.w`. This already holds (the existing mapping uses `self.engine.scene().canvas` = design). KEEP the existing mapping — it stays correct because hit-testing runs against the design scene, not the resolved one. (Frame-skin hotspots that are anchored will hit-test at their design positions; for Spec 1's example the title-bar buttons are top-anchored, so design and resolved positions coincide at the top edge. Document this; full anchored-hotspot hit-testing is Spec 2's input-routing work.)

5. `WindowEvent::Resized` already calls `gpu.reconfigure`; that's sufficient (the next redraw recomputes `logical` and re-runs `engine.layout`). Ensure a redraw is requested after resize.

- [ ] **Step 5: Run + sweep + commit**

Run: `cargo test -p carapace --test manifest_resizable` → PASS.
```bash
cargo build -p carapace-demo
cargo test --workspace
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): manifest resizable/min_size + resizable frame-skin window (logical/DPI)"
```

---

### Task 6: The example frame skin + nested app-shell

**Files:**
- Create: `crates/carapace-demo/skins/frame/skin.toml`, `skins/frame/skin.lua`, `skins/frame/assets/window.png`
- Modify: `crates/carapace-demo/src/main.rs` (add `skins/frame` to `MEDIA_SKINS`; an `AppShell` nested engine like `Monitor`; supply its texture for the `view{ id="app" }`)

**Interfaces:**
- Consumes: `frame{}`, anchors, `Engine::layout`, `view{}` + the LHVR composite (`view_tex`), the `Monitor`/nested-engine pattern from the LHVR demo.
- Produces: a runnable resizable frame skin hosting a reflowing app-shell.

This task is GUI wiring — verified by compile + existing tests staying green + a human smoke check (resize the window, watch chrome stay crisp and content reflow). No new unit tests; the layout engine it exercises is already covered by Tasks 1–4.

- [ ] **Step 1: The frame skin manifest + chrome**

`crates/carapace-demo/skins/frame/skin.toml`:
```toml
schema = 1
id = "frame"
name = "Frame"
engine = "carapace"
entry = "skin.lua"
canvas = { width = 480, height = 320 }
resizable = true
min_size = [320, 220]
asset_dir = "assets"
```
`crates/carapace-demo/skins/frame/skin.lua` — a title bar, 9-slice borders, and a 4-anchored content view:
```lua
-- window border: a hollow-center 9-slice frame anchored to all four edges
frame{ asset = "window.png", x = 0, y = 0, w = 480, h = 320,
       slice = { left = 16, right = 16, top = 36, bottom = 16 }, center = "hollow",
       anchor = { "left", "right", "top", "bottom" } }
-- title bar fill: full width, fixed height, pinned to the top
fill{ path = rect{ x = 0, y = 0, w = 480, h = 30 }, color = { r = 28, g = 34, b = 46 },
      anchor = { "left", "right", "top" } }
text{ text = "carapace://files", font = "vt323.ttf", size = 18, x = 12, y = 4,
      color = { r = 200, g = 220, b = 255 }, anchor = { "left", "top" } }
-- close / minimize hotspots, pinned to the top-right
region{ path = rect{ x = 456, y = 8, w = 14, h = 14 }, anchor = { "right", "top" },
        on_press = function() host.close() end }
region{ path = rect{ x = 436, y = 8, w = 14, h = 14 }, anchor = { "right", "top" },
        on_press = function() host.minimize() end }
-- whole-window drag region (behind the controls)
region{ path = rect{ x = 0, y = 0, w = 480, h = 30 }, anchor = { "left", "right", "top" },
        on_press = function() host.begin_drag() end }
-- the hosted app's content region: stretches to fill, never smaller than 280x150
view{ id = "app", x = 12, y = 36, w = 456, h = 272,
      anchor = { "left", "right", "top", "bottom" }, min = { w = 280, h = 150 } }
```
Provide `assets/window.png` — a 480×320 themed window bitmap whose 16/36/16/16 insets are the corner art (the implementer creates a simple rounded-rect chrome PNG with a darker title strip; a flat-color rounded border is acceptable for the demo). The `vt323.ttf` font is shared with the reference skin — copy it into `skins/frame/assets/` or reference the existing asset path the resolver allows.

- [ ] **Step 2: The nested app-shell engine**

Model an `AppShell` struct on the LHVR `Monitor` (its own `Engine` + `Renderer` + texture). It runs an **app-shell skin** (inline `const APP_SHELL: &str`) that is itself a frame skin reflowed at the view's resolved size each frame:
```rust
const APP_SHELL: &str = "\
    fill{ path = rect{x=0,y=0,w=456,h=272}, color = {r=18,g=20,b=26} }\n\
    fill{ path = rect{x=0,y=0,w=456,h=24}, color = {r=40,g=46,b=60}, anchor={'left','right','top'} }\n\
    text{ text='Name', font='vt323.ttf', size=14, x=64, y=4, color={r=170,g=185,b=210}, anchor={'left','top'} }\n\
    fill{ path = rect{x=0,y=24,w=120,h=248}, color = {r=24,g=28,b=38}, anchor={'left','top','bottom'} }\n\
    text{ text='Places', font='vt323.ttf', size=13, x=12, y=32, color={r=150,g=165,b=190}, anchor={'left','top'} }\n\
    text{ text='~/Music', font='vt323.ttf', size=13, x=12, y=52, color={r=130,g=200,b=150}, anchor={'left','top'} }\n\
    text{ text='~/Docs',  font='vt323.ttf', size=13, x=12, y=72, color={r=130,g=200,b=150}, anchor={'left','top'} }\n\
    text{ text='track-01.mp3   3.2M', font='vt323.ttf', size=13, x=132, y=32, color={r=200,g=210,b=225}, anchor={'left','right','top'} }\n\
    text{ text='track-02.mp3   4.1M', font='vt323.ttf', size=13, x=132, y=52, color={r=200,g=210,b=225}, anchor={'left','right','top'} }\n\
    text{ text='notes.txt      812B', font='vt323.ttf', size=13, x=132, y=72, color={r=200,g=210,b=225}, anchor={'left','right','top'} }\n";
```
The shell's design size is the view's *design* rect size (456×272). Each frame, after the outer layout, the demo:
1. reads the resolved `view{ id="app" }` rect from `self.engine.layout(...).views()` (or tracks it), in **physical** pixels = resolved-logical × scale_factor;
2. resizes the `AppShell` texture to that physical size if it changed;
3. calls `app_shell.engine.layout(view_w_logical, view_h_logical)` to reflow the shell to the view size, renders it into the texture (its resolved scene `canvas` = the view logical size, so its own DPI scale is correct);
4. supplies that texture to the outer `renderer.draw` `view_tex` closure for id `"app"`.

(Reuse the LHVR `view_tex` plumbing verbatim — `|id| if id == "app" { Some(&shell.view) } else { None }`.)

- [ ] **Step 3: Register the skin + verify build**

Add `"skins/frame"` to `MEDIA_SKINS`. Build and run the existing demo tests:
```bash
cargo build -p carapace-demo
cargo test -p carapace-demo
```
Expected: compiles; demo tests green.

- [ ] **Step 4: Human smoke check (manual, not committed)**

`cargo run -p carapace-demo`, Tab to the `frame` skin, drag the window edges: the 9-slice border corners stay crisp while edges stretch; the title bar keeps its height and spans the width; the nav rail keeps its width; the file rows reflow with the content pane. Gadget skins (Headspace/sysmon) still zoom. (If the app texture looks stretched, the shell's design size must match the view design rect; if chrome blurs at corners, check the 9-slice inset clamping.)

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): resizable frame skin hosting a reflowing app-shell"
```

---

### Task 7: Backward-compat pixel guard + README

**Files:**
- Modify: `crates/carapace/tests/render_offscreen.rs` (a gadget-skin sentinel-pixel guard)
- Modify: `README.md`

**Interfaces:**
- Consumes: the gadget-skin render path (must be unchanged).
- Produces: a regression guard + user-facing docs.

- [ ] **Step 1: Add a gadget-skin pixel guard (GPU)**

Append to `crates/carapace/tests/render_offscreen.rs` a test that renders a small gadget scene (a known solid `fill` + a `value_fill`, like the existing `renders_fill_and_value_fill_at_sentinel_pixels`) at a NON-unit scale (e.g. canvas 100×100 into a 300×300 target) and asserts the sentinel pixels land where the uniform `canvas→surface` scale puts them — proving the gadget path (design scene, `canvas` = design size) still scales uniformly after the frame-skin work:
```rust
#[test]
fn gadget_path_still_uniform_scales() {
    let o = offscreen(300, 300); // 3x a 100x100 canvas
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        nodes: vec![/* a red fill over canvas rect 20,20..50,50 */],
        canvas: (100, 100),
    };
    r.draw(&scene, |_| None, |_| None, &RenderTarget { /* width:300,height:300, base black */ });
    let data = readback(&o);
    // 30,30 canvas -> 90,90 surface at 3x; assert red there and skin/base elsewhere.
    assert_eq!(px(&data, 300, 90, 90), [255, 0, 0]);
    assert_eq!(px(&data, 300, 5, 5), [0, 0, 0]);
}
```
Run: `cargo test -p carapace --features gpu-tests --test render_offscreen` → PASS.

- [ ] **Step 2: Update the README**

Document the second archetype: frame skins are resizable themed windows (`resizable=true` + `min_size` in the manifest); positioned primitives take an optional `anchor = { ... }` (`left`/`right`/`top`/`bottom`; both sides of an axis → stretch; default top-left = fixed); the `frame{}` 9-slice primitive (`slice` insets, `center: stretch|hollow`) paints stretchable bitmap chrome; gadget skins keep uniform-zoom scaling and render identically. Note the demo's `frame` skin hosts a reflowing app-shell. Keep claims accurate: the hosted content is **not yet interactive** (navigation/lists are Spec 2). Update any primitive-count or roadmap line (base vocab is now 7).

- [ ] **Step 3: Final sweep + commit**

```bash
cargo test --workspace
cargo test -p carapace --features gpu-tests
cargo fmt --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace --features gpu-tests --all-targets -- -D warnings
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "test(render): gadget-skin pixel guard; docs: README documents frame skins"
```

---

## Plan Self-Review

**Spec coverage:**
- Per-element anchors → Task 1 (`Anchors`/`resolve_bbox`) + Task 2 (parse). ✓
- GPU-free layout pass → Task 1 (`resolve_scene`) + Task 3 (`Engine::layout`). ✓
- `frame{}` 9-slice → Task 4. ✓
- Logical/DPI split → achieved by resolved-scene `canvas` = logical (Task 1 `resolve_scene` + Task 5 demo); gadget pixel-identical guard → Task 7. ✓
- Manifest `resizable`/`min_size`/`max_size` → Task 5. ✓
- Resizable demo window + example frame skin + nested app-shell → Tasks 5–6. ✓
- Backward compat (gadget pixel-identical) → unchanged render path + Task 7 guard. ✓
- Error handling (clamped insets, min clamp, default anchor) → Task 1 (`resolve_axis` clamps), Task 4 (inset `fit`). ✓
- Testing (headless layout bulk, 9-slice GPU, manifest, goldens) → Tasks 1,4,5,7. ✓

**Out of scope held:** no `list{}`/input-routing/file-browser behavior/audio (Specs 2–3); no container/flex; no edge tiling. ✓

**Type consistency:** `Anchors`/`Rect`/`resolve_bbox`/`resolve_scene` (layout.rs), `Slice`/`FrameCenter`/`Node::Frame` (scene.rs), `Engine::layout`/`Engine::scene_anchors` (engine.rs), `Manifest.resizable/min_size/max_size` (skin.rs) used consistently across tasks. The `Primitive` trait and `Node` constructions in existing tests are untouched except `Node::Frame`'s addition (Task 4 updates the demo count asserts 7→8).

**Compile-safety between tasks:** Task 1 adds `layout.rs` with exhaustive matches over current `Node`; Task 4 extends those matches when it adds `Frame` (no wildcard, so the compiler points at every spot). Tasks 2–3 are additive accessors. Task 5 changes `load_source_from`'s return — its three call sites are updated in the same task.

**Risk note for the controller:** Task 4 Step 6 depends on mirroring the exact vello image-draw idiom from the existing `Node::Image` arm (method names/fields differ across vello versions). The implementer must copy that arm's real calls, not the illustrative snippet. Task 6 is GUI-only (human-verified). Both are flagged in-task.

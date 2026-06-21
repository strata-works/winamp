# Phase 5d — Vocab Ergonomics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author-DX upgrades to the base vocabulary: composable shape path-helpers (`circle`/`rect`/`rounded_rect`), an optional `on_press` on drawables so a clickable control is declared once, and a `value_fill` `direction` that clips to the actual path.

**Architecture:** Geometry + parsing stay headless (`shape.rs`, `vocab.rs`, `scene.rs`); only `render.rs` touches the GPU. Shared geometry is powered by changing `Primitive::build` to emit `Vec<Node>` (a draw node + an optional hotspot). Shapes are pure functions injected into the Lua sandbox as path-returning helpers. `value_fill` gains a `FillDir` and renders as path ∩ value-extent via a vello clip layer.

**Tech Stack:** Rust (edition 2024), `vello` 0.9 / `wgpu` 29 (peniko 0.6.1, `Mix::Clip` clip layer), `mlua`, `hittest`, `insta` snapshots.

**Spec:** `docs/superpowers/specs/2026-06-21-phase5d-vocab-ergonomics-design.md`

## Global Constraints

- Rust edition 2024; builds against Rust 1.96. CI builds `--locked`; keep `Cargo.lock` committed/updated. **No new crates in 5d** (so no `sfw` dep step).
- **CI gates on clippy.** Before every commit run BOTH: `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings`. Both must be clean (the Phase 5c CI break was a missed clippy lint).
- **`scene::summary()` stays domain-neutral and geometry-free** — node kinds, binding keys, style enums only; never raw point coordinates. The `value_fill` line gains `dir=<right|left|up|down>`.
- **No visual regression** for existing rectangular `value_fill` bars: `direction` defaults to `Right` and clip-to-path is a no-op when bbox == path.
- All git commits use identity **Daniel Agbemava <danagbemava@gmail.com>**; never add Claude attribution.
- GPU tests run under the `gpu-tests` feature (macOS Metal locally / lavapipe on Linux CI); headless tests must not require a GPU.

---

## File Structure

- `crates/carapace/src/vocab.rs` — `Primitive::build -> Result<Vec<Node>, BuildError>`; `maybe_hotspot` helper; `on_press` on `FillPrim`/`ImagePrim`; `parse_direction`; `ValueFillPrim` reads direction.
- `crates/carapace/src/shape.rs` *(new)* — pure `circle`/`rect`/`rounded_rect` → `Vec<Pt>`.
- `crates/carapace/src/lib.rs` — declare `pub mod shape;`.
- `crates/carapace/src/script.rs` — build loop `extend`s with `Vec<Node>`; injects `circle`/`rect`/`rounded_rect` Lua helpers.
- `crates/carapace/src/scene.rs` — `FillDir` enum; `Node::ValueFill.direction`; `summary()` `dir=`.
- `crates/carapace/src/render.rs` — `value_fill` clip-to-path + direction-aware extent rect.
- `crates/carapace/tests/render_offscreen.rs` — GPU sentinels (direction + clip); update existing `ValueFill` literal for the new field.
- `crates/carapace/tests/snapshots/behavior_snapshots__*.snap` — regenerate for `dir=right`.
- `crates/carapace-demo/skins/classic/skin.lua` — refactor to shapes + `on_press` + vertical meter.
- `crates/carapace-demo/tests/skins_build.rs` — assert refactored scene shape.
- `README.md` — roadmap + vocab list.

---

## Task 1: `Primitive::build` → `Vec<Node>` (mechanical refactor, no behavior change)

**Files:**
- Modify: `crates/carapace/src/vocab.rs` (trait `:33`, the 5 prim impls, the test module)
- Modify: `crates/carapace/src/script.rs` (ctor closure `:90-94`)

**Interfaces:**
- Produces: `Primitive::build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError>`. Each current prim returns a single-element `Vec`. Build-loop appends with `extend`.

- [ ] **Step 1: Change the trait signature**

In `crates/carapace/src/vocab.rs`, change the `Primitive` trait's `build`:

```rust
pub trait Primitive {
    fn id(&self) -> &str;
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError>;
}
```

- [ ] **Step 2: Wrap each prim's result in `vec![]`**

Update all 5 impls so their `Ok(Node::…)` becomes `Ok(vec![Node::…])`:

- `FillPrim::build` → `Ok(vec![Node::Fill { path: parse_path(args)?, paint: parse_paint(args)? }])`
- `RegionPrim::build` → `Ok(vec![Node::Hotspot { region: crate::scene::region_of(&path), on_press: id }])`
- `ValueFillPrim::build` → `Ok(vec![Node::ValueFill { path: parse_path(args)?, value_key, color: parse_color(args)? }])`
- `ImagePrim::build` → `Ok(vec![Node::Image { image, dest: crate::scene::ImageDest { x, y, w, h } }])`
- `TextPrim::build` → wrap its final `Ok(Node::Text { … })` as `Ok(vec![Node::Text { … }])`

(Leave all error returns — `return Err(...)` — unchanged.)

- [ ] **Step 3: Update the script build-loop to `extend`**

In `crates/carapace/src/script.rs`, the ctor closure (`:90-94`) currently does:

```rust
            let mut b = builder.borrow_mut();
            let node = prim
                .build(&args, &mut *b)
                .map_err(|e| mlua::Error::external(format!("{e:?}")))?;
            b.nodes.push(node);
            Ok(())
```

Change to:

```rust
            let mut b = builder.borrow_mut();
            let nodes = prim
                .build(&args, &mut *b)
                .map_err(|e| mlua::Error::external(format!("{e:?}")))?;
            b.nodes.extend(nodes);
            Ok(())
```

- [ ] **Step 4: Add a `one()` test helper and wrap single-node call sites**

In the `vocab.rs` `#[cfg(test)] mod tests`, add near the top of the module:

```rust
    /// Extracts the single node a primitive emits (most prims emit exactly one).
    fn one(r: Result<Vec<Node>, BuildError>) -> Node {
        let v = r.unwrap();
        assert_eq!(v.len(), 1, "expected exactly one node, got {}", v.len());
        v.into_iter().next().unwrap()
    }
```

Then wrap every success-path `Prim.build(...).unwrap()` that is matched as a single `Node` with `one(...)`. Concretely, in these tests change `match <Prim>.build(&t, &mut <ctx>).unwrap() {` to `match one(<Prim>.build(&t, &mut <ctx>)) {`:
- `fill_builds_fill_node` (`FillPrim`)
- `value_fill_keeps_binding_key` (`ValueFillPrim`)
- `region_registers_handler_and_caches_region` (`RegionPrim`)
- `fill_builds_linear_gradient` (`FillPrim`)
- `radial_and_sweep_parse_with_defaults` (both `FillPrim` matches)
- `image_prim_builds_native_and_scaled` (both `ImagePrim` matches)
- `text_prim_builds_static_with_defaults`, `text_prim_builds_bound_with_alignment_and_wrap` (`TextPrim`)

The error-path tests (`missing_field_errors`, `gradient_rejects_bad_type_and_too_few_stops`, `text_prim_content_xor_and_bad_align`) use `matches!(<Prim>.build(...), Err(...))` — leave unchanged (the `Err` variant is the same). Registry-count tests are unchanged.

- [ ] **Step 5: Run tests**

Run: `cargo test -p carapace --lib`
Expected: PASS — pure refactor, no behavior change; node count and base registry (5) unchanged.

- [ ] **Step 6: Clippy + commit**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/vocab.rs crates/carapace/src/script.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "refactor(vocab): Primitive::build emits Vec<Node>"
```

---

## Task 2: `on_press` on `fill{}` and `image{}` (shared draw+hotspot geometry)

**Files:**
- Modify: `crates/carapace/src/vocab.rs` (`maybe_hotspot` helper; `FillPrim`, `ImagePrim`)
- Test: `crates/carapace/src/vocab.rs` tests mod

**Interfaces:**
- Consumes: `Primitive::build -> Vec<Node>` (Task 1), `ctx.register_handler` (existing), `crate::scene::region_of`.
- Produces: `fill{ on_press=fn }` → `[Node::Fill, Node::Hotspot]`; `image{ on_press=fn }` → `[Node::Image, Node::Hotspot]`; without `on_press` → single node.

- [ ] **Step 1: Write the failing tests**

Add to the `vocab.rs` tests mod (note: `FillPrim`/`ImagePrim` need a ctx whose `register_handler` returns an id — `NoHandlers` returns 0; for fill use a counter-free check that a Hotspot is present):

```rust
    #[test]
    fn fill_without_on_press_emits_single_node() {
        let lua = Lua::new();
        let t = tbl(&lua, "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, color = {r=1,g=2,b=3} }");
        let v = FillPrim.build(&t, &mut NoHandlers).unwrap();
        assert_eq!(v.len(), 1);
        assert!(matches!(v[0], Node::Fill { .. }));
    }

    #[test]
    fn fill_with_on_press_emits_fill_then_hotspot() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, color = {r=1,g=2,b=3}, \
               on_press = function() end }",
        );
        let v = FillPrim.build(&t, &mut NoHandlers).unwrap();
        assert_eq!(v.len(), 2, "fill + hotspot");
        assert!(matches!(v[0], Node::Fill { .. }));
        assert!(matches!(v[1], Node::Hotspot { .. }));
    }

    #[test]
    fn image_with_on_press_emits_image_then_hotspot_from_dest_rect() {
        use crate::asset::DecodedImage;
        use std::sync::Arc;
        struct Ctx(Arc<DecodedImage>);
        impl BuildContext for Ctx {
            fn register_handler(&mut self, _f: Function) -> HandlerId { 7 }
            fn image(&mut self, _n: &str) -> Result<Arc<DecodedImage>, crate::asset::AssetError> {
                Ok(self.0.clone())
            }
            fn font(&mut self, n: &str) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError> {
                Err(crate::asset::AssetError::Unresolved(n.to_string()))
            }
        }
        let img = Arc::new(DecodedImage { rgba: vec![0; 4], width: 1, height: 1 });
        let lua = Lua::new();
        let t = tbl(&lua, "return { asset='a.png', x=10, y=20, w=30, h=40, on_press=function() end }");
        let v = ImagePrim.build(&t, &mut Ctx(img)).unwrap();
        assert_eq!(v.len(), 2);
        assert!(matches!(v[0], Node::Image { .. }));
        match &v[1] {
            Node::Hotspot { on_press, region } => {
                assert_eq!(*on_press, 7);
                // dest rect (10,20)-(40,60): a point inside hits, one outside misses.
                assert!(region.contains(hittest::Point { x: 25.0, y: 40.0 }));
                assert!(!region.contains(hittest::Point { x: 5.0, y: 5.0 }));
            }
            other => panic!("expected Hotspot, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p carapace vocab::tests::fill_with_on_press_emits_fill_then_hotspot`
Expected: FAIL — fill emits only `[Fill]` (no hotspot yet).

- [ ] **Step 3: Add the `maybe_hotspot` helper**

Add to `crates/carapace/src/vocab.rs` (after `parse_paint`, near the other helpers):

```rust
/// If `on_press` is present, register it and build a Hotspot over `region`.
fn maybe_hotspot(
    args: &Table,
    region: hittest::Region,
    ctx: &mut dyn BuildContext,
) -> Result<Option<Node>, BuildError> {
    match args.get::<Option<Function>>("on_press")? {
        Some(f) => Ok(Some(Node::Hotspot {
            region,
            on_press: ctx.register_handler(f),
        })),
        None => Ok(None),
    }
}
```

- [ ] **Step 4: Emit the hotspot from `FillPrim` and `ImagePrim`**

`FillPrim::build` (now takes `ctx`, not `_ctx`):

```rust
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let path = parse_path(args)?;
        let mut nodes = vec![Node::Fill {
            path: path.clone(),
            paint: parse_paint(args)?,
        }];
        if let Some(h) = maybe_hotspot(args, crate::scene::region_of(&path), ctx)? {
            nodes.push(h);
        }
        Ok(nodes)
    }
```

`ImagePrim::build` (append after building the `Image` node, before returning):

```rust
        let mut nodes = vec![Node::Image {
            image,
            dest: crate::scene::ImageDest { x, y, w, h },
        }];
        let corners = vec![
            crate::scene::Pt { x, y },
            crate::scene::Pt { x: x + w, y },
            crate::scene::Pt { x: x + w, y: y + h },
            crate::scene::Pt { x, y: y + h },
        ];
        if let Some(hs) = maybe_hotspot(args, crate::scene::region_of(&corners), ctx)? {
            nodes.push(hs);
        }
        Ok(nodes)
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p carapace vocab::`
Expected: PASS (the three new tests + existing).

- [ ] **Step 6: Clippy + commit**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/vocab.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(vocab): on_press on fill/image emits a shared-geometry hotspot"
```

---

## Task 3: `shape.rs` — pure shape path-generators

**Files:**
- Create: `crates/carapace/src/shape.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod shape;`)
- Test: `crates/carapace/src/shape.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  ```rust
  pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Pt>;                 // 4 corners CW from top-left
  pub fn circle(cx: f32, cy: f32, r: f32, segments: u32) -> Vec<Pt>;      // `segments` points at radius r
  pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, segments: u32) -> Vec<Pt>; // 4*segments pts
  ```

- [ ] **Step 1: Declare the module**

Add to `crates/carapace/src/lib.rs` (with the other `pub mod` lines, alphabetically near `scene`):

```rust
pub mod shape;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/carapace/src/shape.rs` with ONLY the tests first (so they fail to compile/RED):

```rust
use crate::scene::Pt;

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: Pt, bx: f32, by: f32) -> f32 {
        ((a.x - bx).powi(2) + (a.y - by).powi(2)).sqrt()
    }

    #[test]
    fn rect_is_four_corners_cw() {
        assert_eq!(
            rect(10.0, 20.0, 30.0, 40.0),
            vec![
                Pt { x: 10.0, y: 20.0 },
                Pt { x: 40.0, y: 20.0 },
                Pt { x: 40.0, y: 60.0 },
                Pt { x: 10.0, y: 60.0 },
            ]
        );
    }

    #[test]
    fn circle_has_n_points_on_the_radius() {
        let pts = circle(5.0, 6.0, 3.0, 16);
        assert_eq!(pts.len(), 16);
        for p in &pts {
            assert!((dist(*p, 5.0, 6.0) - 3.0).abs() < 1e-3, "point off the radius: {p:?}");
        }
    }

    #[test]
    fn rounded_rect_point_count_and_bounds() {
        let segs = 6;
        let pts = rounded_rect(0.0, 0.0, 100.0, 50.0, 8.0, segs);
        assert_eq!(pts.len() as u32, 4 * segs);
        for p in &pts {
            assert!(p.x >= -1e-3 && p.x <= 100.0 + 1e-3, "x out of box: {p:?}");
            assert!(p.y >= -1e-3 && p.y <= 50.0 + 1e-3, "y out of box: {p:?}");
        }
    }

    #[test]
    fn rounded_rect_radius_is_clamped() {
        // radius 999 on a 40x20 box clamps to min(w,h)/2 = 10; corner points stay within the box.
        let pts = rounded_rect(0.0, 0.0, 40.0, 20.0, 999.0, 4);
        for p in &pts {
            assert!(p.x >= -1e-3 && p.x <= 40.0 + 1e-3 && p.y >= -1e-3 && p.y <= 20.0 + 1e-3);
        }
    }
}
```

- [ ] **Step 3: Run to verify RED**

Run: `cargo test -p carapace shape::tests::rect_is_four_corners_cw`
Expected: FAIL — `rect`/`circle`/`rounded_rect` not found.

- [ ] **Step 4: Implement the generators**

Prepend the implementations above the test module in `crates/carapace/src/shape.rs`:

```rust
use crate::scene::Pt;

/// Axis-aligned rectangle as 4 corners, clockwise from the top-left.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Pt> {
    vec![
        Pt { x, y },
        Pt { x: x + w, y },
        Pt { x: x + w, y: y + h },
        Pt { x, y: y + h },
    ]
}

/// Circle approximated by `segments` points evenly spaced on the radius.
pub fn circle(cx: f32, cy: f32, r: f32, segments: u32) -> Vec<Pt> {
    let n = segments.max(3);
    (0..n)
        .map(|i| {
            let a = (i as f32) / (n as f32) * std::f32::consts::TAU;
            Pt {
                x: cx + r * a.cos(),
                y: cy + r * a.sin(),
            }
        })
        .collect()
}

/// Rounded rectangle: 4 corner arcs of `segments` points each (4*segments total); the straight
/// sides are the polygon edges between adjacent arc endpoints. `radius` is clamped to min(w,h)/2.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, segments: u32) -> Vec<Pt> {
    let seg = segments.max(1);
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    // Corner centers and the start angle of each 90° arc, ordered CW so the polygon is continuous:
    // top-right, bottom-right, bottom-left, top-left.
    let corners = [
        (x + w - r, y + r, -std::f32::consts::FRAC_PI_2), // TR: from top, sweeping to right
        (x + w - r, y + h - r, 0.0),                      // BR
        (x + r, y + h - r, std::f32::consts::FRAC_PI_2),  // BL
        (x + r, y + r, std::f32::consts::PI),             // TL
    ];
    let mut pts = Vec::with_capacity((4 * seg) as usize);
    for (ccx, ccy, start) in corners {
        for i in 0..seg {
            let a = start + (i as f32) / ((seg - 1).max(1) as f32) * std::f32::consts::FRAC_PI_2;
            pts.push(Pt {
                x: ccx + r * a.cos(),
                y: ccy + r * a.sin(),
            });
        }
    }
    pts
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p carapace shape::`
Expected: PASS (all four).

- [ ] **Step 6: Clippy + commit**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/shape.rs crates/carapace/src/lib.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(shape): pure circle/rect/rounded_rect path generators"
```

---

## Task 4: Inject `circle`/`rect`/`rounded_rect` Lua helpers

**Files:**
- Modify: `crates/carapace/src/script.rs` (inject helpers into the env in `load`)
- Test: `crates/carapace/tests/image_skin.rs` is asset-focused; add an inline-skin test in `crates/carapace/src/script.rs` tests mod instead.

**Interfaces:**
- Consumes: `crate::shape::{circle, rect, rounded_rect}` (Task 3).
- Produces: Lua globals `circle{cx,cy,r,segments=48}`, `rect{x,y,w,h}`, `rounded_rect{x,y,w,h,radius,segments=8}` returning a sequence of `{x=,y=}`, usable in any `path=`.

- [ ] **Step 1: Write the failing test**

Add to `crates/carapace/src/script.rs` tests mod:

```rust
    #[test]
    fn shape_helpers_produce_usable_paths() {
        let q = new_queue();
        // A circle path feeds `fill`; the fill builds with the tessellated polygon.
        let skin = load(
            &src("fill{ path = circle{cx=20, cy=20, r=10}, color = {r=1,g=2,b=3} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        match &skin.scene.nodes[0] {
            crate::scene::Node::Fill { path, .. } => {
                assert_eq!(path.len(), 48, "default circle segments");
            }
            other => panic!("expected Fill, got {other:?}"),
        }
    }

    #[test]
    fn rect_helper_makes_a_clickable_fill() {
        let q = new_queue();
        let skin = load(
            &src("fill{ path = rect{x=0,y=0,w=10,h=10}, color={r=0,g=0,b=0}, \
                       on_press=function() host.toggle() end }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        // fill + hotspot from the rect; the hotspot hit-tests inside the rect.
        assert_eq!(skin.scene.nodes.len(), 2);
        assert_eq!(skin.scene.hit(crate::scene::Pt { x: 5.0, y: 5.0 }), Some(0));
    }
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace script::tests::shape_helpers_produce_usable_paths`
Expected: FAIL — `circle` is an unknown global (sandbox rejects it).

- [ ] **Step 3: Inject the helpers**

In `crates/carapace/src/script.rs` `load`, after the `env.set("host", host_tbl)?;` line (and before `lua.load(...).set_environment(env).exec()?;`), add:

```rust
    // Base geometry sugar: pure path-generators injected into the sandbox. They return a
    // sequence of {x=,y=} usable in any `path=`; they emit no nodes and carry no capability.
    fn points_table(lua: &Lua, pts: &[crate::scene::Pt]) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        for (i, p) in pts.iter().enumerate() {
            let pt = lua.create_table()?;
            pt.set("x", p.x)?;
            pt.set("y", p.y)?;
            t.set(i + 1, pt)?;
        }
        Ok(t)
    }
    let circle = lua.create_function(|lua, a: Table| {
        let cx: f32 = a.get("cx")?;
        let cy: f32 = a.get("cy")?;
        let r: f32 = a.get("r")?;
        let segments: u32 = a.get::<Option<u32>>("segments")?.unwrap_or(48);
        points_table(lua, &crate::shape::circle(cx, cy, r, segments))
    })?;
    env.set("circle", circle)?;
    let rect = lua.create_function(|lua, a: Table| {
        let x: f32 = a.get("x")?;
        let y: f32 = a.get("y")?;
        let w: f32 = a.get("w")?;
        let h: f32 = a.get("h")?;
        points_table(lua, &crate::shape::rect(x, y, w, h))
    })?;
    env.set("rect", rect)?;
    let rounded_rect = lua.create_function(|lua, a: Table| {
        let x: f32 = a.get("x")?;
        let y: f32 = a.get("y")?;
        let w: f32 = a.get("w")?;
        let h: f32 = a.get("h")?;
        let radius: f32 = a.get("radius")?;
        let segments: u32 = a.get::<Option<u32>>("segments")?.unwrap_or(8);
        points_table(lua, &crate::shape::rounded_rect(x, y, w, h, radius, segments))
    })?;
    env.set("rounded_rect", rounded_rect)?;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace script::`
Expected: PASS. The `sandbox_blocks_globals_and_unknown_names` test still passes (it checks `frobnicate{}` etc., which remain unknown).

- [ ] **Step 5: Clippy + commit**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/script.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(script): inject circle/rect/rounded_rect path helpers into the sandbox"
```

---

## Task 5: `value_fill` direction — data, parse, summary (headless)

**Files:**
- Modify: `crates/carapace/src/scene.rs` (`FillDir`, `Node::ValueFill.direction`, `summary()`, the inline summary test)
- Modify: `crates/carapace/src/vocab.rs` (`parse_direction`, `ValueFillPrim`)
- Modify: `crates/carapace/src/render.rs` (pattern: ignore the new field for now, keep current behavior)
- Modify: `crates/carapace/tests/render_offscreen.rs` (add `direction` to the existing `ValueFill` literal so `gpu-tests` still compiles)
- Modify: `crates/carapace/tests/snapshots/behavior_snapshots__*.snap` (regenerate for `dir=right`)

**Interfaces:**
- Produces: `pub enum FillDir { Right, Left, Up, Down }` (`Clone, Copy, Debug, PartialEq`); `Node::ValueFill { …, direction: FillDir }`; `summary()` line `value_fill key=<k> dir=<d> rgba=…`.

- [ ] **Step 1: Update the inline summary test (RED)**

In `crates/carapace/src/scene.rs` tests, the `summary_is_stable_and_domain_neutral` test builds a `Node::ValueFill { … }` and expects `value_fill key=level rgba=1,2,3,255`. Change the literal to add `direction: FillDir::Right,` and the expected substring to `value_fill key=level dir=right rgba=1,2,3,255`.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace scene::tests::summary_is_stable_and_domain_neutral`
Expected: FAIL — no `FillDir`, `direction` field missing.

- [ ] **Step 3: Add `FillDir` and the field**

In `crates/carapace/src/scene.rs`, add near the other enums:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillDir {
    Right,
    Left,
    Up,
    Down,
}
```

Add `direction: FillDir,` to the `Node::ValueFill { … }` variant.

- [ ] **Step 4: Update `summary()` to print `dir=`**

In `summary()`, change the `Node::ValueFill` arm to bind `direction` and print it:

```rust
                Node::ValueFill {
                    value_key,
                    color,
                    direction,
                    ..
                } => {
                    let dir = match direction {
                        FillDir::Right => "right",
                        FillDir::Left => "left",
                        FillDir::Up => "up",
                        FillDir::Down => "down",
                    };
                    format!(
                        "value_fill key={} dir={} rgba={},{},{},{}",
                        value_key, dir, color.r, color.g, color.b, color.a
                    )
                }
```

- [ ] **Step 5: Parse `direction` in vocab**

In `crates/carapace/src/vocab.rs`, add a helper near `parse_halign`:

```rust
fn parse_direction(args: &Table) -> Result<crate::scene::FillDir, BuildError> {
    use crate::scene::FillDir;
    match args.get::<Option<String>>("direction")?.as_deref() {
        None | Some("right") => Ok(FillDir::Right),
        Some("left") => Ok(FillDir::Left),
        Some("up") => Ok(FillDir::Up),
        Some("down") => Ok(FillDir::Down),
        Some(_) => Err(BuildError::BadType("direction must be right|left|up|down")),
    }
}
```

In `ValueFillPrim::build`, add `direction: parse_direction(args)?,` to the `Node::ValueFill { … }` it returns.

- [ ] **Step 6: Keep `render.rs` compiling (ignore the field for now)**

In `crates/carapace/src/render.rs`, the `Node::ValueFill { path, value_key, color }` match arm now misses the new field. Change the pattern to ignore it (Task 6 rewrites the body):

```rust
                Node::ValueFill {
                    path,
                    value_key,
                    color,
                    ..
                } => {
```

(Leave the arm body unchanged in this task.)

- [ ] **Step 7: Add a parse test + keep gpu-tests compiling**

Add to `crates/carapace/src/vocab.rs` tests:

```rust
    #[test]
    fn value_fill_direction_parses_and_defaults() {
        use crate::scene::FillDir;
        let lua = Lua::new();
        let mk = |s: &str| {
            let t: Table = lua.load(s).eval().unwrap();
            match one(ValueFillPrim.build(&t, &mut NoHandlers)) {
                Node::ValueFill { direction, .. } => direction,
                other => panic!("expected ValueFill, got {other:?}"),
            }
        };
        let base = "return { path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, value='v', color={r=0,g=0,b=0}";
        assert_eq!(mk(&format!("{base} }}")), FillDir::Right); // default
        assert_eq!(mk(&format!("{base}, direction='up' }}")), FillDir::Up);
        let bad: Table = lua
            .load(&format!("{base}, direction='sideways' }}"))
            .eval()
            .unwrap();
        assert!(matches!(
            ValueFillPrim.build(&bad, &mut NoHandlers),
            Err(BuildError::BadType(_))
        ));
    }
```

In `crates/carapace/tests/render_offscreen.rs`, the `renders_fill_and_value_fill_at_sentinel_pixels` test builds a `Node::ValueFill { … }` literal — add `direction: carapace::scene::FillDir::Right,` to it so the `gpu-tests` target compiles.

- [ ] **Step 8: Regenerate the behavior snapshots**

The `behavior_snapshots` summaries now print `dir=right` on every `value_fill` line. Regenerate and confirm the diff is ONLY the `dir=right` insertion:

```bash
INSTA_UPDATE=always cargo test -p carapace --test behavior_snapshots
git diff -- crates/carapace/tests/snapshots/
```
Expected diff: every `value_fill key=level rgba=…` → `value_fill key=level dir=right rgba=…` (and the `rgba=9,9,9,255` line likewise). No other changes.

- [ ] **Step 9: Run headless + gpu-tests compile**

Run: `cargo test -p carapace --lib && cargo test -p carapace --test behavior_snapshots`
Then confirm the GPU target compiles (no GPU run needed): `cargo build -p carapace --features gpu-tests --tests`
Expected: PASS / compiles.

- [ ] **Step 10: Clippy + commit**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/scene.rs crates/carapace/src/vocab.rs crates/carapace/src/render.rs \
  crates/carapace/tests/render_offscreen.rs crates/carapace/tests/snapshots/
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(scene): value_fill direction (FillDir) — data, parse, summary"
```

---

## Task 6: `value_fill` render — direction-aware extent + clip-to-path (GPU)

**Files:**
- Modify: `crates/carapace/src/render.rs` (imports + the `Node::ValueFill` arm body)
- Test: `crates/carapace/tests/render_offscreen.rs` (append direction + clip sentinels)

**Interfaces:**
- Consumes: `FillDir` (Task 5), existing `value_of`, `bbox`, `bez`, `vcolor`.
- Produces: the fill is `path ∩ value-extent`, grown from the direction's edge.

- [ ] **Step 1: Write the failing GPU tests**

Append to `crates/carapace/tests/render_offscreen.rs` (reuses `offscreen`/`readback`/`px`/`rect`):

```rust
#[test]
fn value_fill_up_fills_from_the_bottom() {
    use carapace::scene::{Color, FillDir, Node, Pt, Scene};
    use carapace::state::StateValue;
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::ValueFill {
            path: rect(10.0, 0.0, 30.0, 100.0), // full-height bar
            value_key: "v".to_string(),
            color: Color { r: 0, g: 255, b: 0, a: 255 },
            direction: FillDir::Up,
        }],
    };
    r.draw(&scene, |k| if k == "v" { Some(StateValue::Scalar(0.5)) } else { None },
        &RenderTarget { device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h });
    let data = readback(&o);
    assert_eq!(px(&data, 100, 25, 80), [0, 255, 0], "bottom half filled (up, v=0.5)");
    assert_eq!(px(&data, 100, 25, 20), [0, 0, 0], "top half empty");
}

#[test]
fn value_fill_clips_to_a_non_rect_path() {
    use carapace::scene::{Color, FillDir, Node, Scene};
    use carapace::state::StateValue;
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    // A circle path filled fully (v=1.0). A pixel inside the bbox but OUTSIDE the circle
    // (near a corner of the bounding box) must stay background — proving clip-to-path.
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::ValueFill {
            path: carapace::shape::circle(50.0, 50.0, 40.0, 64),
            value_key: "v".to_string(),
            color: Color { r: 0, g: 255, b: 0, a: 255 },
            direction: FillDir::Right,
        }],
    };
    r.draw(&scene, |_k| Some(StateValue::Scalar(1.0)),
        &RenderTarget { device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h });
    let data = readback(&o);
    assert_eq!(px(&data, 100, 50, 50), [0, 255, 0], "center of the circle is filled");
    assert_eq!(px(&data, 100, 12, 12), [0, 0, 0], "bbox corner outside the circle stays background");
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen value_fill_up_fills_from_the_bottom`
Expected: FAIL — current arm fills the bbox left→right (ignores direction; `up` would leave the bottom-half sentinel wrong) and does not clip.

- [ ] **Step 3: Add the `Mix` import**

In `crates/carapace/src/render.rs`, add `Mix` to the `vello::peniko` import list:

```rust
use vello::peniko::{
    Blob, Brush, Color as VColor, Fill, Gradient as PGradient, ImageAlphaType, ImageBrush,
    ImageData, ImageFormat, ImageQuality, Mix,
};
```

- [ ] **Step 4: Rewrite the `Node::ValueFill` arm**

Replace the arm body with the direction-aware, clipped fill:

```rust
                Node::ValueFill {
                    path,
                    value_key,
                    color,
                    direction,
                } => {
                    use crate::scene::FillDir;
                    let v = value_of(&read_value, value_key);
                    let (x0, y0, x1, y1) = bbox(path);
                    let (w, h) = (x1 - x0, y1 - y0);
                    let extent = match direction {
                        FillDir::Right => Rect::new(x0, y0, x0 + w * v, y1),
                        FillDir::Left => Rect::new(x1 - w * v, y0, x1, y1),
                        FillDir::Up => Rect::new(x0, y1 - h * v, x1, y1),
                        FillDir::Down => Rect::new(x0, y0, x1, y0 + h * v),
                    };
                    // Clip to the actual path, then fill the value-extent rect: result = path ∩ extent.
                    vs.push_layer(Mix::Clip, 1.0, xform, &bez(path));
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &extent);
                    vs.pop_layer();
                }
```

(Note the pattern now binds `direction` instead of `..`.)

- [ ] **Step 5: Run the GPU tests**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS — the two new tests plus the existing fill/value_fill/image/gradient/text sentinels. The existing `renders_fill_and_value_fill_at_sentinel_pixels` (a `Right` bar) still passes — clip is a no-op for the rectangular path, reproducing prior output.

- [ ] **Step 6: Headless + clippy (both feature sets) + commit**

```bash
cargo test -p carapace --lib
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings
git add crates/carapace/src/render.rs crates/carapace/tests/render_offscreen.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(render): value_fill direction + clip-to-path (path ∩ extent)"
```

---

## Task 7: Demo payoff — refactor the `classic` skin

**Files:**
- Modify: `crates/carapace-demo/skins/classic/skin.lua`
- Modify: `crates/carapace-demo/tests/skins_build.rs` (assertions on the refactored scene)

**Interfaces:**
- Consumes: shapes (Task 4), `on_press` on fill (Task 2), `value_fill` direction (Tasks 5–6).

- [ ] **Step 1: Update the failing test**

In `crates/carapace-demo/tests/skins_build.rs`, the `classic_builds` test asserts `build("classic") >= 4`. Replace it with a stronger assertion proving the refactor (shapes + shared geometry + a vertical meter), and add a hit-test check:

```rust
#[test]
fn classic_uses_shared_geometry_and_a_vertical_meter() {
    use carapace::engine::Engine;
    use carapace::scene::{FillDir, Node, Pt};
    use carapace::vocab::VocabRegistry;
    use carapace_demo::demo_host::DemoHost;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/classic");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let e = Engine::new(Box::new(DemoHost::new()), VocabRegistry::base(), source).unwrap();
    let nodes = &e.scene().nodes;
    // shared geometry: at least one Hotspot emitted by a fill{on_press}
    assert!(nodes.iter().any(|n| matches!(n, Node::Hotspot { .. })), "has hotspots");
    // a vertical meter
    assert!(
        nodes.iter().any(|n| matches!(n, Node::ValueFill { direction: FillDir::Up, .. })),
        "has an upward value_fill meter"
    );
    // the play button (a fill{on_press}) is clickable at its center
    assert!(e.scene().hit(Pt { x: 55.0, y: 55.0 }).is_some(), "play button hotspot is hittable");
}
```

Delete the old `classic_builds` test (its `>= 4` count is superseded).

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo --test skins_build classic_uses_shared_geometry_and_a_vertical_meter`
Expected: FAIL — current `classic` skin has no `FillDir::Up` meter (and its buttons are separate region+fill, but the hotspot assertion may already pass; the meter assertion fails).

- [ ] **Step 3: Rewrite the classic skin**

Replace `crates/carapace-demo/skins/classic/skin.lua` with the ergonomic version (single declarations via shapes + `on_press`, a rounded button, a circle knob, and a vertical meter):

```lua
-- backdrop
fill{ path = rect{x=0, y=0, w=300, h=140}, color = {r=24, g=28, b=40} }

-- play button: one declaration is both drawn and clickable (was region{}+fill{})
fill{ path = rect{x=20, y=20, w=70, h=70}, color = {r=80, g=200, b=120},
      on_press = function() host.toggle_play() end }

-- stop button: a rounded chrome rect, also click-as-draw
fill{ path = rounded_rect{x=110, y=20, w=70, h=70, radius=12}, color = {r=200, g=80, b=80},
      on_press = function() host.stop() end }

-- a circular knob (decorative shape helper)
fill{ path = circle{cx=240, cy=55, r=28}, color = {r=180, g=180, b=70} }

-- horizontal seek bar bound to position
value_fill{ path = rect{x=20, y=110, w=260, h=16}, value = "position",
            color = {r=240, g=220, b=80} }

-- vertical meter bound to position, growing upward
value_fill{ path = rect{x=284, y=20, w=10, h=100}, value = "position", direction = "up",
            color = {r=120, g=230, b=200} }
```

- [ ] **Step 4: Run the demo tests**

Run: `cargo test -p carapace-demo`
Expected: PASS — the new `classic` assertion + the other skins' tests.

- [ ] **Step 5: Human smoke check (optional window) + compile**

Run: `cargo build -p carapace-demo` (must compile). Optionally `cargo run -p carapace-demo` and Tab to `classic`: the buttons click (toggle/stop), the rounded + circle shapes render, both meters advance while playing.

- [ ] **Step 6: Clippy + commit**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/skins/classic/skin.lua crates/carapace-demo/tests/skins_build.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): refactor classic skin to shapes + on_press + vertical meter"
```

---

## Task 8: README + roadmap refresh

**Files:**
- Modify: `README.md` (Roadmap; the "Domain-neutral base vocabulary" bullet)

**Interfaces:** none (docs only).

- [ ] **Step 1: Update the roadmap**

In `README.md` Roadmap, mark 5d done and trim the 5d–5e line. Replace the existing `5d`/`5d–5e` bullet with:

```markdown
- **Phase 5d — vocab ergonomics.** ✅ Shape path-helpers (`circle`/`rect`/`rounded_rect`);
  `on_press` on drawables (a control is drawn + clickable from one declaration);
  `value_fill` direction (right/left/up/down) + clip-to-path.
- **Phase 5e** — the host-extension registration mechanism.
```

- [ ] **Step 2: Update the vocabulary bullet**

In the "Domain-neutral base vocabulary, host-extensible" bullet (it currently lists `fill`, `region`, value-bound `value_fill`, `image`, and — after 5c — `text`), note the shape helpers and shared geometry. Append to that bullet:

```markdown
  Shapes (`circle`/`rect`/`rounded_rect`) are composable path-helpers, and any drawable can take
  an `on_press` to be both drawn and hit-testable from one declaration.
```

- [ ] **Step 3: Verify the suite + fmt + clippy**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings`
Expected: PASS / clean. (GPU suite separately: `cargo test -p carapace --features gpu-tests --test render_offscreen`.)

- [ ] **Step 4: Commit**

```bash
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs: README roadmap/vocab current through Phase 5d"
```

---

## Self-Review (completed during planning)

**Spec coverage:**
- Shape helpers (`circle`/`rect`/`rounded_rect`, pure + injected) → Tasks 3, 4. ✅
- `on_press` on drawables via `Primitive::build -> Vec<Node>` → Tasks 1, 2. ✅
- `value_fill` `FillDir` + parse + summary → Task 5; render direction + clip-to-path → Task 6. ✅
- Geometry-neutral `summary()` with `dir=` → Task 5 (+ snapshot regen). ✅
- No-visual-regression for rect bars (default Right, clip no-op) → Task 6 Step 5 (existing sentinel still passes). ✅
- Demo refactor (shapes + shared geometry + vertical meter) → Task 7. ✅
- README current per phase → Task 8. ✅
- Clippy gate (both feature sets) → Global Constraints + every task's commit step. ✅

**Deferred (per spec, no task):** diagonal/angled fills; clickable `value_fill`; ellipse/N-gon/star; image non-rect hit regions; per-shape stroke. RTL etc. n/a.

**Type consistency:** `Primitive::build -> Result<Vec<Node>, BuildError>` (Tasks 1–7); `FillDir { Right, Left, Up, Down }` (Tasks 5–7); `maybe_hotspot(args, hittest::Region, ctx) -> Result<Option<Node>, BuildError>` (Task 2); `shape::{rect,circle,rounded_rect}` signatures (Tasks 3–7). Consistent.

**Ordering/compile-safety:** Task 5 adds the `direction` field and immediately updates every `ValueFill` match/literal (scene summary + render pattern `..` + the gpu-tests literal) so the tree compiles under both feature sets before Task 6 uses the field — mirroring the 5c temporary-arm approach.

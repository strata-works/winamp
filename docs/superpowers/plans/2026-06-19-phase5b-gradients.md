# Phase 5b — Gradient Fills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generalize `fill{}` to paint any path with a solid color **or** a native linear/radial/sweep gradient, and give colors an alpha channel for translucent Y2K sheen.

**Architecture:** Introduce a `Paint = Solid(Color) | Gradient` value type in `scene.rs` (plain data — keeps the headless boundary). `vocab.rs` parses `color=` / `gradient=` from Lua into a `Paint`; `render.rs` is the only GPU step, turning a `Paint` into a peniko `Brush` (solid via `from_rgba8` with real alpha, gradient via `peniko::Gradient::new_*`). Gradients are static style (no state binding), so "scene is a pure projection of state" is untouched.

**Tech Stack:** Rust (edition 2024), vello 0.9.0 / peniko 0.6.1 (already vendored — **no new dependencies**), mlua 0.11.6, insta 1.48 (snapshot tests).

## Global Constraints

- **Rust edition 2024; built against Rust 1.96.** Dependencies are pinned via the committed `Cargo.lock`; do not run `cargo update`.
- **No new third-party dependencies.** Gradients use the already-present peniko/vello. (If that ever changed, the dependency fetch would have to run under `sfw` — but it does not apply here.)
- **Headless boundary:** `Paint`/`Gradient`/`Color`/`ColorStop` are plain data in `scene.rs`; only `render.rs` touches the GPU. Headless tests must build gradient scenes with no GPU.
- **GPU render tests are gated** behind the `gpu-tests` cargo feature (compiled out of `cargo test --workspace`).
- **`summary()` stays domain-neutral and deterministic** — node kinds + style + binding keys, **never float geometry**.
- **Gradient rules (from the spec):** angles in **degrees** (converted to radians at render); **radial is single-circle** (center + radius); **≥ 2 color stops required**; `at` clamped to [0,1] and stops sorted by `at`; extend mode = **Pad** (peniko default).
- **`ValueFill` stays solid** in 5b (it still gains alpha via the shared `Color`). Mesh, gradient `value_fill`, repeat/reflect, two-circle radial, and animated gradients are out of scope.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>`. **No "Generated with Claude" attribution** anywhere.

## File Structure

- `crates/carapace/src/scene.rs` — data model: `Color` gains `a`; new `ColorStop`, `Gradient`, `Paint`; `Node::Fill { color }` → `Node::Fill { paint }`; `summary()` lines.
- `crates/carapace/src/vocab.rs` — parsing: `color_from_table`, `parse_color` alpha, `parse_gradient`, `parse_paint`; `FillPrim` builds a `Paint`.
- `crates/carapace/src/render.rs` — `paint_brush(&Paint) -> peniko::Brush` (solid honors alpha, gradient builds a peniko brush); `Node::Fill` draws with it.
- `crates/carapace/tests/render_offscreen.rs` — gated GPU sentinels: translucent-blend (Task 1) + linear-gradient orientation/interpolation (Task 2).
- `crates/carapace/src/scene.rs` tests + `crates/carapace/tests/snapshots/*.snap` — updated for the `rgba=` summary.
- `crates/carapace-demo/skins/reference/skin.lua`, `crates/carapace-demo/skins/minimal/skin.lua`, `crates/carapace-demo/tests/skins_build.rs` — demo payoff.

---

### Task 1: `Color` alpha + the `Paint` indirection (solid only)

Introduce alpha and the `Paint` enum with **only** `Solid`, migrate every `Node::Fill` site onto it, and make the renderer honor alpha. No gradients yet — the workspace stays green end-to-end. (Splitting solid-first avoids a placeholder render arm: `Paint` is single-variant until Task 2 adds `Gradient`.)

**Files:**
- Modify: `crates/carapace/src/scene.rs`
- Modify: `crates/carapace/src/vocab.rs`
- Modify: `crates/carapace/src/render.rs`
- Modify: `crates/carapace/tests/render_offscreen.rs`
- Modify: `crates/carapace/tests/snapshots/behavior_snapshots__click_then_tick.snap`, `…__swap_preserves_state.snap`, `…__failed_swap_keeps_scene.snap`, `…__switch_host_resets.snap`

**Interfaces:**
- Produces:
  - `scene::Color { r: u8, g: u8, b: u8, a: u8 }` (was `{ r, g, b }`)
  - `scene::Paint { Solid(Color) }` (the `Gradient` variant is added in Task 2)
  - `scene::Node::Fill { path: Vec<Pt>, paint: Paint }` (was `{ path, color: Color }`)
  - `vocab::color_from_table(&Table) -> Result<Color, BuildError>` (reads `r`,`g`,`b`, optional `a` default 255)
  - `vocab::parse_color(&Table) -> Result<Color, BuildError>` (unchanged signature; now reads alpha via `color_from_table`)
- Consumes: existing `parse_path`, `BuildError`, `Primitive`, the vello `fill` API.

- [ ] **Step 1: Update the scene summary unit test to the `rgba=`/`Paint` shape (failing test)**

In `crates/carapace/src/scene.rs`, replace the body of `summary_is_stable_and_domain_neutral` so it builds a `Paint::Solid` fill with alpha and expects `rgba=`:

```rust
    #[test]
    fn summary_is_stable_and_domain_neutral() {
        let scene = Scene {
            canvas: (300, 120),
            nodes: vec![
                Node::Fill {
                    path: vec![Pt { x: 0.0, y: 0.0 }],
                    paint: Paint::Solid(Color { r: 10, g: 20, b: 30, a: 255 }),
                },
                Node::Hotspot {
                    region: region_of(&l_path()),
                    on_press: 2,
                },
                Node::ValueFill {
                    path: vec![Pt { x: 0.0, y: 0.0 }],
                    value_key: "level".to_string(),
                    color: Color { r: 1, g: 2, b: 3, a: 255 },
                },
            ],
        };
        let expected = "canvas 300x120\n\
                        fill rgba=10,20,30,255\n\
                        hotspot handler=2\n\
                        value_fill key=level rgba=1,2,3,255";
        assert_eq!(scene.summary(), expected);
    }
```

Also fix the other test that constructs a `Color` literal — in `summary_includes_image_nodes` the `DecodedImage` has no `Color`, so it is unaffected; no change there.

- [ ] **Step 2: Run it; verify it fails to compile**

Run: `cargo test -p carapace --lib scene 2>&1 | head -30`
Expected: compile error — `Color` has no field `a` / `Node::Fill` has no field `paint` / `Paint` not found.

- [ ] **Step 3: Add alpha + `Paint` + the new `Node::Fill` shape**

In `crates/carapace/src/scene.rs`, change `Color` and add `Paint`, and change the `Fill` variant:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    // Gradient(Gradient) is added in Task 2.
}
```

In the `Node` enum, change the `Fill` variant from `{ path: Vec<Pt>, color: Color }` to:

```rust
    Fill {
        path: Vec<Pt>,
        paint: Paint,
    },
```

- [ ] **Step 4: Update `summary()` for `Paint` + `rgba=`**

In `scene.rs`, replace the `Node::Fill` and `Node::ValueFill` arms of `summary()`:

```rust
                Node::Fill { paint, .. } => match paint {
                    Paint::Solid(c) => format!("fill rgba={},{},{},{}", c.r, c.g, c.b, c.a),
                },
                Node::Hotspot { on_press, .. } => format!("hotspot handler={}", on_press),
                Node::ValueFill {
                    value_key, color, ..
                } => format!(
                    "value_fill key={} rgba={},{},{},{}",
                    value_key, color.r, color.g, color.b, color.a
                ),
```

(The single-arm `match paint` compiles for a single-variant enum; Task 2 adds the `Gradient` arm.)

- [ ] **Step 5: Migrate `vocab.rs` to alpha + `Paint::Solid`**

In `crates/carapace/src/vocab.rs`, add `Paint` to the scene import and replace `parse_color` with a shared `color_from_table` helper (Task 2 reuses it for gradient stops):

```rust
use crate::scene::{Color, Gradient, HandlerId, Node, Paint, Pt};
```

(Adding `Gradient` to the import now is harmless — it is defined in Task 2; if it doesn't exist yet, import only `Paint` in this task and add `Gradient` in Task 2. To keep this task compiling, use: `use crate::scene::{Color, HandlerId, Node, Paint, Pt};`)

```rust
/// Reads r,g,b from a color table; optional `a` defaults to 255 (opaque).
pub fn color_from_table(c: &Table) -> Result<Color, BuildError> {
    Ok(Color {
        r: c.get("r")?,
        g: c.get("g")?,
        b: c.get("b")?,
        a: c.get::<Option<u8>>("a")?.unwrap_or(255),
    })
}

pub fn parse_color(t: &Table) -> Result<Color, BuildError> {
    let c: Table = t
        .get("color")
        .map_err(|_| BuildError::MissingField("color"))?;
    color_from_table(&c)
}
```

Change `FillPrim::build` to produce a `Paint::Solid`:

```rust
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        Ok(Node::Fill {
            path: parse_path(args)?,
            paint: Paint::Solid(parse_color(args)?),
        })
    }
```

- [ ] **Step 6: Update `vocab.rs` tests for the new shapes**

In `vocab.rs` tests, the `fill_builds_fill_node` test matches the old `Node::Fill { color, path }`. Replace it:

```rust
    #[test]
    fn fill_builds_fill_node() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, color = {r=1,g=2,b=3} }",
        );
        let node = FillPrim.build(&t, &mut NoHandlers).unwrap();
        match node {
            Node::Fill { paint, path } => {
                assert_eq!(paint, Paint::Solid(Color { r: 1, g: 2, b: 3, a: 255 }));
                assert_eq!(path.len(), 3);
            }
            other => panic!("expected Fill, got {other:?}"),
        }
    }
```

Add a focused alpha test right after it:

```rust
    #[test]
    fn color_alpha_defaults_opaque_and_parses_explicit() {
        let lua = Lua::new();
        let opaque: Table = lua.load("return { color = {r=1,g=2,b=3} }").eval().unwrap();
        assert_eq!(parse_color(&opaque).unwrap(), Color { r: 1, g: 2, b: 3, a: 255 });
        let translucent: Table = lua.load("return { color = {r=1,g=2,b=3,a=90} }").eval().unwrap();
        assert_eq!(parse_color(&translucent).unwrap(), Color { r: 1, g: 2, b: 3, a: 90 });
    }
```

The `value_fill_keeps_binding_key` test uses `color = {r=0,g=0,b=0}` (Lua) — no Rust `Color` literal — so it is unaffected.

- [ ] **Step 7: Migrate `render.rs` — alpha-honoring solid brush**

In `crates/carapace/src/render.rs`, add `Brush` and `Paint` to imports, replace `vcolor` to honor alpha, add `paint_brush`, and update the `Fill` arm:

```rust
use vello::peniko::{
    Blob, Brush, Color as VColor, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat,
    ImageQuality,
};
```
```rust
use crate::scene::{Color, Node, Paint, Pt, Scene};
```
```rust
fn vcolor(c: Color) -> VColor {
    VColor::from_rgba8(c.r, c.g, c.b, c.a)
}

/// A peniko brush for a Paint. (Task 2 adds the Gradient arm.)
fn paint_brush(paint: &Paint) -> Brush {
    match paint {
        Paint::Solid(c) => Brush::Solid(vcolor(*c)),
    }
}
```

Replace the `Node::Fill` draw arm:

```rust
                Node::Fill { path, paint } => {
                    vs.fill(Fill::NonZero, xform, &paint_brush(paint), None, &bez(path));
                }
```

The `Node::ValueFill` arm keeps `vcolor(*color)` (a `VColor` is `Into<BrushRef>`), now alpha-correct automatically.

- [ ] **Step 8: Run the full workspace test suite (headless) — green**

Run: `cargo test --workspace 2>&1 | tail -25`
Expected: all compile; the `behavior_snapshots` snapshot tests **FAIL** (their `.snap` files still say `rgb=`). Everything else passes. (Next step fixes the snapshots.)

- [ ] **Step 9: Update the 4 insta snapshots to `rgba=`**

The snapshots contain `value_fill key=level rgb=…` lines. Regenerate them:

Run: `INSTA_UPDATE=always cargo test -p carapace --test behavior_snapshots`
Then verify the change is only `rgb=…` → `rgba=…,255` (and the one `rgb=9,9,9` → `rgba=9,9,9,255`):

Run: `git diff --stat crates/carapace/tests/snapshots/ && grep -rn "rgb=" crates/carapace/tests/snapshots/ || echo "no bare rgb= left"`
Expected: the four `.snap` files changed; `grep` finds **no** bare `rgb=` (all now `rgba=`).

- [ ] **Step 10: Add the gated GPU translucent-blend sentinel**

In `crates/carapace/tests/render_offscreen.rs`, update the scene import and the existing `renders_fill_and_value_fill_at_sentinel_pixels` test's `Color`/`Fill` literals, then add a translucent-blend test.

Change the import line:

```rust
use carapace::scene::{Color, Node, Paint, Pt, Scene};
```

In `renders_fill_and_value_fill_at_sentinel_pixels`, change the red square node to use `paint` and give both colors `a: 255`:

```rust
            Node::Fill {
                path: vec![
                    Pt { x: 20.0, y: 20.0 },
                    Pt { x: 100.0, y: 20.0 },
                    Pt { x: 100.0, y: 100.0 },
                    Pt { x: 20.0, y: 100.0 },
                ],
                paint: Paint::Solid(Color { r: 255, g: 0, b: 0, a: 255 }),
            },
```
```rust
                value_key: "v".to_string(),
                color: Color { r: 0, g: 255, b: 0, a: 255 },
```

Add a `rect` helper and the new test at the end of the file:

```rust
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
    vec![
        Pt { x: x0, y: y0 },
        Pt { x: x1, y: y0 },
        Pt { x: x1, y: y1 },
        Pt { x: x0, y: y1 },
    ]
}

#[test]
fn renders_translucent_fill_blended_over_background() {
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![
            // opaque red background
            Node::Fill {
                path: rect(0.0, 0.0, 100.0, 100.0),
                paint: Paint::Solid(Color { r: 255, g: 0, b: 0, a: 255 }),
            },
            // 50%-alpha blue over it
            Node::Fill {
                path: rect(0.0, 0.0, 100.0, 100.0),
                paint: Paint::Solid(Color { r: 0, g: 0, b: 255, a: 128 }),
            },
        ],
    };
    let read = |_k: &str| None;
    r.draw(
        &scene,
        read,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
        },
    );
    let data = readback(&o);
    let p = px(&data, 100, 50, 50);
    // Straight-alpha "blue over red": result ≈ blend of the two. This also pins that
    // alpha is wired through `from_rgba8` (not dropped to 255). Tolerance absorbs the
    // pipeline's blend-space nuance (vello 0.9 byte-passthrough).
    assert!((p[0] as i32 - 127).abs() <= 16, "R blended toward ~127, got {}", p[0]);
    assert!(p[1] <= 8, "no green, got {}", p[1]);
    assert!((p[2] as i32 - 128).abs() <= 16, "B blended toward ~128, got {}", p[2]);
}
```

- [ ] **Step 11: Run the gated GPU test (local Metal)**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen 2>&1 | tail -20`
Expected: all 3 tests pass (the two existing + the new blend sentinel). If `p` differs from ~127/~128 because the pipeline blends in a different space, widen the tolerance to the nearest value the run reports and note it in the comment — do **not** assert green/zero-alpha behavior changed.

- [ ] **Step 12: Confirm fmt + clippy + headless suite**

Run: `cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace 2>&1 | tail -15`
Expected: fmt clean, no clippy warnings, all headless tests pass.

- [ ] **Step 13: Commit**

```bash
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(engine): Color alpha + Paint indirection (solid)

Color gains an alpha channel (default 255). Node::Fill now carries a
Paint = Solid(Color) instead of a bare Color; render honors alpha via
from_rgba8. summary() solid/value_fill lines become rgba=. No gradients
yet (Paint is single-variant until Task 2). Snapshots + GPU blend sentinel updated."
```

---

### Task 2: Gradients end-to-end (data model + parse + render)

Add the `Gradient` data model and the `Paint::Gradient` variant, parse `gradient={…}` from Lua, and render all three kinds as peniko brushes. The variant and its render land together so the `match` stays exhaustive with no placeholder.

**Files:**
- Modify: `crates/carapace/src/scene.rs`
- Modify: `crates/carapace/src/vocab.rs`
- Modify: `crates/carapace/src/render.rs`
- Modify: `crates/carapace/tests/render_offscreen.rs`

**Interfaces:**
- Consumes (from Task 1): `scene::Color { r,g,b,a }`, `scene::Paint::Solid`, `scene::Node::Fill { path, paint }`, `vocab::color_from_table`.
- Produces:
  - `scene::ColorStop { at: f32, color: Color }`
  - `scene::Gradient::{ Linear { from: Pt, to: Pt, stops: Vec<ColorStop> }, Radial { center: Pt, radius: f32, stops: Vec<ColorStop> }, Sweep { center: Pt, start_deg: f32, end_deg: f32, stops: Vec<ColorStop> } }`
  - `scene::Paint::Gradient(Gradient)`
  - `vocab::parse_gradient(&Table) -> Result<Gradient, BuildError>`, `vocab::parse_paint(&Table) -> Result<Paint, BuildError>`
  - `render::paint_brush` gains the `Gradient` arm.

- [ ] **Step 1: Write failing parse tests for gradients**

In `crates/carapace/src/vocab.rs` tests, add:

```rust
    #[test]
    fn fill_builds_linear_gradient() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, gradient = { \
               type='linear', from={x=0,y=0}, to={x=0,y=40}, \
               stops = { {at=1, color={r=9,g=9,b=9,a=0}}, {at=0, color={r=255,g=255,b=255,a=120}} } } }",
        );
        match FillPrim.build(&t, &mut NoHandlers).unwrap() {
            Node::Fill { paint: Paint::Gradient(Gradient::Linear { from, to, stops }), .. } => {
                assert_eq!((from, to), (Pt { x: 0.0, y: 0.0 }, Pt { x: 0.0, y: 40.0 }));
                // stops sorted by `at`
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0].at, 0.0);
                assert_eq!(stops[0].color, Color { r: 255, g: 255, b: 255, a: 120 });
                assert_eq!(stops[1].at, 1.0);
            }
            other => panic!("expected linear gradient fill, got {other:?}"),
        }
    }

    #[test]
    fn radial_and_sweep_parse_with_defaults() {
        let lua = Lua::new();
        let radial = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='radial', center={x=5,y=6}, radius=7, \
               stops = { {at=0, color={r=0,g=0,b=0}}, {at=1, color={r=1,g=1,b=1}} } } }",
        );
        match FillPrim.build(&radial, &mut NoHandlers).unwrap() {
            Node::Fill { paint: Paint::Gradient(Gradient::Radial { center, radius, .. }), .. } => {
                assert_eq!((center, radius), (Pt { x: 5.0, y: 6.0 }, 7.0));
            }
            other => panic!("expected radial, got {other:?}"),
        }
        // sweep with default angles 0..360
        let sweep = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='sweep', center={x=2,y=3}, \
               stops = { {at=0, color={r=0,g=0,b=0}}, {at=1, color={r=1,g=1,b=1}} } } }",
        );
        match FillPrim.build(&sweep, &mut NoHandlers).unwrap() {
            Node::Fill { paint: Paint::Gradient(Gradient::Sweep { start_deg, end_deg, .. }), .. } => {
                assert_eq!((start_deg, end_deg), (0.0, 360.0));
            }
            other => panic!("expected sweep, got {other:?}"),
        }
    }

    #[test]
    fn gradient_rejects_bad_type_and_too_few_stops() {
        let lua = Lua::new();
        let bad_type = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='conic', from={x=0,y=0}, to={x=1,y=1}, \
               stops = { {at=0, color={r=0,g=0,b=0}}, {at=1, color={r=1,g=1,b=1}} } } }",
        );
        assert!(matches!(FillPrim.build(&bad_type, &mut NoHandlers), Err(BuildError::BadType(_))));
        let one_stop = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, gradient = { \
               type='linear', from={x=0,y=0}, to={x=1,y=1}, \
               stops = { {at=0, color={r=0,g=0,b=0}} } } }",
        );
        assert!(matches!(FillPrim.build(&one_stop, &mut NoHandlers), Err(BuildError::BadType(_))));
    }
```

- [ ] **Step 2: Run them; verify they fail to compile**

Run: `cargo test -p carapace --lib vocab 2>&1 | head -20`
Expected: compile error — `Gradient` / `ColorStop` / `Paint::Gradient` not found.

- [ ] **Step 3: Add the gradient data model to `scene.rs`**

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub at: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    Linear { from: Pt, to: Pt, stops: Vec<ColorStop> },
    Radial { center: Pt, radius: f32, stops: Vec<ColorStop> },
    Sweep { center: Pt, start_deg: f32, end_deg: f32, stops: Vec<ColorStop> },
}
```

Add the variant to `Paint`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    Gradient(Gradient),
}
```

- [ ] **Step 4: Add the gradient `summary()` arm**

In `scene.rs` `summary()`, extend the `Node::Fill` match:

```rust
                Node::Fill { paint, .. } => match paint {
                    Paint::Solid(c) => format!("fill rgba={},{},{},{}", c.r, c.g, c.b, c.a),
                    Paint::Gradient(g) => {
                        let (kind, n) = match g {
                            Gradient::Linear { stops, .. } => ("linear", stops.len()),
                            Gradient::Radial { stops, .. } => ("radial", stops.len()),
                            Gradient::Sweep { stops, .. } => ("sweep", stops.len()),
                        };
                        format!("fill gradient={} stops={}", kind, n)
                    }
                },
```

Add a unit test for it in `scene.rs` tests:

```rust
    #[test]
    fn summary_describes_gradient_fills() {
        let scene = Scene {
            canvas: (10, 10),
            nodes: vec![Node::Fill {
                path: vec![Pt { x: 0.0, y: 0.0 }],
                paint: Paint::Gradient(Gradient::Linear {
                    from: Pt { x: 0.0, y: 0.0 },
                    to: Pt { x: 0.0, y: 10.0 },
                    stops: vec![
                        ColorStop { at: 0.0, color: Color { r: 0, g: 0, b: 0, a: 255 } },
                        ColorStop { at: 1.0, color: Color { r: 255, g: 255, b: 255, a: 0 } },
                    ],
                }),
            }],
        };
        assert_eq!(scene.summary(), "canvas 10x10\nfill gradient=linear stops=2");
    }
```

- [ ] **Step 5: Implement `parse_gradient` + `parse_paint` in `vocab.rs`**

Add `Gradient` (and `ColorStop`) to the scene import:

```rust
use crate::scene::{Color, ColorStop, Gradient, HandlerId, Node, Paint, Pt};
```

Add the helpers (after `parse_color`):

```rust
fn parse_pt(t: &Table, key: &'static str) -> Result<Pt, BuildError> {
    let p: Table = t.get(key).map_err(|_| BuildError::MissingField(key))?;
    Ok(Pt {
        x: p.get("x")?,
        y: p.get("y")?,
    })
}

fn parse_stops(g: &Table) -> Result<Vec<ColorStop>, BuildError> {
    let stops_t: Table = g.get("stops").map_err(|_| BuildError::MissingField("stops"))?;
    let mut stops = Vec::new();
    for entry in stops_t.sequence_values::<Table>() {
        let e = entry?;
        let at: f32 = e.get("at").map_err(|_| BuildError::MissingField("at"))?;
        let color_t: Table = e.get("color").map_err(|_| BuildError::MissingField("color"))?;
        stops.push(ColorStop {
            at: at.clamp(0.0, 1.0),
            color: color_from_table(&color_t)?,
        });
    }
    if stops.len() < 2 {
        return Err(BuildError::BadType("gradient needs >= 2 stops"));
    }
    stops.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    Ok(stops)
}

fn parse_gradient(t: &Table) -> Result<Gradient, BuildError> {
    let g: Table = t
        .get("gradient")
        .map_err(|_| BuildError::MissingField("gradient"))?;
    let kind: String = g.get("type").map_err(|_| BuildError::MissingField("type"))?;
    let stops = parse_stops(&g)?;
    Ok(match kind.as_str() {
        "linear" => Gradient::Linear {
            from: parse_pt(&g, "from")?,
            to: parse_pt(&g, "to")?,
            stops,
        },
        "radial" => Gradient::Radial {
            center: parse_pt(&g, "center")?,
            radius: g.get("radius").map_err(|_| BuildError::MissingField("radius"))?,
            stops,
        },
        "sweep" => Gradient::Sweep {
            center: parse_pt(&g, "center")?,
            start_deg: g.get::<Option<f32>>("start_deg")?.unwrap_or(0.0),
            end_deg: g.get::<Option<f32>>("end_deg")?.unwrap_or(360.0),
            stops,
        },
        _ => return Err(BuildError::BadType("gradient type must be linear|radial|sweep")),
    })
}

fn parse_paint(args: &Table) -> Result<Paint, BuildError> {
    if args.contains_key("gradient")? {
        Ok(Paint::Gradient(parse_gradient(args)?))
    } else {
        Ok(Paint::Solid(parse_color(args)?))
    }
}
```

Change `FillPrim::build` to use `parse_paint`:

```rust
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        Ok(Node::Fill {
            path: parse_path(args)?,
            paint: parse_paint(args)?,
        })
    }
```

- [ ] **Step 6: Run the headless vocab + scene tests — green**

Run: `cargo test -p carapace --lib 2>&1 | tail -15`
Expected: the new gradient parse tests + `summary_describes_gradient_fills` pass; existing tests still pass.

- [ ] **Step 7: Implement the gradient render arm**

In `crates/carapace/src/render.rs`, import the peniko gradient type (aliased to avoid clashing with `scene::Gradient`) and `kurbo::Point`:

```rust
use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{
    Blob, Brush, Color as VColor, Fill, Gradient as PGradient, ImageAlphaType, ImageBrush,
    ImageData, ImageFormat, ImageQuality,
};
```
```rust
use crate::scene::{Color, ColorStop, Gradient, Node, Paint, Pt, Scene};
```

Add a stops converter and extend `paint_brush`:

```rust
fn pstops(stops: &[ColorStop]) -> Vec<(f32, VColor)> {
    stops
        .iter()
        .map(|s| (s.at, VColor::from_rgba8(s.color.r, s.color.g, s.color.b, s.color.a)))
        .collect()
}

fn paint_brush(paint: &Paint) -> Brush {
    match paint {
        Paint::Solid(c) => Brush::Solid(vcolor(*c)),
        Paint::Gradient(g) => Brush::Gradient(match g {
            Gradient::Linear { from, to, stops } => PGradient::new_linear(
                KPoint::new(from.x as f64, from.y as f64),
                KPoint::new(to.x as f64, to.y as f64),
            )
            .with_stops(&pstops(stops)[..]),
            Gradient::Radial { center, radius, stops } => PGradient::new_radial(
                KPoint::new(center.x as f64, center.y as f64),
                *radius,
            )
            .with_stops(&pstops(stops)[..]),
            Gradient::Sweep { center, start_deg, end_deg, stops } => PGradient::new_sweep(
                KPoint::new(center.x as f64, center.y as f64),
                start_deg.to_radians(),
                end_deg.to_radians(),
            )
            .with_stops(&pstops(stops)[..]),
        }),
    }
}
```

The `Node::Fill` draw arm already calls `paint_brush(paint)` (from Task 1) — no change needed there. Gradient coordinates are canvas-space and drawn under the existing `xform`.

- [ ] **Step 8: Add the gated GPU linear-gradient sentinel**

In `crates/carapace/tests/render_offscreen.rs`, extend the scene import:

```rust
use carapace::scene::{Color, ColorStop, Gradient, Node, Paint, Pt, Scene};
```

Add the test (uses the `rect` helper from Task 1):

```rust
#[test]
fn renders_linear_gradient_oriented_and_interpolating() {
    let o = offscreen(200, 50);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 50),
        nodes: vec![Node::Fill {
            path: rect(0.0, 0.0, 200.0, 50.0),
            paint: Paint::Gradient(Gradient::Linear {
                from: Pt { x: 0.0, y: 0.0 },
                to: Pt { x: 200.0, y: 0.0 },
                stops: vec![
                    ColorStop { at: 0.0, color: Color { r: 255, g: 0, b: 0, a: 255 } },
                    ColorStop { at: 1.0, color: Color { r: 0, g: 0, b: 255, a: 255 } },
                ],
            }),
        }],
    };
    let read = |_k: &str| None;
    r.draw(
        &scene,
        read,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
        },
    );
    let data = readback(&o);
    let left = px(&data, 200, 10, 25);
    let mid = px(&data, 200, 100, 25);
    let right = px(&data, 200, 190, 25);
    // Endpoints + orientation: left is red-dominant, right is blue-dominant (horizontal red→blue).
    assert!(left[0] > 200 && left[2] < 60, "left ~red, got {:?}", left);
    assert!(right[2] > 200 && right[0] < 60, "right ~blue, got {:?}", right);
    // Interpolation: R decreases and B increases left→right (robust to interpolation color space).
    assert!(mid[0] < left[0] && mid[0] > right[0], "R decreases L→R, mid {:?}", mid);
    assert!(mid[2] > left[2] && mid[2] < right[2], "B increases L→R, mid {:?}", mid);
}
```

- [ ] **Step 9: Run the gated GPU tests (local Metal)**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen 2>&1 | tail -20`
Expected: all 4 tests pass (Task 1's 3 + this gradient sentinel).

- [ ] **Step 10: fmt + clippy + headless suite**

Run: `cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace 2>&1 | tail -15`
Expected: clean; all headless tests pass.

- [ ] **Step 11: Commit**

```bash
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(engine): linear/radial/sweep gradient fills

Add Gradient + ColorStop + Paint::Gradient; parse gradient={...} (type,
geometry, >=2 stops, sorted/clamped, degrees, single-circle radial);
render via peniko Gradient brushes (Pad extend). summary() prints
'fill gradient=<kind> stops=<n>'. Gated GPU sentinel for orientation +
interpolation."
```

---

### Task 3: Demo payoff — gradient accents in the skins

Put gradients on screen: a translucent glass sheen + a radial glossy highlight over the Headspace bitmap, and a sweep swatch on the `minimal` vector skin so all three kinds render live.

**Files:**
- Modify: `crates/carapace-demo/skins/reference/skin.lua`
- Modify: `crates/carapace-demo/skins/minimal/skin.lua`
- Modify: `crates/carapace-demo/tests/skins_build.rs`

**Interfaces:**
- Consumes: the `fill{ gradient = {…} }` Lua API from Task 2; `scene::Node`, `scene::Paint` for the headless build assertions.

- [ ] **Step 1: Write failing headless build assertions**

In `crates/carapace-demo/tests/skins_build.rs`, add `Paint` to the import inside the headspace test and add two assertions. First, extend `headspace_reference_builds_with_bitmap` with a gradient check (add before its closing brace):

```rust
    use carapace::scene::Paint;
    assert!(
        nodes
            .iter()
            .any(|n| matches!(n, Node::Fill { paint: Paint::Gradient(_), .. })),
        "reference skin now has gradient sheen/glossy accents"
    );
```

Then add a new test:

```rust
#[test]
fn minimal_has_a_sweep_gradient() {
    use carapace::scene::{Gradient, Node, Paint};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/minimal");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let e = carapace::engine::Engine::new(
        Box::new(carapace_demo::demo_host::DemoHost::new()),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .unwrap();
    assert!(
        e.scene().nodes.iter().any(|n| matches!(
            n,
            Node::Fill { paint: Paint::Gradient(Gradient::Sweep { .. }), .. }
        )),
        "minimal skin shows a sweep gradient"
    );
}
```

- [ ] **Step 2: Run them; verify they fail**

Run: `cargo test -p carapace-demo --test skins_build 2>&1 | tail -20`
Expected: the two new assertions FAIL (skins have no gradient nodes yet).

- [ ] **Step 3: Add gradient accents to the reference skin**

In `crates/carapace-demo/skins/reference/skin.lua`, after the `image{…}` line and before the `region{…}` overlays, add:

```lua
-- Y2K glass sheen across the header band (translucent white, fading down).
fill{ path = {{x=0,y=0},{x=342,y=0},{x=342,y=46},{x=0,y=46}}, gradient = {
  type = "linear", from = {x=0,y=0}, to = {x=0,y=46},
  stops = { {at=0, color={r=255,g=255,b=255, a=110}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }
-- Radial glossy highlight over the play transport.
fill{ path = {{x=148,y=18},{x=184,y=18},{x=184,y=54},{x=148,y=54}}, gradient = {
  type = "radial", center = {x=166,y=36}, radius = 18,
  stops = { {at=0, color={r=255,g=255,b=255, a=170}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }
```

- [ ] **Step 4: Add a sweep swatch to the minimal skin**

In `crates/carapace-demo/skins/minimal/skin.lua`, append (canvas is 300×140, so the top-right corner is free):

```lua
-- A sweep-gradient swatch in the top-right corner (shows the third gradient kind).
fill{ path = {{x=270,y=8},{x=294,y=8},{x=294,y=32},{x=270,y=32}}, gradient = {
  type = "sweep", center = {x=282,y=20}, start_deg = 0, end_deg = 360,
  stops = { {at=0, color={r=255,g=90,b=90}}, {at=0.5, color={r=90,g=130,b=255}},
            {at=1, color={r=255,g=90,b=90}} } } }
```

- [ ] **Step 5: Run the demo build tests — green**

Run: `cargo test -p carapace-demo --test skins_build 2>&1 | tail -20`
Expected: all pass — `headspace_reference_builds_with_bitmap` (now with the gradient assertion), `minimal_has_a_sweep_gradient`, `classic_builds`, `minimal_builds`.

- [ ] **Step 6: Human visual check**

Run: `cargo run -p carapace-demo`
Expected: the `reference` skin shows a soft white sheen across the header and a glossy highlight over the play button, laid over the Headspace photo; `Tab` to `minimal` shows the angular sweep swatch top-right; a `Tab`-swap preserves the seek bar. (Close the window to end.)

- [ ] **Step 7: fmt + clippy + full workspace suite**

Run: `cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace 2>&1 | tail -15`
Expected: clean; all headless tests pass.

- [ ] **Step 8: Commit**

```bash
git add -A
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): gradient accents — glass sheen + glossy over Headspace, sweep swatch

reference skin gets a translucent linear header sheen and a radial glossy
highlight over the play transport; minimal skin gets a sweep swatch so all
three gradient kinds render live. Headless build asserts the gradient nodes."
```

---

## Self-Review

**1. Spec coverage:**
- Paint = Solid|Gradient generalizing `fill{}` → Task 1 (Solid + indirection), Task 2 (Gradient). ✅
- Color alpha (default 255), used by solid + stops → Task 1 (`color_from_table`), reused for stops in Task 2. ✅
- Linear/radial/sweep, native peniko brushes → Task 2 render. ✅
- Lua API (`type`, geometry, `stops`, degrees, ≥2 stops, sort/clamp, defaults) → Task 2 `parse_gradient`. ✅
- Single-circle radial, Pad extend → Task 2 (`new_radial`, `Extend::default()`). ✅
- `summary()` `rgba=` + `gradient=<kind> stops=<n>`, value_fill rgba → Task 1 + Task 2 + snapshot update. ✅
- ValueFill stays solid, gains alpha via shared Color → Task 1. ✅
- Headless boundary (plain data; only render GPU) → data types in scene.rs; tests build scenes headlessly. ✅
- GPU sentinels: gradient interp + translucent blend → Task 1 (blend), Task 2 (gradient). ✅
- Demo payoff (sheen + glossy over Headspace, sweep on vector skin) → Task 3. ✅
- Out of scope (mesh, gradient value_fill, repeat/reflect, two-circle, animated) → not implemented. ✅

**2. Placeholder scan:** No TBD/TODO; every code step shows complete code; every run step has an exact command + expected result. The one judgment note (Task 1 Step 11 tolerance widening) is a concrete fallback with a rule, not a placeholder.

**3. Type consistency:** `Color { r,g,b,a }`, `Paint::{Solid,Gradient}`, `Gradient::{Linear{from,to,stops}, Radial{center,radius,stops}, Sweep{center,start_deg,end_deg,stops}}`, `ColorStop{at,color}`, `Node::Fill{path,paint}`, `color_from_table`/`parse_color`/`parse_gradient`/`parse_paint`, `paint_brush`/`pstops`/`rect` are named identically across all tasks. `PGradient` aliases peniko's `Gradient` to avoid the clash with `scene::Gradient`. Import lines are stated per task (Task 1 imports `Paint`; Task 2 adds `Gradient`/`ColorStop`).

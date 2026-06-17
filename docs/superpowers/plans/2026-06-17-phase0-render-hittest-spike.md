# Phase 0 — Rendering / Hit-Test Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Choose the engine's 2D rendering backend by proving, against the one hard constraint — clean per-pixel hit resolution on a concave free-form region with hit-testing fully decoupled from rendering — across three candidates (`tiny-skia`, `wgpu`, `vello`).

**Architecture:** A standalone `hittest` crate computes inside/outside for arbitrary polygon regions (even-odd, multi-contour) with **zero rendering dependency**. A `spike-render` crate defines a `Renderer` trait producing an offscreen RGBA8 `Pixmap`, implemented by each candidate. An objective **parity gate** asserts that every unambiguous (non-antialiased) pixel a backend fills agrees with `hittest`'s independent inside/outside verdict for that pixel's center. The backend that passes the gate with the least code and dependency weight wins.

**Tech Stack:** Rust (stable, edition 2021), Cargo workspace. Candidate render crates: `tiny-skia`, `wgpu` (+ `lyon` for tessellation), `vello`. No Tauri in this phase (windowing/shell integration is deferred to a later phase — the spike runs headless/offscreen so its gate is deterministic and display-independent).

## Global Constraints

- Language: Rust, **edition 2021**, stable toolchain. Every crate sets `edition = "2021"`.
- Repository layout: a single Cargo **workspace** at repo root; all crates under `crates/`.
- Crate versions: install with `cargo add <crate>` so Cargo resolves the current release. Do **not** hand-pin versions in this plan.
- The `hittest` crate MUST NOT depend on any rendering, GPU, windowing, or image crate. This decoupling is the whole point — a dependency edge from `hittest` to a render crate is a task failure.
- All rendering in this phase is **offscreen** (render-to-buffer / readback). No window, no event loop, no display required.
- Canvas for every test is **200×200** px. Colors are opaque RGBA8: fill = `[255, 0, 0, 255]` (red), background = `[0, 0, 0, 255]` (black).
- The two canonical test regions (used verbatim everywhere) are defined in Task 1, Step 3.

---

### Task 1: `hittest` kernel — concave + holed region containment

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/hittest/Cargo.toml`
- Create: `crates/hittest/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct Point { pub x: f32, pub y: f32 }`
  - `pub struct Contour { pub points: Vec<Point> }`
  - `pub struct Region { pub contours: Vec<Contour> }`
  - `impl Region { pub fn contains(&self, p: Point) -> bool }` — even-odd fill across all contours (so a region with an inner contour is a hole).
  - `pub fn l_shape() -> Region` and `pub fn ring() -> Region` — the two canonical test regions, reused by every later task.

- [ ] **Step 1: Create the workspace root manifest**

Create `Cargo.toml`:

```toml
[workspace]
members = ["crates/hittest", "crates/spike-render"]
resolver = "2"
```

- [ ] **Step 2: Create the `hittest` crate manifest**

Create `crates/hittest/Cargo.toml`:

```toml
[package]
name = "hittest"
version = "0.0.0"
edition = "2021"

[dependencies]
```

(No dependencies — enforced by Global Constraints.)

- [ ] **Step 3: Write the failing test for a convex square**

Create `crates/hittest/src/lib.rs` with the types, canonical regions, an empty `contains`, and the first test:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct Contour {
    pub points: Vec<Point>,
}

#[derive(Clone, Debug)]
pub struct Region {
    pub contours: Vec<Contour>,
}

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

impl Region {
    pub fn contains(&self, _p: Point) -> bool {
        unimplemented!()
    }
}

/// Concave L-shape on a 200x200 canvas. Concave vertex at (90, 90).
/// Inside examples: (60, 60), (130, 60). Outside (the notch): (130, 130).
pub fn l_shape() -> Region {
    Region {
        contours: vec![Contour {
            points: vec![
                pt(40.0, 40.0),
                pt(160.0, 40.0),
                pt(160.0, 90.0),
                pt(90.0, 90.0),
                pt(90.0, 160.0),
                pt(40.0, 160.0),
            ],
        }],
    }
}

/// Square ring (square with a square hole) on a 200x200 canvas.
/// Inside the ring material: (50, 100). Inside the hole (=outside region): (100, 100).
pub fn ring() -> Region {
    Region {
        contours: vec![
            Contour {
                points: vec![
                    pt(40.0, 40.0),
                    pt(160.0, 40.0),
                    pt(160.0, 160.0),
                    pt(40.0, 160.0),
                ],
            },
            Contour {
                points: vec![
                    pt(80.0, 80.0),
                    pt(120.0, 80.0),
                    pt(120.0, 120.0),
                    pt(80.0, 120.0),
                ],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region {
            contours: vec![Contour {
                points: vec![
                    pt(0.0, 0.0),
                    pt(10.0, 0.0),
                    pt(10.0, 10.0),
                    pt(0.0, 10.0),
                ],
            }],
        }
    }

    #[test]
    fn point_inside_convex_square() {
        assert!(square().contains(pt(5.0, 5.0)));
    }

    #[test]
    fn point_outside_convex_square() {
        assert!(!square().contains(pt(15.0, 5.0)));
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p hittest`
Expected: FAIL — panics with `not implemented` from `unimplemented!()`.

- [ ] **Step 5: Implement `contains` (even-odd ray cast across all contours)**

Replace the `contains` body in `crates/hittest/src/lib.rs`:

```rust
    pub fn contains(&self, p: Point) -> bool {
        // Crossing-number (PNPOLY) ray cast to the right, accumulated across
        // every contour into a single parity. Two nested contours => the
        // overlap toggles twice => hole. This is even-odd fill.
        let mut inside = false;
        for contour in &self.contours {
            let pts = &contour.points;
            let n = pts.len();
            if n < 3 {
                continue;
            }
            let mut j = n - 1;
            for i in 0..n {
                let pi = pts[i];
                let pj = pts[j];
                if (pi.y > p.y) != (pj.y > p.y) {
                    let x_cross = pi.x + (p.y - pi.y) / (pj.y - pi.y) * (pj.x - pi.x);
                    if p.x < x_cross {
                        inside = !inside;
                    }
                }
                j = i;
            }
        }
        inside
    }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p hittest`
Expected: PASS (2 passed).

- [ ] **Step 7: Add the concave L-shape and holed-ring tests**

Add to the `tests` module in `crates/hittest/src/lib.rs`:

```rust
    #[test]
    fn l_shape_interior_points_are_inside() {
        let l = l_shape();
        assert!(l.contains(pt(60.0, 60.0)), "lower-left arm");
        assert!(l.contains(pt(130.0, 60.0)), "top arm");
    }

    #[test]
    fn l_shape_notch_is_outside() {
        // The concave notch — the whole point of the spike.
        assert!(!l_shape().contains(pt(130.0, 130.0)));
    }

    #[test]
    fn ring_material_is_inside_but_hole_is_outside() {
        let r = ring();
        assert!(r.contains(pt(50.0, 100.0)), "ring material");
        assert!(!r.contains(pt(100.0, 100.0)), "the hole");
    }
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p hittest`
Expected: PASS (5 passed).

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/hittest
git commit -m "feat(hittest): even-odd concave/holed region containment kernel"
```

---

### Task 2: `spike-render` — `Renderer` trait, `Pixmap`, and the parity gate

**Files:**
- Create: `crates/spike-render/Cargo.toml`
- Create: `crates/spike-render/src/lib.rs`

**Interfaces:**
- Consumes (from Task 1): `hittest::{Region, Point}`.
- Produces:
  - `pub struct Pixmap { pub width: u32, pub height: u32, pub data: Vec<u8> }` — RGBA8, row-major, `data.len() == width*height*4`.
  - `pub trait Renderer { fn name(&self) -> &'static str; fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap; }`
  - `pub struct ParityReport { pub checked: usize, pub mismatches: Vec<(u32, u32)> }`
  - `pub fn parity_check(region: &Region, pm: &Pixmap, fill: [u8; 4], bg: [u8; 4]) -> ParityReport` — for every pixel whose color is *exactly* `fill` or *exactly* `bg` (antialiased edge pixels are neither, so they are skipped), asserts the pixel's fill-ness equals `region.contains(center)`. Mismatches are returned, not panicked.

- [ ] **Step 1: Create the `spike-render` crate manifest**

Create `crates/spike-render/Cargo.toml`:

```toml
[package]
name = "spike-render"
version = "0.0.0"
edition = "2021"

[dependencies]
hittest = { path = "../hittest" }

[dev-dependencies]
```

- [ ] **Step 2: Write the failing test for the parity gate**

Create `crates/spike-render/src/lib.rs`:

```rust
use hittest::{Point, Region};

#[derive(Clone, Debug)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub trait Renderer {
    fn name(&self) -> &'static str;
    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap;
}

#[derive(Debug)]
pub struct ParityReport {
    pub checked: usize,
    pub mismatches: Vec<(u32, u32)>,
}

pub fn parity_check(_region: &Region, _pm: &Pixmap, _fill: [u8; 4], _bg: [u8; 4]) -> ParityReport {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hittest::{Contour, Region};

    // A 4x4 region covering the left half (x < 2). Pixel centers at x=0.5,1.5
    // are inside; x=2.5,3.5 are outside.
    fn left_half_region() -> Region {
        Region {
            contours: vec![Contour {
                points: vec![
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 2.0, y: 0.0 },
                    Point { x: 2.0, y: 4.0 },
                    Point { x: 0.0, y: 4.0 },
                ],
            }],
        }
    }

    fn solid_pixmap() -> Pixmap {
        // 4x4: left two columns red (fill), right two columns black (bg).
        let fill = [255u8, 0, 0, 255];
        let bg = [0u8, 0, 0, 255];
        let mut data = Vec::with_capacity(4 * 4 * 4);
        for _y in 0..4 {
            for x in 0..4 {
                let c = if x < 2 { fill } else { bg };
                data.extend_from_slice(&c);
            }
        }
        Pixmap { width: 4, height: 4, data }
    }

    #[test]
    fn parity_passes_when_render_matches_hittest() {
        let report = parity_check(
            &left_half_region(),
            &solid_pixmap(),
            [255, 0, 0, 255],
            [0, 0, 0, 255],
        );
        assert_eq!(report.mismatches, Vec::<(u32, u32)>::new());
        assert_eq!(report.checked, 16); // no AA, so every pixel is checked
    }

    #[test]
    fn parity_catches_a_wrong_pixel() {
        let mut pm = solid_pixmap();
        // Corrupt pixel (3,0): paint it red though hittest says outside.
        let i = ((0 * 4 + 3) * 4) as usize;
        pm.data[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        let report = parity_check(&left_half_region(), &pm, [255, 0, 0, 255], [0, 0, 0, 255]);
        assert_eq!(report.mismatches, vec![(3, 0)]);
    }

    #[test]
    fn parity_skips_antialiased_pixels() {
        let mut pm = solid_pixmap();
        // Blend pixel (3,0) to a non-fill, non-bg color: must be skipped, not a mismatch.
        let i = ((0 * 4 + 3) * 4) as usize;
        pm.data[i..i + 4].copy_from_slice(&[128, 0, 0, 255]);
        let report = parity_check(&left_half_region(), &pm, [255, 0, 0, 255], [0, 0, 0, 255]);
        assert_eq!(report.mismatches, Vec::<(u32, u32)>::new());
        assert_eq!(report.checked, 15); // the blended pixel was skipped
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p spike-render`
Expected: FAIL — `not implemented` from `parity_check`.

- [ ] **Step 4: Implement `parity_check`**

Replace the `parity_check` function body in `crates/spike-render/src/lib.rs`:

```rust
pub fn parity_check(region: &Region, pm: &Pixmap, fill: [u8; 4], bg: [u8; 4]) -> ParityReport {
    let mut checked = 0usize;
    let mut mismatches = Vec::new();
    for y in 0..pm.height {
        for x in 0..pm.width {
            let i = ((y * pm.width + x) * 4) as usize;
            let px = [pm.data[i], pm.data[i + 1], pm.data[i + 2], pm.data[i + 3]];
            let is_fill = px == fill;
            let is_bg = px == bg;
            if !is_fill && !is_bg {
                // Antialiased / blended edge pixel — ambiguous, skip it.
                continue;
            }
            checked += 1;
            let inside = region.contains(Point {
                x: x as f32 + 0.5,
                y: y as f32 + 0.5,
            });
            if inside != is_fill {
                mismatches.push((x, y));
            }
        }
    }
    ParityReport { checked, mismatches }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p spike-render`
Expected: PASS (3 passed).

- [ ] **Step 6: Commit**

```bash
git add crates/spike-render
git commit -m "feat(spike-render): Renderer trait, Pixmap, and parity gate"
```

---

### Task 3: `tiny-skia` backend

**Files:**
- Modify: `crates/spike-render/Cargo.toml` (add `tiny-skia`)
- Create: `crates/spike-render/src/tiny_skia_backend.rs`
- Modify: `crates/spike-render/src/lib.rs` (add `pub mod tiny_skia_backend;`)
- Create: `crates/spike-render/tests/parity_tiny_skia.rs`

**Interfaces:**
- Consumes: `Renderer`, `Pixmap` (Task 2); `hittest::{Region}` (Task 1).
- Produces: `pub struct TinySkiaRenderer;` implementing `Renderer` (`name()` returns `"tiny-skia"`).

- [ ] **Step 1: Add the dependency**

Run: `cargo add tiny-skia -p spike-render`
Expected: `tiny-skia` added under `[dependencies]` in `crates/spike-render/Cargo.toml`.

- [ ] **Step 2: Write the failing parity test**

Create `crates/spike-render/tests/parity_tiny_skia.rs`:

```rust
use hittest::{l_shape, ring};
use spike_render::tiny_skia_backend::TinySkiaRenderer;
use spike_render::{parity_check, Renderer};

const FILL: [u8; 4] = [255, 0, 0, 255];
const BG: [u8; 4] = [0, 0, 0, 255];

#[test]
fn tiny_skia_matches_hittest_on_l_shape() {
    let mut r = TinySkiaRenderer;
    let pm = r.render(&l_shape(), (200, 200), FILL, BG);
    let report = parity_check(&l_shape(), &pm, FILL, BG);
    assert!(report.checked > 10_000, "too few unambiguous pixels checked: {}", report.checked);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}

#[test]
fn tiny_skia_matches_hittest_on_ring() {
    let mut r = TinySkiaRenderer;
    let pm = r.render(&ring(), (200, 200), FILL, BG);
    let report = parity_check(&ring(), &pm, FILL, BG);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p spike-render --test parity_tiny_skia`
Expected: FAIL to compile — `tiny_skia_backend` / `TinySkiaRenderer` do not exist yet.

- [ ] **Step 4: Implement the backend**

Create `crates/spike-render/src/tiny_skia_backend.rs`:

```rust
use crate::{Pixmap, Renderer};
use hittest::Region;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap as TsPixmap, Transform};

pub struct TinySkiaRenderer;

impl Renderer for TinySkiaRenderer {
    fn name(&self) -> &'static str {
        "tiny-skia"
    }

    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap {
        let (w, h) = size;
        let mut pm = TsPixmap::new(w, h).expect("valid pixmap size");
        pm.fill(Color::from_rgba8(bg[0], bg[1], bg[2], bg[3]));

        let mut pb = PathBuilder::new();
        for contour in &region.contours {
            if let Some((first, rest)) = contour.points.split_first() {
                pb.move_to(first.x, first.y);
                for p in rest {
                    pb.line_to(p.x, p.y);
                }
                pb.close();
            }
        }

        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(fill[0], fill[1], fill[2], fill[3]));
            paint.anti_alias = true;
            pm.fill_path(&path, &paint, FillRule::EvenOdd, Transform::identity(), None);
        }

        // tiny-skia stores premultiplied RGBA8; fill/bg here are opaque so the
        // stored bytes equal the input colors exactly for solid pixels.
        Pixmap {
            width: w,
            height: h,
            data: pm.data().to_vec(),
        }
    }
}
```

Add to the top of `crates/spike-render/src/lib.rs` (after the imports):

```rust
pub mod tiny_skia_backend;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p spike-render --test parity_tiny_skia`
Expected: PASS (2 passed). If a handful of edge mismatches appear, they indicate the AA-skip color match is off — verify fill/bg are opaque and exact; do not loosen the gate.

- [ ] **Step 6: Commit**

```bash
git add crates/spike-render/Cargo.toml crates/spike-render/src/lib.rs crates/spike-render/src/tiny_skia_backend.rs crates/spike-render/tests/parity_tiny_skia.rs
git commit -m "feat(spike-render): tiny-skia backend passing the parity gate"
```

---

### Task 4: `wgpu` backend

> **Spike note:** `wgpu` draws triangles, so a concave/holed polygon must be tessellated first — this task uses `lyon` with its even-odd fill rule. `wgpu`'s API shifts across 0.x releases. The code below targets the current offscreen-readback pattern; **if the resolved version's signatures differ, follow `wgpu`'s `examples/` for offscreen render + buffer readback.** The contract is the parity gate, not the exact API calls. The extra ceremony here (adapter/device request, tessellation, texture→buffer copy, async map) is itself a finding to record in Task 6.

**Files:**
- Modify: `crates/spike-render/Cargo.toml` (add `wgpu`, `lyon`, `pollster`, `bytemuck`)
- Create: `crates/spike-render/src/wgpu_backend.rs`
- Modify: `crates/spike-render/src/lib.rs` (add `pub mod wgpu_backend;`)
- Create: `crates/spike-render/tests/parity_wgpu.rs`

**Interfaces:**
- Consumes: `Renderer`, `Pixmap` (Task 2); `hittest::Region` (Task 1).
- Produces: `pub struct WgpuRenderer;` implementing `Renderer` (`name()` returns `"wgpu"`). Construct with `WgpuRenderer::new()`.

- [ ] **Step 1: Add the dependencies**

Run: `cargo add wgpu lyon pollster bytemuck -p spike-render`
Then enable `bytemuck` derive: run `cargo add bytemuck --features derive -p spike-render`.
Expected: all four crates under `[dependencies]`.

- [ ] **Step 2: Write the failing parity test**

Create `crates/spike-render/tests/parity_wgpu.rs`:

```rust
use hittest::{l_shape, ring};
use spike_render::wgpu_backend::WgpuRenderer;
use spike_render::{parity_check, Renderer};

const FILL: [u8; 4] = [255, 0, 0, 255];
const BG: [u8; 4] = [0, 0, 0, 255];

#[test]
fn wgpu_matches_hittest_on_l_shape() {
    let mut r = WgpuRenderer::new();
    let pm = r.render(&l_shape(), (200, 200), FILL, BG);
    let report = parity_check(&l_shape(), &pm, FILL, BG);
    assert!(report.checked > 10_000, "too few unambiguous pixels: {}", report.checked);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}

#[test]
fn wgpu_matches_hittest_on_ring() {
    let mut r = WgpuRenderer::new();
    let pm = r.render(&ring(), (200, 200), FILL, BG);
    let report = parity_check(&ring(), &pm, FILL, BG);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p spike-render --test parity_wgpu`
Expected: FAIL to compile — `wgpu_backend` / `WgpuRenderer` do not exist yet.

- [ ] **Step 4: Implement the backend**

Create `crates/spike-render/src/wgpu_backend.rs`:

```rust
use crate::{Pixmap, Renderer};
use bytemuck::{Pod, Zeroable};
use hittest::Region;
use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    // Clip-space position, precomputed from canvas coords.
    pos: [f32; 2],
}

pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl WgpuRenderer {
    pub fn new() -> Self {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("no wgpu adapter available");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("failed to create wgpu device");
            Self { device, queue }
        })
    }

    fn tessellate(region: &Region, size: (u32, u32)) -> (Vec<Vertex>, Vec<u32>) {
        // Build a lyon path from the region's contours.
        let mut builder = Path::builder();
        for contour in &region.contours {
            if let Some((first, rest)) = contour.points.split_first() {
                builder.begin(point(first.x, first.y));
                for p in rest {
                    builder.line_to(point(p.x, p.y));
                }
                builder.end(true);
            }
        }
        let path = builder.build();

        let (w, h) = (size.0 as f32, size.1 as f32);
        let mut geometry: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let mut tess = FillTessellator::new();
        tess.tessellate_path(
            &path,
            &FillOptions::default().with_fill_rule(FillRule::EvenOdd),
            &mut BuffersBuilder::new(&mut geometry, |v: FillVertex| {
                let p = v.position();
                // Canvas (0..w, 0..h, y-down) -> clip space (-1..1, y-up).
                Vertex {
                    pos: [p.x / w * 2.0 - 1.0, 1.0 - p.y / h * 2.0],
                }
            }),
        )
        .expect("tessellation failed");

        (geometry.vertices, geometry.indices)
    }
}

impl Renderer for WgpuRenderer {
    fn name(&self) -> &'static str {
        "wgpu"
    }

    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap {
        pollster::block_on(async {
            let (w, h) = size;
            let (vertices, indices) = Self::tessellate(region, size);

            let format = wgpu::TextureFormat::Rgba8Unorm;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("solid"),
                source: wgpu::ShaderSource::Wgsl(
                    r#"
@vertex fn vs(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}
@group(0) @binding(0) var<uniform> color: vec4<f32>;
@fragment fn fs() -> @location(0) vec4<f32> {
    return color;
}
"#
                    .into(),
                ),
            });

            let color = [
                fill[0] as f32 / 255.0,
                fill[1] as f32 / 255.0,
                fill[2] as f32 / 255.0,
                fill[3] as f32 / 255.0,
            ];
            let color_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("color"),
                contents: bytemuck::cast_slice(&color),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: color_buf.as_entire_binding(),
                }],
            });

            let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_layout],
                push_constant_ranges: &[],
            });
            let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

            let vbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ibuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            // Readback buffer must have 256-byte-aligned row stride.
            let unpadded = w * 4;
            let align = 256;
            let padded = ((unpadded + align - 1) / align) * align;
            let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: bg[0] as f64 / 255.0,
                                g: bg[1] as f64 / 255.0,
                                b: bg[2] as f64 / 255.0,
                                a: bg[3] as f64 / 255.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &out_buf,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            self.queue.submit(Some(encoder.finish()));

            let slice = out_buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device.poll(wgpu::Maintain::Wait);
            let mapped = slice.get_mapped_range();

            // Strip row padding into a tight RGBA8 buffer.
            let mut data = Vec::with_capacity((unpadded * h) as usize);
            for row in 0..h {
                let start = (row * padded) as usize;
                data.extend_from_slice(&mapped[start..start + unpadded as usize]);
            }
            drop(mapped);
            out_buf.unmap();

            Pixmap { width: w, height: h, data }
        })
    }
}
```

Add to `crates/spike-render/src/lib.rs`:

```rust
pub mod wgpu_backend;
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p spike-render --test parity_wgpu`
Expected: PASS (2 passed). Note: `MultisampleState::default()` means no MSAA, so polygon edges are hard — almost every pixel is unambiguous and checked. If signatures fail to compile, reconcile against the installed `wgpu` version's offscreen example (see the spike note); the gate assertions stay as written.

- [ ] **Step 6: Commit**

```bash
git add crates/spike-render/Cargo.toml crates/spike-render/src/lib.rs crates/spike-render/src/wgpu_backend.rs crates/spike-render/tests/parity_wgpu.rs
git commit -m "feat(spike-render): wgpu+lyon backend passing the parity gate"
```

---

### Task 5: `vello` backend

> **Spike note:** `vello` is a GPU compute rasterizer built on `wgpu`; it fills vector paths directly (no manual tessellation) but renders into a `wgpu` texture you must still read back. Its API is younger and shifts more than `wgpu`'s. The code below targets the current `Scene` + `Renderer::render_to_texture` pattern; **if signatures differ, follow `vello`'s `examples/` (especially the headless example) for the render-to-texture call.** The parity gate is the contract.

**Files:**
- Modify: `crates/spike-render/Cargo.toml` (add `vello`)
- Create: `crates/spike-render/src/vello_backend.rs`
- Modify: `crates/spike-render/src/lib.rs` (add `pub mod vello_backend;`)
- Create: `crates/spike-render/tests/parity_vello.rs`

**Interfaces:**
- Consumes: `Renderer`, `Pixmap` (Task 2); `hittest::Region` (Task 1); reuses the `wgpu` device/queue/readback approach from Task 4.
- Produces: `pub struct VelloRenderer;` implementing `Renderer` (`name()` returns `"vello"`). Construct with `VelloRenderer::new()`.

- [ ] **Step 1: Add the dependency**

Run: `cargo add vello -p spike-render`
Expected: `vello` under `[dependencies]`. (`wgpu`, `pollster`, `bytemuck` already present from Task 4.)

- [ ] **Step 2: Write the failing parity test**

Create `crates/spike-render/tests/parity_vello.rs`:

```rust
use hittest::{l_shape, ring};
use spike_render::vello_backend::VelloRenderer;
use spike_render::{parity_check, Renderer};

const FILL: [u8; 4] = [255, 0, 0, 255];
const BG: [u8; 4] = [0, 0, 0, 255];

#[test]
fn vello_matches_hittest_on_l_shape() {
    let mut r = VelloRenderer::new();
    let pm = r.render(&l_shape(), (200, 200), FILL, BG);
    let report = parity_check(&l_shape(), &pm, FILL, BG);
    assert!(report.checked > 8_000, "too few unambiguous pixels: {}", report.checked);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}

#[test]
fn vello_matches_hittest_on_ring() {
    let mut r = VelloRenderer::new();
    let pm = r.render(&ring(), (200, 200), FILL, BG);
    let report = parity_check(&ring(), &pm, FILL, BG);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}
```

> Note the lower `checked` floor (8_000): vello antialiases, so more edge pixels are blended and skipped than in the no-MSAA wgpu case. Interior and notch pixels remain unambiguous and are still asserted.

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p spike-render --test parity_vello`
Expected: FAIL to compile — `vello_backend` / `VelloRenderer` do not exist yet.

- [ ] **Step 4: Implement the backend**

Create `crates/spike-render/src/vello_backend.rs`:

```rust
use crate::{Pixmap, Renderer};
use hittest::Region;
use vello::kurbo::{Affine, BezPath, Point as KPoint};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, RenderParams, Scene};

pub struct VelloRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
}

impl VelloRenderer {
    pub fn new() -> Self {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("no wgpu adapter available");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("failed to create wgpu device");
            let renderer = vello::Renderer::new(
                &device,
                vello::RendererOptions::default(),
            )
            .expect("failed to create vello renderer");
            Self { device, queue, renderer }
        })
    }

    fn build_path(region: &Region) -> BezPath {
        let mut path = BezPath::new();
        for contour in &region.contours {
            if let Some((first, rest)) = contour.points.split_first() {
                path.move_to(KPoint::new(first.x as f64, first.y as f64));
                for p in rest {
                    path.line_to(KPoint::new(p.x as f64, p.y as f64));
                }
                path.close_path();
            }
        }
        path
    }
}

impl Renderer for VelloRenderer {
    fn name(&self) -> &'static str {
        "vello"
    }

    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap {
        pollster::block_on(async {
            let (w, h) = size;

            let mut scene = Scene::new();
            scene.fill(
                Fill::EvenOdd,
                Affine::IDENTITY,
                Color::from_rgba8(fill[0], fill[1], fill[2], fill[3]),
                None,
                &Self::build_path(region),
            );

            let format = wgpu::TextureFormat::Rgba8Unorm;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vello-target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    &scene,
                    &view,
                    &RenderParams {
                        base_color: Color::from_rgba8(bg[0], bg[1], bg[2], bg[3]),
                        width: w,
                        height: h,
                        antialiasing_method: AaConfig::Area,
                    },
                )
                .expect("vello render failed");

            // Texture -> padded buffer -> tight RGBA8 (same readback as Task 4).
            let unpadded = w * 4;
            let padded = ((unpadded + 255) / 256) * 256;
            let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &out_buf,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            self.queue.submit(Some(encoder.finish()));

            let slice = out_buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device.poll(wgpu::Maintain::Wait);
            let mapped = slice.get_mapped_range();
            let mut data = Vec::with_capacity((unpadded * h) as usize);
            for row in 0..h {
                let start = (row * padded) as usize;
                data.extend_from_slice(&mapped[start..start + unpadded as usize]);
            }
            drop(mapped);
            out_buf.unmap();

            Pixmap { width: w, height: h, data }
        })
    }
}
```

Add to `crates/spike-render/src/lib.rs`:

```rust
pub mod vello_backend;
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p spike-render --test parity_vello`
Expected: PASS (2 passed). Vello requires storage-texture support on the adapter; on macOS/Metal this is available. If signatures fail to compile, reconcile against the installed `vello` version's headless example (see the spike note).

- [ ] **Step 6: Commit**

```bash
git add crates/spike-render/Cargo.toml crates/spike-render/src/lib.rs crates/spike-render/src/vello_backend.rs crates/spike-render/tests/parity_vello.rs
git commit -m "feat(spike-render): vello backend passing the parity gate"
```

---

### Task 6: Evaluation, decision, and prune

**Files:**
- Create: `crates/spike-render/tests/all_backends.rs`
- Create: `docs/superpowers/specs/2026-06-17-phase0-backend-decision.md`
- Modify: `docs/superpowers/specs/2026-06-17-skinning-engine-design.md` (record the chosen backend under "Open items deferred by design")
- Modify (prune): delete the two losing backends' source + tests + their `Cargo.toml` deps and `lib.rs` `pub mod` lines.

**Interfaces:**
- Consumes: all three backends + `parity_check` from Tasks 3–5.
- Produces: a committed backend decision and a workspace containing only the winning backend.

- [ ] **Step 1: Write a single test that runs all three backends through the gate together**

Create `crates/spike-render/tests/all_backends.rs`:

```rust
use hittest::{l_shape, ring};
use spike_render::tiny_skia_backend::TinySkiaRenderer;
use spike_render::vello_backend::VelloRenderer;
use spike_render::wgpu_backend::WgpuRenderer;
use spike_render::{parity_check, Renderer};

const FILL: [u8; 4] = [255, 0, 0, 255];
const BG: [u8; 4] = [0, 0, 0, 255];

fn assert_clean(r: &mut dyn Renderer) {
    for region in [l_shape(), ring()] {
        let pm = r.render(&region, (200, 200), FILL, BG);
        let report = parity_check(&region, &pm, FILL, BG);
        assert!(
            report.mismatches.is_empty(),
            "{} mismatched on a region: {:?}",
            r.name(),
            report.mismatches
        );
    }
}

#[test]
fn all_backends_pass_the_gate() {
    assert_clean(&mut TinySkiaRenderer);
    assert_clean(&mut WgpuRenderer::new());
    assert_clean(&mut VelloRenderer::new());
}
```

- [ ] **Step 2: Run it and confirm all three pass**

Run: `cargo test -p spike-render --test all_backends`
Expected: PASS. This confirms all three can satisfy the hard constraint — so the decision is made on the secondary criteria below, not correctness.

- [ ] **Step 3: Gather the decision metrics**

Run and record the output of each:

```bash
# Lines of backend code (lower = simpler integration)
wc -l crates/spike-render/src/tiny_skia_backend.rs crates/spike-render/src/wgpu_backend.rs crates/spike-render/src/vello_backend.rs

# Dependency weight pulled in by each backend (count transitive crates)
cargo tree -p spike-render -e normal | wc -l

# Clean build time of the whole spike crate
cargo clean && /usr/bin/time -p cargo build -p spike-render 2>&1 | tail -3
```

Record three things per backend in a scratch note: **integration LOC**, **subjective API friction** (1–5: did it need a tessellator? async readback? storage textures?), and **does it run headless on this machine without a display**.

- [ ] **Step 4: Write the decision document**

Create `docs/superpowers/specs/2026-06-17-phase0-backend-decision.md`. Fill the bracketed values from Step 3's measurements:

```markdown
# Phase 0 — Rendering Backend Decision

**Date:** 2026-06-17
**Outcome:** [tiny-skia | wgpu | vello]

## Method

All three candidates were implemented behind the `Renderer` trait and held to one
objective gate: every unambiguous (non-antialiased) pixel a backend fills must agree
with the `hittest` kernel's independent inside/outside verdict, on both a concave
L-shape and a holed ring. The kernel has zero rendering dependency, proving the
hit-test ↔ render decoupling holds under each backend.

## Result

All three PASSED the correctness gate. Decision made on secondary criteria:

| Backend   | Integration LOC | Deps | API friction | Notes |
|-----------|-----------------|------|--------------|-------|
| tiny-skia | [N]             | [N]  | [1-5]        | CPU rasterizer; fills paths directly; trivial readback |
| wgpu      | [N]             | [N]  | [1-5]        | Needs lyon tessellation + async texture readback |
| vello     | [N]             | [N]  | [1-5]        | GPU path fill, no tessellation; younger, faster-moving API |

## Decision & rationale

[2-4 sentences: which backend, and why — weigh integration simplicity and dependency
weight against the engine's needs. The hard constraint (free-form hit resolution) is
satisfied by all three and lives in the decoupled `hittest` module regardless, so it
is NOT the deciding factor — that was the point of decoupling.]

## What carries forward vs. what is thrown away

- **Carries forward:** the `hittest` kernel and the `Renderer`/`Pixmap`/`parity_check`
  contract — these seed the real `hittest` and `render` modules (design doc Phase 3).
- **Thrown away:** the two non-chosen backend implementations (pruned in this task).
```

- [ ] **Step 5: Record the decision in the main design doc**

In `docs/superpowers/specs/2026-06-17-skinning-engine-design.md`, under "## Open items deferred by design", change the rendering-backend bullet from deferred to resolved:

```markdown
- **Exact rendering backend** — RESOLVED by the Phase 0 spike: **[chosen backend]**.
  See `2026-06-17-phase0-backend-decision.md`.
```

- [ ] **Step 6: Prune the two losing backends**

For each of the two non-chosen backends `<loser>` (`tiny_skia` / `wgpu` / `vello`):

```bash
git rm crates/spike-render/src/<loser>_backend.rs crates/spike-render/tests/parity_<loser>.rs
```

Then edit `crates/spike-render/src/lib.rs` to remove the two losing `pub mod <loser>_backend;` lines, edit `crates/spike-render/tests/all_backends.rs` to drop the pruned backends (leaving only the winner), and run `cargo remove <crate> -p spike-render` for deps now unused (e.g. if wgpu loses: `cargo remove wgpu lyon vello pollster bytemuck -p spike-render`; if tiny-skia loses: `cargo remove tiny-skia -p spike-render`).

- [ ] **Step 7: Verify the pruned workspace still builds and passes**

Run: `cargo test`
Expected: PASS — `hittest` tests, the parity-gate unit tests, and the winner's parity test all green; no reference to pruned modules.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: decide Phase 0 rendering backend and prune losers"
```

---

## Self-Review

**Spec coverage (against the design doc's Phase 0):**
- "One irregular concave hotspot (L-shape or ring)" → Task 1 defines both `l_shape()` and `ring()`. ✓
- "Rendered + hit-tested under each candidate (wgpu, vello, tiny-skia)" → Tasks 3, 4, 5. ✓
- "Clean per-path/per-pixel hit resolution on the concave shape" → the parity gate (Task 2) asserts per-pixel agreement between render and hittest, including the notch. ✓
- "`hittest` module already decoupled from the backend" → Global Constraint forbidding `hittest` render deps; `hittest` has empty `[dependencies]`. ✓
- "Output: committed rendering backend + proven hittest↔render boundary" → Task 6 decision doc + design-doc update. ✓

**Placeholder scan:** The only bracketed `[...]` values are measured metrics in the *decision document the engineer fills at Step 3/4* — these are runtime measurements, not plan placeholders. All code steps contain complete code. ✓

**Type consistency:** `Region`, `Contour`, `Point`, `l_shape()`, `ring()` (Task 1) are used unchanged in Tasks 2–6. `Renderer::render(&mut self, &Region, (u32,u32), [u8;4], [u8;4]) -> Pixmap`, `Pixmap { width, height, data }`, and `parity_check(&Region, &Pixmap, [u8;4], [u8;4]) -> ParityReport` (Task 2) are called with matching signatures in every backend test and in `all_backends.rs`. ✓
```

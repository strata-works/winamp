# Phase 3b — Render-to-Surface + Live Host App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the carapace engine's scene live to a host-owned GPU surface at vsync (no offscreen readback), driven by a real winit host app, fixing the Phase 1 ~31fps/fixed-`dt` dead end — and ship a hand-traced Headspace reference skin.

**Architecture:** A host-owned `render::Renderer` in `carapace` (vello+wgpu) draws a `Scene` directly into a `RenderTarget` (the host's surface view); the engine stays headless. A new `carapace-demo` winit app owns the surface + loop, drives `handle_pointer → update(wall-clock dt) → render`, presents at vsync. Render is tested offscreen under a `lavapipe` software adapter (its own CI job); perf benches run locally.

**Tech Stack:** Rust edition 2024; `vello` 0.9, `wgpu` 29.0.3, `pollster` (carapace render + test); `winit` 0.30.13, `pollster` (demo app); `criterion` (benches); Mesa `lavapipe` (CI render job).

## Global Constraints

- Rust, **edition 2024**, stable. Work on branch `phase3b-render-and-host` (off `main`, which has 3a + CI).
- The engine stays **headless except `render.rs`**: only `render.rs` may use vello/wgpu; `Engine` gains no GPU and `Engine::new` no device. `winit` appears ONLY in `crates/carapace-demo`, never in `carapace`.
- **Zero domain knowledge in `carapace`**: media names (`playing`, `position`, `toggle_play`, `stop`) live only in `carapace-demo` (`DemoHost`) and the demo skin files.
- Render **reads, never writes** host state: `value_fill` resolves via the `read_value: impl Fn(&str)->Option<StateValue>` closure (pure projection, Phase 2 invariant).
- The GPU render test is gated behind a `gpu-tests` cargo feature so the fast `check` CI job (`cargo test -p hittest -p carapace`) does NOT run it; the new `render` CI job runs `cargo test -p carapace --features gpu-tests` under `lavapipe`.
- Versions: match what `main` already resolved — `vello 0.9.0`, `wgpu 29.0.3`, `winit 0.30.13`. Reference (do not import): `crates/spike-render/src/vello_backend.rs` (vello render-to-texture + 256-byte-aligned readback, proven) and `crates/proto/examples/viewer.rs` (winit 0.30 ApplicationHandler structure, proven). The GPU/window APIs are churny; if a signature differs, reconcile against those files + the resolved crate examples. The test/build gate is the contract.
- Present mode **Fifo** (vsync); `dt` from a real `std::time::Instant` delta (Phase 1 fix).

### Spec elaboration (intentional)

- The gated render test uses **sentinel-pixel assertions** (specific known pixels = expected colors) rather than a committed full-PNG golden. Sentinel pixels are deterministic across the `lavapipe` software adapter; a full-PNG golden is fragile to AA/driver differences. This is the robust subset of the spec's "parity/golden" intent. A `--dump` PNG path is provided for human eyeballing (not committed/asserted).

---

### Task 1: `render` module — Renderer + RenderTarget + draw

**Files:**
- Modify: `crates/carapace/Cargo.toml` (add `vello`, `wgpu` deps; `pollster` dev-dep; `[features] gpu-tests = []`)
- Create: `crates/carapace/src/render.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod render;`)
- Create: `crates/carapace/tests/render_offscreen.rs` (gated behind `gpu-tests`)

**Interfaces:**
- Consumes: `scene::{Scene, Node, Pt, Color}`, `state::StateValue`.
- Produces:
  - `pub struct RenderTarget<'a> { pub device: &'a wgpu::Device, pub queue: &'a wgpu::Queue, pub view: &'a wgpu::TextureView, pub width: u32, pub height: u32 }`
  - `pub struct Renderer { /* vello::Renderer */ }` with `pub fn new(device: &wgpu::Device) -> Self`
  - `pub fn draw(&mut self, scene: &Scene, read_value: impl Fn(&str) -> Option<StateValue>, target: &RenderTarget)`

- [ ] **Step 1: Add deps + feature**

Run: `cargo add vello@0.9.0 wgpu@29.0.3 -p carapace` and `cargo add pollster --dev -p carapace`.
Then add to `crates/carapace/Cargo.toml`:

```toml
[features]
gpu-tests = []
```

- [ ] **Step 2: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod render;`

- [ ] **Step 3: Implement `render.rs`**

Create `crates/carapace/src/render.rs`. The vello `Scene`-building is ours (complete below); model `Renderer::new` on `vello::Renderer::new` and keep the `render_to_texture` call shape identical to `crates/spike-render/src/vello_backend.rs`.

```rust
use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{Color as VColor, Fill};
use vello::{AaConfig, RenderParams, Scene as VScene};

use crate::scene::{Color, Node, Pt, Scene};
use crate::state::StateValue;

pub struct RenderTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

pub struct Renderer {
    inner: vello::Renderer,
}

fn vcolor(c: Color) -> VColor {
    VColor::from_rgba8(c.r, c.g, c.b, 255)
}

fn bez(path: &[Pt]) -> BezPath {
    let mut bp = BezPath::new();
    if let Some((first, rest)) = path.split_first() {
        bp.move_to(KPoint::new(first.x as f64, first.y as f64));
        for p in rest {
            bp.line_to(KPoint::new(p.x as f64, p.y as f64));
        }
        bp.close_path();
    }
    bp
}

fn bbox(path: &[Pt]) -> (f64, f64, f64, f64) {
    let xs = path.iter().map(|p| p.x as f64);
    let ys = path.iter().map(|p| p.y as f64);
    (
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    )
}

fn value_of(read: &impl Fn(&str) -> Option<StateValue>, key: &str) -> f64 {
    match read(key) {
        Some(StateValue::Scalar(v)) => v.clamp(0.0, 1.0) as f64,
        Some(StateValue::Bool(true)) => 1.0,
        _ => 0.0,
    }
}

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let inner = vello::Renderer::new(device, vello::RendererOptions::default())
            .expect("create vello renderer");
        Self { inner }
    }

    pub fn draw(
        &mut self,
        scene: &Scene,
        read_value: impl Fn(&str) -> Option<StateValue>,
        target: &RenderTarget,
    ) {
        let (cw, ch) = scene.canvas;
        // Uniform canvas -> surface scale so the skin fills the window.
        let sx = target.width as f64 / cw.max(1) as f64;
        let sy = target.height as f64 / ch.max(1) as f64;
        let xform = Affine::scale_non_uniform(sx, sy);

        let mut vs = VScene::new();
        for node in &scene.nodes {
            match node {
                Node::Fill { path, color } => {
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &bez(path));
                }
                Node::Hotspot { .. } => {} // invisible
                Node::ValueFill { path, value_key, color } => {
                    let v = value_of(&read_value, value_key);
                    let (x0, y0, x1, y1) = bbox(path);
                    let filled = Rect::new(x0, y0, x0 + (x1 - x0) * v, y1);
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &filled);
                }
            }
        }

        self.inner
            .render_to_texture(
                target.device,
                target.queue,
                &vs,
                target.view,
                &RenderParams {
                    base_color: VColor::from_rgba8(0, 0, 0, 255),
                    width: target.width,
                    height: target.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("vello render_to_texture");
    }
}
```

If `vello::Renderer::new` / `render_to_texture` / `RendererOptions` signatures differ from this against the resolved vello 0.9, reconcile against `crates/spike-render/src/vello_backend.rs` (it uses the same calls and is known to compile). The texture target here must be created by the caller with `STORAGE_BINDING | COPY_SRC` usage (the test does this; the demo's surface texture uses `RENDER_ATTACHMENT` — note vello may require an intermediate storage texture; if `render_to_texture` rejects the surface view, render to an owned storage texture then blit/copy to the surface — see vello's `render_to_surface` helper if present in 0.9 and prefer it for the demo).

> **Surface vs texture note for the implementer:** vello 0.9 renders into a **storage** texture. The offscreen test (Task 1) creates that storage texture directly. The demo app (Task 6) cannot render vello directly into a wgpu *surface* texture (surfaces aren't storage-bindable); use vello's `render_to_surface` if available in 0.9, else render to an owned storage texture and `copy_texture_to_texture` into the surface frame. Resolve this in Task 6 against the resolved vello API; Task 1's offscreen path is unaffected.

- [ ] **Step 4: Write the gated offscreen render test**

Create `crates/carapace/tests/render_offscreen.rs`:

```rust
#![cfg(feature = "gpu-tests")]

use carapace::render::{RenderTarget, Renderer};
use carapace::scene::{Color, Node, Pt, Scene};
use carapace::state::StateValue;

// Build a device + an offscreen Rgba8Unorm storage texture, render, read back.
struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

fn offscreen(w: u32, h: u32) -> Offscreen {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("no wgpu adapter (need a GPU or lavapipe)");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("device");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Offscreen { device, queue, texture, view, w, h }
    })
}

// Read the texture back into tight RGBA8 (256-byte-aligned readback — see vello_backend.rs).
fn readback(o: &Offscreen) -> Vec<u8> {
    pollster::block_on(async {
        let unpadded = o.w * 4;
        let padded = ((unpadded + 255) / 256) * 256;
        let buf = o.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rb"),
            size: (padded * o.h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = o.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &o.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(o.h),
                },
            },
            wgpu::Extent3d { width: o.w, height: o.h, depth_or_array_layers: 1 },
        );
        o.queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        o.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let mapped = slice.get_mapped_range();
        let mut data = Vec::with_capacity((unpadded * o.h) as usize);
        for row in 0..o.h {
            let start = (row * padded) as usize;
            data.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buf.unmap();
        data
    })
}

fn px(data: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2]]
}

#[test]
fn renders_fill_and_value_fill_at_sentinel_pixels() {
    let o = offscreen(200, 200);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![
            // red square covering the top-left quadrant
            Node::Fill {
                path: vec![
                    Pt { x: 20.0, y: 20.0 },
                    Pt { x: 100.0, y: 20.0 },
                    Pt { x: 100.0, y: 100.0 },
                    Pt { x: 20.0, y: 100.0 },
                ],
                color: Color { r: 255, g: 0, b: 0 },
            },
            // a value_fill bar across the bottom, value=0.5 -> fills left half of its bbox
            Node::ValueFill {
                path: vec![
                    Pt { x: 0.0, y: 150.0 },
                    Pt { x: 200.0, y: 150.0 },
                    Pt { x: 200.0, y: 170.0 },
                    Pt { x: 0.0, y: 170.0 },
                ],
                value_key: "v".to_string(),
                color: Color { r: 0, g: 255, b: 0 },
            },
        ],
    };
    let read = |k: &str| if k == "v" { Some(StateValue::Scalar(0.5)) } else { None };
    r.draw(&scene, read, &RenderTarget {
        device: &o.device,
        queue: &o.queue,
        view: &o.view,
        width: o.w,
        height: o.h,
    });
    let data = readback(&o);
    // sentinels (canvas==surface here, so coords map 1:1):
    assert_eq!(px(&data, 200, 60, 60), [255, 0, 0], "inside the red fill");
    assert_eq!(px(&data, 200, 150, 60), [0, 0, 0], "outside any fill = base black");
    assert_eq!(px(&data, 200, 50, 160), [0, 255, 0], "value_fill filled half (x=50 < 100)");
    assert_eq!(px(&data, 200, 150, 160), [0, 0, 0], "value_fill empty half (x=150 > 100)");
}
```

- [ ] **Step 5: Run the gated test (local GPU)**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS on a machine with a GPU (your Mac/Metal). On a GPU-less box it will fail at `request_adapter` — that's why it's gated and only the `lavapipe` CI job runs it. Also confirm the fast path is unaffected: `cargo test -p hittest -p carapace` (no feature) must still pass and NOT compile this test.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/Cargo.toml crates/carapace/src/lib.rs crates/carapace/src/render.rs crates/carapace/tests/render_offscreen.rs
git commit -m "feat(carapace): render module — vello draw to a host surface (gated offscreen test)"
```

---

### Task 2: Criterion perf benches (local)

**Files:**
- Modify: `crates/carapace/Cargo.toml` (add `criterion` dev-dep + `[[bench]]`)
- Create: `crates/carapace/benches/engine.rs`

**Interfaces:**
- Consumes: `Scene::hit`, `Engine` (drain/update), `skin`/`script` (rebuild via `Engine::new`), `render::Renderer` (frame-time), `fixture::FixtureHost`.

- [ ] **Step 1: Add criterion + bench target**

Run: `cargo add criterion --dev -p carapace`.
Add to `crates/carapace/Cargo.toml`:

```toml
[[bench]]
name = "engine"
harness = false
```

- [ ] **Step 2: Write the benches**

Create `crates/carapace/benches/engine.rs`:

```rust
use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::scene::Pt;
use carapace::vocab::VocabRegistry;
use criterion::{criterion_group, criterion_main, Criterion};

const SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
    value_fill{ path={{x=0,y=120},{x=200,y=120},{x=200,y=140},{x=0,y=140}},
                value='level', color={r=1,g=2,b=3} }
"#;

fn src(s: &str) -> SkinSource {
    SkinSource { lua_src: s.to_string(), canvas: (200, 200) }
}

fn engine() -> Engine {
    Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), src(SKIN)).unwrap()
}

fn benches(c: &mut Criterion) {
    c.bench_function("scene_hit", |b| {
        let e = engine();
        b.iter(|| e.scene().hit(std::hint::black_box(Pt { x: 50.0, y: 50.0 })));
    });
    c.bench_function("drain_toggle", |b| {
        let mut e = engine();
        b.iter(|| {
            e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
            e.update(Duration::ZERO);
        });
    });
    c.bench_function("scene_rebuild", |b| {
        b.iter(|| std::hint::black_box(engine()));
    });
}

criterion_group!(g, benches);
criterion_main!(g);
```

> The **render frame-time** bench needs a GPU device and the offscreen scaffolding from Task 1's test. Add it as a separate gated bench in a follow-up step once Task 1's offscreen helpers can be shared — for this task, ship the three headless benches (hit/drain/rebuild). Recorded for completion in Task 2 Step 4.

- [ ] **Step 3: Run the benches locally**

Run: `cargo bench -p carapace`
Expected: Criterion runs the three benches and prints timings (sub-microsecond to low-microsecond for hit/drain; rebuild dominated by Lua). No assertion — this is a baseline; Criterion stores baselines under `target/criterion`.

- [ ] **Step 4: Add the render frame-time bench**

Add to `crates/carapace/benches/engine.rs` a fourth bench, gated so it only builds with a GPU available. Since benches can't easily use a cargo feature per-bench, guard it at runtime: attempt `wgpu::Instance::default().request_adapter(...)`; if `None`, skip the bench body with a printed notice. Append inside `benches`:

```rust
    c.bench_function("render_frame", |b| {
        // Build a GPU device once; skip if no adapter (e.g. CI without GPU).
        let setup = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await?;
            let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.ok()?;
            Some((device, queue))
        });
        let Some((device, queue)) = setup else {
            eprintln!("render_frame: no GPU adapter, skipping");
            b.iter(|| ());
            return;
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 342, height: 394, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut r = carapace::render::Renderer::new(&device);
        let e = engine();
        b.iter(|| {
            r.draw(e.scene(), |k| e.state(k), &carapace::render::RenderTarget {
                device: &device, queue: &queue, view: &view, width: 342, height: 394,
            });
        });
    });
```

Add `pollster` is already a dev-dep (Task 1). `wgpu`/`carapace::render` are available. Run `cargo bench -p carapace` again; on your Mac the `render_frame` bench runs (this is the path behind the ~31fps finding — record the per-frame time).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/Cargo.toml crates/carapace/benches/engine.rs
git commit -m "perf(carapace): criterion benches — hit/drain/rebuild + render frame-time"
```

---

### Task 3: CI `render` job (lavapipe)

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:** none (infra).

- [ ] **Step 1: Add a second job to the workflow**

Add this job to `.github/workflows/ci.yml` under `jobs:` (alongside the existing `check` job — do not modify `check`):

```yaml
  render:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        run: rustup toolchain install stable --profile minimal
      - name: Install Mesa lavapipe (software Vulkan)
        run: |
          sudo apt-get update
          sudo apt-get install -y mesa-vulkan-drivers libvulkan1 vulkan-tools
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-render-${{ hashFiles('**/Cargo.toml') }}
          restore-keys: ${{ runner.os }}-cargo-render-
      - name: Render tests (software adapter)
        env:
          # Force the lavapipe software Vulkan ICD.
          VK_ICD_FILENAMES: /usr/share/vulkan/icd.d/lvp_icd.x86_64.json
          WGPU_BACKEND: vulkan
        run: cargo test -p carapace --features gpu-tests --test render_offscreen
```

> The `lvp_icd` path is the standard Mesa lavapipe ICD location on Ubuntu; if the job can't find an adapter, `vulkan-tools`' `vulkaninfo` (run as a debug step) confirms the ICD path. This job may be slower/occasionally flaky (software rendering) — keep it a separate job so the `check` gate stays fast. It can start as non-required and be promoted once stable.

- [ ] **Step 2: Validate the yaml + the command shape locally**

You cannot run GH Actions locally, but confirm: (a) the yaml is valid (no tabs, correct nesting); (b) the exact test command works on your GPU: `cargo test -p carapace --features gpu-tests --test render_offscreen` passed in Task 1. CI green is confirmed only after push; note that.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add lavapipe software-render job for gated offscreen render tests"
```

---

### Task 4: `carapace-demo` crate + `DemoHost`

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `crates/carapace-demo/Cargo.toml`, `crates/carapace-demo/src/main.rs` (stub for now), `crates/carapace-demo/src/demo_host.rs`

**Interfaces:**
- Consumes: `carapace::host::{Host, ActionSpec, Value}`, `carapace::state::StateValue`.
- Produces: `pub struct DemoHost` impl `carapace::host::Host` (state `playing`/`position`, actions `toggle_play`/`stop`).

- [ ] **Step 1: Add the crate to the workspace**

Edit root `Cargo.toml` `members`: add `"crates/carapace-demo"`.

- [ ] **Step 2: Manifest + stub main**

Create `crates/carapace-demo/Cargo.toml`:

```toml
[package]
name = "carapace-demo"
version = "0.0.0"
edition = "2024"

[dependencies]
carapace = { path = "../carapace" }
winit = "0.30.13"
wgpu = "29.0.3"
pollster = "0.4.0"

[[bin]]
name = "carapace-demo"
path = "src/main.rs"
```

Create `crates/carapace-demo/src/main.rs` (stub; Task 6 fills it):

```rust
mod demo_host;

fn main() {
    // Replaced in Task 6 with the winit/wgpu host loop.
    println!("carapace-demo: run the windowed app (implemented in Task 6)");
}
```

- [ ] **Step 3: Write the failing test + `demo_host.rs`**

Create `crates/carapace-demo/src/demo_host.rs`:

```rust
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;

pub struct DemoHost {
    playing: bool,
    position: f32,
}

impl DemoHost {
    pub fn new() -> Self {
        Self { playing: false, position: 0.0 }
    }
}

impl Default for DemoHost {
    fn default() -> Self {
        Self::new()
    }
}

const ACTIONS: &[ActionSpec] =
    &[ActionSpec { name: "toggle_play" }, ActionSpec { name: "stop" }];

impl Host for DemoHost {
    fn name(&self) -> &str {
        "demo-media"
    }
    fn tick(&mut self, dt: Duration) {
        if self.playing {
            self.position = (self.position + dt.as_secs_f32() * 0.1).min(1.0);
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => Some(StateValue::Scalar(self.position)),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        match action {
            "toggle_play" => self.playing = !self.playing,
            "stop" => {
                self.playing = false;
                self.position = 0.0;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_and_advance_and_stop() {
        let mut h = DemoHost::new();
        h.invoke("toggle_play", &[]);
        h.tick(Duration::from_secs(1));
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.1)));
        assert_eq!(h.get("playing"), Some(StateValue::Bool(true)));
        h.invoke("stop", &[]);
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.0)));
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
    }
}
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p carapace-demo`
Expected: PASS (1 test). (Builds winit/wgpu but the stub `main` runs nothing GPU.)

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace-demo
git add Cargo.toml crates/carapace-demo
git commit -m "feat(carapace-demo): crate scaffold + DemoHost (media-style host)"
```

---

### Task 5: The three skins (classic, minimal, Headspace reference) + headless skin test

**Files:**
- Create: `crates/carapace-demo/skins/classic/{skin.toml,skin.lua}`
- Create: `crates/carapace-demo/skins/minimal/{skin.toml,skin.lua}`
- Create: `crates/carapace-demo/skins/reference/{skin.toml,skin.lua,headspace-source.png}`
- Create: `crates/carapace-demo/tests/skins_build.rs`

**Interfaces:**
- Consumes: `carapace::skin::load_dir`, `carapace::engine::Engine`, `carapace::vocab::VocabRegistry`, `DemoHost` (via `carapace_demo`? — `DemoHost` is in the binary crate; for the test, the test is part of `carapace-demo` so it can use `crate::demo_host` only if exposed). To let the integration test use `DemoHost`, expose it: in `main.rs` change `mod demo_host;` to `pub mod demo_host;` is not enough for a `[[bin]]`. **Resolution:** add a tiny `src/lib.rs` to `carapace-demo` exporting `pub mod demo_host;`, and have `main.rs` use `carapace_demo::demo_host::DemoHost`. The integration test then uses `carapace_demo::demo_host::DemoHost`.

- [ ] **Step 1: Make `carapace-demo` a lib+bin so tests can use `DemoHost`**

Create `crates/carapace-demo/src/lib.rs`:

```rust
pub mod demo_host;
```

Edit `crates/carapace-demo/src/main.rs` top: replace `mod demo_host;` with `use carapace_demo::demo_host as _;` is unnecessary — simply delete the `mod demo_host;` line (the bin will reference `carapace_demo::demo_host::DemoHost` in Task 6). For the stub main, no demo_host use is needed yet. Ensure `Cargo.toml` has both `[lib]` (implicit via `src/lib.rs`) and `[[bin]]` (already present).

- [ ] **Step 2: Create the simple skins**

`crates/carapace-demo/skins/classic/skin.toml`:

```toml
schema = 1
id = "classic"
name = "Classic"
engine = "^0.1"
canvas = { width = 300, height = 140 }
entry = "skin.lua"
```

`crates/carapace-demo/skins/classic/skin.lua`:

```lua
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=140},{x=0,y=140}}, color = {r=24,g=28,b=40} }
region{ path = {{x=20,y=20},{x=90,y=20},{x=90,y=90},{x=20,y=90}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=20,y=20},{x=90,y=20},{x=90,y=90},{x=20,y=90}}, color = {r=80,g=200,b=120} }
region{ path = {{x=110,y=20},{x=180,y=20},{x=180,y=90},{x=110,y=90}},
        on_press = function() host.stop() end }
fill{ path = {{x=110,y=20},{x=180,y=20},{x=180,y=90},{x=110,y=90}}, color = {r=200,g=80,b=80} }
value_fill{ path = {{x=20,y=110},{x=280,y=110},{x=280,y=126},{x=20,y=126}},
            value = "position", color = {r=240,g=220,b=80} }
```

`crates/carapace-demo/skins/minimal/skin.toml`:

```toml
schema = 1
id = "minimal"
name = "Minimal"
engine = "^0.1"
canvas = { width = 300, height = 140 }
entry = "skin.lua"
```

`crates/carapace-demo/skins/minimal/skin.lua`:

```lua
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=140},{x=0,y=140}}, color = {r=12,g=12,b=12} }
region{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=80},{x=30,y=80}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=80},{x=30,y=80}}, color = {r=120,g=120,b=120} }
value_fill{ path = {{x=30,y=100},{x=270,y=100},{x=270,y=108},{x=30,y=108}},
            value = "position", color = {r=0,g=220,b=220} }
```

- [ ] **Step 3: Create the Headspace reference skin**

Copy the source PNG (already downloaded during design) into the skin dir:

```bash
mkdir -p crates/carapace-demo/skins/reference
cp .git/sdd/refskin/headspace.png crates/carapace-demo/skins/reference/headspace-source.png
```

`crates/carapace-demo/skins/reference/skin.toml`:

```toml
schema = 1
id = "headspace"
name = "Headspace (reference homage)"
engine = "^0.1"
canvas = { width = 342, height = 394 }
entry = "skin.lua"
```

`crates/carapace-demo/skins/reference/skin.lua` — a flat-color vector homage of Headspace
(342×394), hand-traced from the source image. Skins have no `math`, so speakers are
hand-plotted octagons. Deferred to Phase 5 (asset/text/gradients): the photographic face is
a flat placeholder fill, the screen is solid, no text/visualizer.

```lua
-- body silhouette (green organic head + wings), traced as a filled blob
fill{ path = {
  {x=70,y=10},{x=272,y=10},{x=300,y=40},{x=332,y=70},{x=332,y=230},{x=300,y=250},
  {x=300,y=300},{x=240,y=370},{x=171,y=392},{x=102,y=370},{x=42,y=300},{x=42,y=250},
  {x=10,y=230},{x=10,y=70},{x=42,y=40}
}, color = {r=86,g=196,b=40} }

-- bottom "face" region (flat placeholder; real photo is Phase 5)
fill{ path = {{x=70,y=250},{x=272,y=250},{x=240,y=370},{x=171,y=392},{x=102,y=370}},
      color = {r=58,g=132,b=36} }

-- black display screen
fill{ path = {{x=72,y=56},{x=270,y=56},{x=270,y=206},{x=72,y=206}}, color = {r=8,g=8,b=10} }

-- 6 speaker grilles as octagons (left wing x~37, right wing x~305; y 100/152/204)
fill{ path = {{x=22,y=92},{x=52,y=92},{x=66,y=106},{x=66,y=130},{x=52,y=144},{x=22,y=144},{x=8,y=130},{x=8,y=106}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=22,y=144},{x=52,y=144},{x=66,y=158},{x=66,y=182},{x=52,y=196},{x=22,y=196},{x=8,y=182},{x=8,y=158}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=22,y=196},{x=52,y=196},{x=66,y=210},{x=66,y=234},{x=52,y=248},{x=22,y=248},{x=8,y=234},{x=8,y=210}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=290,y=92},{x=320,y=92},{x=334,y=106},{x=334,y=130},{x=320,y=144},{x=290,y=144},{x=276,y=130},{x=276,y=106}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=290,y=144},{x=320,y=144},{x=334,y=158},{x=334,y=182},{x=320,y=196},{x=290,y=196},{x=276,y=182},{x=276,y=158}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=290,y=196},{x=320,y=196},{x=334,y=210},{x=334,y=234},{x=320,y=248},{x=290,y=248},{x=276,y=234},{x=276,y=210}}, color = {r=150,g=170,b=150} }

-- transport row (play -> toggle_play, stop -> stop), drawn + hotspot each
region{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}}, on_press = function() host.toggle_play() end }
fill{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}}, color = {r=200,g=235,b=200} }
region{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}}, on_press = function() host.stop() end }
fill{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}}, color = {r=200,g=235,b=200} }

-- sunburst options button (top-right) as a diamond
fill{ path = {{x=300,y=24},{x=320,y=44},{x=300,y=64},{x=280,y=44}}, color = {r=235,g=240,b=120} }

-- side arrows
fill{ path = {{x=8,y=160},{x=24,y=150},{x=24,y=170}}, color = {r=120,g=210,b=70} }
fill{ path = {{x=334,y=160},{x=318,y=150},{x=318,y=170}}, color = {r=120,g=210,b=70} }

-- seek bar bound to position
value_fill{ path = {{x=78,y=216},{x=264,y=216},{x=264,y=230},{x=78,y=230}},
            value = "position", color = {r=120,g=230,b=80} }

-- center logo button
fill{ path = {{x=156,y=236},{x=186,y=236},{x=186,y=256},{x=156,y=256}}, color = {r=40,g=120,b=30} }
```

> Phase-5 note (record in the skin as a comment too): a "safe shapes" sandbox helper (e.g. a `circle{cx,cy,r}` primitive or a math-free generator) would remove the hand-plotted octagons; deferred with the rest of the vocabulary work.

- [ ] **Step 4: Write the headless skin-build test**

Create `crates/carapace-demo/tests/skins_build.rs`:

```rust
use std::path::Path;

use carapace::engine::Engine;
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;

fn build(skin_dir: &str) -> usize {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins").join(skin_dir);
    let (_m, source) = carapace::skin::load_dir(&dir).expect("load skin dir");
    let e = Engine::new(Box::new(DemoHost::new()), VocabRegistry::base(), source)
        .expect("skin builds into a scene");
    e.scene().nodes.len()
}

#[test]
fn classic_builds() {
    assert!(build("classic") >= 4);
}

#[test]
fn minimal_builds() {
    assert!(build("minimal") >= 3);
}

#[test]
fn headspace_reference_builds() {
    // The reference skin is intentionally busy — a render/perf stress scene.
    let n = build("reference");
    assert!(n >= 15, "headspace homage should be a busy scene, got {n} nodes");
}
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p carapace-demo --test skins_build`
Expected: PASS (3 tests). This proves all three skins **parse, pass schema/engine compat, and build into scenes** against the real engine — headless, no GPU. If `headspace_reference_builds` fails on node count, the Lua has a parse/build error (e.g. a bad action name — only `toggle_play`/`stop` are valid for `DemoHost`); fix the skin, not the threshold.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p carapace-demo
git add crates/carapace-demo/src/lib.rs crates/carapace-demo/src/main.rs crates/carapace-demo/skins crates/carapace-demo/tests/skins_build.rs
git commit -m "feat(carapace-demo): classic/minimal skins + Headspace reference homage + headless build test"
```

---

### Task 6: The live winit + wgpu host app

> The window/surface boilerplate is churny. Reuse the winit 0.30 `ApplicationHandler` structure from `crates/proto/examples/viewer.rs` (proven). The NEW part vs the proto viewer: a **wgpu surface** instead of softbuffer, and vello rendering into it. Resolve the vello-into-surface detail per Task 1's "Surface vs texture note" (prefer `render_to_surface` if vello 0.9 exposes it; else render to an owned storage texture and `copy_texture_to_texture` into the surface frame). GUI launch is human-verified.

**Files:**
- Create/replace: `crates/carapace-demo/src/main.rs`

**Interfaces:**
- Consumes: `carapace::engine::{Engine, PointerEvent}`, `carapace::command::{Command, SkinSource}`, `carapace::scene::Pt`, `carapace::render::{Renderer, RenderTarget}`, `carapace::vocab::VocabRegistry`, `carapace::skin::load_dir`, `carapace_demo::demo_host::DemoHost`.

- [ ] **Step 1: Implement the host loop**

Replace `crates/carapace-demo/src/main.rs` with the winit + wgpu app. Structure (mirror `proto/examples/viewer.rs` for the winit `ApplicationHandler`/window/DPI/cursor handling; swap softbuffer for wgpu surface):

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Pt;
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const SKINS: [&str; 3] = ["skins/classic", "skins/minimal", "skins/reference"];

fn skin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn load_source(i: usize) -> (SkinSource, (u32, u32)) {
    let (_m, src) = carapace::skin::load_dir(&skin_root().join(SKINS[i])).expect("load skin");
    let canvas = src.canvas;
    (src, canvas)
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

struct App {
    skin_index: usize,
    engine: Engine,
    cursor: (f64, f64),
    last: Instant,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    renderer: Option<Renderer>,
}

impl App {
    fn new() -> Self {
        let (src, _canvas) = load_source(0);
        let engine = Engine::new(Box::new(DemoHost::new()), VocabRegistry::base(), src).unwrap();
        Self { skin_index: 0, engine, cursor: (0.0, 0.0), last: Instant::now(),
               window: None, gpu: None, renderer: None }
    }
}
// ... resumed(): create window (sized from initial skin canvas * scale); pollster::block_on
//     to make wgpu instance, surface (Arc<Window>), adapter, device, queue; configure surface
//     with PresentMode::Fifo; Renderer::new(&device).
// ... window_event(): RedrawRequested -> { let dt = now - last; last = now;
//       engine.update(dt); acquire surface frame; renderer.draw(engine.scene(),
//       |k| engine.state(k), &RenderTarget{...frame view...}); present; window.request_redraw() }
//     MouseInput Left Pressed -> map physical cursor -> canvas coords (canvas/physical), engine.handle_pointer(.., Press)
//     KeyboardInput Tab -> skin_index = (skin_index+1)%3; let (src,_)=load_source(skin_index); engine.handle_command(Command::Swap(src))
//     KeyboardInput Escape / CloseRequested -> exit
//     Resized -> reconfigure surface
fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
```

Fill in the elided `ApplicationHandler` impl using the proto viewer as the template for winit specifics (window creation guard, physical-size handling, cursor capture, the `Key::Character`/`NamedKey` matching). For the wgpu surface: `wgpu::Instance::default()`, `instance.create_surface(window.clone())`, `request_adapter` with the surface as compatible, `request_device`, `surface.get_capabilities`, configure with `Fifo`. Each frame: `surface.get_current_texture()` → its `.texture.create_view(...)` is the `RenderTarget.view`; after `renderer.draw`, call `frame.present()`. Handle `SurfaceError::Lost/Outdated` by reconfiguring and skipping the frame (no panic). Apply the vello-into-surface resolution from Task 1's note.

- [ ] **Step 2: Build (the gate)**

Run: `cargo build -p carapace-demo`
Expected: compiles cleanly. If winit/wgpu/vello signatures differ, reconcile against `proto/examples/viewer.rs`, `spike-render/src/vello_backend.rs`, and the resolved crate versions.

- [ ] **Step 3: Smoke-launch (human-verified)**

If a display is available: `cargo run -p carapace-demo`. Confirm a window opens showing the **classic** skin; clicking the green square toggles play (the seek bar starts advancing **smoothly at vsync**); the red square stops/resets; **Tab** cycles classic → minimal → **Headspace** with the seek position **preserved** across swaps; the Headspace homage renders (green head, black screen, 6 speakers, seek bar); Esc/close quits. If headless, note interactive launch was not verified and rely on the clean build; the human will run it. Compare the felt smoothness to `proto`'s ~31fps.

- [ ] **Step 4: Commit**

```bash
cargo fmt -p carapace-demo
git add crates/carapace-demo/src/main.rs
git commit -m "feat(carapace-demo): live winit + wgpu host app rendering at vsync"
```

---

## Self-Review

**Spec coverage (against the 3b design):**
- §1 render in engine, headless boundary, per-frame flow → Task 1 (render.rs) + Task 6 (loop). ✓
- §2 render API (`Renderer`/`RenderTarget`/`draw`, read_value closure, canvas→surface scale) → Task 1. ✓
- §3 offscreen render test → Task 1 (gated, sentinel pixels); `lavapipe` CI job → Task 3; perf benches → Task 2. ✓
- §4 `DemoHost` + interaction → Tasks 4, 6; three skins → Task 5. ✓
- §5 Headspace reference homage (flat vector, ~15-20 nodes, deferred fidelity noted) → Task 5. ✓
- §6 decomposition (render half: T1-3; app half: T4-6) + error handling (surface-loss reconfigure, transactional swap) → Tasks 1, 6. ✓
- Zero domain knowledge in carapace (DemoHost + skins only) → Task 4/5 constraint. ✓
- Headless boundary + fast `check` job unaffected (gpu test gated) → Task 1 Step 5 verifies. ✓

**Placeholder scan:** Task 6's `main.rs` shows the full struct/skeleton with the `ApplicationHandler` impl described as prose-with-exact-calls + a reference to the proven `proto/examples/viewer.rs`, rather than re-printing ~150 lines of churny winit/wgpu boilerplate verbatim. This is a reuse-of-verified-code directive for version-fragile GUI code (the same approach used for every prior GPU/window task), with the gate being a clean `cargo build` + human run — not an unwritten-logic placeholder. All our logic (render draw, DemoHost, skins, benches, the offscreen test, skin-build test) is complete code. Task 2 Step 4's render bench has a runtime GPU-skip guard (not a placeholder). No TBDs.

**Type consistency:** `RenderTarget`/`Renderer`/`draw(scene, read_value, target)` (Task 1) used identically in Task 2 (bench) + Task 6 (app). `DemoHost` (Task 4) consumed by Task 5 test + Task 6. `Engine::{new,handle_pointer,update,handle_command,scene,state}`, `Command::Swap`, `SkinSource`, `Pt`, `VocabRegistry::base`, `skin::load_dir`, `StateValue`, `Scene::hit` all match the carapace surfaces on `main`. Skin action names (`toggle_play`/`stop`) match `DemoHost::actions`. value keys (`position`) match `DemoHost::get`.

## Deferred (recorded, not built here)

- Phase 5: asset/bitmap loading, text, gradients, visualizer, richer vocab → upgrades the Headspace reference toward fidelity; a "safe shapes" sandbox helper to retire hand-plotted octagons.
- A future **skin-generation** phase (ground-truth = the Headspace reference skin).
- Remove throwaway `proto`/`spike-render`; widen CI clippy/test to `--workspace` → end of Phase 3.
- Promote the `lavapipe` `render` job from best-effort to required once its stability is known.

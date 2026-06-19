# Total Window Replacement — Feasibility Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A throwaway scratch binary that proves a carapace skin can render as a borderless, transparent, draggable window with working drawn min/close controls on macOS, and characterizes click-through.

**Architecture:** A self-contained `crates/window-spike` binary. It reuses `carapace` to load the real Headspace (`reference`) skin and decode its bitmap, then renders that bitmap through a *local mirror* of the carapace pipeline (vello → intermediate `Rgba8Unorm` → `TextureBlitter` → premultiplied surface) with a **transparent base color**, in a borderless/transparent winit window. Interaction (drag, min/close, click-through probe) is wired directly to winit. The engine and `carapace-demo` are untouched.

**Tech Stack:** Rust (edition 2024), winit 0.30.13, wgpu 29.0.3, vello 0.9.0 / peniko 0.6.1 (via `carapace`), pollster — all already in the workspace lock (no new dependencies).

## Global Constraints

- **Throwaway spike.** Output is *learnings*, not merged feature code. Spike-grade quality is fine: `expect`/`unwrap` on setup is acceptable; do **not** gold-plate. No unit/integration tests — this is a GUI spike verified by build + launch + human-visual observation.
- **macOS/Metal only.** Verify on the dev machine; do not chase cross-platform.
- **No changes to `carapace-demo` or the `carapace`/`hittest` engine crates.** The spike is purely additive (`crates/window-spike` + workspace-member line + a findings doc).
- **No new third-party dependencies.** winit/wgpu/vello/peniko/pollster are already in the workspace; match the existing versions, do not run `cargo update` or `cargo add`.
- **Success gate = Tier 1 + Tier 2.** Tier 1: borderless + transparent window, desktop visible through the skin's transparent margins (head floats). Tier 2: drag the window by the skin body + the drawn min/close glyphs minimize / quit. **Tier 3 (click-through) is probed and documented, not required to work.**
- **Deliverables:** the working scratch crate, a committed screenshot, and a findings doc (`docs/superpowers/specs/2026-06-19-window-replacement-spike-findings.md`) recording the exact winit/wgpu knobs that worked (incl. which surface `alpha_mode` the adapter offered), the `render.rs` transparent-base interface gap, and the click-through verdict.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>`. No "Generated with Claude" attribution.

## File Structure

- `crates/window-spike/Cargo.toml` — new throwaway binary crate; deps on `carapace` (path) + winit/wgpu/vello/peniko/pollster (workspace versions).
- `crates/window-spike/src/main.rs` — the entire spike: window setup, surface, local transparent render of the Headspace bitmap, interaction.
- `Cargo.toml` (root) — add `crates/window-spike` to `members`.
- `docs/superpowers/specs/2026-06-19-window-replacement-spike-findings.md` — the findings deliverable (Task 2).
- `crates/window-spike/screenshot.png` — committed human-visual evidence (Task 2).

---

### Task 1: Chrome-less floating skin (Tier 1)

Scaffold the crate and get the real Headspace bitmap rendering in a borderless, transparent window — desktop visible through the transparent margins.

**Files:**
- Create: `crates/window-spike/Cargo.toml`
- Create: `crates/window-spike/src/main.rs`
- Modify: `Cargo.toml` (root) — add the workspace member

**Interfaces:**
- Consumes: `carapace::skin::load_dir(&Path) -> Result<(Manifest, SkinSource), _>`, `carapace::engine::Engine::new(Box<dyn Host>, VocabRegistry, SkinSource)`, `carapace::engine::Engine::scene() -> &Scene`, `carapace::scene::Node::Image { image: Arc<DecodedImage>, dest }`, `carapace::asset::DecodedImage { rgba: Vec<u8>, width: u32, height: u32 }`, `carapace_demo::demo_host::DemoHost::new()` (reuse the existing demo host to build the engine).
- Produces: a `cargo run -p window-spike` binary that opens the floating skin window.

- [ ] **Step 1: Create the crate manifest**

`crates/window-spike/Cargo.toml`:

```toml
[package]
name = "window-spike"
version = "0.0.0"
edition = "2024"
publish = false

[[bin]]
name = "window-spike"
path = "src/main.rs"

[dependencies]
carapace = { path = "../carapace" }
carapace-demo = { path = "../carapace-demo" }
winit = "0.30.13"
wgpu = "29.0.3"
vello = "0.9.0"
peniko = "0.6.1"
pollster = "0.4"
```

- [ ] **Step 2: Register the workspace member**

In the root `Cargo.toml`, add `"crates/window-spike"` to the `members` array:

```toml
[workspace]
members = ["crates/hittest", "crates/carapace", "crates/carapace-demo", "crates/window-spike"]
resolver = "2"
```

- [ ] **Step 3: Write the spike — borderless/transparent window + transparent render of the Headspace bitmap**

`crates/window-spike/src/main.rs`. This loads the real `reference` skin, pulls the decoded Headspace image out of its scene, and renders it through the carapace pipeline shape (vello → intermediate `Rgba8Unorm` → `TextureBlitter` → premultiplied surface) with a transparent base. `SCALE` matches the demo's feel.

```rust
//! THROWAWAY feasibility spike — total window replacement. Not production code.
//! Proves a borderless/transparent/draggable window rendering the real Headspace skin.

use std::sync::Arc;

use carapace::scene::Node;
use carapace_demo::demo_host::DemoHost;
use vello::kurbo::Affine;
use vello::peniko::{Blob, Color as VColor, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use vello::{AaConfig, RenderParams, Scene as VScene};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const SCALE: u32 = 2;

/// The decoded skin bitmap + its native size, extracted from the real reference skin.
struct Bitmap {
    rgba: Vec<u8>,
    w: u32,
    h: u32,
}

fn load_headspace_bitmap() -> Bitmap {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../carapace-demo/skins/reference");
    let (_m, source) = carapace::skin::load_dir(&dir).expect("load reference skin");
    let engine = carapace::engine::Engine::new(
        Box::new(DemoHost::new()),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .expect("build engine");
    // The reference skin's first Image node is the Headspace faceplate.
    for node in &engine.scene().nodes {
        if let Node::Image { image, .. } = node {
            return Bitmap {
                rgba: image.rgba.clone(),
                w: image.width,
                h: image.height,
            };
        }
    }
    panic!("reference skin has no Image node");
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
    intermediate: wgpu::TextureView,
}

struct App {
    bitmap: Bitmap,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    renderer: Option<vello::Renderer>,
}

impl App {
    fn new() -> Self {
        Self {
            bitmap: load_headspace_bitmap(),
            window: None,
            gpu: None,
            renderer: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (cw, ch) = (self.bitmap.w, self.bitmap.h);
        let attrs = Window::default_attributes()
            .with_title("window-spike")
            .with_decorations(false) // no OS chrome
            .with_transparent(true) // transparent window
            .with_inner_size(winit::dpi::LogicalSize::new(cw * SCALE, ch * SCALE));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let phys = window.inner_size();
        let (pw, ph) = (phys.width.max(1), phys.height.max(1));

        let gpu = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let surface = instance.create_surface(window.clone()).expect("surface");
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .expect("adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("device");
            let caps = surface.get_capabilities(&adapter);
            let surface_format = *caps.formats.first().expect("format");
            // FINDING: record which alpha modes the macOS adapter offers. Prefer PreMultiplied;
            // fall back to PostMultiplied; Opaque would defeat transparency.
            eprintln!("surface alpha_modes offered: {:?}", caps.alpha_modes);
            let alpha_mode = if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                caps.alpha_modes[0]
            };
            eprintln!("using alpha_mode: {alpha_mode:?}");
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: pw,
                height: ph,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            let blitter = wgpu::util::TextureBlitter::new(&device, surface_format);
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("intermediate"),
                size: wgpu::Extent3d {
                    width: pw,
                    height: ph,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let intermediate = tex.create_view(&wgpu::TextureViewDescriptor::default());
            Gpu {
                surface,
                device,
                queue,
                config,
                blitter,
                intermediate,
            }
        });
        self.renderer = Some(
            vello::Renderer::new(
                &gpu.device,
                vello::RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .expect("vello renderer"),
        );
        self.window = Some(window.clone());
        self.gpu = Some(gpu);
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let (Some(gpu), Some(renderer), Some(_win)) =
                    (self.gpu.as_mut(), self.renderer.as_mut(), self.window.as_ref())
                else {
                    return;
                };
                let frame = match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => return,
                };
                let surface_view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Build a vello scene: draw the bitmap scaled to fill, on a TRANSPARENT base.
                let mut vs = VScene::new();
                let img = ImageData {
                    data: Blob::new(Arc::new(self.bitmap.rgba.clone())),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: self.bitmap.w,
                    height: self.bitmap.h,
                };
                let sx = gpu.config.width as f64 / self.bitmap.w as f64;
                let sy = gpu.config.height as f64 / self.bitmap.h as f64;
                vs.draw_image(
                    ImageBrush::new(img).as_ref(),
                    Affine::scale_non_uniform(sx, sy),
                );
                renderer
                    .render_to_texture(
                        &gpu.device,
                        &gpu.queue,
                        &vs,
                        &gpu.intermediate,
                        &RenderParams {
                            // TRANSPARENT base — the whole point of the spike.
                            base_color: VColor::from_rgba8(0, 0, 0, 0),
                            width: gpu.config.width,
                            height: gpu.config.height,
                            antialiasing_method: AaConfig::Area,
                        },
                    )
                    .expect("render");
                let mut enc = gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                gpu.blitter
                    .copy(&gpu.device, &mut enc, &gpu.intermediate, &surface_view);
                gpu.queue.submit(Some(enc.finish()));
                frame.present();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut App::new()).unwrap();
}
```

- [ ] **Step 4: Build and check it compiles cleanly**

Run: `cargo build -p window-spike && cargo clippy -p window-spike -- -D warnings`
Expected: builds, no clippy warnings. (If a wgpu/vello/winit API name differs from the demo's usage, mirror exactly what `crates/carapace-demo/src/main.rs` does — it uses the same versions.)

- [ ] **Step 5: Launch and human-verify Tier 1**

Run: `cargo run -p window-spike`
Expected (human-visual): a window with **no title bar** opens; the Headspace head renders and the **desktop is visible through the transparent margins** around the head (not a black or opaque rectangle). The `eprintln!` lines report the offered `alpha_modes` and the one chosen — note them for the findings doc.

If the margins are black/opaque rather than transparent, that is a finding to capture: try the other `alpha_mode`, and confirm `with_transparent(true)` + the transparent `base_color` are both in effect. Iterate until the desktop shows through (Tier 1 met) or record precisely what blocked it.

- [ ] **Step 6: Commit**

```bash
git add crates/window-spike Cargo.toml
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(window): borderless transparent window rendering the Headspace bitmap

Throwaway crates/window-spike: real reference skin loaded via carapace, its
bitmap rendered through a local mirror of the vello->intermediate->blit->
premultiplied-surface pipeline with a transparent base. Tier 1 (chrome-less
floating skin) — engine and demo untouched."
```

---

### Task 2: Drag + controls + click-through probe + findings (Tier 2 & 3)

Add interaction wired directly to winit, probe click-through, and write the findings doc + commit a screenshot.

**Files:**
- Modify: `crates/window-spike/src/main.rs`
- Create: `docs/superpowers/specs/2026-06-19-window-replacement-spike-findings.md`
- Create: `crates/window-spike/screenshot.png`

**Interfaces:**
- Consumes: the `App`/`Gpu` setup from Task 1; winit `Window::drag_window()`, `Window::set_minimized(bool)`, `Window::set_cursor_hittest(bool)`, `ActiveEventLoop::exit()`.
- Produces: the spike's interaction behavior + the findings deliverable.

- [ ] **Step 1: Track the cursor and route presses (drag / minimize / close)**

In `App`, add a cursor field: `cursor: (f64, f64)` (init `(0.0, 0.0)` in `App::new`). Handle `CursorMoved` and `MouseInput` in `window_event` (add these arms alongside the existing ones). Hit-test in **canvas space** (cursor scaled by `canvas / inner_size`) so it is HiDPI-robust. The min/close glyph rects are eyeballed from the Headspace bitmap's top-center `_ X` glyphs (canvas is `w × h` ≈ 342×394); tune by observation.

```rust
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                let Some(win) = self.window.as_ref() else { return };
                let size = win.inner_size();
                // cursor (physical) -> canvas coords
                let cx = self.cursor.0 * self.bitmap.w as f64 / size.width.max(1) as f64;
                let cy = self.cursor.1 * self.bitmap.h as f64 / size.height.max(1) as f64;
                // Eyeballed Headspace min/close glyph rects (canvas space) — tune by observation.
                let in_rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
                    cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1
                };
                if in_rect(150.0, 4.0, 170.0, 24.0) {
                    win.set_minimized(true); // minimize glyph
                } else if in_rect(172.0, 4.0, 194.0, 24.0) {
                    event_loop.exit(); // close glyph
                } else {
                    let _ = win.drag_window(); // press on body -> move window
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        logical_key: winit::keyboard::Key::Character(ref c),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } if c == "t" => {
                // Tier 3 probe: toggle whole-window click-through.
                self.clickthrough = !self.clickthrough;
                if let Some(win) = self.window.as_ref() {
                    let _ = win.set_cursor_hittest(!self.clickthrough);
                    eprintln!("cursor_hittest set to {}", !self.clickthrough);
                }
            }
```

Add the `clickthrough: bool` field to `App` (init `false`).

- [ ] **Step 2: Build and check**

Run: `cargo build -p window-spike && cargo clippy -p window-spike -- -D warnings`
Expected: clean build, no warnings.

- [ ] **Step 3: Human-verify Tier 2 + probe Tier 3**

Run: `cargo run -p window-spike`
Expected (human-visual):
- **Drag:** press on the head body and move the mouse — the window follows (Tier 2). 
- **Controls:** click the minimize glyph (top-center `_`) → window minimizes; click the close glyph (`X`) → app quits. If the rects miss the drawn glyphs, adjust the `in_rect` coordinates and re-run until they line up (spike tuning).
- **Click-through (Tier 3 probe):** press `t`; observe whether clicks now pass through the whole window to apps behind (and the skin stops receiving them). Note the exact behavior — this is whole-window, not per-pixel; record what `set_cursor_hittest` does and whether per-pixel passthrough seems reachable on macOS.

- [ ] **Step 4: Capture a screenshot**

Take a screenshot of the floating, chrome-less head on the desktop (with something visible behind it to show transparency) and save it to `crates/window-spike/screenshot.png`.

- [ ] **Step 5: Write the findings doc**

Create `docs/superpowers/specs/2026-06-19-window-replacement-spike-findings.md` capturing the actual results. Fill every section with what was observed (not placeholders):

```markdown
# Total Window Replacement — Spike Findings (2026-06-19)

Spike crate: `crates/window-spike` (throwaway). Platform tested: macOS / Metal.

## Tier 1 — chrome-less transparent window: <WORKED | PARTIAL | BLOCKED>
- winit attrs used: `with_decorations(false)`, `with_transparent(true)`.
- Surface `alpha_modes` offered by the adapter: <paste the eprintln output>.
- `alpha_mode` chosen: <PreMultiplied | PostMultiplied | ...>.
- vello `base_color`: transparent `from_rgba8(0,0,0,0)`.
- Result: <does the desktop show through the margins? any premultiply/blend artifacts?>

## render.rs interface gap (for the real phase)
- `carapace::render::Renderer::draw` clears to opaque black; the real feature needs a
  configurable transparent base. Concrete proposal: <e.g. add `base_color` to `RenderTarget`
  or a param on `draw`>.

## Tier 2 — drag + controls: <WORKED | PARTIAL | BLOCKED>
- Drag via `Window::drag_window()` on body press: <result>.
- Min/close via eyeballed canvas-space rects → `set_minimized` / `exit`: <result, final rects used>.

## Tier 3 — click-through verdict
- `Window::set_cursor_hittest(false)`: <what it did — whole-window pass-through?>.
- Per-pixel click-through (clicks on transparent pixels only): <reachable on macOS? how — e.g.
  native NSWindow `isOpaque`/`ignoresMouseEvents`, shaped input region, or not feasible via winit>.

## Recommendation for the real phase
- <Is total window replacement feasible? What's the host/engine work? What stays risky?>
```

- [ ] **Step 6: Commit**

```bash
git add crates/window-spike/src/main.rs crates/window-spike/screenshot.png \
  docs/superpowers/specs/2026-06-19-window-replacement-spike-findings.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(window): drag + min/close controls + click-through probe + findings

Tier 2 (drag window by body, drawn min/close glyphs drive set_minimized/exit) and
Tier 3 probe (set_cursor_hittest). Findings doc records the working winit/wgpu knobs,
the render.rs transparent-base gap, and the click-through verdict for the real phase.
Screenshot = human-visual evidence."
```

---

## Self-Review

**1. Spec coverage:**
- Throwaway scratch crate rendering real skin via mirrored pipeline w/ transparent base → Task 1. ✅
- Borderless + transparent + premultiplied alpha + transparent base (Tier 1) → Task 1 Steps 3/5. ✅
- Drag + min/close controls (Tier 2) → Task 2 Step 1/3. ✅
- Click-through probe (Tier 3, characterize only) → Task 2 Step 1/3 (the `t` toggle). ✅
- macOS-only, engine/demo untouched, no new deps → Global Constraints + additive file list. ✅
- Deliverables: working crate (T1), screenshot (T2 Step 4), findings doc (T2 Step 5). ✅
- render.rs transparent-base gap recorded → findings doc section. ✅

**2. Placeholder scan:** No TBD/TODO in the *plan*. The findings-doc *template* contains `<angle-bracket>` fill-ins by design — it is a deliverable the implementer completes from real observation (Step 5 says "fill every section with what was observed"). That is correct for a findings artifact, not a plan placeholder.

**3. Type consistency:** `App` fields (`bitmap`, `window`, `gpu`, `renderer`, `cursor`, `clickthrough`) are introduced in Task 1 and extended in Task 2 (Step 1 explicitly adds `cursor` and `clickthrough`). `Bitmap { rgba, w, h }`, `Gpu { surface, device, queue, config, blitter, intermediate }`, and the vello/wgpu calls match the versions and usage in `crates/carapace-demo/src/main.rs`. The `Node::Image { image, .. }` / `DecodedImage { rgba, width, height }` shapes match `carapace::scene` / `carapace::asset`.

**Note on testing:** This plan intentionally has no unit/integration tests — it is a throwaway GUI spike whose success is human-visual (transparent floating window, drag, controls). Forcing assert-based tests on "is the window transparent" would be fake coverage. Verification is `cargo build`/`cargo clippy` clean + the human-observed run + the findings doc. This is a deliberate, spec-sanctioned deviation from the usual TDD structure.

# Live Host View Region Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A skin declares a `view{}` region the embedding app fills with its own live pixels; carapace composites that content (an embedder-supplied wgpu texture) into the rect and frames it with the skin — proven by a live CPU monitor rendered inside the Headspace screen.

**Architecture:** `view{}` → a plain-data `Node::View { id, dest }` that draws *nothing* (a transparent hole); `Scene::views()` exposes the rects. `Renderer::draw` gains a per-view content source and, after the vello skin pass, composites each supplied texture into its surface-space rect with a textured-quad render pass. No texture for a view → the hole is left for an external compositor.

**Tech Stack:** Rust (edition 2024), `vello` 0.9 / `wgpu` 29 (a small composite pipeline + WGSL), the existing engine/vocab model.

**Spec:** `docs/superpowers/specs/2026-06-22-live-host-view-region-design.md`

## Global Constraints

- Rust edition 2024 / Rust 1.96. CI builds `--locked`. No new third-party crates.
- **GPU-texture transport only** (shared wgpu device); no CPU readback.
- The engine composites a **generic texture into a generic rect** — zero app/domain knowledge (neutrality holds; the engine just gains a composite pass).
- **Target requirement:** the composite is a render pass into `target.view`, so that texture must be created with `RENDER_ATTACHMENT` usage (in addition to vello's `STORAGE_BINDING` and the blit's `TEXTURE_BINDING`). The demo intermediate and the GPU-test offscreen texture add it.
- **CI gates on clippy + fmt.** Before every commit run `cargo fmt`, then BOTH `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings`. Clean.
- `scene::summary()` stays geometry-neutral (`view id=<id>`, no coordinates).
- Forward-compat (north star, not built): keep the view API plain-data (rects as `ImageDest` values; content as an opaque `&wgpu::TextureView`) so a future C ABI slots on.
- All commits use identity **Daniel Agbemava <danagbemava@gmail.com>**; no Claude attribution.

---

## File Structure

- `crates/carapace/src/scene.rs` — `Node::View { id, dest: ImageDest }`; `Scene::views()`; `summary()` arm.
- `crates/carapace/src/vocab.rs` — `ViewPrim` (`view{}` → `Node::View`); register in `base()` (→ 6).
- `crates/carapace/src/render.rs` — `Node::View` temp no-op arm (Task 1) → real composite (Task 2); `draw()` gains `view_tex`; a composite pipeline + WGSL.
- `crates/carapace/tests/render_offscreen.rs` — `offscreen()` gains `RENDER_ATTACHMENT`; callers pass `view_tex`; composite GPU tests.
- `crates/carapace-demo/examples/shoot.rs`, `crates/carapace-demo/src/main.rs` — pass `view_tex` (`|_| None` / the monitor texture).
- `crates/carapace-demo/src/main.rs` — intermediate gains `RENDER_ATTACHMENT`; a monitor sub-render painted into the Headspace `view{}`.
- `crates/carapace-demo/skins/reference/skin.lua` — `view{ id="display" }` over the screen.
- `README.md`.

---

## Task 1: `view{}` primitive — `Node::View`, `Scene::views()`, vocab, summary (headless)

**Files:**
- Modify: `crates/carapace/src/scene.rs` (`Node` enum, `views()`, `summary()`, tests)
- Modify: `crates/carapace/src/vocab.rs` (`ViewPrim`, `base()`, tests)
- Modify: `crates/carapace/src/render.rs` (temporary `Node::View` no-op arm so the crate compiles)

**Interfaces:**
- Produces: `Node::View { id: String, dest: crate::scene::ImageDest }`; `Scene::views() -> Vec<(String, ImageDest)>`; `view{ id, x, y, w, h }` Lua primitive; `summary()` line `view id=<id>`.

- [ ] **Step 1: Write the failing tests**

In `crates/carapace/src/scene.rs` tests:

```rust
    #[test]
    fn views_accessor_and_summary() {
        let scene = Scene {
            canvas: (300, 200),
            nodes: vec![Node::View {
                id: "display".to_string(),
                dest: ImageDest { x: 10.0, y: 20.0, w: 100.0, h: 80.0 },
            }],
        };
        assert_eq!(
            scene.views(),
            vec![("display".to_string(), ImageDest { x: 10.0, y: 20.0, w: 100.0, h: 80.0 })]
        );
        assert_eq!(scene.summary(), "canvas 300x200\nview id=display");
    }
```

In `crates/carapace/src/vocab.rs` tests:

```rust
    #[test]
    fn view_prim_builds_view_node() {
        let lua = Lua::new();
        let t = tbl(&lua, "return { id='display', x=10, y=20, w=100, h=80 }");
        match one(ViewPrim.build(&t, &mut NoHandlers)) {
            Node::View { id, dest } => {
                assert_eq!(id, "display");
                assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 20.0, 100.0, 80.0));
            }
            other => panic!("expected View, got {other:?}"),
        }
    }

    #[test]
    fn view_prim_requires_id_and_geometry() {
        let lua = Lua::new();
        let no_id = tbl(&lua, "return { x=0, y=0, w=1, h=1 }");
        assert!(matches!(ViewPrim.build(&no_id, &mut NoHandlers), Err(BuildError::MissingField("id"))));
        let no_w = tbl(&lua, "return { id='d', x=0, y=0, h=1 }");
        assert!(matches!(ViewPrim.build(&no_w, &mut NoHandlers), Err(BuildError::MissingField("w"))));
    }
```

Update the base-registry count test (it currently asserts 5 — `base_registry_now_has_five`): rename to `base_registry_now_has_six` and assert `6`.

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace scene::tests::views_accessor_and_summary`
Expected: FAIL — no `Node::View`.

- [ ] **Step 3: Add `Node::View` + `views()` + `summary()` arm**

In `crates/carapace/src/scene.rs`, add to `enum Node` (after `Image`):

```rust
    View {
        id: String,
        dest: ImageDest,
    },
```

Add the accessor in `impl Scene` (near `hit`):

```rust
    /// The host-content regions a skin declares, in canvas coords — the embedder fills these.
    pub fn views(&self) -> Vec<(String, ImageDest)> {
        self.nodes
            .iter()
            .filter_map(|n| match n {
                Node::View { id, dest } => Some((id.clone(), *dest)),
                _ => None,
            })
            .collect()
    }
```

In `summary()`, add a match arm after `Node::Image`:

```rust
                Node::View { id, .. } => format!("view id={id}"),
```

- [ ] **Step 4: Add `ViewPrim` + register it**

In `crates/carapace/src/vocab.rs`, add after `ImagePrim`:

```rust
struct ViewPrim;
impl Primitive for ViewPrim {
    fn id(&self) -> &str {
        "view"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let id: String = args.get("id").map_err(|_| BuildError::MissingField("id"))?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").map_err(|_| BuildError::MissingField("w"))?;
        let h: f32 = args.get("h").map_err(|_| BuildError::MissingField("h"))?;
        Ok(vec![Node::View {
            id,
            dest: crate::scene::ImageDest { x, y, w, h },
        }])
    }
}
```

In `VocabRegistry::base()`, add `r.register(Box::new(ViewPrim));`.

- [ ] **Step 5: Temporary no-op render arm (so the crate compiles)**

Adding `Node::View` makes `render.rs`'s `draw()` `match node` non-exhaustive. Add a temporary arm (after `Node::Image`) — **Task 2 replaces it with the composite**:

```rust
                Node::View { .. } => {} // composited in the live-host-view-region render task
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p carapace --lib`
Expected: PASS — the new view tests + the `6`-count test + everything else (the no-op render arm compiles).

- [ ] **Step 7: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/scene.rs crates/carapace/src/vocab.rs crates/carapace/src/render.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(scene): view{} primitive (Node::View + Scene::views)"
```

---

## Task 2: Render composite — `draw()` view source + textured-quad pass (GPU)

**Files:**
- Modify: `crates/carapace/src/render.rs` (imports, `Renderer` fields + `new`, `draw` signature + composite; replace the no-op arm)
- Create: `crates/carapace/src/composite.wgsl`
- Modify: `crates/carapace/tests/render_offscreen.rs` (`offscreen()` + `RENDER_ATTACHMENT`; all `draw()` callers pass `view_tex`; composite tests)
- Modify: `crates/carapace-demo/examples/shoot.rs`, `crates/carapace-demo/src/main.rs` (callers pass `view_tex: |_| None` — main.rs's real texture is Task 3)

**Interfaces:**
- Consumes: `Node::View`, `Scene` (Task 1).
- Produces: `Renderer::draw(scene, read_value, view_tex, target)` where `view_tex: impl Fn(&str) -> Option<&wgpu::TextureView>`; composites each supplied texture into its view rect.

> **GPU note.** The composite is a standard textured-quad render pass into `target.view` (`LoadOp::Load`, viewport = the view's surface-space rect). It requires `target.view`'s texture to have `RENDER_ATTACHMENT` usage. If vello 0.9's `render_to_texture` rejects a target that also has `RENDER_ATTACHMENT` (it shouldn't — multiple usages are legal), fall back to a compute composite (write into the `STORAGE_BINDING` intermediate); the GPU tests are the source of truth. Verify content orientation with the demo (Task 3) — if the monitor is vertically flipped, swap the `uv.y` mapping in the shader.

- [ ] **Step 1: Write the WGSL shader**

Create `crates/carapace/src/composite.wgsl`:

```wgsl
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    // A triangle-strip quad covering the render pass viewport.
    var p = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    var uv = array<vec2<f32>, 4>(vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(0.0, 0.0), vec2(1.0, 0.0));
    var o: VsOut;
    o.pos = vec4(p[vi], 0.0, 1.0);
    o.uv = uv[vi];
    return o;
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src, samp, in.uv);
}
```

- [ ] **Step 2: Write the failing GPU tests**

Append to `crates/carapace/tests/render_offscreen.rs`. First, the existing `offscreen()` texture needs `RENDER_ATTACHMENT` — change its `usage` to `STORAGE_BINDING | COPY_SRC | RENDER_ATTACHMENT`. Then add:

```rust
// A solid-color source texture for a view (proves the composite accepts an ARBITRARY texture).
fn solid_source(o: &Offscreen, w: u32, h: u32, rgba: [u8; 4]) -> wgpu::Texture {
    let tex = o.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("view-src"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let data = vec![rgba; (w * h) as usize].concat();
    o.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    tex
}

#[test]
fn view_composites_supplied_texture_into_its_rect() {
    use carapace::scene::{Color, ImageDest, Node, Paint, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![
            Node::Fill { path: rect(0.0, 0.0, 100.0, 100.0), paint: Paint::Solid(Color { r: 10, g: 10, b: 10, a: 255 }) },
            Node::View { id: "v".to_string(), dest: ImageDest { x: 30.0, y: 30.0, w: 40.0, h: 40.0 } },
        ],
    };
    let src = solid_source(&o, 40, 40, [255, 0, 0, 255]);
    let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
    r.draw(
        &scene, |_| None,
        |id| if id == "v" { Some(&src_view) } else { None },
        &RenderTarget { device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h, base_color: Color { r: 0, g: 0, b: 0, a: 255 } },
    );
    let data = readback(&o);
    assert_eq!(px(&data, 100, 50, 50), [255, 0, 0], "view rect shows the supplied texture");
    assert_eq!(px(&data, 100, 10, 10), [10, 10, 10], "outside the view shows the skin fill");
}

#[test]
fn view_without_texture_leaves_the_hole() {
    use carapace::scene::{Color, ImageDest, Node, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::View { id: "v".to_string(), dest: ImageDest { x: 30.0, y: 30.0, w: 40.0, h: 40.0 } }],
    };
    r.draw(
        &scene, |_| None, |_| None,
        &RenderTarget { device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h, base_color: Color { r: 7, g: 7, b: 7, a: 255 } },
    );
    assert_eq!(px(&readback(&o), 100, 50, 50), [7, 7, 7], "no texture -> the hole stays the base color");
}
```

Update every existing `r.draw(&scene, read, &target)` call in `render_offscreen.rs` to `r.draw(&scene, read, |_| None, &target)`.

- [ ] **Step 3: Run to verify RED**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen view_composites_supplied_texture_into_its_rect`
Expected: FAIL to compile — `draw` takes no `view_tex` yet.

- [ ] **Step 4: Add the composite pipeline to `Renderer`**

In `crates/carapace/src/render.rs`, add fields to `Renderer`:

```rust
    composite_pipeline: wgpu::RenderPipeline,
    composite_sampler: wgpu::Sampler,
    composite_bgl: wgpu::BindGroupLayout,
```

In `Renderer::new`, after building `inner`, create them:

```rust
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("view-composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("view-composite-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("view-composite-pl"), bind_group_layouts: &[&composite_bgl], push_constant_ranges: &[],
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("view-composite-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None, multisample: Default::default(), multiview: None, cache: None,
        });
        let composite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("view-composite-sampler"),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });
        Self { inner, font_cx: FontContext::new(), layout_cx: LayoutContext::new(), families: HashMap::new(), layouts: HashMap::new(), composite_pipeline, composite_sampler, composite_bgl }
```

(Adjust exact `wgpu` 29 field names with `cargo doc -p wgpu` if any differ; the GPU tests are the source of truth.)

- [ ] **Step 5: Add `view_tex` to `draw` + composite after vello**

Change the `draw` signature to add `view_tex` before `target`:

```rust
    pub fn draw(
        &mut self,
        scene: &Scene,
        read_value: impl Fn(&str) -> Option<StateValue>,
        view_tex: impl Fn(&str) -> Option<&wgpu::TextureView>,
        target: &RenderTarget,
    ) {
```

Keep the `Node::View { .. } => {}` no-op in the vello loop (the view draws nothing — a hole). After the existing `self.inner.render_to_texture(...)` call (the vello pass), add the composite:

```rust
        // Composite embedder-supplied content into each view's surface-space rect.
        let mut srcs: Vec<(Rect, &wgpu::TextureView)> = Vec::new();
        for node in &scene.nodes {
            if let Node::View { id, dest } = node
                && let Some(tex) = view_tex(id)
            {
                let r = Rect::new(
                    dest.x as f64 * sx, dest.y as f64 * sy,
                    (dest.x + dest.w) as f64 * sx, (dest.y + dest.h) as f64 * sy,
                );
                srcs.push((r, tex));
            }
        }
        if !srcs.is_empty() {
            let bgs: Vec<wgpu::BindGroup> = srcs.iter().map(|(_, tex)| {
                target.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("view-composite-bg"), layout: &self.composite_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(tex) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.composite_sampler) },
                    ],
                })
            }).collect();
            let mut enc = target.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("view-composite-enc") });
            {
                let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("view-composite-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target.view, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
                });
                rp.set_pipeline(&self.composite_pipeline);
                for ((r, _), bg) in srcs.iter().zip(bgs.iter()) {
                    rp.set_viewport(r.x0 as f32, r.y0 as f32, (r.x1 - r.x0) as f32, (r.y1 - r.y0) as f32, 0.0, 1.0);
                    rp.set_bind_group(0, bg, &[]);
                    rp.draw(0..4, 0..1);
                }
            }
            target.queue.submit(Some(enc.finish()));
        }
```

Remove the standalone `Node::View { .. } => {}` arm comment update if needed (the vello-loop arm stays a no-op; the composite is the new post-pass block).

- [ ] **Step 6: Update the demo callers (no real texture yet)**

In `crates/carapace-demo/examples/shoot.rs` and `crates/carapace-demo/src/main.rs`, change the `r.draw(...)` / `renderer.draw(...)` call to pass `|_| None` as the new `view_tex` arg. (Task 3 swaps main.rs's to the monitor texture.)

- [ ] **Step 7: Run GPU + lib tests**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen` and `cargo test -p carapace --lib` and `cargo build --workspace`
Expected: PASS — both composite tests + all existing sentinels; the demo compiles.

- [ ] **Step 8: fmt + both clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings
git add crates/carapace/src/render.rs crates/carapace/src/composite.wgsl crates/carapace/tests/render_offscreen.rs \
  crates/carapace-demo/examples/shoot.rs crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(render): composite embedder-supplied textures into view{} regions"
```

---

## Task 3: Demo — a live monitor inside the Headspace screen

**Files:**
- Modify: `crates/carapace-demo/skins/reference/skin.lua` (add `view{ id="display" }`)
- Modify: `crates/carapace-demo/src/main.rs` (intermediate `RENDER_ATTACHMENT`; a monitor sub-render painted into the view, supplied via `view_tex`)

**Interfaces:**
- Consumes: `view{}`/`Scene::views()` (Task 1), the `view_tex` composite (Task 2), the existing `SysmonHost` + `GaugePrim` (Phase 6), a second `Renderer`.

GUI wiring — verified by compile + the human smoke check.

- [ ] **Step 1: Declare the view in the Headspace skin**

In `crates/carapace-demo/skins/reference/skin.lua`, add over the faceplate's display screen (the black rect; trace its bounds — roughly x=78, y=50, w=186, h=150):

```lua
-- the host-content region: the embedder paints a live monitor into the display screen
view{ id = "display", x = 78, y = 50, w = 186, h = 150 }
```

(Add it after the `image{}`/drag region; its rect is opaque-covered by the composited texture, so order vs the black bitmap display doesn't matter.)

- [ ] **Step 2: Give the intermediate `RENDER_ATTACHMENT`**

In `crates/carapace-demo/src/main.rs` `make_intermediate`, change the texture `usage` to:

```rust
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
```

- [ ] **Step 3: Build a monitor sub-render**

Add to `App` (or a small `Monitor` struct) the pieces to render a gauge scene into a texture: a second `Engine` on a `SysmonHost` running a tiny inline gauge skin, a second `Renderer`, and a target texture sized to the view rect. The monitor content reuses `GaugePrim` (so register it in that engine's registry):

```rust
struct Monitor {
    engine: carapace::engine::Engine,
    renderer: Renderer,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}
const MONITOR_SKIN: &str = "\
    fill{ path = rect{x=0,y=0,w=186,h=150}, color = {r=12,g=16,b=22} }\n\
    gauge{ x = 16,  y = 20, value = 'cpu',  label = 'CPU' }\n\
    gauge{ x = 76,  y = 20, value = 'mem',  label = 'MEM' }\n\
    gauge{ x = 136, y = 20, value = 'swap', label = 'SWP' }\n";

impl Monitor {
    fn new(device: &wgpu::Device, outbox: WindowOutbox) -> Self {
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(carapace_demo::gauge::GaugePrim));
        let engine = Engine::new(
            Box::new(carapace_demo::sysmon_host::SysmonHost::with_outbox(outbox)),
            reg,
            carapace::command::SkinSource::inline(MONITOR_SKIN, (186, 150)),
        ).unwrap();
        let (tex, view) = Self::make_tex(device, 186, 150);
        Self { engine, renderer: Renderer::new(device), tex, view, size: (186, 150) }
    }
    fn make_tex(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("monitor"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }
    fn paint(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, dt: std::time::Duration) {
        self.engine.update(dt);
        self.renderer.draw(
            self.engine.scene(),
            |k| self.engine.state(k),
            |_| None,
            &RenderTarget {
                device, queue, view: &self.view, width: self.size.0, height: self.size.1,
                base_color: carapace::scene::Color { r: 12, g: 16, b: 22, a: 255 },
            },
        );
    }
}
```

Add a `monitor: Option<Monitor>` field to `App`, created when the GPU is set up (it needs the device). The `SysmonHost::with_outbox` reuses `App::window_outbox.clone()`.

- [ ] **Step 4: Paint the monitor + supply it to the main draw**

In `RedrawRequested`, after `self.engine.update(dt)` and `apply_window_ops`, paint the monitor and pass its texture to the main draw's `view_tex`:

```rust
                if let (Some(mon), Some(gpu)) = (self.monitor.as_mut(), self.gpu.as_ref()) {
                    mon.paint(&gpu.device, &gpu.queue, dt);
                }
                // … in the main renderer.draw(...) call, pass the monitor texture for "display":
                let mon_view = self.monitor.as_ref().map(|m| &m.view);
                renderer.draw(
                    engine.scene(),
                    |k| engine.state(k),
                    |id| if id == "display" { mon_view } else { None },
                    &RenderTarget { /* …existing fields… */ },
                );
```

(Resolve the borrow ordering so `self.monitor` is painted before the immutable borrows for the main draw; the `Option<&TextureView>` is captured before the draw closure.)

- [ ] **Step 5: Build + human smoke check**

Run: `cargo build -p carapace-demo`, then `cargo run -p carapace-demo`.
Expected: the Headspace skin's black screen now shows a **live CPU/MEM/SWP monitor** inside the floating window; drag/close still work; Tab/H still cycle. If the monitor is vertically flipped, swap the `uv.y` values in `composite.wgsl` (Task 2) and rebuild.

- [ ] **Step 6: Demo tests still pass + fmt + clippy + commit**

```bash
cargo test -p carapace-demo
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/skins/reference/skin.lua crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): live monitor painted into the Headspace view region"
```

---

## Task 4: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document the live host view region**

In `README.md`, add to the vocabulary/roadmap: a `view{ id, x, y, w, h }` primitive declares a host-content region; the embedding app renders its own pixels (a UI / video / visualizer / monitor) into a wgpu texture and hands it to `Renderer::draw`, and carapace composites it into the rect and frames it with the skin — the seam by which an app embeds carapace and "wears" a skin. Note the demo's Headspace screen hosts a live system monitor through it. Keep claims accurate: GPU-texture transport, same-device; foreign-process app embedding and responsive layout are NOT part of this.

- [ ] **Step 2: Verify + commit**

```bash
cargo test --workspace && cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs: README documents the live host view region (view{})"
```

---

## Self-Review (completed during planning)

**Spec coverage:**
- `view{}` → `Node::View` + `Scene::views()` + `summary()` → Task 1. ✅
- `ViewPrim` registered in `base()` (→6) → Task 1. ✅
- `Node::View` draws nothing (hole); `Renderer::draw` gains `view_tex`; engine composites a supplied texture into the rect; no texture → hole → Task 2 (+ both GPU tests). ✅
- GPU-texture transport, `RENDER_ATTACHMENT` target requirement → Task 2 (offscreen) + Task 3 (intermediate). ✅
- Demo: Headspace `view{}` + a live embedder-painted monitor → Task 3. ✅
- Geometry-neutral `summary()` (`view id=<id>`) → Task 1. ✅
- README → Task 4. ✅
- Neutrality (generic texture, no app knowledge); forward-compat (plain rects + opaque texture handle) → upheld; no C-ABI/framework/foreign-embedding/responsiveness built (all out of scope). ✅

**Compile-safety:** Task 1 adds `Node::View` and immediately gives `render.rs` a temporary no-op arm so the crate compiles before Task 2 supplies the composite (the 5c/5d pattern). Task 2's `draw()` signature change updates every caller in the same task.

**GUI caveat:** Task 3's monitor wiring is verified by compile + human smoke; the *testable* core (the composite of an arbitrary supplied texture into a view rect, and the no-texture hole) is fully covered by Task 2's GPU tests.

**Type consistency:** `Node::View { id: String, dest: ImageDest }`, `Scene::views() -> Vec<(String, ImageDest)>`, `draw(…, view_tex: impl Fn(&str) -> Option<&wgpu::TextureView>, target)` used consistently across tasks.

# carapace-ffi v4 — Seamless Skin Swap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `carapace_swap_skin` seamless — the old skin keeps animating while the new one loads and warms, then dissolves into it over a skin-authored crossfade, with no render-loop stall and no visual pop.

**Architecture:** A skin declares its transition in `skin.toml` (`[transition]` table, engine crate). The FFI render thread gains a swap state machine (`Idle → Warming → Crossfading → Idle`): it warms the incoming engine offscreen while the old skin keeps presenting, then blends two engines' offscreen renders with a new FFI-side GPU pass. The C ABI is unchanged (MINOR bumps 3.0 → 3.1). The native showcase is rewired to exercise the live swap and animates its window to the new canvas after the fade.

**Tech Stack:** Rust 2024 (`carapace`, `carapace-ffi`), wgpu 29 / Metal / IOSurface (Apple-only render path), Swift/SwiftUI (`showcase`), cbindgen (C header), TOML/serde (manifest).

## Global Constraints

- Rust edition **2024**; repo standard rustfmt.
- FFI render path is **Apple-only** (`#[cfg(any(target_os = "macos", target_os = "ios"))]`); the crate is a near-empty shell elsewhere. New render-thread code lives behind that cfg.
- **CI gates on `clippy -D warnings`** and a **`gpu-tests` feature lane** in addition to `cargo test --workspace` + `cargo fmt`. GPU-touching tests MUST be `#[cfg(feature = "gpu-tests")]` (or under the existing `#[cfg(all(test, target_os = "macos"))]` render-test module) so the headless `check` lane doesn't panic `no wgpu adapter`.
- Crate has `#![deny(missing_docs)]` — every new `pub` item carries a `///` doc.
- **C ABI stays byte-compatible**: no new/renamed/reordered exports, structs, or enum variants. Only `CARAPACE_ABI_MINOR` changes (0 → 1).
- Skin manifest **schema stays `1`**; the `[transition]` table is fully defaulted so every existing skin still loads.
- Default transition when absent = **crossfade, 250 ms**; `duration_ms` clamped to `≤ 5000`.
- Commit after each task. Git identity is already configured for this repo. Work happens on branch `carapace-ffi-v4-seamless-swap` (already checked out); do **not** push to `main`.
- First fetch of any new third-party dependency must run via `sfw cargo ...` (Socket Firewall). This plan adds **no** new dependencies.

---

## File Structure

**Engine crate (`crates/carapace/`):**
- Modify `src/skin.rs` — add `TransitionKind`, `Transition`, defaults + clamp, and a `transition` field on `Manifest`.

**FFI crate (`crates/carapace-ffi/`):**
- Create `src/crossfade.rs` — `CrossfadeBlender`: a self-contained GPU pass that blends two `Rgba8Unorm` textures by an alpha into a target view. Owns its WGSL, pipeline, sampler, uniform buffer.
- Modify `src/render_thread.rs` — `SwapState` enum, new `RenderThread` fields, `build()` wiring, `render_one`/`apply` restructure, `run_loop` keep-ticking-during-swap, the pure `crossfade_t` helper, and the rewritten `Command::SwapSkin` handler. New gpu-tests.
- Modify `src/lib.rs` — register `mod crossfade`; update the ABI version test.
- Modify `src/guard.rs` — `CARAPACE_ABI_MINOR` 0 → 1.
- Modify `include/carapace.h` — regenerated (version constant only).
- Modify `tests/header.rs` — version expectation (if it pins the minor).

**Showcase (`showcase/`):**
- Modify `Sources/Showcase/SkinManifest.swift` — add `duration(atDir:)` parsing `[transition] duration_ms`.
- Modify `Sources/Showcase/App.swift` — rewire `cycleSkin` to live-swap + post-fade window resize.
- Modify `Tests/ShowcaseTests/...` — a unit test for the duration parse.

**Docs:**
- Modify `crates/carapace-ffi/README.md` and `docs/api/` — document the `[transition]` capability + seamless swap.

---

## Task 1: Skin-authored transition in the manifest (engine crate)

**Files:**
- Modify: `crates/carapace/src/skin.rs`
- Test: `crates/carapace/src/skin.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub enum TransitionKind { Cut, Crossfade }` (Copy)
  - `pub struct Transition { pub kind: TransitionKind, pub duration_ms: u32 }` (Copy, `Default` = `{ Crossfade, 250 }`), with `duration_ms` clamped to `≤ 5000` on load.
  - `Manifest` gains `pub transition: Transition` (defaulted).

- [ ] **Step 1: Write failing tests**

Add to the existing `#[cfg(test)] mod tests` in `crates/carapace/src/skin.rs`:

```rust
    #[test]
    fn transition_defaults_to_crossfade_250_when_absent() {
        let (m, _) = load_dir(&skins_dir().join("ok")).unwrap();
        assert_eq!(m.transition.kind, TransitionKind::Crossfade);
        assert_eq!(m.transition.duration_ms, 250);
    }

    #[test]
    fn transition_parses_explicit_cut() {
        let dir = tempdir_with(
            "schema=1\nid='x'\nname='x'\nengine='^0.1'\ncanvas={width=1,height=1}\nentry='s.lua'\n\
             [transition]\nkind='cut'\nduration_ms=100",
            "return {}",
        );
        let (m, _) = load_dir(dir.path()).unwrap();
        assert_eq!(m.transition.kind, TransitionKind::Cut);
        assert_eq!(m.transition.duration_ms, 100);
    }

    #[test]
    fn transition_duration_is_clamped() {
        let dir = tempdir_with(
            "schema=1\nid='x'\nname='x'\nengine='^0.1'\ncanvas={width=1,height=1}\nentry='s.lua'\n\
             [transition]\nkind='crossfade'\nduration_ms=999999",
            "return {}",
        );
        let (m, _) = load_dir(dir.path()).unwrap();
        assert_eq!(m.transition.duration_ms, 5000);
    }
```

Note: `tempdir_with(manifest, lua)` already exists in this test module (used by `rejects_unknown_schema`). Confirm its second arg is the Lua entry body; `"return {}"` is a valid empty skin script.

- [ ] **Step 2: Run tests — verify they fail to compile**

Run: `cargo test -p carapace skin::tests::transition -- --nocapture`
Expected: FAIL — `TransitionKind`/`transition` field don't exist.

- [ ] **Step 3: Implement the transition types + manifest field**

In `crates/carapace/src/skin.rs`, add these constants/helpers near `default_asset_dir` (around line 10):

```rust
const MAX_TRANSITION_MS: u32 = 5000;

fn default_transition_kind() -> TransitionKind {
    TransitionKind::Crossfade
}
fn default_transition_ms() -> u32 {
    250
}

/// How a skin dissolves in when another skin is swapped to it. Declared by the *incoming* skin's
/// `skin.toml` `[transition]` table. Absent table → [`Transition::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Instant replacement (still stall-free — the load is warmed off the presented frame).
    Cut,
    /// Alpha dissolve from the outgoing skin to this one over `duration_ms`.
    Crossfade,
}

/// The incoming skin's swap transition. An absent `[transition]` table yields the default
/// (`Crossfade`, 250 ms). `duration_ms` is clamped to a sane ceiling by [`load_dir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Transition {
    /// The dissolve style.
    #[serde(default = "default_transition_kind")]
    pub kind: TransitionKind,
    /// Dissolve duration in milliseconds (clamped to 5000 on load).
    #[serde(default = "default_transition_ms")]
    pub duration_ms: u32,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            kind: default_transition_kind(),
            duration_ms: default_transition_ms(),
        }
    }
}
```

Add the field to `Manifest` (after `max_size`, around line 49):

```rust
    /// How this skin dissolves in when swapped to. Defaulted; see [`Transition`].
    #[serde(default)]
    pub transition: Transition,
```

In `load_dir`, clamp the duration right after the manifest is parsed (after the `engine` check, around line 95):

```rust
    let mut manifest = manifest;
    manifest.transition.duration_ms = manifest.transition.duration_ms.min(MAX_TRANSITION_MS);
```

(Change the `let manifest: Manifest = ...` binding to feed this, and return `manifest` in the `Ok((manifest, source))` tuple as today.)

- [ ] **Step 4: Run tests — verify pass**

Run: `cargo test -p carapace skin`
Expected: PASS (new transition tests + existing skin tests green).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace/src/skin.rs
git commit -m "feat(engine): skin-authored [transition] table in manifest"
```

---

## Task 2: CrossfadeBlender GPU pass (FFI crate)

**Files:**
- Create: `crates/carapace-ffi/src/crossfade.rs`
- Modify: `crates/carapace-ffi/src/lib.rs` (register the module)
- Test: `crates/carapace-ffi/src/crossfade.rs` (inline gpu-test)

**Interfaces:**
- Consumes: `crate::render::GpuCtx` (`{ device, queue }`), offscreen views are `Rgba8Unorm` with `TEXTURE_BINDING`; the target view is `Rgba8Unorm` with `RENDER_ATTACHMENT`.
- Produces:
  - `pub struct CrossfadeBlender` with `pub fn new(device: &wgpu::Device) -> Self`
  - `pub fn draw(&self, gpu: &GpuCtx, old_view: &wgpu::TextureView, new_view: &wgpu::TextureView, dst_view: &wgpu::TextureView, t: f32)` — renders `mix(old, new, t)` into `dst_view`.

- [ ] **Step 1: Write the failing gpu-test**

Create `crates/carapace-ffi/src/crossfade.rs` with the test first (implementation stubs follow in Step 3). The test renders a solid-red and solid-blue source, blends at `t = 0.5`, reads back the center pixel, and asserts it's the midpoint.

```rust
//! `CrossfadeBlender` — a self-contained GPU pass that blends two `Rgba8Unorm` textures by an
//! alpha `t` into a target view (`out = mix(old, new, t)`). Used by the render thread's crossfade
//! swap; contains no engine or IOSurface knowledge, so it is unit-testable in isolation.
#![cfg(any(target_os = "macos", target_os = "ios"))]

use crate::render::GpuCtx;

// ... (implementation added in Step 3) ...

#[cfg(all(test, target_os = "macos", feature = "gpu-tests"))]
mod tests {
    use super::*;
    use crate::render::init_gpu;

    fn solid(gpu: &GpuCtx, w: u32, h: u32, rgba: [u8; 4]) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("solid"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: Vec<u8> = std::iter::repeat(rgba).take((w * h) as usize).flatten().collect();
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    #[test]
    fn blends_two_solids_at_half() {
        let gpu = init_gpu().expect("gpu");
        let (w, h) = (4u32, 4u32);
        let (_r, red) = solid(&gpu, w, h, [255, 0, 0, 255]);
        let (_b, blue) = solid(&gpu, w, h, [0, 0, 255, 255]);
        let dst = crate::render::new_offscreen(&gpu.device, w, h);

        let blender = CrossfadeBlender::new(&gpu.device);
        blender.draw(&gpu, &red, &blue, &dst.view, 0.5);

        let px = crate::render::readback_rgba(&gpu, &dst.tex, w, h);
        // mix(red, blue, 0.5) ≈ (128, 0, 128). Allow rounding slack.
        assert!((px[0] as i32 - 128).abs() <= 4, "R was {}", px[0]);
        assert_eq!(px[1], 0, "G");
        assert!((px[2] as i32 - 128).abs() <= 4, "B was {}", px[2]);
    }
}
```

- [ ] **Step 2: Register the module and run the test — verify it fails**

In `crates/carapace-ffi/src/lib.rs`, add under the Apple-gated section (next to `mod render_thread;`):

```rust
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod crossfade;
```

Run: `cargo test -p carapace-ffi --features gpu-tests crossfade::tests::blends_two_solids_at_half`
Expected: FAIL — `CrossfadeBlender` not defined.

- [ ] **Step 3: Implement `CrossfadeBlender`**

Fill in the body of `crossfade.rs` (between the module doc and the test module):

```rust
/// The WGSL for the blend: a fullscreen triangle whose fragment shader outputs
/// `mix(old, new, t)`. `t` arrives in `u.x` of a `vec4<f32>` uniform (padded for 16-byte alignment).
const SHADER: &str = r#"
@group(0) @binding(0) var t_old: texture_2d<f32>;
@group(0) @binding(1) var t_new: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var<uniform> u: vec4<f32>;

struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var xy = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var out: VsOut;
    let p = xy[i];
    out.pos = vec4(p, 0.0, 1.0);
    out.uv = vec2((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let a = textureSample(t_old, samp, in.uv);
    let b = textureSample(t_new, samp, in.uv);
    return mix(a, b, u.x);
}
"#;

/// Blends two `Rgba8Unorm` textures by an alpha into a target `Rgba8Unorm` view. Built once and
/// reused for every crossfade frame; the per-frame `draw` writes `t` into a uniform and re-binds
/// the (stable) source views.
pub struct CrossfadeBlender {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
}

impl CrossfadeBlender {
    /// Build the blend pipeline against the offscreen format (`Rgba8Unorm`).
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crossfade"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crossfade-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crossfade-pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crossfade-pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vs"),
                buffers: &[], compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("crossfade-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crossfade-u"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, layout, sampler, uniform }
    }

    /// Render `mix(old, new, t)` into `dst_view`. Submits its own encoder; ordering with the
    /// downstream present (blit/readback of `dst`) is guaranteed by same-queue submission order.
    pub fn draw(
        &self,
        gpu: &GpuCtx,
        old_view: &wgpu::TextureView,
        new_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        t: f32,
    ) {
        // Uniform is a padded vec4; only .x is read by the shader.
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&t.to_le_bytes());
        gpu.queue.write_buffer(&self.uniform, 0, &bytes);

        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crossfade-bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(old_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(new_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: self.uniform.as_entire_binding() },
            ],
        });

        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crossfade-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([enc.finish()]);
    }
}
```

Note on wgpu 29 API surface: `entry_point` is `Option<&str>` in this version (hence `Some("vs")`), `new_offscreen`/`readback_rgba`/`init_gpu` are already `pub` in `render.rs`. If any field name (e.g. `compilation_options`, `cache`) mismatches the pinned wgpu, cross-check against the existing pipeline construction in `carapace`'s renderer — do not add or bump wgpu.

- [ ] **Step 4: Run the gpu-test — verify pass**

Run: `cargo test -p carapace-ffi --features gpu-tests crossfade::tests::blends_two_solids_at_half`
Expected: PASS (center pixel ≈ (128, 0, 128)).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-ffi/src/crossfade.rs crates/carapace-ffi/src/lib.rs
git commit -m "feat(ffi): CrossfadeBlender — GPU mix pass for skin dissolve"
```

---

## Task 3: Crossfade progress helper (pure, no GPU)

**Files:**
- Modify: `crates/carapace-ffi/src/render_thread.rs`
- Test: `crates/carapace-ffi/src/render_thread.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `fn crossfade_t(elapsed: Duration, dur: Duration) -> f32` — clamped, smoothstep-eased progress in `[0, 1]`. `dur == 0` → returns `1.0` (instantly complete).

- [ ] **Step 1: Write failing tests**

Add to the existing `#[cfg(test)] mod tests` in `render_thread.rs` (the small module near line 463, NOT the gpu `render_tests`):

```rust
    #[test]
    fn crossfade_t_endpoints_and_midpoint() {
        use std::time::Duration;
        let dur = Duration::from_millis(200);
        assert_eq!(super::crossfade_t(Duration::ZERO, dur), 0.0);
        assert_eq!(super::crossfade_t(Duration::from_millis(200), dur), 1.0);
        assert_eq!(super::crossfade_t(Duration::from_millis(400), dur), 1.0); // clamped past end
        // smoothstep(0.5) == 0.5
        let mid = super::crossfade_t(Duration::from_millis(100), dur);
        assert!((mid - 0.5).abs() < 1e-6, "mid was {mid}");
        // zero duration completes instantly
        assert_eq!(super::crossfade_t(Duration::ZERO, Duration::ZERO), 1.0);
    }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p carapace-ffi crossfade_t_endpoints_and_midpoint`
Expected: FAIL — `crossfade_t` not defined.

- [ ] **Step 3: Implement**

Add near `frame_interval` (around line 341) in `render_thread.rs`:

```rust
/// Eased crossfade progress in `[0, 1]`: linear ratio `elapsed/dur`, clamped, then smoothstep for a
/// natural dissolve. A zero duration completes instantly (`1.0`), so a `duration_ms = 0` skin never
/// wedges the loop in a blend.
fn crossfade_t(elapsed: Duration, dur: Duration) -> f32 {
    if dur.is_zero() {
        return 1.0;
    }
    let x = (elapsed.as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
```

- [ ] **Step 4: Run — verify pass**

Run: `cargo test -p carapace-ffi crossfade_t_endpoints_and_midpoint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-ffi/src/render_thread.rs
git commit -m "feat(ffi): crossfade_t — smoothstep progress helper"
```

---

## Task 4: SwapState scaffold + render_one restructure (no behavior change)

Introduce the swap state and new fields, wire them into `build()`, and route the render path through explicit per-state helpers — while keeping `Idle` behavior **byte-identical** to today. No warming/crossfading logic yet (that is Task 5). This isolates the risky borrow refactor from the new behavior.

**Files:**
- Modify: `crates/carapace-ffi/src/render_thread.rs`

**Interfaces:**
- Consumes: `crate::crossfade::CrossfadeBlender`, `crate::render::{new_offscreen, OffscreenTarget}`, `carapace::engine::Engine`, `carapace::skin::Transition`.
- Produces (private to the render thread):
  - `enum SwapState { Idle, Warming { incoming: Engine, transition: Transition }, Crossfading { outgoing: Engine, elapsed: Duration, dur: Duration } }` (no `incoming_canvas` field — `cw/ch` auto-refreshes from `self.engine`, which becomes the incoming skin on entering `Crossfading`).
  - `RenderThread` fields: `swap: SwapState`, `tex_a: OffscreenTarget`, `tex_b: OffscreenTarget`, `blender: CrossfadeBlender`.
  - `RenderThread::render_one(dt)` dispatches on `self.swap`; `Idle` path unchanged.

- [ ] **Step 1: Add imports, fields, and the `SwapState` enum**

At the top of `render_thread.rs` imports (near line 21), extend the `render` import and add crossfade + skin:

```rust
use crate::render::{
    GpuCtx, IOSurfaceRef, OffscreenTarget, Present, Tier, build_content, build_present, init_gpu,
    new_offscreen,
};
use crate::crossfade::CrossfadeBlender;
use carapace::skin::Transition;
```

Add the enum above `struct RenderThread` (around line 53):

```rust
/// The render thread's live skin-swap phase. `Idle` is the normal single-skin path. `Warming` holds
/// a freshly built incoming engine that has not yet been warmed (asset decode/upload happens on its
/// first offscreen render). `Crossfading` holds the *outgoing* engine while `self.engine` is already
/// the incoming skin; the two are blended by `elapsed/dur` progress.
enum SwapState {
    Idle,
    Warming {
        incoming: Engine,
        transition: Transition,
    },
    Crossfading {
        outgoing: Engine,
        elapsed: Duration,
        dur: Duration,
    },
}
```

Add fields to `RenderThread` (after `force_panic`, keeping the `#[cfg(test)]` field last is fine — put these before it):

```rust
    /// Live skin-swap phase (Task 4/5). `Idle` when no swap is in flight.
    swap: SwapState,
    /// Scratch offscreen the *incoming* skin renders into during warm/crossfade.
    tex_a: OffscreenTarget,
    /// Scratch offscreen the *outgoing* skin renders into during crossfade.
    tex_b: OffscreenTarget,
    /// The GPU pass that blends `tex_b` (old) and `tex_a` (new) into the present offscreen.
    blender: CrossfadeBlender,
```

- [ ] **Step 2: Initialize the new fields in `build()`**

In `build()` (the `Ok(RenderThread { ... })` literal around line 127), add:

```rust
        swap: SwapState::Idle,
        tex_a: new_offscreen(&gpu.device, w, h),
        tex_b: new_offscreen(&gpu.device, w, h),
        blender: CrossfadeBlender::new(&gpu.device),
```

Ordering caveat: `tex_a`/`tex_b`/`blender` borrow `gpu.device`, and `gpu` is moved into the struct in the same literal — build these values into `let` bindings *before* the struct literal to avoid a move-before-borrow error:

```rust
    let tex_a = new_offscreen(&gpu.device, w, h);
    let tex_b = new_offscreen(&gpu.device, w, h);
    let blender = CrossfadeBlender::new(&gpu.device);
```

then reference `tex_a, tex_b, blender` in the literal.

- [ ] **Step 3: Extract the current present logic into a helper, and split `render_one`**

Add a helper method that performs the blit/readback of a chosen present's offscreen into its surface (this is the tail half of today's `render_one`, verbatim in effect):

```rust
    /// Present offscreen `presents[idx].off` into pooled `surfaces[idx]` (Tier-2 blit / Tier-1
    /// readback). Assumes the offscreen already holds this frame's pixels.
    fn present_offscreen(&self, idx: usize) {
        match &self.presents[idx] {
            Present::Shared { off, iosurface_view, blitter, .. } => {
                crate::render::blit(&self.gpu, blitter, &off.view, iosurface_view);
            }
            Present::Readback { off } => {
                let rgba = crate::render::readback_rgba(&self.gpu, &off.tex, self.w, self.h);
                unsafe { crate::render::copy_into_iosurface(self.surfaces[idx], &rgba, self.w, self.h) };
            }
        }
    }
```

Now rewrite `render_one` so the head (pick surface + upload content) and tail (bookkeeping) are shared, and the middle is a single call to a per-frame render helper. In this task the helper is always the single-skin path (`swap` is never set to anything but `Idle` yet); Task 5 turns this one call into a `match` on `self.swap`. Replace the body from the destructure (line ~189) through the `frame_id += 1` bookkeeping with:

```rust
        // (head unchanged: pick_free_surface + upload host content — keep lines 168..184)

        // swap is always Idle in this task; Task 5 replaces this call with a match on self.swap.
        let scene = self.render_single_into_present(idx, dt);

        self.held[idx] = true;
        self.next_surface = (idx + 1) % self.surfaces.len();
        self.frame_id += 1;
        let (cw, ch) = self.engine.scene().canvas;
        self.cw = cw;
        self.ch = ch;
        // (tail unchanged: return Some((scene, idx as u32, self.frame_id)) etc.)
```

Note: the destructure that previously lived inline in `render_one` now moves **into** `render_single_into_present` (below). The `let RenderThread { .. } = self;` split-borrow pattern only needs to exist inside that helper.

And add `render_single_into_present` holding today's Idle draw (the destructure + per-tier `render_frame` + blit/readback), now factored to render `self.engine` into `presents[idx].off` and present it:

```rust
    /// Render the current `self.engine` into `presents[idx].off` and present it. This is the
    /// unchanged single-skin path (former inline body of `render_one`).
    fn render_single_into_present(&mut self, idx: usize, dt: Duration) -> Scene {
        let RenderThread { engine, renderer, gpu, presents, surfaces, content, w, h, .. } = self;
        let (w, h) = (*w, *h);
        let host_view = content.as_ref().map(|c| ("host", &c.view));
        match &presents[idx] {
            Present::Shared { off, iosurface_view, blitter, .. } => {
                let scene = crate::render::render_frame(engine, renderer, gpu, &off.view, w, h, dt, false, host_view);
                crate::render::blit(gpu, blitter, &off.view, iosurface_view);
                scene
            }
            Present::Readback { off } => {
                let scene = crate::render::render_frame(engine, renderer, gpu, &off.view, w, h, dt, true, host_view);
                let rgba = crate::render::readback_rgba(gpu, &off.tex, w, h);
                unsafe { crate::render::copy_into_iosurface(surfaces[idx], &rgba, w, h) };
                scene
            }
        }
    }
```

Note: `present_offscreen` (Step 3a) is used by Task 5's crossfade path; `render_single_into_present` keeps the Idle path self-contained (renders AND presents in one, exactly as today). Both exist after this task; `present_offscreen` may be `#[allow(dead_code)]` until Task 5 wires it — add that attribute to avoid a warnings-gate failure, and remove it in Task 5.

- [ ] **Step 4: Build + run the full existing suite — verify no behavior change**

```bash
cargo build -p carapace-ffi
cargo test -p carapace-ffi
cargo test -p carapace-ffi --features gpu-tests
cargo clippy -p carapace-ffi --all-targets -- -D warnings
```
Expected: all existing tests PASS (the render path is unchanged); clippy clean. This task adds no new test — it is a structure-preserving refactor guarded by the existing suite.

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-ffi/src/render_thread.rs
git commit -m "refactor(ffi): SwapState scaffold + render_one dispatch (no behavior change)"
```

---

## Task 5: Warming + Crossfading behavior (FFI crate)

Implement the state machine: `SwapSkin` builds the incoming engine into `Warming`; the next render warms it and enters `Crossfading` (or promotes on `Cut`); crossfade frames blend both engines and complete at `t ≥ 1`. Keep the loop ticking while a swap is in flight.

**Files:**
- Modify: `crates/carapace-ffi/src/render_thread.rs`

**Interfaces:**
- Consumes: `crossfade_t` (Task 3), `CrossfadeBlender::draw` (Task 2), `SwapState` (Task 4), `carapace::vocab::VocabRegistry`, `crate::host::FfiHost`.
- Behavior: pointer + hit-test canvas (`cw/ch`) switch to the incoming skin at crossfade start; input during crossfade routes to `self.engine` (incoming); the loop renders every frame while `!matches!(self.swap, SwapState::Idle)` even when `fps == 0`.

- [ ] **Step 1a: Create the `cut` fixture skin**

The crossfade test uses the default (swapping to `minimal` triggers the default 250 ms crossfade — no fixture needed). The `cut` test needs a skin that declares `kind = "cut"`. Create it by copying an existing minimal base-vocab skin and adding a transition table:

```bash
cp -R crates/carapace-demo/skins/minimal crates/carapace-ffi/tests/skins/cut
printf '\n[transition]\nkind = "cut"\n' >> crates/carapace-ffi/tests/skins/cut/skin.toml
```

Confirm `crates/carapace-ffi/tests/skins/cut/skin.toml` still has `schema = 1`, its `canvas`, and `entry`, plus the appended `[transition]` table. (The `minimal` skin loads under `VocabRegistry::base()`, which the test harness uses.)

- [ ] **Step 1b: Write the failing gpu-tests**

These live in the **`pacing_tests`** module (`render_thread.rs`, ~line 616) — it already has the per-test leaked counter, the `count_ready` callback, the `make(counter)` handle builder (pool size 3, `classic` skin), and the surface-release pattern. Add both tests there. They assert on **`frame_ready` counts**, not pixels: a crossfade auto-advances while paused (a burst of frames from the old skin still animating + the blend), while a `cut` promotes in a single warming frame.

The `dt` per frame is clamped to `frame_interval(0)*4 ≈ 66 ms`, so a 250 ms crossfade always spans **≥ 3** frames even if the first (vello-compile) frame stalls — hence the `>= 3` lower bound.

```rust
    #[test]
    fn crossfade_auto_advances_while_paused() {
        let count: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        let h = make(count); // default fps
        unsafe { assert_eq!(crate::handle::carapace_set_frame_rate(h, 0), crate::guard::CarapaceStatus::Ok) };
        // Let any startup frames settle, then zero the baseline so we count only swap-driven frames.
        std::thread::sleep(std::time::Duration::from_millis(80));
        for i in 0..3 { unsafe { let _ = crate::handle::carapace_release_surface(h, i); } }
        std::thread::sleep(std::time::Duration::from_millis(40));
        count.store(0, Ordering::SeqCst);

        // Swap classic -> minimal: absent [transition] → default crossfade (250 ms).
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../carapace-demo/skins/minimal")).unwrap();
        unsafe { assert_eq!(crate::handle::carapace_swap_skin(h, dir.as_ptr()), crate::guard::CarapaceStatus::Ok) };

        // Release surfaces continuously so the auto-advancing crossfade never backpressures.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline && count.load(Ordering::SeqCst) < 3 {
            for i in 0..3 { unsafe { let _ = crate::handle::carapace_release_surface(h, i); } }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        let n = count.load(Ordering::SeqCst);
        assert!(n >= 3, "crossfade should auto-produce frames while paused (old skin keeps animating), got {n}");
        unsafe { crate::handle::carapace_destroy(h) };
    }

    #[test]
    fn cut_swap_promotes_without_crossfade_burst() {
        let count: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        let h = make(count);
        unsafe { assert_eq!(crate::handle::carapace_set_frame_rate(h, 0), crate::guard::CarapaceStatus::Ok) };
        std::thread::sleep(std::time::Duration::from_millis(80));
        for i in 0..3 { unsafe { let _ = crate::handle::carapace_release_surface(h, i); } }
        std::thread::sleep(std::time::Duration::from_millis(40));
        count.store(0, Ordering::SeqCst);

        // Swap to the `cut` fixture: promotes in one warming frame, no crossfade.
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/skins/cut")).unwrap();
        unsafe { assert_eq!(crate::handle::carapace_swap_skin(h, dir.as_ptr()), crate::guard::CarapaceStatus::Ok) };

        // Keep releasing for well past a crossfade's worth of time; a cut must NOT spawn a burst.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < deadline {
            for i in 0..3 { unsafe { let _ = crate::handle::carapace_release_surface(h, i); } }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        let n = count.load(Ordering::SeqCst);
        assert!(n <= 2, "cut swap promotes in one frame — no crossfade burst; got {n}");
        unsafe { crate::handle::carapace_destroy(h) };
    }
```

Note on the existing `swap_skin_applies_and_bad_dir_is_rejected` test (in the `render_tests` module): it asserts only that the surface is **non-blank** after a swap, which stays true throughout a crossfade (the outgoing skin is non-blank at `t≈0`), so it continues to pass **unmodified**. Its bad-dir assertion (`ErrBadSkin`, engine intact) also still holds — a failed `load_dir` returns before `SwapState` is touched. Do not change it.

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p carapace-ffi --features gpu-tests pacing_tests::crossfade pacing_tests::cut_swap`
Expected: both new tests FAIL (warming/crossfading not implemented — a paused swap produces no auto-advancing frames yet).

- [ ] **Step 3: Rewrite the `SwapSkin` command handler**

Replace the `Command::SwapSkin` arm (render_thread.rs:268) with a version that builds the incoming engine and enters `Warming`, keeping synchronous error reporting:

```rust
            Command::SwapSkin { dir, reply } => {
                let status = match carapace::skin::load_dir(&dir) {
                    Ok((manifest, source)) => {
                        match Engine::new(
                            Box::new(FfiHost::new(self.vtable)),
                            carapace::vocab::VocabRegistry::base(),
                            source,
                        ) {
                            Ok(incoming) => {
                                // Last-writer-wins: a swap already in flight is replaced.
                                self.swap = SwapState::Warming {
                                    incoming,
                                    transition: manifest.transition,
                                };
                                *invalidated = true; // drive the warm/blend immediately
                                CarapaceStatus::Ok
                            }
                            Err(e) => {
                                set_last_error(&format!("swap_skin: engine init failed: {e:?}"));
                                CarapaceStatus::ErrBadSkin
                            }
                        }
                    }
                    Err(e) => {
                        set_last_error(&format!("swap_skin: load failed: {e:?}"));
                        CarapaceStatus::ErrBadSkin
                    }
                };
                let _ = reply.send(status);
            }
```

Requires `use carapace::engine::Engine;` (already imported) and `use crate::host::FfiHost;` (already imported). Add `use carapace::vocab` path inline as shown.

- [ ] **Step 4: Implement the Warming + Crossfading render branches**

Replace Task 4's single `let scene = self.render_single_into_present(idx, dt);` line in `render_one` with real dispatch. Because the `Warming`/`Crossfading` branches move engines out of `self.swap`, take the state with `std::mem::replace` (which ends the borrow of `self.swap` before the arm bodies run `&mut self` helpers):

```rust
        let scene = match std::mem::replace(&mut self.swap, SwapState::Idle) {
            SwapState::Idle => self.render_single_into_present(idx, dt),

            SwapState::Warming { mut incoming, transition } => {
                // 1. Old skin keeps presenting this frame.
                let scene = self.render_single_into_present(idx, dt);
                // 2. Warm the incoming engine: one offscreen render forces asset decode + upload.
                self.warm_incoming(&mut incoming, dt);
                // 3. Transition. On entering Crossfading, `self.engine` becomes the incoming skin,
                //    so the render_one tail's `cw/ch = self.engine.scene().canvas` flips hit-testing
                //    to the new skin from the first crossfade frame.
                match transition.kind {
                    carapace::skin::TransitionKind::Cut => {
                        self.engine = incoming; // promote; swap stays Idle (already replaced)
                    }
                    carapace::skin::TransitionKind::Crossfade => {
                        let outgoing = std::mem::replace(&mut self.engine, incoming);
                        self.swap = SwapState::Crossfading {
                            outgoing,
                            elapsed: Duration::ZERO,
                            dur: Duration::from_millis(transition.duration_ms as u64),
                        };
                    }
                }
                scene
            }

            SwapState::Crossfading { mut outgoing, elapsed, dur } => {
                let elapsed = elapsed + dt;
                let t = crossfade_t(elapsed, dur);
                let scene = self.render_crossfade(idx, &mut outgoing, dt, t);
                // t < 1 → stay crossfading (carry the advanced elapsed); t >= 1 → drop `outgoing`,
                // swap is already `Idle` from the mem::replace above.
                if t < 1.0 {
                    self.swap = SwapState::Crossfading { outgoing, elapsed, dur };
                }
                scene
            }
        };
```

Add the two rendering helpers:

```rust
    /// Render the incoming engine once into scratch `tex_a` purely to force its lazy asset decode
    /// and GPU texture upload (the cost we hide behind the still-animating old skin). The result is
    /// discarded — the old skin's frame is the one presented this iteration.
    fn warm_incoming(&mut self, incoming: &mut Engine, dt: Duration) {
        let RenderThread { renderer, gpu, tex_a, content, w, h, .. } = self;
        let host_view = content.as_ref().map(|c| ("host", &c.view));
        let _ = crate::render::render_frame(incoming, renderer, gpu, &tex_a.view, *w, *h, dt, false, host_view);
    }

    /// Render `self.engine` (incoming) into `tex_a` and `outgoing` into `tex_b`, blend by `t` into
    /// `presents[idx].off`, then present. Returns the incoming engine's laid-out scene (what the
    /// snapshot publishes — hit-testing already targets the incoming skin).
    fn render_crossfade(&mut self, idx: usize, outgoing: &mut Engine, dt: Duration, t: f32) -> Scene {
        // Render incoming (self.engine) -> tex_a; capture its scene for the snapshot.
        let scene = {
            let RenderThread { engine, renderer, gpu, tex_a, content, w, h, .. } = self;
            let host_view = content.as_ref().map(|c| ("host", &c.view));
            crate::render::render_frame(engine, renderer, gpu, &tex_a.view, *w, *h, dt, false, host_view)
        };
        // Render outgoing -> tex_b.
        {
            let RenderThread { renderer, gpu, tex_b, content, w, h, .. } = self;
            let host_view = content.as_ref().map(|c| ("host", &c.view));
            let _ = crate::render::render_frame(outgoing, renderer, gpu, &tex_b.view, *w, *h, dt, false, host_view);
        }
        // Blend tex_b (old) over/into tex_a (new) into the present offscreen (`off.view` is the same
        // for both tiers), then present it via the shared blit/readback path.
        let off_view = match &self.presents[idx] {
            Present::Shared { off, .. } => &off.view,
            Present::Readback { off } => &off.view,
        };
        self.blender.draw(&self.gpu, &self.tex_b.view, &self.tex_a.view, off_view, t);
        self.present_offscreen(idx);
        scene
    }
```

Remove the `#[allow(dead_code)]` from `present_offscreen` now that it is used.

- [ ] **Step 5: Switch the hit-test canvas to the incoming skin at crossfade start**

`render_one`'s tail already refreshes `cw/ch` from `self.engine.scene().canvas` after each frame. Since `self.engine` becomes the incoming skin the instant we enter `Crossfading` (Step 4), `cw/ch` will reflect the new skin from the first crossfade frame — no extra change needed. Verify this by reading the tail (around line 232); if it reads from a source other than `self.engine`, fix it to read `self.engine.scene().canvas`.

- [ ] **Step 6: Keep the loop ticking while a swap is in flight**

In `run_loop` (render_thread.rs:362), the paused branch (`fps == 0`) blocks on commands. During a crossfade we must render every frame even while paused. Change the wait computation so an in-flight swap paces like `fps > 0`:

```rust
        let animating = rt.fps > 0 || !matches!(rt.swap, SwapState::Idle);
        let wait = if animating {
            frame_interval(rt.fps).saturating_sub(rt.last_render.elapsed())
        } else {
            Duration::from_secs(3600)
        };
```

And in the `Err(RecvTimeoutError::Timeout)` arm, render when `animating` (not only `fps > 0`):

```rust
            Err(RecvTimeoutError::Timeout) => {
                if rt.fps > 0 || !matches!(rt.swap, SwapState::Idle) {
                    render_guarded(rt, cell, poisoned, poison_msg);
                }
            }
```

- [ ] **Step 7: Run the gpu-tests — verify pass**

```bash
cargo test -p carapace-ffi --features gpu-tests pacing_tests:: render_tests::
cargo test -p carapace-ffi
cargo clippy -p carapace-ffi --all-targets --features gpu-tests -- -D warnings
```
Expected: new crossfade/cut tests PASS; existing tests (including the unmodified `swap_skin_applies...`) PASS; clippy clean.

- [ ] **Step 8: Commit**

```bash
git add crates/carapace-ffi/src/render_thread.rs crates/carapace-ffi/tests/skins/
git commit -m "feat(ffi): seamless swap — warm offscreen + skin-authored crossfade"
```

---

## Task 6: ABI minor bump to 3.1 + header regen

**Files:**
- Modify: `crates/carapace-ffi/src/guard.rs`
- Modify: `crates/carapace-ffi/src/lib.rs`
- Modify: `crates/carapace-ffi/include/carapace.h` (regenerated)
- Modify: `crates/carapace-ffi/tests/header.rs` (if it pins the minor)

**Interfaces:**
- Produces: `carapace_abi_version()` returns `3 << 16 | 1`; `CARAPACE_ABI_MINOR == 1`.

- [ ] **Step 1: Update the ABI version test**

In `lib.rs`, replace the `abi_version_is_v3` test:

```rust
    #[test]
    fn abi_version_is_v3_1() {
        assert_eq!(carapace_abi_version(), (3 << 16) | 1);
        assert_eq!(CARAPACE_ABI_MAJOR, 3);
        assert_eq!(CARAPACE_ABI_MINOR, 1);
    }
```

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p carapace-ffi abi_version_is_v3_1`
Expected: FAIL — `CARAPACE_ABI_MINOR` is still 0.

- [ ] **Step 3: Bump the constant**

In `guard.rs:42`: `pub const CARAPACE_ABI_MINOR: u32 = 1;`

- [ ] **Step 4: Run — verify pass**

Run: `cargo test -p carapace-ffi abi_version_is_v3_1`
Expected: PASS.

- [ ] **Step 5: Regenerate the header and check the freshness test**

Regenerate `include/carapace.h` the same way the repo does (the header freshness test in `tests/header.rs` documents the command — typically `cbindgen`). Run:

```bash
cargo test -p carapace-ffi --test header
```
If it fails on staleness, regenerate per its instructions (e.g. `cbindgen --config cbindgen.toml --crate carapace-ffi --output include/carapace.h`) and re-run. Confirm the only diff is the `CARAPACE_ABI_MINOR` value (no symbol changes).

Also update the mirrored header the showcase links: `showcase/Sources/CCarapace/include/carapace.h` — copy the regenerated header so the version constant matches (no signature changes).

- [ ] **Step 6: Commit**

```bash
git add crates/carapace-ffi/src/guard.rs crates/carapace-ffi/src/lib.rs \
        crates/carapace-ffi/include/carapace.h crates/carapace-ffi/tests/header.rs \
        showcase/Sources/CCarapace/include/carapace.h
git commit -m "chore(ffi): bump ABI to 3.1 (seamless-swap capability)"
```

---

## Task 7: Showcase — parse transition duration from the manifest

**Files:**
- Modify: `showcase/Sources/Showcase/SkinManifest.swift`
- Test: `showcase/Tests/ShowcaseTests/SkinManifestTests.swift` (create or extend)

**Interfaces:**
- Produces: `SkinManifest.durationMs(atDir dir: String, fallback: Int) -> Int` and `SkinManifest.parseDurationMs(fromTOML:) -> Int?`.

- [ ] **Step 1: Write the failing test**

Create `showcase/Tests/ShowcaseTests/SkinManifestTests.swift` (or add to the existing showcase test target):

```swift
import XCTest
@testable import Showcase

final class SkinManifestTests: XCTestCase {
    func testParsesDurationMs() {
        let toml = "[transition]\nkind = \"crossfade\"\nduration_ms = 300\n"
        XCTAssertEqual(SkinManifest.parseDurationMs(fromTOML: toml), 300)
    }
    func testMissingDurationIsNil() {
        XCTAssertNil(SkinManifest.parseDurationMs(fromTOML: "canvas = { width = 1, height = 1 }"))
    }
}
```

- [ ] **Step 2: Run — verify fail**

Run: `swift test --package-path showcase --filter SkinManifestTests`
Expected: FAIL — `parseDurationMs` undefined.

- [ ] **Step 3: Implement**

Add to `SkinManifest` in `showcase/Sources/Showcase/SkinManifest.swift`:

```swift
    /// Parse `duration_ms = N` from a skin.toml's `[transition]` table. Nil when absent (caller
    /// falls back to the engine default). Deliberately regex-tiny, matching parseCanvas's style.
    static func parseDurationMs(fromTOML toml: String) -> Int? {
        guard let r = toml.range(of: "duration_ms\\s*=\\s*([0-9]+)", options: .regularExpression) else { return nil }
        let digits = toml[r].drop(while: { !$0.isNumber })
        return Int(digits)
    }

    /// The incoming skin's crossfade duration in ms, or `fallback` (the engine default, 250) when
    /// the skin declares none.
    static func durationMs(atDir dir: String, fallback: Int = 250) -> Int {
        let path = (dir as NSString).appendingPathComponent("skin.toml")
        guard let toml = try? String(contentsOfFile: path, encoding: .utf8),
              let ms = parseDurationMs(fromTOML: toml) else { return fallback }
        return ms
    }
```

- [ ] **Step 4: Run — verify pass**

Run: `swift test --package-path showcase --filter SkinManifestTests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/SkinManifest.swift showcase/Tests/ShowcaseTests/SkinManifestTests.swift
git commit -m "feat(showcase): parse [transition] duration_ms from skin.toml"
```

---

## Task 8: Showcase — live-swap on skin cycle + post-fade window resize

Rewire `cycleSkin` to use the live-swap API (crossfade inside the current window), then animate the window to the new skin's canvas after the fade. This makes the flagship app the living verification of v4.

**Files:**
- Modify: `showcase/Sources/Showcase/App.swift`

**Interfaces:**
- Consumes: `CarapaceBridge.swap(skinDir:) -> Bool` (already exists), `SkinManifest.canvas(atDir:fallback:)`, `SkinManifest.durationMs(atDir:)` (Task 7).

- [ ] **Step 1: Add a live-swap path alongside `applySkin`**

Add a new method that swaps live and schedules the resize. This replaces the `cycleSkin` → `applySkin` call for the *swap* case (first-time setup still uses `applySkin`):

```swift
    /// Live-swap to `dir`: the engine crossfades the incoming skin in over its declared duration
    /// while the old skin keeps animating (no teardown). After the fade, animate the borderless
    /// window to the new skin's canvas size (the fixed IOSurface scales during the brief settle).
    private func swapSkin(dir: String) {
        guard let b = bridge, b.swap(skinDir: dir) else {
            // Fallback: if the live swap is rejected, fall back to the full rebuild.
            applySkin(dir: dir)
            return
        }
        positionTrafficLights(forDir: dir)  // re-place chrome for the incoming skin
        let ms = SkinManifest.durationMs(atDir: dir)
        let (w, h) = SkinManifest.canvas(atDir: dir, fallback: (420, 660))
        // Resize AFTER the crossfade completes so the seamless dissolve isn't disturbed.
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(ms)) { [weak self] in
            guard let self, let window = self.view.window else { return }
            let topY = window.frame.origin.y + window.frame.height
            var frame = window.frame
            frame.size = NSSize(width: w, height: h)
            frame.origin.y = topY - h
            window.animator().setFrame(frame, display: true) // top-left-anchored resize
            self.view.canvasW = Double(w)
            self.view.canvasH = Double(h)
        }
    }
```

Adjust the exact window/view field names to match `App.swift` (it uses `window`, `view.canvasW/H`, `positionTrafficLights(forDir:)` — all present in the current file). If `view.window` isn't the borderless window reference used elsewhere, use the same `window` property `applySkin` uses.

- [ ] **Step 2: Point `cycleSkin` at the live-swap path**

Change `cycleSkin` (App.swift:159):

```swift
    private func cycleSkin() {
        skinIndex = (skinIndex + 1) % skinDirs.count
        swapSkin(dir: skinDirs[skinIndex]) // live crossfade; window settles to new size after the fade
    }
```

Leave the initial `applySkin(dir:)` call in setup (App.swift:44) unchanged — the first skin still builds the bridge normally.

- [ ] **Step 3: Build the showcase**

Run: `swift build --package-path showcase`
Expected: builds clean.

Known consideration (document, do not block): the dither content surface is provisioned at bridge-create for the *starting* skin only. Swapping live into Studio (which declares a `view{ id="host" }` cutout) keeps the original bridge's content surface, so the cutout shows whatever surface exists (or empty) rather than a freshly-sized dither. Pixel-exact per-skin content-surface refit is deferred (spec: "Pool re-fit at the exact new size is deferred"). This does not affect the crossfade seamlessness being verified.

- [ ] **Step 4: Manual verification (the real judgment)**

Run the showcase (via the repo's run path / `swift run --package-path showcase` or the existing launch skill). Press Tab to cycle skins. Confirm:
- The old skin keeps animating during the swap — no freeze.
- The new skin dissolves in smoothly (no hard pop).
- The window settles to the new skin's size after the fade.
- `MusicHost` playback/state persists across the swap.

Capture a short screen recording if the run tooling supports it, for the PR.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/App.swift
git commit -m "feat(showcase): live-swap skin cycling with post-fade window resize"
```

---

## Task 9: Documentation

**Files:**
- Modify: `crates/carapace-ffi/README.md`
- Modify: `docs/api/` (the mdBook guide + any rustdoc landing that lists swap behavior)

**Interfaces:** none (docs only).

- [ ] **Step 1: Update the FFI README**

Document that `carapace_swap_skin` is seamless as of ABI 3.1: the old skin keeps animating during load, and the incoming skin dissolves in per its `[transition]` table (default crossfade 250 ms; `kind = "cut"` for an instant, still-stall-free swap). Note the C ABI is unchanged.

- [ ] **Step 2: Update the Carapace API docs**

In `docs/api/`, add the `[transition]` table to the skin-manifest reference (fields: `kind = "cut" | "crossfade"`, `duration_ms`, defaults, 5000 ms clamp) and a short "seamless swap" note in the FFI/embedding guide. Keep the crate READMEs concise per the repo convention (full how-to lives in the centralized API docs).

- [ ] **Step 3: Verify docs build (if the repo builds them)**

Run the docs build the repo uses (e.g. `mdbook build docs/api` and/or `cargo doc -p carapace-ffi --no-deps`). Expected: builds clean, `#![deny(missing_docs)]` satisfied (all new pub items already carry `///`).

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-ffi/README.md docs/api/
git commit -m "docs(ffi): document seamless swap + [transition] manifest table"
```

---

## Final verification (before opening the PR)

Run the full gate the CI enforces:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test -p carapace-ffi --features gpu-tests
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p carapace-ffi --all-targets --features gpu-tests -- -D warnings
swift build --package-path showcase && swift test --package-path showcase
```

Expected: all green. Then open a PR from `carapace-ffi-v4-seamless-swap` into `main` (no direct push to main; no Claude attribution in the PR description).

## Spec coverage check

- Skin-authored `[transition]` (Component 1) → Task 1.
- Swap state machine `Idle/Warming/Crossfading` (Component 2) → Tasks 4, 5.
- Crossfade blend pass, no engine diff (Component 3) → Task 2.
- Showcase live-swap + resize-after-fade (Component 4) → Tasks 7, 8.
- ABI unchanged, MINOR 3.0 → 3.1 → Task 6.
- Default crossfade 250 for existing skins; `cut` opt-in → Tasks 1, 5.
- Inline warm, no worker thread → Task 5 (`warm_incoming`).
- Pointer/hit-test canvas flips at crossfade start → Task 5, Step 5.
- Loop keeps ticking while paused during a swap → Task 5, Step 6.
- Determinism via accumulated `dt` (not injected `Instant`) → Task 3 + Task 5.
- README/API docs current per phase → Task 9.

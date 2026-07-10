# Engine `shader{}` Primitive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a first-class `shader{}` primitive to the carapace engine: a skin declares a rect the engine fills each frame by running author-supplied WGSL, driven by `time`/`resolution` + literal + live host-data-bound `f32` uniforms.

**Architecture:** A `shader{}` renders as a **background layer** under the 2D UI. When a scene contains `Node::Shader`, `Renderer::draw` switches from its 2-stage pipeline (vello→target, then view-composite) to a **4-stage** one: (1) run each shader's WGSL into the target, (2) render vello 2D into a transparent offscreen, (3) composite that offscreen over the shader background, (4) view-composite host content on top. Scenes with no `shader{}` keep the current fast path unchanged. WGSL is validated with `naga` at skin load (`ErrBadSkin` on error, no GPU needed); pipelines are compiled lazily in the renderer.

**Tech Stack:** Rust, wgpu + vello (engine core `crates/carapace`), naga (WGSL front-end validation), mlua (skin scripting). Cross-platform (engine core is not Apple-gated); GPU tests behind the `gpu-tests` feature.

**Spec:** `docs/superpowers/specs/2026-07-09-engine-shader-primitive-design.md`

## Global Constraints

- **Engine core only.** All work is in `crates/carapace` (+ its call sites). No `carapace-ffi` Apple gating — the engine core is cross-platform.
- **Background-layer only (v1).** `shader{}` renders under the 2D UI. No foreground shaders.
- **Perf is first-class.** The 4-stage path runs **only** when a scene has ≥1 `Node::Shader`. A scene with no shader nodes MUST render byte-identically to today (2-stage path untouched). Task 1 gates on measured cost.
- **Uniforms are `f32` only (v1).** Standard `time` (f32) + `resolution` (vec2<f32>) always present; user uniforms are scalars. Literal (number) or host-bound (string key resolved each frame). Host-bound scalars read the **raw** `StateValue::Scalar` — NOT `value_of` (which clamps to 0..1; wrong for temp/season/etc.).
- **WGSL validated at load.** `naga::front::wgsl::parse_str` on the combined (prelude + author) source at skin load → `BuildError`/`ErrBadSkin` with the naga message. Pipeline creation is lazy in the renderer (pre-validated).
- **New dep fetch via sfw.** Adding `naga` (if not already a direct dep) uses `sfw cargo add naga -p carapace` (Socket Firewall on first 3rd-party fetch).
- **Local gate before every push:** `cargo fmt --all`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo test -p carapace --features gpu-tests` (real adapter — this Mac has one).
- **Git identity:** Daniel Agbemava <danagbemava@gmail.com>. Branch `engine-shader-primitive` (off `origin/main`); never commit to `main`. No Claude attribution in commit/PR bodies.

## File Structure

- **New:** `crates/carapace/src/shader.rs` — `ShaderPrim` (the `Primitive` impl), the WGSL prelude generator, the uniform-binding model (`ShaderUniform { name, source: UniformSource }`), and the std140 layout helper. One responsibility: turn a `shader{}` Lua table into a validated `Node::Shader`.
- **Modify:** `crates/carapace/src/scene.rs` — add `Node::Shader { dest, wgsl, uniforms, key }` variant + `Scene::has_shaders()`/`Scene::shaders()` accessors.
- **Modify:** `crates/carapace/src/vocab.rs` — add `BuildContext::shader_src` method; register `ShaderPrim` in `base()`; bump the count test.
- **Modify:** `crates/carapace/src/script.rs` — implement `BuildContext::shader_src` on `SceneBuilder`; add the method to the `NoHandlers` test double.
- **Modify:** `crates/carapace/src/asset.rs` — `AssetResolver::shader_src(name) -> Result<Arc<str>, AssetError>` (thin wrapper over `bytes`, UTF-8).
- **Modify:** `crates/carapace/src/engine.rs` — accumulate `elapsed: Duration` in `update(dt)`; `elapsed_secs() -> f32`.
- **Modify:** `crates/carapace/src/render.rs` — add `time: f32` to `RenderTarget`; add shader pipeline cache + uniform BGL to `Renderer`; the 4-stage path in `draw`; a raw-scalar uniform resolver.
- **New:** `crates/carapace/src/shader_prelude.wgsl` — the fixed vertex stage + `VsOut` reused by every shader (mirrors `composite.wgsl`'s `vs`).
- **Modify (call sites of `RenderTarget`/`draw`):** `crates/carapace-demo/src/main.rs`, `crates/carapace-ffi/src/render.rs`, `crates/carapace-preview/*`, and `crates/carapace/tests/render_offscreen.rs` — set `time`.
- **Test:** `crates/carapace/tests/render_offscreen.rs` (gpu-tests) + unit tests in `shader.rs`/`engine.rs`.
- **Docs:** `docs/api/skin-authoring.md`.

---

### Task 1: Spike — prove the 4-stage compositing order + measure cost (GO/NO-GO)

De-risk the crux before touching production code: prove vello can render into a **transparent offscreen** and that a shader background composites **under** the vello 2D, via a self-contained gpu-test that builds the pipeline inline with a hardcoded shader. Report the per-frame cost delta.

**Files:**
- Test: `crates/carapace/tests/shader_spike.rs` (new, `#![cfg(feature = "gpu-tests")]`)

**Interfaces:**
- Produces: nothing consumed by later tasks (throwaway spike). Its findings gate the design.

- [ ] **Step 1: Write the spike test**

Create `crates/carapace/tests/shader_spike.rs`. Copy the `Offscreen`/`offscreen`/`readback`/`px` harness from `crates/carapace/tests/render_offscreen.rs:8-105` (verbatim — it's the standard device+readback rig). Then add:

```rust
// Proves: (bg shader) -> target, (vello 2D) -> transparent offscreen, composite offscreen OVER target,
// yields 2D-over-shader-background. Uses a hardcoded solid-color "shader" (a clear) as the stand-in
// background so the test isolates the COMPOSITING ORDER, not shader authoring.
#[test]
fn four_stage_composites_2d_over_shader_background() {
    let (w, h) = (64u32, 64u32);
    let o = offscreen(w, h); // RENDER_ATTACHMENT target, Rgba8Unorm

    // Stage 1: "shader" background — clear the target to solid blue via a render pass.
    {
        let mut enc = o.device.create_command_encoder(&Default::default());
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bg"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &o.view, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 }), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        });
        o.queue.submit(Some(enc.finish()));
    }

    // Stage 2: vello 2D into a TRANSPARENT offscreen (a red fill covering the left half).
    let off2 = offscreen(w, h);
    let mut r = carapace::Renderer::new(&o.device);
    let scene = carapace::scene::Scene {
        canvas: (w, h),
        nodes: vec![carapace::scene::Node::Fill {
            path: vec![
                carapace::scene::Pt { x: 0.0, y: 0.0 }, carapace::scene::Pt { x: (w as f32)/2.0, y: 0.0 },
                carapace::scene::Pt { x: (w as f32)/2.0, y: h as f32 }, carapace::scene::Pt { x: 0.0, y: h as f32 },
            ],
            paint: carapace::scene::Paint::Solid(carapace::scene::Color { r: 255, g: 0, b: 0, a: 255 }),
        }],
    };
    r.draw(&scene, |_| None, |_| None, &carapace::render::RenderTarget {
        device: &o.device, queue: &o.queue, view: &off2.view, width: w, height: h,
        base_color: carapace::scene::Color { r: 0, g: 0, b: 0, a: 0 }, // TRANSPARENT — key to the spike
    });

    // Stage 3: composite off2 OVER the target with premultiplied-alpha blending (reuse composite.wgsl inline,
    // or a trivial textureSample pass). Assert:
    let bg = readback(&o);
    // Right half (2D transparent there) shows the blue background:
    assert_eq!(px(&bg, w, (3*w/4) as usize, (h/2) as usize), [0, 0, 255]);
    // Left half (red 2D) shows red OVER the blue background:
    assert_eq!(px(&off2_composited_over_bg /* see note */, w, (w/4) as usize, (h/2) as usize), [255, 0, 0]);

    // Perf: time 100 iterations of (vello->offscreen + composite) and print the mean ms.
    // eprintln!("4-stage add'l per-frame: {:.3} ms", mean_ms);
}
```

(The spike proves the vello→transparent-offscreen render and that compositing it over a pre-filled target yields 2D-over-background. Wire the Stage-3 composite with an inline `textureSample` pass sampling `off2` into `o.view` with `LoadOp::Load` + premultiplied blend — copy `composite.wgsl` (`crates/carapace/src/composite.wgsl`) as the shader source. Note: `RenderTarget`/`Renderer::new`/`Renderer::draw` are the real engine APIs at `render.rs:22-35,146,257`.)

- [ ] **Step 2: Run the spike; record findings**

Run: `cargo test -p carapace --features gpu-tests --test shader_spike -- --nocapture`
Expected: PASS, and the printed per-frame add'l cost is small (target: well under one 60fps frame budget, ~1-3 ms on this Mac). Write a 5-line `docs/superpowers/specs/2026-07-09-engine-shader-primitive-findings.md`: GO/NO-GO + the measured ms + any surprises (e.g. vello offscreen alpha behavior). **If NO-GO (broken order or unacceptable cost), STOP and report — do not proceed.**

- [ ] **Step 3: Commit**

```bash
cargo fmt --all
git add crates/carapace/tests/shader_spike.rs docs/superpowers/specs/2026-07-09-engine-shader-primitive-findings.md
git commit -m "spike(engine): prove 4-stage shader-background compositing + measure cost"
```

---

### Task 2: Engine-accumulated animation clock + `time` on `RenderTarget`

The engine has no elapsed clock today (`update(dt)` only forwards `dt` to `Host::tick`). Add an engine-owned accumulated time so the shader `time` uniform is engine-provided (per spec) and synced to `update`.

**Files:**
- Modify: `crates/carapace/src/engine.rs` (`Engine::update` ~113-133; add field + `elapsed_secs`)
- Modify: `crates/carapace/src/render.rs` (`RenderTarget` struct ~22-35; add `time: f32`)
- Modify call sites: `crates/carapace-demo/src/main.rs`, `crates/carapace-ffi/src/render.rs`, `crates/carapace-preview/src/*` (every `RenderTarget { .. }` literal), `crates/carapace/tests/render_offscreen.rs`, `crates/carapace/tests/shader_spike.rs`.
- Test: `crates/carapace/src/engine.rs` unit test.

**Interfaces:**
- Produces: `Engine::elapsed_secs(&self) -> f32`; `RenderTarget.time: f32`.

- [ ] **Step 1: Write the failing test**

Add to `engine.rs` tests:
```rust
#[test]
fn update_accumulates_elapsed_time() {
    let mut e = /* build a minimal Engine as other engine.rs tests do — copy their setup */;
    assert_eq!(e.elapsed_secs(), 0.0);
    e.update(std::time::Duration::from_millis(500));
    e.update(std::time::Duration::from_millis(250));
    assert!((e.elapsed_secs() - 0.75).abs() < 1e-6);
}
```
(Read the existing `engine.rs` test module for how a test `Engine` is constructed and copy that setup.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace update_accumulates_elapsed_time`
Expected: FAIL to compile (`elapsed_secs` undefined).

- [ ] **Step 3: Implement**

In `engine.rs`: add `elapsed: std::time::Duration` to the `Engine` struct (init `Duration::ZERO` in every constructor — `Engine::new` and any `swap`/rebuild path). In `update`, before/after the existing body add `self.elapsed += dt;`. Add:
```rust
/// Seconds since this engine was created, accumulated from every `update(dt)` — the clock a
/// `shader{}`'s `time` uniform animates on, in lockstep with the rest of the scene.
pub fn elapsed_secs(&self) -> f32 { self.elapsed.as_secs_f32() }
```

In `render.rs`, add `pub time: f32,` to `RenderTarget`. Update the doc comment.

- [ ] **Step 4: Fix all `RenderTarget { .. }` call sites**

Add `time: <value>` to every `RenderTarget` literal:
- Real render loops (`carapace-demo/src/main.rs`, `carapace-ffi/src/render.rs`, `carapace-preview`): `time: engine.elapsed_secs()` (thread the engine ref; these already hold `&engine`).
- Tests (`render_offscreen.rs`, `shader_spike.rs`): `time: 0.0`.

Run `cargo build --workspace` and fix each compile error (the compiler enumerates every missing-field site).

- [ ] **Step 5: Run tests**

Run: `cargo test -p carapace update_accumulates_elapsed_time` → PASS. Then `cargo test --workspace` → all pass (signature change fixed everywhere).

- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(engine): engine-accumulated elapsed clock + time on RenderTarget"
```

---

### Task 3: `Node::Shader` + `ShaderPrim` — parse, prelude-gen, naga-validate at load

Add the scene node, the primitive that parses `shader{ src, x,y,w,h, uniforms }`, the WGSL prelude generator, and naga validation — all GPU-free (unit-testable without an adapter).

**Files:**
- New: `crates/carapace/src/shader.rs`
- New: `crates/carapace/src/shader_prelude.wgsl`
- Modify: `crates/carapace/src/scene.rs` (add `Node::Shader`, `Scene::has_shaders`)
- Modify: `crates/carapace/src/asset.rs` (add `shader_src`)
- Modify: `crates/carapace/src/vocab.rs` (add `BuildContext::shader_src`, register `ShaderPrim`, bump count test)
- Modify: `crates/carapace/src/script.rs` (impl `shader_src` on `SceneBuilder`; add to `NoHandlers`)
- Modify: `crates/carapace/src/lib.rs` (add `mod shader;`)
- Modify: `crates/carapace/Cargo.toml` (add `naga` dep via sfw)

**Interfaces:**
- Consumes: `BuildContext` (vocab.rs:30-45), `AssetResolver::bytes` (asset.rs:90), `Node`/`ImageDest` (scene.rs).
- Produces:
  - `Node::Shader { dest: ImageDest, wgsl: std::sync::Arc<str>, uniforms: Vec<ShaderUniform>, key: u64 }` — `wgsl` is the FULL combined source (prelude + author fragment), pre-validated; `key` is a content hash for the renderer's pipeline cache.
  - `pub struct ShaderUniform { pub name: String, pub source: UniformSource }` and `pub enum UniformSource { Literal(f32), Host(String) }` (in `shader.rs`, re-exported).
  - `Scene::has_shaders(&self) -> bool`.
  - `BuildContext::shader_src(&mut self, name: &str) -> Result<std::sync::Arc<str>, crate::asset::AssetError>`.

- [ ] **Step 1: Write the failing tests**

Add a `#[cfg(test)] mod tests` in `shader.rs`:
```rust
#[test]
fn shader_prim_parses_dest_and_uniforms() {
    // Build a Lua table { src="x.wgsl", x=0,y=0,w=480,h=320, uniforms={ season=2, temp="wx_temp" } }
    // via mlua (copy the table-construction pattern from vocab.rs's primitive tests), a NoHandlers
    // ctx whose shader_src returns a trivial valid fragment, then ShaderPrim.build(&args, &mut ctx).
    let nodes = /* ShaderPrim.build(...) */;
    let Node::Shader { dest, uniforms, .. } = &nodes[0] else { panic!() };
    assert_eq!((dest.x, dest.y, dest.w, dest.h), (0.0, 0.0, 480.0, 320.0));
    // season literal, temp host-bound:
    assert!(uniforms.iter().any(|u| u.name == "season" && matches!(u.source, UniformSource::Literal(2.0))));
    assert!(uniforms.iter().any(|u| u.name == "temp" && matches!(&u.source, UniformSource::Host(k) if k == "wx_temp")));
}

#[test]
fn shader_prim_rejects_malformed_wgsl() {
    // shader_src returns "this is not wgsl {{{". build() must return Err (BuildError::BadType/Lua carrying naga msg).
    let err = /* ShaderPrim.build(...).unwrap_err() */;
    assert!(matches!(err, BuildError::BadType(_) | BuildError::Lua(_)));
}

#[test]
fn generated_prelude_declares_named_uniform_fields() {
    // prelude_for(&["season","temp"]) contains "struct U", "time: f32", "res: vec2<f32>",
    // "season: f32", "temp: f32", and "@group(0) @binding(0) var<uniform> u: U;"
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p carapace shader_prim`
Expected: FAIL to compile (`shader` module / `ShaderPrim` undefined).

- [ ] **Step 3: Add `naga`, the prelude file, and the asset loader**

- `sfw cargo add naga -p carapace` (use the version wgpu already pulls; confirm `cargo tree -p carapace -i naga`).
- Create `crates/carapace/src/shader_prelude.wgsl` = the `VsOut` struct + `vs` fullscreen-quad vertex stage copied verbatim from `crates/carapace/src/composite.wgsl:1-19` (the triangle-strip `vs`, no `src`/`samp` bindings). This is the fixed vertex stage every shader reuses; the author supplies only `fs`.
- In `asset.rs`, add next to `image`:
```rust
/// Load a `.wgsl` source file's UTF-8 text (thin typed wrapper over `bytes`).
pub fn shader_src(&self, name: &str) -> Result<std::sync::Arc<str>, AssetError> {
    let bytes = self.bytes(name)?;
    let s = std::str::from_utf8(&bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
    Ok(std::sync::Arc::from(s))
}
```

- [ ] **Step 4: Add the `Node::Shader` variant + `has_shaders`**

In `scene.rs`, add to `enum Node`:
```rust
/// A GPU shader fill: the engine runs `wgsl` (prelude + author fragment) into `dest` each frame as a
/// BACKGROUND layer (under the 2D UI), driven by `uniforms`. `key` content-addresses the compiled
/// pipeline in the renderer cache.
Shader { dest: ImageDest, wgsl: std::sync::Arc<str>, uniforms: Vec<crate::shader::ShaderUniform>, key: u64 },
```
Add:
```rust
impl Scene {
    /// True if any node is a `shader{}` (selects the renderer's 4-stage compositing path).
    pub fn has_shaders(&self) -> bool { self.nodes.iter().any(|n| matches!(n, Node::Shader { .. })) }
}
```

- [ ] **Step 5: Implement `shader.rs`**

Implement in `crates/carapace/src/shader.rs`:
- `ShaderUniform`/`UniformSource` as above.
- `fn prelude_for(names: &[&str]) -> String` — emits `include_str!("shader_prelude.wgsl")` + a generated `struct U { time: f32, res: vec2<f32>, <name: f32>… }` (named f32 fields for tight std140 packing — do NOT use `array<f32,N>`, whose uniform stride is 16) + `@group(0) @binding(0) var<uniform> u: U;`.
- `ShaderPrim` (`impl Primitive`): `id()="shader"`; `build()`:
  1. `src: String = args.get("src")` (MissingField "src"); `x/y/w/h: f32` (MissingField each).
  2. Parse `uniforms` table (optional) → `Vec<ShaderUniform>`: for each key, a Lua number → `Literal(v as f32)`, a Lua string → `Host(s)`, else `BadType`.
  3. `let author = ctx.shader_src(&src).map_err(BuildError::Asset)?;`
  4. `let full = format!("{}\n{}", prelude_for(&names), author);`
  5. **Validate:** `naga::front::wgsl::parse_str(&full).map_err(|e| BuildError::BadType(Box::leak(format!("shader {src}: {e}").into_boxed_str())))?;` (or add a `BuildError::Shader(String)` variant — see Step 6; leaking is fine only if a `&'static str` is truly required, prefer a `String`-carrying variant).
  6. `key` = a stable hash of `full` (`std::hash` FNV/DefaultHasher).
  7. Return `vec![Node::Shader { dest: ImageDest{x,y,w,h}, wgsl: Arc::from(full.as_str()), uniforms, key }]`.
- Register: in `vocab.rs` `base()` add `r.register(Box::new(crate::shader::ShaderPrim));`. Add `mod shader;` to `lib.rs`.

- [ ] **Step 6: Add `BuildContext::shader_src` + `BuildError` variant + bump count test**

- In `vocab.rs`, add to the `BuildContext` trait: `fn shader_src(&mut self, name: &str) -> Result<std::sync::Arc<str>, crate::asset::AssetError>;`. Add a `BuildError::Shader(String)` variant for validation errors (cleaner than leaking a `&'static str`).
- In `script.rs` `SceneBuilder`, implement it forwarding to `self.assets.shader_src(name)` (mirror the `image`/`font` impls).
- Add the same method to the `NoHandlers` test double (vocab.rs test mod — the map notes 3 occurrences; find each `impl BuildContext for NoHandlers`/similar and add a `shader_src` returning a trivial valid fragment like `Ok(Arc::from("@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return vec4(0.0); }"))` — or a per-test override).
- Bump `vocab.rs:940-942` `base_registry_now_has_nine`: rename to `base_registry_now_has_ten`, assert `== 10`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p carapace shader_prim generated_prelude base_registry_now_has_ten`
Expected: PASS. Then `cargo test --workspace` → all pass.

- [ ] **Step 8: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(engine): shader{} primitive — parse, prelude-gen, naga-validate at load"
```

---

### Task 4: Renderer — compile + render the shader background (4-stage) with standard uniforms

Wire `Node::Shader` into `Renderer::draw`: when `scene.has_shaders()`, run the shader pass into the target, render vello into a transparent offscreen, composite it over, then view-composite. Provide `time` + `resolution` uniforms. Scenes without shaders keep the 2-stage path.

**Files:**
- Modify: `crates/carapace/src/render.rs` (`Renderer` struct ~42-56; `Renderer::new` ~146; `Renderer::draw` ~257-658)
- Test: `crates/carapace/tests/render_offscreen.rs`

**Interfaces:**
- Consumes: `Node::Shader`, `Scene::has_shaders`, `RenderTarget.time` (Task 2/3).
- Produces: internal renderer state only.

- [ ] **Step 1: Write the failing gpu-test**

Add to `render_offscreen.rs`:
```rust
#[test]
fn shader_background_renders_under_2d() {
    let (w, h) = (64u32, 64u32);
    let o = offscreen(w, h);
    let mut r = carapace::Renderer::new(&o.device);
    // A trivial shader that outputs solid green everywhere, as a full-canvas background.
    let frag = "@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return vec4(0.0, 1.0, 0.0, 1.0); }";
    let full = format!("{}\n{}", carapace::shader::prelude_for(&[]), frag);
    let key = /* same hash used in ShaderPrim */;
    let scene = carapace::scene::Scene { canvas: (w, h), nodes: vec![
        carapace::scene::Node::Shader { dest: carapace::scene::ImageDest { x:0.0,y:0.0,w:w as f32,h:h as f32 },
            wgsl: std::sync::Arc::from(full.as_str()), uniforms: vec![], key },
        carapace::scene::Node::Fill { /* red fill, left half, as in Task 1 */ },
    ]};
    r.draw(&scene, |_| None, |_| None, &carapace::render::RenderTarget {
        device:&o.device, queue:&o.queue, view:&o.view, width:w, height:h, time:0.0,
        base_color: carapace::scene::Color{r:0,g:0,b:0,a:0} });
    let d = readback(&o);
    assert_eq!(px(&d, w, (3*w/4) as usize, (h/2) as usize), [0,255,0]); // bg shader shows (right half)
    assert_eq!(px(&d, w, (w/4) as usize, (h/2) as usize), [255,0,0]);   // 2D fill OVER shader (left half)
}

#[test]
fn no_shader_scene_uses_2stage_path_unchanged() {
    // A scene with only a Fill renders identically whether or not the shader path exists — assert the
    // known sentinel pixels from the existing renders_fill_and_value_fill test still hold.
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen shader_background_renders_under_2d`
Expected: FAIL — `Node::Shader` is unhandled (renders nothing / wrong order).

- [ ] **Step 3: Implement the 4-stage path**

In `render.rs`:
- Add to `Renderer`: `shader_pipelines: std::collections::HashMap<u64, wgpu::RenderPipeline>`, `shader_bgl: wgpu::BindGroupLayout`, `shader_prelude_vs: wgpu::ShaderModule` is NOT needed (vs is in each shader's module since the prelude is prepended). Build `shader_bgl` in `Renderer::new` (one uniform buffer at `@group(0) @binding(0)`, `ShaderStages::FRAGMENT | VERTEX`).
- In `draw`, after computing `sx,sy`: `let has_shaders = scene.has_shaders();`.
- **Vello target:** if `has_shaders`, render vello into a **transient offscreen** texture (created per-draw or cached by size) with `base_color` = transparent; else render into `target.view` as today.
- **Before vello (has_shaders only):** for each `Node::Shader`, look up/create its pipeline in `shader_pipelines` keyed by `key` (create with `device.create_shader_module(Wgsl(node.wgsl))`, the `shader_bgl` layout, `TriangleStrip`, target format `Rgba8Unorm`, no blend for the background/opaque bg — or `PREMULTIPLIED_ALPHA_BLENDING` if shaders may be translucent; v1: opaque, blend `None`). Write the uniform buffer (`time`, `resolution=(dest.w*sx, dest.h*sy)`, then user uniforms — Task 5 fills these; for now just `time`+`resolution`). Begin a render pass into `target.view` with `LoadOp::Clear`(transparent) for the FIRST shader / `LoadOp::Load` after, `set_viewport(dest*scale)`, `set_bind_group`, `draw(0..4)`.
- **After vello:** if `has_shaders`, composite the vello offscreen over `target.view` (reuse the `composite_pipeline` + a bind group sampling the vello offscreen, full-target viewport, `LoadOp::Load`, premultiplied blend). Then run the existing view-composite pass (unchanged).
- If `!has_shaders`: the existing 2-stage path runs verbatim (vello→target, view-composite). Guard the new code with `if has_shaders`.

(Model every wgpu pass on the existing view-composite pass at `render.rs:586-658`: encoder, `begin_render_pass` with `LoadOp::Load`, `set_pipeline`, `set_viewport`, `set_bind_group`, `draw(0..4)`.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS (new tests + all existing render tests, incl. the transparent-base-color test, still green).

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings
git add -A
git commit -m "feat(engine): render shader{} as a background layer (4-stage composite)"
```

---

### Task 5: Reactive uniforms — literals + host-bound raw scalars

Feed the user uniforms into the shader buffer each frame: literals baked once, host-bound keys read as **raw** scalars (not clamped). Prove reactivity.

**Files:**
- Modify: `crates/carapace/src/render.rs` (shader-pass uniform write; add a raw-scalar resolver)
- Test: `crates/carapace/tests/render_offscreen.rs`

**Interfaces:**
- Consumes: `ShaderUniform`/`UniformSource` (Task 3), `read_value: impl Fn(&str) -> Option<StateValue>` (existing `draw` param).

- [ ] **Step 1: Write the failing gpu-test**

```rust
#[test]
fn shader_uniform_is_reactive() {
    // Fragment outputs vec4(u.intensity, 0, 0, 1). Same shader, two draws:
    //   uniforms=[ShaderUniform{name:"intensity", source: Host("wx")}], read = |k| (k=="wx").then_some(Scalar(0.0)) → red≈0
    //   read = |k| (k=="wx").then_some(Scalar(1.0)) → red≈255
    // Assert the center pixel's red channel differs (≈0 vs ≈255).
}
```
(The prelude must declare `intensity: f32` — pass `&["intensity"]` to `prelude_for` when building the test `wgsl`.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen shader_uniform_is_reactive`
Expected: FAIL — user uniforms aren't written yet (both draws identical).

- [ ] **Step 3: Implement the uniform write**

In `render.rs`, add a raw-scalar resolver (NOT `value_of`, which clamps 0..1):
```rust
fn scalar_of(read: &impl Fn(&str) -> Option<StateValue>, key: &str) -> f32 {
    match read(key) { Some(StateValue::Scalar(v)) => v, Some(StateValue::Bool(b)) => b as i32 as f32, _ => 0.0 }
}
```
In the shader pass, build the uniform buffer bytes in the prelude's field order: `time` (f32), pad to 8, `resolution` (vec2), then each `ShaderUniform` as f32 — `Literal(v) => v`, `Host(k) => scalar_of(&read_value, k)` — packed tightly (4 bytes each) after the 16-byte header, tail-padded to a multiple of 16. Write with `queue.write_buffer` each frame before the draw.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen shader_uniform_is_reactive`
Expected: PASS. Then the full `--features gpu-tests` suite → all pass.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings
git add -A
git commit -m "feat(engine): reactive shader uniforms (literals + host-bound raw scalars)"
```

---

### Task 6: Docs — `shader{}` in skin-authoring

**Files:**
- Modify: `docs/api/skin-authoring.md` (Primitives section, near `### view{}` ~line 115)

- [ ] **Step 1: Document the primitive**

Add a `### shader{}` section: the Lua interface (`src`, `x/y/w/h`, `uniforms`); the fragment-only author contract (engine provides `vs` + `VsOut` with `uv`, and generates the `u` uniform struct — author writes `@fragment fn fs(in: VsOut) -> @location(0) vec4<f32>` referencing `u.time`, `u.res`, and each declared uniform); the standard + literal + host-bound uniform model (`f32`-only, host string keys resolved each frame, raw scalar); **background-layer** semantics (renders under the 2D UI); WGSL validated at load (`ErrBadSkin` on error); and the safety note (arbitrary WGSL on the GPU, not sandboxed — trusted local skins only). Mirror the doc's existing voice + `// source-pointer` style.

- [ ] **Step 2: Build the book + commit**

Run: `mdbook build docs/api` (locate `book.toml` first). Expected: clean.
```bash
git add docs/api
git commit -m "docs(api): document the shader{} primitive"
```

---

### Task 7: Full gate + push + PR

- [ ] **Step 1: Full local gate**

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p carapace --features gpu-tests
```
Expected: all green.

- [ ] **Step 2: Push + draft PR**

```bash
git push -u origin engine-shader-primitive
gh pr create --draft --base main --head engine-shader-primitive \
  --title "feat(engine): shader{} primitive — WGSL background layer with reactive uniforms" \
  --body "Implements docs/superpowers/specs/2026-07-09-engine-shader-primitive-design.md (sub-project 1 of the weather-app showcase). Adds a first-class shader{} primitive: WGSL background under the 2D UI, engine time/resolution + literal + host-bound f32 uniforms, naga-validated at load, 4-stage compositing only when shaders are present."
```

---

## Self-Review

**Spec coverage:**
- 4-stage compositing (shader bg → vello offscreen → composite → view) → Task 4. ✓
- Spike-first, GO/NO-GO, perf measured → Task 1. ✓
- Background-layer-only v1 → Task 4 (guarded by `has_shaders`; non-shader path unchanged). ✓
- `shader{}` Lua interface (src file, x/y/w/h, uniforms) → Task 3. ✓
- Fragment-only contract + engine-generated uniform struct + fixed `vs` → Task 3 (`prelude_for`, `shader_prelude.wgsl`). ✓
- Standard `time` (engine clock) + `resolution` → Task 2 (clock) + Task 4 (resolution). ✓
- Literal + host-bound uniforms, `f32`-only, raw scalar (not clamped) → Task 5. ✓
- Validate at load via naga → `ErrBadSkin` → Task 3. ✓
- Safety limitation stated → Task 6 docs. ✓
- GPU test: non-blank + reacts to a uniform → Tasks 4 + 5. ✓
- Compositing test (2D over shader bg) + non-shader path unchanged → Task 4. ✓
- Cross-platform (engine core, gpu-tests feature) → all engine-core; no Apple gating. ✓
- Docs → Task 6. ✓

**Placeholder scan:** Task 1's spike test leaves the Stage-3 composite/`off2_composited_over_bg` as a "wire it inline copying composite.wgsl" directive (a spike proving a technique, not shipping code — acceptable, and the assertions are concrete). Task 2/3 say "copy the existing test setup / the 3 NoHandlers occurrences" — copy-the-reference directives against exact anchors from the internals map, not vague TODOs. All behavioral code (prelude gen, node variant, 4-stage structure, uniform packing, resolvers, tests) is shown.

**Type consistency:** `Node::Shader { dest, wgsl, uniforms, key }` identical in Tasks 3 & 4; `ShaderUniform`/`UniformSource` identical in Tasks 3 & 5; `prelude_for(&[&str]) -> String` used in Tasks 3, 4, 5; `RenderTarget.time: f32` + `Engine::elapsed_secs()` consistent Tasks 2/4; `scalar_of` (raw, unclamped) distinct from the existing clamped `value_of`. `Renderer::new`/`draw`/`RenderTarget` signatures match the internals map (render.rs:22-35,146,257).

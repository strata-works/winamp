# Paper.design Mesh-Gradient → macOS Skin — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove paper.design's `meshGradientFragmentShader` (plus the shared vertex shader it depends on) can be transpiled GLSL→WGSL and rendered as a correct, flowing *animation* offscreen — retiring the one unproven link before any macOS integration.

**Architecture:** A standalone, deletable `carapace-demo` example. It vendors fully-resolved paper GLSL (fragment + vertex, extracted via a Node import script so template snippets are evaluated), transpiles each stage through a fallback ladder (naga `glsl-in` direct → light preprocess → optional glslang→SPIR-V→naga `spv-in`), links vertex+fragment into a wgpu pipeline with introspected uniforms, and renders a PNG sequence across `u_time`. Zero diff to the `carapace` engine crate.

**Tech Stack:** Rust, wgpu 29, naga 29 (`glsl-in`, `wgsl-out`, `spv-in`), `image` (PNG, already a dev-dep), Node ≥22 + npm (extraction only), Socket Firewall (`sfw`) for all network fetches.

**Scope:** This is **Phase 1** (offscreen transpile + animated render). **Phase 2** (macOS IOSurface + `view{}` compositing in the Swift host) is gated on this and gets its own plan.

## Global Constraints

- **Zero diff to the `carapace` engine crate.** All new code lives under `crates/carapace-demo/examples/paper_mesh_spike/` plus a dev-dependency line in `crates/carapace-demo/Cargo.toml`. If any task appears to need an engine-crate change, STOP and report — that is a spike-scope violation.
- **naga pinned to `29`** — same version wgpu 29 resolves (`Cargo.lock`). The pipeline hands wgpu a **WGSL string**, keeping the spike's naga decoupled from wgpu's internal copy.
- **All network fetches go through `sfw`** (Socket Firewall): `sfw npm install ...`, `sfw cargo add ...`, `sfw gh api ...`, and `sfw bash -c '...'` for compound fetch pipelines.
- **No hand-editing of shader effect logic.** Only mechanical, documented, rule-based preprocessing (ladder rung 2) is allowed. Rewriting a shader body to make it transpile is a failed G1.
- **Git identity:** `Daniel Agbemava <danagbemava@gmail.com>` (`git -c user.name=... -c user.email=... commit`).
- **Working directory:** the worktree at `.claude/worktrees/paper-shader-transpile-spike` (branch `worktree-paper-shader-transpile-spike`). Do not `cd` to the main checkout.

## The target shader (real facts)

`meshGradientFragmentShader` — WebGL2 `#version 300 es`:
- Fragment uniforms: `u_time` (float), `u_colors[10]` (vec4[]), `u_colorsCount` (float), `u_distortion`, `u_swirl`, `u_grainMixer`, `u_grainOverlay` (floats).
- Reads varying `in vec2 v_objectUV;` — produced by paper's shared vertex shader (`vertexShaderSource`), which computes sizing from ~13 of its own uniforms.
- `v_objectUV` is the vertex shader's **first** `out` and the fragment's **only** `in` → both expected at `@location(0)`. The vertex shader's other 6 outputs are unused by the fragment (WGSL permits a vertex to output more than the fragment consumes).
- Interpolates `${declarePI}`, `${rotation2}`, `${proceduralHash21}`, `${maxColorCount}` at JS runtime → must be extracted by evaluating the package.

## The three gates

- **G1 — Transpile:** fragment AND vertex GLSL → valid WGSL, no effect-logic edits; record the rung each stage needed.
- **G2 — Accept + link:** wgpu accepts both WGSL modules and a vertex+fragment render pipeline builds without validation error.
- **G3 — Render:** the linked pipeline renders a flowing mesh-gradient PNG sequence across `u_time` that matches paper's reference and whose frames visibly differ.

## File Structure

```
crates/carapace-demo/
  Cargo.toml                          # MODIFY: add naga dev-dep; add [[example]] entry
  examples/paper_mesh_spike/
    main.rs        # driver: transpile both stages, link, render sequence, print report
    transpile.rs   # the ladder: naga glsl-in, preprocess, optional spv; naga IR introspection
    harness.rs     # offscreen wgpu, vertex+fragment link, uniform introspection+binding, PNG
    extract.mjs    # Node: imports @paper-design/shaders, writes resolved mesh_gradient.frag + vertex.vert
    shaders/       # vendored, fully-resolved GLSL committed for offline reproducibility
      mesh_gradient.frag
      vertex.vert
docs/superpowers/specs/2026-07-02-paper-shader-transpile-spike-findings.md   # CREATE (Task 6)
```

Three focused Rust files: `transpile.rs` (GLSL→WGSL, no GPU), `harness.rs` (GPU, no transpile), `main.rs` (orchestration).

---

## Task 1: Vendor resolved mesh-gradient fragment + vertex GLSL

**Files:**
- Create: `crates/carapace-demo/examples/paper_mesh_spike/extract.mjs`
- Create (generated, committed): `.../shaders/mesh_gradient.frag`, `.../shaders/vertex.vert`

**Interfaces:**
- Produces: two text files. Each is fully-resolved GLSL (`#version 300 es` first line, no `${` markers). Consumed by later tasks via file read.

- [ ] **Step 1: Write the extraction script**

Create `extract.mjs`:
```js
// Run from a scratch dir where `@paper-design/shaders` is installed:
//   sfw npm install @paper-design/shaders
//   node <path>/extract.mjs <path>/shaders
import { writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import * as paper from '@paper-design/shaders';

const outDir = process.argv[2];
mkdirSync(outDir, { recursive: true });

function write(name, src, file) {
  if (typeof src !== 'string') throw new Error(`missing export: ${name}`);
  if (src.includes('${')) throw new Error(`unresolved template in ${name}`);
  writeFileSync(join(outDir, file), src);
  console.log(`wrote ${file} (${src.length} bytes)`);
}

write('meshGradientFragmentShader', paper.meshGradientFragmentShader, 'mesh_gradient.frag');

const vert = paper.vertexShaderSource ?? paper.vertexShader;
if (typeof vert !== 'string') {
  throw new Error('shared vertex shader not exported from package root — ' +
    'mesh-gradient needs v_objectUV; locate the vertex export before proceeding');
}
write('vertexShaderSource', vert, 'vertex.vert');
```
(mesh-gradient *requires* the vertex shader, so a missing vertex export is a hard error here, not a warning.)

- [ ] **Step 2: Install the package (via sfw) and run the script**

Run:
```bash
SPIKE=crates/carapace-demo/examples/paper_mesh_spike
SCRATCH=$(mktemp -d)
( cd "$SCRATCH" && sfw npm install @paper-design/shaders )
node "$SPIKE/extract.mjs" "$SPIKE/shaders" && ls -l "$SPIKE/shaders"
```
Expected: `mesh_gradient.frag` and `vertex.vert` written with non-zero byte counts. (If `import * as paper` fails on the package's export map, resolve the entry with `node -e "console.log(require.resolve('@paper-design/shaders'))"` and import from that path.)

- [ ] **Step 3: Verify both files are resolved GLSL**

Run:
```bash
for f in crates/carapace-demo/examples/paper_mesh_spike/shaders/*; do
  echo "== $f =="; head -1 "$f"; grep -c '\${' "$f" | sed 's/^/  unresolved-markers: /'
done
```
Expected: each first line is `#version 300 es`; each `unresolved-markers` is `0`.

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-demo/examples/paper_mesh_spike/extract.mjs \
        crates/carapace-demo/examples/paper_mesh_spike/shaders/
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(mesh): vendor resolved paper mesh-gradient fragment + vertex GLSL'
```

---

## Task 2: Transpile ladder — rungs 1 & 2 (`transpile.rs`)

**Files:**
- Modify: `crates/carapace-demo/Cargo.toml` (naga dev-dep + example entry)
- Create: `crates/carapace-demo/examples/paper_mesh_spike/transpile.rs`

**Interfaces:**
- Produces (used by `harness.rs` + `main.rs`):
  - `pub enum Rung { Direct, Preprocessed, SpirV, Unavailable }`
  - `pub struct Transpiled { pub wgsl: String, pub rung: Rung, pub module: naga::Module, pub info: naga::valid::ModuleInfo }`
  - `pub fn transpile(glsl: &str, stage: naga::ShaderStage) -> Result<Transpiled, String>` — tries rung 1 then rung 2.
  - `pub fn preprocess(glsl: &str) -> String` — mechanical normalization only.

- [ ] **Step 1: Add the naga dev-dependency and example entry**

Run:
```bash
sfw cargo add --dev --package carapace-demo naga@29 --features glsl-in,wgsl-out,spv-in
```
Add to `crates/carapace-demo/Cargo.toml` (after the existing `[[bin]]`):
```toml
[[example]]
name = "paper_mesh_spike"
path = "examples/paper_mesh_spike/main.rs"
```

- [ ] **Step 2: Write the failing test**

Create `transpile.rs` (and a minimal `main.rs` stub `mod transpile; mod harness; fn main() {}` so the example compiles for tests — `harness.rs` can be an empty stub until Task 4). Add to `transpile.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    const TRIVIAL: &str = "#version 300 es\nprecision mediump float;\n\
        uniform float u_time;\nout vec4 fragColor;\n\
        void main() { fragColor = vec4(abs(sin(u_time)), 0.0, 0.0, 1.0); }";
    #[test]
    fn trivial_fragment_transpiles_direct() {
        let t = transpile(TRIVIAL, naga::ShaderStage::Fragment).expect("should transpile");
        assert!(matches!(t.rung, Rung::Direct));
        assert!(t.wgsl.contains("@fragment"), "wgsl:\n{}", t.wgsl);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p carapace-demo --example paper_mesh_spike trivial_fragment -- --nocapture`
Expected: FAIL to compile (`transpile`/`Rung` not found).

- [ ] **Step 4: Implement rungs 1 & 2**

In `transpile.rs`:
```rust
use naga::back::wgsl;
use naga::front::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

pub enum Rung { Direct, Preprocessed, SpirV, Unavailable }

pub struct Transpiled {
    pub wgsl: String,
    pub rung: Rung,
    pub module: naga::Module,
    pub info: naga::valid::ModuleInfo,
}

/// Mechanical, documented normalization only — never touches effect logic.
pub fn preprocess(glsl: &str) -> String {
    // No-op scaffold; add rule-based rewrites here ONLY as concrete transpile
    // failures demand them, and log each rule in the findings doc.
    glsl.to_string()
}

fn naga_to_wgsl(glsl: &str, stage: naga::ShaderStage)
    -> Result<(naga::Module, naga::valid::ModuleInfo, String), String>
{
    let mut frontend = glsl::Frontend::default();
    let module = frontend
        .parse(&glsl::Options::from(stage), glsl)
        .map_err(|e| format!("glsl parse: {e:?}"))?;
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator.validate(&module).map_err(|e| format!("naga validate: {e:?}"))?;
    let wgsl = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| format!("wgsl out: {e:?}"))?;
    Ok((module, info, wgsl))
}

pub fn transpile(glsl: &str, stage: naga::ShaderStage) -> Result<Transpiled, String> {
    match naga_to_wgsl(glsl, stage) {
        Ok((module, info, wgsl)) => Ok(Transpiled { wgsl, rung: Rung::Direct, module, info }),
        Err(direct_err) => {
            let pre = preprocess(glsl);
            match naga_to_wgsl(&pre, stage) {
                Ok((module, info, wgsl)) =>
                    Ok(Transpiled { wgsl, rung: Rung::Preprocessed, module, info }),
                Err(pre_err) =>
                    Err(format!("direct: {direct_err} | preprocessed: {pre_err}")),
            }
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p carapace-demo --example paper_mesh_spike trivial_fragment -- --nocapture`
Expected: PASS. (If naga 29 symbol names differ, fix against the compiler — the parse→validate→write_string shape is stable.)

- [ ] **Step 6: Commit**

```bash
git add crates/carapace-demo/Cargo.toml crates/carapace-demo/examples/paper_mesh_spike/
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(mesh): naga glsl-in transpile ladder (rungs 1-2) + test'
```

---

## Task 3: Optional SPIR-V fallback rung (`transpile.rs`)

**Files:**
- Modify: `crates/carapace-demo/examples/paper_mesh_spike/transpile.rs`

**Interfaces:**
- Produces: `transpile` attempts rung 3 after rung 2; `pub fn glslang_available() -> bool`. If glslang is absent, rung 3 reports unavailable rather than hard-failing.

- [ ] **Step 1: Write the failing test**

Add to `transpile.rs` tests:
```rust
#[test]
fn spirv_rung_reports_availability() { let _ = glslang_available(); }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-demo --example paper_mesh_spike spirv_rung -- --nocapture`
Expected: FAIL to compile (`glslang_available` not found).

- [ ] **Step 3: Implement rung 3**

Add to `transpile.rs`:
```rust
use std::process::Command;

pub fn glslang_available() -> bool {
    Command::new("glslangValidator").arg("--version").output().is_ok()
}

/// GLSL -> SPIR-V (glslangValidator) -> naga spv-in -> WGSL.
fn via_spirv(glsl: &str, stage: naga::ShaderStage) -> Result<Transpiled, String> {
    if !glslang_available() { return Err("glslangValidator not on PATH".into()); }
    let dir = std::env::temp_dir();
    let ext = match stage {
        naga::ShaderStage::Fragment => "frag",
        naga::ShaderStage::Vertex => "vert",
        _ => "comp",
    };
    let src = dir.join(format!("paper_mesh_in.{ext}"));
    let spv = dir.join("paper_mesh_out.spv");
    std::fs::write(&src, glsl).map_err(|e| e.to_string())?;
    let out = Command::new("glslangValidator")
        .args(["-G", "-o"]).arg(&spv).arg(&src).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("glslang: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let bytes = std::fs::read(&spv).map_err(|e| e.to_string())?;
    let module = naga::front::spv::parse_u8_slice(&bytes, &naga::front::spv::Options::default())
        .map_err(|e| format!("spv-in: {e:?}"))?;
    let mut v = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = v.validate(&module).map_err(|e| format!("validate spv module: {e:?}"))?;
    let wgsl = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| format!("wgsl out (spv): {e:?}"))?;
    Ok(Transpiled { wgsl, rung: Rung::SpirV, module, info })
}
```
Extend `transpile()`'s final `Err` arm to try `via_spirv` before giving up:
```rust
                Err(pre_err) => match via_spirv(glsl, stage) {
                    Ok(t) => Ok(t),
                    Err(spv_err) => Err(format!(
                        "direct: {direct_err} | preprocessed: {pre_err} | spirv: {spv_err}")),
                },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace-demo --example paper_mesh_spike spirv_rung -- --nocapture`
Expected: PASS (regardless of glslang install state).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-demo/examples/paper_mesh_spike/transpile.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(mesh): optional glslang->SPIR-V->naga fallback rung'
```

---

## Task 4: Offscreen harness — vertex+fragment link, uniforms, PNG (`harness.rs`)

**Files:**
- Create/replace: `crates/carapace-demo/examples/paper_mesh_spike/harness.rs` (replaces the Task 2 stub)

**Interfaces:**
- Consumes: `crate::transpile::Transpiled`.
- Produces:
  - `pub struct Gpu { pub device: wgpu::Device, pub queue: wgpu::Queue }`
  - `pub fn new_gpu() -> Gpu`
  - `pub fn wgpu_accepts(gpu: &Gpu, wgsl: &str) -> Result<(), String>` — G2 module check.
  - `pub struct UniformField { pub name: String, pub group: u32, pub binding: u32, pub size: u64 }`
  - `pub fn uniform_fields(module: &naga::Module) -> Vec<UniformField>`
  - `pub fn render_mesh(gpu: &Gpu, vert_wgsl: &str, frag_wgsl: &str, frag_fields: &[UniformField], vert_fields: &[UniformField], w: u32, h: u32, time: f32, out: &std::path::Path) -> Result<(), String>` — G3, links both stages and renders one frame.

- [ ] **Step 1: Copy the proven offscreen + readback scaffolding**

Port `offscreen()` + `readback()` from `crates/carapace-demo/examples/shoot.rs:33-110` into `harness.rs` as `new_gpu()` + internal `readback()`. Use `TextureFormat::Rgba8Unorm`, `usage = RENDER_ATTACHMENT | COPY_SRC`, keep the 256-byte row-padding for PNG readback.

- [ ] **Step 2: Write the failing test (G2 accept on trivial WGSL)**

Add to `harness.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_valid_wgsl() {
        let gpu = new_gpu();
        let wgsl = "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";
        assert!(wgpu_accepts(&gpu, wgsl).is_ok());
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p carapace-demo --example paper_mesh_spike accepts_valid_wgsl -- --nocapture`
Expected: FAIL to compile (`new_gpu`/`wgpu_accepts` not found).

- [ ] **Step 4: Implement `wgpu_accepts` + `uniform_fields`**

```rust
pub fn wgpu_accepts(gpu: &Gpu, wgsl: &str) -> Result<(), String> {
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _m = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("paper"), source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    match pollster::block_on(gpu.device.pop_error_scope()) {
        Some(e) => Err(format!("{e}")),
        None => Ok(()),
    }
}

pub struct UniformField { pub name: String, pub group: u32, pub binding: u32, pub size: u64 }

pub fn uniform_fields(module: &naga::Module) -> Vec<UniformField> {
    let mut layouter = naga::proc::Layouter::default();
    let _ = layouter.update(module.to_ctx());
    module.global_variables.iter().filter_map(|(_, gv)| {
        if gv.space != naga::AddressSpace::Uniform { return None; }
        let rb = gv.binding.as_ref()?;
        Some(UniformField {
            name: gv.name.clone().unwrap_or_default(),
            group: rb.group, binding: rb.binding,
            size: layouter[gv.ty].size as u64,
        })
    }).collect()
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p carapace-demo --example paper_mesh_spike accepts_valid_wgsl -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Observe naga's uniform + varying representation, then implement `render_mesh`**

First run a throwaway observation (temporary prints in `main.rs`): transpile `vertex.vert` (Vertex) and `mesh_gradient.frag` (Fragment), print `uniform_fields(...)` for each and the first ~40 lines of each WGSL. **Record** how naga names the entry points, how it represents the freestanding uniforms (individual `@group/@binding` globals vs. a merged block), and the `@location` of the vertex `v_objectUV` output vs. the fragment input. This observation determines the exact binding code and confirms the location-0 linkage assumption.

Then implement `render_mesh`:
- Create both shader modules; build a `RenderPipeline` with the vertex entry from `vert_wgsl` and fragment entry from `frag_wgsl`, target `Rgba8Unorm`, primitive `TriangleStrip`. Draw a 4-vertex quad (positions from a small vertex-index expansion, mirroring `crates/carapace/src/composite.wgsl:6-15`) OR bind a vertex buffer of the 4 clip-space corners if paper's vertex shader consumes `a_position` (`layout(location=0) in vec4 a_position;` — supply the corner quad as a vertex buffer at location 0).
- For each field in `vert_fields` + `frag_fields`, create a uniform buffer of `size` and bind at `(group, binding)`; fill by name:
  `u_time`→`time`; `u_resolution`→`[w as f32, h as f32]`; `u_pixelRatio`→`1.0`; `u_scale`→`1.0`; `u_fit`→`0.0`; `u_worldWidth`/`u_worldHeight`→`0.0`; `u_originX`/`u_originY`→`0.5`; `u_rotation`/`u_offsetX`/`u_offsetY`→`0.0`; `u_colorsCount`→`4.0`;
  `u_colors` (vec4[10])→ four visible colors then zeros, e.g. `[(0.94,0.28,0.44,1),(0.15,0.39,0.92,1),(0.99,0.76,0.18,1),(0.11,0.78,0.55,1), 0…]`;
  `u_distortion`→`0.6`; `u_swirl`→`0.5`; `u_grainMixer`→`0.0`; `u_grainOverlay`→`0.0`; any unlisted field zero-filled.
- Render pass, `readback()`, write PNG via `image`. Wrap pipeline creation in a validation error scope; return `Err(msg)` on failure.

- [ ] **Step 7: Manually verify the animated mesh gradient**

Run (after Task 5 wires `render_mesh`):
```bash
cargo run -p carapace-demo --example paper_mesh_spike
open /tmp/paper-mesh-spike/mesh_t0.png /tmp/paper-mesh-spike/mesh_t1.png /tmp/paper-mesh-spike/mesh_t2.png
```
Expected: three PNGs showing paper's flowing mesh gradient (soft blended color spots), each **visibly different** (proves `u_time` drives motion). Compare against https://shaders.paper.design/ mesh-gradient reference by eye.

- [ ] **Step 8: Commit**

```bash
git add crates/carapace-demo/examples/paper_mesh_spike/harness.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(mesh): offscreen wgpu harness, vertex+fragment link, uniform binding, PNG'
```

---

## Task 5: Driver — transpile both stages, link, render sequence (`main.rs`)

**Files:**
- Modify: `crates/carapace-demo/examples/paper_mesh_spike/main.rs` (replace the Task 2 stub)

**Interfaces:**
- Consumes: `transpile::{transpile, Rung}`, `harness::{new_gpu, wgpu_accepts, uniform_fields, render_mesh}`.
- Produces: stdout report (per-stage rung, G2, G3) + PNG sequence under `/tmp/paper-mesh-spike/`.

- [ ] **Step 1: Write the driver**

`main.rs`:
```rust
mod transpile;
mod harness;

use std::path::Path;
use transpile::{transpile, Rung};

fn rung_str(r: &Rung) -> &'static str {
    match r { Rung::Direct=>"direct", Rung::Preprocessed=>"preproc", Rung::SpirV=>"spirv", Rung::Unavailable=>"n/a" }
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/paper_mesh_spike/shaders");
    let out = Path::new("/tmp/paper-mesh-spike");
    std::fs::create_dir_all(out).unwrap();
    let gpu = harness::new_gpu();

    let vsrc = std::fs::read_to_string(dir.join("vertex.vert")).unwrap();
    let fsrc = std::fs::read_to_string(dir.join("mesh_gradient.frag")).unwrap();

    // G1
    let vt = transpile(&vsrc, naga::ShaderStage::Vertex);
    let ft = transpile(&fsrc, naga::ShaderStage::Fragment);
    match (&vt, &ft) {
        (Ok(v), Ok(f)) => {
            println!("G1 vertex:   {}", rung_str(&v.rung));
            println!("G1 fragment: {}", rung_str(&f.rung));
            // G2
            let g2v = wgpu_ok(&gpu, &v.wgsl); let g2f = wgpu_ok(&gpu, &f.wgsl);
            println!("G2 vertex:   {g2v}\nG2 fragment: {g2f}");
            // G3
            let vf = harness::uniform_fields(&v.module);
            let ff = harness::uniform_fields(&f.module);
            let mut g3 = "ok";
            for (i, t) in [0.0_f32, 1.3, 2.6].iter().enumerate() {
                let p = out.join(format!("mesh_t{i}.png"));
                if let Err(e) = harness::render_mesh(&gpu, &v.wgsl, &f.wgsl, &ff, &vf, 512, 512, *t, &p) {
                    g3 = "fail"; println!("G3 render t{i}: {e}");
                }
            }
            println!("G3 render: {g3} -> {}", out.display());
        }
        _ => {
            if let Err(e) = &vt { println!("G1 vertex FAIL: {}", e.lines().next().unwrap_or("")); }
            if let Err(e) = &ft { println!("G1 fragment FAIL: {}", e.lines().next().unwrap_or("")); }
        }
    }
}

fn wgpu_ok(gpu: &harness::Gpu, wgsl: &str) -> &'static str {
    if harness::wgpu_accepts(gpu, wgsl).is_ok() { "pass" } else { "fail" }
}
```

- [ ] **Step 2: Run the full spike**

Run: `cargo run -p carapace-demo --example paper_mesh_spike`
Expected: G1 rungs for both stages, G2 pass/fail for both, G3 ok/fail with the PNG dir. This output is the raw feasibility data.

- [ ] **Step 3: Run clippy (CI gate)**

Run: `cargo clippy -p carapace-demo --example paper_mesh_spike -- -D warnings`
Expected: no warnings (CI gates on `-D warnings`). Fix before committing.

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-demo/examples/paper_mesh_spike/main.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(mesh): driver links vertex+fragment, renders animated sequence'
```

---

## Task 6: Findings doc + go/no-go verdict

**Files:**
- Create: `docs/superpowers/specs/2026-07-02-paper-shader-transpile-spike-findings.md`

**Interfaces:** none (documentation).

- [ ] **Step 1: Record results** — paste the Task 5 output: per-stage G1 rung (or failure + first error line), G2 accept for each stage, G3 render result. Note the observed uniform representation (Task 4 Step 6), whether `v_objectUV` linked at location 0, any preprocessing rules added, and `glslangValidator` availability.

- [ ] **Step 2: Write the verdict** — go/no-go on the one question: *can we run paper's mesh gradient live, via transpilation, without hand-porting it — YES / NO / PARTIAL*, plus a one-paragraph **Phase-2 recommendation** (e.g. "both stages transpile naga-direct and render a flowing gradient → Phase 2 reuses the WGSL in wgpu, feeding an IOSurface into `view{}`" vs "vertex needs SPIR-V rung / freestanding uniforms don't map cleanly → prefer naga `msl-out` and render natively in the Swift host").

- [ ] **Step 3: Attach artifacts** — list the `/tmp/paper-mesh-spike/mesh_t*.png` paths and whether they visually matched paper's reference and animated.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-07-02-paper-shader-transpile-spike-findings.md
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'docs(mesh): paper mesh-gradient transpile feasibility findings + verdict'
```

---

## Self-Review

**Spec coverage:**
- G1 transpile of BOTH stages, record rung → Tasks 2–3 (impl), Task 5 (run), Task 6 (report). ✓
- G2 wgpu accept + vertex/fragment link → Task 4 (`wgpu_accepts` + pipeline in `render_mesh`), Task 5. ✓
- G3 animated render matching reference → Task 4 Step 6–7, Task 5, verified Task 6. ✓
- Vendor resolved GLSL (fragment + vertex) via evaluating the package, through sfw → Task 1. ✓
- Transpile fallback ladder (direct → preprocess → glslang/SPIR-V) → Tasks 2–3. ✓
- Uniform defaults (sizing + palette + animate u_time) → Task 4 Step 6. ✓
- Standalone example, zero engine-crate diff → Global Constraints + file structure. ✓
- Phase 2 (macOS IOSurface + view{}, wgpu-vs-MSL) → explicitly out of scope; referenced only in the Task 6 verdict recommendation. ✓

**Placeholder scan:** `preprocess()` is a documented no-op scaffold (rung 2 records what normalization was needed), not missing logic. Task 4 Step 6 has one observe-then-implement step because naga's freestanding-uniform representation is the discovery target; the binding fill-by-name code and default values are fully specified. No TBD/TODO.

**Type consistency:** `Transpiled { wgsl, rung, module, info }`, `Rung`, `UniformField { name, group, binding, size }`, and signatures (`transpile`, `new_gpu`, `wgpu_accepts`, `uniform_fields`, `render_mesh`, `Gpu`) are used identically across Tasks 2, 4, 5. ✓

**Note on naga API:** exact naga 29 symbol names (`glsl::Options::from`, `proc::Layouter`, `spv::parse_u8_slice`) are validated at each compile/run step; if one differs, fix against the compiler — the parse→validate→write_string shape is stable in naga 29.

# Paper.design GLSL → WGSL Feasibility Spike — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove (or disprove) that real, unmodified paper.design fragment shaders can be transpiled GLSL→WGSL and run on wgpu, and produce a go/no-go verdict + pass/fail table.

**Architecture:** A standalone, deletable `carapace-demo` example. It vendors fully-resolved paper GLSL (extracted via a Node import script so template-literal snippets are evaluated), transpiles each shader through a fallback ladder (naga `glsl-in` direct → light preprocess → optional glslang→SPIR-V→naga `spv-in`), checks that wgpu accepts the WGSL, and renders a subset to PNGs by introspecting the shader's uniforms. Zero diff to the `carapace` engine crate.

**Tech Stack:** Rust, wgpu 29, naga 29 (`glsl-in`, `wgsl-out`, `spv-in` features), `image` (PNG, already a dev-dep), Node ≥22 + npm (extraction only), Socket Firewall (`sfw`) for all network fetches.

## Global Constraints

- **Zero diff to the `carapace` engine crate.** All new code lives under `crates/carapace-demo/examples/paper_shader_spike/` plus a dev-dependency line in `crates/carapace-demo/Cargo.toml`. If any task appears to need an engine-crate change, STOP and report — that is a spike-scope violation.
- **naga pinned to `29`** — the same version wgpu 29 already resolves (see `Cargo.lock`). The pipeline hands wgpu a **WGSL string**, so the spike's naga copy stays decoupled from wgpu's internal naga.
- **All network fetches go through `sfw`** (Socket Firewall): `sfw npm install ...`, `sfw cargo add ...`, `sfw gh api ...`. First fetch of any third-party package MUST be `sfw`-wrapped.
- **No hand-editing of shader effect logic.** Only mechanical, documented, rule-based preprocessing is allowed (ladder rung 2). Rewriting a shader's body to make it transpile is a failed G1, not a pass.
- **Git identity for commits:** `Daniel Agbemava <danagbemava@gmail.com>` (`git -c user.name=... -c user.email=... commit`).
- **Working directory:** the worktree at `.claude/worktrees/paper-shader-transpile-spike` (branch `worktree-paper-shader-transpile-spike`). Do not `cd` to the main checkout.

## The three gates (recorded per shader)

- **G1 — Transpile:** GLSL → valid WGSL string, no effect-logic edits. Records the ladder rung used (`Direct` / `Preprocessed` / `SpirV` / `Unavailable`).
- **G2 — Accept:** wgpu `create_shader_module` accepts the WGSL (wgpu's own naga validation passes). Cheap; run for all 6.
- **G3 — Render:** full pipeline built with introspected uniforms renders animated frames whose output visually matches paper's reference. Run for the fragment-self-contained subset first; varying-dependent shaders are a stretch and their linkage status is recorded, not forced.

## Test set (real export names)

| Complexity | Fragment export (from `@paper-design/shaders`) | Sizing | G3 tier |
|---|---|---|---|
| trivial | `staticRadialGradientFragmentShader` | vertex varying `v_objectUV` | stretch (needs vertex) |
| easy | `meshGradientFragmentShader` | varying | stretch |
| medium | `warpFragmentShader` | varying | stretch |
| medium | `ditheringFragmentShader` | `gl_FragCoord`, **no varyings** | full |
| hard | `voronoiFragmentShader` | varying | stretch |
| hard | `metaballsFragmentShader` | varying | stretch |

`ditheringFragmentShader` is the concrete full-G3 first-light target because it is fragment-self-contained. The shared vertex shader (`vertexShaderSource`) is transpiled once (Task 5) so the stretch shaders can attempt full G3; varying-linkage frictions between separately-transpiled stages are a recorded finding.

## File Structure

```
crates/carapace-demo/
  Cargo.toml                                  # MODIFY: add naga dev-dep; add [[example]] entry
  examples/paper_shader_spike/
    main.rs        # driver: runs the set, prints pass/fail table, writes findings doc
    transpile.rs   # the ladder: naga glsl-in, preprocess, optional spv fallback; naga IR introspection
    harness.rs     # offscreen wgpu, quad, uniform introspection+binding, render→PNG
    extract.mjs    # Node: imports @paper-design/shaders, writes resolved .frag/.vert
    shaders/       # vendored, fully-resolved GLSL committed for offline reproducibility
      static_radial_gradient.frag
      mesh_gradient.frag
      warp.frag
      dithering.frag
      voronoi.frag
      metaballs.frag
      vertex.vert
docs/superpowers/specs/2026-07-02-paper-shader-transpile-spike-findings.md   # CREATE (Task 6)
```

Three focused Rust files: `transpile.rs` (GLSL→WGSL, no GPU), `harness.rs` (GPU, no transpile), `main.rs` (orchestration). Split by responsibility so each is independently reasoned about and testable.

---

## Task 1: Vendor fully-resolved paper GLSL

**Files:**
- Create: `crates/carapace-demo/examples/paper_shader_spike/extract.mjs`
- Create (generated, committed): `crates/carapace-demo/examples/paper_shader_spike/shaders/*.frag`, `shaders/vertex.vert`

**Interfaces:**
- Produces: seven text files under `shaders/`. Each `.frag`/`.vert` is fully-resolved GLSL (`#version 300 es` first line, contains no `${` template markers). Consumed by every later task via `include_str!` / file read.

**Why Node:** paper's shader strings interpolate shared snippets (`${simplexNoise}`, `${declarePI}`, `${proceduralHash21}`) and constants (`${maxColorCount}`) at JS runtime. Importing the compiled package evaluates them; hand-stitching would be brittle.

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

// export name -> output filename
const FRAGS = {
  staticRadialGradientFragmentShader: 'static_radial_gradient.frag',
  meshGradientFragmentShader: 'mesh_gradient.frag',
  warpFragmentShader: 'warp.frag',
  ditheringFragmentShader: 'dithering.frag',
  voronoiFragmentShader: 'voronoi.frag',
  metaballsFragmentShader: 'metaballs.frag',
};

for (const [exp, file] of Object.entries(FRAGS)) {
  const src = paper[exp];
  if (typeof src !== 'string') throw new Error(`missing export: ${exp}`);
  if (src.includes('${')) throw new Error(`unresolved template in ${exp}`);
  writeFileSync(join(outDir, file), src);
  console.log(`wrote ${file} (${src.length} bytes)`);
}

// Shared vertex shader — export name may vary; try known names.
const vert = paper.vertexShaderSource ?? paper.vertexShader;
if (typeof vert === 'string') {
  writeFileSync(join(outDir, 'vertex.vert'), vert);
  console.log(`wrote vertex.vert (${vert.length} bytes)`);
} else {
  console.warn('WARN: shared vertex shader not exported from package root; ' +
    'stretch-tier G3 will record vertex-unavailable.');
}
```

- [ ] **Step 2: Install the package (via sfw) and run the script**

Run:
```bash
SPIKE=crates/carapace-demo/examples/paper_shader_spike
SCRATCH=$(mktemp -d)
( cd "$SCRATCH" && sfw npm install @paper-design/shaders )
node "$SPIKE/extract.mjs" "$SPIKE/shaders" \
  && ls -l "$SPIKE/shaders"
```
Expected: six `.frag` files written with non-zero byte counts, plus either `vertex.vert` or the `WARN` line. (If `import * as paper` fails because the package is ESM-subpath-only, re-run the script from inside `$SCRATCH` with `node --input-type=module`, or import from the resolved `@paper-design/shaders` entry printed by `node -e "console.log(require.resolve('@paper-design/shaders'))"`.)

- [ ] **Step 3: Verify each file is real, resolved GLSL**

Run:
```bash
for f in crates/carapace-demo/examples/paper_shader_spike/shaders/*.frag; do
  head -1 "$f"; grep -c '\${' "$f" | sed "s/^/  unresolved-markers: /"
done
```
Expected: every file's first line is `#version 300 es`; every `unresolved-markers` count is `0`.

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-demo/examples/paper_shader_spike/extract.mjs \
        crates/carapace-demo/examples/paper_shader_spike/shaders/
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(paper-shader): vendor resolved paper.design GLSL via node extraction'
```

---

## Task 2: Transpile ladder — rungs 1 & 2 (`transpile.rs`)

**Files:**
- Modify: `crates/carapace-demo/Cargo.toml` (add naga dev-dep + example entry)
- Create: `crates/carapace-demo/examples/paper_shader_spike/transpile.rs`

**Interfaces:**
- Produces (used by `harness.rs` and `main.rs`):
  - `pub enum Rung { Direct, Preprocessed, SpirV, Unavailable }`
  - `pub struct Transpiled { pub wgsl: String, pub rung: Rung, pub module: naga::Module, pub info: naga::valid::ModuleInfo }`
  - `pub fn transpile(glsl: &str, stage: naga::ShaderStage) -> Result<Transpiled, String>` — tries rung 1 then rung 2, returns first success or the last error string.
  - `pub fn preprocess(glsl: &str) -> String` — mechanical normalization only.

- [ ] **Step 1: Add the naga dev-dependency and example entry**

Run:
```bash
sfw cargo add --dev --package carapace-demo naga@29 --features glsl-in,wgsl-out,spv-in
```
Then add to `crates/carapace-demo/Cargo.toml` (after the existing `[[bin]]`):
```toml
[[example]]
name = "paper_shader_spike"
path = "examples/paper_shader_spike/main.rs"
```

- [ ] **Step 2: Write the failing test**

Create `transpile.rs` with a test module (the example's `main.rs` will `mod transpile;` in Task 5; for now the test runs via `cargo test --example paper_shader_spike` once `main.rs` exists — so also create a minimal `main.rs` stub `fn main() {}` with `mod transpile;` to let the test compile). Add to `transpile.rs`:

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

Run: `cargo test -p carapace-demo --example paper_shader_spike trivial_fragment -- --nocapture`
Expected: FAIL to compile (`transpile` / `Rung` not found).

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
/// Rung 2 exists so we can record *what* normalization a shader needed.
pub fn preprocess(glsl: &str) -> String {
    // Currently a no-op scaffold; add rule-based rewrites here only as concrete
    // transpile failures demand them (e.g. stripping an unsupported extension
    // pragma). Every rule added here MUST be logged in the findings doc.
    glsl.to_string()
}

fn naga_to_wgsl(glsl: &str, stage: naga::ShaderStage) -> Result<(naga::Module, naga::valid::ModuleInfo, String), String> {
    let mut frontend = glsl::Frontend::default();
    let module = frontend
        .parse(&glsl::Options::from(stage), glsl)
        .map_err(|e| format!("glsl parse: {e:?}"))?;
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator
        .validate(&module)
        .map_err(|e| format!("naga validate: {e:?}"))?;
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
                Ok((module, info, wgsl)) => Ok(Transpiled { wgsl, rung: Rung::Preprocessed, module, info }),
                Err(pre_err) => Err(format!("direct: {direct_err} | preprocessed: {pre_err}")),
            }
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p carapace-demo --example paper_shader_spike trivial_fragment -- --nocapture`
Expected: PASS. (If the naga API names differ in 29.0.3, fix against the compiler error — the shapes above match naga 29's `Frontend::parse`/`Validator::validate`/`wgsl::write_string`.)

- [ ] **Step 6: Commit**

```bash
git add crates/carapace-demo/Cargo.toml crates/carapace-demo/examples/paper_shader_spike/
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(paper-shader): naga glsl-in transpile ladder (rungs 1-2) + test'
```

---

## Task 3: Optional SPIR-V fallback rung (`transpile.rs`)

**Files:**
- Modify: `crates/carapace-demo/examples/paper_shader_spike/transpile.rs`

**Interfaces:**
- Produces: `pub fn transpile` now attempts rung 3 after rung 2. Rung 3 shells out to `glslangValidator` (GLSL→SPIR-V) then `naga`'s `spv-in`. If `glslangValidator` is absent, rung 3 returns a distinct "unavailable" signal so `main.rs` records `Rung::Unavailable` rather than a hard failure.

- [ ] **Step 1: Write the failing test**

Add to `transpile.rs` tests:
```rust
#[test]
fn spirv_rung_reports_availability() {
    // glslang_available() must not panic and returns a bool.
    let _ = glslang_available();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-demo --example paper_shader_spike spirv_rung -- --nocapture`
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
    if !glslang_available() {
        return Err("glslangValidator not on PATH".into());
    }
    let dir = std::env::temp_dir();
    let ext = match stage { naga::ShaderStage::Fragment => "frag", naga::ShaderStage::Vertex => "vert", _ => "comp" };
    let src = dir.join(format!("paper_spike_in.{ext}"));
    let spv = dir.join("paper_spike_out.spv");
    std::fs::write(&src, glsl).map_err(|e| e.to_string())?;
    let out = Command::new("glslangValidator")
        .args(["-G", "-o"]).arg(&spv).arg(&src)
        .output().map_err(|e| e.to_string())?;
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
Then extend `transpile()`'s final `Err` arm to try `via_spirv` before giving up:
```rust
                Err(pre_err) => match via_spirv(glsl, stage) {
                    Ok(t) => Ok(t),
                    Err(spv_err) => Err(format!(
                        "direct: {direct_err} | preprocessed: {pre_err} | spirv: {spv_err}"
                    )),
                },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace-demo --example paper_shader_spike spirv_rung -- --nocapture`
Expected: PASS (regardless of whether glslang is installed).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-demo/examples/paper_shader_spike/transpile.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(paper-shader): optional glslang->SPIR-V->naga fallback rung'
```

---

## Task 4: Offscreen render harness (`harness.rs`)

**Files:**
- Create: `crates/carapace-demo/examples/paper_shader_spike/harness.rs`

**Interfaces:**
- Consumes: `crate::transpile::Transpiled` (the `module` + `wgsl` fields).
- Produces:
  - `pub struct Gpu { pub device: wgpu::Device, pub queue: wgpu::Queue }`
  - `pub fn new_gpu() -> Gpu`
  - `pub fn wgpu_accepts(gpu: &Gpu, wgsl: &str) -> Result<(), String>` — G2 gate.
  - `pub struct UniformField { pub name: String, pub binding: u32, pub group: u32, pub size: u64 }`
  - `pub fn uniform_fields(module: &naga::Module) -> Vec<UniformField>`
  - `pub fn render_png(gpu: &Gpu, frag_wgsl: &str, fields: &[UniformField], w: u32, h: u32, time: f32, out: &std::path::Path) -> Result<(), String>` — G3 gate for fragment-self-contained shaders.

- [ ] **Step 1: Copy the proven offscreen + readback scaffolding**

Port `offscreen()` and `readback()` from `crates/carapace-demo/examples/shoot.rs:33-110` into `harness.rs` as `new_gpu()` + an internal `readback()`. Use `TextureFormat::Rgba8Unorm`, `usage = RENDER_ATTACHMENT | COPY_SRC` (render target, not storage). Keep the 256-byte row-padding logic from `shoot.rs` for PNG readback.

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

Run: `cargo test -p carapace-demo --example paper_shader_spike accepts_valid_wgsl -- --nocapture`
Expected: FAIL to compile (`new_gpu`/`wgpu_accepts` not found).

- [ ] **Step 4: Implement `wgpu_accepts` and uniform introspection**

`wgpu_accepts`: create a shader module from the WGSL inside `device.on_uncaptured_error` scope (or push_error_scope/pop_error_scope) and return `Err` with the validation message if wgpu rejects it:
```rust
pub fn wgpu_accepts(gpu: &Gpu, wgsl: &str) -> Result<(), String> {
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _m = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("paper-frag"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    match pollster::block_on(gpu.device.pop_error_scope()) {
        Some(e) => Err(format!("{e}")),
        None => Ok(()),
    }
}
```

`uniform_fields`: walk `module.global_variables` for `space == naga::AddressSpace::Uniform`, reading each var's `binding` (`ResourceBinding { group, binding }`), `name`, and byte size via `module.types` + a layouter (`naga::proc::Layouter`). This is what lets the harness bind uniforms without hardcoding per-shader names.
```rust
pub struct UniformField { pub name: String, pub binding: u32, pub group: u32, pub size: u64 }

pub fn uniform_fields(module: &naga::Module) -> Vec<UniformField> {
    let mut layouter = naga::proc::Layouter::default();
    let _ = layouter.update(module.to_ctx());
    module.global_variables.iter().filter_map(|(_, gv)| {
        if gv.space != naga::AddressSpace::Uniform { return None; }
        let rb = gv.binding.as_ref()?;
        let size = layouter[gv.ty].size as u64;
        Some(UniformField {
            name: gv.name.clone().unwrap_or_default(),
            binding: rb.binding, group: rb.group, size,
        })
    }).collect()
}
```
(If naga glsl-in emits freestanding uniforms *without* `binding` — i.e. as push-constants or a merged default block — `uniform_fields` returns fewer entries than expected; **Step 6's observe run reveals which**, and the finding is recorded either way.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p carapace-demo --example paper_shader_spike accepts_valid_wgsl -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Observe how naga represents dithering's uniforms, then implement `render_png`**

Run a throwaway observation (temporary `println!` in `main.rs` or a `dbg!` test) that transpiles `dithering.frag` and prints `uniform_fields(&t.module)` and the first 60 lines of `t.wgsl`. Record the actual group/binding layout. Then implement `render_png`:
- Build a passthrough quad **vertex** shader in WGSL (positions only; ports the quad from `crates/carapace/src/composite.wgsl:6-15`).
- For each `UniformField`, create a uniform buffer of `size`, filled by name: `u_time`→`time`, `u_resolution`→`[w,h]`, `u_pixelRatio`→`1.0`, `u_scale`→`1.0`, `u_fit`→`0.0`, `u_worldWidth`/`u_worldHeight`→`0.0`, `u_originX`/`u_originY`→`0.5`; all other fields zero-filled. Bind each at its `(group, binding)`.
- Create the render pipeline (quad vertex + transpiled fragment, `Rgba8Unorm` target), draw the quad, `readback()`, write PNG via `image`.
Return `Err(msg)` on any wgpu validation error (wrap pipeline creation in an error scope like `wgpu_accepts`).

- [ ] **Step 7: Manually verify the dithering PNG**

Run (after Task 5's `main.rs` calls `render_png` for dithering):
```bash
cargo run -p carapace-demo --example paper_shader_spike
open /tmp/paper-shader-spike/dithering_t0.png /tmp/paper-shader-spike/dithering_t1.png
```
Expected: two PNGs that (a) look like paper's dithering effect (2-color ordered dithering) and (b) **differ from each other** (proves `u_time` animation drives change). Compare against https://shaders.paper.design/ dithering reference by eye.

- [ ] **Step 8: Commit**

```bash
git add crates/carapace-demo/examples/paper_shader_spike/harness.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(paper-shader): offscreen wgpu harness, uniform introspection, PNG render'
```

---

## Task 5: Driver, shared vertex shader, pass/fail table (`main.rs`)

**Files:**
- Modify: `crates/carapace-demo/examples/paper_shader_spike/main.rs` (replace the Task 2 stub)

**Interfaces:**
- Consumes: `transpile::{transpile, Rung}`, `harness::{new_gpu, wgpu_accepts, uniform_fields, render_png}`.
- Produces: stdout pass/fail table + PNGs under `/tmp/paper-shader-spike/`.

- [ ] **Step 1: Write the driver**

`main.rs`:
```rust
mod transpile;
mod harness;

use std::path::Path;
use transpile::{transpile, Rung};

struct Spec { name: &'static str, file: &'static str, self_contained: bool }

const SPECS: &[Spec] = &[
    Spec { name: "static_radial_gradient", file: "static_radial_gradient.frag", self_contained: false },
    Spec { name: "mesh_gradient",          file: "mesh_gradient.frag",          self_contained: false },
    Spec { name: "warp",                   file: "warp.frag",                   self_contained: false },
    Spec { name: "dithering",              file: "dithering.frag",              self_contained: true  },
    Spec { name: "voronoi",                file: "voronoi.frag",                self_contained: false },
    Spec { name: "metaballs",              file: "metaballs.frag",              self_contained: false },
];

fn rung_str(r: &Rung) -> &'static str {
    match r { Rung::Direct => "direct", Rung::Preprocessed => "preproc",
              Rung::SpirV => "spirv", Rung::Unavailable => "n/a" }
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/paper_shader_spike/shaders");
    let out = Path::new("/tmp/paper-shader-spike");
    std::fs::create_dir_all(out).unwrap();
    let gpu = harness::new_gpu();

    println!("{:<24} {:>7} {:>6} {:>6}  note", "shader", "G1", "G2", "G3");
    for s in SPECS {
        let glsl = std::fs::read_to_string(dir.join(s.file)).unwrap();
        let (g1, g2, g3, mut note) = ("fail", "-", "-", String::new());
        match transpile(&glsl, naga::ShaderStage::Fragment) {
            Err(e) => note = format!("G1: {}", e.lines().next().unwrap_or("")),
            Ok(t) => {
                let g1 = rung_str(&t.rung);
                let g2 = if harness::wgpu_accepts(&gpu, &t.wgsl).is_ok() { "pass" } else { "fail" };
                let mut g3 = "skip";
                if s.self_contained && g2 == "pass" {
                    let fields = harness::uniform_fields(&t.module);
                    for (i, time) in [0.0_f32, 1.3].iter().enumerate() {
                        let p = out.join(format!("{}_t{i}.png", s.name));
                        match harness::render_png(&gpu, &t.wgsl, &fields, 512, 512, *time, &p) {
                            Ok(()) => g3 = "png",
                            Err(e) => { g3 = "fail"; note = format!("G3: {e}"); }
                        }
                    }
                }
                println!("{:<24} {:>7} {:>6} {:>6}  {}", s.name, g1, g2, g3, note);
                continue;
            }
        }
        println!("{:<24} {:>7} {:>6} {:>6}  {}", s.name, g1, g2, g3, note);
    }
}
```

- [ ] **Step 2: Transpile the shared vertex shader (G1/G2 for the vertex stage)**

Add, after the GPU init, a one-shot vertex-stage check so the findings can report whether paper's vertex shader itself transpiles (prerequisite for stretch-tier full G3):
```rust
    if let Ok(vsrc) = std::fs::read_to_string(dir.join("vertex.vert")) {
        match transpile(&vsrc, naga::ShaderStage::Vertex) {
            Ok(t) => println!("[vertex] G1 {} / G2 {}", rung_str(&t.rung),
                if harness::wgpu_accepts(&gpu, &t.wgsl).is_ok() {"pass"} else {"fail"}),
            Err(e) => println!("[vertex] G1 fail: {}", e.lines().next().unwrap_or("")),
        }
    } else {
        println!("[vertex] unavailable (not extracted)");
    }
```

- [ ] **Step 3: Run the full spike**

Run: `cargo run -p carapace-demo --example paper_shader_spike`
Expected: a table with one row per shader (G1 rung, G2 pass/fail, G3 png/skip/fail) plus the `[vertex]` line. `dithering` should show `G3 png`. This output IS the raw feasibility data.

- [ ] **Step 4: Run clippy (CI gate)**

Run: `cargo clippy -p carapace-demo --example paper_shader_spike -- -D warnings`
Expected: no warnings. Fix any before committing (CI gates on `-D warnings`).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-demo/examples/paper_shader_spike/main.rs
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'spike(paper-shader): driver, vertex-stage check, pass/fail table'
```

---

## Task 6: Findings doc + go/no-go verdict

**Files:**
- Create: `docs/superpowers/specs/2026-07-02-paper-shader-transpile-spike-findings.md`

**Interfaces:** none (documentation).

- [ ] **Step 1: Record the results**

Paste the actual table from Task 5 Step 3. For each shader record: G1 rung (or failure + first error line), G2 pass/fail, G3 png/skip/fail. Note the vertex-stage result. Include any preprocessing rules added to `preprocess()` and whether `glslangValidator` was available.

- [ ] **Step 2: Write the verdict**

Follow the "Expected findings shape" from the design doc: a go/no-go sentence and a one-paragraph recommendation for the follow-on (e.g. "naga direct handles simple+medium; hard/varying shaders need X → the `shader{}` primitive should ship a build-time transpile step" OR "naga rejects freestanding-uniform default blocks → runtime transpile is not viable, hand-port or SPIR-V build step required"). State explicitly whether the core question — *can we run paper's shaders without hand-porting each one* — is YES, NO, or PARTIAL (which subset).

- [ ] **Step 3: Attach artifacts**

List the PNG paths under `/tmp/paper-shader-spike/` produced, noting which visually matched paper's reference.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-07-02-paper-shader-transpile-spike-findings.md
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m 'docs(spike): paper.design GLSL->WGSL feasibility findings + verdict'
```

---

## Self-Review

**Spec coverage:**
- Goal/success criteria (G1/G2/G3, pass-fail table, verdict) → Tasks 2–6. ✓
- Test set (6 shaders, complexity ladder) → Task 1 vendors all six; Task 5 runs all six. ✓ (real export names substituted for `radial_gradient`→`staticRadialGradient`, documented in the design's substitution rule.)
- Source from `@paper-design/shaders` core, via sfw → Task 1. ✓
- Transpile fallback ladder (naga direct → preprocess → glslang/SPIR-V), record rung → Tasks 2–3, reported in Task 6. ✓
- Uniforms: drive time+resolution+defaults → Task 4 Step 6. ✓
- Standalone example, zero engine-crate diff → Global Constraints + file structure (all under `examples/`). ✓
- winit window → **deliberately changed** to offscreen+PNG-at-multiple-times (matches repo's `shoot.rs` pattern; more verifiable/headless). Noted to the user. ✓
- Out-of-scope items (shader{} primitive, view{} compositing, sandbox, audio, perf) → none appear in any task. ✓

**Placeholder scan:** `preprocess()` ships as a documented no-op scaffold (rung 2 is "record what normalization was needed", legitimately empty until a real failure demands a rule) — not a placeholder for missing logic. Task 4 Step 6 has one genuine observe-then-implement step because naga's freestanding-uniform representation is the spike's discovery target; the two likely outcomes and the concrete binding code are both specified. No other TBD/TODO.

**Type consistency:** `Transpiled { wgsl, rung, module, info }`, `Rung` variants, `UniformField { name, binding, group, size }`, and function signatures (`transpile`, `wgpu_accepts`, `uniform_fields`, `render_png`, `new_gpu`) are used identically across Tasks 2, 4, and 5. ✓

**Note on naga API:** exact naga 29.0.3 symbol names (`glsl::Options::from`, `Layouter`, `spv::parse_u8_slice`) are validated at each task's compile/run step; if a symbol differs, fix against the compiler — the transpile *shape* (parse→validate→write_string) is stable in naga 29.

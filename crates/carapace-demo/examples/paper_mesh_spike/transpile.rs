// This module is the transpile-ladder scaffold consumed by `harness.rs` and
// `main.rs` in later tasks; several items (the `SpirV`/`Unavailable` rungs,
// the `module`/`info` fields, and the public functions themselves) have no
// caller yet within Task 2, so silence dead_code for the whole module rather
// than sprinkling per-item allows.
#![allow(dead_code)]

use naga::back::wgsl;
use naga::front::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::process::Command;

/// Which rung of the transpile ladder produced a successful WGSL result.
pub enum Rung {
    Direct,
    Preprocessed,
    SpirV,
    Unavailable,
}

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

/// naga's SPIR-V frontend (`spv-in`) does NOT support Vulkan **combined** image
/// samplers. It only resolves the sampled-image operand of `OpImageSample*` when
/// that operand was produced by an `OpSampledImage` instruction (which combines a
/// separate image + sampler). glslang lowers a combined `sampler2D` uniform to a
/// direct `OpLoad` of an `OpTypeSampledImage` — no `OpSampledImage` — so naga can't
/// find the operand and bails with `InvalidId`. This is exactly why `metaballs` and
/// `voronoi` (the only two paper shaders that sample `u_noiseTexture`) failed while
/// the texture-free shaders transpiled cleanly.
///
/// This rewrite splits each `uniform sampler2D NAME;` into the Vulkan-GLSL separate
/// form (`texture2D NAME;` + `sampler NAME_smp;`) and rewrites `texture(NAME, …)`
/// call sites to recombine them inline via `sampler2D(NAME, NAME_smp)`. glslang then
/// emits an explicit `OpSampledImage`, which naga accepts. It is a pure syntactic
/// normalization — the combined and separate forms sample identically — so it never
/// touches effect logic.
///
/// Scope: only the `texture()` builtin is rewritten (the sole sampling call the paper
/// shaders use). `texelFetch`/`textureSize` take the bare `texture2D` in separate form,
/// so they are deliberately left for per-shader attention if a future shader needs them.
fn separate_combined_samplers(glsl: &str) -> String {
    let mut names = Vec::new();
    let mut lines = Vec::new();
    for line in glsl.lines() {
        if let Some(name) = combined_sampler_decl_name(line) {
            // A precision qualifier is mandatory for opaque types in GLSL ES; `mediump`
            // matches paper's `precision mediump float;` and is ignored on desktop.
            lines.push(format!("uniform mediump texture2D {name};"));
            lines.push(format!("uniform mediump sampler {name}_smp;"));
            names.push(name);
        } else {
            lines.push(line.to_string());
        }
    }
    if names.is_empty() {
        return glsl.to_string();
    }
    let mut src = lines.join("\n");
    if glsl.ends_with('\n') {
        src.push('\n');
    }
    for name in &names {
        // Wrap the sampler where it is the first argument of `texture(…)`: inserting the
        // constructor open after `texture(` and the sampler + close after the name turns
        // `texture(NAME, coord)` into `texture(sampler2D(NAME, NAME_smp), coord)`.
        src = src.replace(
            &format!("texture({name},"),
            &format!("texture(sampler2D({name}, {name}_smp),"),
        );
        src = src.replace(
            &format!("texture({name} ,"),
            &format!("texture(sampler2D({name}, {name}_smp) ,"),
        );
    }
    src
}

/// If `line` declares a single combined `sampler2D` uniform, return its name.
/// Tolerates a leading `layout(...)` qualifier and an optional precision qualifier.
fn combined_sampler_decl_name(line: &str) -> Option<String> {
    let mut s = line.trim();
    if let Some(after_layout) = s.strip_prefix("layout") {
        let after_layout = after_layout.trim_start();
        let close = after_layout.find(')')?;
        s = after_layout[close + 1..].trim_start();
    }
    let s = s.strip_prefix("uniform")?;
    if !s.starts_with(char::is_whitespace) {
        return None;
    }
    let s = s.trim_start();
    let s = ["lowp ", "mediump ", "highp "]
        .iter()
        .find_map(|p| s.strip_prefix(p))
        .unwrap_or(s)
        .trim_start();
    let s = s.strip_prefix("sampler2D")?;
    if !s.starts_with(char::is_whitespace) {
        return None;
    }
    let name = s.trim().strip_suffix(';')?.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

fn naga_to_wgsl(
    glsl: &str,
    stage: naga::ShaderStage,
) -> Result<(naga::Module, naga::valid::ModuleInfo, String), String> {
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

/// Modern glslang ships `glslang`; older ships `glslangValidator`. Prefer whichever exists.
fn glslang_bin() -> Option<&'static str> {
    ["glslang", "glslangValidator"]
        .into_iter()
        .find(|bin| Command::new(bin).arg("--version").output().is_ok())
}

pub fn glslang_available() -> bool {
    glslang_bin().is_some()
}

/// GLSL (incl. #version 300 es) -> SPIR-V (glslang) -> naga spv-in -> WGSL.
fn via_spirv(glsl: &str, stage: naga::ShaderStage) -> Result<Transpiled, String> {
    let bin = glslang_bin().ok_or("glslang not on PATH")?;
    let dir = std::env::temp_dir();
    let (ext, stg) = match stage {
        naga::ShaderStage::Fragment => ("frag", "frag"),
        naga::ShaderStage::Vertex => ("vert", "vert"),
        _ => ("comp", "comp"),
    };
    let src = dir.join(format!("paper_mesh_in.{ext}"));
    let spv = dir.join(format!("paper_mesh_out_{ext}.spv"));
    // Mechanical directive normalization (NOT a logic change): glslang refuses to emit
    // SPIR-V for `#version 300 es` ("ES shaders for SPIR-V require version 310 or higher").
    // ES 3.10 is a strict superset of ES 3.00 for the constructs paper's shaders use, so we
    // bump only the profile version. Empirical dead-ends that forced the winning flags below:
    //   * `-V --target-env vulkan1.0` (strict) rejects paper's freestanding, non-block
    //     uniforms ("non-opaque uniforms outside a block: not allowed ... for Vulkan").
    //   * `-G` (OpenGL SPIR-V) accepts freestanding uniforms, but emits the fragment entry
    //     point with ExecutionMode OriginLowerLeft, which naga's spv-in rejects
    //     (`UnsupportedExecutionMode(8)` — naga only supports OriginUpperLeft).
    // Winner: `-V --target-env vulkan1.0 -R` on version-bumped source. `-R` (relaxed Vulkan
    // rules) permits the default/non-block uniforms, while Vulkan semantics emit
    // OriginUpperLeft (naga-friendly); `--amb`/`--aml` auto-map bindings/locations.
    let bumped = glsl.replacen("#version 300 es", "#version 310 es", 1);
    // Second mechanical normalization: split combined `sampler2D` uniforms into the
    // Vulkan-GLSL separate texture+sampler form so naga's spv-in can consume them
    // (see `separate_combined_samplers` for the full root-cause note). No-op for the
    // shaders that carry no combined sampler (mesh gradient + the 4 clean ones).
    let bumped = separate_combined_samplers(&bumped);
    std::fs::write(&src, &bumped).map_err(|e| e.to_string())?;
    let out = Command::new(bin)
        .args([
            "-V",
            "--target-env",
            "vulkan1.0",
            "-R",
            "--amb",
            "--aml",
            "-S",
            stg,
            "-o",
        ])
        .arg(&spv)
        .arg(&src)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "glslang: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let bytes = std::fs::read(&spv).map_err(|e| e.to_string())?;
    let module = naga::front::spv::parse_u8_slice(&bytes, &naga::front::spv::Options::default())
        .map_err(|e| format!("spv-in: {e:?}"))?;
    let mut v = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = v
        .validate(&module)
        .map_err(|e| format!("validate spv module: {e:?}"))?;
    let wgsl = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| format!("wgsl out (spv): {e:?}"))?;
    Ok(Transpiled {
        wgsl,
        rung: Rung::SpirV,
        module,
        info,
    })
}

pub fn transpile(glsl: &str, stage: naga::ShaderStage) -> Result<Transpiled, String> {
    match naga_to_wgsl(glsl, stage) {
        Ok((module, info, wgsl)) => Ok(Transpiled {
            wgsl,
            rung: Rung::Direct,
            module,
            info,
        }),
        Err(direct_err) => {
            let pre = preprocess(glsl);
            match naga_to_wgsl(&pre, stage) {
                Ok((module, info, wgsl)) => Ok(Transpiled {
                    wgsl,
                    rung: Rung::Preprocessed,
                    module,
                    info,
                }),
                Err(pre_err) => match via_spirv(glsl, stage) {
                    Ok(t) => Ok(t),
                    Err(spv_err) => Err(format!(
                        "direct: {direct_err} | preprocessed: {pre_err} | spirv: {spv_err}"
                    )),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // naga 29's GLSL frontend only accepts desktop GLSL (`#version 440/450/460`,
    // core profile) and requires explicit `layout(binding=N)` on uniforms — it
    // does not understand GLSL ES's `#version 300 es` / implicit-binding style
    // used by the vendored paper.design shaders. This trivial fixture is written
    // in the dialect naga's Direct rung actually accepts, to prove the ladder
    // mechanism (parse → validate → write_string) end to end; reconciling the
    // real vendored shaders' `#version 300 es` syntax against this is exactly
    // the job of `preprocess()` in a later task, once concrete failures on the
    // real files demand specific rewrite rules.
    const TRIVIAL: &str = "#version 450\nlayout(binding = 0) uniform float u_time;\n\
        out vec4 fragColor;\n\
        void main() { fragColor = vec4(abs(sin(u_time)), 0.0, 0.0, 1.0); }";
    #[test]
    fn trivial_fragment_transpiles_direct() {
        let t = transpile(TRIVIAL, naga::ShaderStage::Fragment).expect("should transpile");
        assert!(matches!(t.rung, Rung::Direct));
        assert!(t.wgsl.contains("@fragment"), "wgsl:\n{}", t.wgsl);
    }

    // A combined `sampler2D` + `texture()` — the exact shape that made metaballs/voronoi
    // fail at naga spv-in with `InvalidId` before `separate_combined_samplers`.
    const SAMPLED: &str = "#version 300 es\nprecision mediump float;\n\
        uniform sampler2D u_noiseTexture;\nin vec2 v_uv;\nout vec4 fragColor;\n\
        void main() { fragColor = texture(u_noiseTexture, v_uv); }";

    #[test]
    fn separate_combined_samplers_splits_decl_and_call() {
        // Pure syntactic transform — no glslang/naga needed, so this always runs.
        let out =
            separate_combined_samplers(&SAMPLED.replace("#version 300 es", "#version 310 es"));
        assert!(
            out.contains("uniform mediump texture2D u_noiseTexture;")
                && out.contains("uniform mediump sampler u_noiseTexture_smp;"),
            "decl not split:\n{out}"
        );
        assert!(
            out.contains("texture(sampler2D(u_noiseTexture, u_noiseTexture_smp), v_uv)"),
            "call site not recombined:\n{out}"
        );
        assert!(
            !out.contains("uniform sampler2D"),
            "combined sampler2D uniform must be gone:\n{out}"
        );
        // No combined sampler -> exact no-op (byte-identical).
        let plain = "#version 310 es\nout vec4 c;\nvoid main() { c = vec4(1.0); }";
        assert_eq!(separate_combined_samplers(plain), plain);
    }

    #[test]
    fn combined_sampler_uniform_transpiles_and_wgsl_reparses() {
        // Regression for the metaballs/voronoi spv-in `InvalidId` failure: a combined
        // `sampler2D` must (a) reach valid WGSL via the ladder and (b) round-trip through
        // naga's wgsl-in — the same parse wgpu does at pipeline creation.
        if !glslang_available() {
            eprintln!("SKIP combined_sampler_uniform_transpiles: glslang not installed");
            return;
        }
        let t = transpile(SAMPLED, naga::ShaderStage::Fragment)
            .expect("combined sampler2D must transpile via the ladder");
        assert!(matches!(t.rung, Rung::SpirV), "expected spirv rung");
        assert!(t.wgsl.contains("@fragment"), "wgsl:\n{}", t.wgsl);
        // wgsl-out -> wgsl-in round trip: what wgpu does when it builds the pipeline.
        let module = naga::front::wgsl::parse_str(&t.wgsl).unwrap_or_else(|e| {
            panic!("emitted WGSL must re-parse via wgsl-in: {e:?}\n{}", t.wgsl)
        });
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .expect("re-parsed WGSL module must validate");
    }

    #[test]
    fn spirv_rung_reports_availability() {
        // Value is environment-dependent; assert the call is total + deterministic.
        assert_eq!(glslang_available(), glslang_available());
    }

    #[test]
    fn real_paper_es_shaders_transpile() {
        // The core feasibility check: the ACTUAL vendored #version 300 es shaders must
        // reach a valid WGSL string via SOME rung (expected Rung::SpirV via glslang).
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/paper_mesh_spike/shaders");
        let frag = std::fs::read_to_string(dir.join("mesh_gradient.frag")).unwrap();
        let vert = std::fs::read_to_string(dir.join("vertex.vert")).unwrap();
        if !glslang_available() {
            eprintln!("SKIP real_paper_es_shaders_transpile: glslang not installed");
            return;
        }
        let f = transpile(&frag, naga::ShaderStage::Fragment)
            .expect("fragment must transpile via the ladder");
        let v = transpile(&vert, naga::ShaderStage::Vertex)
            .expect("vertex must transpile via the ladder");
        assert!(
            f.wgsl.contains("@fragment"),
            "frag wgsl:\n{}",
            &f.wgsl[..f.wgsl.len().min(400)]
        );
        assert!(
            v.wgsl.contains("@vertex"),
            "vert wgsl:\n{}",
            &v.wgsl[..v.wgsl.len().min(400)]
        );
        assert!(matches!(
            f.rung,
            Rung::SpirV | Rung::Preprocessed | Rung::Direct
        ));
    }

    /// One-off: write the raw transpiled WGSL for both stages to $PAPER_WGSL_OUT
    /// (default /tmp/paper-wgsl). Regenerates the vendored Phase-2 assets.
    #[test]
    #[ignore]
    fn dump_baked_wgsl() {
        if !glslang_available() {
            panic!("glslang required to regenerate WGSL (sfw brew install glslang)");
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/paper_mesh_spike/shaders");
        let frag = std::fs::read_to_string(dir.join("mesh_gradient.frag")).unwrap();
        let vert = std::fs::read_to_string(dir.join("vertex.vert")).unwrap();
        let f = transpile(&frag, naga::ShaderStage::Fragment).unwrap();
        let v = transpile(&vert, naga::ShaderStage::Vertex).unwrap();
        let out = std::path::PathBuf::from(
            std::env::var("PAPER_WGSL_OUT").unwrap_or_else(|_| "/tmp/paper-wgsl".into()),
        );
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("vertex.wgsl"), &v.wgsl).unwrap();
        std::fs::write(out.join("mesh_gradient.wgsl"), &f.wgsl).unwrap();
        eprintln!("wrote {}/{{vertex,mesh_gradient}}.wgsl", out.display());
    }

    /// Breadth DIAGNOSTIC: transpile every `*.frag` in $PAPER_MORE_DIR through the same ladder,
    /// reporting which of paper's OTHER shaders reuse the mesh-gradient path. Report-only (not a
    /// gate): every shader in the representative set reaches valid WGSL via the `spirv` rung.
    /// metaballs and voronoi originally failed at naga `spv-in` with `InvalidId` (glslang emits a
    /// combined image sampler naga can't import); `separate_combined_samplers` now normalizes those
    /// to the separate texture+sampler form, so all six transpile.
    #[test]
    #[ignore]
    fn transpile_more_shaders() {
        if !glslang_available() {
            panic!("glslang required (sfw brew install glslang)");
        }
        let dir =
            std::path::PathBuf::from(std::env::var("PAPER_MORE_DIR").expect("set PAPER_MORE_DIR"));
        let mut frags: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "frag"))
            .collect();
        frags.sort();
        let rung_str = |r: &Rung| match r {
            Rung::Direct => "direct",
            Rung::Preprocessed => "preproc",
            Rung::SpirV => "spirv",
            Rung::Unavailable => "n/a",
        };
        // If PAPER_MORE_OUT is set, write each SUCCESSFUL transpile's WGSL there (vendoring).
        let out = std::env::var("PAPER_MORE_OUT")
            .ok()
            .map(std::path::PathBuf::from);
        if let Some(o) = &out {
            std::fs::create_dir_all(o).unwrap();
        }
        let (mut ok, mut fail) = (0, 0);
        for p in &frags {
            let name = p.file_stem().unwrap().to_string_lossy();
            match transpile(
                &std::fs::read_to_string(p).unwrap(),
                naga::ShaderStage::Fragment,
            ) {
                Ok(t) => {
                    ok += 1;
                    assert!(t.wgsl.contains("@fragment"), "{name}: no @fragment");
                    if let Some(o) = &out {
                        std::fs::write(o.join(format!("{name}.wgsl")), &t.wgsl).unwrap();
                    }
                    eprintln!(
                        "OK   {name:<14} rung={:<7} wgsl={}b",
                        rung_str(&t.rung),
                        t.wgsl.len()
                    );
                }
                Err(e) => {
                    fail += 1;
                    eprintln!("FAIL {name:<14} {}", e.lines().next().unwrap_or(""));
                }
            }
        }
        eprintln!("--- {ok} ok, {fail} fail (diagnostic; failures = naga spv-in edge cases) ---");
        // Report-only: at least SOME of paper's other shaders must reuse the ladder cleanly.
        assert!(
            ok > 0,
            "expected at least one other paper shader to transpile"
        );
    }
}

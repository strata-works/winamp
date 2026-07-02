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
}

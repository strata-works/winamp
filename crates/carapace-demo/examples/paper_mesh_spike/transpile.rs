// This module is the transpile-ladder scaffold consumed by `harness.rs` and
// `main.rs` in later tasks; several items (the `SpirV`/`Unavailable` rungs,
// the `module`/`info` fields, and the public functions themselves) have no
// caller yet within Task 2, so silence dead_code for the whole module rather
// than sprinkling per-item allows.
#![allow(dead_code)]

use naga::back::wgsl;
use naga::front::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

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
                Err(pre_err) => Err(format!("direct: {direct_err} | preprocessed: {pre_err}")),
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
}

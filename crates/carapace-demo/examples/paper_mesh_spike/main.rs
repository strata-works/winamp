mod harness;
mod transpile;

use std::path::Path;
use transpile::{Rung, transpile};

fn rung_str(r: &Rung) -> &'static str {
    match r {
        Rung::Direct => "direct",
        Rung::Preprocessed => "preproc",
        Rung::SpirV => "spirv",
        Rung::Unavailable => "n/a",
    }
}

fn wgpu_ok(gpu: &harness::Gpu, wgsl: &str) -> &'static str {
    if harness::wgpu_accepts(gpu, wgsl).is_ok() {
        "pass"
    } else {
        "fail"
    }
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
            let g2v = wgpu_ok(&gpu, &v.wgsl);
            let g2f = wgpu_ok(&gpu, &f.wgsl);
            println!("G2 vertex:   {g2v}\nG2 fragment: {g2f}");
            // G3
            let vf = harness::uniform_fields(&v.module);
            let ff = harness::uniform_fields(&f.module);
            let mut g3 = "ok";
            for (i, t) in [0.0_f32, 1.3, 2.6].iter().enumerate() {
                let p = out.join(format!("mesh_t{i}.png"));
                if let Err(e) =
                    harness::render_mesh(&gpu, &v.wgsl, &f.wgsl, &ff, &vf, 512, 512, *t, &p)
                {
                    g3 = "fail";
                    println!("G3 render t{i}: {e}");
                }
            }
            println!("G3 render: {g3} -> {}", out.display());
        }
        _ => {
            if let Err(e) = &vt {
                println!("G1 vertex FAIL: {}", e.lines().next().unwrap_or(""));
            }
            if let Err(e) = &ft {
                println!("G1 fragment FAIL: {}", e.lines().next().unwrap_or(""));
            }
        }
    }
}

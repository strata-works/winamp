//! In-process proof: a fake host serves level=0.6; the engine ticks once and we dump the frame.
//! Confirms engine + FfiHost + renderer compose without any IOSurface/Swift involved.
use std::ffi::{c_char, c_void, CStr};
use std::time::Duration;

use embed_spike::host::CarapaceHostVTable;
use embed_spike::render::{init_gpu, new_offscreen, readback_rgba, render_frame};

extern "C" fn get_num(_ctx: *mut c_void, key: *const c_char, out: *mut f64) -> bool {
    let k = unsafe { CStr::from_ptr(key) }.to_str().unwrap_or("");
    if k == "level" {
        unsafe { *out = 0.6 };
        true
    } else {
        false
    }
}

fn main() {
    let (w, h) = (240u32, 80u32);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skin");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let vtable = CarapaceHostVTable {
        ctx: std::ptr::null_mut(),
        get_num: Some(get_num),
        get_str: None,
        invoke: None,
    };
    let mut engine = carapace::engine::Engine::new(
        Box::new(embed_spike::host::FfiHost::new(vtable)),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .unwrap();

    let gpu = init_gpu();
    let mut renderer = carapace::render::Renderer::new(&gpu.device);
    let target = new_offscreen(&gpu.device, w, h);

    render_frame(&mut engine, &mut renderer, &gpu, &target.view, w, h, Duration::from_millis(16), true);
    let rgba = readback_rgba(&gpu, &target.tex, w, h);

    // The value bar (green ~120,230,80) must appear somewhere — assert non-empty + has a green-ish pixel.
    let has_green = rgba.chunks_exact(4).any(|p| p[1] > 180 && p[0] < 180 && p[2] < 160 && p[3] > 0);
    assert!(has_green, "expected the value bar to render");

    // Ensure target/ directory exists.
    std::fs::create_dir_all("target").unwrap();
    image::save_buffer("target/render_png.png", &rgba, w, h, image::ColorType::Rgba8).unwrap();
    println!("wrote target/render_png.png");
}

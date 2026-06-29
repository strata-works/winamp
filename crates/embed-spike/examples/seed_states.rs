//! Host-side (macOS) generator for the widget-sample's pre-rendered fallback PNGs.
//!
//! The carapace renderer (Vello) needs GPU INDIRECT_EXECUTION, which the iOS Simulator's
//! Metal does not support — so the live in-app render only runs on a real device. To still
//! demo the WidgetKit pipeline in the Simulator, the sample app bundles these host-rendered
//! PNGs and seeds them into the App Group when the live render fails.
//!
//! Run: `cargo run -p embed-spike --example seed_states`
//! Writes state-0.png … state-3.png into widget-sample/App/Seeded/.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("seed_states is macOS-only (needs a Metal device with INDIRECT_EXECUTION)");
}

#[cfg(target_os = "macos")]
fn main() {
    use std::ffi::CString;
    use std::path::PathBuf;

    use embed_spike::oneshot::carapace_render_png;

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let skin = manifest.join("skin");
    let out_dir = manifest.join("widget-sample/App/Seeded");
    std::fs::create_dir_all(&out_dir).expect("create Seeded dir");

    let skin_c = CString::new(skin.to_str().unwrap()).unwrap();
    let count = 4;
    for i in 0..count {
        let level = i as f64 / (count - 1) as f64; // 0.0 … 1.0
        let out = out_dir.join(format!("state-{i}.png"));
        let out_c = CString::new(out.to_str().unwrap()).unwrap();
        let ok = unsafe { carapace_render_png(skin_c.as_ptr(), 240, 80, level, out_c.as_ptr()) };
        assert!(ok, "render state {i} (level {level}) failed");
        println!("wrote {} (level {level:.2})", out.display());
    }
}

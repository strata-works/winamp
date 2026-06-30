//! Host-side (macOS) generator for the widget-sample's pre-rendered fallback PNG.
//!
//! The carapace renderer (Vello) needs GPU INDIRECT_EXECUTION, which the iOS Simulator's
//! Metal does not support — so the live in-app render only runs on a real device. To still
//! demo the WidgetKit pipeline in the Simulator, the sample app bundles this host-rendered
//! PNG and seeds it into the App Group when the live render fails.
//!
//! Renders the Headspace faceplate (shaped, transparent background → floats in the widget).
//!
//! Run: `cargo run -p embed-spike --example seed_states`
//! Writes faceplate.png into widget-sample/App/Seeded/.

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
    let skin = manifest.join("skin-headspace"); // native 342×394, transparent background
    let out_dir = manifest.join("widget-sample/App/Seeded");
    std::fs::create_dir_all(&out_dir).expect("create Seeded dir");

    let skin_c = CString::new(skin.to_str().unwrap()).unwrap();
    let out = out_dir.join("faceplate.png");
    let out_c = CString::new(out.to_str().unwrap()).unwrap();
    // 2× the native canvas for a crisp retina widget; state is unused by this static skin.
    let ok = unsafe { carapace_render_png(skin_c.as_ptr(), 684, 788, 0.0, out_c.as_ptr()) };
    assert!(ok, "faceplate render failed");
    println!("wrote {}", out.display());
}

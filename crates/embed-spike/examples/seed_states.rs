//! Host-side (macOS) generator for the widget-sample's pre-rendered fallback PNG.
//!
//! The carapace renderer (Vello) needs GPU INDIRECT_EXECUTION, which the iOS Simulator's
//! Metal does not support — so the live in-app render only runs on a real device. To still
//! demo the WidgetKit pipeline in the Simulator, the sample app bundles this host-rendered
//! PNG and seeds it into the App Group when the live render fails.
//!
//! Renders the "Now Playing" skin with sample data — proving the widget shows LIVE INFORMATION
//! (bound text + a seek bar), not just a static bitmap.
//!
//! Run: `cargo run -p embed-spike --example seed_states`
//! Writes nowplaying.png into widget-sample/App/Seeded/.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("seed_states is macOS-only (needs a Metal device with INDIRECT_EXECUTION)");
}

#[cfg(target_os = "macos")]
fn main() {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use embed_spike::oneshot::{render_skin_with_host, InfoHost};

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let skin = manifest.join("skin-nowplaying");
    let out_dir = manifest.join("widget-sample/App/Seeded");
    std::fs::create_dir_all(&out_dir).expect("create Seeded dir");

    let mut values = HashMap::new();
    values.insert("track".into(), "Midnight City".to_string());
    values.insert("artist".into(), "M83".to_string());
    values.insert("time".into(), "2:14 / 4:03".to_string());
    values.insert("position".into(), "0.55".to_string());

    let out = out_dir.join("nowplaying.png");
    // 2× the native 320×140 canvas for a crisp retina widget.
    let ok = render_skin_with_host(
        &skin,
        Box::new(InfoHost { values }),
        640,
        280,
        out.to_str().unwrap(),
    )
    .is_some();
    assert!(ok, "now-playing render failed");
    println!("wrote {}", out.display());
}

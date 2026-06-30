// The `oneshot` module these tests cover is gated to Apple targets (it needs Metal),
// so gate the whole test file the same way — otherwise it fails to compile on Linux CI.
#![cfg(any(target_os = "macos", target_os = "ios"))]

use std::collections::HashMap;
use std::ffi::CString;
use std::path::PathBuf;

use embed_spike::oneshot::{carapace_render_png, render_skin_with_host, InfoHost};

// Counts pixels whose green channel dominates (the value_fill bar is bright green
// {r=120,g=230,b=80} over a near-black {r=18,g=20,b=26} background).
fn green_pixels(path: &std::path::Path) -> u64 {
    let img = image::open(path).unwrap().to_rgba8();
    img.pixels()
        .filter(|p| p[1] > 150 && p[0] < 160 && p[2] < 130)
        .count() as u64
}

#[test]
fn render_png_writes_a_file_and_state_drives_the_bar() {
    let skin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skin");
    let skin_c = CString::new(skin.to_str().unwrap()).unwrap();

    let dir = std::env::temp_dir();
    let low = dir.join("spike_low.png");
    let high = dir.join("spike_high.png");
    let low_c = CString::new(low.to_str().unwrap()).unwrap();
    let high_c = CString::new(high.to_str().unwrap()).unwrap();

    let ok_low = unsafe { carapace_render_png(skin_c.as_ptr(), 240, 80, 0.15, low_c.as_ptr()) };
    let ok_high = unsafe { carapace_render_png(skin_c.as_ptr(), 240, 80, 0.90, high_c.as_ptr()) };

    assert!(ok_low && ok_high, "render should succeed");
    assert!(low.exists() && high.exists(), "PNGs should be written");

    let (g_low, g_high) = (green_pixels(&low), green_pixels(&high));
    assert!(g_low > 0, "low state should still draw some bar");
    assert!(
        g_high > g_low * 2,
        "high state bar must be much larger: low={g_low} high={g_high}"
    );
}

// Non-black pixels approximate "rendered content" (text + bar) over the dark backdrop.
fn lit_pixels(path: &std::path::Path) -> u64 {
    let img = image::open(path).unwrap().to_rgba8();
    img.pixels()
        .filter(|p| p[0] as u16 + p[1] as u16 + p[2] as u16 > 220)
        .count() as u64
}

#[test]
fn render_info_draws_live_data_and_position_drives_the_bar() {
    let skin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skin-nowplaying");
    let dir = std::env::temp_dir();

    let render = |position: &str, out: &std::path::Path| {
        let mut values = HashMap::new();
        values.insert("track".to_string(), "Midnight City".to_string());
        values.insert("artist".to_string(), "M83".to_string());
        values.insert("time".to_string(), "2:14 / 4:03".to_string());
        values.insert("position".to_string(), position.to_string());
        render_skin_with_host(
            &skin,
            Box::new(InfoHost { values }),
            320,
            140,
            out.to_str().unwrap(),
        )
        .is_some()
    };

    let low = dir.join("np_low.png");
    let high = dir.join("np_high.png");
    assert!(
        render("0.1", &low) && render("0.95", &high),
        "info render should succeed"
    );
    assert!(low.exists() && high.exists(), "PNGs should be written");

    // Bound text + the seek bar produce lit pixels; a fuller seek position lights more of the bar.
    let (l_low, l_high) = (lit_pixels(&low), lit_pixels(&high));
    assert!(l_low > 0, "bound text/bar should render something");
    assert!(
        l_high > l_low,
        "higher position must light more of the seek bar: low={l_low} high={l_high}"
    );
}

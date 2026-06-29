use std::ffi::CString;
use std::path::PathBuf;

use embed_spike::oneshot::carapace_render_png;

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

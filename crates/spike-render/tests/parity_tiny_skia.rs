use hittest::{l_shape, ring};
use spike_render::tiny_skia_backend::TinySkiaRenderer;
use spike_render::{parity_check, Renderer};

const FILL: [u8; 4] = [255, 0, 0, 255];
const BG: [u8; 4] = [0, 0, 0, 255];

#[test]
fn tiny_skia_matches_hittest_on_l_shape() {
    let mut r = TinySkiaRenderer;
    let pm = r.render(&l_shape(), (200, 200), FILL, BG);
    let report = parity_check(&l_shape(), &pm, FILL, BG);
    assert!(report.checked > 10_000, "too few unambiguous pixels checked: {}", report.checked);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}

#[test]
fn tiny_skia_matches_hittest_on_ring() {
    let mut r = TinySkiaRenderer;
    let pm = r.render(&ring(), (200, 200), FILL, BG);
    let report = parity_check(&ring(), &pm, FILL, BG);
    assert!(report.mismatches.is_empty(), "mismatches: {:?}", report.mismatches);
}

use hittest::{l_shape, ring};
use spike_render::tiny_skia_backend::TinySkiaRenderer;
use spike_render::vello_backend::VelloRenderer;
use spike_render::wgpu_backend::WgpuRenderer;
use spike_render::{parity_check, Renderer};

const FILL: [u8; 4] = [255, 0, 0, 255];
const BG: [u8; 4] = [0, 0, 0, 255];

fn assert_clean(r: &mut dyn Renderer) {
    for region in [l_shape(), ring()] {
        let pm = r.render(&region, (200, 200), FILL, BG);
        let report = parity_check(&region, &pm, FILL, BG);
        assert!(
            report.mismatches.is_empty(),
            "{} mismatched on a region: {:?}",
            r.name(),
            report.mismatches
        );
    }
}

#[test]
fn all_backends_pass_the_gate() {
    assert_clean(&mut TinySkiaRenderer);
    assert_clean(&mut WgpuRenderer::new());
    assert_clean(&mut VelloRenderer::new());
}

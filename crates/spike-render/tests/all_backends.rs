use hittest::{l_shape, ring};
use spike_render::vello_backend::VelloRenderer;
use spike_render::{Renderer, parity_check};

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
fn chosen_backend_passes_the_gate() {
    // Phase 0 chose vello; the other candidates were pruned after the decision.
    assert_clean(&mut VelloRenderer::new());
}

use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::scene::Node;
use carapace::vocab::VocabRegistry;

// A full-bleed content view anchored to all four edges, in a 100x100 design.
const SKIN: &str = "view{ id='app', x=10, y=10, w=80, h=80, \
    anchor = { 'left','right','top','bottom' } }\n";

#[test]
fn layout_stretches_view_and_sets_canvas_to_logical() {
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    let resolved = e.layout(200.0, 150.0);
    assert_eq!(resolved.canvas, (200, 150)); // canvas = logical size -> render scales by DPI only
    match &resolved.nodes[0] {
        Node::View { dest, .. } => {
            // gaps left/top=10, right=100-90=10, bottom=10. -> x=10,y=10,w=180,h=130.
            assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 180.0, 130.0));
        }
        _ => panic!("expected a View node"),
    }
}

#[test]
fn layout_at_design_size_is_identity() {
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    let resolved = e.layout(100.0, 100.0);
    match &resolved.nodes[0] {
        Node::View { dest, .. } => {
            assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 80.0, 80.0))
        }
        _ => panic!(),
    }
}

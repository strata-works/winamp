use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::layout::Anchors;
use carapace::vocab::VocabRegistry;

const SKIN: &str = "\
    view{ id='a', x=0, y=0, w=10, h=10, anchor = { 'left', 'right', 'top', 'bottom' } }\n\
    view{ id='b', x=0, y=0, w=10, h=10 }\n";

#[test]
fn anchors_parsed_parallel_to_nodes() {
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    let anchors = e.scene_anchors();
    assert_eq!(anchors.len(), e.scene().nodes.len());
    assert_eq!(
        anchors[0],
        Anchors {
            left: true,
            right: true,
            top: true,
            bottom: true,
            min: None,
        }
    );
    assert_eq!(anchors[1], Anchors::TOP_LEFT); // no anchor attr -> default
}

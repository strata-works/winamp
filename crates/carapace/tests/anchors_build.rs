use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::layout::{Anchors, Frac};
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
            max: None,
            frac: Frac::EMPTY,
        }
    );
    assert_eq!(anchors[1], Anchors::TOP_LEFT); // no anchor attr -> default
}

#[test]
fn parses_frac_and_max_without_requiring_anchor() {
    // Element sets frac/max but NO anchor attr — must still be honored (defaults to top-left edges).
    let src = r#"
        fill{ path = rect{ x = 0, y = 0, w = 100, h = 50 },
              color = { r = 0, g = 0, b = 0 },
              frac = { w = 0.3, x = 0.1 }, max = { w = 320 } }
    "#;
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(src, (100, 100)),
    )
    .unwrap();
    let anchors = e.scene_anchors();
    let a = anchors[0];
    assert_eq!(a.frac.w, Some(0.3));
    assert_eq!(a.frac.x, Some(0.1));
    assert_eq!(a.frac.y, None);
    assert_eq!(a.max, Some((320.0, f32::INFINITY)));
    // no anchor attr -> top-left edges, but min/max/frac still read
    assert!(a.left && a.top && !a.right && !a.bottom);
}

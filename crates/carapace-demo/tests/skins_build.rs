use std::path::Path;

use carapace::engine::Engine;
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;

fn build(skin_dir: &str) -> usize {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skins")
        .join(skin_dir);
    let (_m, source) = carapace::skin::load_dir(&dir).expect("load skin dir");
    let e = Engine::new(Box::new(DemoHost::new()), VocabRegistry::base(), source)
        .expect("skin builds into a scene");
    e.scene().nodes.len()
}

#[test]
fn classic_uses_shared_geometry_and_a_vertical_meter() {
    use carapace::engine::Engine;
    use carapace::scene::{FillDir, Node, Pt};
    use carapace::vocab::VocabRegistry;
    use carapace_demo::demo_host::DemoHost;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/classic");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let e = Engine::new(Box::new(DemoHost::new()), VocabRegistry::base(), source).unwrap();
    let nodes = &e.scene().nodes;
    // shared geometry: at least one Hotspot emitted by a fill{on_press}
    assert!(
        nodes.iter().any(|n| matches!(n, Node::Hotspot { .. })),
        "has hotspots"
    );
    // a vertical meter
    assert!(
        nodes.iter().any(|n| matches!(
            n,
            Node::ValueFill {
                direction: FillDir::Up,
                ..
            }
        )),
        "has an upward value_fill meter"
    );
    // the play button (a fill{on_press}) is clickable at its center
    assert!(
        e.scene().hit(Pt { x: 55.0, y: 55.0 }).is_some(),
        "play button hotspot is hittable"
    );
}

#[test]
fn minimal_builds() {
    assert!(build("minimal") >= 3);
}

#[test]
fn headspace_reference_builds_with_bitmap() {
    use carapace::scene::Node;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/reference");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let e = carapace::engine::Engine::new(
        Box::new(carapace_demo::demo_host::DemoHost::new()),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .unwrap();
    let nodes = &e.scene().nodes;
    assert!(
        nodes.iter().any(|n| matches!(n, Node::Image { .. })),
        "draws the headspace bitmap"
    );
    assert!(
        nodes.iter().any(|n| matches!(n, Node::Hotspot { .. })),
        "has interactive hotspots"
    );
    assert!(
        nodes.iter().any(|n| matches!(n, Node::ValueFill { .. })),
        "has the live seek bar"
    );
    use carapace::scene::Paint;
    assert!(
        nodes.iter().any(|n| matches!(
            n,
            Node::Fill {
                paint: Paint::Gradient(_),
                ..
            }
        )),
        "reference skin now has gradient sheen/glossy accents"
    );
    assert!(
        nodes.iter().any(|n| matches!(n, Node::Text { .. })),
        "reference skin has a text readout"
    );
}

fn engine_with_transport(skin_dir: &str) -> carapace::engine::Engine {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skins")
        .join(skin_dir);
    let (_m, source) = carapace::skin::load_dir(&dir).expect("load skin dir");
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(carapace_demo::transport::TransportPrim));
    Engine::new(Box::new(DemoHost::new()), reg, source).expect("skin builds")
}

#[test]
fn transport_extension_builds_and_play_click_toggles_host() {
    use carapace::engine::PointerEvent;
    use carapace::scene::Node;
    use carapace::state::StateValue;
    use std::time::Duration;

    let mut e = engine_with_transport("transport");
    let nodes = e.scene().nodes.clone();
    assert!(
        nodes.iter().any(|n| matches!(n, Node::Hotspot { .. })),
        "has a hotspot"
    );
    assert!(
        nodes.iter().any(|n| matches!(n, Node::ValueFill { .. })),
        "has a seek bar"
    );

    // play button rect = (20,20,40,40) -> center (40,40); clicking fires toggle_play.
    assert_eq!(e.state("playing"), Some(StateValue::Bool(false)));
    e.handle_pointer(
        carapace::scene::Pt { x: 40.0, y: 40.0 },
        PointerEvent::Press,
    );
    e.update(Duration::ZERO);
    assert_eq!(
        e.state("playing"),
        Some(StateValue::Bool(true)),
        "transport play toggled the host"
    );
}

#[test]
fn skins_declare_window_controls() {
    use carapace::scene::Node;
    // Every vector skin must build AND reference the window-control actions (a skin naming an
    // un-allowlisted action fails to load), and expose hotspots for them.
    for skin in ["classic", "minimal", "transport"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skins")
            .join(skin);
        let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(carapace_demo::transport::TransportPrim));
        let e = Engine::new(Box::new(DemoHost::new()), reg, source)
            .unwrap_or_else(|err| panic!("{skin} failed to build: {err:?}"));
        let hotspots = e
            .scene()
            .nodes
            .iter()
            .filter(|n| matches!(n, Node::Hotspot { .. }))
            .count();
        assert!(
            hotspots >= 3,
            "{skin} should have drag + min + close hotspots, found {hotspots}"
        );
    }
}

#[test]
fn minimal_has_a_sweep_gradient() {
    use carapace::scene::{Gradient, Node, Paint};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/minimal");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let e = carapace::engine::Engine::new(
        Box::new(carapace_demo::demo_host::DemoHost::new()),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .unwrap();
    assert!(
        e.scene().nodes.iter().any(|n| matches!(
            n,
            Node::Fill {
                paint: Paint::Gradient(Gradient::Sweep { .. }),
                ..
            }
        )),
        "minimal skin shows a sweep gradient"
    );
}

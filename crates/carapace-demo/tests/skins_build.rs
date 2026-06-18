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
fn classic_builds() {
    assert!(build("classic") >= 4);
}

#[test]
fn minimal_builds() {
    assert!(build("minimal") >= 3);
}

#[test]
fn headspace_reference_builds() {
    // The reference skin is intentionally busy — a render/perf stress scene.
    let n = build("reference");
    assert!(
        n >= 15,
        "headspace homage should be a busy scene, got {n} nodes"
    );
}

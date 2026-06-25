//! The spike skin builds on the real engine and exposes the binding + action the FFI host needs.
use std::path::Path;

#[test]
fn spike_skin_builds_and_binds_level_and_toggle() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skin");
    let (_manifest, source) = carapace::skin::load_dir(&dir).expect("load spike skin");

    // A host that allows the "toggle" action and returns a value for "level",
    // so the skin builds and the binding resolves.
    let host = carapace::fixture::FixtureHost::new();
    let engine = carapace::engine::Engine::new(
        Box::new(host),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .expect("engine builds the spike skin");

    // The scene has nodes (fill + value_fill + hotspot) — the script ran.
    assert!(!engine.scene().nodes.is_empty(), "skin produced a scene");
}

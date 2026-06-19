use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::scene::Pt;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;
use proptest::prelude::*;

const SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
"#;

fn src(s: &str) -> SkinSource {
    SkinSource::inline(s, (200, 200))
}

fn engine() -> Engine {
    Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        src(SKIN),
    )
    .unwrap()
}

#[derive(Clone, Debug)]
enum Op {
    Click(f32, f32),
    Tick(u64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0.0f32..200.0, 0.0f32..200.0).prop_map(|(x, y)| Op::Click(x, y)),
        (0u64..1000).prop_map(Op::Tick),
    ]
}

proptest! {
    // Invariant: no sequence of clicks/ticks ever panics.
    #[test]
    fn never_panics(ops in proptest::collection::vec(op_strategy(), 0..40)) {
        let mut e = engine();
        for op in ops {
            match op {
                Op::Click(x, y) => e.handle_pointer(Pt { x, y }, PointerEvent::Press),
                Op::Tick(ms) => e.update(Duration::from_millis(ms)),
            }
        }
        // also drain anything queued
        e.update(Duration::ZERO);
    }

    // Invariant: a click never mutates host state before the drain.
    #[test]
    fn no_mutation_before_drain(x in 0.0f32..200.0, y in 0.0f32..200.0) {
        let mut e = engine();
        let before = e.state("on");
        e.handle_pointer(Pt { x, y }, PointerEvent::Press); // NO update
        prop_assert_eq!(e.state("on"), before);
    }
}

// Invariant: a swap to a broken skin always leaves the prior scene intact (transactional).
#[test]
fn transactional_swap_invariant() {
    let mut e = engine();
    let before = e.scene().summary();
    for bad in [
        "not lua {{{",
        "frobnicate{}",
        "host.does_not_exist()",
        "io.read()",
    ] {
        e.handle_command(Command::Swap(src(bad)));
        e.update(Duration::ZERO);
        assert_eq!(
            e.scene().summary(),
            before,
            "broken swap `{bad}` changed the scene"
        );
    }
}

// Invariant: the sandbox blocks capability globals for any skin built through the engine.
#[test]
fn sandbox_blocks_capabilities_invariant() {
    for bad in [
        "io.write('x')",
        "os.time()",
        "require('os')",
        "load('return 1')",
    ] {
        let r = Engine::new(
            Box::new(FixtureHost::new()),
            VocabRegistry::base(),
            src(bad),
        );
        assert!(r.is_err(), "sandbox failed to block `{bad}`");
    }
}

// Sanity: a real click DOES toggle after a drain (so the no-mutation test isn't vacuous).
#[test]
fn click_then_drain_toggles() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)));
}

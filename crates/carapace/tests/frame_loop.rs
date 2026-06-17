use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::scene::Pt;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;

fn src(s: &str) -> SkinSource {
    SkinSource { lua_src: s.to_string(), canvas: (200, 200) }
}

// A skin whose hotspot toggles, plus a value_fill bound to "level".
const TOGGLE_SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
    value_fill{ path={{x=0,y=120},{x=200,y=120},{x=200,y=140},{x=0,y=140}},
                value='level', color={r=1,g=2,b=3} }
"#;

fn engine() -> Engine {
    Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), src(TOGGLE_SKIN)).unwrap()
}

#[test]
fn click_enqueues_then_drain_applies() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press); // enqueues toggle
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)), "not applied before drain");
    e.update(Duration::ZERO); // drain
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)), "applied at drain");
}

#[test]
fn click_in_empty_area_is_a_noop() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 5.0, y: 130.0 }, PointerEvent::Press); // value_fill, not a hotspot
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)));
}

#[test]
fn tick_advances_state_after_drain() {
    let mut e = engine();
    e.update(Duration::from_secs_f32(0.25));
    assert_eq!(e.state("level"), Some(StateValue::Scalar(0.25)));
}

#[test]
fn double_click_in_one_frame_applies_twice() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO); // two toggles → back to false
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)), "no dedup; two toggles net to start");
}

#[test]
fn swap_preserves_state() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::from_secs_f32(0.3)); // on=true, level=0.3
    e.handle_command(Command::Swap(src(
        "value_fill{ path={{x=0,y=0},{x=200,y=0},{x=200,y=10}}, value='level', color={r=0,g=0,b=0} }",
    )));
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)), "state survived swap");
    assert_eq!(e.scene().nodes.len(), 1, "scene is the new skin's");
}

#[test]
fn failed_swap_keeps_current_scene() {
    let mut e = engine();
    let before = e.scene().nodes.len();
    e.handle_command(Command::Swap(src("not lua {{{")));
    e.update(Duration::ZERO);
    assert_eq!(e.scene().nodes.len(), before, "failed swap left the prior scene intact");
}

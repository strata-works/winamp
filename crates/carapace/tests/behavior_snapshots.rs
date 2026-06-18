use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::{FixtureHost, OtherFixtureHost};
use carapace::host::Host;
use carapace::scene::Pt;
use carapace::vocab::VocabRegistry;

enum Step {
    Click(f32, f32),
    Cmd(Command),
    Tick(u64), // milliseconds
}

fn src(s: &str) -> SkinSource {
    SkinSource {
        lua_src: s.to_string(),
        canvas: (200, 200),
    }
}

/// Run a scenario and return a full trajectory string: the scene summary + the
/// declared state keys, captured after each step.
fn trajectory(host: Box<dyn Host>, skin: &str, state_keys: &[&str], steps: Vec<Step>) -> String {
    let mut e = Engine::new(host, VocabRegistry::base(), src(skin)).unwrap();
    let snap = |e: &Engine| -> String {
        let states: Vec<String> = state_keys
            .iter()
            .map(|k| format!("{}={:?}", k, e.state(k)))
            .collect();
        format!(
            "scene:\n{}\nstate: {}",
            e.scene().summary(),
            states.join(" ")
        )
    };
    let mut out = format!("=== step 0 (initial) ===\n{}\n", snap(&e));
    for (i, step) in steps.into_iter().enumerate() {
        match step {
            Step::Click(x, y) => {
                e.handle_pointer(Pt { x, y }, PointerEvent::Press);
                e.update(Duration::ZERO);
            }
            Step::Cmd(cmd) => {
                e.handle_command(cmd);
                e.update(Duration::ZERO);
            }
            Step::Tick(ms) => e.update(Duration::from_millis(ms)),
        }
        out.push_str(&format!("=== step {} ===\n{}\n", i + 1, snap(&e)));
    }
    out
}

const TOGGLE_SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
    value_fill{ path={{x=0,y=120},{x=200,y=120},{x=200,y=140},{x=0,y=140}},
                value='level', color={r=1,g=2,b=3} }
"#;

#[test]
fn snapshot_click_then_tick() {
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "level"],
        vec![
            Step::Click(50.0, 50.0),
            Step::Tick(250),
            Step::Click(50.0, 50.0),
        ],
    );
    insta::assert_snapshot!("click_then_tick", t);
}

#[test]
fn snapshot_swap_preserves_state() {
    let other = "value_fill{ path={{x=0,y=0},{x=200,y=0},{x=200,y=10}}, value='level', color={r=9,g=9,b=9} }";
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "level"],
        vec![
            Step::Click(50.0, 50.0),
            Step::Tick(300),
            Step::Cmd(Command::Swap(src(other))),
        ],
    );
    insta::assert_snapshot!("swap_preserves_state", t);
}

#[test]
fn snapshot_failed_swap_keeps_scene() {
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "level"],
        vec![Step::Cmd(Command::Swap(src("not lua {{{")))],
    );
    insta::assert_snapshot!("failed_swap_keeps_scene", t);
}

#[test]
fn snapshot_switch_host_resets() {
    let noop_skin =
        "region{ path={{x=0,y=0},{x=50,y=0},{x=50,y=50}}, on_press=function() host.noop() end }";
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "flag"],
        vec![
            Step::Click(50.0, 50.0),
            Step::Cmd(Command::SwitchHost {
                host: Box::new(OtherFixtureHost::new()),
                skin: src(noop_skin),
            }),
            Step::Click(10.0, 10.0),
        ],
    );
    insta::assert_snapshot!("switch_host_resets", t);
}

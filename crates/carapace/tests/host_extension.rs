// Proves the host-extension seam through the PUBLIC api: an external Primitive impl that binds
// host actions via `host_action`, driven end-to-end through Engine + FixtureHost.
use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::mlua::Table;
use carapace::scene::{Node, Pt, region_of};
use carapace::state::StateValue;
use carapace::vocab::{BuildContext, BuildError, Primitive, VocabRegistry};

// A 100x100 hotspot bound to a configurable host action via host_action.
struct ActionButton {
    id: &'static str,
    action: &'static str,
}
impl Primitive for ActionButton {
    fn id(&self) -> &str {
        self.id
    }
    fn build(&self, _a: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let path = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 100.0, y: 0.0 },
            Pt { x: 100.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let hid = ctx.host_action(self.action, vec![]);
        Ok(vec![Node::Hotspot {
            region: region_of(&path),
            on_press: hid,
        }])
    }
}

fn engine_with(prim: ActionButton, lua: &str) -> Engine {
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(prim));
    Engine::new(
        Box::new(FixtureHost::new()),
        reg,
        SkinSource::inline(lua, (100, 100)),
    )
    .unwrap()
}

#[test]
fn extension_host_action_fires_through_the_drain() {
    // FixtureHost: `toggle` flips `on`.
    let mut e = engine_with(
        ActionButton {
            id: "toggler",
            action: "toggle",
        },
        "toggler{}",
    );
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)));
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(
        e.state("on"),
        Some(StateValue::Bool(true)),
        "extension fired the host action"
    );
}

#[test]
fn extension_unregistered_action_is_dropped_not_panicked() {
    // FixtureHost has no `frobnicate` action -> dropped at drain, no state change, no panic.
    let mut e = engine_with(
        ActionButton {
            id: "bad",
            action: "frobnicate",
        },
        "bad{}",
    );
    let before = e.state("on");
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(
        e.state("on"),
        before,
        "unregistered action left host state unchanged"
    );
}

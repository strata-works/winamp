use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::{Engine, PointerEvent};
use carapace::host::{ActionSpec, Host, Value};
use carapace::scene::Pt;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;

struct SeekHost {
    last: Rc<RefCell<Option<(String, f64)>>>,
}
impl Host for SeekHost {
    fn name(&self) -> &str {
        "seek-test"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, _key: &str) -> Option<StateValue> {
        Some(StateValue::Scalar(0.0))
    }
    fn actions(&self) -> &[ActionSpec] {
        &[ActionSpec { name: "seek" }]
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        let n = match args.first() {
            Some(Value::Num(n)) => *n,
            _ => -1.0,
        };
        *self.last.borrow_mut() = Some((action.to_string(), n));
    }
}

const SKIN: &str =
    "scrub{ x=0, y=0, w=100, h=20, value='position', on_seek='seek', color={r=1,g=2,b=3} }";

#[test]
fn clicking_a_scrub_invokes_on_seek_with_fraction() {
    let last = Rc::new(RefCell::new(None));
    let mut engine = Engine::new(
        Box::new(SeekHost { last: last.clone() }),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 20)),
    )
    .unwrap();

    // Click at x=75 of a 100-wide bar → fraction 0.75.
    engine.handle_pointer_resolved(100.0, 20.0, Pt { x: 75.0, y: 10.0 }, PointerEvent::Press);
    engine.update(Duration::from_millis(0));

    assert_eq!(*last.borrow(), Some(("seek".to_string(), 0.75)));
}

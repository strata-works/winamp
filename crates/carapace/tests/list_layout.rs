use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::scene::Node;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;

struct ListHost {
    rows: Vec<Row>,
}
impl Host for ListHost {
    fn name(&self) -> &str {
        "list-test"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, _key: &str) -> Option<StateValue> {
        None
    }
    fn actions(&self) -> &[ActionSpec] {
        &[]
    }
    fn invoke(&mut self, _action: &str, _args: &[Value]) {}
    fn rows(&self, _collection: &str) -> Vec<Row> {
        self.rows.clone()
    }
}

fn name_row(n: &str) -> Row {
    Row::new().set("name", StateValue::Str(n.into()))
}

const SKIN: &str = "list{ collection='entries', x=0, y=0, w=100, h=80, row_height=20, \
    on_select='open', template={ { bind='name', x=4, y=2, size=12, color={r=1,g=2,b=3} } } }";

#[test]
fn layout_expands_rows_and_clamps_to_visible() {
    // 5 rows, region height 80 / row_height 20 = 4 visible -> clamp to 4.
    let host = ListHost {
        rows: vec![
            name_row("a"),
            name_row("b"),
            name_row("c"),
            name_row("d"),
            name_row("e"),
        ],
    };
    let engine = Engine::new(
        Box::new(host),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();

    let scene = engine.layout(100.0, 100.0);

    // 1 retained List node (count=4) + 4 Text rows (1 cell each).
    let list_count = scene
        .nodes
        .iter()
        .find_map(|n| match n {
            Node::List { count, .. } => Some(*count),
            _ => None,
        })
        .expect("List node retained");
    assert_eq!(list_count, 4, "clamped to 4 visible rows");

    let texts: Vec<&str> = scene
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text {
                content: carapace::scene::TextContent::Static(s),
                ..
            } => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec!["a", "b", "c", "d"],
        "first 4 rows expanded in order"
    );
}

use std::cell::RefCell;
use std::rc::Rc;

struct RecordHost {
    rows: Vec<Row>,
    last: Rc<RefCell<Option<(String, f64)>>>,
}
impl Host for RecordHost {
    fn name(&self) -> &str {
        "record"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, _key: &str) -> Option<StateValue> {
        None
    }
    fn actions(&self) -> &[ActionSpec] {
        &[ActionSpec { name: "open" }]
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        let n = match args.first() {
            Some(Value::Num(n)) => *n,
            _ => -1.0,
        };
        *self.last.borrow_mut() = Some((action.to_string(), n));
    }
    fn rows(&self, _collection: &str) -> Vec<Row> {
        self.rows.clone()
    }
}

#[test]
fn clicking_a_row_invokes_on_select_with_index() {
    let last = Rc::new(RefCell::new(None));
    let host = RecordHost {
        rows: vec![name_row("a"), name_row("b"), name_row("c")],
        last: last.clone(),
    };
    let mut engine = Engine::new(
        Box::new(host),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();

    // Row 1 spans y in [20, 40); click at y=30, within the region's x-range.
    engine.handle_pointer_resolved(
        100.0,
        100.0,
        carapace::scene::Pt { x: 50.0, y: 30.0 },
        carapace::engine::PointerEvent::Press,
    );
    engine.update(Duration::from_millis(0));

    assert_eq!(*last.borrow(), Some(("open".to_string(), 1.0)));
}

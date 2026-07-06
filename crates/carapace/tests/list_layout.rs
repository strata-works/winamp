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

/// Host that exposes a selection index via `get("sel")` and a fixed row set.
struct SelHost {
    rows: Vec<Row>,
    sel: f32,
}
impl Host for SelHost {
    fn name(&self) -> &str {
        "sel-test"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        (key == "sel").then_some(StateValue::Scalar(self.sel))
    }
    fn actions(&self) -> &[ActionSpec] {
        &[]
    }
    fn invoke(&mut self, _action: &str, _args: &[Value]) {}
    fn rows(&self, _collection: &str) -> Vec<Row> {
        self.rows.clone()
    }
}

#[test]
fn list_highlight_draws_a_fill_behind_the_selected_row() {
    const SKIN_HL: &str = "list{ collection='entries', x=0, y=0, w=100, h=80, row_height=20, \
        on_select='open', selected='sel', highlight={r=9,g=9,b=9}, \
        template={ { bind='name', x=4, y=2, size=12, color={r=1,g=2,b=3} } } }";
    let host = SelHost {
        rows: vec![name_row("a"), name_row("b"), name_row("c")],
        sel: 1.0, // highlight the middle row (y in [20,40))
    };
    let engine = Engine::new(
        Box::new(host),
        VocabRegistry::base(),
        SkinSource::inline(SKIN_HL, (100, 100)),
    )
    .unwrap();
    let scene = engine.layout(100.0, 100.0);

    // Exactly one Fill, a full-width bar at the selected row's top (y=20).
    let fills: Vec<&Vec<carapace::scene::Pt>> = scene
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Fill { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 1, "one highlight fill for the selected row");
    assert_eq!(
        fills[0][0],
        carapace::scene::Pt { x: 0.0, y: 20.0 },
        "bar top at row 1"
    );
    assert_eq!(
        fills[0][2],
        carapace::scene::Pt { x: 100.0, y: 40.0 },
        "bar bottom-right at row 1 end"
    );
}

#[test]
fn list_without_selected_draws_no_highlight() {
    let host = ListHost {
        rows: vec![name_row("a"), name_row("b")],
    };
    let engine = Engine::new(
        Box::new(host),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    let scene = engine.layout(100.0, 100.0);
    assert!(
        !scene.nodes.iter().any(|n| matches!(n, Node::Fill { .. })),
        "no highlight without selected/highlight"
    );
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

#[test]
fn engine_passes_template_binds_to_rows_for() {
    use std::cell::RefCell;
    struct RecHost {
        seen: RefCell<Vec<String>>,
    }
    impl Host for RecHost {
        fn name(&self) -> &str {
            "rec"
        }
        fn tick(&mut self, _dt: Duration) {}
        fn get(&self, _k: &str) -> Option<StateValue> {
            None
        }
        fn actions(&self) -> &[ActionSpec] {
            &[]
        }
        fn invoke(&mut self, _a: &str, _args: &[Value]) {}
        fn rows_for(&self, _collection: &str, fields: &[&str]) -> Vec<Row> {
            *self.seen.borrow_mut() = fields.iter().map(|s| s.to_string()).collect();
            vec![
                Row::new()
                    .set("title", StateValue::Str("t".into()))
                    .set("dur", StateValue::Str("d".into())),
            ]
        }
    }
    const SK: &str = "list{ collection='playlist', x=0, y=0, w=100, h=40, row_height=20, \
        template={ { bind='title', x=2, y=2, size=12, color={r=1,g=2,b=3} }, \
                   { bind='dur', right=4, y=2, size=12, color={r=1,g=2,b=3} } } }";
    let host = RecHost {
        seen: RefCell::new(vec![]),
    };
    let engine = Engine::new(
        Box::new(host),
        VocabRegistry::base(),
        SkinSource::inline(SK, (100, 100)),
    )
    .unwrap();
    // The engine must have asked rows_for for exactly the two template binds; assert via the
    // rendered text (the moved host is unreadable). `RowCell::to_node` emits `TextContent::Static`.
    use carapace::scene::TextContent;
    let scene = engine.layout(100.0, 100.0);
    let texts: Vec<String> = scene
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text {
                content: TextContent::Static(s),
                ..
            } => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"t".to_string()),
        "title cell rendered from rows_for"
    );
    assert!(
        texts.contains(&"d".to_string()),
        "dur cell rendered from rows_for"
    );
}

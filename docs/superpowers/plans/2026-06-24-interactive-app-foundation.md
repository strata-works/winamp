# Interactive-App Foundation (Spec 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the Spec-1 frame-skin app-shell (static mock rows) into a live, clickable, navigable two-pane read-only file browser, by adding a dynamic `list{}` engine primitive, demo-side input routing into the `view{}` region, and a `FileBrowserHost`.

**Architecture:** A new `Node::List` parses once at skin-load but expands per-frame inside `Engine::layout()` (after anchor resolution) by calling `Host::rows(collection)` and emitting concrete per-row `Node::Text` nodes, while retaining a lightweight `Node::List` carrying the current row count for hit-testing. Row clicks are resolved by `Scene::hit_row()` (row arithmetic) and dispatched through the existing host-action queue. The demo translates outer clicks into the nested shell engine's coordinate space; the engine gains no new input primitive.

**Tech Stack:** Rust workspace (`carapace` engine crate + `carapace-demo` binary), `mlua` (Lua skin DSL), `wgpu`/`vello` (render). Tests use `cargo test`; the file browser is tested over an in-memory `FileSystem` mock (no disk, no new dependencies).

## Global Constraints

- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>` (use `git -c user.name=... -c user.email=...`).
- **No new third-party dependencies.** Spec 2 uses only `std` + existing crates. (If that ever changes, the first fetch of a new crate MUST run as `sfw cargo ...`.)
- **CI gates (run before every commit that touches engine code):**
  - `cargo fmt --all --check`
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`
  - `cargo test --locked --workspace`
  - GPU regression (golden snapshots): `cargo test --locked -p carapace --features gpu-tests --test render_offscreen`
- **Gadget-skin golden snapshots must stay byte-identical** — no change to the render path for existing node kinds.
- **Performance is first-class:** no per-frame Lua execution for list rows; template parsed once; row expansion is plain Rust.
- **Read-only filesystem throughout.** No writes, no opening files in external apps. `FileBrowserHost` is sandboxed to a `root` and never traverses above it.

---

## File Structure

**Engine crate (`crates/carapace/`):**
- `src/host.rs` — add `Row` type + `Host::rows()` default method (Task 1).
- `src/scene.rs` — add `Node::List`, `RowCell`/`RowTemplate`, `RowCell::to_node()`, `Scene::hit_row()`, `summary()` arm (Tasks 2, 4) + `RowCell::to_node` (Task 5).
- `src/layout.rs` — `node_bbox`/`transform_node` arms for `Node::List` (Task 2).
- `src/render.rs` — `Node::List` no-op arm in `draw()` (Task 2).
- `src/vocab.rs` — `ListPrim` parse + registration (Task 3).
- `src/engine.rs` — `expand_lists()` in `layout()`; row-hit dispatch in `handle_pointer_resolved()` (Tasks 5, 6).
- `tests/list_layout.rs` — new integration test for expansion + row-hit dispatch (Tasks 5, 6).

**Demo crate (`crates/carapace-demo/`):**
- `src/file_browser_host.rs` — NEW: `FileSystem`/`StdFs`/`MockFs`, `DirEntryInfo`, `FileBrowserHost` (Tasks 7, 8).
- `src/main.rs` — register the module; replace `APP_SHELL`; build `FileBrowserHost`; tick + route clicks into the shell; `view_local()` helper (Task 9).

**Docs:**
- `README.md` — document `list{}` + the file-browser demo (Task 10).

---

### Task 1: `Row` type + `Host::rows()` default method

**Files:**
- Modify: `crates/carapace/src/host.rs`
- Test: `crates/carapace/src/host.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `pub struct Row { pub cells: BTreeMap<String, StateValue> }` with `Row::new()`, `Row::set(self, &str, StateValue) -> Self`, `Row::get(&self, &str) -> Option<&StateValue>`; `Host::rows(&self, collection: &str) -> Vec<Row>` (default returns `Vec::new()`).

- [ ] **Step 1: Write the failing test**

Add at the bottom of `crates/carapace/src/host.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::FixtureHost;

    #[test]
    fn rows_defaults_to_empty() {
        let h = FixtureHost::new();
        assert!(h.rows("anything").is_empty());
    }

    #[test]
    fn row_builder_sets_and_gets_cells() {
        let r = Row::new().set("name", StateValue::Str("a.txt".into()));
        assert_eq!(r.get("name"), Some(&StateValue::Str("a.txt".into())));
        assert_eq!(r.get("missing"), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace --lib host::tests`
Expected: FAIL — `Row` and `rows` do not exist (compile error).

- [ ] **Step 3: Write minimal implementation**

At the top of `crates/carapace/src/host.rs`, add the import and after the `Value` enum add `Row`; then add the defaulted `rows` method to the trait:

```rust
use std::collections::BTreeMap;
use std::time::Duration;

use crate::state::StateValue;
```

```rust
/// One row of a host-provided collection: cells addressed by key.
/// BTreeMap keeps cell order deterministic for snapshot tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub cells: BTreeMap<String, StateValue>,
}

impl Row {
    pub fn new() -> Self {
        Self::default()
    }
    /// Builder-style cell insert.
    pub fn set(mut self, key: &str, value: StateValue) -> Self {
        self.cells.insert(key.to_string(), value);
        self
    }
    pub fn get(&self, key: &str) -> Option<&StateValue> {
        self.cells.get(key)
    }
}
```

In the `Host` trait, add (after `invoke`):

```rust
    /// Host-provided collections that `list{}` iterates. Default: no collections.
    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new()
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace --lib host::tests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/host.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): Host::rows() collection method + Row type"
```

---

### Task 2: `Node::List` variant + `RowCell` + exhaustive-match plumbing

This task adds the new node + cell types and satisfies every exhaustive `match` over `Node` (scene summary, layout bbox/transform, render). No expansion behavior yet — the variant compiles and resolves geometry under anchors.

**Files:**
- Modify: `crates/carapace/src/scene.rs` (add types, `summary()` arm)
- Modify: `crates/carapace/src/layout.rs` (`node_bbox`, `transform_node` arms)
- Modify: `crates/carapace/src/render.rs:242` area (`draw()` match arm)
- Test: `crates/carapace/src/scene.rs` (summary test), `crates/carapace/src/layout.rs` (anchor-resolve test)

**Interfaces:**
- Produces:
  - `pub type RowTemplate = Vec<RowCell>;`
  - `pub struct RowCell { pub bind: String, pub x_from_left: Option<f32>, pub x_from_right: Option<f32>, pub y: f32, pub size: f32, pub color: Color, pub halign: HAlign, pub font: Option<Arc<FontData>>, pub font_name: Option<String> }`
  - `Node::List { collection: String, region: ImageDest, row_height: f32, on_select: Option<String>, count: usize, template: RowTemplate }`

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `crates/carapace/src/scene.rs`:

```rust
    #[test]
    fn summary_describes_list_nodes() {
        let scene = Scene {
            canvas: (200, 100),
            nodes: vec![Node::List {
                collection: "entries".to_string(),
                region: ImageDest { x: 10.0, y: 20.0, w: 100.0, h: 60.0 },
                row_height: 20.0,
                on_select: Some("open_entry".to_string()),
                count: 3,
                template: vec![],
            }],
        };
        assert_eq!(scene.summary(), "canvas 200x100\nlist collection=entries rows=3");
    }
```

Add to the `#[cfg(test)] mod tests` in `crates/carapace/src/layout.rs` (create the module if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImageDest, Node, Scene};

    #[test]
    fn list_region_stretches_under_full_anchors() {
        let design = Scene {
            canvas: (200, 100),
            nodes: vec![Node::List {
                collection: "c".to_string(),
                region: ImageDest { x: 10.0, y: 10.0, w: 180.0, h: 80.0 },
                row_height: 20.0,
                on_select: None,
                count: 0,
                template: vec![],
            }],
        };
        let anchors = vec![Anchors {
            left: true,
            right: true,
            top: true,
            bottom: true,
            min: None,
        }];
        let resolved = resolve_scene(&design, &anchors, (300.0, 140.0));
        match &resolved.nodes[0] {
            Node::List { region, .. } => {
                assert_eq!(region.w, 280.0, "w stretched by +100");
                assert_eq!(region.h, 120.0, "h stretched by +40");
            }
            other => panic!("expected List, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace --lib`
Expected: FAIL — `Node::List` variant does not exist (compile error).

- [ ] **Step 3: Write minimal implementation**

In `crates/carapace/src/scene.rs`, add the cell type + alias above the `Node` enum (use the existing `Arc` import — add `use std::sync::Arc;` at the top if not present; the file already uses `std::sync::Arc` fully-qualified, so either works):

```rust
pub type RowTemplate = Vec<RowCell>;

/// One text cell of a list row template, in row-relative coords. Built once at parse time.
#[derive(Clone, Debug)]
pub struct RowCell {
    pub bind: String,
    /// Horizontal placement: from the region's left edge, or from its right edge. Exactly one.
    pub x_from_left: Option<f32>,
    pub x_from_right: Option<f32>,
    pub y: f32,
    pub size: f32,
    pub color: Color,
    pub halign: HAlign,
    pub font: Option<std::sync::Arc<FontData>>,
    pub font_name: Option<String>,
}
```

Add the `List` variant to `enum Node` (after `View`):

```rust
    List {
        collection: String,
        region: ImageDest,
        row_height: f32,
        on_select: Option<String>,
        /// Visible row count, set during layout expansion; 0 in the design scene.
        count: usize,
        template: RowTemplate,
    },
```

In `Scene::summary()`, add an arm (after the `Node::View` arm):

```rust
                Node::List {
                    collection, count, ..
                } => format!("list collection={collection} rows={count}"),
```

In `crates/carapace/src/layout.rs`, add a `node_bbox` arm (after the `Node::Image | Node::View | Node::Frame` arm) — note `List` uses `region`, not `dest`, so it needs its own arm:

```rust
        Node::List { region, .. } => Some(Rect {
            x: region.x,
            y: region.y,
            w: region.w,
            h: region.h,
        }),
```

And a `transform_node` arm (inside the `match &mut n`, after the Image/View/Frame arm):

```rust
        Node::List { region, .. } => {
            *region = ImageDest {
                x: to.x,
                y: to.y,
                w: to.w,
                h: to.h,
            };
        }
```

In `crates/carapace/src/render.rs`, add a no-op arm next to the existing `Node::View { .. } => {}` (around line 264):

```rust
                Node::List { .. } => {} // expands to Text rows during layout; nothing to draw here
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace --lib`
Expected: PASS (including `summary_describes_list_nodes` and `list_region_stretches_under_full_anchors`).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/scene.rs crates/carapace/src/layout.rs crates/carapace/src/render.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): Node::List variant + RowCell; anchor-resolve + render plumbing"
```

---

### Task 3: `list{}` primitive parsing + registration

**Files:**
- Modify: `crates/carapace/src/vocab.rs` (add `ListPrim`, register in `base()`)
- Test: `crates/carapace/src/script.rs` (`#[cfg(test)] mod tests` — it already has `src`, `load`, `FixtureHost`)

**Interfaces:**
- Consumes: `Node::List`, `RowCell` (Task 2); `color_from_table`, `parse_halign` (existing in `vocab.rs`); `BuildContext::font` (existing).
- Produces: a `list` Lua constructor that builds exactly one `Node::List` with `count: 0` and a parsed `template`.

- [ ] **Step 1: Write the failing test**

Add to `crates/carapace/src/script.rs` tests module:

```rust
    #[test]
    fn list_prim_parses_region_and_template() {
        use crate::scene::Node;
        let q = new_queue();
        let skin = load(
            &src(
                "list{ collection='entries', x=10, y=20, w=100, h=80, row_height=20, \
                 on_select='open_entry', template={ \
                   { bind='name', x=4, y=3, size=12, color={r=1,g=2,b=3} }, \
                   { bind='size', right=4, y=3, size=12, halign='right', color={r=4,g=5,b=6} } } }",
            ),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        assert_eq!(skin.scene.nodes.len(), 1);
        match &skin.scene.nodes[0] {
            Node::List {
                collection,
                region,
                row_height,
                on_select,
                count,
                template,
            } => {
                assert_eq!(collection, "entries");
                assert_eq!((region.x, region.y, region.w, region.h), (10.0, 20.0, 100.0, 80.0));
                assert_eq!(*row_height, 20.0);
                assert_eq!(on_select.as_deref(), Some("open_entry"));
                assert_eq!(*count, 0);
                assert_eq!(template.len(), 2);
                assert_eq!(template[0].bind, "name");
                assert_eq!(template[0].x_from_left, Some(4.0));
                assert_eq!(template[0].x_from_right, None);
                assert_eq!(template[1].x_from_left, None);
                assert_eq!(template[1].x_from_right, Some(4.0));
                assert_eq!(template[1].halign, crate::scene::HAlign::Right);
            }
            other => panic!("expected List, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace --lib script::tests::list_prim_parses_region_and_template`
Expected: FAIL — `frobnicate`-style rejection: the `list` constructor is unregistered, so `load` errors.

- [ ] **Step 3: Write minimal implementation**

In `crates/carapace/src/vocab.rs`, add `ListPrim` (place it after `ViewPrim`, before `VocabRegistry`):

```rust
struct ListPrim;
impl ListPrim {
    /// Parse one row-template cell. `ctx` resolves an optional per-cell font asset.
    fn parse_cell(
        t: &Table,
        ctx: &mut dyn BuildContext,
    ) -> Result<crate::scene::RowCell, BuildError> {
        let bind: String = t.get("bind").map_err(|_| BuildError::MissingField("bind"))?;
        let x_from_left: Option<f32> = t.get("x")?;
        let x_from_right: Option<f32> = t.get("right")?;
        if x_from_left.is_none() && x_from_right.is_none() {
            return Err(BuildError::MissingField("x or right"));
        }
        let y: f32 = t.get::<Option<f32>>("y")?.unwrap_or(0.0);
        let size: f32 = t.get::<Option<f32>>("size")?.unwrap_or(16.0);
        let color = parse_color(t)?;
        let halign = parse_halign(t)?;
        let (font, font_name) = match t.get::<Option<String>>("font")? {
            Some(name) => (Some(ctx.font(&name).map_err(BuildError::Asset)?), Some(name)),
            None => (None, None),
        };
        Ok(crate::scene::RowCell {
            bind,
            x_from_left,
            x_from_right,
            y,
            size,
            color,
            halign,
            font,
            font_name,
        })
    }
}
impl Primitive for ListPrim {
    fn id(&self) -> &str {
        "list"
    }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let collection: String = args
            .get("collection")
            .map_err(|_| BuildError::MissingField("collection"))?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").map_err(|_| BuildError::MissingField("w"))?;
        let h: f32 = args.get("h").map_err(|_| BuildError::MissingField("h"))?;
        let row_height: f32 = args
            .get("row_height")
            .map_err(|_| BuildError::MissingField("row_height"))?;
        let on_select: Option<String> = args.get("on_select")?;
        let tpl_table: Table = args
            .get("template")
            .map_err(|_| BuildError::MissingField("template"))?;
        let mut template = Vec::new();
        for entry in tpl_table.sequence_values::<Table>() {
            template.push(Self::parse_cell(&entry?, ctx)?);
        }
        Ok(vec![Node::List {
            collection,
            region: crate::scene::ImageDest { x, y, w, h },
            row_height,
            on_select,
            count: 0,
            template,
        }])
    }
}
```

Register in `VocabRegistry::base()` (after `ViewPrim`):

```rust
        r.register(Box::new(ListPrim));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace --lib script::tests::list_prim_parses_region_and_template`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/vocab.rs crates/carapace/src/script.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): list{} primitive parses collection + row template"
```

---

### Task 4: `Scene::hit_row()`

**Files:**
- Modify: `crates/carapace/src/scene.rs` (add `hit_row` to `impl Scene`)
- Test: `crates/carapace/src/scene.rs` tests

**Interfaces:**
- Produces: `Scene::hit_row(&self, p: Pt) -> Option<(String, usize)>` — the topmost list row under `p` as `(on_select action, row index)`.

- [ ] **Step 1: Write the failing test**

Add to `crates/carapace/src/scene.rs` tests:

```rust
    fn list_scene(count: usize, on_select: Option<&str>) -> Scene {
        Scene {
            canvas: (200, 100),
            nodes: vec![Node::List {
                collection: "c".to_string(),
                region: ImageDest { x: 0.0, y: 0.0, w: 100.0, h: 80.0 },
                row_height: 20.0,
                on_select: on_select.map(|s| s.to_string()),
                count,
                template: vec![],
            }],
        }
    }

    #[test]
    fn hit_row_maps_y_to_index() {
        let s = list_scene(3, Some("open"));
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 10.0 }), Some(("open".to_string(), 0)));
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 30.0 }), Some(("open".to_string(), 1)));
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 50.0 }), Some(("open".to_string(), 2)));
    }

    #[test]
    fn hit_row_misses_beyond_count_and_outside_region() {
        let s = list_scene(3, Some("open"));
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 70.0 }), None, "row 3 >= count");
        assert_eq!(s.hit_row(Pt { x: 50.0, y: -5.0 }), None, "above region");
        assert_eq!(s.hit_row(Pt { x: 150.0, y: 10.0 }), None, "right of region");
    }

    #[test]
    fn hit_row_none_without_on_select() {
        let s = list_scene(3, None);
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 10.0 }), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace --lib scene::tests::hit_row`
Expected: FAIL — `hit_row` not found (compile error).

- [ ] **Step 3: Write minimal implementation**

In `crates/carapace/src/scene.rs`, add to `impl Scene` (next to `hit`):

```rust
    /// Topmost list row under `p`: `(on_select action, row index)`. Lists draw later → reverse.
    pub fn hit_row(&self, p: Pt) -> Option<(String, usize)> {
        for node in self.nodes.iter().rev() {
            let Node::List {
                region,
                row_height,
                on_select,
                count,
                ..
            } = node
            else {
                continue;
            };
            let Some(action) = on_select else { continue };
            if *row_height <= 0.0 || *count == 0 {
                continue;
            }
            if p.x < region.x || p.x > region.x + region.w || p.y < region.y {
                continue;
            }
            let idx = ((p.y - region.y) / row_height).floor() as usize;
            if idx < *count {
                return Some((action.clone(), idx));
            }
        }
        None
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace --lib scene::tests::hit_row`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/scene.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): Scene::hit_row maps a point to a list row + action"
```

---

### Task 5: Layout expansion (`expand_lists`) + `RowCell::to_node`

**Files:**
- Modify: `crates/carapace/src/scene.rs` (`RowCell::to_node`)
- Modify: `crates/carapace/src/engine.rs` (`expand_lists`, wire into `layout`)
- Test: `crates/carapace/tests/list_layout.rs` (new integration test file)

**Interfaces:**
- Consumes: `Host::rows` (Task 1); `Node::List`, `RowCell` (Task 2).
- Produces: `RowCell::to_node(&self, region: &ImageDest, row_top: f32, value: &str) -> Node`; `Engine::layout` now expands list nodes (the returned scene contains the retained `Node::List` with `count = n` followed by `n × template.len()` `Node::Text` rows).

- [ ] **Step 1: Write the failing test**

Create `crates/carapace/tests/list_layout.rs`:

```rust
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
            Node::Text { content, .. } => match content {
                carapace::scene::TextContent::Static(s) => Some(s.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["a", "b", "c", "d"], "first 4 rows expanded in order");
}
```

> If `carapace::command`, `engine`, `host`, `scene`, `state`, or `vocab` are not already public modules, the integration test will fail to compile. They are public (the existing `tests/` integration tests import them). If a path differs, match the imports used in `crates/carapace/tests/engine_layout.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace --test list_layout`
Expected: FAIL — `layout` does not expand lists yet (the scene has 1 node, no Text rows; `texts` is empty).

- [ ] **Step 3: Write minimal implementation**

In `crates/carapace/src/scene.rs`, add to `impl RowCell` (create the impl block):

```rust
impl RowCell {
    /// The concrete Text node for this cell in a row, positioned within `region`.
    pub fn to_node(&self, region: &ImageDest, row_top: f32, value: &str) -> Node {
        let x = match (self.x_from_left, self.x_from_right) {
            (Some(l), _) => region.x + l,
            (None, Some(r)) => region.x + region.w - r,
            (None, None) => region.x,
        };
        Node::Text {
            content: TextContent::Static(value.to_string()),
            font: self.font.clone(),
            font_name: self.font_name.clone(),
            size: self.size,
            paint: Paint::Solid(self.color),
            halign: self.halign,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt {
                x,
                y: row_top + self.y,
            },
        }
    }
}
```

In `crates/carapace/src/engine.rs`, change `layout` to expand lists, and add the free function:

```rust
    pub fn layout(&self, logical_w: f32, logical_h: f32) -> Scene {
        let mut scene =
            crate::layout::resolve_scene(&self.skin.scene, &self.skin.anchors, (logical_w, logical_h));
        expand_lists(&mut scene, self.host.as_ref());
        scene
    }
```

Add at the bottom of `crates/carapace/src/engine.rs` (module-private):

```rust
/// Replace each `Node::List` with [retained List (count=n), then n×template Text rows].
/// `n` is clamped to the rows that fit the region height. Pure Rust — no Lua.
fn expand_lists(scene: &mut Scene, host: &dyn Host) {
    use crate::scene::Node;

    let mut out = Vec::with_capacity(scene.nodes.len());
    for node in std::mem::take(&mut scene.nodes) {
        let Node::List {
            collection,
            region,
            row_height,
            on_select,
            count: _,
            template,
        } = node
        else {
            out.push(node);
            continue;
        };

        let rows = host.rows(&collection);
        let visible = if row_height > 0.0 {
            (region.h / row_height).floor().max(0.0) as usize
        } else {
            0
        };
        let n = rows.len().min(visible);

        out.push(Node::List {
            collection,
            region,
            row_height,
            on_select,
            count: n,
            template: template.clone(),
        });

        for (i, row) in rows.iter().take(n).enumerate() {
            let row_top = region.y + i as f32 * row_height;
            for cell in &template {
                let value = match row.get(&cell.bind) {
                    Some(StateValue::Str(s)) => s.to_string(),
                    Some(StateValue::Scalar(f)) => f.to_string(),
                    Some(StateValue::Bool(b)) => b.to_string(),
                    None => String::new(),
                };
                out.push(cell.to_node(&region, row_top, &value));
            }
        }
    }
    scene.nodes = out;
}
```

> `region` is `ImageDest` which is `Copy`, so using it after moving `collection`/`template` out of the destructured node is fine. `StateValue` is already imported in `engine.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace --test list_layout`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/scene.rs crates/carapace/src/engine.rs crates/carapace/tests/list_layout.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): expand list rows at layout, clamped to visible region"
```

---

### Task 6: Row click dispatches a host action

**Files:**
- Modify: `crates/carapace/src/engine.rs` (`handle_pointer_resolved`)
- Test: `crates/carapace/tests/list_layout.rs` (extend with a dispatch test)

**Interfaces:**
- Consumes: `Scene::hit_row` (Task 4); `Command::HostAction`, `Value::Num` (existing).
- Produces: `handle_pointer_resolved` now, when no polygon hotspot is hit, tries `hit_row` and enqueues `Command::HostAction { action, args: [Value::Num(index)] }` (validated against the host allowlist on `update`).

- [ ] **Step 1: Write the failing test**

Append to `crates/carapace/tests/list_layout.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace --test list_layout clicking_a_row`
Expected: FAIL — `last` stays `None` (no row dispatch yet).

- [ ] **Step 3: Write minimal implementation**

In `crates/carapace/src/engine.rs`, replace `handle_pointer_resolved` with:

```rust
    pub fn handle_pointer_resolved(
        &mut self,
        logical_w: f32,
        logical_h: f32,
        p: Pt,
        _kind: PointerEvent,
    ) {
        let scene = self.layout(logical_w, logical_h);
        if let Some(id) = scene.hit(p) {
            if let Err(e) = self.skin.fire(id) {
                eprintln!("carapace: handler error: {e:?}");
            }
            return;
        }
        if let Some((action, index)) = scene.hit_row(p) {
            self.queue.borrow_mut().push(Command::HostAction {
                action,
                args: vec![crate::host::Value::Num(index as f64)],
            });
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace --test list_layout`
Expected: PASS (all three tests in the file).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/engine.rs crates/carapace/tests/list_layout.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): row clicks dispatch on_select host action with the row index"
```

---

### Task 7: `FileSystem` trait + `StdFs` + `MockFs`

**Files:**
- Create: `crates/carapace-demo/src/file_browser_host.rs`
- Modify: `crates/carapace-demo/src/main.rs` (add `mod file_browser_host;` near the other module declarations / top of file)
- Test: `crates/carapace-demo/src/file_browser_host.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub struct DirEntryInfo { pub name: String, pub is_dir: bool, pub size: u64 }`
  - `pub trait FileSystem { fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>>; }`
  - `pub struct StdFs;` implementing `FileSystem` over read-only `std::fs`
  - `MockFs` (test-only) implementing `FileSystem` from an in-memory map

- [ ] **Step 1: Write the failing test**

Create `crates/carapace-demo/src/file_browser_host.rs` with the test first:

```rust
use std::io;
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn mockfs_returns_seeded_entries() {
        let fs = MockFs::new().dir(
            "/root",
            vec![
                DirEntryInfo { name: "sub".into(), is_dir: true, size: 0 },
                DirEntryInfo { name: "a.txt".into(), is_dir: false, size: 2048 },
            ],
        );
        let entries = fs.read_dir(&PathBuf::from("/root")).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "a.txt" && !e.is_dir && e.size == 2048));
    }

    #[test]
    fn mockfs_unknown_dir_errors() {
        let fs = MockFs::new();
        assert!(fs.read_dir(&PathBuf::from("/nope")).is_err());
    }

    #[test]
    fn stdfs_reads_a_real_directory() {
        let fs = StdFs;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let entries = fs.read_dir(dir).unwrap();
        assert!(
            entries.iter().any(|e| e.name == "Cargo.toml" && !e.is_dir),
            "demo crate dir contains a Cargo.toml file"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

First register the module so it compiles. In `crates/carapace-demo/src/main.rs`, add near the top (with other `mod`/`use` lines):

```rust
mod file_browser_host;
```

Run: `cargo test -p carapace-demo --bin carapace-demo file_browser_host`
Expected: FAIL — `MockFs`, `StdFs`, `DirEntryInfo` undefined (compile error).

- [ ] **Step 3: Write minimal implementation**

Add to the top of `crates/carapace-demo/src/file_browser_host.rs` (above the `tests` module):

```rust
/// One directory entry, filesystem-agnostic.
#[derive(Clone, Debug, PartialEq)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Read-only directory listing. Abstracted so tests use an in-memory tree.
pub trait FileSystem {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>>;
}

/// The real, read-only filesystem.
pub struct StdFs;
impl FileSystem for StdFs {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            let is_dir = meta.is_dir();
            out.push(DirEntryInfo {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir,
                size: if is_dir { 0 } else { meta.len() },
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
pub struct MockFs {
    dirs: std::collections::HashMap<std::path::PathBuf, Vec<DirEntryInfo>>,
}
#[cfg(test)]
impl MockFs {
    pub fn new() -> Self {
        Self { dirs: std::collections::HashMap::new() }
    }
    pub fn dir(mut self, path: &str, entries: Vec<DirEntryInfo>) -> Self {
        self.dirs.insert(std::path::PathBuf::from(path), entries);
        self
    }
}
#[cfg(test)]
impl FileSystem for MockFs {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>> {
        self.dirs
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such mock dir"))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace-demo --bin carapace-demo file_browser_host`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/file_browser_host.rs crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): FileSystem trait with StdFs + in-memory MockFs"
```

---

### Task 8: `FileBrowserHost`

**Files:**
- Modify: `crates/carapace-demo/src/file_browser_host.rs`
- Test: `crates/carapace-demo/src/file_browser_host.rs` tests

**Interfaces:**
- Consumes: `FileSystem`, `DirEntryInfo` (Task 7); `Host`, `Row`, `ActionSpec`, `Value`, `StateValue` (engine).
- Produces: `FileBrowserHost<F: FileSystem>` implementing `Host`. `rows("shortcuts")` → rows with a `label` cell; `rows("entries")` → optional `".."` then dirs-before-files, each with `name` + `size` cells; `invoke("open_entry", [Num(i)])` / `invoke("open_shortcut", [Num(i)])` navigate; `get("current_path")` → `Str`. Sandboxed to `root`.

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/carapace-demo/src/file_browser_host.rs`:

```rust
    use carapace::host::{Host, Value};
    use carapace::state::StateValue;

    fn s(v: &str) -> StateValue {
        StateValue::Str(v.into())
    }

    fn fixture() -> FileBrowserHost<MockFs> {
        let fs = MockFs::new()
            .dir(
                "/root",
                vec![
                    DirEntryInfo { name: "sub".into(), is_dir: true, size: 0 },
                    DirEntryInfo { name: "a.txt".into(), is_dir: false, size: 2048 },
                ],
            )
            .dir(
                "/root/sub",
                vec![DirEntryInfo { name: "b.txt".into(), is_dir: false, size: 512 }],
            );
        FileBrowserHost::new(
            fs,
            PathBuf::from("/root"),
            vec![
                ("Root".into(), PathBuf::from("/root")),
                ("Sub".into(), PathBuf::from("/root/sub")),
            ],
        )
    }

    #[test]
    fn entries_list_dirs_first_no_dotdot_at_root() {
        let h = fixture();
        let rows = h.rows("entries");
        assert_eq!(rows[0].get("name"), Some(&s("sub")));
        assert_eq!(rows[0].get("size"), Some(&s("<dir>")));
        assert_eq!(rows[1].get("name"), Some(&s("a.txt")));
        assert_eq!(rows[1].get("size"), Some(&s("2.0K")));
        assert!(!rows.iter().any(|r| r.get("name") == Some(&s(".."))), "no .. at root");
    }

    #[test]
    fn shortcuts_list_labels() {
        let h = fixture();
        let rows = h.rows("shortcuts");
        assert_eq!(rows[0].get("label"), Some(&s("Root")));
        assert_eq!(rows[1].get("label"), Some(&s("Sub")));
    }

    #[test]
    fn open_entry_enters_dir_then_dotdot_goes_up() {
        let mut h = fixture();
        h.invoke("open_entry", &[Value::Num(0.0)]); // enter "sub"
        assert_eq!(h.get("current_path"), Some(s("/root/sub")));
        let rows = h.rows("entries");
        assert_eq!(rows[0].get("name"), Some(&s("..")), ".. offered below root");
        assert_eq!(rows[1].get("name"), Some(&s("b.txt")));

        h.invoke("open_entry", &[Value::Num(0.0)]); // ".." back up
        assert_eq!(h.get("current_path"), Some(s("/root")));
    }

    #[test]
    fn open_shortcut_jumps_and_stays_within_root() {
        let mut h = fixture();
        h.invoke("open_shortcut", &[Value::Num(1.0)]);
        assert_eq!(h.get("current_path"), Some(s("/root/sub")));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-demo --bin carapace-demo file_browser_host`
Expected: FAIL — `FileBrowserHost` undefined (compile error).

- [ ] **Step 3: Write minimal implementation**

Add to `crates/carapace-demo/src/file_browser_host.rs` (above the tests; add imports at the top of the file):

```rust
use std::path::PathBuf;

use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::state::StateValue;

const FB_ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "open_entry" },
    ActionSpec { name: "open_shortcut" },
];

/// What an `entries` row navigates to. Kept in lockstep with `rows("entries")`.
enum Target {
    Up,
    Dir(PathBuf),
    File,
}

pub struct FileBrowserHost<F: FileSystem> {
    fs: F,
    root: PathBuf,
    current: PathBuf,
    shortcuts: Vec<(String, PathBuf)>,
}

impl<F: FileSystem> FileBrowserHost<F> {
    pub fn new(fs: F, root: PathBuf, shortcuts: Vec<(String, PathBuf)>) -> Self {
        Self { current: root.clone(), fs, root, shortcuts }
    }

    /// Directory entries, dirs first then files, each case-insensitively by name.
    fn sorted_entries(&self) -> Vec<DirEntryInfo> {
        let mut v = self.fs.read_dir(&self.current).unwrap_or_default();
        v.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        v
    }

    /// The (display row, navigation target) pairs for the current directory, in order.
    /// `rows("entries")` and `invoke("open_entry", i)` both derive from this, so indices align.
    fn entry_rows(&self) -> Vec<(Row, Target)> {
        let mut out = Vec::new();
        if self.current != self.root {
            out.push((
                Row::new()
                    .set("name", StateValue::Str("..".into()))
                    .set("size", StateValue::Str("".into())),
                Target::Up,
            ));
        }
        for e in self.sorted_entries() {
            let (size, target) = if e.is_dir {
                ("<dir>".to_string(), Target::Dir(self.current.join(&e.name)))
            } else {
                (human_size(e.size), Target::File)
            };
            out.push((
                Row::new()
                    .set("name", StateValue::Str(e.name.as_str().into()))
                    .set("size", StateValue::Str(size.as_str().into())),
                target,
            ));
        }
        out
    }

    /// Only allow navigating to paths at or under `root`.
    fn within_root(&self, p: &Path) -> bool {
        p.starts_with(&self.root)
    }
}

fn human_size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b < K {
        format!("{bytes}B")
    } else if b < K * K {
        format!("{:.1}K", b / K)
    } else {
        format!("{:.1}M", b / (K * K))
    }
}

impl<F: FileSystem> Host for FileBrowserHost<F> {
    fn name(&self) -> &str {
        "file-browser"
    }
    fn tick(&mut self, _dt: std::time::Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "current_path" => Some(StateValue::Str(self.current.to_string_lossy().as_ref().into())),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        FB_ACTIONS
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        let index = match args.first() {
            Some(Value::Num(n)) if *n >= 0.0 => *n as usize,
            _ => return,
        };
        match action {
            "open_entry" => {
                let targets = self.entry_rows();
                match targets.into_iter().nth(index).map(|(_, t)| t) {
                    Some(Target::Up) => {
                        if let Some(parent) = self.current.parent() {
                            if self.within_root(parent) {
                                self.current = parent.to_path_buf();
                            }
                        }
                    }
                    Some(Target::Dir(p)) => {
                        if self.within_root(&p) {
                            self.current = p;
                        }
                    }
                    _ => {}
                }
            }
            "open_shortcut" => {
                if let Some((_, path)) = self.shortcuts.get(index) {
                    if self.within_root(path) {
                        self.current = path.clone();
                    }
                }
            }
            _ => {}
        }
    }
    fn rows(&self, collection: &str) -> Vec<Row> {
        match collection {
            "shortcuts" => self
                .shortcuts
                .iter()
                .map(|(label, _)| Row::new().set("label", StateValue::Str(label.as_str().into())))
                .collect(),
            "entries" => self.entry_rows().into_iter().map(|(row, _)| row).collect(),
            _ => Vec::new(),
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace-demo --bin carapace-demo file_browser_host`
Expected: PASS (7 tests total in the file).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/file_browser_host.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): FileBrowserHost — sandboxed read-only navigation over FileSystem"
```

---

### Task 9: Demo wiring — list skin, FileBrowserHost, shell tick + click routing

**Files:**
- Modify: `crates/carapace-demo/src/main.rs`
- Test: `crates/carapace-demo/src/main.rs` (`#[cfg(test)]` test for `view_local`)

**Interfaces:**
- Consumes: `FileBrowserHost`, `StdFs` (Tasks 7–8); `Engine::handle_pointer_resolved`, `Engine::update` (engine); `Scene::views` (existing).
- Produces: a two-`list{}` `APP_SHELL`; `AppShell::tick(&mut self, dt)` and `AppShell::handle_click(&mut self, inner: Pt, view_w: f32, view_h: f32)`; `view_local(p: Pt, dest: &ImageDest) -> Option<Pt>`; redraw loop ticks the shell; left-clicks inside the `app` view route to the shell.

- [ ] **Step 1: Write the failing test**

Add a `#[cfg(test)]` module near the bottom of `crates/carapace-demo/src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::view_local;
    use carapace::scene::{ImageDest, Pt};

    #[test]
    fn view_local_translates_inside_and_rejects_outside() {
        let d = ImageDest { x: 10.0, y: 20.0, w: 100.0, h: 50.0 };
        assert_eq!(view_local(Pt { x: 10.0, y: 20.0 }, &d), Some(Pt { x: 0.0, y: 0.0 }));
        assert_eq!(view_local(Pt { x: 60.0, y: 45.0 }, &d), Some(Pt { x: 50.0, y: 25.0 }));
        assert_eq!(view_local(Pt { x: 5.0, y: 45.0 }, &d), None, "left of region");
        assert_eq!(view_local(Pt { x: 60.0, y: 80.0 }, &d), None, "below region");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-demo --bin carapace-demo view_local`
Expected: FAIL — `view_local` undefined (compile error).

- [ ] **Step 3: Write the implementation**

**(3a)** Add the `view_local` helper (near the top-level functions in `main.rs`, e.g. next to `skin_root`):

```rust
/// Map an outer-logical point into a view region's local coords, or None if outside.
/// The nested shell reflows to the view's logical size, so the mapping is a pure translate.
fn view_local(p: Pt, dest: &carapace::scene::ImageDest) -> Option<Pt> {
    if p.x < dest.x || p.x > dest.x + dest.w || p.y < dest.y || p.y > dest.y + dest.h {
        return None;
    }
    Some(Pt { x: p.x - dest.x, y: p.y - dest.y })
}
```

**(3b)** Replace the `APP_SHELL` constant (and keep `APP_SHELL_SIZE = (456, 272)`) with a two-pane list skin:

```rust
/// Inline skin for the nested file-browser shown inside the frame skin's `view{ id="app" }`.
/// Design size matches the view's design rect (456×272). Two live lists + a path line.
const APP_SHELL: &str = "\
    fill{ path = rect{x=0,y=0,w=456,h=272}, color = {r=18,g=20,b=26} }\n\
    fill{ path = rect{x=0,y=0,w=456,h=24}, color = {r=40,g=46,b=60}, anchor={'left','right','top'} }\n\
    text{ value='current_path', size=13, x=8, y=4, color={r=170,g=185,b=210}, anchor={'left','right','top'} }\n\
    fill{ path = rect{x=0,y=24,w=120,h=248}, color = {r=24,g=28,b=38}, anchor={'left','top','bottom'} }\n\
    list{ collection='shortcuts', x=8, y=32, w=104, h=232, row_height=20, on_select='open_shortcut',\n\
          anchor={'left','top','bottom'},\n\
          template={ { bind='label', x=4, y=2, size=13, color={r=150,g=200,b=170} } } }\n\
    list{ collection='entries', x=128, y=32, w=320, h=232, row_height=20, on_select='open_entry',\n\
          anchor={'left','right','top','bottom'},\n\
          template={ { bind='name', x=4, y=2, size=13, color={r=200,g=210,b=225} },\n\
                     { bind='size', right=8, y=2, size=13, halign='right', color={r=150,g=160,b=175} } } }\n";
```

**(3c)** In `AppShell::new`, build a `FileBrowserHost<StdFs>` instead of `DemoHost`. The `outbox` arg is no longer needed by the host; keep the signature or simplify. Minimal change — replace the `Engine::new` host argument:

```rust
    fn new(device: &wgpu::Device, _outbox: WindowOutbox) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")) // .../crates/carapace-demo
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let shortcuts = vec![
            ("Repo".to_string(), root.clone()),
            ("Crates".to_string(), root.join("crates")),
            ("Docs".to_string(), root.join("docs")),
        ];
        let engine = Engine::new(
            Box::new(file_browser_host::FileBrowserHost::new(
                file_browser_host::StdFs,
                root,
                shortcuts,
            )),
            VocabRegistry::base(),
            SkinSource::inline(APP_SHELL, APP_SHELL_SIZE),
        )
        .unwrap();
        // ... rest unchanged (texture creation, struct construction) ...
    }
```

> Keep the remaining body of `new` (texture + struct fields) as-is. If `DemoHost`/`with_outbox` imports become unused, remove them to satisfy clippy; if `DemoHost` is still used elsewhere (the gadget skins), leave it.

**(3d)** Add two methods to `impl AppShell`:

```rust
    /// Drain queued navigation host-actions (the shell engine is not ticked elsewhere).
    fn tick(&mut self, dt: std::time::Duration) {
        self.engine.update(dt);
    }

    /// Forward a click already translated into the shell's local coords.
    fn handle_click(&mut self, inner: Pt, view_w: f32, view_h: f32) {
        self.engine
            .handle_pointer_resolved(view_w, view_h, inner, PointerEvent::Press);
    }
```

**(3e)** In the `RedrawRequested` handler, tick the shell each frame. Right after the existing `self.engine.update(dt);` (line ~499):

```rust
                if let Some(shell) = self.app_shell.as_mut() {
                    shell.tick(dt);
                }
```

**(3f)** In the `MouseInput` handler, route clicks inside the `app` view to the shell. Replace the `if self.meta.resizable { ... }` branch body with:

```rust
                    if self.meta.resizable {
                        let sf = window.scale_factor() as f32;
                        let logical_w = pw as f32 / sf;
                        let logical_h = ph as f32 / sf;
                        let p = Pt {
                            x: self.cursor.0 as f32 / sf,
                            y: self.cursor.1 as f32 / sf,
                        };
                        // If the click is inside the "app" view, route it into the nested shell
                        // engine; otherwise hit-test the outer (chrome) scene.
                        let app_dest = self
                            .engine
                            .layout(logical_w, logical_h)
                            .views()
                            .into_iter()
                            .find(|(id, _)| id == "app")
                            .map(|(_, d)| d);
                        let routed = match (app_dest, self.app_shell.as_mut()) {
                            (Some(dest), Some(shell)) => match view_local(p, &dest) {
                                Some(inner) => {
                                    shell.handle_click(inner, dest.w, dest.h);
                                    true
                                }
                                None => false,
                            },
                            _ => false,
                        };
                        if !routed {
                            self.engine.handle_pointer_resolved(
                                logical_w,
                                logical_h,
                                p,
                                PointerEvent::Press,
                            );
                        }
                    } else {
```

(Leave the gadget `else` branch unchanged.)

- [ ] **Step 4: Run the test + build to verify**

Run: `cargo test -p carapace-demo --bin carapace-demo view_local`
Expected: PASS.

Run: `cargo build -p carapace-demo`
Expected: builds clean (no unused-import warnings under clippy in Step 5).

- [ ] **Step 5: Manual smoke check + commit**

Manual (optional but recommended): `cargo run -p carapace-demo`, press `Tab` until the frame skin appears; the right pane lists the repo root; clicking a folder (e.g. `crates`) enters it; `..` returns; clicking a left shortcut jumps panes. Resizing the window reflows both lists.

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): live two-pane file browser — list skin, FileBrowserHost, click routing"
```

---

### Task 10: README + golden-snapshot regression verification

**Files:**
- Modify: `README.md`
- Verify: full test suite incl. GPU goldens

**Interfaces:** none (docs + verification).

- [ ] **Step 1: Update the README**

Add `list{}` to the vocabulary/primitive documentation and describe the demo's file browser. Locate the existing primitive list in `README.md` (search for `view{` or `frame{`) and add an entry alongside it:

```markdown
- `list{ collection, x, y, w, h, row_height, on_select, template }` — a dynamic, host-driven
  list. The engine calls `Host::rows(collection)` each frame and expands the `template` (a list of
  `{ bind, x|right, y, size, color, halign }` cells) into one row per item, clamped to the visible
  region. A click on a row invokes the `on_select` host action with the row index.
```

And, where the demo is described, update the app-shell paragraph:

```markdown
The frame skin hosts a live, two-pane read-only **file browser** in its `view{ id="app" }` region:
a shortcuts column and a directory listing, both driven by `list{}` over a `FileBrowserHost`.
Clicks inside the view are translated into the nested shell engine's coordinate space by the demo.
```

(Match the README's existing heading structure and wording; keep it concise.)

- [ ] **Step 2: Run the full CI gate locally**

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

Expected: all PASS.

- [ ] **Step 3: Run the GPU golden regression**

Run: `cargo test --locked -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS — gadget-skin goldens **byte-identical** (Spec 2 added no render-path change for existing node kinds). If a golden differs, that is a regression to investigate, not to re-bless.

- [ ] **Step 4: Commit**

```bash
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "docs(readme): document list{} primitive + file-browser demo"
```

---

## Self-Review

**Spec coverage:**
- `Host::rows()` typed collection method → Task 1. ✓
- `list{}` primitive + `Node::List` + `RowCell` → Tasks 2, 3. ✓
- Deferred expansion at layout, clamp to visible → Task 5. ✓
- `Scene::hit_row` + engine dispatch reusing host action → Tasks 4, 6. ✓
- Demo-side input routing into `view{}` (coordinate transform + forward) → Task 9 (`view_local`, click routing). ✓
- Shell engine ticking (so navigation drains) → Task 9 (3e). ✓
- `FileSystem`/`StdFs`/`MockFs` + `FileBrowserHost` (sandboxed, read-only) → Tasks 7, 8. ✓
- Two-pane live browser in one app view → Task 9 skin. ✓
- Render/compositing unchanged; gadget goldens byte-identical → Task 2 (no-op arm), Task 10 (verify). ✓
- README current in same PR → Task 10. ✓
- Out-of-scope (no scroll/selection/second view/file-open/FS-write) → respected throughout.

**Placeholder scan:** No `TBD`/`TODO`/"add error handling"/"similar to Task N". Every code step shows complete code.

**Type consistency:** `Row`/`Row::set`/`Row::get` (Task 1) used in 5, 8. `Node::List { collection, region, row_height, on_select, count, template }` consistent across Tasks 2–6. `RowCell` fields (`x_from_left`/`x_from_right`) consistent across Tasks 2, 3, 5. `Scene::hit_row -> Option<(String, usize)>` (Task 4) consumed in Task 6. `FileSystem::read_dir`/`DirEntryInfo` (Task 7) consumed in Task 8. `view_local`/`AppShell::tick`/`AppShell::handle_click` (Task 9) consistent with their test and call sites. `FB_ACTIONS` names (`open_entry`, `open_shortcut`) match the skin's `on_select` values in Task 9.

---

## Execution Handoff

Recommended: subagent-driven development (fresh subagent per task + review between tasks). Tasks are ordered so the engine crate (1–6) is fully green before the demo crate (7–9) depends on it; Task 10 verifies the whole.

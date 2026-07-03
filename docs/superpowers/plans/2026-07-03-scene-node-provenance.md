# Scene Node Provenance + Picking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-node source provenance and topmost-node picking to the `carapace` engine so an authoring tool (`carapace-preview` Plan B) can map a rendered scene node back to the `fill{}`/`text{}`/… call that produced it, and edit its literal props.

**Architecture:** At skin load, each primitive constructor closure reads the Lua caller's current line via mlua's `inspect_stack` and stamps one `Origin` per emitted node — a parallel `Vec<Origin>` mirroring the existing per-node `anchors` vec. Origins are carried through `layout()`'s resolve + list-expand passes and returned via a new `layout_with_origins()`; `layout()` itself is unchanged. A new `Scene::pick` returns the topmost node index whose bbox contains a point, reusing the (now `pub`) `node_bbox`.

**Tech Stack:** Rust (edition 2024), `carapace` engine crate, `mlua = 0.11.6` (`lua54`, `vendored`) — already a dependency; **no new crates**.

See design: `docs/superpowers/specs/2026-07-03-scene-node-provenance-design.md`.

## Global Constraints

- **Edition:** `edition = "2024"` (repo standard). `crates/carapace` already uses it.
- **No new dependencies.** `mlua` 0.11.6 is already in `crates/carapace/Cargo.toml:11`. Nothing to fetch through `sfw`.
- **Zero behavior change to existing consumers.** `Engine::layout(w, h) -> Scene` keeps its exact signature and output. `Scene`'s public shape (`{ nodes, canvas }`) is unchanged — origins ride in a parallel `Vec`, never on `Scene`, so none of the ~25 `Scene { nodes, canvas }` literal sites across the workspace (`carapace-ffi`, `embed-spike`, tests) need edits. All existing `carapace` tests must pass unchanged.
- **Commit after every task.** Repo git identity (Daniel Agbemava <danagbemava@gmail.com>) is the configured default — do not pass `--author`.
- **Before finishing:** `cargo clippy -p carapace --all-targets -- -D warnings` and `cargo fmt` must pass — CI gates on clippy `-D warnings`. These provenance tests need **no GPU**, so `cargo test -p carapace` runs them on any machine.

## Verified facts this plan is built on

- `mlua::Lua::inspect_stack(level: usize, f: impl FnOnce(&Debug) -> R) -> Option<R>` (`~/.cargo/.../mlua-0.11.6/src/state.rs:917`). Level 0 = the current running (C) function — our constructor closure; level 1 = the Lua caller — the skin chunk. `Debug::current_line(&self) -> Option<usize>` (`debug.rs:143`) lazily runs `lua_getinfo(state, "l")`. So `lua.inspect_stack(1, |d| d.current_line()).flatten()` = the executing skin line (1-based).
- The constructor closure is `move |_, args: Table| { … }` (`crates/carapace/src/script.rs:133`). Its first param is `&Lua` (currently ignored). Nodes and anchors are pushed per emitted node at `script.rs:143-146`.
- `LoadedSkin` (`script.rs:40-46`) already holds `pub anchors: Vec<Anchors>`; `Engine::scene_anchors()` (`engine.rs:131`) exposes it. We mirror both for origins.
- `Scene { pub nodes: Vec<Node>, pub canvas: (u32, u32) }` (`scene.rs:239-243`), `#[derive(Clone, Debug)]`. `Pt { pub x: f32, pub y: f32 }` (`scene.rs:3`). `Node` is a 9-variant enum (`scene.rs:175-237`).
- `Engine::layout(&self, logical_w, logical_h) -> Scene` (`engine.rs:139`) = `resolve_scene(&skin.scene, &skin.anchors, (w,h))` then `expand_lists(&mut scene, host)`.
- `resolve_scene(design: &Scene, anchors: &[Anchors], logical: (f32,f32)) -> Scene` (`layout.rs:201`) maps nodes 1:1.
- `expand_lists(scene: &mut Scene, host: &dyn Host)` (`engine.rs:156`) is private, called only at `engine.rs:145`; replaces each `List` node with `List` + optional highlight `Fill` + N row `Text`s.
- `node_bbox(node: &Node) -> Option<Rect>` (`layout.rs:79`) is private; handles all kinds (Text → zero-size point). `Rect` (`layout.rs:6`) is `pub`.
- `carapace::host::Host::rows(&self, collection: &str) -> Vec<Row>` defaults to empty (`host.rs:47`). `Row::new()` + `Row::set(key, StateValue)` (`host.rs:20-27`). `StateValue::{Bool(bool), Scalar(f32), Str(Arc<str>)}` (`state.rs:2`). `SkinSource::inline(lua_src, canvas)` (`command.rs:16`) builds an inline source (its `lua_src` is the verbatim string, so line numbers are 1-based within it). `FixtureHost` (`crate::fixture`) has no `rows` override.

## File Structure

All changes are in `crates/carapace/src/`:
- `scene.rs` — add `Origin` type; add `Scene::pick`.
- `script.rs` — capture origins at load; `SceneBuilder.origins` + `call_seq`; `LoadedSkin.origins`.
- `engine.rs` — `scene_origins()`; origin-aware `expand_lists`; `resolve_expand` helper; `layout_with_origins()`.
- `layout.rs` — make `node_bbox` `pub`.

---

### Task 1: `Origin` type + load-time capture + `scene_origins()`

**Files:**
- Modify: `crates/carapace/src/scene.rs` (add `Origin` near `Scene`)
- Modify: `crates/carapace/src/script.rs` (`SceneBuilder`, ctor closure, `LoadedSkin`)
- Modify: `crates/carapace/src/engine.rs` (add `scene_origins`)

**Interfaces:**
- Produces:
  - `carapace::scene::Origin { pub line: Option<u32>, pub call: Option<u32> }` — `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]`.
  - `LoadedSkin.origins: Vec<Origin>` (pub field, parallel to the design `scene.nodes`).
  - `Engine::scene_origins(&self) -> &[carapace::scene::Origin]`.

- [ ] **Step 1: Add the `Origin` type to `scene.rs`**

Insert directly above the `Scene` struct definition (`scene.rs:239`):

```rust
/// Where a scene node came from in the skin source. Metadata only — the renderer and
/// hit-test ignore it. Populated at load; carried through `layout`. See
/// `docs/superpowers/specs/2026-07-03-scene-node-provenance-design.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Origin {
    /// 1-based line of the primitive call in the skin's entry Lua, if known.
    pub line: Option<u32>,
    /// Monotonic index of the primitive call that emitted this node. Nodes from the
    /// same call (e.g. a `fill{}` with `on_press` emits Fill + Hotspot) share it.
    /// `None` for engine-generated nodes (list rows, selection highlight).
    pub call: Option<u32>,
}
```

- [ ] **Step 2: Write the failing capture tests in `script.rs`**

Add these tests to the existing `#[cfg(test)] mod tests` block in `crates/carapace/src/script.rs` (after the last test, before the closing `}`):

```rust
    #[test]
    fn origins_capture_line_and_call_per_node() {
        // Two fills on lines 1 and 2 of the inline source.
        let s = "fill{ path={{x=0,y=0},{x=10,y=0},{x=10,y=5}}, color={r=1,g=2,b=3} }\n\
                 fill{ path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color={r=4,g=5,b=6} }";
        let skin = load(&src(s), &FixtureHost::new(), Rc::new(VocabRegistry::base()), new_queue()).unwrap();
        assert_eq!(skin.origins.len(), skin.scene.nodes.len());
        assert_eq!(skin.origins.len(), 2);
        assert_eq!(skin.origins[0].line, Some(1));
        assert_eq!(skin.origins[1].line, Some(2));
        assert_eq!(skin.origins[0].call, Some(0));
        assert_eq!(skin.origins[1].call, Some(1));
    }

    #[test]
    fn one_call_emitting_two_nodes_shares_a_call_ordinal() {
        // `fill` with `on_press` emits Fill + Hotspot from a single constructor call.
        let s = "fill{ path=rect{x=0,y=0,w=10,h=10}, color={r=0,g=0,b=0}, \
                 on_press=function() host.toggle() end }";
        let skin = load(&src(s), &FixtureHost::new(), Rc::new(VocabRegistry::base()), new_queue()).unwrap();
        assert_eq!(skin.scene.nodes.len(), 2, "fill + hotspot");
        assert_eq!(skin.origins.len(), 2);
        assert_eq!(skin.origins[0].call, Some(0));
        assert_eq!(skin.origins[1].call, Some(0), "both share the one call");
        assert_eq!(skin.origins[0].line, Some(1));
        assert_eq!(skin.origins[1].line, Some(1));
    }

    #[test]
    fn a_loop_yields_same_line_distinct_calls() {
        let s = "for i=1,3 do\n\
                 \x20 fill{ path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color={r=1,g=1,b=1} }\n\
                 end";
        let skin = load(&src(s), &FixtureHost::new(), Rc::new(VocabRegistry::base()), new_queue()).unwrap();
        assert_eq!(skin.origins.len(), 3);
        assert!(skin.origins.iter().all(|o| o.line == Some(2)), "fill body is line 2");
        let calls: Vec<_> = skin.origins.iter().map(|o| o.call).collect();
        assert_eq!(calls, vec![Some(0), Some(1), Some(2)]);
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p carapace --lib script::tests::origins_capture_line_and_call_per_node script::tests::one_call_emitting_two_nodes_shares_a_call_ordinal script::tests::a_loop_yields_same_line_distinct_calls`
Expected: FAIL — `skin.origins` field does not exist (compile error).

- [ ] **Step 4: Add `origins`/`call_seq` to `SceneBuilder` and capture in the ctor closure**

In `crates/carapace/src/script.rs`:

1. Import `Origin`: change `use crate::scene::{HandlerId, Node, Scene};` (line 9) to
   ```rust
   use crate::scene::{HandlerId, Node, Origin, Scene};
   ```

2. Add fields to `SceneBuilder` (after `anchors: Vec<crate::layout::Anchors>,`, line 51):
   ```rust
       origins: Vec<Origin>,
       call_seq: u32,
   ```

3. Initialize them in the `Rc::new(RefCell::new(SceneBuilder { … }))` literal (`script.rs:120-125`), after `anchors: Vec::new(),`:
   ```rust
           origins: Vec::new(),
           call_seq: 0,
   ```

4. Replace the constructor closure body (`script.rs:133-148`) with the origin-aware version — note `_` becomes `lua`, and the anchors/origins pushes are merged into one loop:
   ```rust
           let ctor = lua.create_function(move |lua, args: Table| {
               let prim = registry
                   .iter()
                   .find(|p| p.id() == id_for_closure)
                   .expect("primitive id stable for skin lifetime");
               // Line of the `fill{…}`/`text{…}` call in the skin chunk (level 1 = Lua caller).
               let line = lua
                   .inspect_stack(1, |d| d.current_line())
                   .flatten()
                   .map(|n| n as u32);
               let mut b = builder.borrow_mut();
               let nodes = prim
                   .build(&args, &mut *b)
                   .map_err(|e| mlua::Error::external(format!("{e:?}")))?;
               let anchors = parse_anchors(&args)?;
               let call = b.call_seq;
               b.call_seq += 1;
               for _ in &nodes {
                   b.anchors.push(anchors);
                   b.origins.push(Origin { line, call: Some(call) });
               }
               b.nodes.extend(nodes);
               Ok(())
           })?;
   ```

5. Extend the drain at `script.rs:226-233` to also take `origins`:
   ```rust
       let (nodes, builder_anchors, builder_origins, specs) = {
           let mut b = builder.borrow_mut();
           (
               std::mem::take(&mut b.nodes),
               std::mem::take(&mut b.anchors),
               std::mem::take(&mut b.origins),
               std::mem::take(&mut b.handlers),
           )
       };
   ```

6. Add the field to `LoadedSkin` (after `pub anchors: Vec<crate::layout::Anchors>,`, line 42):
   ```rust
       pub origins: Vec<Origin>,
   ```

7. Set it in the returned `LoadedSkin { … }` literal (`script.rs:242-251`), after `anchors: builder_anchors,`:
   ```rust
           origins: builder_origins,
   ```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p carapace --lib script::`
Expected: PASS — the three new tests plus all existing `script` tests.

- [ ] **Step 6: Expose `scene_origins()` on `Engine`**

In `crates/carapace/src/engine.rs`, add directly after `scene_anchors` (`engine.rs:131-133`):

```rust
    /// The per-node source origins parallel to `scene().nodes` (the design scene). For the
    /// post-layout scene, use `layout_with_origins`.
    pub fn scene_origins(&self) -> &[crate::scene::Origin] {
        &self.skin.origins
    }
```

- [ ] **Step 7: Verify the crate builds and all tests pass**

Run: `cargo test -p carapace --lib`
Expected: PASS (all tests). Run `cargo build -p carapace` — clean.

- [ ] **Step 8: Commit**

```bash
git add crates/carapace/src/scene.rs crates/carapace/src/script.rs crates/carapace/src/engine.rs
git commit -m "feat(engine): capture per-node source origins at skin load"
```

---

### Task 2: Carry origins through layout (`layout_with_origins` + origin-aware `expand_lists`)

**Files:**
- Modify: `crates/carapace/src/engine.rs` (`layout`, new `resolve_expand` + `layout_with_origins`, `expand_lists` signature)

**Interfaces:**
- Consumes: `LoadedSkin.origins` (Task 1), `crate::scene::Origin`.
- Produces:
  - `Engine::layout_with_origins(&self, logical_w: f32, logical_h: f32) -> (Scene, Vec<Origin>)` — origins aligned 1:1 with the returned `scene.nodes`.
  - `Engine::layout` unchanged in signature/behavior.

- [ ] **Step 1: Write the failing tests**

`engine.rs` has no test module yet. Add one at the end of `crates/carapace/src/engine.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::new_queue;
    use crate::fixture::FixtureHost;
    use crate::host::{ActionSpec, Host, Row, Value};
    use crate::scene::Node;
    use crate::state::StateValue;
    use crate::vocab::VocabRegistry;
    use std::time::Duration;

    fn engine_with(host: Box<dyn Host>, lua: &str, canvas: (u32, u32)) -> Engine {
        Engine::new(host, VocabRegistry::base(), SkinSource::inline(lua, canvas)).unwrap()
    }

    #[test]
    fn layout_with_origins_aligns_and_survives_resolve() {
        let e = engine_with(
            Box::new(FixtureHost::new()),
            "fill{ path={{x=0,y=0},{x=10,y=0},{x=10,y=5}}, color={r=1,g=2,b=3} }\n\
             fill{ path={{x=0,y=0},{x=5,y=0},{x=5,y=5}}, color={r=4,g=5,b=6} }",
            (100, 60),
        );
        let (scene, origins) = e.layout_with_origins(100.0, 60.0);
        assert_eq!(scene.nodes.len(), origins.len());
        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0].line, Some(1));
        assert_eq!(origins[1].line, Some(2));
    }

    #[test]
    fn layout_matches_layout_with_origins_scene() {
        let e = engine_with(
            Box::new(FixtureHost::new()),
            "fill{ path={{x=0,y=0},{x=10,y=0},{x=10,y=5}}, color={r=1,g=2,b=3} }",
            (100, 60),
        );
        // The scene from the unchanged `layout` equals the scene half of `layout_with_origins`.
        assert_eq!(e.layout(100.0, 60.0).summary(), e.layout_with_origins(100.0, 60.0).0.summary());
    }

    // Host that returns two rows for any collection, so a `list{}` expands.
    struct RowsHost;
    impl Host for RowsHost {
        fn name(&self) -> &str { "rows" }
        fn tick(&mut self, _dt: Duration) {}
        fn get(&self, _key: &str) -> Option<StateValue> { None }
        fn actions(&self) -> &[ActionSpec] { &[] }
        fn invoke(&mut self, _action: &str, _args: &[Value]) {}
        fn rows(&self, _collection: &str) -> Vec<Row> {
            vec![
                Row::new().set("name", StateValue::Str("a".into())),
                Row::new().set("name", StateValue::Str("b".into())),
            ]
        }
    }

    #[test]
    fn list_expansion_marks_generated_rows_as_call_none() {
        let e = engine_with(
            Box::new(RowsHost),
            "list{ collection='entries', x=10, y=20, w=100, h=80, row_height=20, \
             template={ { bind='name', x=4, y=3, size=12, color={r=1,g=2,b=3} } } }",
            (200, 120),
        );
        let (scene, origins) = e.layout_with_origins(200.0, 120.0);
        assert_eq!(scene.nodes.len(), origins.len());
        // Node 0 is the retained List (real call); the rest are generated rows.
        assert!(matches!(scene.nodes[0], Node::List { .. }));
        assert!(origins[0].call.is_some(), "the list{{}} call is real");
        assert!(origins.len() > 1, "rows were generated");
        assert!(
            origins[1..].iter().all(|o| o.call.is_none()),
            "generated rows carry no call ordinal"
        );
        assert!(
            origins[1..].iter().all(|o| o.line == origins[0].line),
            "generated rows inherit the list's source line"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace --lib engine::tests`
Expected: FAIL — `layout_with_origins` not found.

- [ ] **Step 3: Make `expand_lists` origin-aware**

In `crates/carapace/src/engine.rs`, change the `expand_lists` signature and body to thread a parallel origins vec. Replace the function (`engine.rs:156-243`):

```rust
/// Replace each `Node::List` with [retained List (count=n), then n×template Text rows], keeping
/// `origins` aligned 1:1 with the rebuilt node list. Generated nodes (highlight + rows) inherit the
/// list's source line with `call: None`. Pure Rust — no Lua.
fn expand_lists(scene: &mut Scene, host: &dyn Host, origins: &mut Vec<crate::scene::Origin>) {
    use crate::scene::{Node, Origin, Paint, Pt};

    let old_nodes = std::mem::take(&mut scene.nodes);
    let old_origins = std::mem::take(origins);
    let mut out = Vec::with_capacity(old_nodes.len());
    let mut out_origins: Vec<Origin> = Vec::with_capacity(old_nodes.len());

    for (node, origin) in old_nodes.into_iter().zip(old_origins) {
        let Node::List {
            collection,
            region,
            row_height,
            on_select,
            count: _,
            template,
            highlight,
            selected,
        } = node
        else {
            out.push(node);
            out_origins.push(origin);
            continue;
        };

        // Every node produced from this list inherits its line but is engine-generated.
        let gen = Origin { line: origin.line, call: None };

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
            highlight,
            selected: selected.clone(),
        });
        out_origins.push(origin);

        if let (Some(color), Some(key)) = (highlight, selected.as_deref())
            && let Some(StateValue::Scalar(s)) = host.get(key)
        {
            let idx = s.max(0.0) as usize;
            if idx < n {
                let top = region.y + idx as f32 * row_height;
                let bottom = top + row_height;
                out.push(Node::Fill {
                    path: vec![
                        Pt { x: region.x, y: top },
                        Pt { x: region.x + region.w, y: top },
                        Pt { x: region.x + region.w, y: bottom },
                        Pt { x: region.x, y: bottom },
                    ],
                    paint: Paint::Solid(color),
                });
                out_origins.push(gen);
            }
        }

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
                out_origins.push(gen);
            }
        }
    }
    scene.nodes = out;
    *origins = out_origins;
}
```

> This is the existing `expand_lists` logic verbatim, with an `origins`/`out_origins` vec threaded through and `use crate::scene::StateValue;` no longer needed at fn scope — keep the existing `use crate::state::StateValue;` import that the module already relies on (the original `expand_lists` referenced `StateValue` via the `crate::scene::...` re-export path already in scope; if the compiler reports `StateValue` unresolved, add `use crate::state::StateValue;` inside the fn).

- [ ] **Step 4: Add `resolve_expand` + `layout_with_origins`, and delegate `layout`**

Replace `Engine::layout` (`engine.rs:139-147`) with the delegating version plus the new method, and add the private helper. In the `impl Engine` block:

```rust
    /// Resolve the design scene to a logical scene for the given window logical size, using the
    /// skin's per-element anchors. The result's `canvas` equals the logical size. Frame skins call
    /// this on resize; gadget skins render the design scene directly.
    pub fn layout(&self, logical_w: f32, logical_h: f32) -> Scene {
        self.resolve_expand(logical_w, logical_h).0
    }

    /// Like `layout`, but also returns per-node source origins aligned 1:1 with the returned
    /// `scene.nodes`. For authoring tools (the preview inspector). See `scene_origins` for the
    /// pre-layout design origins.
    pub fn layout_with_origins(&self, logical_w: f32, logical_h: f32) -> (Scene, Vec<crate::scene::Origin>) {
        self.resolve_expand(logical_w, logical_h)
    }

    fn resolve_expand(&self, logical_w: f32, logical_h: f32) -> (Scene, Vec<crate::scene::Origin>) {
        let mut scene =
            crate::layout::resolve_scene(&self.skin.scene, &self.skin.anchors, (logical_w, logical_h));
        // resolve_scene preserves node order 1:1, so design origins line up with the resolved nodes.
        let mut origins = self.skin.origins.clone();
        expand_lists(&mut scene, self.host.as_ref(), &mut origins);
        (scene, origins)
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p carapace --lib engine::tests`
Expected: PASS (3 tests). Then `cargo test -p carapace --lib` — all existing tests still pass (layout behavior unchanged).

- [ ] **Step 6: Commit**

```bash
git add crates/carapace/src/engine.rs
git commit -m "feat(engine): carry node origins through layout (layout_with_origins)"
```

---

### Task 3: `Scene::pick` + public `node_bbox`

**Files:**
- Modify: `crates/carapace/src/layout.rs` (make `node_bbox` `pub`)
- Modify: `crates/carapace/src/scene.rs` (add `Scene::pick`)

**Interfaces:**
- Consumes: `crate::layout::node_bbox`, `Rect` (`layout.rs:6`).
- Produces:
  - `carapace::layout::node_bbox(node: &Node) -> Option<Rect>` (now `pub`).
  - `Scene::pick(&self, p: Pt) -> Option<usize>` — index of the topmost node whose bbox contains `p`, skipping zero-area nodes.

- [ ] **Step 1: Write the failing tests in `scene.rs`**

Add to the existing `#[cfg(test)] mod tests` in `crates/carapace/src/scene.rs` (match the module's existing imports; `Node`, `Pt`, `Paint`, `Color`, `Scene` are all in `crate::scene`):

```rust
    #[test]
    fn pick_returns_topmost_node_by_bbox() {
        fn fill(pts: &[(f32, f32)]) -> Node {
            Node::Fill {
                path: pts.iter().map(|&(x, y)| Pt { x, y }).collect(),
                paint: Paint::Solid(Color { r: 0, g: 0, b: 0, a: 255 }),
            }
        }
        let scene = Scene {
            nodes: vec![
                fill(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]), // big, drawn first
                fill(&[(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)]), // small, drawn on top
            ],
            canvas: (100, 100),
        };
        assert_eq!(scene.pick(Pt { x: 20.0, y: 20.0 }), Some(1), "topmost wins");
        assert_eq!(scene.pick(Pt { x: 80.0, y: 80.0 }), Some(0), "only the big fill");
        assert_eq!(scene.pick(Pt { x: 200.0, y: 200.0 }), None, "empty space");
    }

    #[test]
    fn pick_skips_zero_area_nodes() {
        // A degenerate single-point path has a zero-area bbox — same as a Text node (which
        // node_bbox reports as a zero-size point). Neither is pickable.
        let scene = Scene {
            nodes: vec![Node::Fill {
                path: vec![Pt { x: 5.0, y: 5.0 }],
                paint: Paint::Solid(Color { r: 0, g: 0, b: 0, a: 255 }),
            }],
            canvas: (50, 50),
        };
        assert_eq!(scene.pick(Pt { x: 5.0, y: 5.0 }), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace --lib scene::tests::pick_returns_topmost_node_by_bbox scene::tests::pick_skips_zero_area_nodes`
Expected: FAIL — `Scene::pick` not found.

- [ ] **Step 3: Make `node_bbox` public**

In `crates/carapace/src/layout.rs:79`, change:
```rust
fn node_bbox(node: &Node) -> Option<Rect> {
```
to:
```rust
/// The design/logical bounding box of a node (rect for rect-nodes; point-bbox for text;
/// path bbox otherwise). `None` for nodes without geometry. Public for authoring tools that
/// render selection outlines and pick nodes.
pub fn node_bbox(node: &Node) -> Option<Rect> {
```

- [ ] **Step 4: Add `Scene::pick`**

In `crates/carapace/src/scene.rs`, add to the `impl Scene { … }` block (near the other query methods like `hit_any`):

```rust
    /// Index of the topmost (last in z-order) node whose bounding box contains `p`. Zero-area
    /// nodes (Text has no measured size) are skipped. This is a scene-level pick for authoring
    /// tools — broader than `hit_any`, which dispatches only interactive kinds. Call on a
    /// layout-resolved scene so bounds are in logical coordinates.
    pub fn pick(&self, p: Pt) -> Option<usize> {
        self.nodes.iter().enumerate().rev().find_map(|(i, node)| {
            let b = crate::layout::node_bbox(node)?;
            let inside = b.w > 0.0
                && b.h > 0.0
                && p.x >= b.x
                && p.x <= b.x + b.w
                && p.y >= b.y
                && p.y <= b.y + b.h;
            inside.then_some(i)
        })
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p carapace --lib scene::`
Expected: PASS (both new tests + existing scene tests).

- [ ] **Step 6: Full crate check + clippy + fmt**

Run:
```bash
cargo test -p carapace --lib
cargo clippy -p carapace --all-targets -- -D warnings
cargo fmt -p carapace
git diff --stat
```
Expected: all tests pass; clippy clean; fmt no-ops (or only touches these files). Confirm no unrelated files changed.

- [ ] **Step 7: Commit**

```bash
git add crates/carapace/src/layout.rs crates/carapace/src/scene.rs
git commit -m "feat(engine): Scene::pick topmost node + public node_bbox"
```

---

## Self-Review checklist (run after implementing)

- **API surface delivered:** `scene::Origin`, `LoadedSkin.origins`, `Engine::scene_origins`, `Engine::layout_with_origins`, origin-aware `expand_lists`, `Scene::pick`, `pub layout::node_bbox`. ✔ against the design spec's "What we add".
- **Zero behavior change:** `layout()` signature unchanged and its scene output equals `layout_with_origins().0` (asserted in `layout_matches_layout_with_origins_scene`); `Scene` shape unchanged (no literal-site churn). Run the full workspace once: `cargo test --workspace` and `cargo build -p carapace-ffi` to confirm downstream crates still compile.
- **Loop/generated semantics:** loop → same line, distinct calls; fill+hotspot → shared call; list rows → `call: None`. All asserted.
- **No placeholders:** every step has exact code and commands.
```
</content>

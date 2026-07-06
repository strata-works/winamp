use std::rc::Rc;
use std::time::Duration;

use crate::command::{Command, Queue, SkinSource, new_queue};
use crate::host::Host;
use crate::scene::{Pt, Scene};
use crate::script::{LoadedSkin, ScriptError};
use crate::state::StateValue;
use crate::swap::rebuild;
use crate::vocab::VocabRegistry;

/// A pointer input event fed to [`Engine::handle_pointer`] / [`Engine::handle_pointer_resolved`].
pub enum PointerEvent {
    /// A press (e.g. mouse-down / touch-down) at the given point.
    Press,
}

/// An opaque handle to a loaded skin: the running Lua-built [`Scene`], its layout anchors, and the
/// [`Host`] it talks to.
///
/// Construct with [`Engine::new`], drive it once per frame by feeding input (`handle_pointer` /
/// `handle_pointer_resolved` / `handle_command`) and calling [`Engine::update`], then read geometry
/// with [`Engine::layout`] or [`Engine::scene`] to render. `Engine` is `!Send`/`!Sync` — construct,
/// drive, and drop it on a single thread.
pub struct Engine {
    host: Box<dyn Host>,
    registry: Rc<VocabRegistry>,
    queue: Queue,
    skin: LoadedSkin,
}

impl Engine {
    /// Construct the engine: wraps `registry` in an `Rc`, creates the command queue, and builds
    /// the initial skin. Errors if the Lua entry script fails to load or run.
    pub fn new(
        host: Box<dyn Host>,
        registry: VocabRegistry,
        initial: SkinSource,
    ) -> Result<Engine, ScriptError> {
        let registry = Rc::new(registry);
        let queue = new_queue();
        let skin = rebuild(&initial, host.as_ref(), registry.clone(), queue.clone())?;
        Ok(Engine {
            host,
            registry,
            queue,
            skin,
        })
    }

    /// Hit-test the **design** scene at `p` and fire the matched hotspot's handler.
    ///
    /// Phase 1 (input): resolve the hit and run the handler, which only enqueues commands (errors
    /// are logged, not propagated).
    pub fn handle_pointer(&mut self, p: Pt, _kind: PointerEvent) {
        if let Some(id) = self.skin.scene.hit(p)
            && let Err(e) = self.skin.fire(id)
        {
            // A bad handler drops its command(s); the loop continues.
            eprintln!("carapace: handler error: {e:?}");
        }
    }

    /// Like [`handle_pointer`](Engine::handle_pointer), but hit-tests the **layout-resolved** scene
    /// for the given logical window size, so anchored/frame hotspots are hit where they are
    /// actually drawn rather than at their design positions. `p` is in logical (window-point)
    /// coordinates.
    ///
    /// Dispatches hotspot handlers, list-row selects, and scrub seeks.
    pub fn handle_pointer_resolved(
        &mut self,
        logical_w: f32,
        logical_h: f32,
        p: Pt,
        _kind: PointerEvent,
    ) {
        use crate::scene::Hit;
        let scene = self.layout(logical_w, logical_h);
        // Single z-ordered hit: the topmost interactive node wins, regardless of kind, so a
        // list row / scrub drawn over a background drag hotspot beats it.
        match scene.hit_any(p) {
            Some(Hit::Handler(id)) => {
                if let Err(e) = self.skin.fire(id) {
                    eprintln!("carapace: handler error: {e:?}");
                }
            }
            Some(Hit::Row { action, index }) => {
                self.queue.borrow_mut().push(Command::HostAction {
                    action,
                    args: vec![crate::host::Value::Num(index as f64)],
                });
            }
            Some(Hit::Scrub { action, fraction }) => {
                self.queue.borrow_mut().push(Command::HostAction {
                    action,
                    args: vec![crate::host::Value::Num(fraction as f64)],
                });
            }
            None => {}
        }
    }

    /// Enqueue a meta command (host-app-level, not from skin picking).
    pub fn handle_command(&mut self, cmd: Command) {
        self.queue.borrow_mut().push(cmd);
    }

    /// Drain queued commands and tick the host.
    ///
    /// Phase 2 (drain) + Phase 3 (tick): validates each `HostAction` against the current host's
    /// `actions()` allowlist before invoking it, applies `Swap`/`SwitchHost` transactionally, then
    /// calls `host.tick(dt)`.
    pub fn update(&mut self, dt: Duration) {
        let cmds: Vec<Command> = std::mem::take(&mut *self.queue.borrow_mut());
        for cmd in cmds {
            match cmd {
                Command::HostAction { action, args } => {
                    // Validate against the CURRENT host's allowlist (handles post-switch).
                    if self.host.actions().iter().any(|a| a.name == action) {
                        self.host.invoke(&action, &args);
                    } else {
                        eprintln!("carapace: dropped action '{action}' not in host allowlist");
                    }
                }
                Command::Swap(source) => self.apply_swap(&source),
                Command::SwitchHost { host, skin } => {
                    self.host = host;
                    self.apply_swap(&skin);
                }
            }
        }
        self.host.tick(dt);
    }

    fn apply_swap(&mut self, source: &SkinSource) {
        match rebuild(
            source,
            self.host.as_ref(),
            self.registry.clone(),
            self.queue.clone(),
        ) {
            Ok(skin) => self.skin = skin, // transactional: only replace on success
            Err(e) => eprintln!("carapace: swap failed, keeping current skin: {e:?}"),
        }
    }

    /// The current **design** scene (unresolved).
    pub fn scene(&self) -> &Scene {
        &self.skin.scene
    }

    /// Per-node anchors, parallel to [`scene`](Engine::scene)`().nodes`.
    pub fn scene_anchors(&self) -> &[crate::layout::Anchors] {
        &self.skin.anchors
    }

    /// Per-node source origins for the design scene, parallel to
    /// [`scene`](Engine::scene)`().nodes`. For the post-layout scene, use
    /// [`layout_with_origins`](Engine::layout_with_origins).
    pub fn scene_origins(&self) -> &[crate::scene::Origin] {
        &self.skin.origins
    }

    /// Resolve the design scene to a logical scene for a window size (result `canvas` equals the
    /// logical size); expands `List` nodes into rows.
    ///
    /// Frame skins call this on resize; gadget skins render [`scene`](Engine::scene) directly.
    pub fn layout(&self, logical_w: f32, logical_h: f32) -> Scene {
        self.resolve_expand(logical_w, logical_h).0
    }

    /// Same as [`layout`](Engine::layout) but also returns origins aligned 1:1 with the resolved
    /// nodes — for authoring tools.
    pub fn layout_with_origins(
        &self,
        logical_w: f32,
        logical_h: f32,
    ) -> (Scene, Vec<crate::scene::Origin>) {
        self.resolve_expand(logical_w, logical_h)
    }

    fn resolve_expand(&self, logical_w: f32, logical_h: f32) -> (Scene, Vec<crate::scene::Origin>) {
        let mut scene = crate::layout::resolve_scene(
            &self.skin.scene,
            &self.skin.anchors,
            (logical_w, logical_h),
        );
        // resolve_scene preserves node order 1:1, so design origins line up with the resolved nodes.
        let mut origins = self.skin.origins.clone();
        expand_lists(&mut scene, self.host.as_ref(), &mut origins);
        (scene, origins)
    }

    /// Read a host data value by key (delegates to [`Host::get`]).
    pub fn state(&self, key: &str) -> Option<StateValue> {
        self.host.get(key)
    }
}

/// Replace each `Node::List` with [retained List (count=n), then n×template Text rows], keeping
/// `origins` aligned 1:1 with the rebuilt node list. Generated nodes (highlight + rows) inherit the
/// list's source line with `call: None`. Pure Rust — no Lua.
fn expand_lists(scene: &mut Scene, host: &dyn Host, origins: &mut Vec<crate::scene::Origin>) {
    use crate::scene::{Node, Origin, Paint, Pt};

    debug_assert_eq!(
        scene.nodes.len(),
        origins.len(),
        "expand_lists: origins must be parallel to scene.nodes"
    );

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
        let generated = Origin {
            line: origin.line,
            call: None,
        };

        let fields: Vec<&str> = template.iter().map(|c| c.bind.as_str()).collect();
        let rows = host.rows_for(&collection, &fields);
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

        // Selection highlight: a bar behind the row whose index == the host scalar at `selected`.
        if let (Some(color), Some(key)) = (highlight, selected.as_deref())
            && let Some(StateValue::Scalar(s)) = host.get(key)
        {
            let idx = s.max(0.0) as usize;
            if idx < n {
                let top = region.y + idx as f32 * row_height;
                let bottom = top + row_height;
                out.push(Node::Fill {
                    path: vec![
                        Pt {
                            x: region.x,
                            y: top,
                        },
                        Pt {
                            x: region.x + region.w,
                            y: top,
                        },
                        Pt {
                            x: region.x + region.w,
                            y: bottom,
                        },
                        Pt {
                            x: region.x,
                            y: bottom,
                        },
                    ],
                    paint: Paint::Solid(color),
                });
                out_origins.push(generated);
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
                out_origins.push(generated);
            }
        }
    }
    scene.nodes = out;
    *origins = out_origins;
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(
            e.layout(100.0, 60.0).summary(),
            e.layout_with_origins(100.0, 60.0).0.summary()
        );
    }

    // Host that returns two rows for any collection, so a `list{}` expands.
    struct RowsHost;
    impl Host for RowsHost {
        fn name(&self) -> &str {
            "rows"
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

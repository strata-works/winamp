use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use mlua::{Function, Lua, RegistryKey, Table, Value as LuaValue};

use crate::command::{Command, Queue, SkinSource};
use crate::host::{Host, Value};
use crate::scene::{HandlerId, Node, Scene};
use crate::vocab::{BuildContext, BuildError, VocabRegistry};

#[derive(Debug)]
pub enum ScriptError {
    Lua(mlua::Error),
    Build(BuildError),
}
impl From<mlua::Error> for ScriptError {
    fn from(e: mlua::Error) -> Self {
        ScriptError::Lua(e)
    }
}
impl From<BuildError> for ScriptError {
    fn from(e: BuildError) -> Self {
        ScriptError::Build(e)
    }
}

/// What the builder collects while a skin runs (pre Lua-registry).
enum HandlerSpec {
    Lua(Function),
    HostAction { action: String, args: Vec<Value> },
}

/// What a loaded skin stores; `fire` dispatches on the kind.
enum Handler {
    Lua(RegistryKey),
    HostAction { action: String, args: Vec<Value> },
}

pub struct LoadedSkin {
    pub scene: Scene,
    pub anchors: Vec<crate::layout::Anchors>,
    lua: Lua,
    handlers: Vec<Handler>,
    queue: Queue,
}

/// Collects nodes built by primitives and registers their Lua handlers.
struct SceneBuilder {
    nodes: Vec<Node>,
    anchors: Vec<crate::layout::Anchors>,
    handlers: Vec<HandlerSpec>,
    assets: std::rc::Rc<crate::asset::AssetResolver>,
}
impl BuildContext for SceneBuilder {
    fn register_handler(&mut self, f: Function) -> HandlerId {
        self.handlers.push(HandlerSpec::Lua(f));
        self.handlers.len() - 1
    }
    fn host_action(&mut self, action: &str, args: Vec<Value>) -> HandlerId {
        self.handlers.push(HandlerSpec::HostAction {
            action: action.to_string(),
            args,
        });
        self.handlers.len() - 1
    }
    fn image(
        &mut self,
        name: &str,
    ) -> Result<Arc<crate::asset::DecodedImage>, crate::asset::AssetError> {
        self.assets.image(name)
    }
    fn font(
        &mut self,
        name: &str,
    ) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError> {
        self.assets.font(name)
    }
}

fn lua_args_to_values(args: mlua::MultiValue) -> Vec<Value> {
    args.into_iter()
        .filter_map(|v| match v {
            LuaValue::Boolean(b) => Some(Value::Bool(b)),
            LuaValue::Integer(i) => Some(Value::Num(i as f64)),
            LuaValue::Number(n) => Some(Value::Num(n)),
            LuaValue::String(s) => s.to_str().ok().map(|s| Value::Str(s.to_string())),
            _ => None,
        })
        .collect()
}

fn parse_anchors(args: &Table) -> mlua::Result<crate::layout::Anchors> {
    use crate::layout::Anchors;
    let edges: Vec<String> = match args.get::<Option<Table>>("anchor")? {
        Some(t) => t
            .sequence_values::<String>()
            .filter_map(|v| v.ok())
            .collect(),
        None => return Ok(Anchors::TOP_LEFT),
    };
    let refs: Vec<&str> = edges.iter().map(|s| s.as_str()).collect();
    let mut a = Anchors::from_edges(&refs);
    if let Some(m) = args.get::<Option<Table>>("min")? {
        let w: f32 = m.get::<Option<f32>>("w")?.unwrap_or(0.0);
        let h: f32 = m.get::<Option<f32>>("h")?.unwrap_or(0.0);
        a.min = Some((w, h));
    }
    Ok(a)
}

pub fn load(
    source: &SkinSource,
    host: &dyn Host,
    registry: Rc<VocabRegistry>,
    queue: Queue,
) -> Result<LoadedSkin, ScriptError> {
    let lua = Lua::new();
    let env = lua.create_table()?;
    let builder = Rc::new(RefCell::new(SceneBuilder {
        nodes: Vec::new(),
        anchors: Vec::new(),
        handlers: Vec::new(),
        assets: source.assets.clone(),
    }));

    // One Lua constructor per registry primitive id (data-driven — not hardcoded).
    let ids: Vec<String> = registry.iter().map(|p| p.id().to_string()).collect();
    for id in ids {
        let registry = registry.clone();
        let builder = builder.clone();
        let id_for_closure = id.clone();
        let ctor = lua.create_function(move |_, args: Table| {
            let prim = registry
                .iter()
                .find(|p| p.id() == id_for_closure)
                .expect("primitive id stable for skin lifetime");
            let mut b = builder.borrow_mut();
            let nodes = prim
                .build(&args, &mut *b)
                .map_err(|e| mlua::Error::external(format!("{e:?}")))?;
            let anchors = parse_anchors(&args)?;
            for _ in &nodes {
                b.anchors.push(anchors);
            }
            b.nodes.extend(nodes);
            Ok(())
        })?;
        env.set(id, ctor)?;
    }

    // host table: one enqueue-shim per allowlisted action.
    let host_tbl = lua.create_table()?;
    for spec in host.actions() {
        let name = spec.name; // &'static str
        let queue = queue.clone();
        let shim = lua.create_function(move |_, args: mlua::MultiValue| {
            queue.borrow_mut().push(Command::HostAction {
                action: name.to_string(),
                args: lua_args_to_values(args),
            });
            Ok(())
        })?;
        host_tbl.set(name, shim)?;
    }
    env.set("host", host_tbl)?;

    // Base geometry sugar: pure path-generators injected into the sandbox. They return a
    // sequence of {x=,y=} usable in any `path=`; they emit no nodes and carry no capability.
    fn points_table(lua: &Lua, pts: &[crate::scene::Pt]) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        for (i, p) in pts.iter().enumerate() {
            let pt = lua.create_table()?;
            pt.set("x", p.x)?;
            pt.set("y", p.y)?;
            t.set(i + 1, pt)?;
        }
        Ok(t)
    }
    let circle = lua.create_function(|lua, a: Table| {
        let cx: f32 = a.get("cx")?;
        let cy: f32 = a.get("cy")?;
        let r: f32 = a.get("r")?;
        let segments: u32 = a.get::<Option<u32>>("segments")?.unwrap_or(48);
        points_table(lua, &crate::shape::circle(cx, cy, r, segments))
    })?;
    env.set("circle", circle)?;
    let rect = lua.create_function(|lua, a: Table| {
        let x: f32 = a.get("x")?;
        let y: f32 = a.get("y")?;
        let w: f32 = a.get("w")?;
        let h: f32 = a.get("h")?;
        points_table(lua, &crate::shape::rect(x, y, w, h))
    })?;
    env.set("rect", rect)?;
    let rounded_rect = lua.create_function(|lua, a: Table| {
        let x: f32 = a.get("x")?;
        let y: f32 = a.get("y")?;
        let w: f32 = a.get("w")?;
        let h: f32 = a.get("h")?;
        let radius: f32 = a.get("radius")?;
        let segments: u32 = a.get::<Option<u32>>("segments")?.unwrap_or(8);
        points_table(
            lua,
            &crate::shape::rounded_rect(x, y, w, h, radius, segments),
        )
    })?;
    env.set("rounded_rect", rounded_rect)?;

    // Lua's standard `math` library. Every function in it (sin/cos/sqrt/pi/floor/min/max/random/…)
    // is a PURE, capability-free computation — no filesystem, process, or module access — exactly
    // like the string methods documented below, and genuinely useful for procedural geometry
    // (arcs, radial layouts). Exposing it keeps skins from hand-rolling trig tables. `io`, `os`,
    // `package`, and the other capability-bearing base libraries stay absent.
    env.set("math", lua.globals().get::<Table>("math")?)?;

    // Run the skin once under the sandboxed env.  The env exposes the registry primitive
    // constructors, the `host` table of allowlisted action shims, and the safe `math` library;
    // `io`, `os`, `require`, `load`, and all other base globals are absent.  One deliberate subtlety:
    // Lua's string metatable is wired by the VM at startup and is not detached by swapping
    // `_ENV`, so string methods on literals (e.g. `('x'):upper()`) remain reachable — but
    // they carry no capability (no filesystem, process, or module access) and are useful,
    // so we accept and document them as part of the sandbox contract.
    lua.load(&source.lua_src).set_environment(env).exec()?;

    let (nodes, builder_anchors, specs) = {
        let mut b = builder.borrow_mut();
        (
            std::mem::take(&mut b.nodes),
            std::mem::take(&mut b.anchors),
            std::mem::take(&mut b.handlers),
        )
    };
    let handlers = specs
        .into_iter()
        .map(|s| match s {
            HandlerSpec::Lua(f) => Ok(Handler::Lua(lua.create_registry_value(f)?)),
            HandlerSpec::HostAction { action, args } => Ok(Handler::HostAction { action, args }),
        })
        .collect::<mlua::Result<Vec<_>>>()?;

    Ok(LoadedSkin {
        scene: Scene {
            nodes,
            canvas: source.canvas,
        },
        anchors: builder_anchors,
        lua,
        handlers,
        queue,
    })
}

impl LoadedSkin {
    pub fn fire(&self, id: HandlerId) -> Result<(), ScriptError> {
        match &self.handlers[id] {
            Handler::Lua(key) => {
                let f: Function = self.lua.registry_value(key)?;
                f.call::<()>(())?;
            }
            Handler::HostAction { action, args } => {
                self.queue.borrow_mut().push(Command::HostAction {
                    action: action.clone(),
                    args: args.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::new_queue;
    use crate::fixture::FixtureHost;
    use crate::vocab::VocabRegistry;
    use std::rc::Rc;

    fn src(s: &str) -> SkinSource {
        SkinSource::inline(s, (300, 120))
    }

    #[test]
    fn builds_scene_via_registry() {
        let q = new_queue();
        let skin = load(
            &src("value_fill{ path={{x=0,y=0},{x=10,y=0},{x=10,y=5}}, value='level', color={r=1,g=2,b=3} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        assert_eq!(skin.scene.nodes.len(), 1);
    }

    #[test]
    fn handler_enqueues_command_without_touching_host() {
        let q = new_queue();
        let skin = load(
            &src("region{ path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, on_press=function() host.toggle() end }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q.clone(),
        )
        .unwrap();
        assert!(q.borrow().is_empty());
        skin.fire(0).unwrap();
        assert_eq!(q.borrow().len(), 1);
        match &q.borrow()[0] {
            Command::HostAction { action, .. } => assert_eq!(action, "toggle"),
            _ => panic!("expected HostAction"),
        }
    }

    #[test]
    fn action_args_are_captured() {
        let q = new_queue();
        let skin = load(
            &src("region{ path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, on_press=function() host.bump(0.5) end }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q.clone(),
        )
        .unwrap();
        skin.fire(0).unwrap();
        match &q.borrow()[0] {
            Command::HostAction { action, args } => {
                assert_eq!(action, "bump");
                assert_eq!(args, &vec![Value::Num(0.5)]);
            }
            _ => panic!("expected HostAction"),
        }
    }

    #[test]
    fn sandbox_blocks_globals_and_unknown_names() {
        let reg = Rc::new(VocabRegistry::base());
        for bad in [
            "io.write('x')",
            "os.time()",
            "require('os')",
            "host.nope()",
            "frobnicate{}",
        ] {
            let r = load(&src(bad), &FixtureHost::new(), reg.clone(), new_queue());
            assert!(r.is_err(), "expected sandbox/registry to reject `{bad}`");
        }
    }

    #[test]
    fn shape_helpers_produce_usable_paths() {
        let q = new_queue();
        // A circle path feeds `fill`; the fill builds with the tessellated polygon.
        let skin = load(
            &src("fill{ path = circle{cx=20, cy=20, r=10}, color = {r=1,g=2,b=3} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        match &skin.scene.nodes[0] {
            crate::scene::Node::Fill { path, .. } => {
                assert_eq!(path.len(), 48, "default circle segments");
            }
            other => panic!("expected Fill, got {other:?}"),
        }
    }

    #[test]
    fn math_library_is_available_to_skins() {
        // `math` is exposed to the sandbox (pure + capability-free) so skins can compute geometry.
        // math.sin(pi/2) == 1, so x resolves to 15.
        let skin = load(
            &src(
                "fill{ path = rect{x=10 + math.sin(math.pi/2)*5, y=0, w=10, h=10}, \
                  color={r=0,g=0,b=0} }",
            ),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            new_queue(),
        )
        .expect("a skin using math should load");
        match &skin.scene.nodes[0] {
            crate::scene::Node::Fill { path, .. } => {
                assert_eq!(path[0].x as i32, 15, "math.sin(pi/2)*5 + 10");
            }
            other => panic!("expected Fill, got {other:?}"),
        }
    }

    #[test]
    fn rect_helper_makes_a_clickable_fill() {
        let q = new_queue();
        let skin = load(
            &src(
                "fill{ path = rect{x=0,y=0,w=10,h=10}, color={r=0,g=0,b=0}, \
                       on_press=function() host.toggle() end }",
            ),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        // fill + hotspot from the rect; the hotspot hit-tests inside the rect.
        assert_eq!(skin.scene.nodes.len(), 2);
        assert_eq!(skin.scene.hit(crate::scene::Pt { x: 5.0, y: 5.0 }), Some(0));
    }

    #[test]
    fn host_action_handler_enqueues_directly_without_lua() {
        use crate::command::Command;
        use crate::scene::{Node, Pt, region_of};
        use crate::vocab::{BuildContext, BuildError, Primitive};
        use mlua::Table;

        // A minimal host extension: binds a host action via host_action (no Lua function).
        struct PingPrim;
        impl Primitive for PingPrim {
            fn id(&self) -> &str {
                "ping"
            }
            fn build(
                &self,
                _a: &Table,
                ctx: &mut dyn BuildContext,
            ) -> Result<Vec<Node>, BuildError> {
                let path = vec![
                    Pt { x: 0.0, y: 0.0 },
                    Pt { x: 10.0, y: 0.0 },
                    Pt { x: 10.0, y: 10.0 },
                    Pt { x: 0.0, y: 10.0 },
                ];
                let id = ctx.host_action("toggle", vec![]);
                Ok(vec![Node::Hotspot {
                    region: region_of(&path),
                    on_press: id,
                }])
            }
        }

        let mut reg = VocabRegistry::base();
        reg.register(Box::new(PingPrim));
        let q = new_queue();
        let skin = load(&src("ping{}"), &FixtureHost::new(), Rc::new(reg), q.clone()).unwrap();
        assert!(q.borrow().is_empty());
        skin.fire(0).unwrap();
        match &q.borrow()[0] {
            Command::HostAction { action, args } => {
                assert_eq!(action, "toggle");
                assert!(args.is_empty());
            }
            other => panic!("expected HostAction enqueued directly, got {other:?}"),
        }
    }

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
                ..
            } => {
                assert_eq!(collection, "entries");
                assert_eq!(
                    (region.x, region.y, region.w, region.h),
                    (10.0, 20.0, 100.0, 80.0)
                );
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

    #[test]
    fn scrub_prim_parses_region_value_and_on_seek() {
        use crate::scene::{FillDir, Node};
        let q = new_queue();
        let skin = load(
            &src("scrub{ x=10, y=20, w=200, h=12, value='position', on_seek='seek', color={r=1,g=2,b=3} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        assert_eq!(skin.scene.nodes.len(), 1);
        match &skin.scene.nodes[0] {
            Node::Scrub {
                region,
                value_key,
                direction,
                on_seek,
                ..
            } => {
                assert_eq!(
                    (region.x, region.y, region.w, region.h),
                    (10.0, 20.0, 200.0, 12.0)
                );
                assert_eq!(value_key, "position");
                assert_eq!(on_seek, "seek");
                assert!(matches!(direction, FillDir::Right));
            }
            other => panic!("expected Scrub, got {other:?}"),
        }
    }

    /// String methods on literals are reachable via the string metatable (accepted,
    /// documented sandbox boundary), but `os`, `load`, and `require` remain blocked.
    #[test]
    fn string_methods_reachable_but_capability_free() {
        let reg = Rc::new(VocabRegistry::base());

        // String methods work — this is the accepted, documented behaviour.
        let ok = load(
            &src("local _ = ('x'):upper()"),
            &FixtureHost::new(),
            reg.clone(),
            new_queue(),
        );
        assert!(ok.is_ok(), "string metatable methods must be reachable");

        // Capabilities remain blocked despite the string metatable being present.
        let r_os = load(
            &src("os.time()"),
            &FixtureHost::new(),
            reg.clone(),
            new_queue(),
        );
        assert!(r_os.is_err(), "os.time() must be blocked");

        let r_load = load(
            &src("load('')"),
            &FixtureHost::new(),
            reg.clone(),
            new_queue(),
        );
        assert!(r_load.is_err(), "load() must be blocked");

        let r_req = load(
            &src("require('os')"),
            &FixtureHost::new(),
            reg.clone(),
            new_queue(),
        );
        assert!(r_req.is_err(), "require('os') must be blocked");
    }
}

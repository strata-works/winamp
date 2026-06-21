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

pub struct LoadedSkin {
    pub scene: Scene,
    lua: Lua,
    handlers: Vec<RegistryKey>,
}

/// Collects nodes built by primitives and registers their Lua handlers.
struct SceneBuilder {
    nodes: Vec<Node>,
    handler_fns: Vec<Function>,
    assets: std::rc::Rc<crate::asset::AssetResolver>,
}
impl BuildContext for SceneBuilder {
    fn register_handler(&mut self, f: Function) -> HandlerId {
        self.handler_fns.push(f);
        self.handler_fns.len() - 1
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
        handler_fns: Vec::new(),
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
        points_table(lua, &crate::shape::rounded_rect(x, y, w, h, radius, segments))
    })?;
    env.set("rounded_rect", rounded_rect)?;

    // Run the skin once under the sandboxed env.  The env exposes only the registry
    // primitive constructors + the `host` table of allowlisted action shims; `io`, `os`,
    // `require`, `load`, and all other base globals are absent.  One deliberate subtlety:
    // Lua's string metatable is wired by the VM at startup and is not detached by swapping
    // `_ENV`, so string methods on literals (e.g. `('x'):upper()`) remain reachable — but
    // they carry no capability (no filesystem, process, or module access) and are useful,
    // so we accept and document them as part of the sandbox contract.
    lua.load(&source.lua_src).set_environment(env).exec()?;

    let (nodes, handler_fns) = {
        let mut b = builder.borrow_mut();
        (
            std::mem::take(&mut b.nodes),
            std::mem::take(&mut b.handler_fns),
        )
    };
    let handlers = handler_fns
        .into_iter()
        .map(|f| lua.create_registry_value(f))
        .collect::<mlua::Result<Vec<_>>>()?;

    Ok(LoadedSkin {
        scene: Scene {
            nodes,
            canvas: source.canvas,
        },
        lua,
        handlers,
    })
}

impl LoadedSkin {
    pub fn fire(&self, id: HandlerId) -> Result<(), ScriptError> {
        let f: Function = self.lua.registry_value(&self.handlers[id])?;
        f.call::<()>(())?;
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
    fn rect_helper_makes_a_clickable_fill() {
        let q = new_queue();
        let skin = load(
            &src("fill{ path = rect{x=0,y=0,w=10,h=10}, color={r=0,g=0,b=0}, \
                       on_press=function() host.toggle() end }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        // fill + hotspot from the rect; the hotspot hit-tests inside the rect.
        assert_eq!(skin.scene.nodes.len(), 2);
        assert_eq!(skin.scene.hit(crate::scene::Pt { x: 5.0, y: 5.0 }), Some(0));
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

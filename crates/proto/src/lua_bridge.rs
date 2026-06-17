use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, Lua, RegistryKey, Table};

use crate::host::Host;
use crate::scene::{Color, HandlerId, Node, Pt, Scene};

pub type SharedHost = Rc<RefCell<Box<dyn Host>>>;

pub struct LoadedSkin {
    pub scene: Scene,
    lua: Lua,
    handlers: Vec<RegistryKey>,
    // Kept alive so the Lua env (and its host upvalues) outlive `load`.
    _host: SharedHost,
}

fn parse_path(t: &Table) -> mlua::Result<Vec<Pt>> {
    let path: Table = t.get("path")?;
    let mut pts = Vec::new();
    for entry in path.sequence_values::<Table>() {
        let p = entry?;
        pts.push(Pt { x: p.get("x")?, y: p.get("y")? });
    }
    Ok(pts)
}

fn parse_color(t: &Table) -> mlua::Result<Color> {
    let c: Table = t.get("color")?;
    Ok(Color { r: c.get("r")?, g: c.get("g")?, b: c.get("b")? })
}

pub fn load(lua_src: &str, canvas: (u32, u32), host: SharedHost) -> mlua::Result<LoadedSkin> {
    let lua = Lua::new();
    let nodes: Rc<RefCell<Vec<Node>>> = Rc::new(RefCell::new(Vec::new()));
    let handler_fns: Rc<RefCell<Vec<Function>>> = Rc::new(RefCell::new(Vec::new()));

    let env = lua.create_table()?;

    {
        let nodes = nodes.clone();
        let f = lua.create_function(move |_, t: Table| {
            let path = parse_path(&t)?;
            let color = parse_color(&t)?;
            nodes.borrow_mut().push(Node::Fill { path, color });
            Ok(())
        })?;
        env.set("fill", f)?;
    }
    {
        let nodes = nodes.clone();
        let handler_fns = handler_fns.clone();
        let f = lua.create_function(move |_, t: Table| {
            let path = parse_path(&t)?;
            let on_press: Function = t.get("on_press")?;
            let id = {
                let mut h = handler_fns.borrow_mut();
                h.push(on_press);
                h.len() - 1
            };
            nodes.borrow_mut().push(Node::Hotspot { path, on_press: id });
            Ok(())
        })?;
        env.set("region", f)?;
    }
    {
        let nodes = nodes.clone();
        let f = lua.create_function(move |_, t: Table| {
            let path = parse_path(&t)?;
            let color = parse_color(&t)?;
            let value_key: String = t.get("value")?;
            nodes.borrow_mut().push(Node::ValueFill { path, value_key, color });
            Ok(())
        })?;
        env.set("value_fill", f)?;
    }

    // host table: exactly the actions this host registered, nothing else.
    let host_tbl = lua.create_table()?;
    let action_names: Vec<&'static str> = host.borrow().actions().to_vec();
    for name in action_names {
        let host = host.clone();
        let f = lua.create_function(move |_, ()| {
            host.borrow_mut().invoke(name);
            Ok(())
        })?;
        host_tbl.set(name, f)?;
    }
    env.set("host", host_tbl)?;

    // Run the skin once with `env` as its _ENV — no access to base globals.
    lua.load(lua_src).set_environment(env).exec()?;

    let handlers = handler_fns
        .borrow()
        .iter()
        .map(|f| lua.create_registry_value(f.clone()))
        .collect::<mlua::Result<Vec<_>>>()?;
    let scene = Scene { nodes: nodes.borrow().clone(), canvas };
    Ok(LoadedSkin { scene, lua, handlers, _host: host })
}

impl LoadedSkin {
    pub fn fire(&self, id: HandlerId) -> mlua::Result<()> {
        let f: Function = self.lua.registry_value(&self.handlers[id])?;
        f.call(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{MediaHost, StateValue};

    fn shared(host: impl Host + 'static) -> SharedHost {
        Rc::new(RefCell::new(Box::new(host) as Box<dyn Host>))
    }

    #[test]
    fn builds_scene_nodes_from_lua() {
        let src = r#"
            value_fill{ path = {{x=0,y=0},{x=10,y=0},{x=10,y=5},{x=0,y=5}},
                        value = "position", color = {r=255,g=0,b=0} }
            region{ path = {{x=0,y=0},{x=10,y=0},{x=10,y=10},{x=0,y=10}},
                    on_press = function() host.toggle_play() end }
        "#;
        let skin = load(src, (300, 120), shared(MediaHost::new())).unwrap();
        assert_eq!(skin.scene.nodes.len(), 2);
        match &skin.scene.nodes[0] {
            Node::ValueFill { value_key, .. } => assert_eq!(value_key, "position"),
            other => panic!("expected ValueFill, got {other:?}"),
        }
        assert!(matches!(skin.scene.nodes[1], Node::Hotspot { .. }));
    }

    #[test]
    fn firing_a_handler_invokes_the_host_action() {
        let host = shared(MediaHost::new());
        let src = r#"
            region{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1},{x=0,y=1}},
                    on_press = function() host.toggle_play() end }
        "#;
        let skin = load(src, (10, 10), host.clone()).unwrap();
        assert_eq!(host.borrow().get("playing"), Some(StateValue::Bool(false)));
        skin.fire(0).unwrap();
        assert_eq!(host.borrow().get("playing"), Some(StateValue::Bool(true)));
    }

    #[test]
    fn sandbox_blocks_io_os_require() {
        for forbidden in ["io.write('x')", "os.time()", "require('os')"] {
            let res = load(forbidden, (10, 10), shared(MediaHost::new()));
            assert!(res.is_err(), "expected sandbox to reject `{forbidden}`");
        }
    }

    #[test]
    fn calling_unregistered_host_action_errors() {
        // MediaHost does not register `toggle_sampling`.
        let src = "host.toggle_sampling()";
        let res = load(src, (10, 10), shared(MediaHost::new()));
        assert!(res.is_err(), "calling an unexposed action must error");
    }
}

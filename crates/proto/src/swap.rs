use std::cell::RefCell;
use std::rc::Rc;

use crate::host::{Host, StateValue};
use crate::lua_bridge::{self, LoadedSkin, SharedHost};
use crate::scene::{Pt, Scene};

pub struct Engine {
    host: SharedHost,
    skin: LoadedSkin,
}

impl Engine {
    pub fn new(host: Box<dyn Host>, lua_src: &str, canvas: (u32, u32)) -> mlua::Result<Engine> {
        let host: SharedHost = Rc::new(RefCell::new(host));
        let skin = lua_bridge::load(lua_src, canvas, host.clone())?;
        Ok(Engine { host, skin })
    }

    pub fn tick(&mut self, dt: f32) {
        self.host.borrow_mut().tick(dt);
    }

    /// Rebuild the scene from a new skin. Host state is left untouched.
    pub fn swap(&mut self, lua_src: &str, canvas: (u32, u32)) -> mlua::Result<()> {
        self.skin = lua_bridge::load(lua_src, canvas, self.host.clone())?;
        Ok(())
    }

    pub fn scene(&self) -> &Scene {
        &self.skin.scene
    }

    pub fn state(&self, key: &str) -> Option<StateValue> {
        self.host.borrow().get(key)
    }

    pub fn click(&self, p: Pt) -> mlua::Result<()> {
        if let Some(id) = self.skin.scene.hit(p) {
            self.skin.fire(id)?;
        }
        Ok(())
    }

    pub fn render_with(&self, renderer: &mut crate::render::Renderer) -> crate::render::Pixmap {
        renderer.render(&self.skin.scene, &**self.host.borrow())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::MediaHost;

    const SKIN_A: &str = r#"
        region{ path = {{x=0,y=0},{x=50,y=0},{x=50,y=50},{x=0,y=50}},
                on_press = function() host.toggle_play() end }
        value_fill{ path = {{x=0,y=60},{x=100,y=60},{x=100,y=70},{x=0,y=70}},
                    value = "position", color = {r=255,g=255,b=0} }
    "#;
    const SKIN_B: &str = r#"
        value_fill{ path = {{x=0,y=0},{x=200,y=0},{x=200,y=10},{x=0,y=10}},
                    value = "position", color = {r=0,g=255,b=255} }
    "#;

    #[test]
    fn state_survives_swap() {
        let mut e = Engine::new(Box::new(MediaHost::new()), SKIN_A, (300, 120)).unwrap();
        // start playback and advance
        e.click(Pt { x: 25.0, y: 25.0 }).unwrap(); // toggle_play
        e.tick(3.0);
        let before = e.state("position");
        assert_eq!(before, Some(StateValue::Scalar(0.3)));

        // swap skins mid-playback
        e.swap(SKIN_B, (300, 120)).unwrap();

        // host state is identical; scene was rebuilt from the new skin
        assert_eq!(e.state("position"), before, "position survived the swap");
        assert_eq!(e.scene().nodes.len(), 1, "scene is skin B's, not skin A's");
    }

    #[test]
    fn click_in_empty_area_is_a_noop() {
        let e = Engine::new(Box::new(MediaHost::new()), SKIN_A, (300, 120)).unwrap();
        e.click(Pt { x: 250.0, y: 250.0 }).unwrap(); // no hotspot there
        assert_eq!(e.state("playing"), Some(StateValue::Bool(false)));
    }
}

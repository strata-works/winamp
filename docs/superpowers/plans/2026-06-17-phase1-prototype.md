# Phase 1 — Throwaway Prototype Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a throwaway prototype that surfaces real problems in the three risk areas — free-form hit-testing, the Lua↔host call boundary, and state-survives-swap — across two hosts (media player, system monitor) with two swappable skins each.

**Architecture:** A new disposable crate `crates/proto`. A `Host` exposes a generic capability surface (named scalar/bool state + named actions + `tick`). A sandboxed `mlua` runtime runs each skin's `skin.lua` once to build a `Scene` of nodes that hold *binding keys*, never state. A frame loop ticks the host, renders the scene with vello to an offscreen pixmap, and presents it via softbuffer; clicks resolve against skin-defined geometry with the `hittest` kernel and fire allowlisted host actions. Swapping a skin rebuilds the scene from the untouched host state.

**Tech Stack:** Rust (edition 2024). `mlua` (Lua 5.4, vendored) for scripting; `hittest` (Phase 0) for hit resolution; `vello` + `wgpu` + `pollster` for rendering; `winit` + `softbuffer` for the window; `toml` + `serde` for the manifest.

## Global Constraints

- Rust, **edition 2024**, stable toolchain. New crate `crates/proto`; add it to the workspace `members`.
- This is a **throwaway** prototype (decision 7). Do not modify the Phase 0 crates (`crates/hittest`, `crates/spike-render`). Reuse `hittest` via a path dependency.
- The engine carries **zero domain knowledge**: `host`, `scene`, `lua_bridge`, `swap`, `render` must contain no string literal naming a media or sysmon concept (`play`, `cpu`, `position`, …). Those names live only in `host.rs`'s two `Host` impls and in the skin `.lua`/`.toml` files. A domain name in the generic modules is a defect.
- The skin sandbox env contains **only** `fill`, `region`, `value_fill`, and `host` (whose fields are exactly the current host's registered actions). `io`, `os`, `require`, `print`, and all other base globals are absent.
- Scene nodes hold **binding keys** (e.g. the string `"position"`), never state values.
- Colors are opaque RGBA8; canvas size comes from the skin manifest.
- `mlua`/`vello`/`winit`/`softbuffer` are version-churny. The code targets recent APIs; if a resolved version's signatures differ, align with that version's docs/examples (the test gate is the contract). For vello render-to-texture + readback and the winit/softbuffer window, the Phase 0 files `crates/spike-render/src/vello_backend.rs` and `crates/spike-render/examples/viewer.rs` are known-good templates — copy their patterns.

---

### Task 1: Crate scaffold + `host` module

**Files:**
- Modify: `Cargo.toml` (workspace `members`)
- Create: `crates/proto/Cargo.toml`
- Create: `crates/proto/src/host.rs`
- Create: `crates/proto/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub enum StateValue { Bool(bool), Scalar(f32) }` (derives `Clone, Copy, PartialEq, Debug`)
  - `pub trait Host { fn name(&self) -> &'static str; fn tick(&mut self, dt: f32); fn get(&self, key: &str) -> Option<StateValue>; fn actions(&self) -> &'static [&'static str]; fn invoke(&mut self, action: &str); }`
  - `pub struct MediaHost { … }` with `MediaHost::new() -> Self`
  - `pub struct SysmonHost { … }` with `SysmonHost::new() -> Self`

- [ ] **Step 1: Add the crate to the workspace**

Edit the root `Cargo.toml` `members` array to include `"crates/proto"`:

```toml
[workspace]
members = ["crates/hittest", "crates/spike-render", "crates/proto"]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

Create `crates/proto/Cargo.toml`:

```toml
[package]
name = "proto"
version = "0.0.0"
edition = "2024"

[dependencies]
hittest = { path = "../hittest" }

[[bin]]
name = "proto"
path = "src/main.rs"
```

(Later tasks add more dependencies and `src/main.rs`. Until `main.rs` exists, build the library with `cargo build -p proto --lib` or run tests with `cargo test -p proto --lib`.)

- [ ] **Step 3: Create the lib root declaring the module**

Create `crates/proto/src/lib.rs`:

```rust
pub mod host;
```

- [ ] **Step 4: Write the failing tests for the hosts**

Create `crates/proto/src/host.rs`:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StateValue {
    Bool(bool),
    Scalar(f32),
}

/// A host exposes a generic capability surface. The engine knows none of the
/// concrete names — only this trait.
pub trait Host {
    fn name(&self) -> &'static str;
    fn tick(&mut self, dt: f32);
    fn get(&self, key: &str) -> Option<StateValue>;
    fn actions(&self) -> &'static [&'static str];
    fn invoke(&mut self, action: &str);
}

pub struct MediaHost {
    playing: bool,
    position: f32,
}

impl MediaHost {
    pub fn new() -> Self {
        Self { playing: false, position: 0.0 }
    }
}

impl Host for MediaHost {
    fn name(&self) -> &'static str {
        "media"
    }
    fn tick(&mut self, dt: f32) {
        if self.playing {
            self.position = (self.position + dt * 0.1).min(1.0);
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => Some(StateValue::Scalar(self.position)),
            _ => None,
        }
    }
    fn actions(&self) -> &'static [&'static str] {
        &["toggle_play", "stop"]
    }
    fn invoke(&mut self, action: &str) {
        match action {
            "toggle_play" => self.playing = !self.playing,
            "stop" => {
                self.playing = false;
                self.position = 0.0;
            }
            _ => {}
        }
    }
}

pub struct SysmonHost {
    cpu: f32,
    sampling: bool,
    phase: f32,
}

impl SysmonHost {
    pub fn new() -> Self {
        Self { cpu: 0.3, sampling: true, phase: 0.0 }
    }
}

impl Host for SysmonHost {
    fn name(&self) -> &'static str {
        "sysmon"
    }
    fn tick(&mut self, dt: f32) {
        if self.sampling {
            self.phase += dt;
            self.cpu = 0.5 + 0.5 * self.phase.sin();
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "cpu" => Some(StateValue::Scalar(self.cpu)),
            "sampling" => Some(StateValue::Bool(self.sampling)),
            _ => None,
        }
    }
    fn actions(&self) -> &'static [&'static str] {
        &["toggle_sampling"]
    }
    fn invoke(&mut self, action: &str) {
        if action == "toggle_sampling" {
            self.sampling = !self.sampling;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_position_advances_only_while_playing() {
        let mut h = MediaHost::new();
        h.tick(1.0);
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.0)));
        h.invoke("toggle_play");
        h.tick(1.0);
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.1)));
    }

    #[test]
    fn media_stop_resets_position_and_pauses() {
        let mut h = MediaHost::new();
        h.invoke("toggle_play");
        h.tick(2.0);
        h.invoke("stop");
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.0)));
    }

    #[test]
    fn unknown_key_and_action_are_inert() {
        let mut h = MediaHost::new();
        assert_eq!(h.get("nope"), None);
        h.invoke("nope"); // must not panic
    }

    #[test]
    fn sysmon_sampling_toggles_and_freezes_cpu() {
        let mut h = SysmonHost::new();
        h.invoke("toggle_sampling"); // now false
        let before = h.get("cpu");
        h.tick(1.0);
        assert_eq!(h.get("cpu"), before, "cpu frozen while not sampling");
        assert_eq!(h.get("sampling"), Some(StateValue::Bool(false)));
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p proto --lib host`
Expected: PASS (4 passed). (`f32` arithmetic here is exact: `0.0 + 1.0*0.1` and `0.5+0.5*sin` compared to itself.)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/proto/Cargo.toml crates/proto/src/lib.rs crates/proto/src/host.rs
git commit -m "feat(proto): host capability model with media + sysmon hosts"
```

---

### Task 2: `scene` module + hit resolution

**Files:**
- Create: `crates/proto/src/scene.rs`
- Modify: `crates/proto/src/lib.rs`

**Interfaces:**
- Consumes: `hittest::{Region, Contour, Point}`.
- Produces:
  - `pub struct Pt { pub x: f32, pub y: f32 }`
  - `pub struct Color { pub r: u8, pub g: u8, pub b: u8 }`
  - `pub type HandlerId = usize;`
  - `pub enum Node { Fill { path: Vec<Pt>, color: Color }, Hotspot { path: Vec<Pt>, on_press: HandlerId }, ValueFill { path: Vec<Pt>, value_key: String, color: Color } }`
  - `pub struct Scene { pub nodes: Vec<Node>, pub canvas: (u32, u32) }`
  - `impl Scene { pub fn hit(&self, p: Pt) -> Option<HandlerId> }` — topmost (last-drawn) hotspot containing `p`.

- [ ] **Step 1: Declare the module**

Add to `crates/proto/src/lib.rs`:

```rust
pub mod scene;
```

- [ ] **Step 2: Write the failing test**

Create `crates/proto/src/scene.rs`:

```rust
use hittest::{Contour, Point, Region};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub type HandlerId = usize;

#[derive(Clone, Debug)]
pub enum Node {
    Fill { path: Vec<Pt>, color: Color },
    Hotspot { path: Vec<Pt>, on_press: HandlerId },
    ValueFill { path: Vec<Pt>, value_key: String, color: Color },
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub canvas: (u32, u32),
}

fn region_of(path: &[Pt]) -> Region {
    Region {
        contours: vec![Contour {
            points: path.iter().map(|p| Point { x: p.x, y: p.y }).collect(),
        }],
    }
}

impl Scene {
    /// Topmost hotspot containing `p` (later nodes draw on top, so iterate in reverse).
    pub fn hit(&self, p: Pt) -> Option<HandlerId> {
        for node in self.nodes.iter().rev() {
            if let Node::Hotspot { path, on_press } = node {
                if region_of(path).contains(Point { x: p.x, y: p.y }) {
                    return Some(*on_press);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Concave L-shape (same shape family as the Phase 0 kernel test).
    fn l_path() -> Vec<Pt> {
        vec![
            Pt { x: 40.0, y: 40.0 },
            Pt { x: 160.0, y: 40.0 },
            Pt { x: 160.0, y: 90.0 },
            Pt { x: 90.0, y: 90.0 },
            Pt { x: 90.0, y: 160.0 },
            Pt { x: 40.0, y: 160.0 },
        ]
    }

    #[test]
    fn click_inside_hotspot_returns_its_handler() {
        let scene = Scene {
            nodes: vec![Node::Hotspot { path: l_path(), on_press: 7 }],
            canvas: (200, 200),
        };
        assert_eq!(scene.hit(Pt { x: 60.0, y: 60.0 }), Some(7));
    }

    #[test]
    fn click_in_concave_notch_misses() {
        let scene = Scene {
            nodes: vec![Node::Hotspot { path: l_path(), on_press: 7 }],
            canvas: (200, 200),
        };
        assert_eq!(scene.hit(Pt { x: 130.0, y: 130.0 }), None);
    }

    #[test]
    fn topmost_overlapping_hotspot_wins() {
        let square = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 100.0, y: 0.0 },
            Pt { x: 100.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let scene = Scene {
            nodes: vec![
                Node::Hotspot { path: square.clone(), on_press: 1 },
                Node::Hotspot { path: square, on_press: 2 },
            ],
            canvas: (200, 200),
        };
        assert_eq!(scene.hit(Pt { x: 50.0, y: 50.0 }), Some(2));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p proto --lib scene`
Expected: PASS (3 passed).

- [ ] **Step 4: Commit**

```bash
git add crates/proto/src/lib.rs crates/proto/src/scene.rs
git commit -m "feat(proto): scene node types + free-form hit resolution"
```

---

### Task 3: `lua_bridge` — sandbox, constructors, load, fire

> **Spike note:** `mlua` API specifics (e.g. `Chunk::set_environment`, `create_registry_value`, `registry_value`, `Function` cloning) may differ across versions. Target the resolved version; if a signature differs, align with its docs. The test gate (scene built correctly + sandbox blocks `io`/`os`) is the contract. Use `cargo add mlua --features lua54,vendored -p proto` — `vendored` builds Lua from source (needs a C compiler; present on macOS/Linux).

**Files:**
- Modify: `crates/proto/Cargo.toml` (add `mlua`)
- Create: `crates/proto/src/lua_bridge.rs`
- Modify: `crates/proto/src/lib.rs`

**Interfaces:**
- Consumes: `host::Host`, `scene::{Scene, Node, Pt, Color, HandlerId}`.
- Produces:
  - `pub type SharedHost = std::rc::Rc<std::cell::RefCell<Box<dyn Host>>>;`
  - `pub struct LoadedSkin { pub scene: Scene, /* private: lua, handlers, host */ }`
  - `pub fn load(lua_src: &str, canvas: (u32, u32), host: SharedHost) -> mlua::Result<LoadedSkin>`
  - `impl LoadedSkin { pub fn fire(&self, id: HandlerId) -> mlua::Result<()> }`

- [ ] **Step 1: Add mlua**

Run: `cargo add mlua --features lua54,vendored -p proto`
Expected: `mlua` under `[dependencies]` with features `["lua54", "vendored"]`.

- [ ] **Step 2: Declare the module**

Add to `crates/proto/src/lib.rs`:

```rust
pub mod lua_bridge;
```

- [ ] **Step 3: Write the failing tests**

Create `crates/proto/src/lua_bridge.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p proto --lib lua_bridge`
Expected: PASS (4 passed). First build compiles vendored Lua (slow once). If `set_environment`/registry calls fail to compile, reconcile against the resolved `mlua` version (see spike note).

- [ ] **Step 5: Commit**

```bash
git add crates/proto/Cargo.toml crates/proto/src/lib.rs crates/proto/src/lua_bridge.rs
git commit -m "feat(proto): sandboxed Lua bridge — constructors, host allowlist, handlers"
```

---

### Task 4: `skin` loader + the four skin directories

**Files:**
- Modify: `crates/proto/Cargo.toml` (add `serde`, `toml`)
- Create: `crates/proto/src/skin.rs`
- Modify: `crates/proto/src/lib.rs`
- Create: `crates/proto/skins/media-classic/skin.toml`, `…/skin.lua`
- Create: `crates/proto/skins/media-minimal/skin.toml`, `…/skin.lua`
- Create: `crates/proto/skins/sysmon-bars/skin.toml`, `…/skin.lua`
- Create: `crates/proto/skins/sysmon-dial/skin.toml`, `…/skin.lua`

**Interfaces:**
- Produces:
  - `pub struct Manifest { pub id: String, pub name: String, pub width: u32, pub height: u32, pub entry: String }`
  - `pub struct SkinFiles { pub manifest: Manifest, pub lua_src: String }`
  - `pub fn load_dir(dir: &std::path::Path) -> Result<SkinFiles, SkinError>`
  - `pub enum SkinError { Io(std::io::Error), Toml(toml::de::Error) }`

- [ ] **Step 1: Add deps**

Run: `cargo add serde --features derive -p proto` and `cargo add toml -p proto`.

- [ ] **Step 2: Declare the module**

Add to `crates/proto/src/lib.rs`:

```rust
pub mod skin;
```

- [ ] **Step 3: Write the failing test + the loader**

Create `crates/proto/src/skin.rs`:

```rust
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub entry: String,
}

#[derive(Debug)]
pub struct SkinFiles {
    pub manifest: Manifest,
    pub lua_src: String,
}

#[derive(Debug)]
pub enum SkinError {
    Io(std::io::Error),
    Toml(toml::de::Error),
}

impl From<std::io::Error> for SkinError {
    fn from(e: std::io::Error) -> Self {
        SkinError::Io(e)
    }
}
impl From<toml::de::Error> for SkinError {
    fn from(e: toml::de::Error) -> Self {
        SkinError::Toml(e)
    }
}

pub fn load_dir(dir: &Path) -> Result<SkinFiles, SkinError> {
    let manifest_src = std::fs::read_to_string(dir.join("skin.toml"))?;
    let manifest: Manifest = toml::from_str(&manifest_src)?;
    let lua_src = std::fs::read_to_string(dir.join(&manifest.entry))?;
    Ok(SkinFiles { manifest, lua_src })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_media_classic_skin_dir() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/media-classic");
        let skin = load_dir(&dir).unwrap();
        assert_eq!(skin.manifest.id, "media-classic");
        assert_eq!(skin.manifest.width, 300);
        assert!(skin.lua_src.contains("value_fill"));
    }
}
```

- [ ] **Step 4: Create the four skins**

`crates/proto/skins/media-classic/skin.toml`:

```toml
id = "media-classic"
name = "Media Classic"
width = 300
height = 120
entry = "skin.lua"
```

`crates/proto/skins/media-classic/skin.lua`:

```lua
-- background
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=20,g=30,b=60} }
-- play/pause hotspot (left square)
region{ path = {{x=20,y=20},{x=80,y=20},{x=80,y=80},{x=20,y=80}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=20,y=20},{x=80,y=20},{x=80,y=80},{x=20,y=80}}, color = {r=80,g=200,b=120} }
-- stop hotspot (second square)
region{ path = {{x=100,y=20},{x=160,y=20},{x=160,y=80},{x=100,y=80}},
        on_press = function() host.stop() end }
fill{ path = {{x=100,y=20},{x=160,y=20},{x=160,y=80},{x=100,y=80}}, color = {r=200,g=80,b=80} }
-- position progress bar (bound to host state)
value_fill{ path = {{x=20,y=95},{x=280,y=95},{x=280,y=110},{x=20,y=110}},
            value = "position", color = {r=240,g=240,b=80} }
```

`crates/proto/skins/media-minimal/skin.toml`:

```toml
id = "media-minimal"
name = "Media Minimal"
width = 300
height = 120
entry = "skin.lua"
```

`crates/proto/skins/media-minimal/skin.lua`:

```lua
-- minimalist: dark bg, one wide toggle bar, a thin progress line
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=10,g=10,b=10} }
region{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=70},{x=30,y=70}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=70},{x=30,y=70}}, color = {r=120,g=120,b=120} }
value_fill{ path = {{x=30,y=90},{x=270,y=90},{x=270,y=98},{x=30,y=98}},
            value = "position", color = {r=0,g=220,b=220} }
```

`crates/proto/skins/sysmon-bars/skin.toml`:

```toml
id = "sysmon-bars"
name = "Sysmon Bars"
width = 300
height = 120
entry = "skin.lua"
```

`crates/proto/skins/sysmon-bars/skin.lua`:

```lua
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=15,g=15,b=25} }
-- click anywhere on the big panel toggles sampling
region{ path = {{x=20,y=20},{x=280,y=20},{x=280,y=60},{x=20,y=60}},
        on_press = function() host.toggle_sampling() end }
fill{ path = {{x=20,y=20},{x=280,y=20},{x=280,y=60},{x=20,y=60}}, color = {r=40,g=60,b=90} }
-- cpu meter
value_fill{ path = {{x=20,y=75},{x=280,y=75},{x=280,y=105},{x=20,y=105}},
            value = "cpu", color = {r=120,g=240,b=120} }
```

`crates/proto/skins/sysmon-dial/skin.toml`:

```toml
id = "sysmon-dial"
name = "Sysmon Dial"
width = 300
height = 120
entry = "skin.lua"
```

`crates/proto/skins/sysmon-dial/skin.lua`:

```lua
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=25,g=15,b=15} }
-- L-shaped concave toggle hotspot, to stress concave hit-testing in the live app
region{ path = {{x=30,y=20},{x=150,y=20},{x=150,y=55},{x=85,y=55},{x=85,y=95},{x=30,y=95}},
        on_press = function() host.toggle_sampling() end }
fill{ path = {{x=30,y=20},{x=150,y=20},{x=150,y=55},{x=85,y=55},{x=85,y=95},{x=30,y=95}},
      color = {r=200,g=140,b=60} }
value_fill{ path = {{x=170,y=20},{x=280,y=20},{x=280,y=100},{x=170,y=100}},
            value = "cpu", color = {r=240,g=120,b=120} }
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p proto --lib skin`
Expected: PASS (1 passed).

- [ ] **Step 6: Commit**

```bash
git add crates/proto/Cargo.toml crates/proto/src/lib.rs crates/proto/src/skin.rs crates/proto/skins
git commit -m "feat(proto): skin.toml loader + four prototype skins"
```

---

### Task 5: `swap` — the Engine and the state-survives-swap proof

**Files:**
- Create: `crates/proto/src/swap.rs`
- Modify: `crates/proto/src/lib.rs`

**Interfaces:**
- Consumes: `host::{Host, StateValue}`, `lua_bridge::{self, LoadedSkin, SharedHost}`, `scene::{Scene, Pt, HandlerId}`.
- Produces:
  - `pub struct Engine { /* host: SharedHost, skin: LoadedSkin */ }`
  - `impl Engine { pub fn new(host: Box<dyn Host>, lua_src: &str, canvas: (u32,u32)) -> mlua::Result<Engine>; pub fn tick(&mut self, dt: f32); pub fn swap(&mut self, lua_src: &str, canvas: (u32,u32)) -> mlua::Result<()>; pub fn scene(&self) -> &Scene; pub fn state(&self, key: &str) -> Option<StateValue>; pub fn click(&self, p: Pt) -> mlua::Result<()> }`

- [ ] **Step 1: Declare the module**

Add to `crates/proto/src/lib.rs`:

```rust
pub mod swap;
```

- [ ] **Step 2: Write the failing test + the Engine**

Create `crates/proto/src/swap.rs`:

```rust
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
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p proto --lib swap`
Expected: PASS (2 passed). `0.0 + 3.0*0.1 = 0.3` after one toggle_play (exact in f32).

- [ ] **Step 4: Commit**

```bash
git add crates/proto/src/lib.rs crates/proto/src/swap.rs
git commit -m "feat(proto): swap Engine — scene rebuild with host state preserved"
```

---

### Task 6: `render` — vello multi-shape scene to an offscreen pixmap

> **Spike note:** This reuses the exact vello render-to-texture + 256-byte-aligned readback pattern from the known-good `crates/spike-render/src/vello_backend.rs`. Copy that file's structure; the only change is filling MANY paths (and one clipped value-bar) instead of one. Align with the resolved `vello`/`wgpu` versions as that file did.

**Files:**
- Modify: `crates/proto/Cargo.toml` (add `vello`, `wgpu`, `pollster`)
- Create: `crates/proto/src/render.rs`
- Modify: `crates/proto/src/lib.rs`

**Interfaces:**
- Consumes: `host::{Host, StateValue}`, `scene::{Scene, Node, Pt, Color}`.
- Produces:
  - `pub struct Pixmap { pub width: u32, pub height: u32, pub data: Vec<u8> }` (RGBA8 row-major)
  - `pub struct Renderer { /* device, queue, vello renderer */ }`
  - `impl Renderer { pub fn new() -> Self; pub fn render(&mut self, scene: &Scene, host: &dyn Host) -> Pixmap }`
  - Helper (private): `fn value_of(host: &dyn Host, key: &str) -> f32` — `Scalar(v) -> v`, `Bool(true) -> 1.0`, `Bool(false) -> 0.0`, `None -> 0.0`.

- [ ] **Step 1: Add deps**

Run: `cargo add vello wgpu pollster -p proto`.

- [ ] **Step 2: Declare the module**

Add to `crates/proto/src/lib.rs`:

```rust
pub mod render;
```

- [ ] **Step 3: Write the failing test**

Create `crates/proto/tests/render_smoke.rs`:

```rust
use proto::host::MediaHost;
use proto::render::Renderer;
use proto::scene::{Color, Node, Pt, Scene};

#[test]
fn renders_a_filled_rect_at_expected_pixel() {
    // A red 100x100 rect on a black-ish canvas; center pixel must be red.
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![Node::Fill {
            path: vec![
                Pt { x: 50.0, y: 50.0 },
                Pt { x: 150.0, y: 50.0 },
                Pt { x: 150.0, y: 150.0 },
                Pt { x: 50.0, y: 150.0 },
            ],
            color: Color { r: 255, g: 0, b: 0 },
        }],
    };
    let mut r = Renderer::new();
    let pm = r.render(&scene, &MediaHost::new());
    let i = ((100 * 200 + 100) * 4) as usize; // pixel (100,100)
    assert_eq!(&pm.data[i..i + 3], &[255, 0, 0], "center should be red");
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p proto --test render_smoke`
Expected: FAIL to compile — `render::Renderer` does not exist.

- [ ] **Step 5: Implement the renderer**

Create `crates/proto/src/render.rs`. Model the device/queue setup, texture creation, `render_to_texture`, and 256-byte-aligned readback **exactly** on `crates/spike-render/src/vello_backend.rs` (it is known to compile and pass). The scene-specific part:

```rust
use crate::host::{Host, StateValue};
use crate::scene::{Color, Node, Pt, Scene};
use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{Color as VColor, Fill};
use vello::{AaConfig, RenderParams, Scene as VScene};

pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
}

fn value_of(host: &dyn Host, key: &str) -> f32 {
    match host.get(key) {
        Some(StateValue::Scalar(v)) => v.clamp(0.0, 1.0),
        Some(StateValue::Bool(true)) => 1.0,
        _ => 0.0,
    }
}

fn bez(path: &[Pt]) -> BezPath {
    let mut bp = BezPath::new();
    if let Some((first, rest)) = path.split_first() {
        bp.move_to(KPoint::new(first.x as f64, first.y as f64));
        for p in rest {
            bp.line_to(KPoint::new(p.x as f64, p.y as f64));
        }
        bp.close_path();
    }
    bp
}

fn vcolor(c: Color) -> VColor {
    VColor::from_rgba8(c.r, c.g, c.b, 255)
}

fn bbox(path: &[Pt]) -> (f64, f64, f64, f64) {
    let xs = path.iter().map(|p| p.x as f64);
    let ys = path.iter().map(|p| p.y as f64);
    let x0 = xs.clone().fold(f64::INFINITY, f64::min);
    let x1 = xs.fold(f64::NEG_INFINITY, f64::max);
    let y0 = ys.clone().fold(f64::INFINITY, f64::min);
    let y1 = ys.fold(f64::NEG_INFINITY, f64::max);
    (x0, y0, x1, y1)
}

impl Renderer {
    pub fn new() -> Self {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("no wgpu adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("no device");
            let renderer = vello::Renderer::new(&device, vello::RendererOptions::default())
                .expect("vello renderer");
            Self { device, queue, renderer }
        })
    }

    pub fn render(&mut self, scene: &Scene, host: &dyn Host) -> Pixmap {
        let (w, h) = scene.canvas;
        let mut vs = VScene::new();
        for node in &scene.nodes {
            match node {
                Node::Fill { path, color } => {
                    vs.fill(Fill::NonZero, Affine::IDENTITY, vcolor(*color), None, &bez(path));
                }
                Node::Hotspot { .. } => {} // hotspots are invisible; skins add a Fill if they want it drawn
                Node::ValueFill { path, value_key, color } => {
                    let v = value_of(host, value_key) as f64;
                    let (x0, y0, x1, y1) = bbox(path);
                    let filled = Rect::new(x0, y0, x0 + (x1 - x0) * v, y1);
                    vs.fill(Fill::NonZero, Affine::IDENTITY, vcolor(*color), None, &filled);
                }
            }
        }

        // ---- render_to_texture + readback: copy from vello_backend.rs ----
        // Create an Rgba8Unorm texture (STORAGE_BINDING | COPY_SRC), call
        // self.renderer.render_to_texture(&device, &queue, &vs, &view, &RenderParams {
        //     base_color: VColor::from_rgba8(0,0,0,255), width: w, height: h,
        //     antialiasing_method: AaConfig::Area });
        // then copy_texture_to_buffer with 256-byte-aligned bytes_per_row, map, and
        // unpad into a tight RGBA8 Vec<u8>. Return Pixmap { width: w, height: h, data }.
        todo!("inline the readback block from crates/spike-render/src/vello_backend.rs")
    }
}
```

Replace the `todo!(...)` by pasting the texture-creation + `render_to_texture` + `copy_texture_to_buffer` + map + per-row unpad block verbatim from `crates/spike-render/src/vello_backend.rs::render` (it already returns exactly this `Pixmap` shape), using `vs` as the scene and `(w, h)` as the size, and `base_color` black. Do not invent a new readback path — reuse the proven one.

- [ ] **Step 6: Run the test**

Run: `cargo test -p proto --test render_smoke`
Expected: PASS. The interior pixel of an opaque red rect is exactly `[255,0,0]` (vello AA only affects edge pixels; (100,100) is well inside).

- [ ] **Step 7: Commit**

```bash
git add crates/proto/Cargo.toml crates/proto/src/lib.rs crates/proto/src/render.rs crates/proto/tests/render_smoke.rs
git commit -m "feat(proto): vello multi-shape scene renderer with value-bound bars"
```

---

### Task 7: `app` + `main` — the live windowed prototype

> **Spike note:** The window/event loop reuses the known-good `crates/spike-render/examples/viewer.rs` (winit 0.30 `ApplicationHandler` + softbuffer present, DPI-correct). Copy its structure; swap the single-region render for `render::Renderer` + the `swap::Engine`, and add `Tab`/`H` handling. GUI launch is verified by a human (no display in CI); the gate here is a clean `cargo build`.

**Files:**
- Modify: `crates/proto/Cargo.toml` (add `winit`, `softbuffer`)
- Create: `crates/proto/src/app.rs`
- Create: `crates/proto/src/main.rs`
- Modify: `crates/proto/src/lib.rs`

**Interfaces:**
- Consumes: `swap::Engine`, `render::{Renderer, Pixmap}`, `skin::load_dir`, `host::{MediaHost, SysmonHost, Host}`, `scene::Pt`.
- Produces: `pub fn run() -> ()` in `app.rs`; `main.rs` calls it. No library API beyond `run`.

- [ ] **Step 1: Add deps**

Run: `cargo add winit softbuffer -p proto`.

- [ ] **Step 2: Declare the module**

Add to `crates/proto/src/lib.rs`:

```rust
pub mod app;
```

- [ ] **Step 3: Implement the app**

Create `crates/proto/src/app.rs`. Reuse the winit+softbuffer skeleton from `crates/spike-render/examples/viewer.rs` (window creation, surface, physical-size DPI handling, RGBA8→`0x00RRGGBB` blit). Replace its render call and input handling with the logic below. The host roster and skin paths:

```rust
use std::path::PathBuf;

use crate::host::{Host, MediaHost, SysmonHost};
use crate::render::Renderer;
use crate::scene::Pt;
use crate::skin::load_dir;
use crate::swap::Engine;

// (host_index, skin_index) -> the two hosts, two skins each.
const SKIN_DIRS: [[&str; 2]; 2] = [
    ["skins/media-classic", "skins/media-minimal"],
    ["skins/sysmon-bars", "skins/sysmon-dial"],
];

fn skin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn make_host(host_index: usize) -> Box<dyn Host> {
    match host_index {
        0 => Box::new(MediaHost::new()),
        _ => Box::new(SysmonHost::new()),
    }
}

fn load_engine(host_index: usize, skin_index: usize) -> Engine {
    let files = load_dir(&skin_root().join(SKIN_DIRS[host_index][skin_index]))
        .expect("load skin dir");
    Engine::new(
        make_host(host_index),
        &files.lua_src,
        (files.manifest.width, files.manifest.height),
    )
    .expect("build engine")
}
```

App state holds: `host_index`, `skin_index`, `engine: Engine`, `renderer: Renderer`, `cursor: (f64,f64)`, the window + softbuffer surface (as in viewer.rs).

Behaviour to wire into the winit handlers:
- **Each frame / redraw:** `engine.tick(dt)` (use a fixed `dt = 1.0/60.0` for the prototype — simplest; no clock needed). Then `let pm = self.renderer.render(self.engine.scene(), /* host */ ...)`. Note `render` needs `&dyn Host`; expose it from `Engine` by adding a method `pub fn with_host<R>(&self, f: impl FnOnce(&dyn Host) -> R) -> R { f(&**self.host.borrow()) }` to `swap.rs`, OR render inside the engine. Simplest: add to `swap.rs`:

```rust
    pub fn render_with(&self, renderer: &mut crate::render::Renderer) -> crate::render::Pixmap {
        renderer.render(&self.skin.scene, &**self.host.borrow())
    }
```

Then the app calls `let pm = self.engine.render_with(&mut self.renderer);` and blits `pm` to the window (scale CANVAS→physical exactly as viewer.rs scales 200×200→physical). Use the engine's `scene().canvas` as the source size instead of a fixed 200.
- **Left-click (Pressed):** map physical cursor → canvas coords via the same ratio used for the blit, then `let _ = self.engine.click(Pt { x, y });`.
- **`Tab` (Pressed):** `self.skin_index ^= 1;` then `self.engine.swap(&new_lua, new_canvas)` using `load_dir` for `SKIN_DIRS[host_index][skin_index]` — **do not** rebuild the host; swap preserves state. Print `"[{id}] swapped, state preserved"`.
- **`H` (Pressed):** `self.host_index ^= 1; self.skin_index = 0; self.engine = load_engine(self.host_index, 0);` (fresh host). Print the switch.
- **Esc / CloseRequested:** exit.

Add the `render_with` method to `swap.rs` (Step 3 of this task modifies `swap.rs` too — include it in this task's commit).

- [ ] **Step 4: Create main.rs**

Create `crates/proto/src/main.rs`:

```rust
fn main() {
    proto::app::run();
}
```

- [ ] **Step 5: Build (the gate)**

Run: `cargo build -p proto`
Expected: compiles cleanly (lib + `proto` bin). If winit/softbuffer signatures differ, reconcile against `viewer.rs` and the resolved versions.

- [ ] **Step 6: Smoke-launch (best effort, human-verified)**

If a display is available: `cargo run -p proto`. Confirm a window shows the media-classic skin; clicking the green square toggles play (the yellow progress bar starts advancing); clicking it again pauses; `Tab` swaps to media-minimal with the progress **continuing** from where it was; `H` switches to the sysmon skins (cpu meter animating); the concave L-shaped hotspot in sysmon-dial only toggles when you click the filled arms, not the notch. If headless, note that interactive launch was not verified and rely on the clean build; the human will run it.

- [ ] **Step 7: Commit**

```bash
git add crates/proto/Cargo.toml crates/proto/src/lib.rs crates/proto/src/app.rs crates/proto/src/main.rs crates/proto/src/swap.rs
git commit -m "feat(proto): live windowed prototype — render, click, Tab swap, H host-switch"
```

---

## Self-Review

**Spec coverage (against the Phase 1 design):**
- Real `mlua`, sandboxed → Task 3 (`load`, `set_environment`, allowlist) + negative tests. ✓
- Real `skin.toml` + on-disk skins, swappable → Tasks 4, 7. ✓
- No text; shapes/colors + proportional `value_fill` bar → Task 6 (`bbox`-clipped fill). ✓
- Two hosts, two skins each → Task 1 (hosts) + Task 4 (4 skins) + Task 7 (`SKIN_DIRS`, `H`/`Tab`). ✓
- Three primitives (`fill`/`region`/`value_fill`) → Task 3 constructors. ✓
- Update model (lua runs once; nodes hold binding keys; engine reads host each frame) → Tasks 3, 5, 6. ✓
- Free-form hit-testing via `hittest`, concave notch misses → Task 2 + the L-shaped hotspot in sysmon-dial (Task 4) + Task 7 click mapping. ✓
- State-survives-swap, headless proof → Task 5 (`state_survives_swap`); live proof → Task 7. ✓
- Engine carries zero domain knowledge → Global Constraint; domain names confined to `host.rs` + skin files. ✓
- Lessons-learned note → produced after the prototype runs (design doc names it; not a code task — author it as the Phase 1 output before Phase 2).

**Placeholder scan:** Task 6 deliberately points at `vello_backend.rs` for the readback block rather than re-printing ~60 lines of GPU boilerplate verbatim; the block is fully specified by reference to known-good committed code, and the `todo!()` is explicitly flagged to be replaced in Step 5. Task 7 reuses `viewer.rs` similarly. These are reuse-of-verified-code directives, not unwritten logic. All testable logic (host, scene, lua_bridge, skin, swap, render scene-building) has complete code.

**Type consistency:** `Pt`/`Color`/`Node`/`Scene`/`HandlerId` (Task 2) are used unchanged in Tasks 3, 5, 6. `SharedHost = Rc<RefCell<Box<dyn Host>>>`, `LoadedSkin { scene, … }`, `load(...) -> mlua::Result<LoadedSkin>`, `fire(id)` (Task 3) match their uses in Task 5. `Engine::{new,tick,swap,scene,state,click}` + `render_with` (Tasks 5, 7) and `Renderer::{new,render}` + `Pixmap{width,height,data}` (Task 6) are consistent across the app. `Host::{name,tick,get,actions,invoke}` + `StateValue` (Task 1) are used unchanged everywhere.

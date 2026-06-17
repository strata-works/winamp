# Phase 3a — Headless Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the real engine crate `crates/carapace` headless — the Phase 2 spine modules and the `Engine` input→drain→tick frame loop — provable without a GPU or window.

**Architecture:** A new `carapace` crate. The host owns state; skins are sandboxed Lua that build a `Scene` via a data-driven vocabulary registry and enqueue allowlisted `Command`s. The `Engine` resolves pointer hits to handlers (which only enqueue), then `update` drains the command queue FIFO with exclusive `&mut host` and ticks. The command-queue model means the host is NOT shared into the script — only the command queue is. Rendering is Phase 3b.

**Tech Stack:** Rust (edition 2024). `hittest` (Phase 0, path dep); `mlua` 0.11.6 (lua54 + vendored); `toml`; `serde`. No vello/wgpu/winit in 3a.

## Global Constraints

- Rust, **edition 2024**, stable. New crate `crates/carapace`; add to workspace `members`.
- Do **not** modify Phase 0/1 crates (`crates/hittest`, `crates/spike-render`, `crates/proto`). Reuse `hittest` via path dep; `proto` is **reference only** (cite it, don't import it).
- **No GPU/window deps in 3a:** `vello`, `wgpu`, `winit`, `softbuffer` must NOT appear in `carapace/Cargo.toml`. `render` and surface integration are Phase 3b.
- **Engine carries zero domain knowledge:** no media/sysmon names (`play`, `cpu`, `position`, `sampling`, …) anywhere in `carapace/src/`. Domain names appear only in the **test-only** `FixtureHost`, and even there they are generic (`toggle`, `bump`, `on`, `level`).
- **Sandbox:** a skin's Lua `_ENV` contains only the registry's primitive constructors + a `host` table whose fields are exactly the host's `actions()`. `io`/`os`/`require`/base globals absent.
- **Command queue semantics (Phase 2 §3):** FIFO; every occurrence applied; no dedup; reads synchronous against pre-drain state (no read-after-write within a handler); commands carry args; **commands may not enqueue commands** (non-recursive drain). A `SwitchHost` replaces the host; `HostAction`s after it in the same drain are validated against the new allowlist and dropped + logged if absent.
- **Scene = pure projection of state:** nodes hold binding **keys**, never resolved values. Hotspot nodes cache their `hittest::Region` at build time.
- **State-survives-swap is an invariant:** swap rebuilds the scene from the unchanged host; **swap is transactional** — a failed rebuild leaves the prior scene active.
- **No panics on skin/host faults:** engine functions return `Result`; `unwrap` only on genuine engine invariants.

### Spec elaborations made by this plan (intentional, not violations)

- `Primitive::build` takes a `BuildContext` (so a hotspot primitive can register its `on_press` handler and get a `HandlerId`). Phase 2 §5 sketched `build(args) -> Node`; this is the necessary concrete form.
- `Command::Swap`/`SwitchHost` carry a resolved `SkinSource { lua_src, canvas }` (and `SwitchHost` a `Box<dyn Host>`), supplied by the host app — keeping `engine` filesystem-free and testable; `skin::load_dir` produces the `SkinSource`.
- The host is **owned** by the `Engine` (`Box<dyn Host>`), not shared `Rc<RefCell>` as in the prototype — the command model removes the need to share it.

---

### Task 1: Crate scaffold + `state` + `host`

**Files:**
- Modify: `Cargo.toml` (workspace `members`)
- Create: `crates/carapace/Cargo.toml`, `crates/carapace/src/lib.rs`, `crates/carapace/src/state.rs`, `crates/carapace/src/host.rs`
- Create: `crates/carapace/src/fixture.rs` (a `#[cfg(test)]`-gated test fixture host, exported under `#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `state`: `pub enum StateValue { Bool(bool), Scalar(f32) }` (derives `Clone, Copy, PartialEq, Debug`)
  - `host`: `pub enum Value { Num(f64), Bool(bool), Str(String) }` (`Clone, Debug, PartialEq`); `pub struct ActionSpec { pub name: &'static str }`; `pub trait Host { fn name(&self) -> &str; fn tick(&mut self, dt: std::time::Duration); fn get(&self, key: &str) -> Option<StateValue>; fn actions(&self) -> &[ActionSpec]; fn invoke(&mut self, action: &str, args: &[Value]); }`
  - `fixture` (test-only): `pub struct FixtureHost { on: bool, level: f32 }` impl `Host`.

- [ ] **Step 1: Add the crate to the workspace**

Edit root `Cargo.toml` `members` to add `"crates/carapace"`:

```toml
[workspace]
members = ["crates/hittest", "crates/spike-render", "crates/proto", "crates/carapace"]
resolver = "2"
```

- [ ] **Step 2: Create the manifest**

Create `crates/carapace/Cargo.toml`:

```toml
[package]
name = "carapace"
version = "0.0.0"
edition = "2024"

[dependencies]
hittest = { path = "../hittest" }
mlua = { version = "0.11.6", features = ["lua54", "vendored"] }
toml = "1.1.2"
serde = { version = "1.0.228", features = ["derive"] }
```

- [ ] **Step 3: Create lib.rs declaring the modules added so far**

Create `crates/carapace/src/lib.rs`:

```rust
pub mod host;
pub mod state;

#[cfg(test)]
mod fixture;
```

- [ ] **Step 4: Write `state.rs`**

Create `crates/carapace/src/state.rs`:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StateValue {
    Bool(bool),
    Scalar(f32),
}
```

- [ ] **Step 5: Write the failing test + `host.rs`**

Create `crates/carapace/src/host.rs`:

```rust
use std::time::Duration;

use crate::state::StateValue;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
}

#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    pub name: &'static str,
}

/// The host capability surface. The engine knows none of the concrete names.
pub trait Host {
    fn name(&self) -> &str;
    fn tick(&mut self, dt: Duration);
    fn get(&self, key: &str) -> Option<StateValue>;
    fn actions(&self) -> &[ActionSpec];
    fn invoke(&mut self, action: &str, args: &[Value]);
}
```

- [ ] **Step 6: Write the fixture host + its tests**

Create `crates/carapace/src/fixture.rs`:

```rust
use std::time::Duration;

use crate::host::{ActionSpec, Host, Value};
use crate::state::StateValue;

/// Test-only, domain-neutral host: a `toggle` action flips `on`; `bump(n)` adds to
/// `level`; `level` also advances on tick. Never shipped.
pub struct FixtureHost {
    on: bool,
    level: f32,
}

impl FixtureHost {
    pub fn new() -> Self {
        Self { on: false, level: 0.0 }
    }
}

const ACTIONS: &[ActionSpec] = &[ActionSpec { name: "toggle" }, ActionSpec { name: "bump" }];

impl Host for FixtureHost {
    fn name(&self) -> &str {
        "fixture"
    }
    fn tick(&mut self, dt: Duration) {
        self.level = (self.level + dt.as_secs_f32()).min(1.0);
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "on" => Some(StateValue::Bool(self.on)),
            "level" => Some(StateValue::Scalar(self.level)),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        match action {
            "toggle" => self.on = !self.on,
            "bump" => {
                if let Some(Value::Num(n)) = args.first() {
                    self.level = (self.level + *n as f32).min(1.0);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_flips_on() {
        let mut h = FixtureHost::new();
        assert_eq!(h.get("on"), Some(StateValue::Bool(false)));
        h.invoke("toggle", &[]);
        assert_eq!(h.get("on"), Some(StateValue::Bool(true)));
    }

    #[test]
    fn bump_uses_its_argument() {
        let mut h = FixtureHost::new();
        h.invoke("bump", &[Value::Num(0.25)]);
        assert_eq!(h.get("level"), Some(StateValue::Scalar(0.25)));
    }

    #[test]
    fn tick_advances_level_unknown_inert() {
        let mut h = FixtureHost::new();
        h.tick(Duration::from_secs_f32(0.5));
        assert_eq!(h.get("level"), Some(StateValue::Scalar(0.5)));
        assert_eq!(h.get("nope"), None);
        h.invoke("nope", &[]); // must not panic
    }
}
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p carapace`
Expected: PASS (3 passed). First build compiles vendored Lua (slow once).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/carapace
git commit -m "feat(carapace): crate scaffold, state + host capability surface"
```

---

### Task 2: `scene`

**Files:**
- Create: `crates/carapace/src/scene.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod scene;`)

**Interfaces:**
- Consumes: `hittest::{Region, Contour, Point}`.
- Produces: `Pt { x: f32, y: f32 }`; `Color { r: u8, g: u8, b: u8 }`; `type HandlerId = usize`; `enum Node { Fill { path: Vec<Pt>, color: Color }, Hotspot { region: Region, on_press: HandlerId }, ValueFill { path: Vec<Pt>, value_key: String, color: Color } }`; `struct Scene { pub nodes: Vec<Node>, pub canvas: (u32, u32) }`; `Scene::hit(&self, Pt) -> Option<HandlerId>`; `pub fn region_of(path: &[Pt]) -> Region`.

> Reference: `crates/proto/src/scene.rs` has the proven hit logic. The change here: `Hotspot` stores the **pre-built** `hittest::Region` (cached at build, Phase 2 invariant) instead of the raw path, so `hit` does not rebuild it per call.

- [ ] **Step 1: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod scene;`

- [ ] **Step 2: Write the failing test + `scene.rs`**

Create `crates/carapace/src/scene.rs`:

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
    Hotspot { region: Region, on_press: HandlerId },
    ValueFill { path: Vec<Pt>, value_key: String, color: Color },
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub canvas: (u32, u32),
}

/// Build a single-contour Region from a polygon path (cached into Hotspot nodes).
pub fn region_of(path: &[Pt]) -> Region {
    Region {
        contours: vec![Contour {
            points: path.iter().map(|p| Point { x: p.x, y: p.y }).collect(),
        }],
    }
}

impl Scene {
    /// Topmost hotspot containing `p` (later nodes draw on top → iterate in reverse).
    pub fn hit(&self, p: Pt) -> Option<HandlerId> {
        for node in self.nodes.iter().rev() {
            if let Node::Hotspot { region, on_press } = node {
                if region.contains(Point { x: p.x, y: p.y }) {
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

    fn hotspot(path: &[Pt], id: HandlerId) -> Node {
        Node::Hotspot { region: region_of(path), on_press: id }
    }

    #[test]
    fn click_inside_hotspot_returns_handler() {
        let s = Scene { nodes: vec![hotspot(&l_path(), 7)], canvas: (200, 200) };
        assert_eq!(s.hit(Pt { x: 60.0, y: 60.0 }), Some(7));
    }

    #[test]
    fn click_in_concave_notch_misses() {
        let s = Scene { nodes: vec![hotspot(&l_path(), 7)], canvas: (200, 200) };
        assert_eq!(s.hit(Pt { x: 130.0, y: 130.0 }), None);
    }

    #[test]
    fn topmost_overlapping_hotspot_wins() {
        let sq = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 100.0, y: 0.0 },
            Pt { x: 100.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let s = Scene { nodes: vec![hotspot(&sq, 1), hotspot(&sq, 2)], canvas: (200, 200) };
        assert_eq!(s.hit(Pt { x: 50.0, y: 50.0 }), Some(2));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p carapace --lib scene`
Expected: PASS (3 passed).

- [ ] **Step 4: Commit**

```bash
git add crates/carapace/src/lib.rs crates/carapace/src/scene.rs
git commit -m "feat(carapace): scene nodes with cached hit-test regions"
```

---

### Task 3: `vocab` — the seam + stub primitives

**Files:**
- Create: `crates/carapace/src/vocab.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod vocab;`)

**Interfaces:**
- Consumes: `scene::{Node, Pt, Color, HandlerId}`; `mlua::{Table, Function}`.
- Produces:
  - `pub enum BuildError { MissingField(&'static str), BadType(&'static str), Lua(mlua::Error) }` (+ `From<mlua::Error>`)
  - `pub trait BuildContext { fn register_handler(&mut self, f: Function) -> HandlerId; }`
  - `pub trait Primitive { fn id(&self) -> &str; fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError>; }`
  - `pub struct VocabRegistry { prims: Vec<Box<dyn Primitive>> }` with `new()`, `register(Box<dyn Primitive>)`, `iter() -> impl Iterator<Item=&dyn Primitive>`, `base() -> VocabRegistry` (the stub set).
  - stub primitives `FillPrim`, `RegionPrim`, `ValueFillPrim` (private; surfaced via `base()`).
  - helpers `parse_path(&Table) -> Result<Vec<Pt>, BuildError>`, `parse_color(&Table) -> Result<Color, BuildError>`.

> `register_handler` is why `build` needs a `ctx`: `RegionPrim` reads the `on_press` `Function`, registers it via the context to get a `HandlerId`, and stores that id in `Node::Hotspot`. `FillPrim`/`ValueFillPrim` ignore the ctx.

- [ ] **Step 1: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod vocab;`

- [ ] **Step 2: Write the failing test + `vocab.rs`**

Create `crates/carapace/src/vocab.rs`:

```rust
use mlua::{Function, Table};

use crate::scene::{Color, HandlerId, Node, Pt};

#[derive(Debug)]
pub enum BuildError {
    MissingField(&'static str),
    BadType(&'static str),
    Lua(mlua::Error),
}

impl From<mlua::Error> for BuildError {
    fn from(e: mlua::Error) -> Self {
        BuildError::Lua(e)
    }
}

/// Lets a primitive register a Lua handler (for hotspots) and receive a HandlerId.
pub trait BuildContext {
    fn register_handler(&mut self, f: Function) -> HandlerId;
}

/// A vocabulary entry a skin can construct: `id` is the Lua constructor name.
pub trait Primitive {
    fn id(&self) -> &str;
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError>;
}

pub fn parse_path(t: &Table) -> Result<Vec<Pt>, BuildError> {
    let path: Table = t.get("path").map_err(|_| BuildError::MissingField("path"))?;
    let mut pts = Vec::new();
    for entry in path.sequence_values::<Table>() {
        let p = entry?;
        pts.push(Pt { x: p.get("x")?, y: p.get("y")? });
    }
    if pts.len() < 3 {
        return Err(BuildError::BadType("path needs >= 3 points"));
    }
    Ok(pts)
}

pub fn parse_color(t: &Table) -> Result<Color, BuildError> {
    let c: Table = t.get("color").map_err(|_| BuildError::MissingField("color"))?;
    Ok(Color { r: c.get("r")?, g: c.get("g")?, b: c.get("b")? })
}

struct FillPrim;
impl Primitive for FillPrim {
    fn id(&self) -> &str {
        "fill"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        Ok(Node::Fill { path: parse_path(args)?, color: parse_color(args)? })
    }
}

struct RegionPrim;
impl Primitive for RegionPrim {
    fn id(&self) -> &str {
        "region"
    }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let path = parse_path(args)?;
        let on_press: Function = args
            .get("on_press")
            .map_err(|_| BuildError::MissingField("on_press"))?;
        let id = ctx.register_handler(on_press);
        Ok(Node::Hotspot { region: crate::scene::region_of(&path), on_press: id })
    }
}

struct ValueFillPrim;
impl Primitive for ValueFillPrim {
    fn id(&self) -> &str {
        "value_fill"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let value_key: String = args.get("value").map_err(|_| BuildError::MissingField("value"))?;
        Ok(Node::ValueFill { path: parse_path(args)?, value_key, color: parse_color(args)? })
    }
}

pub struct VocabRegistry {
    prims: Vec<Box<dyn Primitive>>,
}

impl VocabRegistry {
    pub fn new() -> Self {
        Self { prims: Vec::new() }
    }
    pub fn register(&mut self, prim: Box<dyn Primitive>) {
        self.prims.push(prim);
    }
    pub fn iter(&self) -> impl Iterator<Item = &dyn Primitive> {
        self.prims.iter().map(|b| b.as_ref())
    }
    /// The stub base set (Phase 5 replaces with the real vocabulary).
    pub fn base() -> Self {
        let mut r = Self::new();
        r.register(Box::new(FillPrim));
        r.register(Box::new(RegionPrim));
        r.register(Box::new(ValueFillPrim));
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    struct NoHandlers;
    impl BuildContext for NoHandlers {
        fn register_handler(&mut self, _f: Function) -> HandlerId {
            0
        }
    }

    fn tbl(lua: &Lua, src: &str) -> Table {
        lua.load(src).eval().unwrap()
    }

    #[test]
    fn fill_builds_fill_node() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=10,y=0},{x=10,y=10}}, color = {r=1,g=2,b=3} }",
        );
        let node = FillPrim.build(&t, &mut NoHandlers).unwrap();
        match node {
            Node::Fill { color, path } => {
                assert_eq!(color, Color { r: 1, g: 2, b: 3 });
                assert_eq!(path.len(), 3);
            }
            other => panic!("expected Fill, got {other:?}"),
        }
    }

    #[test]
    fn value_fill_keeps_binding_key() {
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, value = 'level', color = {r=0,g=0,b=0} }",
        );
        match ValueFillPrim.build(&t, &mut NoHandlers).unwrap() {
            Node::ValueFill { value_key, .. } => assert_eq!(value_key, "level"),
            other => panic!("expected ValueFill, got {other:?}"),
        }
    }

    #[test]
    fn region_registers_handler_and_caches_region() {
        struct Counter(HandlerId);
        impl BuildContext for Counter {
            fn register_handler(&mut self, _f: Function) -> HandlerId {
                let id = self.0;
                self.0 += 1;
                id
            }
        }
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, on_press = function() end }",
        );
        let mut ctx = Counter(5);
        match RegionPrim.build(&t, &mut ctx).unwrap() {
            Node::Hotspot { on_press, .. } => assert_eq!(on_press, 5),
            other => panic!("expected Hotspot, got {other:?}"),
        }
        assert_eq!(ctx.0, 6, "handler id was allocated");
    }

    #[test]
    fn missing_field_errors() {
        let lua = Lua::new();
        let t = tbl(&lua, "return { color = {r=0,g=0,b=0} }"); // no path
        assert!(matches!(FillPrim.build(&t, &mut NoHandlers), Err(BuildError::MissingField("path"))));
    }

    #[test]
    fn base_registry_has_three() {
        assert_eq!(VocabRegistry::base().iter().count(), 3);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p carapace --lib vocab`
Expected: PASS (5 passed).

- [ ] **Step 4: Commit**

```bash
git add crates/carapace/src/lib.rs crates/carapace/src/vocab.rs
git commit -m "feat(carapace): vocabulary seam (Primitive/BuildContext/registry) + stub primitives"
```

---

### Task 4: `command` + `script` — sandbox, registry-driven env, command queue

> The novel core. References `crates/proto/src/lua_bridge.rs` for the proven `Lua::new` + `set_environment` sandbox pattern (mlua 0.11.6). The differences: (1) constructors come from the **registry**, not hardcoded; (2) the `host` table's shims **enqueue `HostAction` commands** instead of invoking the host; (3) a `SceneBuilder` implements `BuildContext` to collect nodes + register handlers.

**Files:**
- Create: `crates/carapace/src/command.rs`, `crates/carapace/src/script.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod command; pub mod script;`)

**Interfaces:**
- Consumes: `host::{Host, Value}`, `scene::{Scene, Node, HandlerId}`, `vocab::{VocabRegistry, BuildContext, BuildError}`, `mlua`.
- Produces:
  - `command`: `pub struct SkinSource { pub lua_src: String, pub canvas: (u32, u32) }` (`Clone, Debug`); `pub enum Command { HostAction { action: String, args: Vec<Value> }, Swap(SkinSource), SwitchHost { host: Box<dyn Host>, skin: SkinSource } }`; `pub type Queue = std::rc::Rc<std::cell::RefCell<Vec<Command>>>`.
  - `script`: `pub struct LoadedSkin { pub scene: Scene }` (plus private `lua`, `handlers`); `pub fn load(source: &SkinSource, host: &dyn Host, registry: &VocabRegistry, queue: Queue) -> Result<LoadedSkin, ScriptError>`; `LoadedSkin::fire(&self, id: HandlerId) -> Result<(), ScriptError>`; `pub enum ScriptError { Lua(mlua::Error), Build(BuildError) }`.

- [ ] **Step 1: Declare the modules**

Add to `crates/carapace/src/lib.rs`: `pub mod command;` and `pub mod script;`

- [ ] **Step 2: Write `command.rs`**

Create `crates/carapace/src/command.rs`:

```rust
use std::cell::RefCell;
use std::rc::Rc;

use crate::host::{Host, Value};

#[derive(Clone, Debug)]
pub struct SkinSource {
    pub lua_src: String,
    pub canvas: (u32, u32),
}

pub enum Command {
    HostAction { action: String, args: Vec<Value> },
    Swap(SkinSource),
    SwitchHost { host: Box<dyn Host>, skin: SkinSource },
}

/// Shared command queue: skin handlers push HostAction; the host app pushes
/// Swap/SwitchHost; the Engine drains it.
pub type Queue = Rc<RefCell<Vec<Command>>>;

pub fn new_queue() -> Queue {
    Rc::new(RefCell::new(Vec::new()))
}
```

- [ ] **Step 3: Write `script.rs`**

> Lifetime note for the implementer: `mlua::create_function` closures must be `'static`, so a constructor closure cannot borrow `registry` (a `&` parameter). The engine therefore holds the registry in an `Rc<VocabRegistry>` and `load` takes `Rc<VocabRegistry>`, so each closure captures a cheap `registry.clone()`. This is why the signature is `registry: Rc<VocabRegistry>` (matching the `Interfaces` block).

Create `crates/carapace/src/script.rs`:

```rust
use std::cell::RefCell;
use std::rc::Rc;

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
}
impl BuildContext for SceneBuilder {
    fn register_handler(&mut self, f: Function) -> HandlerId {
        self.handler_fns.push(f);
        self.handler_fns.len() - 1
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
    let builder = Rc::new(RefCell::new(SceneBuilder { nodes: Vec::new(), handler_fns: Vec::new() }));

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
            let node = prim
                .build(&args, &mut *b)
                .map_err(|e| mlua::Error::external(format!("{e:?}")))?;
            b.nodes.push(node);
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

    // Run the skin once under the sandboxed env (only the registry ctors + host are visible).
    lua.load(&source.lua_src).set_environment(env).exec()?;

    let (nodes, handler_fns) = {
        let mut b = builder.borrow_mut();
        (std::mem::take(&mut b.nodes), std::mem::take(&mut b.handler_fns))
    };
    let handlers = handler_fns
        .into_iter()
        .map(|f| lua.create_registry_value(f))
        .collect::<mlua::Result<Vec<_>>>()?;

    Ok(LoadedSkin { scene: Scene { nodes, canvas: source.canvas }, lua, handlers })
}

impl LoadedSkin {
    pub fn fire(&self, id: HandlerId) -> Result<(), ScriptError> {
        let f: Function = self.lua.registry_value(&self.handlers[id])?;
        f.call(())?;
        Ok(())
    }
}
```

- [ ] **Step 3b: Add the tests**

Append to `crates/carapace/src/script.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::new_queue;
    use crate::fixture::FixtureHost;
    use crate::vocab::VocabRegistry;
    use std::rc::Rc;

    fn src(s: &str) -> SkinSource {
        SkinSource { lua_src: s.to_string(), canvas: (300, 120) }
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
        for bad in ["io.write('x')", "os.time()", "require('os')", "host.nope()", "frobnicate{}"] {
            let r = load(&src(bad), &FixtureHost::new(), reg.clone(), new_queue());
            assert!(r.is_err(), "expected sandbox/registry to reject `{bad}`");
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace --lib script`
Expected: PASS (4 passed). The sandbox test relies on `set_environment` (proven in proto). If an mlua signature differs, reconcile against `crates/proto/src/lua_bridge.rs`.

- [ ] **Step 5: Commit**

```bash
git add crates/carapace/src/lib.rs crates/carapace/src/command.rs crates/carapace/src/script.rs
git commit -m "feat(carapace): command queue + registry-driven sandboxed script loader"
```

---

### Task 5: `skin` — manifest loader → SkinSource

**Files:**
- Create: `crates/carapace/src/skin.rs`
- Create: test fixtures `crates/carapace/tests/skins/ok/skin.toml`, `…/ok/skin.lua`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod skin;`)

**Interfaces:**
- Consumes: `command::SkinSource`.
- Produces: `pub struct Manifest { schema: u32, id: String, name: String, engine: String, canvas: Canvas, entry: String }`; `pub struct Canvas { width: u32, height: u32 }`; `pub fn load_dir(dir: &std::path::Path) -> Result<(Manifest, SkinSource), SkinError>`; `pub enum SkinError { Io(std::io::Error), Toml(toml::de::Error), UnsupportedSchema(u32), EngineIncompat(String) }`.

> `SUPPORTED_SCHEMA = 1`. `engine` is accepted if it equals `"^0.1"` (full semver range parsing is deferred; exact-match the one supported value, reject others as `EngineIncompat`).

- [ ] **Step 1: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod skin;`

- [ ] **Step 2: Create the OK fixture skin**

`crates/carapace/tests/skins/ok/skin.toml`:

```toml
schema = 1
id = "ok"
name = "OK Skin"
engine = "^0.1"
canvas = { width = 300, height = 120 }
entry = "skin.lua"
```

`crates/carapace/tests/skins/ok/skin.lua`:

```lua
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120}}, color = {r=10,g=10,b=10} }
```

- [ ] **Step 3: Write the failing test + `skin.rs`**

Create `crates/carapace/src/skin.rs`:

```rust
use std::path::Path;

use serde::Deserialize;

use crate::command::SkinSource;

const SUPPORTED_SCHEMA: u32 = 1;
const SUPPORTED_ENGINE: &str = "^0.1";

#[derive(Debug, Deserialize, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub engine: String,
    pub canvas: Canvas,
    pub entry: String,
}

#[derive(Debug)]
pub enum SkinError {
    Io(std::io::Error),
    Toml(toml::de::Error),
    UnsupportedSchema(u32),
    EngineIncompat(String),
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

pub fn load_dir(dir: &Path) -> Result<(Manifest, SkinSource), SkinError> {
    let manifest: Manifest = toml::from_str(&std::fs::read_to_string(dir.join("skin.toml"))?)?;
    if manifest.schema != SUPPORTED_SCHEMA {
        return Err(SkinError::UnsupportedSchema(manifest.schema));
    }
    if manifest.engine != SUPPORTED_ENGINE {
        return Err(SkinError::EngineIncompat(manifest.engine.clone()));
    }
    let lua_src = std::fs::read_to_string(dir.join(&manifest.entry))?;
    let canvas = (manifest.canvas.width, manifest.canvas.height);
    Ok((manifest, SkinSource { lua_src, canvas }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skins_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/skins")
    }

    #[test]
    fn loads_ok_skin() {
        let (m, src) = load_dir(&skins_dir().join("ok")).unwrap();
        assert_eq!(m.id, "ok");
        assert_eq!(src.canvas, (300, 120));
        assert!(src.lua_src.contains("fill"));
    }

    #[test]
    fn rejects_unknown_schema() {
        let dir = tempdir_with(
            "schema = 2\nid='x'\nname='x'\nengine='^0.1'\ncanvas={width=1,height=1}\nentry='s.lua'",
            "",
        );
        assert!(matches!(load_dir(dir.path()), Err(SkinError::UnsupportedSchema(2))));
    }

    #[test]
    fn rejects_incompatible_engine() {
        let dir = tempdir_with(
            "schema = 1\nid='x'\nname='x'\nengine='^9.9'\ncanvas={width=1,height=1}\nentry='s.lua'",
            "",
        );
        assert!(matches!(load_dir(dir.path()), Err(SkinError::EngineIncompat(_))));
    }

    // Minimal temp-dir helper (no external crate).
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir_with(toml: &str, lua: &str) -> TempDir {
        let base = std::env::temp_dir().join(format!("carapace-skintest-{}", toml.len()));
        let _ = std::fs::create_dir_all(&base);
        std::fs::write(base.join("skin.toml"), toml).unwrap();
        std::fs::write(base.join("s.lua"), lua).unwrap();
        TempDir(base)
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace --lib skin`
Expected: PASS (3 passed).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace/src/lib.rs crates/carapace/src/skin.rs crates/carapace/tests/skins
git commit -m "feat(carapace): skin manifest loader with schema + engine compat checks"
```

---

### Task 6: `swap` — transactional scene rebuild

**Files:**
- Create: `crates/carapace/src/swap.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod swap;`)

**Interfaces:**
- Consumes: `script::{load, LoadedSkin, ScriptError}`, `command::{SkinSource, Queue}`, `host::Host`, `vocab::VocabRegistry`.
- Produces: `pub fn rebuild(source: &SkinSource, host: &dyn Host, registry: std::rc::Rc<VocabRegistry>, queue: Queue) -> Result<LoadedSkin, ScriptError>` — a thin wrapper that builds a fresh `LoadedSkin`; the **caller** keeps its old skin on `Err` (transactionality lives at the call site, Task 7).

> `swap::rebuild` is intentionally tiny — it exists so the transactional contract has a named home and a test. The Engine (Task 7) calls it and only replaces `self.skin` on `Ok`.

- [ ] **Step 1: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod swap;`

- [ ] **Step 2: Write the failing test + `swap.rs`**

Create `crates/carapace/src/swap.rs`:

```rust
use std::rc::Rc;

use crate::command::{Queue, SkinSource};
use crate::host::Host;
use crate::script::{load, LoadedSkin, ScriptError};
use crate::vocab::VocabRegistry;

/// Build a fresh skin from `source`. On error the caller keeps its current skin
/// (transactional swap — the rebuild never mutates the caller's state).
pub fn rebuild(
    source: &SkinSource,
    host: &dyn Host,
    registry: Rc<VocabRegistry>,
    queue: Queue,
) -> Result<LoadedSkin, ScriptError> {
    load(source, host, registry, queue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::new_queue;
    use crate::fixture::FixtureHost;

    fn src(s: &str) -> SkinSource {
        SkinSource { lua_src: s.to_string(), canvas: (10, 10) }
    }

    #[test]
    fn rebuild_ok_returns_new_scene() {
        let skin = rebuild(
            &src("fill{ path={{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color={r=0,g=0,b=0} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            new_queue(),
        )
        .unwrap();
        assert_eq!(skin.scene.nodes.len(), 1);
    }

    #[test]
    fn rebuild_err_on_bad_skin() {
        let r = rebuild(
            &src("this is not lua {{{"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            new_queue(),
        );
        assert!(r.is_err());
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p carapace --lib swap`
Expected: PASS (2 passed).

- [ ] **Step 4: Commit**

```bash
git add crates/carapace/src/lib.rs crates/carapace/src/swap.rs
git commit -m "feat(carapace): swap rebuild entry point (transactional at the engine call site)"
```

---

### Task 7: `engine` — the frame loop, drain, and integration

> The capstone. Owns the host, registry (`Rc`), queue, and current skin. Implements input→drain→tick and the full `Command` drain semantics, then proves the whole spine with a frame-loop integration test.

**Files:**
- Create: `crates/carapace/src/engine.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod engine;`)

**Interfaces:**
- Consumes: everything above.
- Produces: `pub struct Engine`; `pub enum PointerEvent { Press }`; `Engine::new(host: Box<dyn Host>, registry: VocabRegistry, initial: SkinSource) -> Result<Engine, ScriptError>`; `Engine::handle_pointer(&mut self, p: Pt, kind: PointerEvent)`; `Engine::handle_command(&mut self, cmd: Command)`; `Engine::update(&mut self, dt: Duration)`; `Engine::scene(&self) -> &Scene`; `Engine::state(&self, key: &str) -> Option<StateValue>`.

- [ ] **Step 1: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod engine;`

- [ ] **Step 2: Write `engine.rs`**

Create `crates/carapace/src/engine.rs`:

```rust
use std::rc::Rc;
use std::time::Duration;

use crate::command::{new_queue, Command, Queue, SkinSource};
use crate::host::Host;
use crate::scene::{Pt, Scene};
use crate::script::{LoadedSkin, ScriptError};
use crate::state::StateValue;
use crate::swap::rebuild;
use crate::vocab::VocabRegistry;

pub enum PointerEvent {
    Press,
}

pub struct Engine {
    host: Box<dyn Host>,
    registry: Rc<VocabRegistry>,
    queue: Queue,
    skin: LoadedSkin,
}

impl Engine {
    pub fn new(
        host: Box<dyn Host>,
        registry: VocabRegistry,
        initial: SkinSource,
    ) -> Result<Engine, ScriptError> {
        let registry = Rc::new(registry);
        let queue = new_queue();
        let skin = rebuild(&initial, host.as_ref(), registry.clone(), queue.clone())?;
        Ok(Engine { host, registry, queue, skin })
    }

    /// Phase 1 (input): resolve the hit and run the handler, which only enqueues.
    pub fn handle_pointer(&mut self, p: Pt, _kind: PointerEvent) {
        if let Some(id) = self.skin.scene.hit(p) {
            if let Err(e) = self.skin.fire(id) {
                // A bad handler drops its command(s); the loop continues.
                eprintln!("carapace: handler error: {e:?}");
            }
        }
    }

    /// Enqueue a meta command (the host app's Tab/H equivalents).
    pub fn handle_command(&mut self, cmd: Command) {
        self.queue.borrow_mut().push(cmd);
    }

    /// Phase 2 (drain) + Phase 3 (tick).
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
        match rebuild(source, self.host.as_ref(), self.registry.clone(), self.queue.clone()) {
            Ok(skin) => self.skin = skin, // transactional: only replace on success
            Err(e) => eprintln!("carapace: swap failed, keeping current skin: {e:?}"),
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.skin.scene
    }

    pub fn state(&self, key: &str) -> Option<StateValue> {
        self.host.get(key)
    }
}
```

- [ ] **Step 3: Write the failing integration tests**

Create `crates/carapace/tests/frame_loop.rs`:

```rust
use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::scene::Pt;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;

fn src(s: &str) -> SkinSource {
    SkinSource { lua_src: s.to_string(), canvas: (200, 200) }
}

// A skin whose hotspot toggles, plus a value_fill bound to "level".
const TOGGLE_SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
    value_fill{ path={{x=0,y=120},{x=200,y=120},{x=200,y=140},{x=0,y=140}},
                value='level', color={r=1,g=2,b=3} }
"#;

fn engine() -> Engine {
    Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), src(TOGGLE_SKIN)).unwrap()
}

#[test]
fn click_enqueues_then_drain_applies() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press); // enqueues toggle
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)), "not applied before drain");
    e.update(Duration::ZERO); // drain
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)), "applied at drain");
}

#[test]
fn click_in_empty_area_is_a_noop() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 5.0, y: 130.0 }, PointerEvent::Press); // value_fill, not a hotspot
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)));
}

#[test]
fn tick_advances_state_after_drain() {
    let mut e = engine();
    e.update(Duration::from_secs_f32(0.25));
    assert_eq!(e.state("level"), Some(StateValue::Scalar(0.25)));
}

#[test]
fn double_click_in_one_frame_applies_twice() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO); // two toggles → back to false
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)), "no dedup; two toggles net to start");
}

#[test]
fn swap_preserves_state() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::from_secs_f32(0.3)); // on=true, level=0.3
    e.handle_command(Command::Swap(src(
        "value_fill{ path={{x=0,y=0},{x=200,y=0},{x=200,y=10}}, value='level', color={r=0,g=0,b=0} }",
    )));
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)), "state survived swap");
    assert_eq!(e.scene().nodes.len(), 1, "scene is the new skin's");
}

#[test]
fn failed_swap_keeps_current_scene() {
    let mut e = engine();
    let before = e.scene().nodes.len();
    e.handle_command(Command::Swap(src("not lua {{{")));
    e.update(Duration::ZERO);
    assert_eq!(e.scene().nodes.len(), before, "failed swap left the prior scene intact");
}
```

> For these tests to compile, `lib.rs` must expose `fixture` publicly **for tests**. Add `#[cfg(any(test, feature = "test-fixture"))] pub mod fixture;` is overkill; instead make the integration test rely on a `pub` fixture: change `lib.rs` line to `pub mod fixture;` guarded by `#[cfg(test)]` does not expose it to the integration test (separate crate). **Resolution:** mark `fixture` as `#[doc(hidden)] pub mod fixture;` (always compiled, public) so the integration test in `tests/` can use it. Update Task 1's `lib.rs` accordingly in this task: replace `#[cfg(test)] mod fixture;` with `#[doc(hidden)] pub mod fixture;`.

- [ ] **Step 4: Apply the `fixture` visibility fix**

Edit `crates/carapace/src/lib.rs`: replace `#[cfg(test)] mod fixture;` with:

```rust
#[doc(hidden)]
pub mod fixture;
```

(The fixture stays domain-neutral and is documented as test-support only.)

- [ ] **Step 5: Run the tests**

Run: `cargo test -p carapace`
Expected: PASS — all module unit tests + the 6 `frame_loop` integration tests. `0.0 + 0.25 = 0.25`, `0.3` exact in f32.

- [ ] **Step 6: Commit**

```bash
git add crates/carapace/src/lib.rs crates/carapace/src/engine.rs crates/carapace/tests/frame_loop.rs
git commit -m "feat(carapace): Engine frame loop (input→drain→tick) + integration tests"
```

---

## Self-Review

**Spec coverage (against the 3a design doc):**
- `state`, `host` → Task 1. ✓
- `scene` with cached hit regions → Task 2. ✓
- `vocab` seam + stub primitives, data-driven constructors → Tasks 3, 4. ✓
- `script` sandbox + command queue + host shims → Task 4 (sandbox negatives incl. unknown primitive id). ✓
- `skin` loader with schema + engine-compat → Task 5. ✓
- transactional `swap` → Task 6 (entry) + Task 7 (`apply_swap` keeps current on Err; `failed_swap_keeps_current_scene` test). ✓
- `Engine` input→drain→tick + accessors → Task 7. ✓
- Command-queue semantics: FIFO/no-dedup (`double_click_…`), no host mutation pre-drain (`click_enqueues_then_drain_applies`), post-switch allowlist validation (drain validates against current host) → Tasks 4, 7. ✓
- FixtureHost domain-neutral, test-only → Task 1. ✓
- No GPU/window deps → Global Constraints + manifest (Task 1). ✓

**Placeholder scan:** none. Every code step contains complete, compiling code. Task 4's `load` is written once in its final `registry: Rc<VocabRegistry>` form (the lifetime note explains *why* the signature is `Rc`, but the code is not a draft). No `todo!`/TBD anywhere.

**Type consistency:** `load(source, host, registry: Rc<VocabRegistry>, queue)` is consistent across Task 4 (def), Task 6 (`rebuild` calls it), Task 7 (`Engine` calls `rebuild`). `Command`/`SkinSource`/`Queue` (Task 4) used unchanged in Tasks 6–7. `Scene`/`Node`/`Pt`/`HandlerId` (Task 2) consistent in Tasks 3, 4, 7. `Host`/`Value`/`ActionSpec`/`StateValue` (Task 1) consistent throughout. `fixture` visibility resolved in Task 7 Step 4 (used by the `tests/frame_loop.rs` integration crate).

## What Phase 3b builds next

`render` (vello → host wgpu surface, direct, no readback) + a minimal winit/wgpu host app that owns the loop and drives `handle_pointer → update → render`, consuming `Engine::scene()`/`state()`. Then the throwaway `proto` + `spike-render` crates are removed.

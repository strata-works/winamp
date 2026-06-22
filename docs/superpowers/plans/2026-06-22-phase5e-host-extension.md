# Phase 5e — Host-Extension Mechanism Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a host register a domain primitive that binds its own actions with no Lua glue — a Rust-side `host_action` handler — and prove the seam with a `transport{}` extension defined in the external demo crate.

**Architecture:** A handler becomes `Lua(RegistryKey) | HostAction{action,args}`; `BuildContext::host_action(name,args)` lets a Rust primitive bind an allowlisted action; `LoadedSkin::fire` routes both kinds through the existing allowlist-validated drain. `carapace` re-exports `mlua` so external crates can implement `Primitive`. The demo's `TransportPrim` (in `carapace-demo`) composes 5d shapes + `host_action` into a one-line transport.

**Tech Stack:** Rust (edition 2024), `mlua` (re-exported), the existing `Engine`/`VocabRegistry`/command-queue model.

**Spec:** `docs/superpowers/specs/2026-06-22-phase5e-host-extension-design.md`

## Global Constraints

- Rust edition 2024; builds against Rust 1.96. CI builds `--locked`; keep `Cargo.lock` committed. **No new third-party crates** (the demo reaches `mlua` via carapace's re-export) — so no `sfw` step.
- **CI gates on clippy.** Before every commit run BOTH `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings`. Both clean (avoid unused-import lints in particular).
- The branch must stay **`cargo fmt --check` clean** — run `cargo fmt` before committing (the per-task clippy step does not catch formatting; 5c/5d both accrued fmt debt).
- **`scene::summary()` is unchanged** — extensions emit base nodes; a host-action hotspot prints the existing geometry-neutral `hotspot handler=<id>`.
- **No new validation:** a `host_action` handler fires a normal `Command::HostAction`, validated against `host.actions()` at drain (drop+warn if absent) — the existing path.
- **Closed `Node` enum** — extensions compose base nodes only; no custom render / new variants.
- All git commits use identity **Daniel Agbemava <danagbemava@gmail.com>**; never add Claude attribution.

---

## File Structure

- `crates/carapace/src/lib.rs` — `pub use mlua;` re-export.
- `crates/carapace/src/vocab.rs` — `BuildContext::host_action`; update all 5 impls.
- `crates/carapace/src/script.rs` — `HandlerSpec`/`Handler` enums; `SceneBuilder.handlers`; `LoadedSkin.queue`; `fire` routes both kinds.
- `crates/carapace/tests/host_extension.rs` *(new)* — engine-level positive + negative integration tests via the public API.
- `crates/carapace-demo/src/transport.rs` *(new)* — `TransportPrim` (external host extension).
- `crates/carapace-demo/src/lib.rs` — `pub mod transport;`.
- `crates/carapace-demo/src/main.rs` — register `TransportPrim`; add the `transport` skin to `SKINS`.
- `crates/carapace-demo/skins/transport/` *(new)* — `skin.toml` + `skin.lua`.
- `crates/carapace-demo/tests/skins_build.rs` — transport skin builds + click fires `toggle_play`.
- `README.md` — roadmap (Phase 5 complete) + host-extension note.

---

## Task 1: The `host_action` handler mechanism + `mlua` re-export

**Files:**
- Modify: `crates/carapace/src/lib.rs` (add `pub use mlua;`)
- Modify: `crates/carapace/src/vocab.rs` (`BuildContext` trait `:22-31`; the 5 impls at script.rs:40, vocab.rs:369/459/615/784)
- Modify: `crates/carapace/src/script.rs` (`SceneBuilder` `:35-57`, `LoadedSkin` `:28-32`, `load()` handler conversion, `fire()`)
- Create: `crates/carapace/tests/host_extension.rs`
- Test: `crates/carapace/src/script.rs` tests mod + the new integration test file

**Interfaces:**
- Produces: `BuildContext::host_action(&mut self, action: &str, args: Vec<crate::host::Value>) -> HandlerId`; `carapace::mlua` (re-export); `fire` enqueues a `Command::HostAction` for a host-action handler.

- [ ] **Step 1: Re-export mlua**

In `crates/carapace/src/lib.rs`, add near the top (after the module declarations):

```rust
/// Re-exported so host extensions can implement `vocab::Primitive` (whose `build` takes an
/// `mlua::Table`) without depending on `mlua` directly and version-matching the engine.
pub use mlua;
```

- [ ] **Step 2: Write the failing script-level test**

In `crates/carapace/src/script.rs` tests mod, add:

```rust
    #[test]
    fn host_action_handler_enqueues_directly_without_lua() {
        use crate::command::Command;
        use crate::scene::{region_of, Node, Pt};
        use crate::vocab::{BuildContext, BuildError, Primitive};
        use mlua::Table;

        // A minimal host extension: binds a host action via host_action (no Lua function).
        struct PingPrim;
        impl Primitive for PingPrim {
            fn id(&self) -> &str {
                "ping"
            }
            fn build(&self, _a: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
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
```

- [ ] **Step 3: Run to verify RED**

Run: `cargo test -p carapace script::tests::host_action_handler_enqueues_directly_without_lua`
Expected: FAIL — no `host_action` on `BuildContext`.

- [ ] **Step 4: Add the handler enums + generalize `SceneBuilder`**

In `crates/carapace/src/script.rs`, add the two enums (near the top, after the `use` lines):

```rust
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
```

Change `LoadedSkin` (`:28-32`) to:

```rust
pub struct LoadedSkin {
    pub scene: Scene,
    lua: Lua,
    handlers: Vec<Handler>,
    queue: Queue,
}
```

Change `SceneBuilder` (`:35-39`) — rename `handler_fns` → `handlers: Vec<HandlerSpec>`:

```rust
struct SceneBuilder {
    nodes: Vec<Node>,
    handlers: Vec<HandlerSpec>,
    assets: std::rc::Rc<crate::asset::AssetResolver>,
}
```

Update its `BuildContext` impl (`register_handler` pushes a Lua spec; add `host_action`):

```rust
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
```

- [ ] **Step 5: Update `load()`'s builder init + handler conversion + `LoadedSkin` construction**

In `load()`, the builder init changes `handler_fns: Vec::new()` → `handlers: Vec::new()`. The take/convert block becomes:

```rust
    let (nodes, specs) = {
        let mut b = builder.borrow_mut();
        (std::mem::take(&mut b.nodes), std::mem::take(&mut b.handlers))
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
        lua,
        handlers,
        queue,
    })
```

(`queue` is `load()`'s argument — the host shims already captured their own clones, so moving it into `LoadedSkin` here is fine.)

- [ ] **Step 6: Route `fire()` on the handler kind**

Replace `impl LoadedSkin { fn fire … }`:

```rust
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
```

- [ ] **Step 7: Add `host_action` to the `BuildContext` trait + all test impls**

In `crates/carapace/src/vocab.rs`, add to the trait (after `register_handler`):

```rust
    fn host_action(&mut self, action: &str, args: Vec<crate::host::Value>) -> HandlerId;
```

Add this stub method to each of the 4 test `BuildContext` impls (`NoHandlers` `:369`, `Counter` `:459`, `Ctx` `:615`, `Ctx` `:784`):

```rust
        fn host_action(&mut self, _action: &str, _args: Vec<crate::host::Value>) -> HandlerId {
            0
        }
```

- [ ] **Step 8: Run the script tests**

Run: `cargo test -p carapace --lib`
Expected: PASS — the new test plus the existing Lua-handler tests (`handler_enqueues_command_without_touching_host`, `action_args_are_captured`) still green (Lua path unchanged).

- [ ] **Step 9: Write the engine integration tests (positive + negative)**

Create `crates/carapace/tests/host_extension.rs`:

```rust
// Proves the host-extension seam through the PUBLIC api: an external Primitive impl that binds
// host actions via `host_action`, driven end-to-end through Engine + FixtureHost.
use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::mlua::Table;
use carapace::scene::{region_of, Node, Pt};
use carapace::state::StateValue;
use carapace::vocab::{BuildContext, BuildError, Primitive, VocabRegistry};

// A 100x100 hotspot bound to a configurable host action via host_action.
struct ActionButton {
    id: &'static str,
    action: &'static str,
}
impl Primitive for ActionButton {
    fn id(&self) -> &str {
        self.id
    }
    fn build(&self, _a: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let path = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 100.0, y: 0.0 },
            Pt { x: 100.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let hid = ctx.host_action(self.action, vec![]);
        Ok(vec![Node::Hotspot {
            region: region_of(&path),
            on_press: hid,
        }])
    }
}

fn engine_with(prim: ActionButton, lua: &str) -> Engine {
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(prim));
    Engine::new(
        Box::new(FixtureHost::new()),
        reg,
        SkinSource::inline(lua, (100, 100)),
    )
    .unwrap()
}

#[test]
fn extension_host_action_fires_through_the_drain() {
    // FixtureHost: `toggle` flips `on`.
    let mut e = engine_with(ActionButton { id: "toggler", action: "toggle" }, "toggler{}");
    assert_eq!(e.state("on"), Some(StateValue::Bool(false)));
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)), "extension fired the host action");
}

#[test]
fn extension_unregistered_action_is_dropped_not_panicked() {
    // FixtureHost has no `frobnicate` action -> dropped at drain, no state change, no panic.
    let mut e = engine_with(ActionButton { id: "bad", action: "frobnicate" }, "bad{}");
    let before = e.state("on");
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), before, "unregistered action left host state unchanged");
}
```

- [ ] **Step 10: Run the integration tests + clippy + fmt + commit**

```bash
cargo test -p carapace --test host_extension
cargo test -p carapace --lib
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings
git add crates/carapace/src/lib.rs crates/carapace/src/vocab.rs crates/carapace/src/script.rs \
  crates/carapace/tests/host_extension.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(vocab): host_action handler so extensions bind host actions (no Lua)"
```
Expected: all green; both integration tests pass.

---

## Task 2: The `transport{}` demo extension (external crate)

**Files:**
- Create: `crates/carapace-demo/src/transport.rs`
- Modify: `crates/carapace-demo/src/lib.rs` (`pub mod transport;`)
- Modify: `crates/carapace-demo/src/main.rs` (register the prim; add the skin to `SKINS`)
- Create: `crates/carapace-demo/skins/transport/skin.toml`, `crates/carapace-demo/skins/transport/skin.lua`
- Modify: `crates/carapace-demo/tests/skins_build.rs` (build + click test)

**Interfaces:**
- Consumes: `carapace::vocab::{Primitive, BuildContext, BuildError}`, `carapace::mlua::Table` (Task 1), `carapace::shape::rect`, `carapace::scene::{Node, Pt, Color, Paint, FillDir, region_of}`, `ctx.host_action` (Task 1).
- Produces: `carapace_demo::transport::TransportPrim`; the `transport` skin.

- [ ] **Step 1: Write the failing demo test**

In `crates/carapace-demo/tests/skins_build.rs`, add a transport-aware builder and a test:

```rust
fn engine_with_transport(skin_dir: &str) -> carapace::engine::Engine {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins").join(skin_dir);
    let (_m, source) = carapace::skin::load_dir(&dir).expect("load skin dir");
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(carapace_demo::transport::TransportPrim));
    Engine::new(Box::new(DemoHost::new()), reg, source).expect("skin builds")
}

#[test]
fn transport_extension_builds_and_play_click_toggles_host() {
    use carapace::engine::PointerEvent;
    use carapace::scene::Node;
    use carapace::state::StateValue;
    use std::time::Duration;

    let mut e = engine_with_transport("transport");
    let nodes = e.scene().nodes.clone();
    assert!(nodes.iter().any(|n| matches!(n, Node::Hotspot { .. })), "has a hotspot");
    assert!(nodes.iter().any(|n| matches!(n, Node::ValueFill { .. })), "has a seek bar");

    // play button rect = (20,20,40,40) -> center (40,40); clicking fires toggle_play.
    assert_eq!(e.state("playing"), Some(StateValue::Bool(false)));
    e.handle_pointer(carapace::scene::Pt { x: 40.0, y: 40.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(e.state("playing"), Some(StateValue::Bool(true)), "transport play toggled the host");
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo --test skins_build transport_extension_builds_and_play_click_toggles_host`
Expected: FAIL — `carapace_demo::transport` does not exist.

- [ ] **Step 3: Write `TransportPrim`**

Create `crates/carapace-demo/src/transport.rs`:

```rust
//! A host extension: the media transport, registered by the demo host. Defined in this
//! external crate (not the engine) — it implements `carapace::vocab::Primitive` from
//! carapace's public API alone, binding the host's own actions via `host_action`.

use carapace::mlua::Table;
use carapace::scene::{region_of, Color, FillDir, Node, Paint, Pt};
use carapace::shape;
use carapace::vocab::{BuildContext, BuildError, Primitive};

pub struct TransportPrim;

fn solid(r: u8, g: u8, b: u8) -> Paint {
    Paint::Solid(Color { r, g, b, a: 255 })
}

impl Primitive for TransportPrim {
    fn id(&self) -> &str {
        "transport"
    }

    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;

        let play = shape::rect(x, y, 40.0, 40.0);
        let play_id = ctx.host_action("toggle_play", vec![]);
        let stop = shape::rect(x + 48.0, y, 40.0, 40.0);
        let stop_id = ctx.host_action("stop", vec![]);
        let seek = shape::rect(x, y + 48.0, 88.0, 10.0);

        Ok(vec![
            Node::Fill { path: play.clone(), paint: solid(80, 200, 120) },
            Node::Hotspot { region: region_of(&play), on_press: play_id },
            Node::Fill { path: stop.clone(), paint: solid(200, 80, 80) },
            Node::Hotspot { region: region_of(&stop), on_press: stop_id },
            Node::ValueFill {
                path: seek,
                value_key: "position".to_string(),
                color: Color { r: 240, g: 220, b: 80, a: 255 },
                direction: FillDir::Right,
            },
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carapace::vocab::VocabRegistry;

    #[test]
    fn registers_into_a_vocab_registry() {
        // The seam: an external crate's primitive registers like a built-in (base 5 + this = 6).
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(TransportPrim));
        assert_eq!(reg.iter().count(), 6);
    }
}
```

Note: `Pt` is imported because `region_of(&play)` takes `&[Pt]` and `shape::rect` returns `Vec<Pt>` — if clippy flags `Pt` as unused, drop the import (the slices are already `Vec<Pt>`); keep the import only if referenced. Verify with clippy in Step 7.

- [ ] **Step 4: Export the module**

In `crates/carapace-demo/src/lib.rs`, add:

```rust
pub mod transport;
```

- [ ] **Step 5: Register the extension + add the skin in `main.rs`**

In `crates/carapace-demo/src/main.rs`:
- Change `const SKINS: [&str; 3] = [...]` to include the transport skin:
  ```rust
  const SKINS: [&str; 4] = ["skins/classic", "skins/minimal", "skins/reference", "skins/transport"];
  ```
- Add a registry helper (near the top of the file's `impl App` or as a free fn):
  ```rust
  fn demo_registry() -> VocabRegistry {
      let mut r = VocabRegistry::base();
      r.register(Box::new(carapace_demo::transport::TransportPrim));
      r
  }
  ```
- In `App::new`, change the `Engine::new(..., VocabRegistry::base(), src)` call to use `demo_registry()`:
  ```rust
  let engine = Engine::new(Box::new(DemoHost::new()), demo_registry(), src).unwrap();
  ```
  (The Tab-swap path reuses the engine's stored registry, so this single change wires the extension into swaps too.)

- [ ] **Step 6: Create the transport skin**

`crates/carapace-demo/skins/transport/skin.toml`:

```toml
schema = 1
id = "transport"
name = "Transport (host extension)"
engine = "^0.1"
canvas = { width = 300, height = 140 }
entry = "skin.lua"
```

`crates/carapace-demo/skins/transport/skin.lua`:

```lua
-- A backdrop plus a host-registered extension: one declaration is a full transport,
-- wired to the host's own actions (toggle_play / stop) with zero Lua glue.
fill{ path = rect{x=0, y=0, w=300, h=140}, color = {r=20, g=24, b=34} }
transport{ x = 20, y = 20 }
```

- [ ] **Step 7: Run the tests + build + clippy + fmt + commit**

```bash
cargo test -p carapace-demo
cargo build -p carapace-demo
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/transport.rs crates/carapace-demo/src/lib.rs \
  crates/carapace-demo/src/main.rs crates/carapace-demo/skins/transport \
  crates/carapace-demo/tests/skins_build.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): transport{} host extension wired to host actions"
```
Expected: demo tests pass (incl. the click-toggles test + the registry unit test); compiles; clippy clean.

- [ ] **Step 8: Human smoke check (optional)**

Run: `cargo run -p carapace-demo`, Tab to the `transport` skin: clicking the green play button toggles playback (the seek bar starts/stops advancing); the red stop button resets it.

---

## Task 3: README + roadmap (Phase 5 complete)

**Files:**
- Modify: `README.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Update the roadmap**

In `README.md` Roadmap, mark 5e done and note Phase 5 is complete. Replace the `5e` bullet with:

```markdown
- **Phase 5e — host-extension mechanism.** ✅ A host registers a domain primitive
  (`VocabRegistry::register`) that binds its own actions via a Rust-side `host_action` handler —
  no Lua glue. The demo's `transport{}` (defined in the demo crate, not the engine) proves the
  seam. **Phase 5 is complete.**
- **Phase 6 — validation** against both a media-player and a system-monitor host, proving zero
  media-specific knowledge in the engine.
```

(If a `Phase 6` bullet already exists below, replace it with this single one to avoid duplication.)

- [ ] **Step 2: Note host extensions in the vocabulary bullet**

In the "Domain-neutral base vocabulary, host-extensible" bullet, append:

```markdown
  A host registers its own domain primitives through `VocabRegistry::register`; they appear in the
  skin env exactly like built-ins and can bind the host's allowlisted actions directly (e.g. the
  demo's `transport{}`). `carapace` re-exports `mlua` so an extension crate needs no direct `mlua`
  dependency.
```

- [ ] **Step 3: Verify suite + fmt + clippy**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings`
Expected: PASS / clean. (GPU suite separately: `cargo test -p carapace --features gpu-tests --test render_offscreen`.)

- [ ] **Step 4: Commit**

```bash
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs: README roadmap current through Phase 5e (Phase 5 complete)"
```

---

## Self-Review (completed during planning)

**Spec coverage:**
- `Handler = Lua | HostAction` + `BuildContext::host_action` + `fire` routing → Task 1. ✅
- `mlua` re-export (so external crates implement `Primitive`) → Task 1 Step 1 (gap the spec's "external crate" requirement implies). ✅
- Headless mechanism test (host_action → fire → queue) → Task 1 Step 2; engine positive + negative integration → Task 1 Step 9. ✅
- `TransportPrim` in the demo crate (external), registered by the host → Task 2. ✅
- New `transport` skin + Tab cycling → Task 2 Steps 5–6. ✅
- Demo build + click-fires-toggle_play test → Task 2 Step 1. ✅
- `summary()` unchanged; closed `Node`; no new validation → respected (no task touches summary/Node/drain). ✅
- README roadmap (Phase 5 complete) → Task 3. ✅
- clippy (both feature sets) + fmt gates → Global Constraints + each task's commit step. ✅

**Deferred (per spec, no task):** custom render / new Node variants (visualizer / live host view region); click-to-seek; build-time action validation.

**Type consistency:** `host_action(&mut self, action: &str, args: Vec<crate::host::Value>) -> HandlerId` (trait + all impls + extension call sites); `Handler`/`HandlerSpec` mirror `Lua | HostAction { action: String, args: Vec<Value> }`; `TransportPrim` emits the base `Node` variants with their post-5d shapes (`Fill{path,paint}`, `Hotspot{region,on_press}`, `ValueFill{path,value_key,color,direction}`). Consistent.

**Compile-safety:** Task 1 adds a trait method → every `BuildContext` impl (1 real + 4 test) updated in the same task, so the crate compiles before tests run. No `Node`/match changes (the variants are unchanged), so no exhaustiveness breakage.

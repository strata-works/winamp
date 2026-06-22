# Phase 5e — Host-Extension Mechanism — Design

**Date:** 2026-06-22
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Phase 5 (base vocabulary + host extensions + assets), decomposed — **5e is the
fifth and final sub-project**, after 5a (assets/`image`), 5b (`Paint`/gradients), 5c (text/fonts),
and 5d (vocab ergonomics). Exercises the Phase 2 vocab/host-extension seam.

## Purpose

Make the Phase 2 host-extension seam genuinely usable: let a **host** register its own *domain*
primitive that binds the host's **own actions**, and prove it by defining a real extension
(`transport{}`) in an **external crate** (the demo), not the engine. This closes Phase 5 — the base
vocabulary plus the mechanism by which a host extends it.

The registration mechanism itself (`VocabRegistry::register`) already exists from Phase 2 and is
used by every skin (`VocabRegistry::base()`). The gap 5e fills is that a base primitive wires
interactivity through a **Lua function** (`region{ on_press=fn }` → `register_handler(mlua::Function)`),
but a host extension is **Rust** that knows its own action names (`toggle_play`, `stop`) and has no
Lua function to hand over. 5e adds a Rust-side action handler so an extension can bind its actions
directly — no Lua glue in the skin.

### Phase 5 decomposition (recorded; 5e is fifth/final)

| Sub-project | Adds | Status |
|---|---|---|
| 5a | asset resolver + `image` primitive | done |
| 5b | `Paint` (solid + linear/radial/sweep gradient) + color alpha | done |
| 5c | text + fonts (`text{}`, parley, value-bound strings) | done |
| 5d | shape helpers; `on_press` on drawables; `value_fill` direction + clip | done |
| **5e (this doc)** | host-extension: Rust-side `host_action` handler + a demo `transport{}` extension | this spec |

## Scope

**In scope:**
- A **`host_action` handler kind**: `Handler = Lua(RegistryKey) | HostAction { action, args }`;
  `BuildContext::host_action(&mut self, action: &str, args: Vec<Value>) -> HandlerId`; `LoadedSkin`
  fires either kind (Lua call, or a direct `Command::HostAction` push) through the existing drain.
- A **demo host extension `transport{}`** defined in the `carapace-demo` crate (external to the
  engine), registered by the demo host, composing base nodes (5d `shape::rect` fills + hotspots
  bound via `host_action` + a `value_fill` seek) into a one-line transport widget.
- A new **`transport` demo skin** using it, and the demo registering `TransportPrim`.
- Tests proving: the `host_action` path enqueues + drains correctly (no Lua), the negative
  (unregistered action dropped at drain), and the extension/skin build + click end to end.

**Out of scope (later / separate):**
- **Custom render / new `Node` variants** (a visualizer or host-painted region) — the closed
  `Node` enum is unchanged; extensions compose **base** nodes only. The host-painted "live host
  view region" is separately planned for after Phase 6.
- **Click-to-value** (e.g. click-to-seek mapping a click position to an action arg) — deferred in
  5d; the seek bar stays display-only here.
- **Build-time action validation** — the drain already validates `HostAction` against the current
  host's allowlist and drops+warns if absent (Phase 2 cross-host rule); 5e reuses that, adding no
  new validation.
- **Per-extension state/asset capabilities beyond the existing `BuildContext`** (`image`, `font`,
  `register_handler`, and the new `host_action`).

## 1. Architecture & invariants

The headless/GPU split and the Phase 2 seam are preserved. Extensions are **pure carapace-public-API
Rust** — they never touch `mlua` or the GPU.

- **Seam proven externally.** `TransportPrim` lives in `carapace-demo` and implements
  `carapace::vocab::Primitive` using only public items (`Primitive`, `BuildContext`, `Node`,
  `host_action`, `region_of`, `shape::rect`, `scene` types, `host::Value`). If it compiles and
  works from an external crate, the seam is genuinely usable by a host.
- **Extensions are domain-trusted Rust, the skin stays sandboxed.** A skin only *names* the
  `transport` constructor (it appears in the env exactly like a built-in); it cannot fabricate
  capabilities. The extension is shipped by the host and may bind the host's allowlisted actions.
- **One handler-id space.** `register_handler` (Lua) and `host_action` (Rust) both allocate from the
  same `HandlerId` sequence; `Scene::hit` / `LoadedSkin::fire` are agnostic to the kind.
- **Same drain, same validation.** A `host_action` handler fires a normal `Command::HostAction`,
  validated against `host.actions()` at drain (drop+warn if absent) — identical to a Lua-issued
  action. No new control path.
- **Closed `Node`, unchanged `summary()`.** Extensions emit base nodes; a host-action hotspot prints
  the existing geometry-neutral `hotspot handler=<id>`. Transactional swap, scene-as-projection, and
  the domain-neutral engine are all unaffected (the *demo* carries the media meaning, not the engine).

```
vocab.rs   # BuildContext gains host_action(action, args) -> HandlerId; update all impls
script.rs  # HandlerSpec (builder) + Handler (loaded) enums; SceneBuilder collects a unified
           #   Vec<HandlerSpec>; LoadedSkin keeps a queue clone + Vec<Handler>; fire() matches kind
crates/carapace-demo/src/transport.rs   (new) # TransportPrim: external host extension primitive
crates/carapace-demo/src/main.rs        # build registry = base() + register(TransportPrim)
crates/carapace-demo/skins/transport/   (new) # backdrop + transport{ x, y }
crates/carapace-demo/tests/skins_build.rs     # transport skin builds + click fires toggle_play
README.md                               # roadmap 5e done; note host extensions
```

## 2. Handler model (`script.rs`, `vocab.rs`)

Today `SceneBuilder` collects `handler_fns: Vec<Function>` and `register_handler` returns the index.
Generalize to a unified handler list carrying either kind:

```rust
// script.rs — what the builder collects (pre-Lua-registration)
enum HandlerSpec {
    Lua(mlua::Function),
    HostAction { action: String, args: Vec<crate::host::Value> },
}

// script.rs — what the loaded skin stores
enum Handler {
    Lua(mlua::RegistryKey),
    HostAction { action: String, args: Vec<crate::host::Value> },
}
```

- `SceneBuilder` holds `handlers: Vec<HandlerSpec>`.
  - `register_handler(f)` → push `HandlerSpec::Lua(f)`, return `len-1` (unchanged behavior).
  - `host_action(action, args)` → push `HandlerSpec::HostAction { action: action.into(), args }`,
    return `len-1`.
- `BuildContext` (`vocab.rs`) gains:
  ```rust
  fn host_action(&mut self, action: &str, args: Vec<crate::host::Value>) -> HandlerId;
  ```
  All impls update: the real `SceneBuilder`, and the test impls (`NoHandlers`, `Counter`, `Ctx` in
  vocab tests). The test impls can return a stub id (e.g. `0`) — they don't drain.
- `load()` converts each `HandlerSpec` → `Handler`: `Lua(f)` → `Handler::Lua(create_registry_value(f))`;
  `HostAction{..}` → `Handler::HostAction{..}` (move-through).
- `LoadedSkin` gains a `queue: Queue` clone (already available in `load`'s args) and stores
  `handlers: Vec<Handler>`.
- `fire(&self, id)`:
  ```rust
  match &self.handlers[id] {
      Handler::Lua(key) => { let f: Function = self.lua.registry_value(key)?; f.call::<()>(())?; }
      Handler::HostAction { action, args } => {
          self.queue.borrow_mut().push(Command::HostAction { action: action.clone(), args: args.clone() });
      }
  }
  ```
  Both end with a `HostAction` in the queue; `engine.update` drains and validates it identically.

## 3. The demo extension — `transport{}` (`carapace-demo`)

`TransportPrim` is defined in `crates/carapace-demo/src/transport.rs` and implements
`carapace::vocab::Primitive` using only carapace's public API — the external-crate proof.

```lua
-- a whole working transport from one declaration; the extension wires the host's actions:
transport{ x = 20, y = 20 }
```

`build(args, ctx)`:
- read `x: f32`, `y: f32` (required → `MissingField`).
- **play button:** `Node::Fill { path: shape::rect(x, y, 40.0, 40.0), paint: <green> }` and a
  `Node::Hotspot { region: region_of(&rect), on_press: ctx.host_action("toggle_play", vec![]) }`.
- **stop button:** a second rect fill + hotspot → `ctx.host_action("stop", vec![])`, offset to the
  right of play.
- **seek bar:** `Node::ValueFill { path: shape::rect(x, y+48, 88, 10), value_key: "position",
  color: <accent>, direction: FillDir::Right }`.
- returns `vec![play_fill, play_hotspot, stop_fill, stop_hotspot, seek]` (5d multi-emit).

The seek stays display-only (click-to-seek deferred). Colors are the extension's own choice (generic
RGB), not engine knowledge.

**Registration (the host's job)** — `crates/carapace-demo/src/main.rs`:
```rust
let mut reg = VocabRegistry::base();
reg.register(Box::new(carapace_demo::transport::TransportPrim));
let engine = Engine::new(Box::new(DemoHost::new()), reg, src)?;
```

**New skin** — `crates/carapace-demo/skins/transport/`: `skin.toml` (canvas, entry) + `skin.lua`:
```lua
fill{ path = rect{x=0, y=0, w=300, h=140}, color = {r=20, g=24, b=34} }
transport{ x = 20, y = 20 }
```
Tab cycles to it alongside classic / minimal / reference. (`carapace_demo` is a lib crate — add a
`pub mod transport;` so the bin and tests can reach `TransportPrim`.)

## 4. Testing

**Headless (engine, `carapace`):**
- A test extension primitive (in the `vocab` or `script` tests) that calls
  `ctx.host_action("toggle", vec![])` and emits a hotspot over a known region; firing the hotspot
  enqueues `Command::HostAction { action: "toggle", .. }` (assert the queue) with no Lua involved.
- End-to-end via `Engine` + `FixtureHost`: register the test extension, build a skin that uses it,
  click inside the hotspot, `update` → `FixtureHost`'s `on` flips (action invoked through the drain).
- Negative: an extension issuing `host_action("nope", vec![])` → after a drain the host is unchanged
  and a drop is logged (reuses the existing allowlist drop; assert no state change).
- `register_handler` (Lua) path still works unchanged (existing `script` tests stay green).

**Demo (`carapace-demo`):**
- `TransportPrim` builds the expected node set (a `Hotspot`, a `ValueFill`, fills) from
  `transport{ x, y }`.
- The `transport` skin loads and a `Scene::hit` at the play button's center returns a handler that,
  when fired and drained, invokes `toggle_play` on `DemoHost` (`playing` flips).

**Human:** `cargo run -p carapace-demo` → Tab to the `transport` skin: the play/stop buttons click
(toggle/stop the demo host), the seek bar advances while playing.

## 5. Error handling

- Malformed `transport{}` args (missing `x`/`y`) → `BuildError` → transactional swap keeps the prior
  scene.
- A `host_action` naming an unregistered action → dropped at drain with a logged warning (existing
  behavior); never panics.
- No panics on a skin/extension fault; the engine returns `Result` / degrades. `unwrap` only on
  engine invariants (as today).

## Definition of done (5e)

`BuildContext::host_action` exists and a Rust extension can bind a host action with no Lua function;
`fire` routes both handler kinds through the same allowlist-validated drain; `TransportPrim` is
defined in the demo crate (external to the engine) and registered by the demo host; a `transport`
skin renders a one-line working transport whose buttons fire the host's `toggle_play`/`stop` and
whose seek bar advances; the headless boundary, the fast `check` CI job (incl. `clippy -D warnings`,
both feature sets), fmt, and the snapshot harness are all green. Phase 5 is complete.

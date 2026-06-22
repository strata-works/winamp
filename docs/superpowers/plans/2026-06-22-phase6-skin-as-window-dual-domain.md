# Phase 6 — Skin-as-Window, Dual-Domain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The demo renders the skin *as* a borderless, transparent, shaped, draggable window, and live-switches the whole window between a media player and a real `sysinfo`-backed system monitor on one engine.

**Architecture:** One domain-neutral engine change (a transparent `base_color` on the renderer). Everything else is in `carapace-demo`: a borderless/transparent winit window; window-control host actions (`begin_drag`/`minimize`/`close`) recorded into a shared `WindowOutbox` the App drains; a `SysmonHost` + `gauge{}` extension; an `H`-key `SwitchHost`.

**Tech Stack:** Rust (edition 2024), `vello` 0.9 / `wgpu` 29, `winit` 0.30, `sysinfo` (demo-only), the existing engine/vocab/command model.

**Spec:** `docs/superpowers/specs/2026-06-22-phase6-skin-as-window-dual-domain-design.md`

## Global Constraints

- Rust edition 2024 / Rust 1.96. CI builds `--locked`; keep `Cargo.lock` committed.
- **`sysinfo` is the only new crate**, added to **`carapace-demo`** via `sfw cargo add sysinfo -p carapace-demo` (never the engine).
- **Neutrality proof:** the only `crates/carapace/src/` change in the whole phase is `render.rs` (the `base_color` field). No other engine-source file changes. (The `render_offscreen` test changes only as a caller.)
- **CI gates on clippy + fmt.** Before every commit run `cargo fmt`, then BOTH `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings`. All clean.
- Borderless/transparent is validated on **macOS/Metal** (the spike's platform); the host codes defensively for the surface alpha mode (`PreMultiplied` unavailable on Metal → `PostMultiplied`).
- All git commits use identity **Daniel Agbemava <danagbemava@gmail.com>**; never add Claude attribution.

---

## File Structure

**Group 1 — total window replacement**
- `crates/carapace/src/render.rs` — `RenderTarget.base_color`; `draw` uses it.
- `crates/carapace/tests/render_offscreen.rs` — callers pass `base_color`; a transparency GPU test.
- `crates/carapace-demo/examples/shoot.rs`, `crates/carapace-demo/src/main.rs` — callers pass `base_color` (opaque in Task 1; main flips to transparent in Task 3).
- `crates/carapace-demo/src/window.rs` *(new)* — `WindowOp`, `WindowOutbox`, `WINDOW_ACTIONS`, `handle_window_action`.
- `crates/carapace-demo/src/demo_host.rs` — window-control actions + outbox.
- `crates/carapace-demo/src/main.rs` — borderless/transparent window; transparent base; drain the outbox.
- `crates/carapace-demo/skins/{classic,minimal,transport,reference}/skin.lua` — `rounded_rect` backdrop + drag/min/close.

**Group 2 — dual-domain**
- `crates/carapace-demo/src/sysmon_host.rs` *(new)* — `SysmonHost` (`sysinfo`).
- `crates/carapace-demo/src/gauge.rs` *(new)* — `gauge{}` extension.
- `crates/carapace-demo/skins/sysmon/` *(new)* — sysmon skin.
- `crates/carapace-demo/src/main.rs` — `H`-key `SwitchHost`; per-domain skin lists; registry unions `transport`+`gauge`.
- `crates/carapace-demo/src/lib.rs` — `pub mod window/sysmon_host/gauge;`.
- `crates/carapace-demo/tests/host_switch.rs` *(new)* — cross-domain on one `Engine`.
- `README.md` — roadmap (Phases 0–6 complete).

---

## Task 1: Engine — configurable transparent `base_color`

**Files:**
- Modify: `crates/carapace/src/render.rs` (`RenderTarget` `:21-27`; `draw` `RenderParams` `:346`)
- Modify: `crates/carapace/tests/render_offscreen.rs` (10 `RenderTarget` literals + a new test)
- Modify: `crates/carapace-demo/examples/shoot.rs` (`:140` `RenderTarget`), `crates/carapace-demo/src/main.rs` (the `RenderTarget` in `RedrawRequested`)

**Interfaces:**
- Produces: `RenderTarget { …, base_color: crate::scene::Color }`. Opaque `Color{0,0,0,255}` = today's behavior; `Color{0,0,0,0}` = transparent.

- [ ] **Step 1: Write the failing GPU test**

Append to `crates/carapace/tests/render_offscreen.rs` (add an alpha reader near `px`):

```rust
fn alpha_at(data: &[u8], w: u32, x: u32, y: u32) -> u8 {
    data[((y * w + x) * 4 + 3) as usize]
}

#[test]
fn transparent_base_color_leaves_undrawn_pixels_clear() {
    use carapace::scene::{Color, Node, Paint, Pt, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    // One opaque fill in the top-left; the rest of the canvas is the transparent base.
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::Fill {
            path: rect(0.0, 0.0, 20.0, 20.0),
            paint: Paint::Solid(Color { r: 255, g: 0, b: 0, a: 255 }),
        }],
    };
    r.draw(
        &scene,
        |_k| None,
        &RenderTarget {
            device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h,
            base_color: Color { r: 0, g: 0, b: 0, a: 0 },
        },
    );
    let data = readback(&o);
    assert_eq!(alpha_at(&data, 100, 10, 10), 255, "drawn pixel is opaque");
    assert_eq!(alpha_at(&data, 100, 80, 80), 0, "undrawn pixel is transparent (base alpha 0)");
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen transparent_base_color_leaves_undrawn_pixels_clear`
Expected: FAIL to compile — `RenderTarget` has no `base_color` field.

- [ ] **Step 3: Add the field + use it**

In `crates/carapace/src/render.rs`, add to `RenderTarget`:

```rust
    pub base_color: crate::scene::Color,
```

In `draw`, change the `RenderParams.base_color` line from the hardcoded value to:

```rust
                    base_color: VColor::from_rgba8(
                        target.base_color.r,
                        target.base_color.g,
                        target.base_color.b,
                        target.base_color.a,
                    ),
```

- [ ] **Step 4: Update every existing `RenderTarget` caller to opaque (no behavior change)**

In `crates/carapace/tests/render_offscreen.rs`, every existing `RenderTarget { … }` literal (there are 10) gains `base_color: carapace::scene::Color { r: 0, g: 0, b: 0, a: 255 },`. In `crates/carapace-demo/examples/shoot.rs` (`:140`) and `crates/carapace-demo/src/main.rs` (the `RenderTarget` in `RedrawRequested`), add the same opaque field. (Main flips to transparent in Task 3.)

- [ ] **Step 5: Run the GPU suite + workspace build**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen` then `cargo build --workspace`
Expected: PASS — the new transparency test plus all existing sentinels (now passing an explicit opaque base); the demo compiles.

- [ ] **Step 6: fmt + both clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace --all-targets --features gpu-tests -- -D warnings
git add crates/carapace/src/render.rs crates/carapace/tests/render_offscreen.rs \
  crates/carapace-demo/examples/shoot.rs crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(render): configurable transparent base_color on RenderTarget"
```

---

## Task 2: Window-control mechanism + `DemoHost` integration

**Files:**
- Create: `crates/carapace-demo/src/window.rs`
- Modify: `crates/carapace-demo/src/lib.rs` (`pub mod window;`)
- Modify: `crates/carapace-demo/src/demo_host.rs`
- Test: `window.rs` + `demo_host.rs` test mods

**Interfaces:**
- Produces:
  ```rust
  pub enum WindowOp { BeginDrag, Minimize, Close }
  pub type WindowOutbox = std::rc::Rc<std::cell::RefCell<Vec<WindowOp>>>;
  pub const WINDOW_ACTIONS: &[carapace::host::ActionSpec];   // begin_drag, minimize, close
  pub fn handle_window_action(action: &str, out: &WindowOutbox) -> bool;
  ```
  `DemoHost::with_outbox(WindowOutbox) -> Self` (and `new()` = a self-contained outbox).

- [ ] **Step 1: Write the failing tests**

`crates/carapace-demo/src/window.rs` (tests first, so it's RED until impl):

```rust
use carapace::host::ActionSpec;

#[derive(Debug, PartialEq)]
pub enum WindowOp {
    BeginDrag,
    Minimize,
    Close,
}

pub type WindowOutbox = std::rc::Rc<std::cell::RefCell<Vec<WindowOp>>>;

pub const WINDOW_ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "begin_drag" },
    ActionSpec { name: "minimize" },
    ActionSpec { name: "close" },
];

/// Records the matching window op; returns true iff `action` was a window-control action.
pub fn handle_window_action(action: &str, out: &WindowOutbox) -> bool {
    let op = match action {
        "begin_drag" => WindowOp::BeginDrag,
        "minimize" => WindowOp::Minimize,
        "close" => WindowOp::Close,
        _ => return false,
    };
    out.borrow_mut().push(op);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_window_ops_and_ignores_others() {
        let out: WindowOutbox = Default::default();
        assert!(handle_window_action("minimize", &out));
        assert!(handle_window_action("close", &out));
        assert!(!handle_window_action("toggle_play", &out), "domain action is not window-control");
        assert_eq!(&*out.borrow(), &[WindowOp::Minimize, WindowOp::Close]);
    }
}
```

In `crates/carapace-demo/src/demo_host.rs` tests mod, add:

```rust
    #[test]
    fn window_action_is_recorded_to_the_outbox() {
        use crate::window::{WindowOp, WindowOutbox};
        let out: WindowOutbox = Default::default();
        let mut h = DemoHost::with_outbox(out.clone());
        h.invoke("minimize", &[]);
        assert_eq!(&*out.borrow(), &[WindowOp::Minimize]);
        // domain actions still work
        h.invoke("toggle_play", &[]);
        assert_eq!(h.get("playing"), Some(StateValue::Bool(true)));
    }
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo window::tests::records_window_ops_and_ignores_others`
Expected: FAIL — `window` module not declared.

- [ ] **Step 3: Declare the module + wire `DemoHost`**

Add to `crates/carapace-demo/src/lib.rs`: `pub mod window;`.

Rewrite `crates/carapace-demo/src/demo_host.rs` to hold the outbox and a composed action list:

```rust
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;

use crate::window::{handle_window_action, WindowOutbox, WINDOW_ACTIONS};

pub struct DemoHost {
    playing: bool,
    position: f32,
    track_title: String,
    window: WindowOutbox,
    actions: Vec<ActionSpec>,
}

const DOMAIN_ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "toggle_play" },
    ActionSpec { name: "stop" },
];

impl DemoHost {
    pub fn with_outbox(window: WindowOutbox) -> Self {
        let mut actions = DOMAIN_ACTIONS.to_vec();
        actions.extend_from_slice(WINDOW_ACTIONS);
        Self {
            playing: false,
            position: 0.0,
            track_title: "Headspace — Track 01".to_string(),
            window,
            actions,
        }
    }
    pub fn new() -> Self {
        Self::with_outbox(WindowOutbox::default())
    }
}

impl Default for DemoHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for DemoHost {
    fn name(&self) -> &str {
        "demo-media"
    }
    fn tick(&mut self, dt: Duration) {
        if self.playing {
            self.position = (self.position + dt.as_secs_f32() * 0.1).min(1.0);
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => Some(StateValue::Scalar(self.position)),
            "track_title" => Some(StateValue::Str(std::sync::Arc::from(self.track_title.as_str()))),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        if handle_window_action(action, &self.window) {
            return;
        }
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
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace-demo`
Expected: PASS — window unit test + the demo-host window-op test + existing demo-host/skin tests (they call `DemoHost::new()`, still valid).

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/window.rs crates/carapace-demo/src/lib.rs crates/carapace-demo/src/demo_host.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): window-control actions via a shared WindowOutbox"
```

---

## Task 3: Borderless / transparent window + outbox drain (`main.rs`)

**Files:**
- Modify: `crates/carapace-demo/src/main.rs` (window attrs; surface `alpha_mode`; `RenderTarget.base_color`; App holds the outbox + constructs `DemoHost::with_outbox`; drain after `update`)

**Interfaces:**
- Consumes: `RenderTarget.base_color` (Task 1), `WindowOutbox`/`WindowOp`/`DemoHost::with_outbox` (Task 2).
- Produces: a borderless transparent window whose skin floats on the desktop; window-control ops applied.

GUI wiring — verified by compile + the human smoke check (no unit test).

- [ ] **Step 1: Add an outbox field + construct the host with it**

Add `use carapace_demo::window::{WindowOp, WindowOutbox};` and the `transport`/`demo_host` imports as needed. Add to `struct App` a field `window_outbox: WindowOutbox,`. In `App::new`, build the outbox first and pass it to the host:

```rust
    fn new() -> Self {
        let window_outbox: WindowOutbox = Default::default();
        let (src, _canvas) = load_source(0);
        let engine = Engine::new(
            Box::new(DemoHost::with_outbox(window_outbox.clone())),
            demo_registry(),
            src,
        )
        .unwrap();
        Self {
            skin_index: 0,
            engine,
            cursor: (0.0, 0.0),
            last: Instant::now(),
            window: None,
            gpu: None,
            renderer: None,
            window_outbox,
        }
    }
```

- [ ] **Step 2: Borderless + transparent window attrs**

Change the window attrs (`:144`) to drop the title and add the flags:

```rust
        let attrs = Window::default_attributes()
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(winit::dpi::LogicalSize::new(cw * INIT_SCALE, ch * INIT_SCALE));
```

- [ ] **Step 3: Pick a transparency-friendly surface alpha mode**

Replace `alpha_mode: caps.alpha_modes[0],` (`:167`) with a preference (PreMultiplied → PostMultiplied → first):

```rust
            let alpha_mode = if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                caps.alpha_modes[0]
            };
```
and use `alpha_mode,` in the `SurfaceConfiguration`.

- [ ] **Step 4: Transparent render base**

In the `RedrawRequested` `RenderTarget { … }`, set `base_color: carapace::scene::Color { r: 0, g: 0, b: 0, a: 0 },` (was opaque from Task 1).

- [ ] **Step 5: Drain the window outbox after `update`**

Add a method on `App`:

```rust
    fn apply_window_ops(&self, event_loop: &ActiveEventLoop) {
        for op in self.window_outbox.borrow_mut().drain(..) {
            match op {
                WindowOp::BeginDrag => {
                    if let Some(w) = &self.window {
                        let _ = w.drag_window();
                    }
                }
                WindowOp::Minimize => {
                    if let Some(w) = &self.window {
                        w.set_minimized(true);
                    }
                }
                WindowOp::Close => event_loop.exit(),
            }
        }
    }
```

Call it immediately after `self.engine.update(dt);` in `RedrawRequested`:

```rust
                self.engine.update(dt);
                self.apply_window_ops(event_loop);
```

- [ ] **Step 6: Build + human smoke check**

Run: `cargo build -p carapace-demo` (must compile), then `cargo run -p carapace-demo`.
Expected: a **borderless, transparent** window — no OS title bar, the desktop visible around the skin. (Skins still draw a full-canvas rectangle until Task 4 shapes them; window controls become clickable once Task 4 adds the glyphs.) Escape still quits.

- [ ] **Step 7: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): borderless transparent window + window-op drain"
```

---

## Task 4: Skin self-shaping + window-control glyphs

**Files:**
- Modify: `crates/carapace-demo/skins/classic/skin.lua`, `skins/minimal/skin.lua`, `skins/transport/skin.lua`, `skins/reference/skin.lua`
- Test: `crates/carapace-demo/tests/skins_build.rs` (assert the skins still build with the new window-control hotspots)

**Interfaces:**
- Consumes: `host.begin_drag()/minimize()/close()` (allowlisted by Task 2's `DemoHost`), `rounded_rect`/`text`/`region` vocab.

- [ ] **Step 1: Write the failing test**

In `crates/carapace-demo/tests/skins_build.rs`, add:

```rust
#[test]
fn skins_declare_window_controls() {
    use carapace::scene::Node;
    // Every vector skin must build AND reference the window-control actions (a skin naming an
    // un-allowlisted action fails to load), and expose hotspots for them.
    for skin in ["classic", "minimal", "transport"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins").join(skin);
        let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(carapace_demo::transport::TransportPrim));
        let e = Engine::new(Box::new(DemoHost::new()), reg, source)
            .unwrap_or_else(|err| panic!("{skin} failed to build: {err:?}"));
        let hotspots = e.scene().nodes.iter().filter(|n| matches!(n, Node::Hotspot { .. })).count();
        assert!(hotspots >= 3, "{skin} should have drag + min + close hotspots, found {hotspots}");
    }
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo --test skins_build skins_declare_window_controls`
Expected: FAIL — current skins have fewer hotspots / don't call the window actions.

- [ ] **Step 3: Add a shared chrome snippet to each vector skin**

**Placement matters** (`Scene::hit` returns the *last-added* hotspot covering a point, since it iterates in reverse): the whole-backdrop **drag region must be added FIRST** (lowest priority) so the skin's existing interactive controls — which stay *below* this block in the file — win their clicks; the min/close glyph regions sit in the empty top-right corner (no overlap with controls), so adding them in this top block is fine.

To `skins/classic/skin.lua`, `skins/minimal/skin.lua`, and `skins/transport/skin.lua`, **replace the existing backdrop `fill` line (the first line) with this block** — it stays at the top; the skin's existing interactive content remains after it. For **classic** (300×140):

```lua
-- shaped, draggable backdrop (rounded corners float over the desktop via the transparent base)
fill{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14}, color = {r=24, g=28, b=40} }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14},
        on_press = function() host.begin_drag() end }
-- minimize / close glyphs, top-right
text{ text = "_", x = 270, y = 4, size = 16, color = {r=200,g=200,b=210} }
region{ path = rect{x=266, y=4, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 286, y = 4, size = 16, color = {r=230,g=140,b=140} }
region{ path = rect{x=282, y=4, w=14, h=16}, on_press = function() host.close() end }
```

For **minimal** and **transport** (also 300×140), use the same chrome block (same coords). Keep each skin's existing interactive content (buttons/gauges/meters) **after** the drag region so it wins hit-testing (`Scene::hit` returns the topmost/last-added hotspot).

For **reference** (342×394 Headspace bitmap — stays rectangular, no `rounded_rect`): keep the `image{}` backdrop, add the same drag region over the full bitmap rect and min/close glyphs in its top-right (adjust x to ~318/334, y=6):

```lua
region{ path = rect{x=0, y=0, w=342, h=394}, on_press = function() host.begin_drag() end }
text{ text = "_", x = 314, y = 6, size = 16, color = {r=220,g=255,b=220} }
region{ path = rect{x=310, y=6, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 330, y = 6, size = 16, color = {r=255,g=160,b=160} }
region{ path = rect{x=326, y=6, w=14, h=16}, on_press = function() host.close() end }
```

(Note: the drag `region` is added first so the play/stop/transport hotspots, added later, sit on top and win clicks; only presses on bare backdrop start a drag.)

- [ ] **Step 4: Run the test + existing skin tests**

Run: `cargo test -p carapace-demo`
Expected: PASS — `skins_declare_window_controls` plus the existing per-skin tests (they assert their own content, which is preserved). If an existing test asserted an exact node count, update it for the added chrome nodes.

- [ ] **Step 5: Human smoke check**

Run: `cargo run -p carapace-demo` — the window is now a rounded floating shape; dragging the backdrop moves it; the `_`/`x` glyphs minimize/close; the play/stop controls still work.

- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/skins crates/carapace-demo/tests/skins_build.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): shaped skins with drag + minimize/close chrome"
```

---

## Task 5: `SysmonHost` (sysinfo)

**Files:**
- Modify: `crates/carapace-demo/Cargo.toml` (add `sysinfo`)
- Create: `crates/carapace-demo/src/sysmon_host.rs`
- Modify: `crates/carapace-demo/src/lib.rs` (`pub mod sysmon_host;`)
- Test: `sysmon_host.rs` test mod

**Interfaces:**
- Produces: `SysmonHost::with_outbox(WindowOutbox) -> Self`, `new()`; a `Host` exposing `cpu`/`mem`/`swap` `Scalar(0..1)` + `cpu_pct`/`mem_used` `Str`; `actions()` = `WINDOW_ACTIONS`.

- [ ] **Step 1: Add the dependency (via sfw)**

Run: `sfw cargo add sysinfo -p carapace-demo`
Then `cargo tree -p carapace-demo | grep sysinfo` to confirm the version resolved.

- [ ] **Step 2: Write the failing test**

Create `crates/carapace-demo/src/sysmon_host.rs` with the test first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use carapace::host::Host;
    use carapace::state::StateValue;
    use std::time::Duration;

    #[test]
    fn cpu_mem_swap_are_scalars_in_unit_range() {
        let mut h = SysmonHost::new();
        h.tick(Duration::from_millis(200)); // second sample populates cpu delta
        for key in ["cpu", "mem", "swap"] {
            match h.get(key) {
                Some(StateValue::Scalar(v)) => assert!((0.0..=1.0).contains(&v), "{key}={v}"),
                other => panic!("{key} should be a unit Scalar, got {other:?}"),
            }
        }
        assert!(matches!(h.get("cpu_pct"), Some(StateValue::Str(_))));
        assert!(h.get("nope").is_none());
    }
}
```

- [ ] **Step 3: Run to verify RED**

Run: `cargo test -p carapace-demo sysmon_host::tests::cpu_mem_swap_are_scalars_in_unit_range`
Expected: FAIL — module/type missing (declare `pub mod sysmon_host;` in `lib.rs` first to surface the real error).

- [ ] **Step 4: Implement `SysmonHost`**

Add `pub mod sysmon_host;` to `crates/carapace-demo/src/lib.rs`. Prepend to `sysmon_host.rs`:

```rust
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;
use sysinfo::System;

use crate::window::{handle_window_action, WindowOutbox, WINDOW_ACTIONS};

pub struct SysmonHost {
    sys: System,
    cpu: f32,
    mem: f32,
    swap: f32,
    window: WindowOutbox,
}

fn frac(used: u64, total: u64) -> f32 {
    if total == 0 { 0.0 } else { (used as f64 / total as f64) as f32 }
}

impl SysmonHost {
    pub fn with_outbox(window: WindowOutbox) -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self { sys, cpu: 0.0, mem: 0.0, swap: 0.0, window }
    }
    pub fn new() -> Self {
        Self::with_outbox(WindowOutbox::default())
    }
}

impl Default for SysmonHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for SysmonHost {
    fn name(&self) -> &str {
        "demo-sysmon"
    }
    fn tick(&mut self, _dt: Duration) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.cpu = (self.sys.global_cpu_usage() / 100.0).clamp(0.0, 1.0);
        self.mem = frac(self.sys.used_memory(), self.sys.total_memory());
        self.swap = frac(self.sys.used_swap(), self.sys.total_swap());
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "cpu" => Some(StateValue::Scalar(self.cpu)),
            "mem" => Some(StateValue::Scalar(self.mem)),
            "swap" => Some(StateValue::Scalar(self.swap)),
            "cpu_pct" => Some(StateValue::Str(std::sync::Arc::from(
                format!("{}%", (self.cpu * 100.0) as u32).as_str(),
            ))),
            "mem_used" => Some(StateValue::Str(std::sync::Arc::from(
                format!("{} MiB", self.sys.used_memory() / 1024 / 1024).as_str(),
            ))),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        WINDOW_ACTIONS
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        handle_window_action(action, &self.window);
    }
}
```

(Confirm the exact `sysinfo` method names against the resolved version with `cargo doc -p sysinfo --no-deps` — `global_cpu_usage`/`used_memory`/`total_memory`/`used_swap`/`total_swap`/`refresh_cpu_usage`/`refresh_memory` are the 0.3x API; adapt if the resolved version differs. The test is the source of truth.)

- [ ] **Step 5: Run the test + fmt + clippy + commit**

```bash
cargo test -p carapace-demo sysmon_host::
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/Cargo.toml Cargo.lock crates/carapace-demo/src/sysmon_host.rs crates/carapace-demo/src/lib.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): SysmonHost backed by sysinfo (cpu/mem/swap)"
```

---

## Task 6: `gauge{}` extension

**Files:**
- Create: `crates/carapace-demo/src/gauge.rs`
- Modify: `crates/carapace-demo/src/lib.rs` (`pub mod gauge;`)
- Test: `gauge.rs` test mod

**Interfaces:**
- Consumes: `carapace::mlua::Table`, `carapace::shape`, `carapace::scene::{Node,Color,Paint,FillDir,Pt,region_of}`, `carapace::vocab::{Primitive,BuildContext,BuildError}`.
- Produces: `carapace_demo::gauge::GaugePrim` (id `"gauge"`), emitting a frame fill + a vertical `value_fill` + a text label.

- [ ] **Step 1: Write the failing test**

Create `crates/carapace-demo/src/gauge.rs` with the test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use carapace::scene::{FillDir, Node};
    use carapace::vocab::VocabRegistry;

    #[test]
    fn registers_and_builds_a_vertical_gauge_with_label() {
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(GaugePrim));
        assert_eq!(reg.iter().count(), 6); // base 5 + gauge

        let lua = carapace::mlua::Lua::new();
        let t: carapace::mlua::Table = lua
            .load("return { x=10, y=10, value='cpu', label='CPU' }")
            .eval()
            .unwrap();
        struct NoCtx;
        impl carapace::vocab::BuildContext for NoCtx {
            fn register_handler(&mut self, _f: carapace::mlua::Function) -> usize { 0 }
            fn host_action(&mut self, _a: &str, _args: Vec<carapace::host::Value>) -> usize { 0 }
            fn image(&mut self, n: &str) -> Result<std::sync::Arc<carapace::asset::DecodedImage>, carapace::asset::AssetError> {
                Err(carapace::asset::AssetError::Unresolved(n.to_string()))
            }
            fn font(&mut self, n: &str) -> Result<std::sync::Arc<carapace::scene::FontData>, carapace::asset::AssetError> {
                Err(carapace::asset::AssetError::Unresolved(n.to_string()))
            }
        }
        let nodes = GaugePrim.build(&t, &mut NoCtx).unwrap();
        assert!(nodes.iter().any(|n| matches!(n, Node::ValueFill { direction: FillDir::Up, .. })));
        assert!(nodes.iter().any(|n| matches!(n, Node::Text { .. })));
    }
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo gauge::tests::registers_and_builds_a_vertical_gauge_with_label`
Expected: FAIL — `gauge` module/type missing.

- [ ] **Step 3: Implement `GaugePrim`**

Add `pub mod gauge;` to `crates/carapace-demo/src/lib.rs`. Prepend to `gauge.rs`:

```rust
//! A system-monitor domain extension: a labeled vertical meter. Defined in the demo crate,
//! registered by the host — composes the base vocab (a vertical value_fill + a text label + a
//! frame) entirely from carapace's public API.

use carapace::mlua::Table;
use carapace::scene::{Color, FillDir, HAlign, Node, Paint, Pt, TextContent, VAlign};
use carapace::shape;
use carapace::vocab::{BuildContext, BuildError, Primitive};

pub struct GaugePrim;

fn solid(r: u8, g: u8, b: u8) -> Paint {
    Paint::Solid(Color { r, g, b, a: 255 })
}

impl Primitive for GaugePrim {
    fn id(&self) -> &str {
        "gauge"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let value: String = args.get("value").map_err(|_| BuildError::MissingField("value"))?;
        let label: String = args.get("label").map_err(|_| BuildError::MissingField("label"))?;

        let frame = shape::rounded_rect(x, y, 40.0, 100.0, 6.0, 6);
        let bar = shape::rect(x + 6.0, y + 6.0, 28.0, 88.0);
        Ok(vec![
            Node::Fill { path: frame, paint: solid(30, 36, 48) },
            Node::ValueFill {
                path: bar,
                value_key: value,
                color: Color { r: 90, g: 210, b: 160, a: 255 },
                direction: FillDir::Up,
            },
            Node::Text {
                content: TextContent::Static(label),
                font: None,
                font_name: None,
                size: 12.0,
                paint: solid(210, 220, 230),
                halign: HAlign::Center,
                valign: VAlign::Top,
                max_width: None,
                pos: Pt { x: x + 20.0, y: y + 104.0 },
            },
        ])
    }
}
```

(The `Node::Text` field names/types are the real 5c shape — `content/font/font_name/size/paint/halign: HAlign/valign: VAlign/max_width/pos`. Cross-check against `scene.rs` if anything mismatches; the test's `Node::Text { .. }` match compiles against the real shape regardless.)

- [ ] **Step 4: Run the test + fmt + clippy + commit**

```bash
cargo test -p carapace-demo gauge::
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/gauge.rs crates/carapace-demo/src/lib.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): gauge{} system-monitor extension (vertical meter + label)"
```

---

## Task 7: The sysmon skin

**Files:**
- Create: `crates/carapace-demo/skins/sysmon/skin.toml`, `crates/carapace-demo/skins/sysmon/skin.lua`
- Test: `crates/carapace-demo/tests/skins_build.rs`

**Interfaces:**
- Consumes: `gauge{}` (Task 6), the window-control chrome pattern (Task 4), `SysmonHost` (Task 5).

- [ ] **Step 1: Write the failing test**

In `crates/carapace-demo/tests/skins_build.rs`, add:

```rust
#[test]
fn sysmon_skin_builds_gauges() {
    use carapace::scene::{FillDir, Node};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/sysmon");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(carapace_demo::gauge::GaugePrim));
    let e = Engine::new(Box::new(carapace_demo::sysmon_host::SysmonHost::new()), reg, source).unwrap();
    let up_meters = e
        .scene()
        .nodes
        .iter()
        .filter(|n| matches!(n, Node::ValueFill { direction: FillDir::Up, .. }))
        .count();
    assert!(up_meters >= 3, "cpu/mem/swap gauges, found {up_meters}");
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo --test skins_build sysmon_skin_builds_gauges`
Expected: FAIL — the skin dir doesn't exist.

- [ ] **Step 3: Create the skin**

`crates/carapace-demo/skins/sysmon/skin.toml`:

```toml
schema = 1
id = "sysmon"
name = "System Monitor"
engine = "^0.1"
canvas = { width = 300, height = 140 }
entry = "skin.lua"
```

`crates/carapace-demo/skins/sysmon/skin.lua`:

```lua
-- shaped, draggable backdrop
fill{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14}, color = {r=18, g=22, b=30} }
region{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14},
        on_press = function() host.begin_drag() end }
-- minimize / close
text{ text = "_", x = 270, y = 4, size = 16, color = {r=200,g=200,b=210} }
region{ path = rect{x=266, y=4, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 286, y = 4, size = 16, color = {r=230,g=140,b=140} }
region{ path = rect{x=282, y=4, w=14, h=16}, on_press = function() host.close() end }
-- live metrics, each a one-line gauge extension
gauge{ x = 20,  y = 24, value = "cpu",  label = "CPU" }
gauge{ x = 90,  y = 24, value = "mem",  label = "MEM" }
gauge{ x = 160, y = 24, value = "swap", label = "SWP" }
```

- [ ] **Step 4: Run the test + fmt + clippy + commit**

```bash
cargo test -p carapace-demo --test skins_build sysmon_skin_builds_gauges
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/skins/sysmon
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): system-monitor skin (cpu/mem/swap gauges)"
```

---

## Task 8: Live host-switch (`H` key) + cross-domain test

**Files:**
- Modify: `crates/carapace-demo/src/main.rs` (registry unions `gauge`; per-domain skin lists; `H`-key `SwitchHost`)
- Create: `crates/carapace-demo/tests/host_switch.rs`

**Interfaces:**
- Consumes: `SysmonHost` (Task 5), `GaugePrim` (Task 6), the sysmon skin (Task 7), `Command::SwitchHost`.

- [ ] **Step 1: Write the failing cross-domain test**

Create `crates/carapace-demo/tests/host_switch.rs`:

```rust
// Proves one Engine runs two domains: media -> SwitchHost(sysmon) on the same instance.
use std::path::Path;
use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::Engine;
use carapace::scene::{FillDir, Node};
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;
use carapace_demo::gauge::GaugePrim;
use carapace_demo::sysmon_host::SysmonHost;
use carapace_demo::transport::TransportPrim;

fn src(dir: &str) -> SkinSource {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins").join(dir);
    carapace::skin::load_dir(&p).unwrap().1
}

#[test]
fn one_engine_switches_media_to_system_monitor() {
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(TransportPrim));
    reg.register(Box::new(GaugePrim));
    let mut e = Engine::new(Box::new(DemoHost::new()), reg, src("classic")).unwrap();

    // Live-switch the whole domain on the same engine instance.
    e.handle_command(Command::SwitchHost {
        host: Box::new(SysmonHost::new()),
        skin: src("sysmon"),
    });
    e.update(Duration::from_millis(200)); // applies the switch + ticks the sysmon host

    assert!(
        e.scene().nodes.iter().any(|n| matches!(n, Node::ValueFill { direction: FillDir::Up, .. })),
        "sysmon scene has vertical gauges"
    );
    match e.state("cpu") {
        Some(StateValue::Scalar(v)) => assert!((0.0..=1.0).contains(&v)),
        other => panic!("cpu should be a unit Scalar on the sysmon host, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run to verify RED**

Run: `cargo test -p carapace-demo --test host_switch`
Expected: FAIL to compile — `GaugePrim`/`SysmonHost` paths exist (Tasks 5–6) so it should compile; it FAILS only if `SwitchHost` drain doesn't rebuild. If it already passes, that's fine — this test is the cross-domain proof and the `H`-key wiring below is the human-facing half.

- [ ] **Step 3: Register `gauge` + per-domain skin lists + the `H` key in `main.rs`**

In `crates/carapace-demo/src/main.rs`:
- Add `GaugePrim` to `demo_registry()`:
  ```rust
  fn demo_registry() -> VocabRegistry {
      let mut r = VocabRegistry::base();
      r.register(Box::new(carapace_demo::transport::TransportPrim));
      r.register(Box::new(carapace_demo::gauge::GaugePrim));
      r
  }
  ```
- Replace the single `SKINS` const with per-domain lists and a domain flag on `App`:
  ```rust
  const MEDIA_SKINS: &[&str] = &["skins/classic", "skins/minimal", "skins/reference", "skins/transport"];
  const SYSMON_SKINS: &[&str] = &["skins/sysmon"];
  ```
  Add `sysmon: bool` to `App` (default `false`). Replace `SKINS` uses: the active list is `if self.sysmon { SYSMON_SKINS } else { MEDIA_SKINS }`; `load_source` indexes the active list.
- In the keyboard handler, add an `H` arm that switches host + domain (constructing the new host with the shared outbox):
  ```rust
  Key::Character(c) if c == "h" || c == "H" => {
      self.sysmon = !self.sysmon;
      self.skin_index = 0;
      let list = if self.sysmon { SYSMON_SKINS } else { MEDIA_SKINS };
      let (src, _) = load_source_from(list, 0);
      let host: Box<dyn carapace::host::Host> = if self.sysmon {
          Box::new(SysmonHost::with_outbox(self.window_outbox.clone()))
      } else {
          Box::new(DemoHost::with_outbox(self.window_outbox.clone()))
      };
      self.engine.handle_command(Command::SwitchHost { host, skin: src });
      if let Some(w) = &self.window { w.request_redraw(); }
  }
  ```
  Adapt `load_source`/Tab to take the active list (a small `load_source_from(list, i)` helper). Import `SysmonHost`.

- [ ] **Step 4: Run tests + build + human smoke**

Run: `cargo test -p carapace-demo` and `cargo build -p carapace-demo`.
Then `cargo run -p carapace-demo`: **Tab** cycles the media skins; press **`H`** and the floating window becomes the live system monitor (cpu/mem/swap bars moving); **`H`** again returns to the media player — all on one engine.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt
cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/main.rs crates/carapace-demo/tests/host_switch.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): H-key live host-switch between media and system monitor"
```

---

## Task 9: README + roadmap (Phases 0–6 complete)

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the roadmap + status**

In `README.md`:
- Mark Phase 6 done and note the project's founding theses are demonstrated:
  ```markdown
  - **Phase 6 — skin-as-window + cross-domain validation.** ✅ The demo renders the skin *as* a
    borderless, transparent, shaped, draggable window (skin-drawn minimize/close), and the **H** key
    live-switches the whole window between a media player and a real `sysinfo` system monitor on one
    engine — proving total window replacement **and** zero domain knowledge (the only engine change
    is a transparent render base color). **Phases 0–6 complete.**
  ```
- Update the `crates/carapace-demo` table row and the intro status callout to say the demo is a
  borderless dual-domain embedder (media player + real system monitor), switchable with `H`.

- [ ] **Step 2: Verify suite + fmt + clippy**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --locked --workspace --all-targets -- -D warnings`
Expected: PASS / clean. (GPU suite separately: `cargo test -p carapace --features gpu-tests --test render_offscreen`.)

- [ ] **Step 3: Commit**

```bash
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs: README current through Phase 6 (project core complete)"
```

---

## Self-Review (completed during planning)

**Spec coverage:**
- Transparent `base_color` (engine, the only `src/` change) → Task 1. ✅
- Borderless/transparent host + alpha-mode selection → Task 3. ✅
- Window controls (host actions → `WindowOutbox` → App drains) → Tasks 2 (mechanism), 3 (drain). ✅
- Skin self-shaping + min/close glyphs → Task 4. ✅
- `SysmonHost` (sysinfo) → Task 5; `gauge{}` → Task 6; sysmon skin → Task 7. ✅
- Live `H`-key host-switch + one registry → Task 8. ✅
- Cross-domain test on one Engine; transparent-base GPU test; sysmon range test; window-op test → Tasks 1,2,5,8. ✅
- README/Phase 6 → Task 9. ✅
- Neutrality proof (engine src change = render.rs only) → Global Constraints + verified at final review.

**Deferred (per spec, no task):** alpha-shaping the Headspace bitmap (stays rectangular); per-pixel click-through; non-macOS transparency guarantees.

**Type consistency:** `RenderTarget.base_color: scene::Color`; `WindowOutbox = Rc<RefCell<Vec<WindowOp>>>`, `WINDOW_ACTIONS: &[ActionSpec]`, `handle_window_action(&str,&WindowOutbox)->bool`; `DemoHost::with_outbox`/`SysmonHost::with_outbox`; `GaugePrim`/`TransportPrim` emit base `Node`s with the real `Node::Text`/`ValueFill` shapes. Consistent.

**GUI caveat:** Tasks 3 and 8's window/host-switch wiring is verified by compile + human smoke (windowing can't be unit-tested); the *testable* cores (window-op recording, cross-domain switch on a headless Engine, sysmon ranges, gauge build) are covered by Tasks 2/5/6/8 tests.

**Engine-untouched checkpoint:** only Task 1 touches `crates/carapace/`. Every other task is `carapace-demo` (+ README). If any later task needs an engine change, STOP and surface it — it would dent the neutrality proof.

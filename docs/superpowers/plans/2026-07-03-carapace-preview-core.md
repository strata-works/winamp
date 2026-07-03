# carapace-preview (core live previewer) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `carapace-preview <skin-dir>` — a native dev tool that renders a carapace skin live in a browser tab with hot-reload, click-to-interact, an editable host-data panel, and continuous animation.

**Architecture:** A new `crates/carapace-preview/` binary crate drives the **public** `carapace` API on a single thread (the engine is `!Send` — it holds `Rc`/`RefCell`/`mlua::Lua`). That main/engine thread owns the `Engine`, a headless wgpu device, an offscreen `Rgba8Unorm` render target, a `notify` file-watcher feed, and a render loop. It renders each frame offscreen, reads it back to RGBA, PNG-encodes it, and pushes it over a WebSocket to a thin browser viewer. Two server threads (a `tiny_http` static-page server and a `tungstenite` WebSocket acceptor) talk to the engine thread only through `std::sync::mpsc` channels, so nothing `!Send` ever crosses a thread boundary.

**Tech Stack:** Rust (edition 2024), `carapace` (path dep, the engine), `wgpu` 29 + `pollster` (headless GPU), `image` 0.25 (PNG encode), `tiny_http` (serve the viewer page), `tungstenite` (WebSocket), `notify` (file watch), `serde`/`serde_json` (wire protocol). Single-file browser viewer (`assets/index.html`, inline JS/CSS).

## Global Constraints

- **Edition:** `edition = "2024"` (repo standard — `rustfmt.toml` + all non-spike crates). Copy `carapace-ffi/Cargo.toml`'s inline comment convention.
- **Zero engine-crate diff.** Do not modify `crates/carapace/` (or any existing crate) except appending the new member to the workspace `Cargo.toml`. Everything is built on the public `carapace` API.
- **Dependency fetch policy:** the first fetch of any new third-party crate must go through Socket Firewall — use `sfw cargo add <crate>` (never a bare `cargo add`). Already-vendored crates (`image`, `wgpu`, `pollster`) are pinned to the versions already in `Cargo.lock` to reuse the resolved copies: `image = "0.25.10"`, `wgpu = "29.0.3"`, `pollster = "0.4.0"`.
- **Engine is single-threaded / `!Send`.** The `Engine`, `PreviewHost`, `GpuCtx`, `Renderer`, and offscreen target live only on the main thread. Server/watcher threads communicate via channels carrying only `Send` data (bytes, strings, numbers, `mpsc::Sender`s).
- **wgpu is version-29 era.** Use the exact API names: `TexelCopyTextureInfo` / `TexelCopyBufferInfo` / `TexelCopyBufferLayout`, `PollType::wait_indefinitely()`, `device.poll(...)`, `RequestAdapterOptions { compatible_surface: None, .. }`. `required_limits: adapter.limits()` (not defaults).
- **Commit after every task.** Use the repo git identity (Daniel Agbemava <danagbemava@gmail.com>) — it is already the configured default; do not pass `--author`.
- **Before pushing / finishing:** `cargo clippy -p carapace-preview --all-targets -- -D warnings` and `cargo fmt` must pass — CI gates on clippy `-D warnings` (including a gpu-tests variant).

## Verified facts this plan is built on (from the carapace public API)

- `Host` trait (`carapace::host::Host`): `fn name(&self) -> &str`, `fn tick(&mut self, dt: Duration)`, `fn get(&self, key: &str) -> Option<StateValue>`, `fn actions(&self) -> &[ActionSpec]`, `fn invoke(&mut self, action: &str, args: &[Value])`, and a defaulted `fn rows(&self, _: &str) -> Vec<Row>`.
- `ActionSpec { pub name: &'static str }` (`Copy`). `Value::{Num(f64), Bool(bool), Str(String)}`.
- `StateValue::{Bool(bool), Scalar(f32), Str(Arc<str>)}` — numbers are `Scalar(f32)`, strings are `Str(Arc<str>)`. No `f64` variant, no `From` impls.
- Loading: `carapace::skin::load_dir(dir: &Path) -> Result<(Manifest, SkinSource), SkinError>`. `Manifest.canvas.{width,height}: u32`, `Manifest.name: String`.
- `Engine::new(host: Box<dyn Host>, registry: VocabRegistry, initial: SkinSource) -> Result<Engine, ScriptError>`. Registry via `carapace::vocab::VocabRegistry::base()`. **`Engine::new` runs the skin's Lua** and validates `host.<name>` calls against `host.actions()` — so the action allowlist must be populated in the `PreviewHost` *before* `Engine::new`.
- `engine.update(dt: Duration)`, `engine.layout(w: f32, h: f32) -> Scene`, `engine.state(key: &str) -> Option<StateValue>`, `engine.handle_pointer_resolved(w: f32, h: f32, p: Pt, kind: PointerEvent)`. `Pt { x: f32, y: f32 }`, `PointerEvent::Press`.
- Rendering: `carapace::render::Renderer::new(device: &wgpu::Device)`; `renderer.draw(scene: &Scene, read_value: impl Fn(&str)->Option<StateValue>, view_tex: impl Fn(&str)->Option<&wgpu::TextureView>, target: &RenderTarget)`. `RenderTarget { device, queue, view, width, height, base_color }` (all `pub`; `base_color: carapace::scene::Color { r,g,b,a: u8 }`).
- Skin Lua syntax (for fixtures): `fill{ path = {{x=0,y=0},...}, color = {r=10,g=10,b=10} }`; `region{ path = ..., on_press = function() host.begin_drag() end }`; `value_fill{ path = ..., value = "position", color = {...} }`.

## File Structure

```
crates/carapace-preview/
  Cargo.toml
  README.md
  src/
    main.rs           # arg parse; spawn server + watcher threads; run engine loop on main thread; open browser
    protocol.rs       # ClientMsg (browser→engine, serde) + OutMsg (engine→browser) + WS message mapping
    preview_host.rs   # PreviewHost: carapace::host::Host — shared host-value map, action log, scanned allowlist
    render.rs         # headless GpuCtx, offscreen Rgba8 target, render_frame (public carapace API), readback, PNG, frame hash
    skin_session.rs   # SkinSession: load_dir → Engine, reload on watch, capture load errors (non-fatal)
    server.rs         # tiny_http static page + tungstenite WS acceptor; per-connection duplex; client registry
  assets/
    index.html        # single-file viewer (canvas + data panel + action log + error banner)
  tests/
    fixtures/         # (only if needed) — otherwise tests reference existing repo skins
```

Engine-thread ↔ server-thread channels:
- `EngineMsg` (server/watcher → engine, `Send`): `ClientConnected(mpsc::Sender<OutMsg>)`, `Client(ClientMsg)`, `Reload`.
- `OutMsg` (engine → each connected client, `Send`): `Frame(Vec<u8>)`, `Meta{..}`, `ActionLog{..}`, `Error{..}`.

Each browser connection owns an `mpsc::Receiver<OutMsg>`; the engine thread holds a `Vec<Sender<OutMsg>>` and broadcasts by `retain(|tx| tx.send(msg.clone()).is_ok())`, which prunes dead clients automatically (no explicit disconnect message).

---

### Task 1: Crate scaffold + workspace wiring + dependencies

**Files:**
- Create: `crates/carapace-preview/Cargo.toml`
- Create: `crates/carapace-preview/src/main.rs`
- Modify: `Cargo.toml` (workspace root — append member)

**Interfaces:**
- Produces: a buildable `carapace-preview` bin crate with all deps resolved, and a `main` that prints a usage line and exits non-zero when no `<skin-dir>` arg is given.

- [ ] **Step 1: Append the workspace member**

Edit the root `/Users/nexus/projects/experiments/winamp/Cargo.toml` `members` array to add `"crates/carapace-preview"`:

```toml
[workspace]
members = ["crates/hittest", "crates/carapace", "crates/carapace-demo", "crates/window-spike", "crates/embed-spike", "crates/carapace-ffi", "crates/carapace-preview"]
resolver = "2"
```

- [ ] **Step 2: Write the crate manifest (vendored deps only for now)**

Create `crates/carapace-preview/Cargo.toml`:

```toml
[package]
name = "carapace-preview"
version = "0.1.0"
edition = "2024"   # repo standard (rustfmt.toml + all non-spike crates)
description = "Live, interactive browser previewer for carapace skins (dev tool)."
publish = false

[[bin]]
name = "carapace-preview"
path = "src/main.rs"

[dependencies]
carapace = { path = "../carapace" }
wgpu = "29.0.3"
pollster = "0.4.0"
image = "0.25.10"
# new third-party deps are added via `sfw cargo add` in Step 4 (notify, tiny_http, tungstenite, serde, serde_json)
```

- [ ] **Step 3: Write a minimal `main.rs`**

Create `crates/carapace-preview/src/main.rs`:

```rust
//! carapace-preview — a live, interactive browser previewer for carapace skins.
//! See docs/superpowers/specs/2026-07-01-carapace-preview-design.md.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(skin_dir) = args.next() else {
        eprintln!("usage: carapace-preview <skin-dir> [--port <n>]");
        return ExitCode::FAILURE;
    };
    println!("carapace-preview: {skin_dir}");
    ExitCode::SUCCESS
}
```

- [ ] **Step 4: Add the new third-party deps through Socket Firewall**

Run (from repo root):

```bash
sfw cargo add -p carapace-preview notify
sfw cargo add -p carapace-preview tiny_http
sfw cargo add -p carapace-preview tungstenite
sfw cargo add -p carapace-preview serde --features derive
sfw cargo add -p carapace-preview serde_json
```

Expected: each resolves and is appended to `crates/carapace-preview/Cargo.toml` `[dependencies]`. (If `serde`/`serde_json` are already in the lock they still get added to this crate's manifest.) `tungstenite` default features are TLS-free — do not enable `native-tls`/`rustls`.

- [ ] **Step 5: Build and verify usage behavior**

Run:

```bash
cargo build -p carapace-preview
./target/debug/carapace-preview ; echo "exit=$?"
```

Expected: build succeeds; running with no args prints `usage: carapace-preview <skin-dir> [--port <n>]` and `exit=1`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/carapace-preview
git commit -m "feat(preview): scaffold carapace-preview crate + deps"
```

---

### Task 2: Wire protocol (`protocol.rs`)

**Files:**
- Create: `crates/carapace-preview/src/protocol.rs`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod protocol;`)

**Interfaces:**
- Consumes: `serde`, `serde_json`, `tungstenite`.
- Produces:
  - `pub enum ClientMsg { Pointer{x:f32,y:f32}, SetValue{key:String, value:serde_json::Value}, SetCanvas{w:u32,h:u32} }` (`Deserialize`, tagged by `"type"`, camelCase).
  - `pub fn parse_client_msg(text: &str) -> Result<ClientMsg, serde_json::Error>`.
  - `pub enum OutMsg { Frame(Vec<u8>), Meta{name:String,w:u32,h:u32}, ActionLog{action:String}, Error{message:Option<String>} }` (`Clone`).
  - `pub fn out_to_ws(msg: &OutMsg) -> tungstenite::Message`.

- [ ] **Step 1: Write the failing test**

Create `crates/carapace-preview/src/protocol.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pointer() {
        let m = parse_client_msg(r#"{"type":"pointer","x":12.5,"y":7.0}"#).unwrap();
        assert!(matches!(m, ClientMsg::Pointer { x, y } if x == 12.5 && y == 7.0));
    }

    #[test]
    fn parses_set_value_number_and_string() {
        let n = parse_client_msg(r#"{"type":"setValue","key":"level","value":0.4}"#).unwrap();
        assert!(matches!(n, ClientMsg::SetValue { ref key, value: serde_json::Value::Number(_) } if key == "level"));
        let s = parse_client_msg(r#"{"type":"setValue","key":"track","value":"Song"}"#).unwrap();
        assert!(matches!(s, ClientMsg::SetValue { value: serde_json::Value::String(_), .. }));
    }

    #[test]
    fn parses_set_canvas() {
        let m = parse_client_msg(r#"{"type":"setCanvas","w":320,"h":200}"#).unwrap();
        assert!(matches!(m, ClientMsg::SetCanvas { w: 320, h: 200 }));
    }

    #[test]
    fn frame_maps_to_binary_others_to_text() {
        let f = out_to_ws(&OutMsg::Frame(vec![1, 2, 3]));
        assert!(f.is_binary());
        let meta = out_to_ws(&OutMsg::Meta { name: "S".into(), w: 300, h: 120 });
        assert!(meta.is_text());
        assert!(meta.into_text().unwrap().contains("\"type\":\"meta\""));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p carapace-preview protocol`
Expected: FAIL — `ClientMsg` / `parse_client_msg` / `out_to_ws` / `OutMsg` not found.

- [ ] **Step 3: Write the implementation (above the tests module)**

Prepend to `crates/carapace-preview/src/protocol.rs`:

```rust
use serde::Deserialize;
use serde_json::json;

/// Messages the browser viewer sends up to the engine thread.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMsg {
    Pointer { x: f32, y: f32 },
    SetValue { key: String, value: serde_json::Value },
    SetCanvas { w: u32, h: u32 },
}

pub fn parse_client_msg(text: &str) -> Result<ClientMsg, serde_json::Error> {
    serde_json::from_str(text)
}

/// Messages the engine thread broadcasts down to each connected browser client.
#[derive(Debug, Clone)]
pub enum OutMsg {
    Frame(Vec<u8>), // PNG-encoded RGBA
    Meta { name: String, w: u32, h: u32 },
    ActionLog { action: String },
    Error { message: Option<String> },
}

pub fn out_to_ws(msg: &OutMsg) -> tungstenite::Message {
    match msg {
        OutMsg::Frame(bytes) => tungstenite::Message::binary(bytes.clone()),
        OutMsg::Meta { name, w, h } => {
            tungstenite::Message::text(json!({"type":"meta","name":name,"w":w,"h":h}).to_string())
        }
        OutMsg::ActionLog { action } => {
            tungstenite::Message::text(json!({"type":"actionLog","action":action}).to_string())
        }
        OutMsg::Error { message } => {
            tungstenite::Message::text(json!({"type":"error","message":message}).to_string())
        }
    }
}
```

Add `mod protocol;` to `src/main.rs` (below the doc-comment).

> Note: `Message::binary`/`Message::text` are the version-stable constructors (accept `Into<Bytes>`/`Into<Utf8Bytes>` or `Into<Vec<u8>>`/`Into<String>` depending on the resolved tungstenite). If the build complains about `is_binary`/`is_text`/`into_text`, adjust the test assertions to the resolved tungstenite `Message` API — the constructors themselves are stable.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace-preview protocol`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/protocol.rs crates/carapace-preview/src/main.rs
git commit -m "feat(preview): wire protocol (ClientMsg/OutMsg + WS mapping)"
```

---

### Task 3: `PreviewHost` + action source-scan (`preview_host.rs`)

**Files:**
- Create: `crates/carapace-preview/src/preview_host.rs`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod preview_host;`)

**Interfaces:**
- Consumes: `carapace::host::{Host, StateValue, ActionSpec, Value}`.
- Produces:
  - `pub type Values = std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, StateValue>>>`.
  - `pub type ActionLog = std::rc::Rc<std::cell::RefCell<Vec<String>>>`.
  - `pub fn scan_actions(lua_src: &str) -> Vec<&'static str>` — deduped `host.<ident>` names, each `Box::leak`ed to `&'static str`.
  - `pub struct PreviewHost { .. }` with `pub fn new(values: Values, log: ActionLog, actions: Vec<ActionSpec>) -> Self` implementing `carapace::host::Host`.

- [ ] **Step 1: Write the failing tests**

Create `crates/carapace-preview/src/preview_host.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use carapace::host::{Host, StateValue, Value};
    use std::sync::Arc;

    const MINIMAL_SRC: &str = r#"
        region{ on_press = function() host.begin_drag() end }
        region{ on_press = function() host.minimize() end }
        region{ on_press = function() host.close() end }
        region{ on_press = function() host.toggle_play() end }
        region{ on_press = function() host.toggle_play() end }  -- duplicate
    "#;

    #[test]
    fn scan_finds_deduped_action_names_including_in_closures() {
        let mut got = scan_actions(MINIMAL_SRC);
        got.sort_unstable();
        assert_eq!(got, vec!["begin_drag", "close", "minimize", "toggle_play"]);
    }

    #[test]
    fn get_reads_the_shared_value_map() {
        let values: Values = Default::default();
        values.borrow_mut().insert("level".into(), StateValue::Scalar(0.5));
        let host = PreviewHost::new(values, Default::default(), Vec::new());
        assert_eq!(host.get("level"), Some(StateValue::Scalar(0.5)));
        assert_eq!(host.get("missing"), None);
    }

    #[test]
    fn invoke_appends_to_the_action_log() {
        let log: ActionLog = Default::default();
        let mut host = PreviewHost::new(Default::default(), log.clone(), Vec::new());
        host.invoke("toggle_play", &[Value::Num(1.0)]);
        host.invoke("close", &[]);
        assert_eq!(*log.borrow(), vec!["toggle_play".to_string(), "close".to_string()]);
    }

    #[test]
    fn actions_reports_scanned_allowlist() {
        let specs: Vec<carapace::host::ActionSpec> =
            scan_actions("host.play() host.stop()").into_iter()
                .map(|name| carapace::host::ActionSpec { name })
                .collect();
        let host = PreviewHost::new(Default::default(), Default::default(), specs);
        let names: Vec<&str> = host.actions().iter().map(|a| a.name).collect();
        assert!(names.contains(&"play") && names.contains(&"stop"));
        let _ = Arc::new(()); // silence unused import if trimmed
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview preview_host`
Expected: FAIL — `scan_actions` / `PreviewHost` / `Values` / `ActionLog` not found.

- [ ] **Step 3: Write the implementation (above the tests module)**

Prepend to `crates/carapace-preview/src/preview_host.rs`:

```rust
use carapace::host::{ActionSpec, Host, StateValue, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

/// Host-value map the browser data panel drives; shared with the engine thread.
pub type Values = Rc<RefCell<HashMap<String, StateValue>>>;
/// Action-invocation log; drained + broadcast by the engine loop each tick.
pub type ActionLog = Rc<RefCell<Vec<String>>>;

/// Scan skin source for every `host.<ident>` call, dedupe, and leak each name to
/// `&'static str` (required because `ActionSpec.name` is `&'static str`). A handful
/// of leaked strings per reload is acceptable for a dev tool.
pub fn scan_actions(lua_src: &str) -> Vec<&'static str> {
    let mut seen: HashSet<String> = HashSet::new();
    let bytes = lua_src.as_bytes();
    let needle = b"host.";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let mut j = i + needle.len();
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + needle.len() {
                seen.insert(lua_src[i + needle.len()..j].to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    seen.into_iter()
        .map(|s| &*Box::leak(s.into_boxed_str()))
        .collect()
}

/// A `carapace::host::Host` for the previewer: values come from the browser panel,
/// invoked actions are logged (never mutate values — the panel owns those).
pub struct PreviewHost {
    values: Values,
    log: ActionLog,
    actions: Vec<ActionSpec>,
}

impl PreviewHost {
    pub fn new(values: Values, log: ActionLog, actions: Vec<ActionSpec>) -> Self {
        Self { values, log, actions }
    }
}

impl Host for PreviewHost {
    fn name(&self) -> &str {
        "preview"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        self.values.borrow().get(key).cloned()
    }
    fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        self.log.borrow_mut().push(action.to_string());
    }
}
```

Add `mod preview_host;` to `src/main.rs`. (Remove the `use std::sync::Arc;` / dummy line from the test if clippy flags it.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview preview_host`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/preview_host.rs crates/carapace-preview/src/main.rs
git commit -m "feat(preview): PreviewHost + host.<action> source-scan allowlist"
```

---

### Task 4: Offscreen render context + PNG + frame hashing (`render.rs`)

**Files:**
- Create: `crates/carapace-preview/src/render.rs`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod render;`)

**Interfaces:**
- Consumes: `wgpu`, `pollster`, `image`, `carapace::render::{Renderer, RenderTarget}`, `carapace::engine::Engine`, `carapace::scene::Color`.
- Produces:
  - `pub struct GpuCtx { pub device: wgpu::Device, pub queue: wgpu::Queue }` + `pub fn init_gpu() -> GpuCtx`.
  - `pub struct Offscreen { pub tex: wgpu::Texture, pub view: wgpu::TextureView, pub w: u32, pub h: u32 }` + `pub fn new_offscreen(device:&wgpu::Device, w:u32, h:u32) -> Offscreen`.
  - `pub fn render_rgba(engine:&mut Engine, renderer:&mut Renderer, gpu:&GpuCtx, off:&Offscreen, dt:Duration) -> Vec<u8>` — update→layout→draw→readback, returns tightly-packed RGBA (`w*h*4`).
  - `pub fn encode_png(rgba:&[u8], w:u32, h:u32) -> Vec<u8>`.
  - `pub fn frame_hash(rgba:&[u8]) -> u64`.

- [ ] **Step 1: Write the failing tests**

Create `crates/carapace-preview/src/render.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    // The canonical minimal render fixture that ships with the engine crate.
    const OK_SKIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../carapace/tests/skins/ok");

    fn load_ok_engine() -> (carapace::engine::Engine, u32, u32) {
        let (manifest, source) =
            carapace::skin::load_dir(Path::new(OK_SKIN)).expect("load ok skin");
        let host: Box<dyn carapace::host::Host> =
            Box::new(crate::preview_host::PreviewHost::new(
                Default::default(),
                Default::default(),
                Vec::new(),
            ));
        let engine = carapace::engine::Engine::new(
            host,
            carapace::vocab::VocabRegistry::base(),
            source,
        )
        .expect("engine");
        (engine, manifest.canvas.width, manifest.canvas.height)
    }

    #[test]
    fn renders_a_nonempty_frame_of_expected_dims() {
        let (mut engine, w, h) = load_ok_engine();
        let gpu = init_gpu();
        let mut renderer = carapace::render::Renderer::new(&gpu.device);
        let off = new_offscreen(&gpu.device, w, h);
        let rgba = render_rgba(&mut engine, &mut renderer, &gpu, &off, Duration::ZERO);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // The "ok" skin fills a dark triangle over transparent — at least one pixel is opaque.
        assert!(rgba.chunks_exact(4).any(|p| p[3] > 0), "expected some opaque pixels");
    }

    #[test]
    fn png_round_trips() {
        let w = 2;
        let h = 2;
        let rgba = vec![255u8; (w * h * 4) as usize];
        let png = encode_png(&rgba, w, h);
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (w, h));
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        let a = vec![0u8, 1, 2, 3];
        let b = vec![0u8, 1, 2, 3];
        let c = vec![9u8, 1, 2, 3];
        assert_eq!(frame_hash(&a), frame_hash(&b));
        assert_ne!(frame_hash(&a), frame_hash(&c));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview render`
Expected: FAIL — `init_gpu` / `new_offscreen` / `render_rgba` / `encode_png` / `frame_hash` not found.

- [ ] **Step 3: Write the implementation (above the tests module)**

Prepend to `crates/carapace-preview/src/render.rs` (mirrors the proven `embed-spike` offscreen path against the public API):

```rust
use carapace::engine::Engine;
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Color;
use std::hash::{Hash, Hasher};
use std::time::Duration;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct GpuCtx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Headless GPU — no surface; we render into our own textures.
pub fn init_gpu() -> GpuCtx {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no wgpu adapter — carapace-preview needs a GPU");
    let mut required_features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::BGRA8UNORM_STORAGE)
    {
        required_features |= wgpu::Features::BGRA8UNORM_STORAGE;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("wgpu device");
    GpuCtx { device, queue }
}

pub struct Offscreen {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

pub fn new_offscreen(device: &wgpu::Device, w: u32, h: u32) -> Offscreen {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("carapace-preview-offscreen"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Offscreen { tex, view, w, h }
}

/// update → layout(at target size) → draw → readback. Lays out at the offscreen
/// size so resizable skins reflow; transparent base preserves skin alpha.
pub fn render_rgba(
    engine: &mut Engine,
    renderer: &mut Renderer,
    gpu: &GpuCtx,
    off: &Offscreen,
    dt: Duration,
) -> Vec<u8> {
    engine.update(dt);
    let scene = engine.layout(off.w as f32, off.h as f32);
    let no_views = |_id: &str| None;
    renderer.draw(
        &scene,
        |k| engine.state(k),
        no_views,
        &RenderTarget {
            device: &gpu.device,
            queue: &gpu.queue,
            view: &off.view,
            width: off.w,
            height: off.h,
            base_color: Color { r: 0, g: 0, b: 0, a: 0 },
        },
    );
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    readback_rgba(gpu, &off.tex, off.w, off.h)
}

/// Copy an RGBA8 texture back to CPU, returning tightly-packed rows (no padding).
fn readback_rgba(gpu: &GpuCtx, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let bpp = 4u32;
    let unpadded = w * bpp;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;

    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([enc.finish()]);

    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range();

    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    buf.unmap();
    out
}

pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .expect("rgba buffer matches w*h*4");
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("png encode");
    buf.into_inner()
}

pub fn frame_hash(rgba: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rgba.hash(&mut hasher);
    hasher.finish()
}
```

Add `mod render;` to `src/main.rs`. Note `render.rs` references `crate::preview_host::PreviewHost` only in its test — ensure Task 3 is committed first.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview render`
Expected: PASS (3 tests). The render test needs a GPU adapter; it runs under the repo's gpu-tests CI variant (same as `embed-spike`'s render tests). If your local machine has no adapter, the test panics at `init_gpu` with a clear message — that is expected off-GPU.

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/render.rs crates/carapace-preview/src/main.rs
git commit -m "feat(preview): headless offscreen render + PNG encode + frame hash"
```

---

### Task 5: Skin session — load, watch, reload (`skin_session.rs`)

**Files:**
- Create: `crates/carapace-preview/src/skin_session.rs`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod skin_session;`)

**Interfaces:**
- Consumes: `carapace::{skin, engine, vocab}`, `preview_host::{PreviewHost, Values, ActionLog, scan_actions}`, `carapace::host::ActionSpec`.
- Produces:
  - `pub struct SkinSession { pub dir: PathBuf, pub engine: Option<Engine>, pub name: String, pub canvas: (u32,u32), pub last_error: Option<String>, values: Values, log: ActionLog }`.
  - `pub fn new(dir: PathBuf, values: Values, log: ActionLog) -> SkinSession` — attempts an initial load (errors captured, not fatal).
  - `pub fn reload(&mut self)` — rebuild the `Engine` sharing the same `values`/`log`; on failure set `last_error`, keep the previous engine.
  - `pub fn load_result(&self) -> Result<(),String>` — current error state as a Result for broadcasting.

- [ ] **Step 1: Write the failing tests**

Create `crates/carapace-preview/src/skin_session.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp_skin(lua: &str) -> PathBuf {
        // Unique dir under the crate target dir; no external tempfile dep.
        let base = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "skin_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("skin.toml"),
            "schema = 1\nid = \"t\"\nname = \"Temp\"\nengine = \"^0.1\"\ncanvas = { width = 100, height = 60 }\nentry = \"skin.lua\"\n",
        )
        .unwrap();
        fs::write(base.join("skin.lua"), lua).unwrap();
        base
    }

    #[test]
    fn loads_a_valid_skin() {
        let dir = tmp_skin("fill{ path = {{x=0,y=0},{x=100,y=0},{x=100,y=60}}, color = {r=1,g=2,b=3} }");
        let s = SkinSession::new(dir, Default::default(), Default::default());
        assert!(s.engine.is_some());
        assert!(s.last_error.is_none());
        assert_eq!(s.canvas, (100, 60));
        assert_eq!(s.name, "Temp");
    }

    #[test]
    fn broken_lua_is_captured_not_fatal_and_keeps_last_good() {
        let dir = tmp_skin("fill{ path = {{x=0,y=0},{x=100,y=0},{x=100,y=60}}, color = {r=1,g=2,b=3} }");
        let mut s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        assert!(s.engine.is_some());
        // Overwrite with a syntax error and reload.
        std::fs::write(dir.join("skin.lua"), "fill{ this is not lua ")
            .unwrap();
        s.reload();
        assert!(s.last_error.is_some(), "error should be captured");
        assert!(s.engine.is_some(), "last-good engine should survive");
    }

    #[test]
    fn reload_picks_up_a_valid_change() {
        let dir = tmp_skin("fill{ path = {{x=0,y=0},{x=100,y=0},{x=100,y=60}}, color = {r=1,g=2,b=3} }");
        let mut s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        std::fs::write(
            dir.join("skin.lua"),
            "fill{ path = {{x=0,y=0},{x=50,y=0},{x=50,y=30}}, color = {r=9,g=9,b=9} }",
        )
        .unwrap();
        s.reload();
        assert!(s.last_error.is_none());
        assert!(s.engine.is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace-preview skin_session`
Expected: FAIL — `SkinSession` not found.

- [ ] **Step 3: Write the implementation (above the tests module)**

Prepend to `crates/carapace-preview/src/skin_session.rs`:

```rust
use crate::preview_host::{scan_actions, ActionLog, PreviewHost, Values};
use carapace::engine::Engine;
use carapace::host::{ActionSpec, Host};
use carapace::vocab::VocabRegistry;
use std::path::{Path, PathBuf};

pub struct SkinSession {
    pub dir: PathBuf,
    pub engine: Option<Engine>,
    pub name: String,
    pub canvas: (u32, u32),
    pub last_error: Option<String>,
    values: Values,
    log: ActionLog,
}

/// Build an `Engine` for `dir`, scanning the source for the host-action allowlist first.
fn build_engine(
    dir: &Path,
    values: &Values,
    log: &ActionLog,
) -> Result<(Engine, String, (u32, u32)), String> {
    let (manifest, source) = carapace::skin::load_dir(dir).map_err(|e| format!("{e:?}"))?;
    let actions: Vec<ActionSpec> = scan_actions(&source.lua_src)
        .into_iter()
        .map(|name| ActionSpec { name })
        .collect();
    let host: Box<dyn Host> = Box::new(PreviewHost::new(values.clone(), log.clone(), actions));
    let engine = Engine::new(host, VocabRegistry::base(), source).map_err(|e| format!("{e:?}"))?;
    Ok((
        engine,
        manifest.name,
        (manifest.canvas.width, manifest.canvas.height),
    ))
}

impl SkinSession {
    pub fn new(dir: PathBuf, values: Values, log: ActionLog) -> SkinSession {
        let mut s = SkinSession {
            dir,
            engine: None,
            name: String::new(),
            canvas: (0, 0),
            last_error: None,
            values,
            log,
        };
        s.reload();
        s
    }

    pub fn reload(&mut self) {
        match build_engine(&self.dir, &self.values, &self.log) {
            Ok((engine, name, canvas)) => {
                self.engine = Some(engine);
                self.name = name;
                self.canvas = canvas;
                self.last_error = None;
            }
            Err(e) => {
                // Keep the last-good engine up; surface the error.
                self.last_error = Some(e);
            }
        }
    }

    pub fn load_result(&self) -> Result<(), String> {
        match &self.last_error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}
```

Add `mod skin_session;` to `src/main.rs`.

> Note: `source.lua_src` is a public field of `SkinSource` (`carapace::command::SkinSource { pub lua_src: String, pub canvas: (u32,u32), pub assets: Rc<AssetResolver> }`), so the scan reads the exact loaded source.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p carapace-preview skin_session`
Expected: PASS (3 tests). (These build the `Engine`/run Lua but need no GPU.)

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-preview/src/skin_session.rs crates/carapace-preview/src/main.rs
git commit -m "feat(preview): skin session load + reload with captured Lua errors"
```

---

### Task 6: Server — static page + WebSocket duplex (`server.rs` + `assets/index.html`)

**Files:**
- Create: `crates/carapace-preview/src/server.rs`
- Create: `crates/carapace-preview/assets/index.html`
- Modify: `crates/carapace-preview/src/main.rs` (add `mod server;`)

**Interfaces:**
- Consumes: `tiny_http`, `tungstenite`, `protocol::{ClientMsg, OutMsg, parse_client_msg, out_to_ws}`.
- Produces:
  - `pub enum EngineMsg { ClientConnected(std::sync::mpsc::Sender<OutMsg>), Client(ClientMsg), Reload }`.
  - `pub struct Ports { pub http: u16, pub ws: u16 }`.
  - `pub fn serve(http_port: u16, engine_tx: std::sync::mpsc::Sender<EngineMsg>) -> Ports` — binds an HTTP server (static viewer, with the live WS port templated in) and a WS acceptor on an ephemeral `127.0.0.1` port; spawns their accept loops; returns the bound ports.
  - `pub fn render_index(ws_port: u16) -> String` — the viewer HTML with `{{WS_PORT}}` substituted (unit-tested).

- [ ] **Step 1: Write the failing test (HTML templating — no sockets)**

Create `crates/carapace-preview/src/server.rs` with the templating fn + test first:

```rust
pub fn render_index(ws_port: u16) -> String {
    include_str!("../assets/index.html").replace("{{WS_PORT}}", &ws_port.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_the_ws_port_into_the_page() {
        let html = render_index(54321);
        assert!(html.contains("54321"));
        assert!(!html.contains("{{WS_PORT}}"));
    }
}
```

- [ ] **Step 2: Create a minimal `assets/index.html` so `include_str!` resolves**

Create `crates/carapace-preview/assets/index.html` (full viewer — inline CSS/JS, single file):

```html
<!-- carapace-preview viewer -->
<meta charset="utf-8" />
<title>carapace-preview</title>
<style>
  body { margin: 0; font: 13px system-ui, sans-serif; background: #1b1b1f; color: #ddd; display: flex; }
  #stage { flex: 1; display: flex; flex-direction: column; align-items: center; padding: 16px; gap: 8px; }
  #skinName { font-weight: 600; }
  #canvasWrap { background:
      linear-gradient(45deg,#2a2a2e 25%,transparent 25%,transparent 75%,#2a2a2e 75%),
      linear-gradient(45deg,#2a2a2e 25%,#232327 25%,#232327 75%,#2a2a2e 75%);
    background-size: 20px 20px; background-position: 0 0, 10px 10px; }
  canvas { display: block; }
  #err { display: none; background: #5a1c1c; color: #ffd9d9; padding: 8px 12px; border-radius: 6px; max-width: 640px; white-space: pre-wrap; font-family: ui-monospace, monospace; }
  #panel { width: 280px; background: #222226; padding: 12px; box-sizing: border-box; height: 100vh; overflow: auto; }
  #panel h2 { font-size: 12px; text-transform: uppercase; letter-spacing: .06em; color: #999; margin: 16px 0 6px; }
  .row { display: flex; gap: 6px; margin-bottom: 4px; }
  .row input { flex: 1; min-width: 0; background: #2c2c31; border: 1px solid #3a3a40; color: #eee; padding: 4px 6px; border-radius: 4px; }
  button { background: #33343a; border: 1px solid #45464d; color: #eee; padding: 4px 8px; border-radius: 4px; cursor: pointer; }
  #log { font-family: ui-monospace, monospace; font-size: 12px; background: #17171a; border-radius: 4px; padding: 6px; height: 160px; overflow: auto; }
  #status { color: #888; }
</style>

<div id="stage">
  <div id="skinName">carapace-preview</div>
  <div id="err"></div>
  <div id="canvasWrap"><canvas id="c" width="10" height="10"></canvas></div>
  <div id="status">connecting…</div>
</div>

<div id="panel">
  <h2>Canvas size</h2>
  <div class="row">
    <input id="cw" type="number" placeholder="w" />
    <input id="ch" type="number" placeholder="h" />
    <button id="applyCanvas">Set</button>
  </div>

  <h2>Host data</h2>
  <div id="values"></div>
  <div class="row">
    <input id="newKey" placeholder="key" />
    <input id="newVal" placeholder="value" />
    <button id="addVal">Add</button>
  </div>

  <h2>Action log</h2>
  <div id="log"></div>
</div>

<script>
  const canvas = document.getElementById("c");
  const ctx = canvas.getContext("2d");
  const err = document.getElementById("err");
  const status = document.getElementById("status");
  const logEl = document.getElementById("log");
  const valuesEl = document.getElementById("values");
  let designW = 10, designH = 10;

  const ws = new WebSocket("ws://127.0.0.1:{{WS_PORT}}");
  ws.binaryType = "arraybuffer";

  ws.onopen = () => (status.textContent = "connected");
  ws.onclose = () => {
    status.textContent = "disconnected — retrying…";
    setTimeout(() => location.reload(), 1000);
  };

  ws.onmessage = (ev) => {
    if (ev.data instanceof ArrayBuffer) {
      const blob = new Blob([ev.data], { type: "image/png" });
      createImageBitmap(blob).then((bmp) => {
        ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
        bmp.close();
      });
      return;
    }
    const msg = JSON.parse(ev.data);
    if (msg.type === "meta") {
      document.getElementById("skinName").textContent = msg.name;
      designW = msg.w; designH = msg.h;
      canvas.width = msg.w; canvas.height = msg.h;
      document.getElementById("cw").value = msg.w;
      document.getElementById("ch").value = msg.h;
    } else if (msg.type === "actionLog") {
      const line = document.createElement("div");
      line.textContent = "▶ " + msg.action;
      logEl.appendChild(line);
      logEl.scrollTop = logEl.scrollHeight;
    } else if (msg.type === "error") {
      if (msg.message) { err.style.display = "block"; err.textContent = msg.message; }
      else { err.style.display = "none"; }
    }
  };

  function send(obj) { if (ws.readyState === 1) ws.send(JSON.stringify(obj)); }

  canvas.addEventListener("click", (e) => {
    const r = canvas.getBoundingClientRect();
    const x = (e.clientX - r.left) * (designW / r.width);
    const y = (e.clientY - r.top) * (designH / r.height);
    send({ type: "pointer", x, y });
  });

  document.getElementById("applyCanvas").onclick = () => {
    const w = parseInt(document.getElementById("cw").value, 10);
    const h = parseInt(document.getElementById("ch").value, 10);
    if (w > 0 && h > 0) send({ type: "setCanvas", w, h });
  };

  function addValueRow(key, val) {
    const row = document.createElement("div");
    row.className = "row";
    const k = document.createElement("input"); k.value = key; k.placeholder = "key";
    const v = document.createElement("input"); v.value = val; v.placeholder = "value";
    const push = () => {
      const num = parseFloat(v.value);
      send({ type: "setValue", key: k.value, value: (v.value.trim() !== "" && !isNaN(num) && String(num) === v.value.trim()) ? num : v.value });
    };
    v.addEventListener("input", push);
    k.addEventListener("change", push);
    row.appendChild(k); row.appendChild(v);
    valuesEl.appendChild(row);
  }

  document.getElementById("addVal").onclick = () => {
    const key = document.getElementById("newKey").value.trim();
    if (!key) return;
    addValueRow(key, document.getElementById("newVal").value);
    document.getElementById("newKey").value = "";
    document.getElementById("newVal").value = "";
    // fire once immediately
    const num = parseFloat(document.querySelector("#values .row:last-child input:nth-child(2)").value);
  };
</script>
```

- [ ] **Step 3: Run the templating test to verify it fails, then passes**

Run: `cargo test -p carapace-preview server`
Expected: PASS once `assets/index.html` exists (the test only needs `render_index`). If it fails to compile because later `serve`/`EngineMsg` are referenced elsewhere, they are added next — this step's test is self-contained.

- [ ] **Step 4: Implement `EngineMsg`, `Ports`, and `serve` (above the tests module)**

Prepend to `crates/carapace-preview/src/server.rs`:

```rust
use crate::protocol::{out_to_ws, parse_client_msg, ClientMsg, OutMsg};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

/// Messages delivered to the single-threaded engine loop.
pub enum EngineMsg {
    ClientConnected(Sender<OutMsg>),
    Client(ClientMsg),
    Reload,
}

pub struct Ports {
    pub http: u16,
    pub ws: u16,
}

/// Bind the HTTP viewer server + the WebSocket acceptor, spawn their loops, return the ports.
pub fn serve(http_port: u16, engine_tx: Sender<EngineMsg>) -> Ports {
    // WebSocket acceptor on an ephemeral loopback port.
    let ws_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ws port");
    let ws_port = ws_listener.local_addr().unwrap().port();

    // HTTP server for the single viewer page (page carries the live ws port).
    let http = tiny_http::Server::http(("127.0.0.1", http_port)).expect("bind http port");
    let bound_http = http.server_addr().to_ip().unwrap().port();
    let page = render_index(ws_port);

    // HTTP accept loop.
    std::thread::spawn(move || {
        for req in http.incoming_requests() {
            let resp = tiny_http::Response::from_string(page.clone()).with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                    .unwrap(),
            );
            let _ = req.respond(resp);
        }
    });

    // WS accept loop — one thread per browser connection.
    std::thread::spawn(move || {
        for stream in ws_listener.incoming().flatten() {
            let tx = engine_tx.clone();
            std::thread::spawn(move || ws_connection(stream, tx));
        }
    });

    Ports { http: bound_http, ws: ws_port }
}

/// One browser connection: full-duplex pump over a single blocking socket with a
/// short read timeout (so we can interleave outbound frames without splitting the stream).
fn ws_connection(stream: std::net::TcpStream, engine_tx: Sender<EngineMsg>) {
    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(_) => return,
    };
    // After the handshake, make reads time out so the loop can also write.
    let _ = ws
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(10)));

    let (out_tx, out_rx): (Sender<OutMsg>, Receiver<OutMsg>) = std::sync::mpsc::channel();
    if engine_tx.send(EngineMsg::ClientConnected(out_tx)).is_err() {
        return;
    }

    loop {
        // 1. Drain everything the engine wants to send this client.
        let mut engine_gone = false;
        loop {
            match out_rx.try_recv() {
                Ok(msg) => {
                    if ws.send(out_to_ws(&msg)).is_err() {
                        return; // socket dead
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    engine_gone = true;
                    break;
                }
            }
        }
        if engine_gone {
            return;
        }

        // 2. Read one inbound message (or time out).
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                if let Ok(cm) = parse_client_msg(t.as_str()) {
                    if engine_tx.send(EngineMsg::Client(cm)).is_err() {
                        return;
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => return,
            Ok(_) => {} // ping/pong/binary from browser: ignore
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return,
        }
    }
}
```

Add `mod server;` to `src/main.rs`.

> Note: `ws.send(msg)` = write + flush in tungstenite 0.21+. If the resolved tungstenite lacks `send`, use `ws.write(msg).and_then(|()| ws.flush())`. `Message::Text` holds a `Utf8Bytes`/`String` depending on version — `t.as_str()` works for both; if not, use `&t`.

- [ ] **Step 5: Verify it all compiles and the templating test still passes**

Run: `cargo test -p carapace-preview server`
Expected: PASS. Run `cargo build -p carapace-preview` — expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/carapace-preview/src/server.rs crates/carapace-preview/assets/index.html crates/carapace-preview/src/main.rs
git commit -m "feat(preview): http viewer server + websocket duplex + client registry"
```

---

### Task 7: Wire the engine loop + file watcher + browser launch (`main.rs`)

**Files:**
- Modify: `crates/carapace-preview/src/main.rs` (full implementation)

**Interfaces:**
- Consumes: everything above — `render::{init_gpu, new_offscreen, render_rgba, encode_png, frame_hash, Offscreen}`, `skin_session::SkinSession`, `server::{serve, EngineMsg}`, `protocol::{ClientMsg, OutMsg}`, `preview_host::{Values, ActionLog}`, `carapace::scene::Pt`, `carapace::engine::PointerEvent`.
- Produces: a working `carapace-preview <skin-dir> [--port <n>]` binary.

- [ ] **Step 1: Replace `main.rs` with the full wiring**

Rewrite `crates/carapace-preview/src/main.rs` (keep the module declarations):

```rust
//! carapace-preview — a live, interactive browser previewer for carapace skins.
//! See docs/superpowers/specs/2026-07-01-carapace-preview-design.md.

mod preview_host;
mod protocol;
mod render;
mod server;
mod skin_session;

use carapace::engine::PointerEvent;
use carapace::host::StateValue;
use carapace::scene::Pt;
use preview_host::{ActionLog, Values};
use protocol::{ClientMsg, OutMsg};
use server::{serve, EngineMsg};
use skin_session::SkinSession;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(skin_dir) = args.next() else {
        eprintln!("usage: carapace-preview <skin-dir> [--port <n>]");
        return ExitCode::FAILURE;
    };
    let mut port: u16 = 0; // 0 = ephemeral
    while let Some(a) = args.next() {
        if a == "--port" {
            port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        }
    }
    let dir = PathBuf::from(&skin_dir);
    if !dir.join("skin.toml").exists() {
        eprintln!("error: {skin_dir} has no skin.toml");
        return ExitCode::FAILURE;
    }

    // Shared, single-thread-owned host state.
    let values: Values = Default::default();
    let action_log: ActionLog = Default::default();

    // Engine-thread inbox.
    let (engine_tx, engine_rx) = mpsc::channel::<EngineMsg>();

    // HTTP + WS servers.
    let ports = serve(port, engine_tx.clone());
    let url = format!("http://127.0.0.1:{}", ports.http);
    println!("carapace-preview serving {url}  (skin: {skin_dir})");

    // File watcher → Reload messages.
    spawn_watcher(dir.clone(), engine_tx.clone());

    // Best-effort browser open (macOS `open`); harmless if it fails.
    let _ = std::process::Command::new("open").arg(&url).spawn();

    // Engine loop runs on THIS (main) thread — Engine is !Send.
    run_engine_loop(dir, values, action_log, engine_rx);
    ExitCode::SUCCESS
}

fn spawn_watcher(dir: PathBuf, engine_tx: mpsc::Sender<EngineMsg>) {
    use notify::{RecursiveMode, Watcher};
    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("watch disabled: {e}");
                return;
            }
        };
        if watcher.watch(&dir, RecursiveMode::Recursive).is_err() {
            eprintln!("watch disabled for {}", dir.display());
            return;
        }
        // Coalesce bursts: on any event, send a single Reload.
        while let Ok(ev) = rx.recv() {
            if ev.is_ok() {
                // Drain any queued events so a save = one reload.
                while rx.try_recv().is_ok() {}
                if engine_tx.send(EngineMsg::Reload).is_err() {
                    return;
                }
            }
        }
    });
}

fn run_engine_loop(
    dir: PathBuf,
    values: Values,
    action_log: ActionLog,
    engine_rx: mpsc::Receiver<EngineMsg>,
) {
    let mut session = SkinSession::new(dir, values.clone(), action_log.clone());

    let gpu = render::init_gpu();
    let mut renderer = carapace::render::Renderer::new(&gpu.device);
    let mut off = render::new_offscreen(&gpu.device, session.canvas.0.max(1), session.canvas.1.max(1));
    let mut render_size = session.canvas;

    let mut clients: Vec<mpsc::Sender<OutMsg>> = Vec::new();
    let mut last_hash: Option<u64> = None;
    let mut last_png: Option<Vec<u8>> = None;
    let mut clock = Instant::now();

    loop {
        // 1. Drain inbound messages (non-blocking).
        while let Ok(msg) = engine_rx.try_recv() {
            match msg {
                EngineMsg::ClientConnected(tx) => {
                    // Greet: meta, current error state, last frame.
                    let _ = tx.send(OutMsg::Meta {
                        name: session.name.clone(),
                        w: render_size.0,
                        h: render_size.1,
                    });
                    let _ = tx.send(OutMsg::Error {
                        message: session.last_error.clone(),
                    });
                    if let Some(png) = &last_png {
                        let _ = tx.send(OutMsg::Frame(png.clone()));
                    }
                    clients.push(tx);
                }
                EngineMsg::Client(ClientMsg::Pointer { x, y }) => {
                    if let Some(engine) = session.engine.as_mut() {
                        engine.handle_pointer_resolved(
                            render_size.0 as f32,
                            render_size.1 as f32,
                            Pt { x, y },
                            PointerEvent::Press,
                        );
                        engine.update(Duration::ZERO); // drain enqueued host action → log
                    }
                }
                EngineMsg::Client(ClientMsg::SetValue { key, value }) => {
                    if let Some(sv) = json_to_state(&value) {
                        values.borrow_mut().insert(key, sv);
                        last_hash = None; // force a resend
                    }
                }
                EngineMsg::Client(ClientMsg::SetCanvas { w, h }) => {
                    let (w, h) = (w.max(1), h.max(1));
                    render_size = (w, h);
                    off = render::new_offscreen(&gpu.device, w, h);
                    last_hash = None;
                    broadcast(&mut clients, &OutMsg::Meta {
                        name: session.name.clone(),
                        w,
                        h,
                    });
                }
                EngineMsg::Reload => {
                    session.reload();
                    // Canvas may have changed on reload; keep current render_size unless
                    // it was never set (first successful load handled below via meta).
                    broadcast(&mut clients, &OutMsg::Error {
                        message: session.last_error.clone(),
                    });
                    broadcast(&mut clients, &OutMsg::Meta {
                        name: session.name.clone(),
                        w: render_size.0,
                        h: render_size.1,
                    });
                    last_hash = None;
                }
            }
        }

        // 2. Drain the action log → broadcast.
        {
            let mut log = action_log.borrow_mut();
            for action in log.drain(..) {
                broadcast(&mut clients, &OutMsg::ActionLog { action });
            }
        }

        // 3. Render only when someone is watching and a skin is loaded.
        if !clients.is_empty() {
            if let Some(engine) = session.engine.as_mut() {
                let dt = clock.elapsed();
                clock = Instant::now();
                let rgba = render::render_rgba(engine, &mut renderer, &gpu, &off, dt);
                let h = render::frame_hash(&rgba);
                if last_hash != Some(h) {
                    last_hash = Some(h);
                    let png = render::encode_png(&rgba, off.w, off.h);
                    broadcast(&mut clients, &OutMsg::Frame(png.clone()));
                    last_png = Some(png);
                }
            }
        } else {
            clock = Instant::now(); // reset dt so animation doesn't jump after reconnect
        }

        std::thread::sleep(Duration::from_millis(16)); // ~60fps ceiling
    }
}

/// Broadcast to all clients, pruning any whose receiver has dropped.
fn broadcast(clients: &mut Vec<mpsc::Sender<OutMsg>>, msg: &OutMsg) {
    clients.retain(|tx| tx.send(msg.clone()).is_ok());
}

fn json_to_state(v: &serde_json::Value) -> Option<StateValue> {
    match v {
        serde_json::Value::Number(n) => n.as_f64().map(|f| StateValue::Scalar(f as f32)),
        serde_json::Value::String(s) => Some(StateValue::Str(Arc::from(s.as_str()))),
        serde_json::Value::Bool(b) => Some(StateValue::Bool(*b)),
        _ => None,
    }
}
```

- [ ] **Step 2: Build and lint**

Run:

```bash
cargo build -p carapace-preview
cargo clippy -p carapace-preview --all-targets -- -D warnings
```

Expected: clean build, no clippy warnings. (Fix any `unused`/`needless_clone` findings — e.g. the `png.clone()` into `last_png` is intentional; if clippy objects, restructure to encode once and clone for the broadcast.)

- [ ] **Step 3: Manual end-to-end verification (the design's success bar)**

Run against the interactive demo skin (has `region{}` hotspots + a `value_fill{ value="position" }`):

```bash
cargo run -p carapace-preview -- crates/carapace-demo/skins/minimal
```

Confirm each, in the opened browser tab:
1. **Renders** — the minimal skin's dark rounded backdrop appears on the checkerboard.
2. **Interactive** — clicking the mid grey panel logs `▶ toggle_play`; clicking the top-right `x`/`_` glyph areas logs `close`/`minimize`.
3. **Data-bound** — add key `position`, value `0.6`; the cyan `value_fill` bar fills to ~60%. Change it to `0.2`; the bar shrinks live.
4. **Hot reload** — edit `crates/carapace-demo/skins/minimal/skin.lua` (e.g. change a `fill` color), save; the preview re-renders within ~a second. **Restore the file afterward** (`git checkout crates/carapace-demo/skins/minimal`).
5. **Lua error survives** — introduce a syntax error, save; a red error banner appears and the last good frame stays; fix it, save; banner clears. Restore the file.

Then Ctrl-C to stop. (These are manual observations — the design explicitly validates the viewer "by eye against a running server.")

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-preview/src/main.rs
git commit -m "feat(preview): engine loop + file watcher + browser launch (end-to-end)"
```

---

### Task 8: README + final polish

**Files:**
- Create: `crates/carapace-preview/README.md`

**Interfaces:** none (docs only).

- [ ] **Step 1: Write the README**

Create `crates/carapace-preview/README.md`:

```markdown
# carapace-preview

> A live, interactive browser previewer for carapace skins. (Dev tool.)

Editing a skin used to mean a full rebuild — the native demo, or an iOS device
round-trip — just to see a change. `carapace-preview` closes that loop.

## Usage

```bash
cargo run -p carapace-preview -- path/to/skin-dir [--port <n>]
```

It serves `http://127.0.0.1:<port>` (a random free port unless `--port` is given)
and opens a browser tab. The **real carapace engine** renders the skin offscreen
(headless wgpu / Vello); the browser is a thin display + control surface.

## What you get

- **Live render** of the skin via the real engine.
- **Hot reload:** saving `skin.lua` / `skin.toml` / an asset re-renders within a
  moment. A skin that fails to load shows its **Lua error** as a banner — the
  server and last-good frame survive.
- **Click to interact:** clicking the preview forwards a pointer event, so
  `region{}` hotspots fire their actions (shown in the action log).
- **Host-data panel:** add/edit the host values the skin binds
  (`value_fill{ value="level" }`, `text{ value="track" }`, …); edits re-render live.
- **Animated skins play** — the engine ticks continuously with wall-clock `dt`.
- **Canvas size** input re-lays-out resizable (anchored / frame) skins.

## Not yet (planned — Plan B)

The property inspector and skin-parameters panel that **write edits back to
`skin.lua`** (source provenance via `full_moon` + mlua debug hooks) are a separate
follow-up. This tool is view + host-data-drive + hot-reload only.

## How it works

A single engine thread owns the `Engine` (which is `!Send`), a headless wgpu
device, an offscreen `Rgba8` target, and the render loop. It renders → reads back
RGBA → PNG-encodes → pushes over a WebSocket, and only re-sends when the frame
actually changed (hash compare), so a settled static skin streams nothing. A
`tiny_http` server serves the one-page viewer; a `tungstenite` WebSocket carries
frames down and pointer/value/canvas edits up. Nothing `!Send` crosses a thread.

See the design: `docs/superpowers/specs/2026-07-01-carapace-preview-design.md`.
```

- [ ] **Step 2: Final workspace-wide checks**

Run:

```bash
cargo fmt
cargo clippy -p carapace-preview --all-targets -- -D warnings
cargo test -p carapace-preview
```

Expected: fmt clean, no clippy warnings, all non-GPU tests pass (the render test passes where a GPU adapter exists).

- [ ] **Step 3: Confirm no unintended changes to other crates**

Run: `git status --short` and `git diff --stat main -- crates/carapace crates/hittest`
Expected: the only tracked change outside `crates/carapace-preview/` is the one line appended to the root `Cargo.toml` `members`. If `crates/carapace-demo/skins/minimal` shows as modified, restore it (`git checkout crates/carapace-demo/skins/minimal`).

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-preview/README.md
git commit -m "docs(preview): README for carapace-preview core previewer"
```

---

## Self-Review

**Spec coverage** (against `2026-07-01-carapace-preview-design.md`):
- Goal 1 (renders via real engine, offscreen) → Task 4 (`render_rgba`) + Task 7 (loop).
- Goal 2 (hot reload; Lua error → page, not crash) → Task 5 (`SkinSession::reload` captures errors) + Task 7 (watcher, `Error` broadcast) + `index.html` error banner.
- Goal 3 (click → region actions logged) → Task 7 (`handle_pointer_resolved` + action-log drain) + Task 3 (`PreviewHost::invoke`).
- Goal 4 (data-bound panel edits host values) → Task 7 (`SetValue` → shared `Values`) + `index.html` panel + Task 3 (`get`).
- Goal 5 (animated skins play) → Task 7 (continuous `render_rgba` with wall-clock `dt`).
- Goal 6 (inspector/params write-back) → **out of scope (Plan B)**, per the agreed split; README states this.
- Architecture (offscreen engine, browser thin viewer, `!Send` isolation via channels) → Tasks 4/6/7.
- `PreviewHost` source-scan allowlist w/ `Box::leak` → Task 3.
- Frame change-detection (hash, resend only on change) → Task 4 (`frame_hash`) + Task 7.
- PNG transport via `image` → Task 4 (`encode_png`) + `index.html` decode.
- Web stack (`tiny_http` + `tungstenite`, no async tree) + `notify` → Tasks 1/6/7. (`full_moon` is deferred to Plan B — not added here.)
- Testing (source-scan, frame change-detection, headless smoke) → Tasks 3/4; viewer "by eye" → Task 7 Step 3.
- Deliverables (crate + viewer + README + workspace member) → Tasks 1/6/7/8.

**Type consistency:** `Values`/`ActionLog` aliases defined in Task 3 are reused verbatim in Tasks 5/7. `OutMsg`/`ClientMsg`/`EngineMsg` names are consistent across Tasks 2/6/7. `render::{init_gpu,new_offscreen,render_rgba,encode_png,frame_hash,Offscreen}` signatures defined in Task 4 match their call sites in Task 7. `StateValue::Scalar(f32)`/`Str(Arc<str>)` construction matches the verified API.

**Placeholder scan:** every code step contains complete code; no TBD/TODO/"handle errors appropriately".

## Known risks / notes for the executor

- **tungstenite / tiny_http API drift.** `sfw cargo add` pulls the latest versions; the exact `Message` constructors (`text`/`binary`), `ws.read()`/`ws.send()`, and `tiny_http::Server::http` signatures may differ slightly from what's written. The TDD build loop surfaces any mismatch immediately — adjust to the resolved API; the *architecture* (accept → duplex pump with a read timeout, ephemeral WS port templated into the page) does not change.
- **GPU in tests.** The Task 4 render test needs a wgpu adapter (runs under the repo's gpu-tests CI variant, like `embed-spike`). If a CI lane has no GPU, mark that one test `#[ignore]` — do not gate the whole suite.
- **`clock` reset when idle.** When no clients are attached, `dt` is reset each tick so a reconnecting client doesn't get a huge first-frame `dt`.
```

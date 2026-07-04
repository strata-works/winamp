# Engine API (Rust)

The public Rust surface of the `carapace` crate — for embedding the engine into a Rust host. If you're embedding via C/Swift/Flutter, see [FFI / C ABI](./ffi-c-abi.md). If you're authoring skins, see the [Skin Authoring Reference](./skin-authoring.md).

Crate: `carapace` (edition 2024), `crates/carapace/`. Reflects source as of 2026-07-04 with `file:line` citations.

> **Threading invariant:** `Engine` is **`!Send` / `!Sync`** — it holds `Rc`-based state and a Lua VM (`engine.rs:16-18`). Construct, drive, and drop it on a single thread. `Renderer` is likewise tied to the thread owning its wgpu device. `SkinSource` (and the `Swap`/`SwitchHost` commands carrying it) are also not `Send`.

Public modules (`lib.rs:1-13`): `asset`, `command`, `engine`, `host`, `layout`, `render`, `scene`, `script`, `shape`, `skin`, `state`, `swap`, `vocab`. The crate re-exports `pub use mlua;` (`lib.rs:17`) so primitive-extension authors use the exact matching `mlua` version.

## The lifecycle in one paragraph

Build a `Host` implementation + a `VocabRegistry` (usually `VocabRegistry::base()`) + a `SkinSource` (from `skin::load_dir` or `SkinSource::inline`), then `Engine::new(host, registry, source)`. Each frame: feed input via `handle_pointer` / `handle_pointer_resolved` / `handle_command`, call `update(dt)` to drain queued commands and tick the host, then `layout(w, h)` (or read `scene()`) to get geometry, and hand it to `Renderer::draw`.

## Engine

`pub struct Engine` (`engine.rs:16`, opaque). `pub enum PointerEvent { Press }` (`engine.rs:12`).

| Signature | Description |
|---|---|
| `fn new(host: Box<dyn Host>, registry: VocabRegistry, initial: SkinSource) -> Result<Engine, ScriptError>` | Construct: wraps `registry` in `Rc`, creates the command queue, builds the initial skin. Errors if the Lua entry fails to load/run. `engine.rs:24-38` |
| `fn handle_pointer(&mut self, p: Pt, kind: PointerEvent)` | Hit-test the **design** scene at `p` and fire the matched hotspot's handler (handlers only enqueue commands; errors are logged). `engine.rs:41-48` |
| `fn handle_pointer_resolved(&mut self, logical_w: f32, logical_h: f32, p: Pt, kind: PointerEvent)` | Like above but against the **layout-resolved** scene for a logical size (so anchored/frame hotspots hit where drawn); dispatches hotspot handlers, list-row selects, and scrub seeks. `engine.rs:53-84` |
| `fn handle_command(&mut self, cmd: Command)` | Enqueue a meta command (host-app-level, not from skin picking). `engine.rs:87-89` |
| `fn update(&mut self, dt: Duration)` | Drain queued commands — validating each `HostAction` against the current host's `actions()` allowlist before `host.invoke` — apply `Swap`/`SwitchHost` transactionally, then call `host.tick(dt)`. `engine.rs:92-112` |
| `fn scene(&self) -> &Scene` | The current **design** scene (unresolved). `engine.rs:126-128` |
| `fn scene_anchors(&self) -> &[layout::Anchors]` | Per-node anchors, parallel to `scene().nodes`. `engine.rs:131-133` |
| `fn scene_origins(&self) -> &[scene::Origin]` | Per-node source origins for the design scene. `engine.rs:137-139` |
| `fn layout(&self, logical_w: f32, logical_h: f32) -> Scene` | Resolve the design scene to a logical scene for a window size (result `canvas` = logical size); expands `List` into rows. Frame skins call this on resize; gadget skins render `scene()` directly. `engine.rs:144-146` |
| `fn layout_with_origins(&self, logical_w: f32, logical_h: f32) -> (Scene, Vec<scene::Origin>)` | Same as `layout` but also returns origins aligned 1:1 with the resolved nodes — for authoring tools. `engine.rs:151-157` |
| `fn state(&self, key: &str) -> Option<StateValue>` | Read a host data value by key (delegates to `Host::get`). `engine.rs:171-173` |

## Skin loading

```rust
pub fn skin::load_dir(dir: &Path) -> Result<(Manifest, SkinSource), SkinError>   // skin.rs:65-85
```

Loads and validates `skin.toml` (`schema == 1`, `engine == "^0.1"`), reads the Lua entry file, resolves the asset directory, and returns `(Manifest, SkinSource)`. A skin directory holds `skin.toml`, the `entry` Lua file, and an `asset_dir` (default `assets/`, recursively indexed by relative path; symlinks skipped to prevent sandbox escape — `asset.rs:34-37`).

```rust
pub struct Manifest {          // skin.rs:20-39
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub engine: String,
    pub canvas: Canvas,        // { width: u32, height: u32 }  (skin.rs:15-18)
    pub entry: String,
    pub asset_dir: String,     // default "assets"
    pub resizable: bool,
    pub min_size: Option<(u32, u32)>,
    pub max_size: Option<(u32, u32)>,
}

pub enum SkinError {           // skin.rs:41-48
    Io(std::io::Error), Toml(toml::de::Error),
    UnsupportedSchema(u32), EngineIncompat(String), Asset(asset::AssetError),
}

pub struct SkinSource {        // command.rs:8-12
    pub lua_src: String,
    pub canvas: (u32, u32),
    pub assets: Rc<AssetResolver>,
}
pub fn SkinSource::inline(lua_src: impl Into<String>, canvas: (u32, u32)) -> Self  // command.rs:16-22 (no on-disk assets; tests/asset-free skins)
```

See the manifest field table in the [Skin Authoring Reference](./skin-authoring.md#the-skintoml-manifest).

## Host

The capability surface you implement to expose your app's data and actions. Boxed as `Box<dyn Host>` and passed to `Engine::new` (and `Command::SwitchHost`). The engine knows no concrete names — everything flows through this trait.

```rust
pub trait Host {                                           // host.rs:40-50
    fn name(&self) -> &str;                                // identify the host
    fn tick(&mut self, dt: Duration);                      // once per Engine::update, after draining
    fn get(&self, key: &str) -> Option<StateValue>;        // data reads (value/text bindings, Engine::state)
    fn actions(&self) -> &[ActionSpec];                    // allowlist checked before invoke
    fn invoke(&mut self, action: &str, args: &[Value]);    // perform an allowlisted action
    fn rows(&self, collection: &str) -> Vec<Row> { Vec::new() } // list{} collections (default empty)
}
```

Supporting types:

```rust
pub enum Value { Num(f64), Bool(bool), Str(String) }       // host.rs:6-11 (invoke args)

pub struct Row { pub cells: BTreeMap<String, StateValue> } // host.rs:16-32
impl Row {
    pub fn new() -> Self;
    pub fn set(self, key: &str, value: StateValue) -> Self; // builder-style
    pub fn get(&self, key: &str) -> Option<&StateValue>;
}

pub struct ActionSpec { pub name: &'static str }           // host.rs:34-37
```

**Window controls / drag** are *not* methods on `Host`. Instead the engine classifies scene regions: an author declares `role = "drag"` (→ `HotspotRole::Drag`), and the embedder reads `Scene::hit_kind` / `Hit::Handler` to translate a `HitKind::Drag` into an OS window-move itself. The engine never calls back into `Host` for window chrome.

A minimal reference `Host` lives at `crates/carapace-preview/src/preview_host.rs`.

## State

```rust
pub enum StateValue {          // state.rs:2-6  (Clone, PartialEq, Debug)
    Bool(bool),                // flags; `true` == 1.0 for fills
    Scalar(f32),               // 0..1 levels, indices
    Str(Arc<str>),             // shared text (titles, cells)
}
```

How host data crosses the engine/skin boundary. Returned by `Host::get`/`Row::get`; consumed by `Engine::state`, list expansion, and the renderer's `value_of`/`text_of`.

## Scene / picking

```rust
pub struct Scene { pub nodes: Vec<Node>, pub canvas: (u32, u32) }   // scene.rs:253-256
```

An ordered node list (later nodes draw on top / win hit-tests) plus the canvas they're authored/resolved against. `Node` variants (`scene.rs:176-237`): `Fill`, `Hotspot`, `ValueFill`, `Image`, `Frame`, `View`, `List`, `Scrub`, `Text`.

```rust
pub struct Origin { pub line: Option<u32>, pub call: Option<u32> }  // scene.rs:243-250
pub struct Pt { pub x: f32, pub y: f32 }                            // scene.rs:4-7
```

`Origin` is source-provenance metadata (renderer/hit-test ignore it): `line` is the 1-based primitive-call line; `call` is a monotonic per-call index (nodes from one call share it; `None` for engine-generated rows/highlights).

Scene query methods:

| Signature | Description |
|---|---|
| `fn views(&self) -> Vec<(String, ImageDest)>` | Host-content regions (`Node::View`) the embedder fills. `scene.rs:383-391` |
| `fn hit_row(&self, p: Pt) -> Option<(String, usize)>` | Topmost `List` row under `p`: `(on_select action, index)`. `scene.rs:394-419` |
| `fn hit_scrub(&self, p: Pt) -> Option<(String, f32)>` | Topmost `Scrub` under `p`: `(on_seek action, 0..1 fraction)`. `scene.rs:422-445` |
| `fn hit(&self, p: Pt) -> Option<HandlerId>` | Topmost `Hotspot` containing `p`. `scene.rs:448-459` |
| `fn hit_any(&self, p: Pt) -> Option<Hit>` | Topmost interactive node of any kind — the recommended single input entry point. `scene.rs:464-503` |
| `fn pick(&self, p: Pt) -> Option<usize>` | Index of the topmost node by bounding box (via `layout::node_bbox`); skips zero-area. For authoring tools; call on a resolved scene. `scene.rs:509-520` |
| `fn covers(&self, p: Pt) -> bool` | Whether `p` is inside any drawn node's opaque bounds — for a shaped-window / click-through mask. `scene.rs:526-542` |
| `fn hit_kind(&self, p: Pt) -> HitKind` | Classify `p` for the embedder **without** firing any handler. `scene.rs:547-578` |
| `fn summary(&self) -> String` | Stable textual dump for snapshot tests. `scene.rs:270-380` |

```rust
pub enum Hit { Handler(HandlerId), Row { action: String, index: usize }, Scrub { action: String, fraction: f32 } } // scene.rs:606-614
pub enum HitKind { Passthrough, Control, Drag }            // scene.rs:617-625  (Drag => host moves the window)
pub enum HotspotRole { Control, Drag, Passthrough }        // scene.rs:165-173  (author-declared)
pub type HandlerId = usize;                                // scene.rs:97
pub fn layout::node_bbox(node: &Node) -> Option<Rect>      // layout.rs:80-136 (bbox for pick/selection outlines)
```

Also public in `scene.rs`: `Color {r,g,b,a: u8}`, `ColorStop`, `Gradient::{Linear,Radial,Sweep}`, `Paint::{Solid,Gradient}`, `TextContent::{Static,Bound}`, `FillDir`, `HAlign`, `VAlign`, `FontData` (+ `FontData::new`), `ImageDest`, `Slice`, `FrameCenter`, `RowTemplate`/`RowCell`.

A **resolved** scene (from `layout`/`layout_with_origins`) has `canvas` = the requested logical size, every node reflowed per its `Anchors`, and `List` nodes expanded into a `count` plus trailing generated `Text`/`Fill` (highlight) nodes.

## Vocab

```rust
pub struct VocabRegistry { /* private */ }                 // vocab.rs:509-543
impl VocabRegistry {
    pub fn new() -> Self;                                  // empty
    pub fn register(&mut self, prim: Box<dyn Primitive>);  // add one primitive
    pub fn iter(&self) -> impl Iterator<Item = &dyn Primitive>;
    pub fn base() -> Self;                                 // the stock 9 primitives
}
```

`base()` registers `fill`, `region`, `value_fill`, `image`, `frame`, `text`, `view`, `list`, `scrub` — the typical `registry` argument to `Engine::new`. To add custom primitives, build on `new()` (or `base()`) and `register` your own:

```rust
pub trait Primitive {                                      // vocab.rs:34-37
    fn id(&self) -> &str;                                  // the Lua constructor name
    fn build(&self, args: &mlua::Table, ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError>;
}
pub trait BuildContext {                                   // vocab.rs:22-31
    fn register_handler(&mut self, f: mlua::Function) -> HandlerId;
    fn host_action(&mut self, action: &str, args: Vec<host::Value>) -> HandlerId;
    fn image(&mut self, name: &str) -> Result<Arc<DecodedImage>, AssetError>;
    fn font(&mut self, name: &str) -> Result<Arc<FontData>, AssetError>;
}
pub enum BuildError { MissingField(&'static str), BadType(&'static str), Lua(mlua::Error), Asset(AssetError) } // vocab.rs:8-19
// helpers: parse_path, color_from_table, parse_color   (vocab.rs:39-72)
```

## Render

```rust
pub struct RenderTarget<'a> {                              // render.rs:21-28
    pub device: &'a wgpu::Device, pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView, pub width: u32, pub height: u32,
    pub base_color: scene::Color,
}
pub struct Renderer { /* private */ }                      // render.rs:30-44
impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self;             // vello renderer + font/layout ctx + view-composite pipeline
    pub fn draw<'v>(
        &mut self,
        scene: &Scene,
        read_value: impl Fn(&str) -> Option<StateValue>,   // resolve bound values (wrap Host::get / Engine::state)
        view_tex: impl Fn(&str) -> Option<&'v wgpu::TextureView>, // supply Node::View textures (cover art, live view)
        target: &RenderTarget,
    );
}
```

`Renderer::draw` is the per-frame entry point: it scales canvas coords to the target pixel size, draws all node kinds via vello, resolves bound values through `read_value`, and composites `Node::View` regions via `view_tex` (premultiplied-alpha blend over the scene). Like the engine, the renderer is single-threaded — use it on the thread owning its wgpu device.

## See also

- [FFI / C ABI](./ffi-c-abi.md) — the same engine wrapped for native/C hosts.
- [Preview Tool](./preview-tool.md) — a working host + renderer harness (`carapace-preview`).
- Reference `Host` implementation: `crates/carapace-preview/src/preview_host.rs`.

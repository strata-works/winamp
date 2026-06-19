# Phase 5a — Asset Loading + `image` Primitive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a generic, sandboxed asset resolver + an `image` primitive so skins can draw real bitmap artwork (PNG/JPEG/GIF/BMP), and upgrade the demo `reference` skin to render the genuine Headspace bitmap.

**Architecture:** A type-agnostic `AssetResolver` (scan a sandboxed `assets/` dir → name→bytes, cached) with a PNG/JPEG/GIF/BMP decoder as its first consumer. An `image` vocabulary primitive builds a `Node::Image { Arc<DecodedImage>, dest }`; decoding is headless (the `image` crate), only the GPU upload is in `render` (vello image, sRGB-correct). `SkinSource` carries the skin's `Rc<AssetResolver>` so swaps resolve their own assets.

**Tech Stack:** Rust edition 2024; the `image` crate (decode); existing `vello` 0.9 / `wgpu` 29 for the GPU draw.

## Global Constraints

- Rust, **edition 2024**, stable. Work on branch `phase5a-asset-loading` (off `main`).
- **Dependency policy (`sfw`):** any command that fetches a third-party package runs through **`sfw`** (Socket Firewall). Add the `image` crate with **`sfw cargo add image --no-default-features --features png,jpeg,gif,bmp -p carapace`**, and run the first build/test that downloads it as **`sfw cargo test ...`** / **`sfw cargo build ...`**. Subsequent offline builds/tests don't need `sfw`.
- **Headless boundary holds:** decoding is headless (`image` crate, CPU); only `render.rs` touches the GPU. `Node::Image` carries **decoded RGBA8** so headless skin-build tests work with no GPU.
- **Zero domain knowledge in `carapace`:** no media/skin-specific names in the engine; `asset`/`image` are generic.
- **Sandbox:** the asset resolver only serves files under the scanned root; names containing `..` or escaping the skin dir are rejected. The Lua sandbox is unchanged — skins only *name* assets.
- **Color:** sRGB assumed (no ICC); decode to straight-alpha RGBA8; the render pipeline must sample images as sRGB (gamma-correct), guarded by a color-sentinel in the gated GPU test. Wide-gamut/HDR out of scope.
- **Image formats:** PNG, JPEG, GIF (first frame), BMP; content-detected via `image::load_from_memory`. Unsupported/corrupt → `AssetError::Decode`.
- The fast `check` CI job stays headless/green; the gated render test (`gpu-tests` feature) is the only GPU-running test.

---

### Task 1: `asset` module — resolver + decoder

**Files:**
- Modify: `crates/carapace/Cargo.toml` (add `image` via `sfw`)
- Create: `crates/carapace/src/asset.rs`
- Modify: `crates/carapace/src/lib.rs` (add `pub mod asset;`)

**Interfaces:**
- Produces:
  - `pub struct DecodedImage { pub rgba: Vec<u8>, pub width: u32, pub height: u32 }` (straight-alpha sRGB RGBA8)
  - `pub enum AssetError { Unresolved(String), Io(String), Decode(String) }`
  - `pub struct AssetResolver` with `pub fn resolve(skin_dir: &Path, asset_dir: &str) -> Result<Self, AssetError>`, `pub fn empty() -> Self`, `pub fn bytes(&self, name: &str) -> Result<std::sync::Arc<[u8]>, AssetError>`, `pub fn image(&self, name: &str) -> Result<std::sync::Arc<DecodedImage>, AssetError>`.

- [ ] **Step 1: Add the `image` crate via sfw**

Run: `sfw cargo add image --no-default-features --features png,jpeg,gif,bmp -p carapace`
Expected: `image` added under `[dependencies]` with exactly those format features. (Run under `sfw` so the package fetch is filtered.)

- [ ] **Step 2: Declare the module**

Add to `crates/carapace/src/lib.rs`: `pub mod asset;`

- [ ] **Step 3: Write the failing tests + `asset.rs`**

Create `crates/carapace/src/asset.rs`:

```rust
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum AssetError {
    Unresolved(String),
    Io(String),
    Decode(String),
}

/// Type-agnostic, sandboxed asset resolver: scans a directory into a name->path index,
/// then serves raw bytes (any type) or decoded images, caching both.
pub struct AssetResolver {
    index: HashMap<String, PathBuf>,
    bytes_cache: RefCell<HashMap<String, Arc<[u8]>>>,
    image_cache: RefCell<HashMap<String, Arc<DecodedImage>>>,
}

fn walk(root: &Path, dir: &Path, index: &mut HashMap<String, PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, index)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                index.insert(rel.to_string_lossy().replace('\\', "/"), path.clone());
            }
        }
    }
    Ok(())
}

impl AssetResolver {
    /// An empty resolver — for inline skins with no assets. Every lookup is `Unresolved`.
    pub fn empty() -> Self {
        Self {
            index: HashMap::new(),
            bytes_cache: RefCell::new(HashMap::new()),
            image_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Resolve (scan) the asset directory under `skin_dir`, recursively, sandboxed.
    /// A missing asset dir yields an empty resolver (a skin may legitimately have no assets).
    pub fn resolve(skin_dir: &Path, asset_dir: &str) -> Result<Self, AssetError> {
        let root = skin_dir.join(asset_dir);
        let mut index = HashMap::new();
        if root.is_dir() {
            walk(&root, &root, &mut index).map_err(|e| AssetError::Io(e.to_string()))?;
        }
        Ok(Self {
            index,
            bytes_cache: RefCell::new(HashMap::new()),
            image_cache: RefCell::new(HashMap::new()),
        })
    }

    pub fn bytes(&self, name: &str) -> Result<Arc<[u8]>, AssetError> {
        if name.contains("..") {
            return Err(AssetError::Unresolved(name.to_string()));
        }
        if let Some(b) = self.bytes_cache.borrow().get(name) {
            return Ok(b.clone());
        }
        let path = self.index.get(name).ok_or_else(|| AssetError::Unresolved(name.to_string()))?;
        let raw = std::fs::read(path).map_err(|e| AssetError::Io(e.to_string()))?;
        let arc: Arc<[u8]> = Arc::from(raw.into_boxed_slice());
        self.bytes_cache.borrow_mut().insert(name.to_string(), arc.clone());
        Ok(arc)
    }

    pub fn image(&self, name: &str) -> Result<Arc<DecodedImage>, AssetError> {
        if let Some(img) = self.image_cache.borrow().get(name) {
            return Ok(img.clone());
        }
        let bytes = self.bytes(name)?;
        let dynimg = image::load_from_memory(&bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
        let rgba = dynimg.to_rgba8();
        let decoded = Arc::new(DecodedImage {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        });
        self.image_cache.borrow_mut().insert(name.to_string(), decoded.clone());
        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a temp skin dir with an assets/ subdir holding a tiny PNG (encoded via the image crate).
    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn temp_skin(seed: &str) -> Tmp {
        let base = std::env::temp_dir().join(format!("carapace-asset-{seed}"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("assets/sub")).unwrap();
        // a 2x2 RGBA PNG: top-left red, rest transparent
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.save(base.join("assets/red.png")).unwrap();
        img.save(base.join("assets/sub/red.png")).unwrap();
        std::fs::write(base.join("assets/not_an_image.txt"), b"hello").unwrap();
        Tmp(base)
    }

    #[test]
    fn resolves_and_decodes_png() {
        let t = temp_skin("decode");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        let img = r.image("red.png").unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]); // top-left red, opaque
    }

    #[test]
    fn recursive_keying_works() {
        let t = temp_skin("recursive");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        assert!(r.image("sub/red.png").is_ok(), "nested asset resolvable by relative path");
    }

    #[test]
    fn unresolved_and_traversal_error() {
        let t = temp_skin("sandbox");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        assert!(matches!(r.image("nope.png"), Err(AssetError::Unresolved(_))));
        assert!(matches!(r.bytes("../secret"), Err(AssetError::Unresolved(_))));
    }

    #[test]
    fn corrupt_image_is_decode_error() {
        let t = temp_skin("corrupt");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        assert!(matches!(r.image("not_an_image.txt"), Err(AssetError::Decode(_))));
    }

    #[test]
    fn bytes_are_cached_and_empty_resolver_is_inert() {
        let t = temp_skin("cache");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        let a = r.bytes("red.png").unwrap();
        let b = r.bytes("red.png").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "second read hits the cache");
        assert!(matches!(AssetResolver::empty().image("red.png"), Err(AssetError::Unresolved(_))));
    }
}
```

- [ ] **Step 4: Run the tests (first run downloads `image` — under sfw)**

Run: `sfw cargo test -p carapace --lib asset`
Expected: PASS (5 passed). The first build compiles the `image` crate (fetched through `sfw`). If a later offline re-run is needed, plain `cargo test -p carapace --lib asset` is fine.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/Cargo.toml crates/carapace/src/lib.rs crates/carapace/src/asset.rs
git commit -m "feat(carapace): sandboxed asset resolver + image decoder"
```

---

### Task 2: `scene` — `Node::Image` + summary

**Files:**
- Modify: `crates/carapace/src/scene.rs`

**Interfaces:**
- Consumes: `asset::DecodedImage`.
- Produces: `pub struct ImageDest { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }`; `Node::Image { image: std::sync::Arc<crate::asset::DecodedImage>, dest: ImageDest }`; `Scene::summary` emits a line for it.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/carapace/src/scene.rs`:

```rust
    #[test]
    fn summary_includes_image_nodes() {
        use crate::asset::DecodedImage;
        use std::sync::Arc;
        let scene = Scene {
            canvas: (342, 394),
            nodes: vec![Node::Image {
                image: Arc::new(DecodedImage { rgba: vec![0; 4], width: 342, height: 394 }),
                dest: ImageDest { x: 0.0, y: 0.0, w: 342.0, h: 394.0 },
            }],
        };
        assert_eq!(scene.summary(), "canvas 342x394\nimage 342x394 at 0,0 dest 342x394");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p carapace --lib scene::tests::summary_includes_image_nodes`
Expected: FAIL — `ImageDest` / `Node::Image` not found.

- [ ] **Step 3: Add the node + dest + summary arm**

In `crates/carapace/src/scene.rs`, add the `ImageDest` struct (near `Color`):

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageDest {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
```

Add the variant to `enum Node` (after `ValueFill`):

```rust
    Image { image: std::sync::Arc<crate::asset::DecodedImage>, dest: ImageDest },
```

Add the arm to `Scene::summary`'s `match node` (after the `ValueFill` arm):

```rust
                Node::Image { image, dest } => format!(
                    "image {}x{} at {},{} dest {}x{}",
                    image.width, image.height, dest.x as i64, dest.y as i64, dest.w as i64, dest.h as i64
                ),
```

(`Scene::hit` already ignores non-`Hotspot` nodes, so `Image` is correctly invisible to hit-testing.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace --lib scene`
Expected: PASS (all scene tests incl. the new one).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/src/scene.rs
git commit -m "feat(carapace): Node::Image scene node + summary"
```

---

### Task 3: `vocab` — `image` primitive + `BuildContext::image`

**Files:**
- Modify: `crates/carapace/src/vocab.rs`

**Interfaces:**
- Consumes: `asset::DecodedImage`, `scene::{Node, ImageDest}`.
- Produces: `BuildContext` gains `fn image(&mut self, name: &str) -> Result<std::sync::Arc<crate::asset::DecodedImage>, crate::asset::AssetError>`; `BuildError` gains `Asset(crate::asset::AssetError)`; new `ImagePrim` (id `"image"`); `VocabRegistry::base()` now registers **4** primitives (fill/region/value_fill/image).

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/carapace/src/vocab.rs`:

```rust
    #[test]
    fn image_prim_builds_native_and_scaled() {
        use crate::asset::DecodedImage;
        use std::sync::Arc;
        struct Ctx(Arc<DecodedImage>);
        impl BuildContext for Ctx {
            fn register_handler(&mut self, _f: Function) -> HandlerId {
                0
            }
            fn image(&mut self, _name: &str) -> Result<Arc<DecodedImage>, crate::asset::AssetError> {
                Ok(self.0.clone())
            }
        }
        let img = Arc::new(DecodedImage { rgba: vec![0; 16], width: 4, height: 2 });
        let lua = mlua::Lua::new();
        // native: dest = (x,y, native w,h)
        let t: Table = lua.load("return { asset='a.png', x=10, y=20 }").eval().unwrap();
        match ImagePrim.build(&t, &mut Ctx(img.clone())).unwrap() {
            Node::Image { dest, .. } => {
                assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 20.0, 4.0, 2.0));
            }
            other => panic!("expected Image, got {other:?}"),
        }
        // scaled: explicit w,h
        let t2: Table = lua.load("return { asset='a.png', x=0, y=0, w=40, h=30 }").eval().unwrap();
        match ImagePrim.build(&t2, &mut Ctx(img)).unwrap() {
            Node::Image { dest, .. } => assert_eq!((dest.w, dest.h), (40.0, 30.0)),
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn base_registry_now_has_four() {
        assert_eq!(VocabRegistry::base().iter().count(), 4);
    }
```

> Note: the existing `BuildContext` test stubs (`NoHandlers`, `Counter`) will no longer satisfy the trait once `image` is added. Update them in Step 3 (give each an `image` method returning `AssetError::Unresolved`).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p carapace --lib vocab`
Expected: FAIL to compile — `BuildContext::image` / `ImagePrim` not defined; existing stubs miss `image`.

- [ ] **Step 3: Implement**

In `crates/carapace/src/vocab.rs`:

Add to `enum BuildError`:

```rust
    Asset(crate::asset::AssetError),
```

Add the method to the `BuildContext` trait:

```rust
    fn image(&mut self, name: &str) -> Result<std::sync::Arc<crate::asset::DecodedImage>, crate::asset::AssetError>;
```

Add `ImagePrim` (alongside the other prims):

```rust
struct ImagePrim;
impl Primitive for ImagePrim {
    fn id(&self) -> &str {
        "image"
    }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        let name: String = args.get("asset").map_err(|_| BuildError::MissingField("asset"))?;
        let image = ctx.image(&name).map_err(BuildError::Asset)?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").unwrap_or(image.width as f32);
        let h: f32 = args.get("h").unwrap_or(image.height as f32);
        Ok(Node::Image { image, dest: crate::scene::ImageDest { x, y, w, h } })
    }
}
```

Register it in `VocabRegistry::base()` (after `ValueFillPrim`):

```rust
        r.register(Box::new(ImagePrim));
```

Update the in-test `BuildContext` stubs `NoHandlers` and `Counter` to implement `image`:

```rust
    fn image(&mut self, name: &str) -> Result<std::sync::Arc<crate::asset::DecodedImage>, crate::asset::AssetError> {
        Err(crate::asset::AssetError::Unresolved(name.to_string()))
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace --lib vocab`
Expected: PASS (the new tests + updated existing tests, incl. `base_registry_now_has_four`).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/src/vocab.rs
git commit -m "feat(carapace): image vocabulary primitive + BuildContext::image"
```

---

### Task 4: thread the resolver — `command` + `script`

**Files:**
- Modify: `crates/carapace/src/command.rs`
- Modify: `crates/carapace/src/script.rs`

**Interfaces:**
- Produces: `SkinSource { lua_src, canvas, assets: std::rc::Rc<crate::asset::AssetResolver> }` + `SkinSource::inline(lua_src: impl Into<String>, canvas: (u32, u32)) -> SkinSource` (empty assets). `script::load` wires the resolver into the `SceneBuilder` so `BuildContext::image` resolves real assets.

- [ ] **Step 1: Update `SkinSource` (command.rs)**

In `crates/carapace/src/command.rs`, change `SkinSource` and add the constructor:

```rust
use std::rc::Rc;

use crate::asset::AssetResolver;
use crate::host::{Host, Value};

#[derive(Clone)]
pub struct SkinSource {
    pub lua_src: String,
    pub canvas: (u32, u32),
    pub assets: Rc<AssetResolver>,
}

impl SkinSource {
    /// An inline skin with no assets (tests, asset-free skins).
    pub fn inline(lua_src: impl Into<String>, canvas: (u32, u32)) -> Self {
        Self { lua_src: lua_src.into(), canvas, assets: Rc::new(AssetResolver::empty()) }
    }
}
```

(Remove the old `#[derive(Debug)]` on `SkinSource` if present — `AssetResolver` isn't `Debug`; keep `Clone`. `Rc<AssetResolver>` is `Clone`.)

- [ ] **Step 2: Wire the resolver into `SceneBuilder` (script.rs)**

In `crates/carapace/src/script.rs`: the `SceneBuilder` gains the resolver, and its `BuildContext` impl implements `image`. Update `SceneBuilder`:

```rust
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
    ) -> Result<std::sync::Arc<crate::asset::DecodedImage>, crate::asset::AssetError> {
        self.assets.image(name)
    }
}
```

In `load`, build the `SceneBuilder` with the source's resolver (replace the existing `SceneBuilder { nodes: Vec::new(), handler_fns: Vec::new() }` construction):

```rust
    let builder = Rc::new(RefCell::new(SceneBuilder {
        nodes: Vec::new(),
        handler_fns: Vec::new(),
        assets: source.assets.clone(),
    }));
```

(`source` is the `&SkinSource` param to `load`; its `assets` field is the per-skin resolver.)

- [ ] **Step 3: Migrate inline `SkinSource` construction sites**

Search the crate for `SkinSource {` literals in tests and replace with `SkinSource::inline(...)`. The known sites are the per-file `src()` helpers and inline constructions in: `script.rs` tests, `swap.rs` tests. Example replacement (apply the analogous change everywhere a `SkinSource { lua_src: ..., canvas: ... }` literal appears):

```rust
// before:  SkinSource { lua_src: s.to_string(), canvas: (300, 120) }
// after:
SkinSource::inline(s, (300, 120))
```

Run `cargo build -p carapace --tests 2>&1 | grep -n "missing field \`assets\`"` to find any remaining literal sites and convert each.

- [ ] **Step 4: Run the carapace lib + its in-crate tests**

Run: `cargo test -p carapace --lib`
Expected: PASS (script + swap + all module tests). If integration tests (`tests/`) reference `SkinSource { ... }`, those are migrated in Task 5/6/7's own files; the lib tests should pass here.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/src/command.rs crates/carapace/src/script.rs
git commit -m "feat(carapace): SkinSource carries per-skin AssetResolver; script resolves images"
```

---

### Task 5: `skin` loader resolves assets + headless image skin test

**Files:**
- Modify: `crates/carapace/src/skin.rs`
- Modify: `crates/carapace/tests/*` and `benches/engine.rs` — migrate any `SkinSource { ... }` literals to `SkinSource::inline(...)` (integration crates).

**Interfaces:**
- Produces: `Manifest` gains `#[serde(default = "default_asset_dir")] pub asset_dir: String`; `load_dir` resolves `<dir>/<asset_dir>` into an `AssetResolver` and returns it in the `SkinSource`.

- [ ] **Step 1: Add `asset_dir` to the manifest + resolve in `load_dir`**

In `crates/carapace/src/skin.rs`:

```rust
fn default_asset_dir() -> String {
    "assets".to_string()
}
```

Add to `struct Manifest`:

```rust
    #[serde(default = "default_asset_dir")]
    pub asset_dir: String,
```

In `load_dir`, after computing `canvas` and reading `lua_src`, build the resolver and the source:

```rust
    let assets = std::rc::Rc::new(crate::asset::AssetResolver::resolve(dir, &manifest.asset_dir)?);
    let source = SkinSource { lua_src, canvas, assets };
    Ok((manifest, source))
```

Add `From<crate::asset::AssetError> for SkinError`:

```rust
impl From<crate::asset::AssetError> for SkinError {
    fn from(e: crate::asset::AssetError) -> Self {
        SkinError::Asset(e)
    }
}
```

and the variant:

```rust
    Asset(crate::asset::AssetError),
```

- [ ] **Step 2: Migrate integration-crate `SkinSource` literals**

Run `cargo build -p carapace --tests 2>&1 | grep -n "missing field \`assets\`"` and convert each remaining `SkinSource { lua_src, canvas }` in `crates/carapace/tests/*.rs` and `crates/carapace/benches/engine.rs` to `SkinSource::inline(...)`.

- [ ] **Step 3: Write the headless image-skin test**

Create `crates/carapace/tests/image_skin.rs`:

```rust
use std::path::Path;

use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::scene::Node;
use carapace::vocab::VocabRegistry;

// Builds a temp skin with an assets/ PNG and an `image` node; verifies it builds headlessly.
struct Tmp(std::path::PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_image_skin() -> Tmp {
    let base = std::env::temp_dir().join("carapace-image-skin");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("assets")).unwrap();
    let mut img = image::RgbaImage::new(8, 6);
    img.put_pixel(0, 0, image::Rgba([0, 255, 0, 255]));
    img.save(base.join("assets/face.png")).unwrap();
    std::fs::write(
        base.join("skin.toml"),
        "schema=1\nid='img'\nname='img'\nengine='^0.1'\ncanvas={width=100,height=80}\nentry='skin.lua'\n",
    )
    .unwrap();
    std::fs::write(base.join("skin.lua"), "image{ asset='face.png', x=0, y=0 }\n").unwrap();
    Tmp(base)
}

#[test]
fn image_skin_builds_headlessly() {
    let t = temp_image_skin();
    let (_m, source) = carapace::skin::load_dir(&t.0).unwrap();
    let e = Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), source).unwrap();
    let has_image = e.scene().nodes.iter().any(|n| matches!(n, Node::Image { .. }));
    assert!(has_image, "the skin built an Image node from its asset");
}

#[test]
fn missing_asset_fails_to_build() {
    let base = std::env::temp_dir().join("carapace-image-skin-missing");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("assets")).unwrap();
    std::fs::write(
        base.join("skin.toml"),
        "schema=1\nid='m'\nname='m'\nengine='^0.1'\ncanvas={width=10,height=10}\nentry='skin.lua'\n",
    )
    .unwrap();
    std::fs::write(base.join("skin.lua"), "image{ asset='nope.png', x=0, y=0 }\n").unwrap();
    let (_m, source) = carapace::skin::load_dir(&base).unwrap();
    let r = Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), source);
    assert!(r.is_err(), "missing asset makes the skin fail to build");
    let _ = std::fs::remove_dir_all(&base);
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p carapace`
Expected: PASS — all unit + integration tests, including `image_skin` (2) and the migrated suites.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/src/skin.rs crates/carapace/tests crates/carapace/benches
git commit -m "feat(carapace): skin loader resolves assets; headless image-skin test"
```

---

### Task 6: `render` — draw `Node::Image` (gated GPU test + color sentinel)

> **Spike note:** vello 0.9 image drawing — build a `peniko::Image` from the RGBA8 blob and `vello::Scene::draw_image(&image, transform)`. The exact `peniko::Image` constructor + format/alpha/sRGB knobs may differ in the resolved version; reconcile against vello 0.9's docs/examples. The **color sentinel** assertion is the gate that catches a wrong color space. Reference the existing `render.rs` for the scene-walk + `render_to_texture` structure.

**Files:**
- Modify: `crates/carapace/src/render.rs`
- Modify: `crates/carapace/tests/render_offscreen.rs`

**Interfaces:**
- Consumes: `scene::Node::Image`, `asset::DecodedImage`.

- [ ] **Step 1: Add the `Image` arm to `draw`**

In `crates/carapace/src/render.rs`, inside `draw`'s `match node`, add (after the `ValueFill` arm):

```rust
                Node::Image { image, dest } => {
                    // sRGB RGBA8 blob -> vello image, placed at dest, composed with canvas->surface scale.
                    let blob = vello::peniko::Blob::new(std::sync::Arc::new(image.rgba.clone()));
                    let vimg = vello::peniko::Image::new(
                        blob,
                        vello::peniko::ImageFormat::Rgba8,
                        image.width,
                        image.height,
                    );
                    // scale the native image to dest.w x dest.h, then translate to dest.x,dest.y,
                    // all under the canvas->surface transform.
                    let place = Affine::translate((dest.x as f64, dest.y as f64))
                        * Affine::scale_non_uniform(
                            dest.w as f64 / image.width.max(1) as f64,
                            dest.h as f64 / image.height.max(1) as f64,
                        );
                    vs.draw_image(&vimg, xform * place);
                }
```

> If vello 0.9's `peniko::Image::new` / `ImageFormat` / `Scene::draw_image` signatures differ, reconcile against the resolved vello — the contract is: the image is drawn at `dest` under the canvas→surface scale, sRGB-correct (so the color-sentinel test passes). If vello renders images linearly and the source is sRGB, set the image's sRGB format flag (or convert) so the sentinel matches.

- [ ] **Step 2: Add the gated image-draw + color-sentinel test**

Add to `crates/carapace/tests/render_offscreen.rs` (still under `#![cfg(feature = "gpu-tests")]`):

```rust
#[test]
fn renders_an_image_at_sentinel_pixels() {
    use carapace::asset::DecodedImage;
    use carapace::scene::{ImageDest, Node};
    use std::sync::Arc;

    // 2x2 sRGB image: TL pure red, TR pure green, BL pure blue, BR mid-grey (188 = sRGB ~0.5 linear).
    let rgba = vec![
        255, 0, 0, 255, /*TL*/ 0, 255, 0, 255, /*TR*/
        0, 0, 255, 255, /*BL*/ 188, 188, 188, 255, /*BR*/
    ];
    let img = Arc::new(DecodedImage { rgba, width: 2, height: 2 });
    let o = offscreen(200, 200);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![Node::Image {
            image: img,
            dest: ImageDest { x: 0.0, y: 0.0, w: 200.0, h: 200.0 }, // scale 2x2 -> full 200x200
        }],
    };
    let read = |_k: &str| None;
    r.draw(&scene, read, &RenderTarget {
        device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h,
    });
    let data = readback(&o);
    // Each source texel maps to a 100x100 block; sample block centers.
    assert_eq!(px(&data, 200, 50, 50), [255, 0, 0], "TL red");
    assert_eq!(px(&data, 200, 150, 50), [0, 255, 0], "TR green");
    assert_eq!(px(&data, 200, 50, 150), [0, 0, 255], "BL blue");
    // Color sentinel: a mid-grey sRGB texel must round-trip to ~188 (NOT ~128), proving the
    // pipeline treats the image as sRGB (samples sRGB->linear, composites, returns sRGB).
    let g = px(&data, 200, 150, 150);
    assert!((g[0] as i32 - 188).abs() <= 4, "sRGB grey round-trips to ~188, got {}", g[0]);
}
```

> Image sampling may apply bilinear filtering at block edges; the sampled points are block *centers* (50/150), well inside each 100×100 texel block, so they read the texel's solid color. If vello defaults to a filtering/quality mode that blurs even centers, set the image's nearest/quality knob; the sentinels are exact solid colors by construction.

- [ ] **Step 3: Run the gated test (local Metal)**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS (the prior fill/value_fill test + the new image+sentinel test). If the grey sentinel reads ~128 instead of ~188, the image is being sampled as linear — fix by marking the vello image sRGB (per Step 1's note). Confirm the headless path is still unaffected: `cargo test -p hittest -p carapace`.

- [ ] **Step 4: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/src/render.rs crates/carapace/tests/render_offscreen.rs
git commit -m "feat(carapace): render Node::Image via vello (sRGB), gated image + color-sentinel test"
```

---

### Task 7: demo payoff — the real Headspace bitmap skin

**Files:**
- Move: `crates/carapace-demo/skins/reference/headspace-source.png` → `crates/carapace-demo/skins/reference/assets/headspace.png`
- Modify: `crates/carapace-demo/skins/reference/skin.lua`
- Modify: `crates/carapace-demo/tests/skins_build.rs`

**Interfaces:** none new (skin content + a test threshold).

- [ ] **Step 1: Move the PNG into the resolved asset dir**

```bash
mkdir -p crates/carapace-demo/skins/reference/assets
git mv crates/carapace-demo/skins/reference/headspace-source.png crates/carapace-demo/skins/reference/assets/headspace.png
```

- [ ] **Step 2: Rewrite the reference skin to draw the bitmap + interactive overlays**

Replace `crates/carapace-demo/skins/reference/skin.lua` entirely:

```lua
-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
image{ asset = "headspace.png", x = 0, y = 0 }

-- Invisible interactive overlays on top of the bitmap (positions traced from the artwork):
-- play/pause hotspot over the transport area
region{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}},
        on_press = function() host.toggle_play() end }
-- stop hotspot
region{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}},
        on_press = function() host.stop() end }
-- live seek bar bound to position, over the bitmap's seek groove
value_fill{ path = {{x=78,y=216},{x=264,y=216},{x=264,y=230},{x=78,y=230}},
            value = "position", color = {r=120,g=230,b=80} }
```

- [ ] **Step 3: Update the skins-build test for the bitmap reference skin**

In `crates/carapace-demo/tests/skins_build.rs`, replace the `headspace_reference_builds` test so it asserts the bitmap is present rather than a high node count (the flat-vector octagons are gone):

```rust
#[test]
fn headspace_reference_builds_with_bitmap() {
    use carapace::scene::Node;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/reference");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let e = carapace::engine::Engine::new(
        Box::new(carapace_demo::demo_host::DemoHost::new()),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .unwrap();
    let nodes = &e.scene().nodes;
    assert!(nodes.iter().any(|n| matches!(n, Node::Image { .. })), "draws the headspace bitmap");
    assert!(nodes.iter().any(|n| matches!(n, Node::Hotspot { .. })), "has interactive hotspots");
    assert!(nodes.iter().any(|n| matches!(n, Node::ValueFill { .. })), "has the live seek bar");
}
```

(Adjust the test's imports/helpers to match the file; the `classic`/`minimal` tests are unchanged.)

- [ ] **Step 4: Run the demo's tests + build**

Run: `cargo test -p carapace-demo` and `cargo build -p carapace-demo`
Expected: PASS (DemoHost + classic/minimal + the bitmap reference test); clean build.

- [ ] **Step 5: Smoke-launch (human-verified)**

If a display is available: `cargo run -p carapace-demo`, then **Tab** to the reference skin. Confirm the **real Headspace artwork** renders (green alien head, photo face, speakers — the actual bitmap), the play hotspot toggles, and the seek bar advances over the groove. If headless, rely on the clean build + tests; the human verifies the look. This is the retro-look payoff.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p carapace-demo
git add crates/carapace-demo/skins/reference crates/carapace-demo/tests/skins_build.rs
git commit -m "feat(carapace-demo): reference skin draws the real Headspace bitmap + overlays"
```

---

## Self-Review

**Spec coverage (against the 5a design):**
- §1 generic asset resolver + threading → Tasks 1 (resolver), 4 (SkinSource/script), 5 (skin loader). ✓
- §2 resolver API, `BuildContext::image`, `ImagePrim`, formats, sandbox, `scene::summary` → Tasks 1, 2, 3. Color space → Task 6 (sentinel). ✓
- §3 render image + the Headspace bitmap demo → Tasks 6, 7. ✓
- §4 testing: headless resolver/decode/build (1,3,5), gated GPU image + color sentinel (6), snapshot summary line (2), human run (7). ✓
- Headless boundary, sandbox traversal rejection, transactional-swap on missing asset → Tasks 1, 5. ✓
- `sfw` for the `image` dep → Task 1 Step 1/4 + Global Constraints. ✓

**Placeholder scan:** Task 6's vello image-draw is reference-reconciled (the churny GPU API), with the color sentinel as the contract — same verified-code-directive pattern as every prior GPU task, not unwritten logic. Tasks 4/5 include a `grep "missing field \`assets\`"` discovery step for the mechanical `SkinSource` migration (the sites can't all be hand-enumerated without reading every test, so the grep finds them deterministically). All other code is complete; no TBDs.

**Type consistency:** `DecodedImage`/`AssetResolver`/`AssetError` (Task 1) used in 2,3,4,5,6. `ImageDest`/`Node::Image` (Task 2) in 3,6,7. `BuildContext::image` + `ImagePrim` + `BuildError::Asset` (Task 3) consistent with `SceneBuilder` (Task 4). `SkinSource { lua_src, canvas, assets }` + `SkinSource::inline` (Task 4) used in 5 + the migrations. `VocabRegistry::base()` count 3→4 updated in the Task 3 test.

## Deferred (recorded)

- 5b gradients · 5c text/fonts (reuse this resolver for font bytes) · 5d shapes-helper + shared draw+hotspot + value-fill direction · 5e host-extension mechanism.
- Animation (animated GIF beyond first frame); additional formats (WebP/TIFF — one feature-flag add); ICC/wide-gamut/HDR.

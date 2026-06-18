# Phase 5a — Asset Loading + the `image` Primitive — Design

**Date:** 2026-06-18
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Phase 5 (base vocabulary + host extensions + assets), decomposed — **5a is the
first sub-project**. Builds on the Phase 2 vocab seam and the Phase 3 engine + render.

## Purpose

Add **asset loading** to the engine and an **`image` primitive** as its first consumer — the
keystone for the authentic retro look. Real WMP skins are mostly bitmap faceplates; this one
slice lets a skin draw real artwork (the genuine Headspace skin, not a flat-vector homage),
with hotspots + dynamic elements overlaid.

### Phase 5 decomposition (recorded; 5a is first)

| Sub-project | Adds | Order rationale |
|---|---|---|
| **5a (this doc)** | asset resolver + `image` primitive | keystone for the retro look; biggest visual payoff |
| 5b | gradient fills | Y2K chrome/sheen |
| 5c | text + fonts | labels/numerals (reuses the 5a asset resolver for font files) |
| 5d | vocab ergonomics: `shapes` helper (circle), shared draw+hotspot geometry, value-fill direction | Phase 1 lessons #2–#3 + author DX |
| 5e | host-extension mechanism (`VocabRegistry::register` flow + a demo extension) | exercises the Phase 2 seam |

## Scope

**In scope:** a **type-agnostic asset resolver** (scan a sandboxed asset directory → name→bytes,
cached), an **image decoder** (PNG/JPEG/GIF/BMP → sRGB RGBA8), the **`image` primitive** + a
`Node::Image`, render of images via vello (gamma-correct sRGB pipeline), and upgrading the demo
`reference` skin to draw the real Headspace bitmap.

**Out of scope (later 5x / phases):** gradients (5b), text/fonts (5c) — though the resolver is
built generic so 5c reuses it; vector/region-mask/cursor asset types; **animation** (animated
GIF decodes to its first frame); **image hotspots / shared draw+hotspot geometry** (5d — images
are visual-only in 5a, hotspots stay `region`); ICC color management, wide-gamut, HDR (sRGB
assumed).

## 1. Architecture & threading

The asset layer is **generic**; `image` is its first consumer. The headless/GPU split holds:
**decoding is headless** (the `image` crate, CPU), only the GPU upload is in `render`.

> **Dependency policy:** the new `image` crate (and any third-party package) is added via
> **`sfw cargo add image -p carapace`** and first fetched/built under `sfw` (Socket Firewall
> supply-chain filtering). The plan encodes this; subagents adding deps must use `sfw`.

```
carapace gains the `image` crate (decode — headless).

asset.rs   (new) # AssetResolver: scan a sandboxed dir → name→path index; bytes(name) (raw,
                 #   cached, type-agnostic) + image(name) (decode → sRGB RGBA8, cached)
skin.rs          # manifest gains optional `asset_dir` (default "assets"); load_dir RESOLVES it
                 #   into an AssetResolver
command.rs       # SkinSource carries Rc<AssetResolver> alongside lua_src + canvas
vocab.rs         # BuildContext gains `image(name) -> Result<Arc<DecodedImage>, AssetError>`;
                 #   new ImagePrim (the `image` constructor)
scene.rs         # new Node::Image { image: Arc<DecodedImage>, dest: ImageDest }; summary() line
script.rs        # threads the AssetResolver into the SceneBuilder so ImagePrim can resolve
render.rs        # draws Node::Image via vello (peniko::Image -> Scene::draw_image), sRGB-correct
```

**Per-skin lifecycle:** `load_dir` scans `<skin>/<asset_dir>` once → an `AssetResolver`
(name→path, sandboxed; traversal rejected). At build, `ImagePrim` calls `ctx.image("face.png")`
→ resolver reads bytes (cached) → decodes to `DecodedImage { rgba, w, h }` (cached) → a
`Node::Image` carries an `Arc<DecodedImage>` + dest rect. On swap, the old skin's resolver +
caches drop. Assets are sandboxed to the skin dir; the Lua sandbox is unchanged (a skin only
*names* assets; the engine resolves them — no filesystem access from Lua).

**Headless boundary intact:** `Node::Image` holds **decoded RGBA** (CPU) — existing headless
skin-build tests build image scenes with no GPU; `render.rs` is the only GPU step.

## 2. Asset resolver, `BuildContext`, `image` primitive

```rust
// scene.rs
pub struct DecodedImage { pub rgba: Vec<u8>, pub width: u32, pub height: u32 } // straight-alpha sRGB RGBA8
pub struct ImageDest { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
pub enum Node {
    Fill { .. }, Hotspot { .. }, ValueFill { .. },
    Image { image: std::sync::Arc<DecodedImage>, dest: ImageDest },   // NEW
}

// asset.rs — generic, type-agnostic, sandboxed to the skin dir
pub struct AssetResolver { /* index: HashMap<String,PathBuf>, byte_cache, image_cache (interior-mut) */ }
impl AssetResolver {
    pub fn resolve(skin_dir: &Path, asset_dir: &str) -> Result<Self, AssetError>; // scan; reject traversal
    pub fn bytes(&self, name: &str) -> Result<Arc<[u8]>, AssetError>;             // raw, cached (any type)
    pub fn image(&self, name: &str) -> Result<Arc<DecodedImage>, AssetError>;     // decode -> sRGB RGBA8, cached
}
pub enum AssetError { Unresolved(String), Io(String), Decode(String) }

// vocab.rs
pub trait BuildContext {
    fn register_handler(&mut self, f: Function) -> HandlerId;
    fn image(&mut self, name: &str) -> Result<std::sync::Arc<DecodedImage>, AssetError>;
}
```

The `image` Lua constructor (via `ImagePrim`):
```lua
image{ asset = "headspace.png", x = 0, y = 0 }                  -- native size at (0,0)
image{ asset = "logo.png", x = 156, y = 236, w = 30, h = 20 }  -- scaled into 30×20
```
`ImagePrim::build`: read `asset` (string) → `ctx.image(name)` → `Arc<DecodedImage>`; read `x,y`
and optional `w,h` (default to native size); push `Node::Image { image, dest }`. A missing or
undecodable asset → `BuildError` → caught by the **transactional swap** (skin fails to load;
prior scene stays).

**Asset reference model (Flutter-style, directory-resolved):** the skin declares (or defaults
to) an asset **directory**; the loader **resolves** it by scanning — no per-file declaration —
and a file is usable **iff** it was resolved. Scan **recursively**, keying by path relative to
the root (`image{ asset = "wings/speaker.png" }` works). Decode **lazily on first use**, cached.
"Resolved = usable" is the sandbox boundary; referencing an unresolved name → `Unresolved`.

**Image formats:** decode via the `image` crate's `load_from_memory` (content-detected, not by
extension) → `to_rgba8()`. Enabled set for 5a: **PNG, JPEG, GIF, BMP** (the formats WMP skins
use). PNG/GIF alpha preserved; JPEG/BMP opaque. Animated GIF → **first frame only** (animation
deferred). Unsupported/corrupt → `AssetError::Decode`.

**Color space:** **sRGB assumed, no color management** (ICC profiles ignored; the `image` crate
gives raw stored values). Internal representation is straight-alpha sRGB RGBA8. The render
pipeline must be **gamma-correct**: image textures sampled as sRGB (sRGB→linear on sample, blend
in linear), with the vello image format + intermediate texture + surface kept sRGB-consistent so
fills and photos share one correct color path. (The exact vello-0.9 knobs are verified during
implementation; a render-time color sentinel guards against regressions.) Wide-gamut/HDR out of
scope.

**`scene::summary`** gains a domain-neutral line for the snapshot harness:
`image <w>x<h> at <x>,<y> dest <w>x<h>` (dimensions + position, never pixel data).

## 3. Render & the demo payoff

**Render:** `Node::Image` → `vello::peniko::Image` from the `Arc<DecodedImage>` (sRGB RGBA8) →
`vello::Scene::draw_image` at a transform placing it at `dest`, composed with the canvas→surface
scale (same scale fills use). sRGB-correct pipeline per §2.

**Demo payoff — Headspace becomes real.** The `reference` skin is upgraded to draw the actual
`headspace.png` as the faceplate, with the existing interactive overlays on top:
```lua
image{ asset = "headspace.png", x = 0, y = 0 }   -- the genuine WMP artwork, native 342×394
region{ path = {..play..},  on_press = function() host.toggle_play() end }
region{ path = {..stop..},  on_press = function() host.stop() end }
value_fill{ path = {..seek..}, value = "position", color = {..} }
```
This is how real WMP skins work — a bitmap faceplate + hotspot regions + a few dynamic overlays.
The flat-vector octagons are removed; the demo shows the **genuine Headspace skin with live
play/seek**. The source PNG moves into `crates/carapace-demo/skins/reference/assets/headspace.png`
(a drawn asset). That is the retro look delivered, and the visible proof 5a works.

## 4. Testing

**Headless (no GPU):**
- `asset` — resolve/scan builds the index; **traversal rejection** (a `../` or absolute escape
  errors); recursive keying; `bytes` caches; unresolved name → `Unresolved`.
- image decode — bytes→RGBA8 with correct dims for PNG (+ at least one of JPEG/GIF/BMP via a
  tiny fixture); corrupt bytes → `Decode`; animated GIF → first frame.
- `ImagePrim::build` — native dest (x,y + native w,h) vs scaled (explicit w,h); missing asset →
  `BuildError`.
- `scene::summary` — the `image …` line.
- a **skin-build test** that builds the bitmap Headspace `reference` scene headlessly (decode on
  CPU; asserts a `Node::Image` is present + the overlay nodes).

**Gated GPU (`gpu-tests` feature; lavapipe CI / local Metal):**
- the `render_offscreen` test gains an **image-draw case** (draw a known small image, read back,
  assert sentinel pixels match the source) **+ a color sentinel** (a known sRGB pixel asserted
  at the right value — catches gamma mistakes).

**Snapshot harness:** continues to work via the `image …` summary line (no RGBA hashing).

**Human:** `cargo run -p carapace-demo`, Tab to the `reference` skin → the real Headspace renders;
click plays; the seek bar advances and survives a `Tab` swap.

## Error handling

- Asset resolve failure (bad/escaping dir) → `SkinError` at load (no panic).
- Missing/undecodable asset at build → `BuildError` → transactional swap keeps the prior scene.
- Engine/render return `Result` / degrade; no panic on a skin/asset fault. `unwrap` only on engine
  invariants.

## Definition of done (5a)

The asset resolver + `image` primitive exist; a skin can draw bitmaps (PNG/JPEG/GIF/BMP) from a
sandboxed, directory-resolved `assets/` dir; render is sRGB-gamma-correct (color sentinel passes);
the demo `reference` skin renders the **real Headspace bitmap** with live play/seek; the headless
boundary, the fast `check` CI job, and the snapshot harness are all unchanged/green.

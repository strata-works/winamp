# Phase 5c — Text + Fonts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `text{}` vocabulary primitive that lays out and renders real vector-font text — static labels and strings bound to host state — with multi-line wrapping and 2-D (halign × valign) anchoring, filled by the 5b `Paint`.

**Architecture:** Plain-data `Node::Text` in `scene.rs` (headless), parsing in `vocab.rs` (headless), and parley text shaping behind the GPU seam in `render.rs` (layout at render time, after any bound state key resolves — mirroring `value_fill`). Fonts flow through the existing 5a `AssetResolver`; system fonts provide fallback. A render-side layout cache means unchanged text is never re-shaped per frame.

**Tech Stack:** Rust (edition 2024), `vello` 0.9 / `wgpu` 29 (peniko 0.6.1, skrifa 0.42.1), `parley` for text layout, `mlua` for the skin script, `insta` for snapshots.

**Spec:** `docs/superpowers/specs/2026-06-20-phase5c-text-fonts-design.md`

## Global Constraints

- Rust edition 2024; builds against Rust 1.96. CI builds `--locked`; keep `Cargo.lock` committed and updated.
- **New third-party deps are added via `sfw cargo add <crate> -p carapace`** (Socket Firewall supply-chain filtering) — applies to `parley` in Task 5.
- **Single `peniko` in the tree.** parley must resolve to a version whose transitive `peniko`/`skrifa` unify with vello 0.9.0's (`peniko` 0.6.1, `skrifa` 0.42.1). Verify with `cargo tree -d -p carapace | grep -E 'peniko|skrifa'` showing one version each. If duplicated, pin parley (`sfw cargo add parley@<ver> -p carapace`) to the release that shares peniko 0.6.
- **`scene::summary()` stays domain-neutral and geometry-free** — never glyph metrics, `pos`, or `max_width`; bound text shows the *key*, not a resolved value. Snapshot tests depend on this for cross-platform stability under system fallback.
- The bundled demo/test font must be permissively licensed (OFL/Apache); ship its license file alongside the `.ttf`.
- All git commits use identity **Daniel Agbemava <danagbemava@gmail.com>** (commands below include the `-c` flags). Never add Claude attribution to commits or PRs.
- GPU tests run under the `gpu-tests` feature (macOS Metal locally / lavapipe on Linux CI); headless tests must not require a GPU.

---

## File Structure

- `crates/carapace/src/state.rs` — `StateValue` gains `Str`; enum becomes `Clone` (was `Copy`).
- `crates/carapace/src/scene.rs` — new `TextContent`, `HAlign`, `VAlign`, `FontData`; `Node::Text`; `summary()` arm.
- `crates/carapace/src/asset.rs` — `AssetResolver::font()` + `font_cache`.
- `crates/carapace/src/vocab.rs` — `BuildContext::font()`; `TextPrim`; register in `base()`.
- `crates/carapace/src/script.rs` — `SceneBuilder::font()` plumbing.
- `crates/carapace/src/render.rs` — parley `FontContext`/`LayoutContext` + layout cache on `Renderer`; draw `Node::Text`.
- `crates/carapace/tests/render_offscreen.rs` — GPU text sentinels (append).
- `crates/carapace/tests/fonts/` — committed test font (`vt323.ttf` + `OFL.txt`).
- `crates/carapace-demo/src/demo_host.rs` — `track_title` string state.
- `crates/carapace-demo/skins/reference/` — `text{}` accents + bundled font; `skins/minimal/` wrapped fallback label.
- `crates/carapace-demo/tests/skins_build.rs` — assert a `Text` node builds.
- `README.md` — roadmap refresh (5b done, 5c done).

---

## Task 1: `StateValue::Str` — string state

**Files:**
- Modify: `crates/carapace/src/state.rs:1-5`
- Modify: `crates/carapace/src/render.rs:93-99` (`value_of` — confirm it ignores `Str`)
- Test: `crates/carapace/src/state.rs` (new `#[cfg(test)] mod tests`), `crates/carapace/src/render.rs` (new `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `enum StateValue { Bool(bool), Scalar(f32), Str(std::sync::Arc<str>) }`, now `Clone + PartialEq + Debug` (no longer `Copy`). `Host::get(&self, key) -> Option<StateValue>` is unchanged in signature.

- [ ] **Step 1: Write the failing test (state.rs)**

Append to `crates/carapace/src/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn str_value_constructs_clones_and_compares() {
        let a = StateValue::Str(Arc::from("hello"));
        let b = a.clone();
        assert_eq!(a, b);
        match b {
            StateValue::Str(s) => assert_eq!(&*s, "hello"),
            _ => panic!("expected Str"),
        }
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p carapace state::tests::str_value_constructs_clones_and_compares`
Expected: FAIL — `no variant named Str` / `StateValue` does not implement `Clone` for `Arc<str>` under `Copy`.

- [ ] **Step 3: Implement the enum change**

Replace the whole of `crates/carapace/src/state.rs:1-5` with:

```rust
#[derive(Clone, PartialEq, Debug)]
pub enum StateValue {
    Bool(bool),
    Scalar(f32),
    Str(std::sync::Arc<str>),
}
```

- [ ] **Step 4: Confirm `value_of` already ignores `Str` and add a guard test**

`render.rs:93-99` `value_of` matches `Scalar`/`Bool(true)` and falls through `_ => 0.0`, so `Str` clamps to 0 with no change needed. Add a test module at the end of `crates/carapace/src/render.rs` (above is GPU-bearing, but `value_of` is pure CPU):

```rust
#[cfg(test)]
mod tests {
    use super::value_of;
    use crate::state::StateValue;
    use std::sync::Arc;

    #[test]
    fn value_of_ignores_string_state() {
        let read = |_: &str| Some(StateValue::Str(Arc::from("not a number")));
        assert_eq!(value_of(&read, "k"), 0.0);
    }
}
```

- [ ] **Step 5: Run the tests and the whole crate to catch Copy-dependent sites**

Run: `cargo test -p carapace --lib`
Expected: PASS. If any site fails to compile because it relied on `StateValue: Copy`, clone at that site (none are expected outside `render::value_of`, which already moves the value).

- [ ] **Step 6: Commit**

```bash
git add crates/carapace/src/state.rs crates/carapace/src/render.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(state): StateValue::Str for value-bound text"
```

---

## Task 2: Scene data model + `summary()` for text

**Files:**
- Modify: `crates/carapace/src/scene.rs:48-77` (types + `Node`), `:113-130` (`summary()` arm)
- Test: `crates/carapace/src/scene.rs` (tests mod at `:147`)

**Interfaces:**
- Consumes: `Paint`, `Pt` (existing in scene.rs).
- Produces:
  ```rust
  pub enum TextContent { Static(String), Bound(String) }
  pub enum HAlign { Left, Center, Right }   // Clone, Copy, Debug, PartialEq
  pub enum VAlign { Top, Middle, Bottom }   // Clone, Copy, Debug, PartialEq
  pub struct FontData { pub bytes: std::sync::Arc<[u8]> }   // Debug
  // Node variant:
  Node::Text {
      content: TextContent,
      font: Option<std::sync::Arc<FontData>>,
      size: f32,
      paint: Paint,
      halign: HAlign,
      valign: VAlign,
      max_width: Option<f32>,
      pos: Pt,
  }
  ```
- `summary()` lines (geometry-free):
  - Static: `text "<string>" font=<name|system> size=<n> halign=<h> valign=<v> <paint>`
  - Bound:  `text value=<key> font=<name|system> size=<n> halign=<h> valign=<v> <paint>`
  - `<paint>`: `rgba=r,g,b,a` (solid) or `gradient=<kind> stops=<n>` (gradient). `<n>` is integer size. `<h>` ∈ left|center|right, `<v>` ∈ top|middle|bottom.
  - **`FontData` carries no name** (only bytes). The summary's `font=` field needs the asset name. So `Node::Text` also stores the name for the summary: add `font_name: Option<String>` to the variant (the resolved-or-`None` author name; `None` → prints `system`). Keep `font: Option<Arc<FontData>>` for the bytes.

- [ ] **Step 1: Write the failing test**

In `crates/carapace/src/scene.rs` tests mod, add:

```rust
#[test]
fn summary_describes_text_nodes() {
    let scene = Scene {
        canvas: (200, 50),
        nodes: vec![
            Node::Text {
                content: TextContent::Static("HI".to_string()),
                font: None,
                font_name: Some("vt323.ttf".to_string()),
                size: 18.0,
                paint: Paint::Solid(Color { r: 1, g: 2, b: 3, a: 255 }),
                halign: HAlign::Center,
                valign: VAlign::Top,
                max_width: None,
                pos: Pt { x: 40.0, y: 8.0 },
            },
            Node::Text {
                content: TextContent::Bound("track_title".to_string()),
                font: None,
                font_name: None,
                size: 12.0,
                paint: Paint::Gradient(Gradient::Linear {
                    from: Pt { x: 0.0, y: 0.0 },
                    to: Pt { x: 0.0, y: 12.0 },
                    stops: vec![
                        ColorStop { at: 0.0, color: Color { r: 0, g: 0, b: 0, a: 255 } },
                        ColorStop { at: 1.0, color: Color { r: 9, g: 9, b: 9, a: 255 } },
                    ],
                }),
                halign: HAlign::Right,
                valign: VAlign::Middle,
                max_width: Some(120.0),
                pos: Pt { x: 200.0, y: 30.0 },
            },
        ],
    };
    assert_eq!(
        scene.summary(),
        "canvas 200x50\n\
         text \"HI\" font=vt323.ttf size=18 halign=center valign=top rgba=1,2,3,255\n\
         text value=track_title font=system size=12 halign=right valign=middle gradient=linear stops=2"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p carapace scene::tests::summary_describes_text_nodes`
Expected: FAIL — `no variant named Text`, missing types.

- [ ] **Step 3: Add the types**

Insert after `Paint` (after `crates/carapace/src/scene.rs:46`):

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum TextContent {
    Static(String),
    Bound(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug)]
pub struct FontData {
    pub bytes: std::sync::Arc<[u8]>,
}
```

- [ ] **Step 4: Add the `Node::Text` variant**

In `enum Node` (`crates/carapace/src/scene.rs:57-77`), add after the `Image { .. }` arm:

```rust
    Text {
        content: TextContent,
        font: Option<std::sync::Arc<FontData>>,
        font_name: Option<String>,
        size: f32,
        paint: Paint,
        halign: HAlign,
        valign: VAlign,
        max_width: Option<f32>,
        pos: Pt,
    },
```

- [ ] **Step 5: Add the `summary()` arm**

In `summary()` (`crates/carapace/src/scene.rs`), add a match arm after the `Node::Image` arm (before the closing of the `match node`):

```rust
                Node::Text {
                    content,
                    font_name,
                    size,
                    paint,
                    halign,
                    valign,
                    ..
                } => {
                    let head = match content {
                        TextContent::Static(s) => format!("text \"{s}\""),
                        TextContent::Bound(k) => format!("text value={k}"),
                    };
                    let font = font_name.as_deref().unwrap_or("system");
                    let h = match halign {
                        HAlign::Left => "left",
                        HAlign::Center => "center",
                        HAlign::Right => "right",
                    };
                    let v = match valign {
                        VAlign::Top => "top",
                        VAlign::Middle => "middle",
                        VAlign::Bottom => "bottom",
                    };
                    let paint_s = match paint {
                        Paint::Solid(c) => format!("rgba={},{},{},{}", c.r, c.g, c.b, c.a),
                        Paint::Gradient(g) => {
                            let (kind, n) = match g {
                                Gradient::Linear { stops, .. } => ("linear", stops.len()),
                                Gradient::Radial { stops, .. } => ("radial", stops.len()),
                                Gradient::Sweep { stops, .. } => ("sweep", stops.len()),
                            };
                            format!("gradient={kind} stops={n}")
                        }
                    };
                    format!("{head} font={font} size={} halign={h} valign={v} {paint_s}", *size as i64)
                }
```

- [ ] **Step 6: Run the test**

Run: `cargo test -p carapace scene::tests::summary_describes_text_nodes`
Expected: PASS. Also run `cargo test -p carapace --lib` to confirm existing summary tests still pass (they don't use `Text`, so unaffected).

- [ ] **Step 7: Commit**

```bash
git add crates/carapace/src/scene.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(scene): Node::Text data model + summary line"
```

---

## Task 3: `AssetResolver::font()` + `BuildContext::font()`

**Files:**
- Modify: `crates/carapace/src/asset.rs:22-26` (add `font_cache`), `:49-72` (init it), add `font()` after `image()` (`:110`)
- Modify: `crates/carapace/src/vocab.rs:22-28` (`BuildContext` trait), and the three test `BuildContext` impls (`:251-262`, `:336-350`, `:480-491`)
- Modify: `crates/carapace/src/script.rs:40-51` (`SceneBuilder` impl)
- Test: `crates/carapace/src/asset.rs` tests mod

**Interfaces:**
- Produces: `AssetResolver::font(&self, name: &str) -> Result<Arc<crate::scene::FontData>, AssetError>` (raw bytes, cached); `BuildContext::font(&mut self, name: &str) -> Result<Arc<crate::scene::FontData>, AssetError>`.

- [ ] **Step 1: Write the failing test**

In `crates/carapace/src/asset.rs` tests mod, extend `temp_skin` to also write a fake font file and add a test. First add this line inside `temp_skin` after the `not_an_image.txt` write (`:133`):

```rust
        std::fs::write(base.join("assets/face.ttf"), b"\x00\x01\x00\x00FAKEFONTBYTES").unwrap();
```

Then add:

```rust
    #[test]
    fn font_returns_raw_bytes_and_caches() {
        let t = temp_skin("font");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        let a = r.font("face.ttf").unwrap();
        let b = r.font("face.ttf").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "second font read hits the cache");
        assert_eq!(&a.bytes[0..4], &[0x00, 0x01, 0x00, 0x00]);
        assert!(matches!(r.font("nope.ttf"), Err(AssetError::Unresolved(_))));
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p carapace asset::tests::font_returns_raw_bytes_and_caches`
Expected: FAIL — no method `font`.

- [ ] **Step 3: Add the `font_cache` field**

In `struct AssetResolver` (`crates/carapace/src/asset.rs:22-26`), add a field:

```rust
    font_cache: RefCell<HashMap<String, Arc<crate::scene::FontData>>>,
```

Initialise it in all three constructors that build the struct literal (`empty()` `:51-57`, `resolve()` `:67-72`) by adding `font_cache: RefCell::new(HashMap::new()),`.

- [ ] **Step 4: Implement `font()`**

Add after `image()` (`crates/carapace/src/asset.rs:110`):

```rust
    pub fn font(&self, name: &str) -> Result<Arc<crate::scene::FontData>, AssetError> {
        if let Some(f) = self.font_cache.borrow().get(name) {
            return Ok(f.clone());
        }
        let bytes = self.bytes(name)?;
        let font = Arc::new(crate::scene::FontData { bytes });
        self.font_cache
            .borrow_mut()
            .insert(name.to_string(), font.clone());
        Ok(font)
    }
```

- [ ] **Step 5: Add `font()` to the `BuildContext` trait and all impls**

In `crates/carapace/src/vocab.rs` trait (`:22-28`), add:

```rust
    fn font(
        &mut self,
        name: &str,
    ) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError>;
```

In `script.rs` `SceneBuilder` impl (after the `image` method, `:50`):

```rust
    fn font(
        &mut self,
        name: &str,
    ) -> Result<Arc<crate::scene::FontData>, crate::asset::AssetError> {
        self.assets.font(name)
    }
```

In the three test `BuildContext` impls in `vocab.rs` (`NoHandlers` `:251`, `Counter` `:336`, `Ctx` `:480`), add to each — for `NoHandlers` and `Counter` (which return errors for assets):

```rust
        fn font(
            &mut self,
            name: &str,
        ) -> Result<std::sync::Arc<crate::scene::FontData>, crate::asset::AssetError> {
            Err(crate::asset::AssetError::Unresolved(name.to_string()))
        }
```

For `Ctx` (Task 3 doesn't need it to succeed) add the same error-returning body.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p carapace asset::tests::font_returns_raw_bytes_and_caches && cargo test -p carapace --lib`
Expected: PASS (all impls satisfy the trait).

- [ ] **Step 7: Commit**

```bash
git add crates/carapace/src/asset.rs crates/carapace/src/vocab.rs crates/carapace/src/script.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(asset): AssetResolver::font + BuildContext::font plumbing"
```

---

## Task 4: `TextPrim` — the `text{}` constructor

**Files:**
- Modify: `crates/carapace/src/vocab.rs` (add `parse_halign`/`parse_valign`, `TextPrim`, register in `base()` `:235-242`, update `base_registry_now_has_four` `:375-377`)
- Test: `crates/carapace/src/vocab.rs` tests mod

**Interfaces:**
- Consumes: `BuildContext::font` (Task 3), `parse_paint` (existing `:137`), `scene::{TextContent, HAlign, VAlign, Node}`.
- Produces: `text{}` Lua constructor building `Node::Text`. Param rules: exactly one of `text=`(Static) / `value=`(Bound); optional `font=` name → `Some(FontData)` + `font_name`; `size=` default 16; `color=`/`gradient=` via `parse_paint`; `halign=` default `"left"`; `valign=` default `"top"`; required `x`,`y`; optional `max_width`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/carapace/src/vocab.rs` tests mod:

```rust
    #[test]
    fn text_prim_builds_static_with_defaults() {
        use crate::scene::{HAlign, TextContent, VAlign};
        let lua = Lua::new();
        let t = tbl(&lua, "return { text='HI', x=5, y=6, color={r=1,g=2,b=3} }");
        match TextPrim.build(&t, &mut NoHandlers).unwrap() {
            Node::Text {
                content, size, halign, valign, font, font_name, max_width, pos, ..
            } => {
                assert_eq!(content, TextContent::Static("HI".to_string()));
                assert_eq!(size, 16.0);
                assert_eq!(halign, HAlign::Left);
                assert_eq!(valign, VAlign::Top);
                assert!(font.is_none() && font_name.is_none());
                assert_eq!(max_width, None);
                assert_eq!((pos.x, pos.y), (5.0, 6.0));
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn text_prim_builds_bound_with_alignment_and_wrap() {
        use crate::scene::{HAlign, TextContent, VAlign};
        let lua = Lua::new();
        let t = tbl(
            &lua,
            "return { value='track_title', x=0, y=0, color={r=0,g=0,b=0}, \
               halign='right', valign='middle', size=12, max_width=120 }",
        );
        match TextPrim.build(&t, &mut NoHandlers).unwrap() {
            Node::Text { content, halign, valign, max_width, .. } => {
                assert_eq!(content, TextContent::Bound("track_title".to_string()));
                assert_eq!(halign, HAlign::Right);
                assert_eq!(valign, VAlign::Middle);
                assert_eq!(max_width, Some(120.0));
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn text_prim_content_xor_and_bad_align() {
        let lua = Lua::new();
        let neither = tbl(&lua, "return { x=0, y=0, color={r=0,g=0,b=0} }");
        assert!(matches!(
            TextPrim.build(&neither, &mut NoHandlers),
            Err(BuildError::MissingField("text"))
        ));
        let both = tbl(&lua, "return { text='a', value='b', x=0, y=0, color={r=0,g=0,b=0} }");
        assert!(matches!(
            TextPrim.build(&both, &mut NoHandlers),
            Err(BuildError::BadType(_))
        ));
        let bad_align = tbl(&lua, "return { text='a', x=0, y=0, color={r=0,g=0,b=0}, halign='up' }");
        assert!(matches!(
            TextPrim.build(&bad_align, &mut NoHandlers),
            Err(BuildError::BadType(_))
        ));
    }

    #[test]
    fn base_registry_now_has_five() {
        assert_eq!(VocabRegistry::base().iter().count(), 5);
    }
```

Delete the old `base_registry_now_has_four` test (`:374-377`).

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p carapace vocab::tests::text_prim_builds_static_with_defaults`
Expected: FAIL — no `TextPrim`.

- [ ] **Step 3: Add alignment parse helpers**

Add to `crates/carapace/src/vocab.rs` (after `parse_paint`, `:143`):

```rust
fn parse_halign(args: &Table) -> Result<crate::scene::HAlign, BuildError> {
    use crate::scene::HAlign;
    match args.get::<Option<String>>("halign")?.as_deref() {
        None | Some("left") => Ok(HAlign::Left),
        Some("center") => Ok(HAlign::Center),
        Some("right") => Ok(HAlign::Right),
        Some(_) => Err(BuildError::BadType("halign must be left|center|right")),
    }
}

fn parse_valign(args: &Table) -> Result<crate::scene::VAlign, BuildError> {
    use crate::scene::VAlign;
    match args.get::<Option<String>>("valign")?.as_deref() {
        None | Some("top") => Ok(VAlign::Top),
        Some("middle") => Ok(VAlign::Middle),
        Some("bottom") => Ok(VAlign::Bottom),
        Some(_) => Err(BuildError::BadType("valign must be top|middle|bottom")),
    }
}
```

- [ ] **Step 4: Add `TextPrim`**

Add after `ImagePrim` (`crates/carapace/src/vocab.rs:212`):

```rust
struct TextPrim;
impl Primitive for TextPrim {
    fn id(&self) -> &str {
        "text"
    }
    fn build(&self, args: &Table, ctx: &mut dyn BuildContext) -> Result<Node, BuildError> {
        use crate::scene::TextContent;
        let has_text = args.contains_key("text")?;
        let has_value = args.contains_key("value")?;
        let content = match (has_text, has_value) {
            (true, true) => {
                return Err(BuildError::BadType("text and value are mutually exclusive"));
            }
            (false, false) => return Err(BuildError::MissingField("text")),
            (true, false) => TextContent::Static(args.get("text")?),
            (false, true) => TextContent::Bound(args.get("value")?),
        };
        let (font, font_name) = match args.get::<Option<String>>("font")? {
            Some(name) => (Some(ctx.font(&name).map_err(BuildError::Asset)?), Some(name)),
            None => (None, None),
        };
        let size: f32 = args.get::<Option<f32>>("size")?.unwrap_or(16.0);
        let paint = parse_paint(args)?;
        let halign = parse_halign(args)?;
        let valign = parse_valign(args)?;
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let max_width: Option<f32> = args.get("max_width")?;
        Ok(Node::Text {
            content,
            font,
            font_name,
            size,
            paint,
            halign,
            valign,
            max_width,
            pos: Pt { x, y },
        })
    }
}
```

- [ ] **Step 5: Register it in `base()`**

In `VocabRegistry::base()` (`crates/carapace/src/vocab.rs:235-242`), add after the `ImagePrim` line:

```rust
        r.register(Box::new(TextPrim));
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p carapace vocab::`
Expected: PASS (all four new tests + existing).

- [ ] **Step 7: Commit**

```bash
git add crates/carapace/src/vocab.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(vocab): text{} primitive (static/bound, paint, halign/valign)"
```

---

## Task 5: Render `Node::Text` via parley

> **Parley integration note.** parley's public API shifts across releases. The code below follows parley's canonical "render to vello" pattern (see parley's `vello`/`swash` render examples). When implementing, run `cargo doc --open -p parley` after adding the dep and **align exact method names/signatures** (`ranged_builder`, `register_fonts`, glyph-run accessors, `Alignment` variants) to the resolved version. The TDD GPU test is the source of truth — iterate until it passes.

**Files:**
- Modify: `crates/carapace/Cargo.toml` (add `parley`)
- Modify: `crates/carapace/src/render.rs` (imports, `Renderer` fields + `new`, draw arm, `text_of` helper)
- Create: `crates/carapace/tests/fonts/vt323.ttf`, `crates/carapace/tests/fonts/OFL.txt`
- Test: `crates/carapace/tests/render_offscreen.rs` (append; reuses `offscreen`/`readback`/`px`)

**Interfaces:**
- Consumes: `Node::Text` (Task 2), `paint_brush` (existing `render.rs:40`), `StateValue::Str` (Task 1).
- Produces: text rendered under the canvas→surface transform with 2-D anchor offset and `Paint` fill.

- [ ] **Step 1: Acquire the test font**

Download **VT323 Regular** (SIL Open Font License) from Google Fonts (`https://fonts.google.com/specimen/VT323` → "Get font" → extract `VT323-Regular.ttf`). Save it as `crates/carapace/tests/fonts/vt323.ttf` and save the accompanying `OFL.txt` license to `crates/carapace/tests/fonts/OFL.txt`. (VT323 is a monospace LED-style face — on-theme for the WMP readout and it contains ASCII digits/letters, so the GPU test never needs system fallback.)

- [ ] **Step 2: Add the parley dependency (via sfw)**

Run:
```bash
sfw cargo add parley -p carapace
cargo tree -d -p carapace | grep -E 'peniko|skrifa'
```
Expected: exactly one `peniko 0.6.1` and one `skrifa 0.42.1`. If a second `peniko`/`skrifa` appears, run `sfw cargo add parley@<version> -p carapace` choosing the parley release built on peniko 0.6 until `cargo tree -d` shows a single version. Commit `Cargo.toml`/`Cargo.lock` changes as part of this task's final commit.

- [ ] **Step 3: Write the failing GPU test**

Append to `crates/carapace/tests/render_offscreen.rs`:

```rust
#[test]
fn renders_bundled_font_text_in_fill_color() {
    use carapace::scene::{FontData, HAlign, Node, TextContent, VAlign};
    use std::sync::Arc;

    let font = Arc::new(FontData {
        bytes: Arc::from(include_bytes!("fonts/vt323.ttf").as_slice()),
    });
    let o = offscreen(200, 80);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 80),
        // Big solid-red glyphs near the top-left; bundled font + ASCII => no fallback.
        nodes: vec![Node::Text {
            content: TextContent::Static("HII".to_string()),
            font: Some(font),
            font_name: Some("vt323.ttf".to_string()),
            size: 64.0,
            paint: Paint::Solid(Color { r: 255, g: 0, b: 0, a: 255 }),
            halign: HAlign::Left,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt { x: 4.0, y: 4.0 },
        }],
    };
    r.draw(
        &scene,
        |_k: &str| None,
        &RenderTarget { device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h },
    );
    let data = readback(&o);
    // Scan the top-left band where the glyphs sit; at least some pixels must be red-dominant
    // (glyph ink), proving the font loaded and drew in the fill color.
    let mut red_ink = 0;
    for y in 4..70 {
        for x in 4..150 {
            let p = px(&data, 200, x, y);
            if p[0] > 180 && p[1] < 60 && p[2] < 60 {
                red_ink += 1;
            }
        }
    }
    assert!(red_ink > 80, "expected red glyph ink, found {red_ink} px");
}

#[test]
fn renders_value_bound_text_from_string_state() {
    use carapace::scene::{FontData, HAlign, Node, TextContent, VAlign};
    use carapace::state::StateValue;
    use std::sync::Arc;

    let font = Arc::new(FontData {
        bytes: Arc::from(include_bytes!("fonts/vt323.ttf").as_slice()),
    });
    let o = offscreen(200, 80);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 80),
        nodes: vec![Node::Text {
            content: TextContent::Bound("title".to_string()),
            font: Some(font),
            font_name: Some("vt323.ttf".to_string()),
            size: 64.0,
            paint: Paint::Solid(Color { r: 0, g: 255, b: 0, a: 255 }),
            halign: HAlign::Left,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt { x: 4.0, y: 4.0 },
        }],
    };
    let read = |k: &str| {
        if k == "title" { Some(StateValue::Str(Arc::from("WW"))) } else { None }
    };
    r.draw(
        &scene,
        read,
        &RenderTarget { device: &o.device, queue: &o.queue, view: &o.view, width: o.w, height: o.h },
    );
    let data = readback(&o);
    let mut green_ink = 0;
    for y in 4..70 {
        for x in 4..150 {
            let p = px(&data, 200, x, y);
            if p[1] > 180 && p[0] < 60 && p[2] < 60 {
                green_ink += 1;
            }
        }
    }
    assert!(green_ink > 80, "expected green ink from bound string state, found {green_ink}");
}
```

- [ ] **Step 4: Run it to verify it fails**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen renders_bundled_font_text_in_fill_color`
Expected: FAIL — `Node::Text` not handled in `render.rs` draw (it currently has no arm; with a non-exhaustive match this is a compile error, so the build fails until Step 6).

- [ ] **Step 5: Add parley state to `Renderer`**

In `crates/carapace/src/render.rs`, add imports near the top:

```rust
use std::collections::HashMap;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontStack, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use vello::Glyph;
```

Change `struct Renderer` (`render.rs:19-21`) and `new` (`:101-113`) to:

```rust
pub struct Renderer {
    inner: vello::Renderer,
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    // Arc<FontData> ptr -> registered parley family name. Keeps system fallback for None fonts.
    families: HashMap<usize, String>,
}
```

In `new`, after building `inner`, return:

```rust
        Self {
            inner,
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            families: HashMap::new(),
        }
```

- [ ] **Step 6: Add the `text_of` helper and the draw arm**

Add near `value_of` (`crates/carapace/src/render.rs:99`):

```rust
fn text_of(read: &impl Fn(&str) -> Option<StateValue>, key: &str) -> String {
    match read(key) {
        Some(StateValue::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}
```

In `draw`, add a `Node::Text` arm to the `match node` (after the `Node::Image` arm). This consumes `&mut self` for `font_cx`/`layout_cx`/`families`, which `draw` already has:

```rust
                Node::Text {
                    content,
                    font,
                    size,
                    paint,
                    halign,
                    valign,
                    max_width,
                    pos,
                    ..
                } => {
                    use crate::scene::{HAlign, TextContent, VAlign};
                    let s = match content {
                        TextContent::Static(s) => s.clone(),
                        TextContent::Bound(k) => text_of(&read_value, k),
                    };
                    if s.is_empty() {
                        continue;
                    }

                    // Register the skin font once (keyed by Arc ptr); None => system default family.
                    let family = font.as_ref().map(|f| {
                        let ptr = std::sync::Arc::as_ptr(f) as *const () as usize;
                        let font_cx = &mut self.font_cx;
                        self.families
                            .entry(ptr)
                            .or_insert_with(|| {
                                let blob = vello::peniko::Blob::new(std::sync::Arc::new(
                                    f.bytes.to_vec(),
                                ));
                                let registered = font_cx.collection.register_fonts(blob, None);
                                let id = registered[0].0;
                                font_cx
                                    .collection
                                    .family_name(id)
                                    .unwrap_or("system-ui")
                                    .to_string()
                            })
                            .clone()
                    });

                    // Build the layout (parley). FontStack chooses the family; absent => default
                    // collection (system fonts) provides the glyphs/fallback.
                    let mut builder =
                        self.layout_cx.ranged_builder(&mut self.font_cx, &s, 1.0, true);
                    builder.push_default(StyleProperty::FontSize(*size));
                    if let Some(fam) = &family {
                        builder.push_default(StyleProperty::FontStack(FontStack::Single(
                            parley::FontFamily::Named(std::borrow::Cow::Owned(fam.clone())),
                        )));
                    }
                    let mut layout = builder.build(&s);
                    layout.break_all_lines(*max_width);
                    let align = match halign {
                        HAlign::Left => Alignment::Start,
                        HAlign::Center => Alignment::Middle,
                        HAlign::Right => Alignment::End,
                    };
                    let block_w = max_width.unwrap_or(layout.width());
                    layout.align(Some(block_w), align, AlignmentOptions::default());

                    // 2-D anchor offset from the block's measured size.
                    let off_x = match halign {
                        HAlign::Left => 0.0,
                        HAlign::Center => -block_w / 2.0,
                        HAlign::Right => -block_w,
                    };
                    let block_h = layout.height();
                    let off_y = match valign {
                        VAlign::Top => 0.0,
                        VAlign::Middle => -block_h / 2.0,
                        VAlign::Bottom => -block_h,
                    };
                    let origin = Affine::translate((
                        (pos.x + off_x) as f64,
                        (pos.y + off_y) as f64,
                    ));
                    let brush = paint_brush(paint);

                    for line in layout.lines() {
                        for item in line.items() {
                            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                                continue;
                            };
                            let run = glyph_run.run();
                            let mut gx = glyph_run.offset();
                            let gy = glyph_run.baseline();
                            let glyphs = glyph_run.glyphs().map(move |g| {
                                let gl = Glyph {
                                    id: g.id as u32,
                                    x: gx + g.x,
                                    y: gy - g.y,
                                };
                                gx += g.advance;
                                gl
                            });
                            vs.draw_glyphs(run.font())
                                .font_size(run.font_size())
                                .brush(&brush)
                                .transform(xform * origin)
                                .draw(Fill::NonZero, glyphs);
                        }
                    }
                }
```

- [ ] **Step 7: Run the GPU tests**

Run: `cargo test -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS — `renders_bundled_font_text_in_fill_color`, `renders_value_bound_text_from_string_state`, and the existing fill/image/gradient sentinels. Iterate on parley method names (per the integration note) until green.

- [ ] **Step 8: Confirm headless build still green**

Run: `cargo test -p carapace --lib && cargo build -p carapace`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
# Run from the repo root (where the workspace Cargo.lock lives).
git add Cargo.toml Cargo.lock crates/carapace/Cargo.toml crates/carapace/src/render.rs \
  crates/carapace/tests/render_offscreen.rs crates/carapace/tests/fonts/
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(render): draw Node::Text via parley (font fallback, 2-D anchor, Paint fill)"
```

---

## Task 6: Demo payoff — live readout + bundled font

**Files:**
- Modify: `crates/carapace-demo/src/demo_host.rs:6-9,42-48` (add `track_title`)
- Create: `crates/carapace-demo/skins/reference/assets/vt323.ttf`, `.../assets/OFL.txt`
- Modify: `crates/carapace-demo/skins/reference/skin.lua` (text accents)
- Modify: `crates/carapace-demo/skins/minimal/skin.lua` (wrapped system-fallback label)
- Modify: `crates/carapace-demo/tests/skins_build.rs` (assert a `Text` node)

**Interfaces:**
- Consumes: everything from Tasks 1–5.

- [ ] **Step 1: Write the failing test**

In `crates/carapace-demo/tests/skins_build.rs`, add to the `headspace_reference_builds_with_bitmap` test body (before its closing brace), and add a fresh assertion:

```rust
    assert!(
        nodes.iter().any(|n| matches!(n, Node::Text { .. })),
        "reference skin has a text readout"
    );
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p carapace-demo --test skins_build headspace_reference_builds_with_bitmap`
Expected: FAIL — no `Text` node yet.

- [ ] **Step 3: Add `track_title` string state to the demo host**

In `crates/carapace-demo/src/demo_host.rs`, add the field and initialiser:

```rust
pub struct DemoHost {
    playing: bool,
    position: f32,
    track_title: String,
}
```
```rust
        Self {
            playing: false,
            position: 0.0,
            track_title: "Headspace — Track 01".to_string(),
        }
```
In `get` (`:42-48`), add an arm before `_ => None`:
```rust
            "track_title" => Some(StateValue::Str(std::sync::Arc::from(self.track_title.as_str()))),
```

- [ ] **Step 4: Bundle the font**

Copy the VT323 files from Task 5 into the reference skin:
```bash
cp crates/carapace/tests/fonts/vt323.ttf crates/carapace-demo/skins/reference/assets/vt323.ttf
cp crates/carapace/tests/fonts/OFL.txt   crates/carapace-demo/skins/reference/assets/OFL.txt
```

- [ ] **Step 5: Add text to the reference skin**

Append to `crates/carapace-demo/skins/reference/skin.lua`:

```lua
-- Gradient-chrome title label (static), centered on the header.
text{ text = "HEADSPACE", font = "vt323.ttf", size = 22, x = 171, y = 6, halign = "center",
      gradient = { type = "linear", from = {x=0,y=0}, to = {x=0,y=22},
                   stops = { {at=0, color={r=235,g=245,b=255}},
                             {at=1, color={r=120,g=150,b=210}} } } }
-- Live value-bound readout: the current track title from host state, left-aligned over the display.
text{ value = "track_title", font = "vt323.ttf", size = 16, x = 78, y = 196,
      color = {r = 120, g = 230, b = 80} }
```

- [ ] **Step 6: Add a wrapped, system-fallback label to the minimal skin**

Append to `crates/carapace-demo/skins/minimal/skin.lua` (no `font=` → system fallback; `max_width` → wraps):

```lua
-- Wrapped multi-line label using the system fallback font (no bundled font named).
text{ text = "carapace\nminimal skin", size = 12, x = 8, y = 8, max_width = 120,
      color = {r = 230, g = 230, b = 230} }
```

- [ ] **Step 7: Run the tests + demo host unit tests**

Run: `cargo test -p carapace-demo`
Expected: PASS — `headspace_reference_builds_with_bitmap` now finds a `Text` node; existing demo-host and skin tests still pass.

- [ ] **Step 8: Human smoke check**

Run: `cargo run -p carapace-demo`
Expected: the `reference` skin shows the gradient-chrome "HEADSPACE" label and the live green track-title readout; **Tab** to `minimal` shows the wrapped two-line label; a skin swap preserves state.

- [ ] **Step 9: Commit**

```bash
git add crates/carapace-demo/src/demo_host.rs crates/carapace-demo/skins crates/carapace-demo/tests/skins_build.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(demo): live track-title readout + gradient-chrome label (text{})"
```

---

## Task 7: README + roadmap refresh

**Files:**
- Modify: `README.md` (Roadmap section + Current status + crate table note)

**Interfaces:** none (docs only).

- [ ] **Step 1: Update the roadmap**

In `README.md` Roadmap, change the 5b line to done and the 5c line to done, and fix the stale "Next" marker. Replace the existing 5b/5c bullets with:

```markdown
- **Phase 5b — gradient fills.** ✅ `Paint` (solid + linear/radial/sweep) + color alpha.
- **Phase 5c — text + fonts.** ✅ `text{}` primitive: parley layout, fonts via the asset
  resolver (system fallback), value-bound strings, multi-line wrap, 2-D (halign × valign)
  anchoring, `Paint`-filled (chrome numerals).
- **Phase 5d–5e** — vocab ergonomics (shape helpers, shared draw+hotspot geometry); the
  host-extension registration mechanism.
```

- [ ] **Step 2: Update the Current status blurb**

In the status paragraph near the top and the `Current status` section, update the "As of **Phase 5a**" phrasing to note text: e.g. change the lead-in to "As of **Phase 5c** skins draw real bitmap artwork, gradient chrome, and laid-out text with live value-bound readouts." Adjust the prose sentence describing the demo to mention the live track-title readout.

- [ ] **Step 3: Verify the build + full suite**

Run: `cargo test --workspace && cargo fmt --check`
Expected: PASS. (GPU tests are separate: `cargo test -p carapace --features gpu-tests --test render_offscreen`.)

- [ ] **Step 4: Commit**

```bash
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs: README roadmap/status current through Phase 5c"
```

---

## Self-Review (completed during planning)

**Spec coverage:**
- `StateValue::Str` / Copy→Clone → Task 1. ✅
- `Node::Text` plain-data model (content, font, size, paint, halign, valign, max_width, pos) → Task 2 (added `font_name` for the geometry-free summary). ✅
- `AssetResolver::font` + `BuildContext::font` plumbing → Task 3. ✅
- `text{}` parsing (text xor value; font; size default 16; parse_paint; halign/valign defaults + bad → BadType; x/y; max_width) → Task 4. ✅
- parley layout at render time, font registration + system fallback, layout-from-state, 2-D anchor offset, Paint fill via `draw_glyphs` → Task 5. ✅
- Geometry-free `summary()` (static + bound + paint variants) → Task 2 test. ✅
- GPU tests with bundled font + ASCII (deterministic; solid + value-bound string) → Task 5. ✅
- Demo payoff (gradient-chrome label + live `track_title` + wrapped system-fallback) → Task 6. ✅
- Dependency policy (sfw add parley; single peniko) → Global Constraints + Task 5 Steps 2. ✅
- README current per phase → Task 7. ✅

**Deferred (per spec, intentionally not in any task):** rich text / mixed runs, font weight/italic selection, logical (start/end) alignment + base-direction control, editable text, per-glyph animation, bitmap fonts. RTL/bidi *text* renders for free via parley (no task needed).

**Note on the layout cache:** the spec calls for a render-side layout cache (perf). It is **not** implemented in Task 5's steps to keep the first cut simple and the GPU test honest. If profiling shows per-frame re-shaping is a bottleneck (the perf-priority concern), add a `HashMap<(usize,u32,HAlign,Option<u32>,String), parley::Layout<Brush>>` keyed by `(font ptr, size.to_bits(), halign, max_width.map(f32::to_bits), resolved string)` as a follow-up — `valign` stays excluded (draw offset, not layout input). Flagged here rather than silently dropped.

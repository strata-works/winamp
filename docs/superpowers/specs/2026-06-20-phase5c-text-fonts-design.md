# Phase 5c — Text + Fonts — Design

**Date:** 2026-06-20
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Phase 5 (base vocabulary + host extensions + assets), decomposed — **5c is the
third sub-project**, after 5a (asset loading + `image`) and 5b (`Paint` + gradients). Builds on
the Phase 2 vocab seam, the Phase 3 engine + render, the 5a `AssetResolver`, and the 5b `Paint`.

## Purpose

Add **text rendering** to the engine: a `text{}` primitive that lays out and draws real
vector-font text — static labels **and** strings bound to host state (a live track title, a
time readout) — with multi-line wrapping and alignment. This is the labels/numerals layer the
WMP look needs (the track name, the time display), and it completes the base visual vocabulary
alongside `fill`/`value_fill`/`image`.

Text is a first-class primitive, not a one-off: it reuses the **5a `AssetResolver`** for font
files and the **5b `Paint`** for its fill, so text can be solid- or gradient-filled (chrome
numerals) with no new fill machinery.

### Phase 5 decomposition (recorded; 5c is third)

| Sub-project | Adds | Status |
|---|---|---|
| 5a | asset resolver + `image` primitive | done |
| 5b | `Paint` (solid + linear/radial/sweep gradient) + color alpha | done |
| **5c (this doc)** | `text{}` primitive: parley layout, fonts via the resolver, value-bound strings, `Paint` fill | this spec |
| 5d | vocab ergonomics: shape helper, shared draw+hotspot geometry, value-fill direction | later |
| 5e | host-extension mechanism | later |

## Scope

**In scope:**
- A **`StateValue::Str`** variant so host state can carry strings (the only non-rendering
  change; `StateValue` becomes `Clone`, no longer `Copy`).
- A **`text{}`** primitive + `Node::Text` carrying plain inputs (content, font, size, paint,
  align, optional wrap-width, position) — no glyph geometry in the scene.
- **Multi-line layout** (explicit `\n` and wrap at `max_width`) and **alignment**
  (left/center/right) via **parley**.
- **Value-bound text:** `value="<key>"` resolves a string state key at render, mirroring
  `value_fill` — the scene binds the key, never the value.
- **Fonts via the 5a resolver:** `AssetResolver::font(name)` returns raw font bytes (cached,
  sandboxed). Bundled font preferred; parley provides **system fallback** for missing glyphs or
  when no font is named.
- **`Paint` fill:** text reuses 5b `Paint` (solid or gradient).
- A domain-neutral, snapshot-stable `summary()` line.
- A demo payoff: the `reference` skin gains a gradient-chrome label + a live value-bound readout.

**Out of scope (later 5x / phases):**
- **Rich text** — mixed styles/runs within one `text{}`, inline color/size changes.
- **Font style selection** — weight/italic/variation axes beyond the bundled font file (one
  `text{}` names one font file).
- **Complex-script / bidi tuning** beyond what parley gives for free; no RTL-specific work.
- **Editable text / caret / selection.**
- **Per-glyph or value-driven text animation** (string updates on rebuild are fine; animated
  glyph effects are not).
- **Bitmap (sprite-sheet) fonts** — vector fonts only (the classic WMP number-strip is a future
  option, not 5c).

## 1. Architecture & invariants

The headless/GPU split holds exactly as in 5a/5b. **Plain data** lives in `scene.rs`; **parsing**
is headless in `vocab.rs`; **only `render.rs` touches the GPU** — and text shaping (parley, CPU)
lives behind that render seam, run at draw time after any state key is resolved.

- **Headless boundary intact.** `Node::Text` carries only plain inputs (strings, a font-bytes
  `Arc`, numbers, a `Paint`). Existing headless skin-build/scene tests construct text nodes with
  no GPU and no font shaping. parley is constructed only inside the renderer.
- **Scene = pure projection of state.** Value-bound text **binds a key, never a value**; the
  string is resolved at render (like `value_fill`'s scalar). Static text binds nothing. The
  "scene is rebuilt from state; never the reverse" rule is untouched.
- **Layout at render time (chosen architecture).** Layout is a function of
  `(string, font, size, align, max_width)`. For value-bound text the string isn't known until
  the key resolves, so **all** text lays out at render — one code path for static and bound text.
  A layout **cache** keyed by the resolved inputs means unchanged text is never re-shaped per
  frame (serves the perf-priority constraint).
- **Sandbox unchanged.** A skin only *names* a font asset; the engine resolves it through the
  sandboxed `AssetResolver` (no `..`/symlink escape, same as images). Lua gets no filesystem
  access. System fonts appear only as parley's render-time *fallback*, never as resolvable
  named assets.
- **Transactional swap, zero domain knowledge** — unaffected; text is generic style. "Track
  title" is just a bound state key; the engine attaches no media meaning.

```
state.rs   # StateValue gains Str(Arc<str>); enum becomes Clone (was Copy)
host.rs    # Host::get already returns Option<StateValue> — now may yield Str; no signature change
scene.rs   # new TextContent, TextAlign, FontData; Node::Text { .. }; summary() line
asset.rs   # AssetResolver::font(name) -> Result<Arc<FontData>, AssetError> (raw bytes, cached)
vocab.rs   # BuildContext gains font(name); new TextPrim (the `text` constructor) using parse_paint
script.rs  # threads font resolution into the SceneBuilder (like image)
render.rs  # Renderer owns parley FontContext + LayoutContext + layout cache; draws Node::Text
           #   via vello draw_glyphs with a Paint brush; resolves bound string from state
crates/carapace-demo/skins/  # reference skin: gradient-chrome label + live value-bound readout
```

## 2. State model (`state.rs`, `host.rs`)

```rust
#[derive(Clone, PartialEq, Debug)]
pub enum StateValue {
    Bool(bool),
    Scalar(f32),
    Str(std::sync::Arc<str>),   // NEW — string state for value-bound text
}
```

`StateValue` drops `Copy` (an `Arc<str>` is not `Copy`) and derives `Clone`. The ripple is
small and mechanical: the few sites that read state by value (`value_fill`'s scalar path,
`engine.state`, the render read-closure) clone instead of copy. `Host::get(&self, key) ->
Option<StateValue>` is **unchanged in signature** — it may now return `Str`. The action-arg
`Value::Str` (host.rs) is a separate, already-existing type and is untouched.

`value_fill`'s `value_of` continues to interpret `Scalar`/`Bool` and **ignores `Str`** (a
string bound into a numeric value_fill clamps to 0, as a non-scalar does today). Text's
resolver does the inverse: it reads `Str` and treats `Scalar`/`Bool`/missing as "no string"
(renders nothing).

## 3. Data model (`scene.rs`)

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum TextContent { Static(String), Bound(String) }   // Bound holds a state key

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextAlign { Left, Center, Right }

#[derive(Debug)]
pub struct FontData { pub bytes: std::sync::Arc<[u8]> }   // raw font file bytes (resolver-owned)

// in Node:
Text {
    content: TextContent,
    font: Option<std::sync::Arc<FontData>>,   // None -> system default family (fallback)
    size: f32,
    paint: Paint,                             // 5b: Solid | Gradient
    align: TextAlign,
    max_width: Option<f32>,                   // None -> no wrap (breaks only on '\n')
    pos: Pt,                                  // top-left anchor, canvas space
}
```

No glyph runs or positions in the scene — shaping is a render concern. `pos` is the **top-left**
of the text block (consistent with `image` `dest.x/y`). `align` governs justification of lines
within the block's width: `max_width` when set, else the natural width of the longest line.

## 4. Asset / font resolution (`asset.rs`, `vocab.rs`, `script.rs`)

`AssetResolver` gains, alongside `bytes()` and `image()`:

```rust
pub fn font(&self, name: &str) -> Result<Arc<FontData>, AssetError>;  // raw bytes, cached
```

`font()` reuses the same sandboxed name→path index and byte cache as `bytes()`/`image()`; it
wraps the bytes in `FontData` and caches the `Arc<FontData>` (so repeated `font="x.ttf"` across
nodes shares one allocation and one render-side registration). No decode/parse happens here —
the bytes are handed to parley at render. A missing/un-resolvable name → `AssetError` →
`BuildError` at build.

`BuildContext` gains `font(name) -> Result<Arc<FontData>, AssetError>`; `script.rs` threads the
resolver into the `SceneBuilder` for `TextPrim` exactly as it does for `ImagePrim`.

## 5. Lua / vocab API (`vocab.rs`)

`TextPrim::build(args: &Table)` reads:

- **content (required, exactly one):** `text = "<string>"` → `TextContent::Static`, **xor**
  `value = "<key>"` → `TextContent::Bound`. Neither → `MissingField("text")`; both →
  `BuildError::BadType` (ambiguous). (`value=` matches `value_fill`'s bound-key spelling.)
- `font = "<asset name>"` (optional) → `ctx.font(name)?` → `Some(Arc<FontData>)`; absent →
  `None` (system default family).
- `size = <number>` (optional, default `16.0`).
- fill via the existing **`parse_paint(args)`**: `gradient={…}` → `Paint::Gradient`, else
  `color={…}` → `Paint::Solid`; neither → `MissingField("color")` (shared 5b behavior).
- `align = "left" | "center" | "right"` (optional, default `"left"`; any other → `BadType`).
- `x`, `y` (required) → `pos`.
- `max_width = <number>` (optional) → `Some(width)`; absent → `None` (no wrap).

```lua
-- static gradient-chrome label
text{ text = "HEADSPACE", font = "digital.ttf", size = 18, x = 40, y = 8, align = "center",
      gradient = { type = "linear", from = {x=0,y=0}, to = {x=0,y=18},
                   stops = { {at=0, color={r=230,g=240,b=255}},
                             {at=1, color={r=120,g=150,b=200}} } } }

-- value-bound live readout (string state key), solid color
text{ value = "track_title", font = "digital.ttf", size = 12, x = 40, y = 30,
      color = { r = 120, g = 230, b = 80 } }

-- wrapped multi-line static text, left aligned, system fallback font (no `font=`)
text{ text = "now playing\nlong title that wraps", size = 11, x = 8, y = 60, max_width = 120 }
```

A malformed `text{}` (both/neither content, bad `align`, missing font asset, bad paint) →
`BuildError` → caught by the **transactional swap** (skin fails to load; prior scene stays).

## 6. Render (`render.rs`)

The `Renderer` owns a parley `FontContext` + `LayoutContext` (built once) and a **layout cache**:

- **Font registration:** a skin font's bytes are registered into the `FontContext` on first use,
  keyed by the `Arc<FontData>` pointer identity → a parley font family. `font: None` uses the
  system default family. parley's default collection includes system fonts, giving **fallback**
  for glyphs the bundled font lacks (and the whole string when no font is named).
- **Layout cache:** keyed by `(font identity, size bits, align, max_width bits, resolved
  string)` → a cached parley `Layout`. Unchanged text (static, or a bound string that didn't
  change between frames) is **not re-shaped** — the perf-critical path.
- **Per `Node::Text` at draw:**
  1. Resolve `content`: `Static(s)` → `s`; `Bound(key)` → `read_value(key)` → `Str(s)` → `s`;
     `Scalar`/`Bool`/missing → empty string → **render nothing** (no panic, like `value_fill`
     degrades).
  2. Get-or-build the `Layout` (font, size, max_width for wrap, align for justification).
  3. For each glyph run: `vs.draw_glyphs(run_font).font_size(size).brush(paint_brush(&paint))
     .transform(xform · translate(pos)).draw(Fill::NonZero, glyphs)` — `paint_brush` is the 5b
     helper (solid color with real alpha, or a peniko gradient). Gradient coordinates are
     canvas-space, same as fills, so chrome text scales with the skin.

Text draws under the same canvas→surface `xform` as every other node, so it scales with the
skin like images and fills.

## 7. `scene::summary()`

Domain-neutral, deterministic, **no glyph geometry** (glyph metrics are OS-dependent under
fallback, so they must never enter a snapshot). One line per text node, with the paint
descriptor reused from 5b:

- Static: `text "<string>" font=<name|system> size=<n> align=<a> <paint>`
- Bound:  `text value=<key> font=<name|system> size=<n> align=<a> <paint>`

where `<paint>` is `rgba=<r>,<g>,<b>,<a>` (solid) or `gradient=<kind> stops=<n>` (gradient) —
exactly the 5b spellings — `<name>` is the font asset name or the literal `system`, `<n>` is the
integer size, `<a>` is `left|center|right`. `max_width` is omitted from the summary (geometry).
The `<string>` is the author's static literal (deterministic); bound text shows the **key**, not
a resolved value.

## 8. Demo payoff

The `reference` (Headspace) skin gains the authentic readout layer:
- a **static gradient-chrome label** (e.g. the skin/app name) — proving gradient-filled text,
- a **live value-bound readout** (`value="track_title"`) driven by a new `Str` state key in the
  fake media-player host — proving string state end to end, updating on rebuild.

The fake host (`carapace-demo`) adds a `track_title` string to its state so `Host::get` can
return `StateValue::Str`. One demo skin also shows a **wrapped, system-fallback** (no `font=`)
label so multi-line + fallback render live in `cargo run -p carapace-demo`.

A bundled font file (a permissively-licensed `.ttf`, e.g. an OFL face) ships in the `reference`
skin's `assets/` so named-font text is exercised without relying on the host's system fonts.

## 9. Testing

**Headless (no GPU, fallback-independent):**
- `parse` (`TextPrim`): `text=` xor `value=` (neither → `MissingField`; both → `BadType`);
  `align` default `left` and bad value → `BadType`; `size` default `16`; missing `font` asset →
  `BuildError`; `parse_paint` solid vs gradient reused from 5b.
- `AssetResolver::font`: resolves a bundled font, caches (same `Arc` on repeat), rejects
  traversal (shares the 5a sandbox tests).
- `summary()`: static line, bound (`value=`) line, and the gradient-paint variant are stable
  (snapshot updated). Static string, font name/`system`, size, align, and paint appear; **no
  geometry**.
- `StateValue::Str`: `value_fill` ignores a `Str` binding (clamps to 0, no panic); state round-
  trips a `Str` through `engine.state`.

**Gated GPU (`gpu-tests` feature; lavapipe CI / local Metal) — deterministic by construction:**
- Render a **bundled font** with **ASCII** that the font contains (so system fallback never
  engages → cross-platform-stable), sample a known glyph-interior pixel, assert the fill color
  within tolerance (catches font-load / brush / transform regressions).
- A **gradient-text** case: a 2-stop gradient over a glyph, sample an interior point, assert the
  interpolated color within tolerance (proves `Paint` flows through `draw_glyphs`).

**Snapshot harness:** continues via `summary()` lines (no RGBA hashing); text lines are
geometry-free so they stay stable across platforms.

**Human:** `cargo run -p carapace-demo` → the `reference` skin shows the gradient-chrome label
and a live `track_title` readout; a vector skin shows a wrapped system-fallback label; Tab-swap
survives with state intact.

## Dependency policy

parley (and its transitive `skrifa`/`fontique`/`swash` stack) is added via
**`sfw cargo add parley -p carapace`** and first fetched/built under `sfw` (Socket Firewall
supply-chain filtering), per the project rule. The bundled demo font is a permissively-licensed
face (OFL/Apache); its license file ships alongside it in the skin's `assets/`.

## Error handling

- Malformed `text{}` / font / paint at build → `BuildError` → transactional swap keeps the prior
  scene.
- A bound key that is missing or non-string at render → empty string → nothing drawn (no panic),
  matching `value_fill`'s degrade-don't-crash behavior.
- No panics on a skin/asset/font fault; the engine returns `Result` / degrades. `unwrap` only on
  engine invariants (as today).

## Definition of done (5c)

`StateValue::Str` exists and round-trips through the host/state path; a `text{}` primitive lays
out static and value-bound strings with multi-line wrapping and left/center/right alignment;
fonts resolve through the sandboxed `AssetResolver` with parley system fallback; text fills via
the 5b `Paint` (solid and gradient both render — the GPU color sentinels pass); the demo
`reference` skin shows a gradient-chrome label and a live `track_title` readout; the headless
boundary, the fast `check` CI job, and the snapshot harness are all unchanged/green.

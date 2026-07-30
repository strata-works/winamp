# Engine — Fractional Sizing + Per-Element `max` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in `frac={x?,y?,w?,h?}` (fractions of the container) and `max={w?,h?}` (extent ceiling) attribute tables to the layout engine, so frame skins can express proportional splits and capped panels — composing with the existing anchor model, with zero behavior change for skins that don't use them.

**Architecture:** Extend the per-node `Anchors` struct (`crates/carapace/src/layout.rs`) with a `max` field and a new `Frac` sub-struct; thread both through `resolve_axis`/`resolve_bbox`; parse the two new tables in `parse_anchors` (`crates/carapace/src/script.rs`), which is also refactored so `min`/`max`/`frac` are honored without an `anchor` field. A new `carapace-demo` skin proves it visually. No new vocab primitive; no FFI change.

**Tech Stack:** Rust, `crates/carapace` (layout + mlua/Lua script binding), `crates/carapace-demo` (winit demo), `cargo test`.

## Global Constraints

- **Only** `crates/carapace/**`, `crates/carapace-demo/**`, and `docs/` may change. **No `crates/carapace-ffi/**` change** (the C-ABI resize seam is SP2), no other crates.
- **No new vocab primitive** — `VocabRegistry::base()` stays at 10; do not touch its count test.
- **All pre-existing golden/snapshot tests must stay byte-identical** (`crates/carapace/tests/snapshots/*.snap`, render/behavior snapshots). This is the primary backward-compat gate: `frac`/`max` are additive and no shipped skin uses them.
- **`frac`/`max` semantics** (verbatim): fractional value overrides only its own axis-field (`x`,`w` scale by logical width; `y`,`h` by logical height); position frac clamps negatives to 0; extent clamp order is **`min` first (raise), then `max` (lower)** — if `max < min`, `max` wins. Sentinels: `min` absent axis = `0.0` (no floor, as today); `max` absent axis = `f32::INFINITY` (no ceiling).
- **Full local gate before push** (repo convention): `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy` gpu-tests variant, `cargo test --workspace` — all green. Clippy or test alone is not enough.
- **Git identity** `Daniel Agbemava <danagbemava@gmail.com>`; no "Generated with Claude Code" footer.
- **Branch** `engine-frac-max-layout` (already created off `main`, already holds the design spec commit). No direct push to `main`; PR at the end.
- No new third-party dependencies (so no `sfw` fetch needed).

---

### Task 1: `frac`/`max` data model + resolver wiring

**Files:**
- Modify: `crates/carapace/src/layout.rs` (the `Anchors` struct + `TOP_LEFT` + `from_edges` at ~20-56; `resolve_axis` at ~57-79; `resolve_bbox` at ~80-86; the two in-module test `Anchors {…}` literals at ~268 and ~302)
- Test: `crates/carapace/tests/layout.rs` (add frac/max resolver tests using the existing `a(...)` helper pattern)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub struct Frac { pub x: Option<f32>, pub y: Option<f32>, pub w: Option<f32>, pub h: Option<f32> }` with `pub const EMPTY: Frac` and `#[derive(Clone, Copy, Debug, Default, PartialEq)]`.
  - `Anchors` gains `pub max: Option<(f32, f32)>` and `pub frac: Frac`.
  - `resolve_axis(p, e, d, l, near, far, min_e: f32, max_e: f32, frac_pos: Option<f32>, frac_ext: Option<f32>) -> (f32, f32)`.
  - `resolve_bbox` unchanged signature (still `(design, logical, bbox, a: Anchors) -> Rect`) but now feeds max/frac to `resolve_axis`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/carapace/tests/layout.rs`. First check the top of that file for the existing `a(...)` helper and imports; it defines `fn a(left,right,top,bottom) -> Anchors`. Add a richer helper and tests:

```rust
use carapace::layout::{resolve_bbox, Anchors, Frac, Rect};

// Anchors with explicit min/max/frac (helper `a(...)` covers the edges-only case).
fn anc(edges: Anchors, min: Option<(f32, f32)>, max: Option<(f32, f32)>, frac: Frac) -> Anchors {
    Anchors { min, max, frac, ..edges }
}

#[test]
fn frac_extent_is_container_fraction() {
    // 30% of logical width, regardless of design width.
    let a = anc(Anchors::from_edges(&["left", "top"]), None, None,
                Frac { w: Some(0.30), ..Frac::EMPTY });
    let r = resolve_bbox((400.0, 300.0), (1000.0, 300.0),
                         Rect { x: 0.0, y: 0.0, w: 144.0, h: 100.0 }, a);
    assert_eq!(r.w, 300.0);   // 0.30 * 1000
    assert_eq!(r.h, 100.0);   // untouched (no frac.h, top-only anchor)
}

#[test]
fn frac_position_only_leaves_extent_to_anchors() {
    // frac.x overrides ONLY position; extent still comes from the anchor rule (independent).
    // No x pins (top,bottom only) -> extent stays design width (neither-pinned keeps e).
    let a = anc(Anchors::from_edges(&["top", "bottom"]), None, None,
                Frac { x: Some(0.30), ..Frac::EMPTY });
    let r = resolve_bbox((400.0, 300.0), (1000.0, 300.0),
                         Rect { x: 144.0, y: 0.0, w: 256.0, h: 300.0 }, a);
    assert_eq!(r.x, 300.0);   // 0.30 * 1000
    assert_eq!(r.w, 256.0);   // NOT stretched -- extent is design width, position frac is independent
}

#[test]
fn frac_fill_remaining_uses_both_x_and_w() {
    // "Fill the rest" = both position AND extent fractional (x=30%, w=70%) -> right edge at 100%.
    let a = anc(Anchors::from_edges(&["top", "bottom"]), None, None,
                Frac { x: Some(0.30), w: Some(0.70), ..Frac::EMPTY });
    let r = resolve_bbox((400.0, 300.0), (1000.0, 300.0),
                         Rect { x: 144.0, y: 0.0, w: 256.0, h: 300.0 }, a);
    assert_eq!(r.x, 300.0);
    assert_eq!(r.w, 700.0);   // right edge = 300 + 700 = 1000
}

#[test]
fn max_caps_a_stretch() {
    // both-edges-pinned stretch of a 100-wide element (design canvas 400), capped at 320.
    let a = anc(Anchors::from_edges(&["left", "right", "top"]), None, Some((320.0, f32::INFINITY)),
                Frac::EMPTY);
    let capped = resolve_bbox((400.0, 300.0), (700.0, 300.0),
                              Rect { x: 0.0, y: 0.0, w: 100.0, h: 20.0 }, a);
    assert_eq!(capped.w, 320.0, "100 + delta 300 = 400, capped to 320");
    let below = resolve_bbox((400.0, 300.0), (480.0, 300.0),
                             Rect { x: 0.0, y: 0.0, w: 100.0, h: 20.0 }, a);
    assert_eq!(below.w, 180.0, "100 + delta 80 = 180, below the 320 cap");
}

#[test]
fn min_then_max_max_wins_when_contradictory() {
    let a = anc(Anchors::from_edges(&["left", "top"]), Some((200.0, 0.0)), Some((100.0, f32::INFINITY)),
                Frac { w: Some(0.10), ..Frac::EMPTY });
    // frac 0.10*1000=100, min raises to 200, max lowers to 100 -> max wins.
    let r = resolve_bbox((400.0, 300.0), (1000.0, 300.0),
                         Rect { x: 0.0, y: 0.0, w: 40.0, h: 20.0 }, a);
    assert_eq!(r.w, 100.0);
}

#[test]
fn sidebar_content_split_composes() {
    // 30% sidebar (max 320) + content occupying the remaining 70% (both x and w fractional).
    let side = anc(Anchors::from_edges(&["left", "top", "bottom"]), None, Some((320.0, f32::INFINITY)),
                   Frac { w: Some(0.30), ..Frac::EMPTY });
    let content = anc(Anchors::from_edges(&["top", "bottom"]), None, None,
                      Frac { x: Some(0.30), w: Some(0.70), ..Frac::EMPTY });
    let d = (480.0, 320.0);
    // At 800 wide: sidebar 240 (<320 cap) at x0; content x=240 w=560 -> they tile, right edge 800.
    let s = resolve_bbox(d, (800.0, 320.0), Rect { x: 0.0, y: 34.0, w: 144.0, h: 270.0 }, side);
    let c = resolve_bbox(d, (800.0, 320.0), Rect { x: 144.0, y: 34.0, w: 336.0, h: 270.0 }, content);
    assert_eq!(s.x, 0.0);
    assert_eq!(s.w, 240.0);
    assert_eq!(c.x, 240.0);
    assert_eq!(c.w, 560.0);
    // At 2000 wide: sidebar caps at 320; content still starts at 30% (600) -> a gap 320..600.
    let s2 = resolve_bbox(d, (2000.0, 320.0), Rect { x: 0.0, y: 34.0, w: 144.0, h: 270.0 }, side);
    assert_eq!(s2.w, 320.0, "capped");
}

#[test]
fn no_frac_no_max_is_identity_with_today() {
    // A both-pinned stretch with neither frac nor max must match the pre-change behavior.
    let a = Anchors::from_edges(&["left", "right", "top", "bottom"]);
    let r = resolve_bbox((200.0, 100.0), (300.0, 140.0),
                         Rect { x: 10.0, y: 10.0, w: 180.0, h: 80.0 }, a);
    assert_eq!(r, Rect { x: 10.0, y: 10.0, w: 280.0, h: 120.0 });
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace --test layout`
Expected: FAIL to compile — `Frac` / `Anchors.max` / `Anchors.frac` don't exist, and `Anchors::from_edges(&[...])` won't have `..edges` fields yet.

- [ ] **Step 3: Extend the data model in `layout.rs`**

Add the `Frac` struct (place it just above `Anchors`):

```rust
/// Per-element fractional overrides. Each present field replaces that field's anchor-resolved
/// value with `fraction * container_length` along its axis (`x`,`w` → width; `y`,`h` → height).
/// Absent fields stay absolute.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frac {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub w: Option<f32>,
    pub h: Option<f32>,
}

impl Frac {
    /// All-absolute (no fractional override on any field).
    pub const EMPTY: Frac = Frac { x: None, y: None, w: None, h: None };
}
```

Extend `Anchors` (add the two fields after `min`):

```rust
pub struct Anchors {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    /// Minimum (w, h) a stretched element collapses to. 0 on an axis = no floor.
    pub min: Option<(f32, f32)>,
    /// Maximum (w, h) a stretched/fractional element grows to. Absent axis = f32::INFINITY (no cap).
    pub max: Option<(f32, f32)>,
    /// Per-field fractional overrides (fractions of the container).
    pub frac: Frac,
}
```

Update `TOP_LEFT` and `from_edges` to include the new fields:

```rust
    pub const TOP_LEFT: Anchors = Anchors {
        left: true,
        right: false,
        top: true,
        bottom: false,
        min: None,
        max: None,
        frac: Frac::EMPTY,
    };

    pub fn from_edges(edges: &[&str]) -> Anchors {
        Anchors {
            left: edges.contains(&"left"),
            right: edges.contains(&"right"),
            top: edges.contains(&"top"),
            bottom: edges.contains(&"bottom"),
            min: None,
            max: None,
            frac: Frac::EMPTY,
        }
    }
```

- [ ] **Step 4: Wire `max`/`frac` into `resolve_axis` + `resolve_bbox`**

Replace `resolve_axis` with:

```rust
/// Resolve one axis: origin `p`, extent `e`, design length `d`, logical length `l`, pins
/// `(near, far)`, floor `min_e`, ceiling `max_e` (INFINITY = none), optional fractional overrides
/// `frac_pos`/`frac_ext` (fractions of `l`). Returns `(p', e')`.
#[allow(clippy::too_many_arguments)]
fn resolve_axis(
    p: f32,
    e: f32,
    d: f32,
    l: f32,
    near: bool,
    far: bool,
    min_e: f32,
    max_e: f32,
    frac_pos: Option<f32>,
    frac_ext: Option<f32>,
) -> (f32, f32) {
    let delta = l - d;
    let (mut np, mut ne) = match (near, far) {
        (true, true) => (p, e + delta),  // both gaps fixed -> stretch
        (true, false) => (p, e),         // near gap fixed
        (false, true) => (p + delta, e), // far gap fixed -> rides far edge
        (false, false) => (p * (l / d.max(1.0)), e), // proportional re-center
    };
    if let Some(f) = frac_pos {
        np = f.max(0.0) * l;
    }
    if let Some(f) = frac_ext {
        ne = f.max(0.0) * l;
    }
    if ne < min_e {
        ne = min_e; // floor first
    }
    if ne > max_e {
        ne = max_e; // then ceiling (max < min -> max wins)
    }
    if ne < 0.0 {
        ne = 0.0;
    }
    if !np.is_finite() {
        np = p;
    }
    (np, ne)
}
```

Replace `resolve_bbox` with:

```rust
/// Resolve a design-space bounding box to a logical bounding box under its anchors.
pub fn resolve_bbox(design: (f32, f32), logical: (f32, f32), bbox: Rect, a: Anchors) -> Rect {
    let (min_w, min_h) = a.min.unwrap_or((0.0, 0.0));
    let (max_w, max_h) = a.max.unwrap_or((f32::INFINITY, f32::INFINITY));
    let (x, w) = resolve_axis(
        bbox.x, bbox.w, design.0, logical.0, a.left, a.right, min_w, max_w, a.frac.x, a.frac.w,
    );
    let (y, h) = resolve_axis(
        bbox.y, bbox.h, design.1, logical.1, a.top, a.bottom, min_h, max_h, a.frac.y, a.frac.h,
    );
    Rect { x, y, w, h }
}
```

- [ ] **Step 5: Fix the in-module test literals**

In `crates/carapace/src/layout.rs` the two `#[cfg(test)]` tests construct `Anchors { left, right, top, bottom, min: None }` (around lines 268 and 302). Add the new fields to each so they compile:

```rust
        let anchors = vec![Anchors {
            left: true,
            right: true,
            top: true,
            bottom: false,
            min: None,
            max: None,
            frac: Frac::EMPTY,
        }];
```

(and the same for the second literal with its own edge bools). Add `Frac` to the test module's `use super::*;`-covered scope — `Frac` is in the same module, so no extra import is needed.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p carapace --test layout` then `cargo test -p carapace --lib layout`
Expected: PASS — new integration tests green; the in-module `scrub_region_stretches_under_full_anchors` / `list_region_stretches_under_full_anchors` still green (unchanged behavior).

- [ ] **Step 7: Commit**

```bash
git add crates/carapace/src/layout.rs crates/carapace/tests/layout.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(engine): frac + max in layout resolver (fractional sizing, extent ceiling)"
```

---

### Task 2: Parse `frac`/`max` in `parse_anchors` (and honor them without `anchor`)

**Files:**
- Modify: `crates/carapace/src/script.rs` (`parse_anchors` at ~110-127)
- Test: `crates/carapace/tests/anchors_build.rs` (add a case loading a skin that sets `frac`/`max`)

**Interfaces:**
- Consumes: `Anchors`, `Frac` (Task 1).
- Produces: `parse_anchors` fills `a.max` and `a.frac` from Lua tables, and no longer early-returns when `anchor` is absent (so `min`/`max`/`frac` work standalone).

- [ ] **Step 1: Write the failing test**

Look at the existing `crates/carapace/tests/anchors_build.rs` to match its skin-loading helper (it builds a skin from a Lua source string and inspects `LoadedSkin.anchors`). Add a test mirroring that pattern:

```rust
#[test]
fn parses_frac_and_max_without_requiring_anchor() {
    // Element sets frac/max but NO anchor attr — must still be honored (defaults to top-left edges).
    let src = r#"
        fill{ path = rect{ x = 0, y = 0, w = 100, h = 50 },
              color = { r = 0, g = 0, b = 0 },
              frac = { w = 0.3, x = 0.1 }, max = { w = 320 } }
    "#;
    let anchors = load_anchors(src); // <-- use whatever this test file's helper is named
    let a = anchors[0];
    assert_eq!(a.frac.w, Some(0.3));
    assert_eq!(a.frac.x, Some(0.1));
    assert_eq!(a.frac.y, None);
    assert_eq!(a.max, Some((320.0, f32::INFINITY)));
    // no anchor attr -> top-left edges, but min/max/frac still read
    assert!(a.left && a.top && !a.right && !a.bottom);
}
```

If `anchors_build.rs` has no reusable loader helper, build the skin the same way its existing test does (copy that setup inline). Use the real helper/skin-loader name from the file — do not invent one.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p carapace --test anchors_build parses_frac_and_max`
Expected: FAIL — `frac`/`max` are not parsed (fields are `None`/default), and the no-anchor path returns bare `TOP_LEFT`.

- [ ] **Step 3: Refactor `parse_anchors`**

Replace the body of `parse_anchors` in `crates/carapace/src/script.rs` with (removes the early return; reads `min`/`max`/`frac` regardless of `anchor`):

```rust
fn parse_anchors(args: &Table) -> mlua::Result<crate::layout::Anchors> {
    use crate::layout::{Anchors, Frac};
    let mut a = match args.get::<Option<Table>>("anchor")? {
        Some(t) => {
            let edges: Vec<String> = t
                .sequence_values::<String>()
                .filter_map(|v| v.ok())
                .collect();
            let refs: Vec<&str> = edges.iter().map(|s| s.as_str()).collect();
            Anchors::from_edges(&refs)
        }
        None => Anchors::TOP_LEFT,
    };
    if let Some(m) = args.get::<Option<Table>>("min")? {
        let w: f32 = m.get::<Option<f32>>("w")?.unwrap_or(0.0);
        let h: f32 = m.get::<Option<f32>>("h")?.unwrap_or(0.0);
        a.min = Some((w, h));
    }
    if let Some(m) = args.get::<Option<Table>>("max")? {
        let w: f32 = m.get::<Option<f32>>("w")?.unwrap_or(f32::INFINITY);
        let h: f32 = m.get::<Option<f32>>("h")?.unwrap_or(f32::INFINITY);
        a.max = Some((w, h));
    }
    if let Some(f) = args.get::<Option<Table>>("frac")? {
        a.frac = Frac {
            x: f.get::<Option<f32>>("x")?,
            y: f.get::<Option<f32>>("y")?,
            w: f.get::<Option<f32>>("w")?,
            h: f.get::<Option<f32>>("h")?,
        };
    }
    Ok(a)
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p carapace --test anchors_build`
Expected: PASS (new test + the existing `anchors[1] == Anchors::TOP_LEFT` case, which has no `min`/`max`/`frac` so still equals `TOP_LEFT`).

- [ ] **Step 5: Guard against a behavior regression + commit**

Run the whole crate's tests + snapshots to confirm nothing shifted:

```bash
cargo test -p carapace
```
Expected: all green, **including every `tests/snapshots/*.snap`** (insta) — byte-identical, since no existing skin uses `min` without `anchor` or the new tables. If any snapshot changed, STOP and investigate (a skin sets `min` without `anchor` and now gets floored — reconcile before proceeding).

```bash
git add crates/carapace/src/script.rs crates/carapace/tests/anchors_build.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(engine): parse frac/max tables; honor min/max/frac without an anchor attr"
```

---

### Task 3: Demo — layout-playground skin

**Files:**
- Create: `crates/carapace-demo/skins/layout/skin.toml`
- Create: `crates/carapace-demo/skins/layout/skin.lua`
- Modify: `crates/carapace-demo/src/main.rs` (add `"skins/layout"` to `MEDIA_SKINS` at ~161-167)

**Interfaces:**
- Consumes: the engine `frac`/`max` from Tasks 1-2. No new Rust interface.
- Produces: a resizable demo skin that visually shows a 30%-sidebar (capped at 320px) + 70% content reflowing on resize. Validated by running the demo (no unit test).

- [ ] **Step 1: Create the skin manifest**

`crates/carapace-demo/skins/layout/skin.toml`:

```toml
canvas = { width = 480, height = 320 }
resizable = true
min_size = [320, 220]
```

- [ ] **Step 2: Create the skin**

`crates/carapace-demo/skins/layout/skin.lua` — pure `fill{}`/`text{}` chrome (no `view{}`, no host content), exercising `frac` + `max`:

```lua
local W, H = 480, 320

-- Background, fills the whole window.
fill{ path = rect{ x = 0, y = 0, w = W, h = H },
      color = { r = 18, g = 20, b = 26 },
      anchor = { "left", "right", "top", "bottom" } }

-- Sidebar: 30% of width, capped at 320px, full height.
fill{ path = rect{ x = 0, y = 0, w = 144, h = H },
      color = { r = 44, g = 52, b = 74 },
      anchor = { "left", "top", "bottom" },
      frac = { w = 0.30 }, max = { w = 320 } }
text{ text = "30% (max 320)", x = 16, y = 20, size = 14,
      color = { r = 220, g = 228, b = 245 },
      anchor = { "left", "top" } }

-- Content: occupies the remaining 70% -- BOTH x and w are fractional (x=30%, w=70%).
fill{ path = rect{ x = 144, y = 0, w = W - 144, h = H },
      color = { r = 28, g = 32, b = 42 },
      anchor = { "top", "bottom" },
      frac = { x = 0.30, w = 0.70 } }
text{ text = "content (30%..100%)", x = 160, y = 20, size = 14,
      color = { r = 200, g = 208, b = 226 },
      frac = { x = 0.30 }, anchor = { "top" } }
```

(Confirm the exact `fill{}`/`text{}`/`rect{}` field names against another demo skin, e.g. `crates/carapace-demo/skins/frame/skin.lua`, before finalizing — match its color/rect conventions.)

- [ ] **Step 3: Register the skin in the demo**

In `crates/carapace-demo/src/main.rs`, add `"skins/layout"` to the `MEDIA_SKINS` array (~line 161):

```rust
const MEDIA_SKINS: &[&str] = &[
    "skins/classic",
    "skins/minimal",
    "skins/reference",
    "skins/transport",
    "skins/frame",
    "skins/shaderdemo",
    "skins/layout",
];
```

**Before running:** read how `main.rs` handles a `resizable` skin whose scene has **no `view{ id="app" }`** (the frame-skin render path around lines 514/597 paints an `AppShell` into `view{app}`). Confirm a view-less resizable skin renders its resolved scene without panicking (the `view_tex` lookup should simply return `None` for a missing id → nothing composited). If the path hard-requires a view, either (a) gate the AppShell composite on the skin actually declaring that view, or (b) give the layout skin a harmless empty `view{ id="app", ... }`. Prefer (a); keep the change minimal and note which you did in the report.

- [ ] **Step 4: Build + run to verify reflow**

```bash
cargo build -p carapace-demo
```
Expected: builds clean.

Then run and cycle to the layout skin (the demo cycles `MEDIA_SKINS` with a key — check `main.rs` for the cycle key, ~line 716/733). Resize the window:
- The sidebar holds ~30% of the width and the content fills the rest.
- Widen past ~1067px (320 / 0.30) → the sidebar stops growing at 320px; the content keeps filling.
- No panic; the other skins still cycle and render as before.

(This is a GUI check. If run headless/in a subagent without a display, report that the build is clean and the skin loads via a headless load — `carapace::skin::load_dir(skin_root().join("skins/layout"))` succeeds — and defer the visual resize check to the controller.)

- [ ] **Step 5: Commit**

```bash
git add crates/carapace-demo/skins/layout crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "demo: layout-playground skin (30% sidebar capped at 320px + 70% content, reflows)"
```

---

### Task 4: Full gate, docs, and ship

**Files:**
- Modify: the skin-authoring docs where `anchor`/`min` are documented (find via grep; likely under `docs/api/` or a `skin-authoring` guide)

- [ ] **Step 1: Document `frac`/`max`**

Find where the layout attributes are documented:

```bash
grep -rln "anchor" docs/ | head
grep -rn "min = {\|\"min\"\|anchor=\|resizable" docs/ | grep -i "layout\|anchor\|frame\|skin-author" | head
```

In the same section that documents `anchor={...}` and `min={w,h}`, add `frac={x?,y?,w?,h?}` (fractions of the container per axis-field; `x`,`w`→width, `y`,`h`→height) and `max={w?,h?}` (extent ceiling, mirror of `min`), with the sidebar/content split as a one-block example and a note that fractional overrides its axis-field, `max<min`→`max` wins, and that these compose with `anchor`. If there is no existing anchor/min doc section, do **not** invent a new doc file — note that in the report and skip (the spec + rustdoc on `Frac`/`Anchors` carry it).

- [ ] **Step 2: Full local gate**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features gpu-tests -- -D warnings   # gpu-tests variant (match repo's clippy invocation)
cargo test --workspace 2>&1 | tail -20
```
Expected: fmt clean; both clippy invocations clean; all tests green **including every pre-existing snapshot** (byte-identical). Confirm scope:

```bash
git diff main --stat | grep -vE "crates/carapace/|crates/carapace-demo/|docs/" && echo "OUT-OF-SCOPE FILE — STOP" || echo "scope ok ✓"
git diff main --stat | grep -E "carapace-ffi|vocab.rs.*count|base_registry" && echo "CHECK: ffi or vocab-count touched — STOP" || echo "no ffi / no vocab-count change ✓"
```

- [ ] **Step 3: Commit any doc/format changes + push**

```bash
git add docs/ crates/
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs(engine): document frac/max layout attributes" --allow-empty
git push -u origin engine-frac-max-layout
```

- [ ] **Step 4: Open the PR**

Open a PR targeting `main`, title `feat(engine): fractional sizing + per-element max (responsive frame skins SP1)`, body summarizing the two new attribute tables, the compose-with-anchors semantics, the backward-compat (goldens byte-identical, no new primitive), and that SP2 (C-ABI resize) + SP3 (weather responsive) follow. **No "Generated with Claude Code" footer.**

---

## Self-Review

**Spec coverage** (against `docs/superpowers/specs/2026-07-30-engine-fractional-max-layout-design.md`):
- `frac={x,y,w,h}` fractions of container → Task 1 (resolve) + Task 2 (parse). ✓
- `max={w,h}` extent ceiling mirroring `min` → Task 1 + Task 2. ✓
- Fractional overrides per axis-field, composes with anchors → Task 1 (`resolve_axis`), tested by `sidebar_content_split_composes`. ✓
- `min` then `max`, `max<min`→`max` wins → Task 1 (`min_then_max_max_wins_when_contradictory`). ✓
- `parse_anchors` honors `min`/`max`/`frac` without `anchor` → Task 2 (`parses_frac_and_max_without_requiring_anchor`). ✓
- Backward compat: no frac/max = identity; goldens byte-identical; no new primitive → Task 1 (`no_frac_no_max_is_identity_with_today`), Task 2 Step 5, Task 4 Step 2. ✓
- Separate demo skin (existing goldens untouched) → Task 3. ✓
- Sentinels (min 0, max INFINITY) → Task 1 (`resolve_bbox` unwrap_or). ✓

**Placeholder scan:** no TBD/TODO. Each code step shows complete code. The two spots that require reading a neighbor first — the `anchors_build.rs` loader-helper name (Task 2 Step 1) and the demo skin field-name/AppShell-view check (Task 3) — are explicit "read the real thing, don't invent" instructions, not placeholders. ✓

**Type consistency:** `Frac { x,y,w,h: Option<f32> }` + `Frac::EMPTY` used identically in Tasks 1/2/3 tests. `Anchors.max: Option<(f32,f32)>` (INFINITY sentinel) consistent between `resolve_bbox` (Task 1) and `parse_anchors` (Task 2) and the test assertion `Some((320.0, f32::INFINITY))`. `resolve_axis` 10-arg signature defined in Task 1 and not called elsewhere (only `resolve_bbox` calls it). ✓

# Engine — Fractional Sizing + Per-Element `max` (Responsive Frame Skins, SP1) — Design

**Date:** 2026-07-30
**Status:** Approved
**Scope:** `crates/carapace` only — the GPU-free layout resolver (`src/layout.rs`) + the Lua attribute parser (`src/script.rs`) + a small `carapace-demo` skin extension. **Additive and opt-in; zero behavior change for any skin that does not use the new tables.**

## Motivation

Responsive frame skins already exist in the engine (PR #18): per-element `anchor={left,right,top,bottom}` + `min`, resolved by `layout.rs::resolve_scene` on window resize, with a `frame{}` 9-slice and `view{}` host cutout. But every coordinate is an **absolute design-space unit**. The anchor model can pin, stretch (both edges pinned), ride the far edge, or proportionally re-center a fixed-size element — but it **cannot express a proportional split** ("a sidebar that is 30% of the window width") or **cap a stretch** ("grow with the window, but never past 320px").

This sub-project adds those two levers — **fractional sizing** and a per-element **`max`** — the smallest extension that makes real responsive layouts (splits, capped panels) authorable. It is SP1 of a three-part arc (SP2 = a C-ABI `carapace_resize` seam so FFI-hosted apps reflow at all; SP3 = making the weather app itself responsive). SP1 is pure engine, independently testable, and shippable on its own.

## Approach

**Parallel attribute tables** (chosen over percent-strings on `x/y/w/h`, and over a new grid/flow layout subsystem). Two new optional per-element tables — `frac` and `max` — parsed alongside the existing `anchor`/`min` tables and stored on the existing per-node `Anchors` struct. This matches the codebase's established pattern (`anchor`/`min` are already parallel tables read by `parse_anchors` and carried in `LoadedSkin.anchors`), needs **no change to how `x/y/w/h` are parsed**, and adds **no new primitive** (the vocab count stays 10). Fractional composes with anchors *per axis-field*, with no sibling awareness (consistent with the existing WinForms-style anchor model — no constraint solver).

## Authoring surface

Two new optional tables, valid on **any** primitive (exactly like `anchor`/`min` today):

```lua
-- 30%-wide sidebar, full height, but never wider than 320px:
--   frac.w sets width; anchor top+bottom stretches height; left pin keeps x at 0.
view{ id="nav", x=0, y=34, w=144, h=270,
      anchor={"left","top","bottom"},
      frac={ w=0.30 }, max={ w=320 } }

-- content pane occupying the remaining 70% (x at 30%, width 70%):
--   BOTH x and w are fractional — see "independent per-field" below.
view{ id="app", x=144, y=34, w=336, h=270,
      anchor={"top","bottom"},
      frac={ x=0.30, w=0.70 } }
```

- **`frac = { x?, y?, w?, h? }`** — each present field is a **fraction of the container** along that field's axis: `x` and `w` scale by logical **width**, `y` and `h` by logical **height**. Absent fields stay absolute. Typical range 0–1; values > 1 are allowed (deliberate overflow); negative values clamp to 0.
- **`max = { w?, h? }`** — a **ceiling** on the resolved extent, the mirror of today's `min`. Absent axis = no ceiling.

The absolute `x/y/w/h` remain **required** and continue to define the design-space geometry (used at the design canvas, by authoring tools, and as the value when no `frac` overrides that field). `frac`/`max` never replace them — they modulate the resolved logical rect.

## Semantics (in `layout.rs`)

Resolution stays per-axis and composes with anchors. Today `resolve_axis(p, e, d, l, near, far, min_e)` returns `(pos, extent)`. It gains three inputs — `max_e: Option<f32>`, `frac_pos: Option<f32>`, `frac_ext: Option<f32>` — and resolves in this order:

1. **Anchor-derived** `(np, ne)` exactly as today (both-pinned → stretch `e+delta`; near → `(p, e)`; far → `(p+delta, e)`; neither → proportional re-center `p*(l/d)`).
2. **Fractional override (per field, independent):** if `frac_pos` is set, `np = frac_pos * l`; if `frac_ext` is set, `ne = frac_ext * l`. A fractional value overrides **only that one field** and does **not** feed back into the other field's anchor computation (extent is still derived from design size + pins, never from a frac-overridden position). So "fill the remaining space" is authored by making **both** position and extent fractional — not by mixing a fractional position with a far-edge anchor. (In the split above: `nav` sets `frac.w=0.30` and lets top/bottom anchors stretch its height; `app` sets `frac.x=0.30` **and** `frac.w=0.70` and lets top/bottom anchors stretch its height. Each references only the container — no sibling awareness. If `nav`'s `max` caps it while `app`'s position stays a pure fraction, a gap opens past the cap — that gap is the visible signature of `max`, not a bug.)
3. **Clamp extent** to `[min_e, max_e]`: apply `min` first (raise), then `max` (lower). If an author sets `max < min`, `max` wins (documented). Existing negative/​non-finite guards are retained (`ne ≥ 0`, finite `np`).

`resolve_bbox` feeds each axis its own frac fields (`frac.x`/`frac.w` + `min.w`/`max.w` for horizontal; `frac.y`/`frac.h` + `min.h`/`max.h` for vertical).

### Data model

`Anchors` (in `layout.rs`) gains:
- `max: Option<(f32, f32)>` — mirrors the existing `min: Option<(f32, f32)>`.
- `frac: Frac` where `struct Frac { x: Option<f32>, y: Option<f32>, w: Option<f32>, h: Option<f32> }` (a `Copy` struct so `Anchors` stays `Copy`).

`Anchors::TOP_LEFT` and `from_edges` default `max = None` and `frac = Frac::default()` (all `None`), so the default and every existing call site are unchanged.

### Parser change (`parse_anchors`)

Today `parse_anchors` **early-returns `TOP_LEFT` when no `anchor` field is present**, so `min` is silently ignored unless `anchor` is also set. To make `frac`/`max`/`min` usable on their own (a fractional element need not also declare anchors), `parse_anchors` is refactored to:
1. Build edges from `anchor` if present, else start from `TOP_LEFT`'s edges (no early return).
2. Read `min`, `max` (`{w?,h?}`), and `frac` (`{x?,y?,w?,h?}`) regardless of whether `anchor` was present.

This is a **behavior change only for a skin that sets `min` without `anchor`** (previously ignored → now honored). No shipped skin does this; the existing golden snapshots guard against any unintended change.

## Backward compatibility

- No `frac`/`max` on an element → `resolve_axis` receives `None`/`None`/`None` and behaves **byte-identically** to today.
- Gadget skins (weather, Headspace) never call `layout()` with a non-design size and don't use these tables — unaffected.
- **The existing gadget and frame golden snapshots must remain byte-identical.** This is the primary backward-compat gate.
- No new vocab primitive — `VocabRegistry::base()` stays at 10; its count test is untouched.

## Testing (TDD)

`layout.rs` resolver unit tests (pure geometry, no GPU):
- **Fractional extent:** `frac={w=0.30}` at logical width 1000 → width 300; `frac={h=0.5}` → half height.
- **Fractional position:** `frac={x=0.30}` → x = 0.30 × logical width.
- **`max` ceiling:** an element stretching under both-pinned anchors, capped by `max={w=320}`, stops at 320 past that window size; below it, stretches normally.
- **`min`/`max` interplay:** `min` raises, `max` lowers; `max < min` → `max` wins.
- **Split composition:** the sidebar/content example resolves to a 30/70 split at several window sizes; sidebar honors its `max` cap.
- **Backward compat:** a node with no `frac`/`max` resolves identically to the pre-change `resolve_bbox` (assert exact rects for stretch/far-ride/re-center cases).
- **Parser:** `frac`/`max` are honored when `anchor` is absent (element still defaults to top-left edges); `frac`/`max` parse correctly alongside `anchor`.

Plus: `cargo test --workspace` green; the full local gate (`cargo fmt --all`, clippy workspace + gpu-tests, test) per repo convention; **existing golden snapshots unchanged**.

## Demo (SP1 visual proof)

A **new, minimal `carapace-demo` skin** dedicated to showing fractional reflow — a small "layout playground": a proportional **30% sidebar with `max={w=320}`** (`fill{}` chrome, `frac={w=0.30}`, `anchor={"left","top","bottom"}`) and a **content panel occupying the remaining 70%** (`frac={x=0.30, w=0.70}`, `anchor={"top","bottom"}`). Pure `fill{}` chrome — no host-content wiring. On resize the sidebar/content tile proportionally; once the sidebar hits its 320px cap, it stops and the cap becomes visible as a gap before the content pane.

It is a **separate skin**, not an edit to the existing frame skin, specifically so **every existing golden snapshot stays byte-identical** (the backward-compat gate below). The demo already reflows on `WindowEvent::Resized` via `engine.layout(w,h)`; wiring is just registering/selecting the new skin in `carapace-demo`. On resize: the sidebar holds 30% until it caps at 320px, then stops; the content fills the rest.

## Out of scope

- The C-ABI resize seam (`carapace_resize`) — SP2. Without it, FFI-hosted apps still can't reflow; this sub-project is verified via the in-process demo + unit tests.
- Making the weather app responsive — SP3.
- Grid/flow layout, per-element aspect-lock, or any sibling-relative constraint solving (rejected — the anchor model stays sibling-unaware).
- Percent-string syntax on `x/y/w/h` (rejected in favor of the parallel tables).
- A `max` on **position** (only extent has a ceiling; `min`/`max` are extent-only, mirroring today's `min`).

## Verification

- Unit + golden gate green (all pre-existing goldens byte-identical); `git diff main --stat` limited to `crates/carapace/**` + `crates/carapace-demo/**` + `docs/`.
- `cargo run -p carapace-demo` with the new layout-playground skin: resize the window and watch the 30% sidebar hold its share then cap at 320px while the content pane fills the rest; the existing frame/gadget skins behave exactly as before.

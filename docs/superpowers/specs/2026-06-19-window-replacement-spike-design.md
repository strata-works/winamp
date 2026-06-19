# Total Window Replacement — Feasibility Spike — Design

**Date:** 2026-06-19
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Type:** **Throwaway feasibility spike** (Phase-0-style). Output is *learnings*, not merged
feature code. A later real phase implements total window replacement properly from these findings.

## Purpose

Prove, on macOS/Metal, that a carapace skin can render as a **borderless, transparent,
draggable** window with working drawn controls — true WMP-style "the skin *is* the window,"
not the skin drawn inside a normal app window (how `carapace-demo` has run through Phases
3–5). And **characterize** how far true non-rectangular click-through can go on macOS.

This de-risks the project's founding motivation ("total window replacement") before a full
phase commits to it. See the memory note `total-window-replacement` for the broader plan.

## Success gate (Tier 1 + Tier 2)

The spike succeeds when, running the scratch binary on macOS:

- **Tier 1 — chrome-less floating skin:** the window has no title bar / OS decorations, is
  transparent, and the desktop is visible through the skin's transparent PNG margins — the
  Headspace head floats as a free-form shape.
- **Tier 2 — interaction:** the window can be **dragged** by pressing on the skin body, and
  the Headspace **min/close glyphs work** as real window controls (minimize / quit).

**Tier 3 (characterize, NOT required):** probe whole-window click-through and document the
per-pixel limitation on macOS. A written verdict is the deliverable; making it work is not
required to call the spike a success.

## Scope

**In scope:** a throwaway scratch crate that renders the real `reference` (Headspace) skin
through carapace's actual renderer in a borderless/transparent winit window, supports
drag + min/close, probes click-through, and produces a findings doc.

**Out of scope:** guaranteed per-pixel click-through; cross-platform support (macOS only);
the engine's `Host`/command plumbing for window actions (the later phase's job); window
resize; controls beyond min/close + drag; **any change to `carapace-demo` or the engine
crates** (the spike is self-contained and additive).

## Architecture

A new **throwaway crate `crates/window-spike`** (a binary), added as a workspace member —
the same pattern as the Phase-0 `spike-render` crate that was later pruned. It is deleted
when the real phase lands; nothing depends on it.

```
crates/window-spike/
  Cargo.toml        # deps: carapace (path), winit 0.30, wgpu 29, vello/peniko (via carapace), pollster
  src/main.rs       # borderless+transparent winit window; wgpu surface w/ premultiplied alpha;
                    #   loads the reference skin (carapace), renders its Scene through a local
                    #   mirror of the carapace pipeline with a TRANSPARENT base color;
                    #   drag + min/close + click-through probe wired directly to winit
```

**Why reuse carapace for the skin but mirror the render pipeline locally:** the transparency
risk lives in our actual pipeline shape — vello renders to an intermediate `Rgba8Unorm`
texture, a `TextureBlitter` copies it to the surface, and the surface `alpha_mode` governs
compositing. The spike reuses carapace to **load the real `reference` skin and build its
`Scene`** (`carapace::skin::load_dir` + `Engine`, real assets, real `scene::Node`s), so the
content is genuine. But `carapace::render::Renderer::draw` clears to an **opaque black**
base (`render.rs`), which would defeat transparency at the source — and the engine stays
unchanged in this spike. So the spike **mirrors that same pipeline locally** (the identical
vello → intermediate `Rgba8Unorm` → `TextureBlitter` → premultiplied-surface shape) but with
a **transparent base color**. This still exercises the real transparency risk (premultiplied
surface compositing + the blitter), which is what matters; that `render.rs` should expose a
configurable transparent base is itself a **finding** for the real phase (see below), not a
blocker. The local render loop is throwaway, so duplicating ~50 lines of the `Node`→vello
draw is acceptable spike cost.

**Why wire interaction directly to winit:** drag/controls reaching the OS window is a
windowing concern, not an engine-plumbing concern. Routing skin hotspots → `Host` actions →
window ops is the later real phase's design. The spike does its own minimal hit-testing on a
few hardcoded control rects and calls winit `Window` methods directly, keeping the spike
about windowing feasibility only.

## The windowing knobs under test

These are the concrete things the spike must get right; the exact values that work are
recorded in the findings doc.

| Concern | Approach to try |
|---|---|
| No OS chrome | `WindowAttributes::with_decorations(false)` |
| Transparent window | `WindowAttributes::with_transparent(true)` |
| Per-pixel alpha to desktop | wgpu surface `alpha_mode = CompositeAlphaMode::PreMultiplied` (today the demo uses `caps.alpha_modes[0]`); fall back to `PostMultiplied` if Pre is unsupported — record which the adapter offers |
| Transparent render background | the spike's local pipeline mirror clears to a **transparent** `base_color` (vello `RenderParams.base_color` with alpha 0); that `carapace::render::Renderer` hardcodes opaque black and should expose a configurable base is recorded as a finding for the real phase |
| Drag | press on skin body (not on a control rect) → `Window::drag_window()` |
| Minimize | min-glyph rect press → `Window::set_minimized(true)` |
| Close | close-glyph rect press → `event_loop.exit()` |
| Click-through (probe) | `Window::set_cursor_hittest(false)`; document whole-window vs per-pixel reality on macOS |

**Note on `base_color`:** because `carapace::render::Renderer::draw` clears to opaque black
and the engine stays unchanged here, the spike renders through its own local mirror of the
pipeline with a transparent base (above). The finding to record for the real phase is the
exact interface `render.rs` should expose — most likely a configurable `base_color` (or an
alpha on it) on `Renderer::draw` / `RenderTarget` — so the real feature renders transparent
through the engine's own renderer rather than a copy.

## Deliverables

1. The working `crates/window-spike` scratch binary.
2. A **screenshot** committed under the spike (head floating chrome-less on the desktop, with
   drag/controls demonstrated) — the human-visual evidence.
3. A **findings doc** `docs/superpowers/specs/2026-06-19-window-replacement-spike-findings.md`
   recording: the exact winit/wgpu knobs that worked, the surface `alpha_mode` the macOS
   adapter offered, whether the renderer needed a transparent-base interface (and what shape),
   the drag/controls result, and the **click-through verdict** (what `set_cursor_hittest`
   does, and whether per-pixel passthrough is reachable on macOS and how). This doc is the
   primary input to the later real phase.

## Testing & verification

This is a throwaway GUI spike — verification is **human-visual**, as with the Phase 0/1
interactive spikes:
- `cargo run -p window-spike` launches a borderless, transparent window; the Headspace head
  floats with the desktop visible around it; dragging the body moves the window; the min and
  close glyphs minimize / quit.
- The only automated bar is that the crate **builds and launches** (no panic on startup);
  `cargo build -p window-spike` and `cargo clippy -p window-spike` are clean.
- No unit/integration tests are written for throwaway code beyond the build/launch check.

## Error handling

Spike-grade: it may `expect`/`unwrap` on setup (adapter, surface, skin load) since it is not
production code. It must not panic during normal interaction (drag, control clicks). If a
required capability is unavailable (e.g. no transparent surface support), that is a **recorded
finding**, not a crash to paper over.

## Definition of done

The scratch binary renders the real Headspace skin in a borderless, transparent window on
macOS with the desktop showing through its margins; dragging the body moves it and the
min/close glyphs work (Tier 1 + 2); the click-through behavior is probed and documented; a
screenshot and the findings doc are committed. The engine and `carapace-demo` are unchanged.

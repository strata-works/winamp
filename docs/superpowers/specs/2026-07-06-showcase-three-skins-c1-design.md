# Showcase Sub-project C1: Three Polished Skins + Per-Skin Sizing

**Date:** 2026-07-06
**Status:** Design — approved direction, pending spec review
**Parent program:** `2026-07-06-showcase-three-skins-design.md` (Sub-project C, part 1)
**Depends on:** Sub-project B (the SwiftUI app, merged PR #36 on `main`). Build C1 on a branch off `main`.

## Goal

The polish round. Replace the deliberately-plain `starter`/`alt` skins with the three real concept
skins — **Faceplate, Studio Deck, Cassette** — each at its own native size/shape, hot-swapping over
the same Swift-owned `MusicHost`. This makes the app the live embodiment of the concept board
(`.superpowers/brainstorm/.../music-realistic-carapace.html`).

C1 is **skins + per-skin sizing**. The filesystem music picker is **C2** (separate cycle). A
`shader{}`-driven Cassette is a future extension (see below).

## Decisions (from brainstorming)

- **Per-skin native sizes** (not a shared canvas): each skin is its own size/shape.
- **Swap mechanism = destroy + recreate** (simplest): on Tab, tear down the engine + IOSurface pool
  and create a new one at the next skin's size; resize the window. Audio/state persist (Swift owns
  them). Accepted cost: a brief GPU re-init flash per swap. (FFI v4 "resize-swap" — swap the skin +
  re-point the pool without engine teardown — is a noted future optimization to remove the flash.)
- **No `carapace`/`carapace-ffi` changes** in C1 — app (Swift) + skins (Lua) only.

## Part 1 — App enhancement: per-skin size via destroy+recreate

Today `AppDelegate` creates one `CarapaceBridge` at a fixed 420×660 and reuses it (`cycleSkin` calls
`bridge.swap`). Change to recreate per skin:

- **Canvas source:** each skin's `skin.toml` declares `canvas = { width = W, height = H }`. Add a
  small Swift parser (`SkinManifest.canvas(atSkinDir:) -> (Int, Int)`) that reads the toml and
  extracts width/height (regex/scan for `width = N`/`height = M` — no TOML dependency). One source
  of truth; the app never hardcodes per-skin sizes.
- **Swap (`cycleSkin`) becomes recreate:**
  1. `bridge = nil` / destroy the current bridge (its `deinit` calls `carapace_destroy`, stopping the
     render thread and engine). Clear the global `frameSink` on teardown.
  2. Read the next skin's `(w, h)` from its toml. Resize the borderless window
     (`window.setContentSize(NSSize(width: w, height: h))`, keep top-left) and the `SkinView` frame.
  3. Create a new `CarapaceBridge(skinDir:, width: w*scale, height: h*scale, onFrame:)` — a fresh
     IOSurface pool at the new size. The new bridge re-points `frameSink` (old engine already gone →
     no stale-frame misfire).
  4. Update the per-skin pointer mapping (see below).
- **Per-skin pointer mapping:** replace the global `CANVAS_W`/`CANVAS_H` constants with `SkinView`
  instance vars `canvasW`/`canvasH` that `AppDelegate` sets to the active skin's canvas on each
  (re)create; `canvasPoint(_:)` maps clicks against them.
- **State persistence:** `MusicHost` + `AVAudioPlayer` are Swift-side and untouched by the engine
  teardown/recreate, so playback/position/volume/selection continue seamlessly across the swap
  (only the rendered skin flashes/rebuilds).
- **Initial skin:** the app opens on `faceplate` (first in the cycle), sizing the window to its
  canvas at launch (the same canvas-from-toml path, applied once at startup).

The Tab cycle list becomes `["showcase/skins/faceplate", "showcase/skins/studio", "showcase/skins/cassette"]`.

## Part 2 — The three skins (Lua, native sizes)

All three bind the same host surface: state `track_title`/`artist`/`time`/`position`/`volume`/
`playing`/`viz_*`; collection `playlist` (`now`/`title`/`artist`/`duration`); actions `toggle_play`/
`stop`/`next`/`prev`/`seek`/`set_volume`/`play_index` + `begin_drag`/`minimize`/`close`. Each is a
shaped, alpha-authored body over the transparent window with a `role='drag'` whole-body region
(declared first, controls after so the topmost hotspot wins hit-test) and min/close glyphs.

### Faceplate — **380×560** (compact portrait gadget)
Shaped rounded silhouette (asymmetric `rounded_rect`) + radial-gradient glow. LCD panel (gradient
text: `track_title` / `artist` / `time`). Horizontal seek `scrub` (`position`→`seek`). A transport
row of hotspots: prev / stop / **play** (toggle_play) / next, plus a compact volume `scrub`
(`volume`→`set_volume`). A detached queue drawer below: `list{ collection="playlist" }` with
active-row highlight (`selected="current_index"`).

### Studio Deck — **720×480** (landscape full surface)
Title bar (`track_title`/`artist`/`time`). A visualizer drawn as `value_fill` bars reading
`viz_0..N`. A volume `scrub` styled as a slider, with value-reactive "knob" `circle` decorations
(decoration only — no rotary drag). Transport row → `toggle_play`/`prev`/`next`/`stop`. Lower
two-column area: playlist `list` + a "library" flavor panel (static count text). **Fixed size** —
true resizable frame skins are a separate future feature, out of scope here.

### Cassette — **600×400** (landscape object)
Cassette body (gradient) with two reels via **`sweep` (conic) gradient** + hub `circle`s; tape label
(`track_title`/`artist`); cassette window; four keys (prev / **play** / stop / next); a slim
`position` scrub styled as a tape counter/progress. `role='drag'` body; min/close.

**Fidelity:** high, vocab-native — linear/radial/**sweep** gradients, `rounded_rect`/`circle`/polygon
paths, gradient text, images. **Honest limits (matched or noted, not faked):** reels are static
(no per-node rotation → no spin); no soft-shadow blur (layered fills/gradients approximate depth);
knobs are decoration + a real linear volume `scrub`.

## Testing & verification

- **Swift unit test** for the `skin.toml` canvas parser: valid toml → `(w, h)`; missing/malformed →
  a sensible fallback (e.g. the previous 420×660) without crashing.
- **Skin-load check:** each skin must load — a bad vocab arg makes `carapace_create` fail
  (`carapace_create failed` logged, bridge init nil). Verified by launching on each skin.
- **Manual run (user- or agent-assisted):** launch (opens on Faceplate at 380×560); press Tab →
  window resizes to Studio Deck (720×480), then Cassette (600×400), then back; confirm
  playback/position/volume/selection persist across the size-changing swaps; screenshot each skin.
- Not CI-gated (Swift). `carapace`/`carapace-ffi` unchanged (app + skins only).

## Future extensions (out of scope for C1)

- **Shader-driven Cassette (`view{}` + content IOSurface):** the FFI create desc already accepts a
  `content_surface` for a `view{ id="host" }` cutout (B passes `null`). A future version can have the
  Cassette declare a `view{}` cutout (tape window or a panel behind the reels) and render an animated
  **paper.design mesh-gradient shader** into the content IOSurface from Swift (Metal/wgpu) — the
  pattern proven by the paper-shader Phase 2 spike and the Flutter torch demo, optionally
  **viz-driven** (pulses with the audio). No engine change (reuses the existing content-surface seam).
  A first-class `shader{}` vocab primitive is the heavier alternative.
- **FFI v4 resize-swap:** remove the destroy+recreate flash by swapping the skin and re-pointing the
  IOSurface pool in one engine (a carapace-ffi cycle).
- **True resizable Studio Deck** (frame skin anchors) — the responsive-frame-skins feature.

## Out of scope for C1

- The filesystem music picker (Sub-project C2: Cmd+O menu + skin open button + mutable playlist +
  AVAsset metadata).
- Any `carapace`/`carapace-ffi` change; reel spin; soft shadows.

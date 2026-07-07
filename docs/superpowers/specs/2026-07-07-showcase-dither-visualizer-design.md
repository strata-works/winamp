# CarapaceShowcase — Studio Dither Visualizer (live, music-reactive) — Design

**Date:** 2026-07-07
**Branch:** `showcase-dither` (off `showcase-c2`, which is off `main`)
**Depends on:** C2 (`showcase-c2`, unmerged) for the current showcase; the paper-shader spike
(`worktree-paper-shader-transpile-spike`) for the vendored `dithering.wgsl` reference.

## Goal

Replace Studio Deck's fake time-driven visualizer bars with a **live, music-reactive dither
field** — paper.design's dithering effect, driven by real audio level. This is the first use of
the engine's `view{}` + host `content_surface` compositing path in the showcase.

A dedicated "Dither" skin (dither as the whole-window hero) is a **later** follow-up; this spec
covers the Studio-panel version only.

## Non-negotiable: zero engine changes

Everything needed is already merged on `main`:
- `view{ id, dest }` primitive (`vocab.rs` `ViewPrim` → `Node::View`).
- The view compositor blits a host-supplied texture into each view's dest rect with
  **premultiplied-alpha blending** (`render.rs:207`), sampling the **entire** content texture
  (UV 0→1) into the dest viewport (`render.rs` view-composite pass + `composite.wgsl`).
- `carapace-ffi` imports a host `content_surface` IOSurface for `view{ id="host" }` and
  **re-uploads it every frame** (`render_thread.rs` `render_one`: "Upload this frame's host
  content …"), so host animation is picked up per frame. ABI: `content_surface` must match the
  main surface size (`carapace.h:267-275`).

`CarapaceBridge.swift:54` already passes `content_surface` — currently `nil`. This feature makes
it non-nil while Studio is active.

## Architecture

```
RealAudioPlayer (metering) --level--> MusicHost.level --\
main-runloop Timer (~60fps) --time--------------------------------------> DitherRenderer (Metal)
                                                            |  renders dither.metal into
                                                            v  a full-canvas BGRA IOSurface
                                            content_surface (IOSurface)
                                                            |
                                              CarapaceBridge (desc.content_surface)
                                                            |
                                    engine composites into view{ id="host" } (Studio's viz panel)
```

### Components (each one job, testable in isolation)

- **`dither.metal`** (new shader) — a clean MSL reimplementation of paper's dithering effect:
  **Bayer ordered-dither between two colors over an animated warp field**, matching paper's look.
  Uniforms: `time`, `level`, `resolution` (the **cutout** size, so the 0→1 stretch-into-dest
  doesn't distort the pattern), `colorBack`, `colorFront`, `pxSize`. _Decision: reimplement cleanly
  rather than mechanically port the ~600-line transpiled `dithering.wgsl` — same visual, far more
  maintainable, and the host is Metal (can't run WGSL directly anyway)._
- **`DitherRenderer`** (new, Swift) — owns a full-canvas BGRA IOSurface + a Metal render pipeline;
  `render(time:level:)` draws one frame into the IOSurface. Driven by a main-runloop `Timer` (~60fps; CVDisplayLink not used — NSView CADisplayLink needs macOS 14, this project targets 13). Exposes
  the IOSurface for the bridge. Uniform packing (`level`/`time`/aspect/colors → the buffer) is a
  pure function, unit-tested.
- **`RealAudioPlayer.level`** — `isMeteringEnabled = true`; `level: Float` calls `updateMeters()`
  and normalizes `averagePower(forChannel:0)` (dB, ≈ −60…0) to a smoothed 0…1
  (`normalizeDB(_:) -> Float`, pure + tested; smoothing via a simple EMA).
- **`MusicHost.level`** — exposes the player's current level to the renderer (read-only pass-through).
- **`CarapaceBridge`** — gains a `contentSurface: IOSurfaceRef?` init param, passed into
  `CarapaceCreateDesc.content_surface`.
- **`AppDelegate`** — creates the `DitherRenderer` + its IOSurface sized to the current skin;
  **gates it to Studio only**: in `applySkin`, when the skin is `studio` it builds the dither
  IOSurface (at the skin's backing-scaled size), starts the `Timer`, and passes the surface
  as `contentSurface`; for other skins it stops the loop and passes `nil`.
- **`studio/skin.lua`** — drop the `value_fill{ value="viz_N" }` lines; add
  `view{ id="host", x=20, y=74, w=474, h=214 }` over the baked viz-glass panel.

## Music mapping

Normalized, smoothed level `L ∈ [0,1]` (EMA of `normalizeDB(averagePower)`):
- **Front-color brightness / coverage** scales with `L` — louder ⇒ brighter, denser phosphor dither.
- **`time`** drives a continuous slow warp of the underlying field (motion even in quiet passages).
- Tasteful: a smoothed pulse, never a strobe (EMA smoothing + clamped range). Exact curve tuned in
  the plan and confirmed visually.

Colors match Studio: `colorBack` = near-black; `colorFront` = Studio's blue (`~77,160,240`).

## Aspect handling

The compositor samples the whole content texture (UV 0→1) into the Studio cutout dest
(`474×214`, aspect ≈ 2.21:1) even though the content IOSurface is full-canvas (`720×480`, 3:2). The
shader receives the **cutout** resolution/aspect as `resolution` and computes the Bayer grid + field
in that space, so the pattern reads correctly after the stretch (no squished dither cells).

## Frame sync

The engine free-runs (~60 fps) and composites whatever is currently in `content_surface`; the
`DitherRenderer` redraws on its own main-runloop `Timer`. They are intentionally unsynchronized — for a
soft two-color dither, any tear is invisible. No locking needed on the pixel data: single writer (the
main-thread Timer callback), and the engine reads by CPU-copying always-valid IOSurface bytes on its
render thread. This lock-free safety holds only while that read stays a CPU copy (see the note in
`DitherRenderer.render()`); a future Tier-2 GPU-alias of the content surface would need a fence.

## Testing

- **`swift test` (unit):**
  - `RealAudioPlayer` / metering: `normalizeDB(_:)` maps −60 dB→0, 0 dB→1, clamps out-of-range;
    EMA smoothing moves toward the target and never exceeds [0,1].
  - `DitherRenderer` uniform packing: `level`/`time`/colors/aspect land in the buffer at the
    expected offsets/values (pure function, no GPU).
- **Manual (README checklist):** run the app, Tab to Studio, confirm the viz panel shows the dither
  field and that it visibly pulses/brightens with the audio (play a track, watch it react); confirm
  Faceplate/Cassette are unchanged and hot-swap still preserves playback.
- Not CI-gated (no Swift/Metal in CI), matching the showcase's existing manual-verification note.

## Out of scope

- The dedicated whole-window "Dither" skin (later follow-up).
- Per-band / FFT reactivity (this spec uses a single averaged level).
- A first-class engine `shader{}` primitive or build-time transpile pipeline (paper spike's
  productization recommendation — separate effort).
- Reacting on Faceplate/Cassette (dither is Studio-only here).

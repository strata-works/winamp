# Native Showcase: One Music Player Host, Three Complete Skins (macOS)

**Date:** 2026-07-06
**Status:** Program design — approved direction; decomposed into sub-projects (see below)
**Origin:** Builds the static concept board (`.superpowers/brainstorm/.../music-realistic-carapace.html`)
into a live, **native macOS** demo.

## Goal

Prove Carapace's founding thesis — **total window replacement, one host, many bodies** — as a
live, **native macOS SwiftUI app**: three visually distinct music-player skins (Faceplate, Studio
Deck, Cassette) that hot-swap over a single Swift-owned music host, each rendered by the Rust engine
and displayed zero-copy through an **IOSurface**, in a borderless window shaped by the skin. Playback,
seek position, playlist selection, and volume survive every skin swap.

## Chosen architecture: native Swift host + carapace-ffi + IOSurface

The delivery vehicle is a **native AppKit/SwiftUI `.app`** that embeds the Rust engine across the
`carapace-ffi` C ABI. Roles (confirmed against `crates/carapace-ffi`):

- **Swift is the host.** The FFI vtable is `{ ctx, get_num, get_str, invoke, frame_ready }`
  (`carapace.h`). The playlist, playback (AVFoundation), and all state/actions live in **Swift**;
  the Rust engine is a renderer + input router that calls back into Swift and lands frames in a
  host-provided IOSurface pool.
- **State survives skin swap for free.** Because state lives in the Swift host, not the engine, a
  skin swap changes only what is drawn — the host is untouched.
- **Swap is content-only.** The render thread renders the engine's design canvas *scaled into* a
  fixed `w×h` IOSurface pool (`render_thread.rs`: surfaces are `w×h`; the engine holds `cw×ch`
  design canvas and `handle_pointer_resolved` uses `cw,ch`). If all three skins share ONE design
  canvas and paint their distinct shapes with transparency, a swap needs **no pool/window resize** —
  the engine swaps the skin (`Engine::handle_command(Command::Swap(..))`) keeping the host.
- **Shaped, draggable window.** `carapace_hit_test` already classifies a point as
  `Passthrough | Control | Drag`; the Swift host uses it to move the borderless window and to shape
  it (alpha from the rendered surface / a mask), realizing total window replacement.

### Considered and rejected: winit standalone

An earlier draft targeted the `carapace-demo` winit app (transparent/draggable window +
`Command::Swap` already work there). Rejected because "this is a macOS app" calls for a genuinely
native `.app` with zero-copy IOSurface display, not winit. The winit path remains a fast *interaction*
sandbox if we ever need to prototype a skin quickly, but it is not the deliverable.

## The blocker: carapace-ffi ABI (v2.0) is too minimal for a music player

The ABI was proven with a battery-bar spike (scalar state + one parameterless toggle). The three
music skins need three capabilities it does not yet have (verified in source, not memory):

1. **No skin hot-swap export.** `carapace.h` has create/destroy/invalidate/pointer/hit_test/… but
   **no `carapace_swap_skin`**. Today, swapping = destroy + recreate, which tears down and rebuilds
   wgpu + the render thread every swap. Wrong mechanism for a hot-swap demo.
2. **No collections/rows.** `FfiHost::rows()` returns `Vec::new()` — *"collections out of scope for
   the spike"* (`host.rs:108`). All three skins center on a playlist/queue `list{}`.
3. **Action arguments are dropped.** The vtable `invoke(ctx, name)` carries no args and
   `FfiHost::invoke` ignores `_args` (`host.rs:102`). So `seek(fraction)`, `set_volume(level)`, and
   `play_index(i)` — the scrub bar, volume slider, and click-a-track — cannot convey their value.

## Decomposition & sequencing (chosen: FFI-first)

Three sub-projects, built in dependency order. Each is its own spec → plan → implementation cycle.

- **Sub-project A — carapace-ffi v3 (BUILD FIRST).** Extend the ABI so a host can drive a
  multi-skin, list-and-argument-driven app: `carapace_swap_skin`, a rows/collections path in the
  vtable, and invoke-with-argument. Additive ABI bump (MINOR where signatures are added, MAJOR only
  if an existing signature must change). Detailed spec:
  `2026-07-06-carapace-ffi-v3-design.md` (this is the one we plan next).
- **Sub-project B — SwiftUI macOS host app.** The music host in Swift (playlist + AVFoundation
  playback + state/actions over the vtable), IOSurface display (CAMetalLayer / `NSViewRepresentable`),
  borderless shaped window driven by `carapace_hit_test`, and the hot-swap UI (Tab / buttons).
- **Sub-project C — the three skins.** Lua skins (Faceplate, Studio Deck, Cassette) authored to ONE
  shared design canvas, bound to the Swift host's state keys and collections. Developable alongside B
  once A lands.

## Shared design decisions (apply across sub-projects)

- **One shared design canvas** for all three skins (e.g. a common max bound); each paints its shape
  with transparency so the IOSurface pool + window size stay constant across swaps.
- **Host state surface** (Swift-owned, served over the vtable): `track_title`, `artist`, `time`,
  `position` (0..1), `playing`, `volume` (0..1), `viz_0..N`; collection `playlist` with row fields
  `now`/`title`/`artist`/`duration`; actions `toggle_play`, `stop`, `next`, `prev`,
  `seek(f)`, `set_volume(f)`, `play_index(i)`, plus window `begin_drag`/`minimize`/`close`.
- **Honest limits:** no per-node rotation (cassette reels won't spin); no soft-shadow blur (layered
  fills/gradients approximate depth); rotary knobs are decoration + a real linear volume control.

## Out of scope (this program)

- iOS / Flutter / WidgetKit hosts (separate spikes already exist).
- Live `view{ id="host" }` content cutout (supported by the ABI, but not needed by these skins).
- Regenerating the HTML concept board.

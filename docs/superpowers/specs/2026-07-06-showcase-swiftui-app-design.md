# Showcase SwiftUI App (Sub-project B) Design

**Date:** 2026-07-06
**Status:** Design — approved, pending spec review
**Parent program:** `2026-07-06-showcase-three-skins-design.md` (Sub-project B)
**Depends on:** carapace-ffi v3 (ABI 3.0) — PR #35 (`2026-07-06-carapace-ffi-v3-design.md`).
Build B on the `showcase-three-skins` branch (or main once #35 merges) so the v3 ABI is present.

## Goal

A native **macOS SwiftUI app** — the first real consumer of `carapace-ffi` — that embeds the
carapace engine, renders skins zero-copy through an IOSurface pool, and displays them in a
borderless/transparent/draggable window. Swift owns the music host (playlist + AVFoundation
playback + state/actions); the engine renders and routes input. Hot-swapping skins keeps playback,
position, volume, and selection because state lives in Swift, not the engine. This validates the
entire v3 ABI (swap + rows + numeric action args) end-to-end and is the vehicle Sub-project C's
three concept skins will drop into.

## Location & package

New SwiftPM package at repo root: `showcase/` (package `CarapaceShowcase`, executable target
`Showcase`, `.macOS(.v13)`). Not under `crates/embed-spike/*` — those are frozen spikes; this is the
flagship app. CI does not build Swift (no change to CI); verification is manual/agent-driven.

## Architecture (layers)

- **`CCarapace` (systemLibrary target)** — `module.modulemap` referencing the committed
  `carapace.h` (copied/symlinked from `crates/carapace-ffi/include/`), `link "carapace_ffi"`, and
  linker flags `-L ../target/debug -lcarapace_ffi` + an `-rpath` to `target/debug`. Mirrors the
  macos-sample's `CCarapace` shim, retargeted from `embed_spike` to the productionized dylib.
- **SwiftUI shell** — `@main struct ShowcaseApp: App` provides the app entry via
  `NSApplicationDelegateAdaptor`. A `.borderless` window cannot become key (needed for input) and a
  SwiftUI `WindowGroup`'s `NSWindow` cannot be subclassed — so the **AppDelegate owns the window**:
  in `applicationDidFinishLaunching` it creates a custom `SkinWindow: NSWindow` (`styleMask =
  [.borderless]`, overriding `canBecomeKey`/`canBecomeMain` = `true`), sets `isOpaque = false`,
  `backgroundColor = .clear`, non-resizable, and installs the layer-backed skin `NSView` as its
  content view. SwiftUI remains the structural app (`App`/`Scene`), with a hidden/`Settings` scene
  so no default chrome window appears. (This is the "SwiftUI-hosted" choice made concrete; a
  `WindowGroup` + `WindowAccessor` variant was rejected because the backing window can't be reclassed
  to become key.)
- **`SkinNSView` (layer-backed NSView)** — hosts a `CALayer` whose `contents` is the current
  IOSurface. Handles `mouseDown`/`mouseDragged`/`mouseUp` and key input. Created at the window's
  backing scale (2× on Retina) for sharp skins; the CALayer maps surface pixels 1:1.
- **`MusicHost` (Swift, `ObservableObject`)** — the single source of truth: `[Track]` playlist
  (title/artist/path/duration), `current` index, `playing`, `AVAudioPlayer`, `volume`. Survives
  skin swaps.
- **`CarapaceEngine` bridge (Swift)** — owns the `CarapaceEngine*` handle, the IOSurface pool
  (`[IOSurface]`, 3 surfaces), the content-surface (unused here → null), and the `CarapaceHostVTable`.
  Creates via `CarapaceCreateDesc`; exposes `swapSkin`, `pointer`, `hitTest`, `releaseSurface`,
  `destroy`.

## The Swift host over the v3 vtable

Vtable callbacks are top-level C functions (no captured context — the macos-sample's weak-box
pattern routes to the live `MusicHost`/`SkinNSView`):

| Callback | Serves |
|---|---|
| `get_num(key)` | `position` (0–1), `volume` (0–1), `playing` (0/1), `current_index`, `viz_0..N` |
| `get_str(key)` | `track_title`, `artist`, `time` ("m:ss / m:ss") |
| `row_count("playlist")` | playlist length |
| `get_row_str("playlist", i, field)` | `now` ("▶"/""), `title`, `artist`, `duration` |
| `invoke(name)` | `toggle_play`, `stop`, `next`, `prev`, `begin_drag`, `minimize`, `close` |
| `invoke_arg(name, f)` | `seek(f)` (0–1), `set_volume(f)` (0–1), `play_index(i)` |
| `frame_ready(idx, id)` | render thread → display (see below) |

- **Playback** — `AVAudioPlayer`: `currentTime`/`duration` for `position` + `seek`; `.volume` for
  `volume`/`set_volume`; play/pause via `play()`/`pause()`; `next`/`prev` load the adjacent track and
  continue playing; auto-advance on finish (via `AVAudioPlayerDelegate` or polling).
- **`viz_*`** — time-driven fake (bass-weighted, flat when paused), matching both existing Rust
  hosts. No real FFT.
- **Window actions** — `begin_drag`/`minimize`/`close` map to `performDrag`/`miniaturize`/`terminate`.

## Render/display, input, shaped window

- **Display** — carapace free-runs its render thread at 60fps into the IOSurface pool. `frame_ready`
  fires on the render thread; it dispatches to `main` and: `skinLayer.contents = surfaces[idx]`
  (re-assigning even the same surface object is the documented way to refresh an IOSurface-backed
  layer), then `carapace_release_surface(prevIdx)`. This replaces the sample's `carapace_tick` loop
  — v3 has no tick.
- **Input** — on `mouseDown`, map the point to design-canvas coordinates (backing-scale → points →
  canvas), then `carapace_hit_test(x,y,&kind)`: `Drag` → begin window drag; `Control` →
  `carapace_pointer(x,y,Press)` (drives skin controls → host actions synchronously, incl. scrub
  fraction and row index via `invoke_arg`); `Passthrough` → ignore (clicks fall through transparent
  margins). `.borderless` windows need an `NSWindow` subclass that returns `canBecomeKey = true`.
- **Coordinate mapping** — all skins share ONE design canvas (program contract), so a single fixed
  scale maps view points to canvas points; no per-swap remap.

## Hot-swap

A key (Tab) and a menu item call `carapace_swap_skin(dir)` over the bridge. `MusicHost` is untouched,
so playback/position/volume/selection persist. B cycles **[starter ↔ `reference`]**:
- **`showcase/skins/starter/`** — one bespoke Lua skin B authors, bound to the FULL host surface
  (title/artist/time/position-scrub/volume-scrub/playlist-list/viz-bars + transport hotspots +
  drag/minimize/close). Deliberately plain — its job is to exercise every binding, not to be pretty.
- **`crates/carapace-demo/skins/reference`** — an existing base-vocab skin as the second swap target,
  proving swap + state-persistence without authoring a second bespoke skin.
Both share the one design canvas. Sub-project C replaces this cycle with Faceplate/Studio/Cassette.

## Testing & verification

- **Swift unit tests** (Swift Testing) for pure `MusicHost` logic: playlist next/prev/wrap,
  `current_index`, `seek`/`set_volume` clamping to 0–1, `time` formatting, and `playlist` row
  generation (now/title/artist/duration) — the logic that needs no window.
- **Agent-driven run** (`agent-device`): build (`cargo build -p carapace-ffi` then `swift build`),
  launch the app, and drive it — start playback, verify a skin renders (screenshot), press Tab to
  hot-swap, and confirm playback/position/volume/selection persist across the swap (screenshot each
  skin; assert via `agent-device is`/`get` where the UI is inspectable, else visual/screenshot diff).
  First verification step confirms `agent-device` can target this macOS app; if not, fall back to
  `swift run` + `screencapture`.

## Honest limits / out of scope

- Not CI-gated (no Swift in CI, like the other spikes).
- `viz_*` is faked; no real audio spectrum.
- Starter skin is functional-not-pretty; visual polish is Sub-project C.
- Single window, single skin at a time; no live `view{}` host-content cutout (not needed here).
- The three concept skins (Faceplate/Studio/Cassette) are Sub-project C, not this spec.

## Definition of done

- `swift run` (after `cargo build -p carapace-ffi`) launches a borderless/transparent/draggable
  window showing the starter skin rendered by carapace via IOSurface.
- Playback works (AVAudioPlayer); transport + scrub + volume + click-a-track all drive `MusicHost`
  through the vtable (incl. `invoke_arg` and the rows callbacks).
- Tab hot-swaps starter ↔ reference with playback/position/volume/selection intact.
- `MusicHost` unit tests pass; an agent-driven (or manual) run is captured with per-skin screenshots.

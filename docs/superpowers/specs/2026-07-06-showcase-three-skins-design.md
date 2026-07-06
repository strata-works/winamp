# Showcase: One Music Player Host, Three Complete Skins

**Date:** 2026-07-06
**Status:** Design — approved, pending spec review
**Origin:** Builds the static concept board (`.superpowers/brainstorm/.../music-realistic-carapace.html`)
into a live, running macOS demo.

## Goal

Prove Carapace's founding thesis — **total window replacement, one host, many bodies** —
as a live demo: three visually distinct music-player skins (Faceplate, Studio Deck, Cassette)
that hot-swap over a single persistent `MusicPlayerHost`, on macOS, each rendering as a real
shaped/transparent/draggable window. Playback, seek position, playlist selection, and volume
survive every skin swap because state lives in the host, not the skin.

This is what the concept board only *asserts* statically; here it actually runs.

## Context: what already exists (reused, not rebuilt)

Exploration of `crates/carapace-demo` and `crates/carapace` established that the hard parts
already ship:

- **Hot-swap over a persistent host.** `MEDIA_SKINS` already cycle over one long-lived
  `MusicPlayerHost` via `Command::Swap(src)` (keeps host + state) on the Tab key. `SwitchHost`
  is only used to change hosts (the `h` sysmon toggle). The showcase reuses `Command::Swap`
  verbatim — the state-survival proof is the existing mechanism, not new plumbing.
- **Shaped/transparent/draggable windows, on macOS, today.** The demo window is created with
  `.with_decorations(false).with_transparent(true)` (`main.rs:369`); the main window surface is
  cleared with `base_color` alpha `0` (`main.rs:613`), so unpainted canvas reads through to the
  desktop; body-press already calls `w.drag_window()` (`main.rs:343`). The `classic` skin already
  floats as a shaped, draggable object ("rounded corners float over the desktop via the
  transparent base"). `window-spike` proved the same transparency pipeline. **Shaped windows are
  free — skins just author with alpha.**
- **Rich vocab.** Native primitives cover the board's look: `fill`/`region`/`value_fill`/`image`/
  `text`/`list`/`scrub`/`view`, with path helpers `rect{}`, `rounded_rect{x,y,w,h,radius}`,
  `circle{cx,cy,r}` and arbitrary polygon point-lists, plus **linear/radial/sweep gradients** and
  gradient text with custom fonts. The cassette's conic reels (`sweep`), the faceplate's radial
  glow, and LCD text are all achievable natively.
- **Window controls as host actions:** `host.begin_drag()`, `host.minimize()`, `host.close()`.
- **Faux spectrum:** the host already exposes `viz_<i>` (0..1, time-driven, flat when paused) that
  `value_fill` bars can read for a live visualizer.

## What's new (the actual work)

Three things: a launch entry, a small host extension, and three authored skins.

### 1. Entry point — showcase launch mode

Add a startup argument to the existing binary: `cargo run -p carapace-demo -- showcase`.

- Selects a new `SHOWCASE_SKINS = ["skins/faceplate", "skins/studio", "skins/cassette"]` list
  over a `MusicPlayerHost`, starting at index 0.
- Tab cycles the three skins (existing cycle logic, pointed at the new list).
- Default launch (no arg) is **unchanged**: `MEDIA_SKINS` + the `h` sysmon toggle are untouched.
- The `h` toggle is disabled/ignored in showcase mode (the showcase is music-only and
  self-contained); Esc still exits.

Rationale: reuses ~700 lines of winit/wgpu/event wiring with near-zero disturbance to existing
behavior. Rejected alternatives: folding a third mode into the `h` rotation (entangles modes);
a standalone `examples/showcase.rs` (duplicates the window/render harness, drift risk).

### 2. Host extension — artist + volume

Extend `MusicPlayerHost` (in `crates/carapace-demo/src/music_player_host.rs`) and the audio
backend so the board's artist line and volume control are **real bindings**, giving a second
live piece of state (volume) that visibly survives swaps.

- **`artist`** — add an `artist` field to each playlist `Track`; `get("artist")` returns the
  current track's artist string. Populate the reference playlist with artist names.
- **`volume`** — add a `volume: f32` field (default e.g. `0.8`); `get("volume")` returns it as a
  `Scalar`.
- **`set_volume(f)`** action — clamp `f` to `0.0..=1.0`, store it, and apply it to the audio
  backend.
- **`AudioBackend::set_volume(&mut self, v: f32)`** — new trait method:
  - `RodioBackend`: call the sink's `set_volume(v)` (rodio supports gain).
  - `MockAudioBackend`: record the value in `MockAudioState` (for tests).
  - `NullBackend`: no-op.
- `viz_<i>`, `time`, `position`, `track_title`, `playing`, `playlist` rows: unchanged.

### 3. The three skins

New skin directories under `crates/carapace-demo/skins/`, each with `skin.lua` + `skin.toml`
(manifest declaring canvas size + archetype). All three bind the **same** capability surface:

| Binding            | Key / action                          |
|--------------------|---------------------------------------|
| Track title        | `text{ value="track_title" }`         |
| Artist             | `text{ value="artist" }`              |
| Time               | `text{ value="time" }`                |
| Seek               | `scrub{ value="position", on_seek="seek" }` |
| Play/pause         | `region/fill on_press → host.toggle_play()` |
| Volume             | `scrub{ value="volume", on_seek="set_volume" }` |
| Visualizer         | `value_fill` bars reading `viz_0..N`  |
| Playlist           | `list{ collection="playlist", on_select="play_index" }` |
| Window drag/min/close | `host.begin_drag()` / `host.minimize()` / `host.close()` |

**A. Faceplate** — gadget archetype, fixed aspect (uniform-zoom resize), authored **alpha** so it
floats as a shaped window. Shaped body via `rounded_rect`/polygon + radial-gradient glow; LCD panel
(gradient text: track / artist / time); horizontal seek `scrub`; transport row of hotspots
(prev / stop / **play** / next) plus a compact volume `scrub`; queue drawer =
`list{ collection="playlist" }` with active-row highlight. Min/close glyphs; body drag.

**B. Studio Deck** — frame archetype, **resizable** (anchor reflow), a rounded-rect floating app
surface (alpha corners). Title bar (track/artist/time); **visualizer drawn as `value_fill` bars
reading `viz_0..N`** — *not* a live `view{}` cutout (the board's `view{ id="visualizer" }` label was
aspirational; the live-host-view seam is not built, so we render real reactive bars); a volume
`scrub` styled as a slider near the deck controls, with the "knobs" rendered as value-reactive
decoration (the vocab has no rotary-drag control); transport row → `toggle_play`; playlist list
(and a small "Library: N tracks" affordance for flavor). Anchors: controls pinned, list stretches.

**C. Cassette** — gadget archetype, authored **alpha** so the cassette silhouette floats as a
shaped window. Cassette body (gradient), two reels via **`sweep` (conic) gradient** + hub circles,
tape label (track_title / artist), cassette window, four keys (prev / **play** / stop / next),
and a `position` scrub styled as tape counter/progress. Body drag; min/close.

## Honest limits (matched or noted, never faked)

- **No per-node rotation** — reels are static conic gradients; they won't spin. Optionally a subtle
  `playing`-driven pulse if it reads well, otherwise left static. Documented, not faked.
- **No soft shadow/blur primitive** — depth is approximated with layered fills + gradients.
- **macOS borderless-window shadow** behavior (whether the OS draws a shape-following shadow around
  the transparent window) to be verified during manual QA; not a blocker.
- **Rotary knobs** are decoration; volume is a real linear `scrub`. Stated plainly.

## Testing

- **Host unit tests** (`music_player_host.rs`): `get("artist")` returns current track's artist;
  `get("volume")` round-trips; `set_volume` clamps out-of-range input to `0..1`; `set_volume`
  reaches the backend (assert via `MockAudioState`).
- **Skin build tests** (`tests/skins_build.rs`): extend so `faceplate`, `studio`, `cassette` each
  load + build against the full demo vocab registry (parity with existing skin coverage).
- **Showcase swap test** (new or in `tests/host_switch.rs`): drive `Command::Swap` across
  faceplate → studio → cassette and assert the host's `position`/`current_index`/`volume` are
  unchanged by the swap (the core thesis, asserted).
- **Manual verification** (via `/run`): launch `-- showcase`, start playback, set a distinctive
  volume, seek partway, Tab through all three skins, and confirm playback/position/volume/selection
  persist and each window renders shaped + draggable. Capture a screenshot per skin.

## Documentation

- Add a concise "Showcase: one host, three skins" section to the demo README with the launch
  command and the Tab/Esc keys (per the per-phase README-currency rule).
- **Out of scope** (possible follow-up): regenerating the HTML concept board to match the shipped
  skins.

## Out of scope

- Live `view{}` host-view cutout for the visualizer (unbuilt seam; bars instead).
- True rotary drag controls; reel spin animation.
- Native Swift/IOSurface embedding path (this is the winit desktop demo).
- Regenerating the HTML board.

# Spec 3 — Headspace as a Real Music Player (design)

**Date:** 2026-06-24
**Status:** Approved, ready for implementation planning
**Builds on:** Spec 2 — Interactive-app foundation (PR #19, `main` at 9ed6942) — reuses the `list{}` primitive, `Host::rows`, and the row-click dispatch seam.
**Roadmap:** Third of three sequenced demo specs (see `demo-apps-roadmap` memory). Does NOT depend on Spec 1 (frame skins); Headspace is a **gadget** skin.

## Goal

Turn the existing Headspace gadget skin (`skins/reference`, 342×394) from a **mock** faceplate — whose `position` just fake-ticks at 10%/sec under `DemoHost` — into a **functioning music player**: real audio playback, transport controls (play/pause/stop/next/prev), click-to-seek, auto-advance, and a clickable **playlist** built on Spec 2's `list{}`.

Three things this introduces that the codebase does not have yet:

1. **Audio playback** — a third-party decode+output dependency (`rodio`) wrapped behind a mockable `AudioBackend` trait. rodio owns its own audio thread, so there is no hand-rolled playback thread.
2. **A `scrub{}` primitive + `Scene::hit_scrub` seam** — a click-to-seek progress bar, mirroring Spec 2's `list{}` + `Scene::hit_row` (a hittable region that reports the click's 0..1 fraction, dispatched as a host action).
3. **Gadget-path list support** — Spec 2's `list{}` only expands inside `Engine::layout()` (the frame-skin path). To host a playlist in a gadget skin, the demo's gadget render+pointer path is routed through `layout()` too (byte-identical for existing list-free gadget skins).

### Out of scope (YAGNI guardrails)

- No volume control, no shuffle, no repeat modes.
- No drag-to-scrub (`PointerEvent` is Press-only) — click-to-seek only.
- No playlist scrolling (clamped to the visible region, exactly like Spec 2's lists).
- No streaming / network / library database — the playlist is a fixed set of bundled clips.
- No waveform/visualizer rendering.

## Architecture decisions

| Decision | Choice | Why |
|---|---|---|
| Audio library | **rodio**, behind an `AudioBackend` trait | rodio owns the output thread (a `Sink`), decodes mp3/flac/wav/ogg via its symphonia feature, and exposes play/pause/stop + `try_seek` + `get_pos`. Far less code than `cpal`+`symphonia`; no hand-rolled decode loop or playback thread. The trait keeps it mockable for tests. |
| Track source | **Bundled CC0 clips** | 2–3 short public-domain clips ship in the skin asset dir, so the demo always plays on any machine and is deterministic. Tests use a mock track source, so CI never touches audio files or hardware. |
| Seek | **Click-to-seek scrubber** | A new generic `scrub{}` primitive + `Scene::hit_scrub`, mirroring the `hit_row` precedent. The most authentic music-player UX; the engine stays neutral (no new input primitive — reuses scene hit-testing + the host-action queue). |
| Host topology | **One engine, one `MusicPlayerHost`** | The faceplate and the playlist share player state (current track, position). A nested sub-engine would split state across two hosts. One engine hit-tests faceplate, playlist rows, and scrubber against one scene. |
| Host scope | **`MusicPlayerHost` replaces `DemoHost` for all media skins** | It is a superset of `DemoHost`'s interface (same `playing`/`position`/`track_title` keys, `toggle_play`/`stop` actions), so classic/minimal/transport become real players for free; only Headspace adds the playlist + scrubber primitives. `DemoHost` is retired. |

Rejected alternatives: `cpal`+`symphonia` (more code, hand-rolled threading); a nested playlist sub-engine (splits player state across two hosts); discrete seek buttons / no-seek (drops the authentic UX the scrubber gives). See the roadmap memory for fuller rationale.

## Component design

### 1. `AudioBackend` trait (mockable)

New module `crates/carapace-demo/src/audio.rs`:

```rust
#[derive(Debug)]
pub enum AudioError { Open(String), Decode(String), Unsupported }

/// One audio output sink. The real impl wraps rodio; tests use an in-memory fake.
pub trait AudioBackend {
    /// Load `path` and begin playing it (replacing any current track).
    fn play(&mut self, path: &Path) -> Result<(), AudioError>;
    fn set_paused(&mut self, paused: bool);
    fn stop(&mut self);
    /// Seek to `fraction` (0..1) of the current track.
    fn seek(&mut self, fraction: f32);
    fn position(&self) -> Duration;
    fn duration(&self) -> Option<Duration>;
    /// The current source has played to its end.
    fn is_finished(&self) -> bool;
}
```

- **`RodioBackend`** (real): holds the `OutputStream` (kept alive) + a `Sink`, and the current track's total `Duration` (from the decoded source). `play` decodes the file and appends to a fresh sink; `position` reads `Sink::get_pos()`; `seek` calls `Sink::try_seek`; `is_finished` is `Sink::empty()` (or pos ≥ duration). Kept deliberately thin — its correctness is validated by the live demo, not CI (build agents have no audio device), exactly as Spec 2 treats `StdFs`.
- **`MockAudio`** (test): records the last `play`/`seek`/pause/stop calls, holds a simulated `position` the test can advance, and a `finished` flag the test can set — so `MusicPlayerHost` logic is fully testable with no hardware or files.

### 2. `MusicPlayerHost`

New module `crates/carapace-demo/src/music_player_host.rs`:

```rust
pub struct Track { pub title: String, pub path: PathBuf }

pub struct MusicPlayerHost {
    backend: Box<dyn AudioBackend>,
    playlist: Vec<Track>,
    current: usize,
    playing: bool,
    window: WindowOutbox,   // reuse the existing host->window op channel
}
```

`impl Host`:
- `get`:
  - `"playing"` → `Bool(self.playing)`
  - `"position"` → `Scalar(pos_secs / dur_secs)` clamped 0..1 (0 when no duration)
  - `"track_title"` → `Str(current track title)`
  - `"time"` → `Str("M:SS / M:SS")` (elapsed / total)
- `rows("playlist")` → one `Row` per track: `title` cell, `duration` cell (`"M:SS"`), and a `now` cell (`"▶"` for `current`, else empty) so the skin can mark the playing row.
- `actions`: `toggle_play`, `stop`, `next`, `prev`, `seek`, `play_index`, plus the existing window ops (`begin_drag`, `minimize`, `close`).
- `invoke`:
  - `toggle_play` → flip `playing`. From a cold start (no track loaded yet, e.g. position 0 and the sink empty) the first `toggle_play` loads+plays `current`; otherwise it just `backend.set_paused(!playing)`.
  - `stop` → `backend.stop()`, `playing = false`.
  - `next`/`prev` → move `current` (clamp at ends; `next` past the end stops), then load+play.
  - `play_index(i)` → `current = i`, load+play (the playlist `on_select`).
  - `seek(frac)` → `backend.seek(frac)`.
- `tick(dt)`: position is read from the backend (never faked). If `playing && backend.is_finished()` → auto-advance to `next` (or stop at the end of the playlist). No thread signalling needed — rodio's sink owns the thread; the host polls.

A small helper `load_current()` calls `backend.play(&playlist[current].path)` and handles the `AudioError` by logging + stopping (read-only/graceful, never panics).

### 3. The `scrub{}` primitive + `Node::Scrub` + `Scene::hit_scrub`

A new engine primitive parallel to `value_fill`, but hittable.

```rust
// scene.rs
Node::Scrub {
    region: ImageDest,
    value_key: String,     // host state read for the fill proportion (e.g. "position")
    direction: FillDir,    // reuse the existing FillDir (Right for a horizontal bar)
    color: Color,
    on_seek: String,       // host action fired with the click fraction
}
```

- **Parsing** (`vocab.rs`, `ScrubPrim` registered in `base()`): `scrub{ x, y, w, h, value, on_seek, color, direction? }`.
- **Render** (`render.rs`): identical fill logic to `Node::ValueFill` — fill a proportion of `region` from `host.get(value_key)`. (Factor the value→proportion fill into a shared helper if it reduces duplication; otherwise mirror the existing arm.)
- **Hit** (`scene.rs`): `Scene::hit_scrub(&self, p: Pt) -> Option<(String, f32)>` — topmost `Node::Scrub` containing `p`, returns `(on_seek, ((p.x − region.x)/region.w).clamp(0,1))`.
- **Dispatch** (`engine.rs`): `handle_pointer_resolved` tries `hit()` → `hit_row()` → `hit_scrub()`, the last enqueuing `Command::HostAction { action: on_seek, args: [Value::Num(fraction)] }`. Same queue/allowlist path as list rows; no new engine input concept.
- **Layout** (`layout.rs`): `Node::Scrub { region, .. }` resolves under anchors exactly like `ValueFill`/`View` (region rect in `node_bbox`/`transform_node`). Needed so it survives the `layout()` pass the gadget path now uses.

### 4. Gadget-path generalization

In the demo (`main.rs`), the gadget (non-resizable) branches change so lists and scrubbers work:

- **Render:** the gadget branch passes `engine.layout(canvas_w, canvas_h)` instead of `engine.scene()`. With default (top-left) anchors and `logical == design`, `resolve_scene` is a verbatim identity for every existing node kind, and `expand_lists` is a no-op when there are no lists — so existing gadget skins render **byte-identically** (guarded by `gadget_path_still_uniform_scales`). The renderer still scales `physical/canvas`.
- **Pointer:** the gadget click branch calls `engine.handle_pointer_resolved(canvas_w, canvas_h, p, Press)` instead of `engine.handle_pointer(p, Press)`. For a list/scrub-free skin this hit-tests an identical scene, so behavior is preserved; for Headspace it now reaches `hit_row`/`hit_scrub`.

`canvas_w/h` come from `engine.scene().canvas`. The pointer `p` is mapped physical→canvas exactly as the gadget path does today.

### 5. Headspace skin changes (`skins/reference/skin.lua`)

Keep the artwork, the play/pause and stop hotspots, the track-title text. Changes:
- Replace the `value_fill{ value="position" }` progress bar with `scrub{ value="position", on_seek="seek", … }` at the same rect.
- Add next/prev hotspots (small regions) bound to `host.next()` / `host.prev()`.
- Add `list{ collection="playlist", on_select="play_index", template={ {bind="now", …}, {bind="title", …}, {bind="duration", right=…, halign="right", …} } }` in a free area of the faceplate (the design has room below the progress bar; exact rect chosen during implementation to fit the art).
- Add a `text{ value="time" }` for the elapsed/total readout.

The skin's canvas (342×394) is unchanged. If the art doesn't leave room for a readable playlist, the implementation may extend the canvas height — a skin-asset decision deferred to implementation, noted but not blocking.

### 6. Bundled audio + demo wiring

- 2–3 short **CC0 / public-domain** clips ship under `crates/carapace-demo/skins/reference/assets/` (or a sibling `audio/` dir) with a `LICENSE`/attribution file naming each clip's source. *Sourcing genuinely license-clean clips is an implementation responsibility; if sourcing is blocked, the documented fallback is a generated tone source — but bundled clips are the intent.*
- `MusicPlayerHost::new` builds the playlist from those bundled paths (resolved relative to the skin/asset dir) with hand-authored titles, and constructs a `RodioBackend`.
- The demo constructs `MusicPlayerHost` as the media host in place of `DemoHost` (the single media host shared across all `MEDIA_SKINS`). `DemoHost` is removed once unused; if anything still references it, that reference is migrated.

## Dependencies

- Add `rodio` to `crates/carapace-demo/Cargo.toml` with the symphonia-backed format features needed for the bundled clip formats (exact version/features verified against current rodio docs via context7 at planning time). **The first fetch MUST run as `sfw cargo add rodio …`** per the `run-dep-fetches-through-sfw` memory.
- No new dependency in the core `carapace` engine crate — the `scrub{}` primitive is pure engine code; audio lives entirely in `carapace-demo`.

## Testing strategy

- **Unit — `MusicPlayerHost` over `MockAudio`:** play/pause toggles `playing` + `set_paused`; `stop` resets; `play_index(i)` loads track `i`; `next`/`prev` clamp at ends and load the new track; `seek(frac)` forwards the fraction to the backend; `tick` auto-advances when `MockAudio` reports finished; `rows("playlist")` returns the right titles/durations and the `now` marker on `current`; `get("position")` reflects the backend's position/duration.
- **Unit — `Scene::hit_scrub`:** fraction math (left edge → 0, right edge → 1, middle → 0.5), x/y bounds rejection, clamp, and `None` when no `Node::Scrub` under the point (mirrors the `hit_row` tests).
- **Integration (engine):** a skin with a `scrub{}` → `handle_pointer_resolved` at a known x enqueues `seek` with the expected fraction; `update` invokes it (allowlist includes `seek`).
- **Parsing:** `scrub{}` builds the expected `Node::Scrub`; registered in base vocab (count assertion bumped).
- **Regression:** gadget golden snapshots stay **byte-identical** under the gadget-path generalization — run `render_offscreen` (gpu-tests), especially `gadget_path_still_uniform_scales`.
- **Not in CI:** `RodioBackend` (needs an audio device) — kept thin and validated via the live demo + runtime smoke (`cargo run`), as Spec 2 treats `StdFs`.
- **CI gate:** `cargo test --workspace` + `cargo fmt --check` + `cargo clippy -D warnings` (+ the gpu-tests variant), per the `run-clippy-before-push` memory.

## Files touched (anticipated)

**Engine (`crates/carapace/`):**
- `src/scene.rs` — `Node::Scrub`, `Scene::hit_scrub`, `summary()` arm.
- `src/vocab.rs` — `ScrubPrim` parse + registration.
- `src/layout.rs` — `Node::Scrub` `node_bbox`/`transform_node` arms.
- `src/render.rs` — `Node::Scrub` draw arm (proportional fill, shared with/mirroring `ValueFill`).
- `src/engine.rs` — `handle_pointer_resolved` tries `hit_scrub`.

**Demo (`crates/carapace-demo/`):**
- `src/audio.rs` (new) — `AudioBackend`, `AudioError`, `RodioBackend`, `MockAudio`.
- `src/music_player_host.rs` (new) — `Track`, `MusicPlayerHost`.
- `src/main.rs` — gadget render/pointer routed through `layout()`; media host = `MusicPlayerHost`; register `ScrubPrim` (engine base) — `scrub{}` is a base primitive, so it needs no demo registration.
- `skins/reference/skin.lua` — `scrub{}`, `list{}` playlist, next/prev, `time` text.
- `skins/reference/assets/` (+ audio) — bundled CC0 clips + `LICENSE`.
- `Cargo.toml` — `rodio` dependency.
- `src/demo_host.rs` — removed (retired) once unused.

**Docs:**
- `README.md` — document `scrub{}`, the music player, and the gadget-path list support, in the same PR.

## Open follow-ups (deferred, non-blocking)

Drag-to-seek, volume/shuffle/repeat, playlist scrolling, and a real music-library scan (vs bundled clips) are explicit non-goals for this spec and natural extensions afterward. Inherited engine follow-ups from earlier specs (per-frame decoded-image Blob caching; double `engine.layout`/frame on the resizable path) remain filed and untouched.

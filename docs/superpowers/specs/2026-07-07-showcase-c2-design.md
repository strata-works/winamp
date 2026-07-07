# CarapaceShowcase C2 — Design

**Date:** 2026-07-07
**Branch:** `showcase-c2`
**Depends on:** C1 (PR #37, merged) — borderless SwiftUI + IOSurface showcase, `MusicHost`
as the Swift-owned source of truth, three baked-chrome skins (Faceplate / Studio / Cassette).

## Goal

Three independent, **showcase-only** polish changes. **Zero engine-crate diff** — the engine
already supports everything needed (`text{ font=... }`, pull-model collections). Work is confined
to `showcase/` (Swift app + skin Lua/assets).

1. NSOpenPanel music picker — load real audio from disk.
2. Grey out the zoom (green) traffic-light button — a fixed borderless skin can't zoom.
3. Monospace DSEG7 seven-segment clock on the skins that show a time counter.

## 1. NSOpenPanel music picker

**UX:** `⌘O` opens an app-modal `NSOpenPanel`; picked items are **appended** to the current
playlist (bundled demo tracks and current playback are preserved). Panel accepts **multiple files
and/or folders**; folders are expanded recursively to their contained audio files.

### Components

- **`TrackImporter`** (new, `Sources/Showcase/TrackImporter.swift`) — pure, testable:
  - Input: `[URL]` (files and/or directories) from the panel.
  - Recursively enumerates directories, keeping only audio files (UTType conformance to `.audio`).
  - Builds `[Track]`: title/artist from `AVURLAsset` common metadata, with fallbacks
    **title → file basename**, **artist → "Unknown Artist"**; duration from `asset.load(.duration)`.
  - Skips URLs that are neither audio files nor directories.
- **`MusicHost`** — `playlist` changes from `let` to `private(set) var`; add `func addTracks(_ tracks: [Track])`
  that appends. `current`, `playing`, `volume`, and the loaded player are untouched, so playback
  continues uninterrupted and the new rows simply extend the list. The engine pulls `rowCount()` /
  `rowString()` every frame, so the `list{}` grows on the next frame with no explicit signalling.
- **Menu + shortcut** (`App.swift`) — install a minimal main menu (App menu + File menu). **File →
  "Open Music…"** bound to `⌘O`. The handler presents the `NSOpenPanel`
  (`canChooseFiles = true`, `canChooseDirectories = true`, `allowsMultipleSelection = true`,
  `allowedContentTypes = [.audio]`), routes the result through `TrackImporter`, and calls
  `host.addTracks`. The borderless window is unaffected; the menu only enables/advertises the shortcut.

### Edge cases

- Empty / cancelled panel → no-op.
- Metadata load failure for a file → still imported with basename title + "Unknown Artist" + duration 0.
- Non-audio files inside a chosen folder → skipped.

## 2. Grey-out zoom traffic light

In `AppDelegate.installTrafficLights()`, the zoom (`.zoomButton`) is created but set
`isEnabled = false` with no action assigned. macOS renders a disabled traffic-light button greyed,
correctly signalling "unavailable". Close and minimize remain fully live. Rationale: the skins are
fixed-canvas borderless windows, so zoom/maximize has no meaningful behavior.

## 3. Monospace DSEG7 clock (Faceplate + Studio)

Only **Faceplate** and **Studio** render a time counter (`text{ value="time" }`); **Cassette** has
no clock, so it is untouched.

### Font asset

- Ship **DSEG7 Classic Regular** (keshikan DSEG family, SIL Open Font License 1.1), fetched from the
  official GitHub release.
- Sandbox note: the engine's asset walker **skips symlinks** (sandbox integrity, `asset.rs`), so the
  font cannot be symlink-shared. Copy `DSEG7Classic-Regular.ttf` into **both**
  `skins/faceplate/assets/` and `skins/studio/assets/`.
- Commit the OFL license text once under `showcase/skins/` (e.g. `showcase/skins/DSEG-OFL.txt`) and
  reference it in the README.

### Format decision — elapsed-only `M:SS`

DSEG7 is seven-segment: reliable coverage for digits and `:`, but `/` and space are not guaranteed.
The clock therefore shows **elapsed time only** (`M:SS`) — which is also the authentic Winamp look
(its main counter shows a single time, not "elapsed / total").

- Add a host key **`clock`** returning `fmtMMSS(currentTime)` (elapsed only). Implemented as a new
  case in `MusicHost.str(_:)`. The existing `time` key ("M:SS / M:SS") is left intact (harmless).
- Re-point the Faceplate and Studio clock elements:
  `text{ value="clock", font="DSEG7Classic-Regular.ttf", ... }`.
- Per-track durations remain visible in the playlist rows, so total-time info is not lost from the UI.

_(Rejected alternative: DSEG14 fourteen-segment keeps the full "elapsed / total" string but reads
less like a classic clock. Not chosen.)_

## Testing

- **`swift test`:**
  - `TrackImporterTests` — folder recursion finds nested audio; metadata vs. filename/artist
    fallbacks; non-audio files skipped; cancelled/empty input yields `[]`. Fixtures: the two bundled
    WAVs (+ a nested temp dir).
  - `MusicHostTests` extension — `addTracks` appends and preserves `current` / `playing` / `volume`;
    `str("clock")` returns elapsed-only `M:SS`.
- **Manual (README checklist):** add steps for `⌘O` import (files + folder, append, playback
  continues), greyed-out zoom button, and DSEG7 elapsed clock on Faceplate + Studio.

## Out of scope

- Drag-and-drop import (⌘O only for now).
- Click-to-toggle remaining/elapsed on the clock.
- Persisting the imported playlist across launches.
- Any engine-crate change.

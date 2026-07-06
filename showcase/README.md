# CarapaceShowcase (macOS)

Native SwiftUI app embedding the carapace engine via `carapace-ffi` (ABI 3.0), rendering skins
zero-copy through an IOSurface pool into a borderless, draggable window. Swift owns the music host
(playlist + AVFoundation), so skin hot-swaps preserve playback, position, volume, and selection.

## Build & run

    cargo build -p carapace-ffi     # from repo root — produces target/debug/libcarapace_ffi.dylib
    cd showcase && swift run Showcase

Press **Tab** to hot-swap skins (starter ↔ alt). Drag the body to move the window;
the min/close glyphs and all transport/scrub/playlist controls are the skin's own.

## Tests

    cd showcase && swift test        # MusicHost + vtable-callback unit tests

## Manual verification

Not built in CI (no Swift toolchain there), and not drivable by `agent-device` — that CLI only
targets iOS/Android simulators, not native macOS windows. Automated coverage is `swift test`
(host logic + vtable callbacks) plus confirming the app launches and renders. Interactive
behavior is confirmed by a human running through this checklist:

1. `cargo build -p carapace-ffi && cd showcase && swift run Showcase`
2. Confirm the borderless starter skin window appears and the playlist is populated.
3. Click **play** — confirm audio starts and the transport/scrub position advances.
4. Click a playlist row — confirm the selection and now-playing track change.
5. Click a point along the volume scrub — confirm the level changes audibly (scrubs are click-to-set: the engine models pointer *press*, not drag).
6. Press **Tab** — confirm the window hot-swaps to the `alt` skin.
7. Confirm playback, position, volume, and playlist selection all persisted across the swap.
8. Press **Tab** again to swap back to `starter` and repeat step 7.

## Notes

- Sub-project B of the "one host, three skins" showcase. The three concept skins are Sub-project C.
- `viz_*` is a time-driven animation, not a real FFT.

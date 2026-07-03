# carapace-preview

> A live, interactive browser previewer for carapace skins. (Dev tool.)

Editing a skin used to mean a full rebuild — the native demo, or an iOS device
round-trip — just to see a change. `carapace-preview` closes that loop.

## Usage

```bash
cargo run -p carapace-preview -- path/to/skin-dir [--port <n>]
```

It serves `http://127.0.0.1:<port>` (a random free port unless `--port` is given)
and opens a browser tab. The **real carapace engine** renders the skin offscreen
(headless wgpu / Vello); the browser is a thin display + control surface.

## What you get

- **Live render** of the skin via the real engine.
- **Hot reload:** saving `skin.lua` / `skin.toml` / an asset re-renders within a
  moment. A skin that fails to load shows its **Lua error** as a banner — the
  server and last-good frame survive.
- **Click to interact:** clicking the preview forwards a pointer event, so
  `region{}` hotspots fire their actions (shown in the action log).
- **Host-data panel:** add/edit the host values the skin binds
  (`value_fill{ value="level" }`, `text{ value="track" }`, …); edits re-render live.
- **Animated skins play** — the engine ticks continuously with wall-clock `dt`.
- **Canvas size** input re-lays-out resizable (anchored / frame) skins.

## Not yet (planned — Plan B)

The property inspector and skin-parameters panel that **write edits back to
`skin.lua`** (source provenance via `full_moon` + mlua debug hooks) are a separate
follow-up. This tool is view + host-data-drive + hot-reload only.

## How it works

A single engine thread owns the `Engine` (which is `!Send`), a headless wgpu
device, an offscreen `Rgba8` target, and the render loop. It renders → reads back
RGBA → PNG-encodes → pushes over a WebSocket, and only re-sends when the frame
actually changed (hash compare), so a settled static skin streams nothing. A
`tiny_http` server serves the one-page viewer; a `tungstenite` WebSocket carries
frames down and pointer/value/canvas edits up. Nothing `!Send` crosses a thread.

See the design: `docs/superpowers/specs/2026-07-01-carapace-preview-design.md`.

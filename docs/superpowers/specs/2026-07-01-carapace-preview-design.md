# carapace-preview — a live, interactive skin previewer (design, 2026-07-01)

A developer tool for authoring carapace skins. Editing a skin today means a full rebuild (native
demo, or the iOS device round-trip) just to see a change — this session's medieval-door skin was
debugged by rendering throwaway PNGs. `carapace-preview` closes that loop: run
`carapace-preview <skin-dir>`, open a browser tab, and see the skin render **live** with hot-reload,
click-to-interact, and an editable panel for the host data the skin binds.

## Goal & success bar

`carapace-preview path/to/skin-dir` serves `http://localhost:PORT`. In the browser:

1. The skin **renders** (the real carapace engine — Vello — rendering offscreen; the browser only
   displays frames).
2. **Hot reload:** saving `skin.lua` / `skin.toml` / an asset re-renders within a moment. A skin
   that fails to load shows its **Lua error** in the page instead of crashing the server.
3. **Interactive:** clicking the preview forwards a pointer event to the engine, so `region{}`
   hotspots fire their actions (logged in the page).
4. **Data-bound:** a side panel edits the host values the skin binds (`value_fill{ value="level" }`,
   `text{ value="track" }`, …); edits re-render live.
5. **Animated skins play** (the engine ticks continuously).

Non-goals (v1): editing the skin *in* the browser (you use your own editor), multi-skin galleries,
a hosted/public deployment, WASM/in-browser rendering (that's the W2 alternative, deferred),
recording/export, and any change to the `carapace` engine crate.

## Architecture & data flow

```
skin dir  ──notify(watch)──►  carapace-preview server (native Rust)        Browser tab (thin viewer)
 skin.lua / skin.toml         ├─ Engine + PreviewHost (carapace)              ├─ <canvas>: draws frames
 assets/                      ├─ render loop: tick → layout → draw            │   click → {x,y}  ─┐
                              │    (offscreen wgpu) → readback RGBA           ├─ panel: host key→val │ up
                              ├─ reload on change | Lua error → page          │     rows  ───────────┤ WS
                              └─ WebSocket  ⇅  frames + log + errors down  ◄──┘   canvas size        │
                                                              input up  ────────────────────────────┘
```

Everything visual is the **real engine**: the same `carapace::render::{Renderer, RenderTarget}`
path `carapace-demo` uses, rendering into an **offscreen** wgpu texture (headless — no OS window)
and reading the frame back to RGBA. The browser is a display + control surface only.

## Components

### `crates/carapace-preview/` (new bin crate)

- **`render.rs` — offscreen render context.** Owns a headless wgpu device + queue, an offscreen
  `Rgba8` target sized to the skin canvas, and a `Renderer`. `render_frame(engine) -> Vec<u8>`
  runs `engine.update(dt)` → `engine.layout(cw, ch)` → `Renderer::draw` into the target → readback
  RGBA. (This is the `carapace-demo` / `embed-spike::oneshot` render path, reimplemented against the
  public `carapace` API — no dependency on the `embed-spike` spike crate.)
- **`preview_host.rs` — `PreviewHost: carapace::host::Host`.**
  - `get(key)` reads a runtime `HashMap<String, StateValue>` the browser panel drives.
  - `invoke(action, args)` appends to an **action log** drained to the browser.
  - `actions()` returns an allowlist. Skins call `host.<name>()`, and the loader rejects names not
    in the allowlist; since we don't know a skin's actions in advance and `ActionSpec.name` is
    `&'static str`, on load we **scan the skin source for `host.<ident>`**, dedupe, and `Box::leak`
    each name into a `&'static str` `ActionSpec`. Re-scanned on every reload. (A handful of leaked
    action-name strings in a dev tool is acceptable; the alternative — a permissive/preview host
    mode in `carapace` — is noted as a cleaner future option but avoided to keep the engine
    untouched.)
- **`skin_session.rs` — load + watch + reload.** Loads the skin dir into an `Engine` with the
  `PreviewHost`. A `notify` watcher on the dir triggers a reload: rebuild the `Engine`, preserving
  the current host-value map + canvas size. **A load failure is captured, not fatal** — the error
  string is streamed to the browser and the last good frame stays up.
- **`server.rs` — HTTP + WebSocket.** Serves the single static viewer page and one WebSocket:
  - **down (server→browser):** `frame` (PNG bytes), `actionlog` (invoked action), `error` (Lua load
    error / cleared), `meta` (skin name, canvas size).
  - **up (browser→server):** `pointer{x,y}` (canvas coords), `setValue{key, value}`,
    `setCanvas{w,h}`.
- **`main.rs`** — parse `<skin-dir>` + `--port`, wire the session + render loop + server, open the
  browser.

### Browser viewer — `assets/index.html` (+ inline JS/CSS, single file)

- A `<canvas>` that draws each incoming `frame` (decode PNG → `drawImage`). Click → map display px
  to canvas coords → send `pointer`.
- **Control panel:** rows to add/edit host `key → value` (number or string) → `setValue`; a
  canvas-size input → `setCanvas`; a scrolling **action log**; the skin name; and an **error banner**
  shown when a `error` message is live.

## Data flow details

- **Render loop & frame transport.** The loop targets ~30 fps but **hashes each rendered RGBA frame
  and only encodes+sends when it changed** — animated skins stream; a settled static skin sends
  nothing. Frames are **PNG-encoded** (`image` crate): these flat-color vector skins compress to a
  few KB, so localhost bandwidth and per-frame encode cost stay negligible, and the browser decodes
  with a plain `Image`. (Raw RGBA was rejected: ~1 MB/frame is wasteful even on localhost.)
- **Interaction.** A `pointer{x,y}` maps browser display coordinates → skin design-canvas
  coordinates (the display may be scaled), then feeds the engine's pointer/hit-test; a hit `region{}`
  enqueues its action, which `PreviewHost::invoke` logs and streams as `actionlog`. Actions don't
  mutate host values (the panel owns those) — the log proves the wiring; the user sets values to see
  effects.
- **Web stack.** Light and synchronous: `tiny_http` (serving) + `tungstenite` (WebSocket), avoiding
  the `axum`/`tokio` async tree. New third-party deps are fetched via `sfw` (Socket Firewall) on
  first add, per repo convention.

## Error handling

- **Skin load / Lua error:** caught at reload; streamed as `error` and shown as a banner; server and
  last-good frame survive.
- **Missing host key:** `get` returns `None` (the binding renders its default/empty) — not an error.
- **wgpu/adapter init failure:** fail fast at startup with a clear message (dev tool, native GPU).
- **WebSocket drop:** the browser auto-reconnects; the server tolerates zero clients (keeps
  rendering only when a client is attached, to stay idle otherwise).

## Testing

- `PreviewHost` source-scan: given skin source, the derived action allowlist contains exactly the
  `host.<name>` identifiers used (incl. inside `on_press` closures), deduped.
- Frame change-detection: identical scene → same hash → no resend; a host-value change → different
  hash → resend.
- Headless smoke test: load a tiny fixture skin, render one frame, assert non-empty RGBA of the
  expected dimensions (reuses the offscreen render path).
- The single-file viewer JS is thin and validated by eye against a running server.

## Deliverables

- `crates/carapace-preview/` (bin) + its single-file browser viewer.
- A short README: `carapace-preview <skin-dir>`, the panel, hot-reload, limitations.
- Wired into the workspace `Cargo.toml` members.

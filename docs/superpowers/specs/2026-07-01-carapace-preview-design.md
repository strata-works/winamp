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
6. **Edit → code:** a **property inspector** (click a node → edit its literal props) and a **skin
   parameters** panel (edit the skin's top-level `local` literals) **rewrite `skin.lua` on disk**;
   the watcher reloads and re-renders. Only literal-backed props / literal params are editable;
   loop-generated & computed nodes are read-only (with the reason), tunable via their parameters.

Non-goals (v1): a from-scratch visual authoring canvas (that's the **skin studio**, a separate
future project — see the end), *individual* editing of loop-generated nodes (no code to map to
without unrolling — you tune them via parameters), multi-skin galleries, a hosted/public deployment,
WASM/in-browser rendering (the W2 alternative, deferred), recording/export, and any change to the
`carapace` engine crate.

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
- **`provenance.rs` — source ↔ scene mapping + write-back.** Captures node→call-span at load
  (mlua debug), parses `skin.lua` (`full_moon`) into literal-field and top-level-`local` indices,
  answers node-pick (topmost node at a point) + editability, and applies a `setProp`/`setParam` by
  rewriting the exact literal span in the source file. (See "Editing" below.)
- **`server.rs` — HTTP + WebSocket.** Serves the single static viewer page and one WebSocket:
  - **down (server→browser):** `frame` (PNG bytes), `actionlog` (invoked action), `error` (Lua load
    error / cleared), `meta` (skin name, canvas size), `nodeInfo` (picked node's editable/read-only
    props + reasons), `params` (the skin's editable parameter list).
  - **up (browser→server):** `pointer{x,y}`, `pick{x,y}` (select node for the inspector),
    `setValue{key, value}`, `setCanvas{w,h}`, `setProp{nodeId, field, value}`, `setParam{name, value}`.
- **`main.rs`** — parse `<skin-dir>` + `--port`, wire the session + render loop + server, open the
  browser.

### Browser viewer — `assets/index.html` (+ inline JS/CSS, single file)

- A `<canvas>` that draws each incoming `frame` (decode PNG → `drawImage`). Click → map display px
  to canvas coords → send `pointer`.
- **Control panel:** rows to add/edit host `key → value` (number or string) → `setValue`; a
  canvas-size input → `setCanvas`; a scrolling **action log**; the skin name; and an **error banner**
  shown when a `error` message is live.
- **Inspector + parameters** (see next section): a node inspector (populated on node-pick) and a
  skin-parameters list; edits emit `setProp` / `setParam` and the file is rewritten on disk.

## Editing: inspector + parameters → write-back to `skin.lua`

The tool doesn't just preview — literal-backed edits rewrite the skin source, so the file stays the
single source of truth. Two editing surfaces, both built on **source provenance**:

- **Provenance capture (at load).** Two correlated sources:
  - *Runtime:* an mlua debug hook / `inspect_stack` reads the **caller source line** each time a
    primitive ctor closure runs, tagging every emitted scene `Node` with the **source span of the
    `fill{}`/`text{}`/… call** that produced it.
  - *Static:* parse `skin.lua` to a Lua AST (`full_moon`) and index, per primitive call, its
    **literal fields** — `color` rgba, `x/y/w/h`, `radius`, `size`, text — each `(value, exact
    span)`; variable/expression fields (`color = STONE_M`, `x = 10 + …`) are flagged non-literal.
    Also index the skin's **top-level `local <NAME> = <literal>`** definitions (the parameters).
- **Node inspector.** Click the preview → server returns the topmost node whose bounds contain the
  point (a scene-level pick, broader than the region-only hit-test) → the browser shows its editable
  props. A prop is **editable iff** its field is a literal at the call site **and** the call emitted
  a **single node** (not loop-multiplied). Non-editable props show read-only **with the reason**
  ("bound to `level`", "from a loop", "computed"). Edit → `setProp{nodeId, field, value}` → server
  rewrites the field's literal span in `skin.lua`.
- **Parameters panel.** Lists the top-level `local` literals (`RI=90`, `RO=150`, `STONE_L={…}`, loop
  bounds like `for k = 1, 9` → a count, …). Editing one → `setParam{name, value}` → rewrites that
  `local`'s literal span → the whole skin responds (this is how procedural art like the voussoir
  arch is edited: reshape via `RI`/`RO`, recolor via `STONE_*`, change the voussoir count). Clicking
  a computed/loop node may **highlight the params it depends on** (best-effort).
- **Write-back.** Every edit is a text rewrite of the exact literal span on disk (never a
  regeneration of the file), then the watcher reloads. Formatting/comments elsewhere are untouched.

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
  the `axum`/`tokio` async tree. Plus `full_moon` (Lua AST for provenance) and `notify` (file watch).
  New third-party deps are fetched via `sfw` (Socket Firewall) on first add, per repo convention.

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
- **Provenance + write-back:** for a fixture skin, a literal `color`/`x` field maps to the right
  source span; `setProp` rewrites exactly that span (surrounding text/comments untouched) and the
  re-parse reflects it; a loop-generated node reports read-only with a reason; `setParam` rewrites a
  top-level `local` literal.
- The single-file viewer JS is thin and validated by eye against a running server.

## Deliverables

- `crates/carapace-preview/` (bin) + its single-file browser viewer.
- A short README: `carapace-preview <skin-dir>`, the panels (host data, inspector, parameters),
  hot-reload, write-back, and the literal-only editing limitation.
- Wired into the workspace `Cargo.toml` members.

## Future direction: skin studio (out of scope here, noted)

A from-scratch **visual authoring** tool — drag primitives onto a canvas, style them, wire host
bindings/actions — that **generates** the Lua carapace needs. It runs **one-way** (studio document →
Lua), so it never has to reverse-engineer arbitrary code — the wall this previewer hits on loop /
computed nodes. It is the previewer **plus** a visual editing canvas + a Lua code-generator on the
**same** live-render + host-data + write-to-file foundation built here, which is why `carapace-preview`
is the sensible first step. Trade-off to carry forward: studio output is **declarative** Lua (explicit
primitives, no loops), so procedural skins stay hand-authored; and the studio authors its own
documents rather than importing arbitrary hand-written Lua. Its own brainstorm → spec → plan when we
get there.

# Preview Tool

`carapace-preview` is the live dev previewer: it renders a skin with the **real engine** (headless wgpu/Vello) and streams frames to a browser, hot-reloading on every save, with panels to inject host data and edit the skin in place.

Crate: `crates/carapace-preview/`. Reflects source as of 2026-07-04.

## Running it

```bash
cargo run -p carapace-preview -- path/to/skin-dir [--port <n>]
```

- The first positional argument is the skin directory (must contain `skin.toml`).
- `--port <n>` is optional; default `0` picks a free ephemeral port.

It prints `carapace-preview serving http://127.0.0.1:<port>  (skin: <dir>)` and best-effort opens your browser (`main.rs:52-59`). The engine renders offscreen on a single dedicated thread (because `Engine` is `!Send`); the browser is purely a display + control surface.

## Architecture

- **HTTP** serves one static viewer page (`assets/index.html`) with the WebSocket port templated in (`server.rs:124-126`).
- **WebSocket** is a full-duplex per-connection channel (`server.rs:69-122`): the server pushes `OutMsg` (binary PNG frames + `meta`/`error`/`params`/`nodeInfo`/`actionLog` JSON) down and parses `ClientMsg` (`pointer`, `pick`, `setValue`/`removeValue`, `setCanvas`, `setProp`, `setParam`) up (`protocol.rs`).
- **File watcher** (`notify`) recursively watches the skin dir and coalesces bursts into a single reload per save (`main.rs:66-94`).
- Frames render only when a client is watching, at up to ~60fps, and are re-sent only when their hash changes — a settled static skin streams nothing (`main.rs:257-275`).

## Browser panels

Each panel header carries an ⓘ tooltip describing it.

- **Canvas size** — width/height inputs + "Set" re-render the skin at a different pixel size (resizable skins reflow; fixed skins scale). Sizes are clamped to the GPU's max texture dimension. (`index.html`, engine side `main.rs:162-178`.)
- **Host data** — live key/value rows for values the skin reads via `host.<name>` (e.g. `track_title`, `position`, `viz_0`). "Add" creates/overrides a key; ✕ removes it so the binding falls back to its default. This is how you feed `value = "..."` bindings while previewing. (`addValueRow`, `main.rs:151-161`.)
- **Parameters** — top-level `local NAME = <literal>` scalars and inline color tables parsed from `skin.lua`; edit to write back (see below).
- **Inspector** — an "Inspect" toggle switches canvas clicks from pointer events to node-picking; click a rendered node to edit its literal props.
- **Action log** — appends a line for every `host.<action>()` the skin fires as you interact.

## Editing & write-back

Two panels edit `skin.lua` **in place**, on top of the render/host-data/hot-reload loop. Both work the same way: re-parse `skin.lua` with `full_moon`, splice the exact byte span of the target literal, and write the file back — **never regenerating or reformatting** the source. The file watcher then reloads and re-renders.

### Parameters panel

Lists top-level `local NAME = <literal>` scalars and numeric tables (colors). Editing a scalar sends `setParam { field: null }`; editing a color subfield sends `setParam { field: <subfield> }` → `SkinSession::apply_param` splices that literal (`skin_session.rs:178-198`).

### Inspector

Toggle **Inspect**, click a node → a `pick` with canvas coords → `SkinSession::pick` hit-tests the resolved scene and correlates the picked node's source `Origin` to the parsed model (`inspector.rs`). Then:

- **Editable literal** props show an input (scalars as one field; an inline `color = {r,g,b,a}` shows one input per numeric subfield).
- **Non-editable** props show a **reason** instead — e.g. `"from a loop"`, bound to host data, or a computed expression.
- Committing sends `setProp { line, field, sub, value }` → `SkinSession::apply_prop` locates the call at that source line, finds the field's (or subfield's) literal span, splices, and writes (`skin_session.rs:131-176`).

### Limitations (by design)

- **Literal-only editing** — bound values, computed expressions, and other non-literals are read-only with a reason shown.
- **One primitive per source line** — node↔source correlation is line-based, so put one primitive per line to keep each individually pickable/editable.
- **Text nodes aren't pickable** — `text{}` nodes have no measured geometry in the scene graph.
- **Loop-generated nodes are read-only** — nodes created inside a Lua loop report `"from a loop"`.

## Hot-reload & error recovery

On any save, `SkinSession::reload` rebuilds the engine from `skin.toml` + entry Lua. On success it swaps in the new engine and clears the error banner. On **failure** it keeps the last-good engine running and shows the Lua error as a red banner in the browser — the server and last frame survive (`skin_session.rs:59-73`, `main.rs:215-245`).

Host data is served by `PreviewHost` (a real `carapace::host::Host` impl) backed by a shared map the Host-data panel edits live; the action allowlist is scanned from the Lua source, and invoking an action just logs it to the Action log (`preview_host.rs`).

## See also

- [Getting Started](./getting-started.md) — author and preview a first skin.
- [Skin Authoring Reference](./skin-authoring.md) — the vocabulary you're editing.
- Crate README: `crates/carapace-preview/README.md`.

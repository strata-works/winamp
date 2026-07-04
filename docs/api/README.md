# Carapace API Documentation

The complete reference for **Carapace** — a skin engine that renders declarative Lua skins on wgpu/vello and embeds into host apps (native macOS, iOS WidgetKit, Flutter) through a safe C ABI.

This is the canonical, version-controlled documentation. Per-crate READMEs stay concise; the full guides and reference live here.

## Contents

| Page | For | What it covers |
|------|-----|----------------|
| [Getting Started](./getting-started.md) | Everyone | Install, run the previewer, author a first skin |
| [Skin Authoring Reference](./skin-authoring.md) | Skin authors | `skin.toml` manifest + the Lua skin vocabulary (primitives, fields, host data & actions) |
| [Engine API (Rust)](./engine-api.md) | Host integrators (Rust) | The `carapace` crate: `Engine`, `Scene`, `Host`, `Vocab`, skin loading, state, layout, picking, rendering |
| [FFI / C ABI](./ffi-c-abi.md) | Host integrators (native/C) | `carapace-ffi` ABI 2.0: exports, handle lifecycle, render thread, IOSurface, hit-testing |
| [Preview Tool](./preview-tool.md) | Skin authors & contributors | `carapace-preview`: live previewer, property inspector, parameters panel, write-back |

## Where to start

- **Writing skins** → [Getting Started](./getting-started.md) then [Skin Authoring Reference](./skin-authoring.md)
- **Embedding the engine in a Rust host** → [Engine API (Rust)](./engine-api.md)
- **Embedding in a native/C host (Swift, Flutter, …)** → [FFI / C ABI](./ffi-c-abi.md)

## Architecture at a glance

- **Engine** (`carapace`) — loads a skin (`skin.toml` + a Lua entry script), runs the script against a registered **vocabulary** of primitives to produce a **scene**, lays the scene out for a target size, and renders it on wgpu/vello. Single-threaded by design (`Engine` is `!Send`).
- **Host** — an integrator-supplied object (the `Host` trait, or the C vtable) that provides live data (`host.<name>`) and performs actions (`host.<action>()`) the skin invokes.
- **FFI** (`carapace-ffi`) — a safe C ABI wrapping the engine on a carapace-owned render thread with a command queue and an IOSurface frame pool (Apple-only today).
- **Preview** (`carapace-preview`) — a live dev previewer with a property inspector + parameters panel that write edits back into `skin.lua`.

> Each page notes the crate/module and (where useful) `file:line` it reflects, so it can be re-verified against the source. Generated from the code as of 2026-07-04.

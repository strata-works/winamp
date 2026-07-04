# Getting Started

This walks you from nothing to a live, edited skin. If you're integrating the engine into a host app instead of authoring a skin, jump to [Engine API (Rust)](./engine-api.md) or [FFI / C ABI](./ffi-c-abi.md).

## Installation

Carapace is not yet published to crates.io — build it from source.

**Prerequisites:**

- **Rust toolchain** — a recent stable Rust (the workspace is edition 2024, built against Rust 1.96). Install via [rustup](https://rustup.rs): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
- **A GPU** — rendering uses wgpu/vello: **macOS** via Metal (nothing extra to install), **Linux** via Vulkan.
- **Linux only** — the text layer links system `fontconfig`, so install the dev package first: `sudo apt install libfontconfig1-dev pkg-config` (Debian/Ubuntu). macOS uses Core Text and needs nothing extra.

**Get the source and build:**

```sh
git clone https://github.com/strata-works/winamp.git
cd winamp

# Build the whole workspace (dependency versions are pinned via a committed Cargo.lock).
cargo build --locked

# Run the test suite (hit-test kernel, engine, headless skin/scene tests).
cargo test --workspace
```

**Verify it works** — launch the live demo (a borderless, draggable GPU window; `Tab` cycles skins, `H` swaps hosts):

```sh
cargo run -p carapace-demo
```

The GPU render-correctness test needs a real adapter and is gated behind a feature, so it runs separately:

```sh
cargo test -p carapace --features gpu-tests --test render_offscreen
```

With the workspace building, you're ready to author a skin.

## What a skin is

A Carapace skin is a directory with two required files:

```
my-skin/
├── skin.toml     # manifest: id, canvas size, entry script, …
└── skin.lua      # the skin: declarative primitive calls
└── assets/       # (optional) fonts & images referenced by the skin
```

The engine loads `skin.toml`, runs `skin.lua` **once** against a vocabulary of primitives to build a static scene, then lays it out and renders it every frame. Live values (track title, meter levels, …) flow in from the **host** at render time; the Lua body does not re-run.

## 1. The manifest — `skin.toml`

```toml
schema = 1                              # must be 1
id = "my-skin"
name = "My Skin"
engine = "^0.1"                         # must be "^0.1"
canvas = { width = 300, height = 140 }  # design resolution
entry = "skin.lua"
```

`schema`, `id`, `name`, `engine`, `canvas`, and `entry` are required. `asset_dir` (default `"assets"`), `resizable`, `min_size`, and `max_size` are optional — see [Skin Authoring Reference → Manifest](./skin-authoring.md#the-skintoml-manifest). Source: `crates/carapace/src/skin.rs`.

## 2. The skin — `skin.lua`

Every primitive is a global function taking one table. A minimal skin:

```lua
-- a background fill
fill{ path = rect{ x = 0, y = 0, w = 300, h = 140 }, color = { r = 12, g = 12, b = 12 } }

-- a text label
text{ text = "hello, carapace", x = 8, y = 8, size = 12, color = { r = 230, g = 230, b = 230 } }
```

`rect{}` is a shape helper that returns a point path; `fill{}` draws a filled polygon; `text{}` draws a label. The full primitive set (`fill`, `region`, `image`, `frame`, `view`, `value_fill`, `list`, `scrub`, `text`), the shape helpers (`rect`, `circle`, `rounded_rect`), gradients, host data bindings, and host actions are documented in the [Skin Authoring Reference](./skin-authoring.md).

A slightly richer skin wires interaction and live data:

```lua
-- whole-window drag region
region{ path = rect{ x = 0, y = 0, w = 300, h = 140 },
        on_press = function() host.begin_drag() end }

-- a play/pause button that calls a host action
fill{ path = rect{ x = 20, y = 40, w = 60, h = 60 }, color = { r = 80, g = 200, b = 120 },
      on_press = function() host.toggle_play() end }

-- a progress bar bound to live host data (0..1)
value_fill{ path = rect{ x = 100, y = 60, w = 180, h = 8 }, value = "position",
            color = { r = 120, g = 240, b = 130 } }

-- live text from the host
text{ value = "track_title", x = 100, y = 30, size = 12, color = { r = 230, g = 230, b = 230 } }
```

- `host.<action>()` calls (like `begin_drag`, `toggle_play`) invoke **actions** the host exposes.
- `value = "..."` / `text{ value = "..." }` bind to **host data** the host provides each frame.

Which action/data names exist is up to the host you run against (see [Skin Authoring Reference → Host data](./skin-authoring.md#host-data-hostname) and [→ Host actions](./skin-authoring.md#host-actions-hostaction)). The previewer supplies these interactively.

## 3. Preview it live

The `carapace-preview` dev tool renders your skin with the real engine and streams it to a browser, hot-reloading on every save:

```bash
cargo run -p carapace-preview -- path/to/my-skin
# or pin the port:
cargo run -p carapace-preview -- path/to/my-skin --port 8080
```

It prints `carapace-preview serving http://127.0.0.1:<port>` and opens your browser. Edit `skin.lua`, save, and the preview reloads within a moment. The browser panels let you:

- inject **Host data** values (so `value = "position"` etc. have something to show),
- edit **Parameters** (top-level `local` values) and pick nodes in the **Inspector** to edit their literals — both write back into `skin.lua`.

See [Preview Tool](./preview-tool.md) for the full UI, and try the bundled demos:

```bash
cargo run -p carapace-preview -- crates/carapace-demo/skins/minimal
cargo run -p carapace-preview -- crates/carapace-demo/skins/reference   # fullest-featured
```

## Where to go next

- **[Skin Authoring Reference](./skin-authoring.md)** — every primitive, field, shape helper, gradient, host binding, and the Lua sandbox rules.
- **[Preview Tool](./preview-tool.md)** — the previewer's inspector, parameters panel, and write-back.
- **[Engine API (Rust)](./engine-api.md)** — embed the engine and implement a `Host` for your own app.

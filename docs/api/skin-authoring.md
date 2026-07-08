# Skin Authoring Reference

The complete vocabulary you can write in `skin.lua`, plus the `skin.toml` manifest and the Lua sandbox rules. New here? Start with [Getting Started](./getting-started.md).

Reflects `crates/carapace/src/{vocab,script,scene,skin,shape,render}.rs` as of 2026-07-04, with `file:line` citations so it can be re-verified against source.

## Contents

- [Primitives](#primitives) — `fill`, `region`, `value_fill`, `image`, `frame`, `view`, `list`, `scrub`, `text`
- [Shapes & helpers](#shapes--helpers) — `rect`, `circle`, `rounded_rect`, paths, colors, gradients, `math`
- [Host data (`host.<name>`)](#host-data-hostname) — live value bindings
- [Host actions (`host.<action>()`)](#host-actions-hostaction) — invoking host behavior
- [The `skin.toml` manifest](#the-skintoml-manifest)
- [The Lua sandbox](#the-lua-sandbox)

## Primitives

Primitives are Lua globals that each take a single table argument, e.g. `fill{ ... }`. The engine registers one constructor per primitive in `VocabRegistry::base()` (`vocab.rs:530-542`) and wires them data-drivenly (`script.rs:132-166`). The stock set is nine primitives; hosts can register more (see [Custom primitives](#custom-primitives)).

**Two fields work on every primitive** (handled outside each primitive's own `build()`):

- `anchor = { "left", "right", "top", "bottom" }` + `min = { w =, h = }` — resize pins for frame skins (`parse_anchors`, `script.rs:96-113`). Default is top-left/fixed (`layout.rs:26-33`).
- `role = "drag" | "passthrough"` — only meaningful on primitives that create a hotspot (`fill`, `image`, `region`); default `Control` (`parse_role`, `vocab.rs:148-156`).

### `fill{}`

A filled polygon; also emits a hotspot if `on_press` is set. Registered at `vocab.rs:205-221`.

| field | type | meaning |
|---|---|---|
| `path` | point path (≥3 points) | polygon outline — from `rect{}`/`circle{}`/`rounded_rect{}` or a literal point list |
| `color` | `{r,g,b,a?}` | solid fill; supply `color` **or** `gradient` (if both are given, `gradient` wins — no error); `a` defaults 255 |
| `gradient` | table (see [Gradients](#gradients)) | gradient fill instead of `color` |
| `on_press` | function | optional; adds a `Hotspot` over the path's bounds |
| `role` | `"drag"\|"passthrough"` | hotspot role (with `on_press`) |

```lua
-- clickable button
fill{ path = rect{ x = 20, y = 20, w = 70, h = 70 }, color = { r = 80, g = 200, b = 120 },
      on_press = function() host.toggle_play() end }

-- gradient fill
fill{ path = { {x=148,y=18},{x=184,y=18},{x=184,y=54},{x=148,y=54} }, gradient = {
  type = "radial", center = { x = 166, y = 36 }, radius = 18,
  stops = { { at = 0, color = { r=255, g=255, b=255, a=150 } },
            { at = 1, color = { r=255, g=255, b=255, a=0 } } } } }
```

### `region{}`

An invisible hotspot (no drawing) — overlay a click target onto artwork already drawn (e.g. bitmap buttons). Registered at `vocab.rs:223-240`.

| field | type | meaning |
|---|---|---|
| `path` | point path | hit-test polygon |
| `on_press` | function | **required** handler |
| `role` | `"drag"\|"passthrough"` | default `Control` |

```lua
region{ path = rounded_rect{ x = 0, y = 0, w = 300, h = 140, radius = 14 },
        on_press = function() host.begin_drag() end }
```

### `value_fill{}`

A polygon whose fill is clipped to a live 0..1 fraction — meters, VU bars, seek fills. Registered at `vocab.rs:242-258`.

| field | type | meaning |
|---|---|---|
| `path` | point path | region to partially fill |
| `value` | string | host binding key, read each frame and clamped to 0..1 (`value_of`, `render.rs:116-121`) |
| `color` | `{r,g,b,a?}` | fill color of the filled portion |
| `direction` | `"right"\|"left"\|"up"\|"down"` | growth direction; default `"right"` |

```lua
for i = 0, 11 do
  value_fill{ path = rect{ x = SX + 4 + i*15, y = SY + 18, w = 10, h = 24 },
              value = "viz_" .. i, direction = "up", color = { r = 80, g = 240, b = 140 } }
end
```

### `image{}`

Draws a bitmap asset; optionally clickable. Registered at `vocab.rs:260-289`.

| field | type | meaning |
|---|---|---|
| `asset` | string | asset filename under the manifest's `asset_dir` |
| `x`, `y` | number | top-left position |
| `w`, `h` | number, optional | destination size; default = native pixel size |
| `on_press` | function, optional | hotspot over the dest rect |
| `role` | string, optional | hotspot role |

```lua
image{ asset = "headspace.png", x = 0, y = 0 }
```

### `frame{}`

9-slice-scaled bitmap window chrome, for resizable frame skins. Registered at `vocab.rs:291-338`.

| field | type | meaning |
|---|---|---|
| `asset` | string | source bitmap |
| `x`,`y`,`w`,`h` | number | destination rect |
| `slice` | `{left,right,top,bottom}` | 9-slice inset widths (all required) |
| `center` | `"stretch"\|"hollow"` | stretch the center tile or leave it empty (transparent viewport); default `"stretch"` |

```lua
frame{ asset = "window.png", x = 0, y = 0, w = 480, h = 320,
       slice = { left = 16, right = 16, top = 34, bottom = 16 },
       center = "hollow", anchor = { "left", "right", "top", "bottom" } }
```

### `view{}`

Declares a named rectangle for **embedder-provided** content (native video, a host UI surface). The skin doesn't draw it; the host composites into it via `Scene::views()` (`scene.rs:383-391`). Registered at `vocab.rs:340-356`.

| field | type | meaning |
|---|---|---|
| `id` | string | identifier the embedder looks up |
| `x`,`y`,`w`,`h` | number | destination rect (all required) |

```lua
view{ id = "app", x = 16, y = 34, w = 448, h = 270,
      anchor = { "left", "right", "top", "bottom" }, min = { w = 288, h = 150 } }
```

### `list{}`

A host-driven row list (e.g. a playlist), expanded each frame from `host.rows(collection)`. Registered at `vocab.rs:358-436`.

| field | type | meaning |
|---|---|---|
| `collection` | string | key passed to `Host::rows(collection)` |
| `x`,`y`,`w`,`h` | number | list region |
| `row_height` | number | pixels per row; visible rows = `floor(h / row_height)` |
| `on_select` | string, optional | host action invoked with the clicked row index |
| `selected` | string, optional | host binding key (scalar index) of the "current" row |
| `highlight` | `{r,g,b,a?}`, optional | highlight bar color for `selected` (set both together) |
| `template` | array of cell tables | text cells per row (see below) |

Each `template` cell (`vocab.rs:360-396`):

| field | type | meaning |
|---|---|---|
| `bind` | string | row-data key (`Row::get`, `host.rs:29-31`) |
| `x` / `right` | number | offset from the region's left / right edge — supply at least one (if both, `x` wins) |
| `y` | number, optional | vertical offset in the row (default 0) |
| `size` | number, optional | font size (default 16) |
| `color` | `{r,g,b,a?}` | text color |
| `halign` | `"left"\|"center"\|"right"`, optional | default `"left"` |
| `font` | string, optional | bundled font asset |

```lua
list{ collection = "playlist", x = SX, y = SY + 52, w = SW, h = 64, row_height = 16,
      on_select = "play_index",
      selected = "current_index", highlight = { r = 36, g = 112, b = 66, a = 175 },
      template = {
        { bind = "title", font = "vt323.ttf", x = 2, y = 1, size = 13, color = { r=190, g=245, b=205 } },
      } }
```

### `scrub{}`

A progress bar + click/drag-to-seek control. Registered at `vocab.rs:438-462`.

| field | type | meaning |
|---|---|---|
| `x`,`y`,`w`,`h` | number | bar rect |
| `value` | string | host binding key for the fill fraction |
| `color` | `{r,g,b,a?}` | fill color |
| `direction` | string, optional | fill direction; default `"right"` |
| `on_seek` | string | **required** host action, invoked with a 0..1 click fraction (`scene.rs:421-445`) |

```lua
scrub{ x = SX, y = SY + 120, w = SW, h = 4, value = "position", on_seek = "seek",
       color = { r = 120, g = 240, b = 130 } }
```

### `text{}`

Static or live-bound text. Registered at `vocab.rs:464-507`.

| field | type | meaning |
|---|---|---|
| `text` | string | static text (mutually exclusive with `value`) |
| `value` | string | host binding key for live text (`text_of`, `render.rs:124-129`) |
| `x`,`y` | number | anchor position |
| `size` | number, optional | font size (default 16) |
| `color` | `{r,g,b,a?}` | solid color (or use `gradient`; if both, `gradient` wins) |
| `gradient` | table | gradient-painted text |
| `font` | string, optional | bundled font; falls back to the system font |
| `halign` | `"left"\|"center"\|"right"`, optional | default `"left"` |
| `valign` | `"top"\|"middle"\|"bottom"`, optional | default `"top"` |
| `max_width` | number, optional | enables word-wrap at this pixel width |

Exactly one of `text` / `value` is required (`vocab.rs:471-480`).

```lua
text{ text = "_", x = 270, y = 4, size = 16, color = { r = 200, g = 200, b = 210 } }
text{ value = "track_title", font = "vt323.ttf", size = 13, x = SX, y = SY, color = { r = 150, g = 250, b = 170 } }
text{ text = "carapace\nminimal skin", size = 12, x = 8, y = 8, max_width = 120, color = { r = 230, g = 230, b = 230 } }
```

### Custom primitives

The vocabulary is open. Any Rust crate can implement `carapace::vocab::Primitive` and register it via `VocabRegistry::register` (`vocab.rs:523-525`); a skin author then calls it like any built-in. The demo host ships two:

- **`transport{ x, y }`** — packaged play/stop/seek control (`crates/carapace-demo/src/transport.rs`).
- **`gauge{ x, y, value, label }`** — labeled vertical meter (`crates/carapace-demo/src/gauge.rs`).

See [Engine API → Vocab](./engine-api.md#vocab) for how to build one.

## Shapes & helpers

Pure path generators injected into the sandbox — they return `{x=,y=}` point lists usable anywhere a `path =` is expected, emit no nodes, and carry no capability (`script.rs:184-224`).

### `rect{ x, y, w, h }`

Axis-aligned rectangle (`shape.rs:3-11`). `fill{ path = rect{ x=0, y=0, w=10, h=10 }, color = {...} }`

### `circle{ cx, cy, r, segments? }`

Circle approximated with `segments` points (default 48) (`shape.rs:13-25`). `fill{ path = circle{ cx=240, cy=55, r=28 }, color = {...} }`

### `rounded_rect{ x, y, w, h, radius, segments? }`

Rounded rectangle; 4 corner arcs of `segments` points each (default 8); `radius` clamps to `min(w,h)/2` (`shape.rs:29-51`).

### Path literal

Any `path =` may instead be a plain array of `{x=,y=}` points (≥3), parsed by `parse_path` (`vocab.rs:39-55`):

```lua
region{ path = { {x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48} }, on_press = ... }
```

### Colors

`{ r, g, b, a? }` with 0-255 channels; `a` defaults to 255 (opaque) (`color_from_table`, `vocab.rs:57-64`).

### Gradients

Supply `gradient = { ... }` wherever a `color =` could go (`fill{}`, `text{}`). If a primitive is given both `color` and `gradient`, `gradient` silently wins (no error) — unlike `text{}`'s `text`/`value` pair, which is a hard either/or that *does* error. (`parse_gradient`, `vocab.rs:82-138`; precedence at `vocab.rs:140-146`.)

| `type` | extra fields | meaning |
|---|---|---|
| `"linear"` | `from = {x,y}`, `to = {x,y}` | linear axis endpoints |
| `"radial"` | `center = {x,y}`, `radius` | radial |
| `"sweep"` | `center = {x,y}`, `start_deg?` (0), `end_deg?` (360) | conic/sweep |

`stops` is a list of `{ at = 0..1, color = {r,g,b,a?} }` (≥2 entries; auto-sorted by `at`).

```lua
gradient = { type = "sweep", center = { x = 282, y = 20 }, start_deg = 0, end_deg = 360,
  stops = { { at = 0,   color = { r=255, g=90,  b=90 } },
            { at = 0.5, color = { r=90,  g=130, b=255 } },
            { at = 1,   color = { r=255, g=90,  b=90 } } } }
```

### `math`

The full standard Lua `math` library (sin/cos/sqrt/pi/floor/min/max/random/…) is exposed verbatim for procedural geometry (`script.rs:226-231`). It is pure and capability-free.

## Host data (`host.<name>`)

Host data is read through **string binding keys** — not `host.foo` calls. You embed a key (`value = "..."`, `collection = "..."`, `selected = "..."`) in a primitive table, and the engine resolves it every frame against the host, never touching Lua at read time.

- `Host::get(key) -> Option<StateValue>` — scalar/bool/string reads (`host.rs:40-50`).
- `Host::rows(collection) -> Vec<Row>` — collection reads for `list{}`.
- `StateValue = Bool(bool) | Scalar(f32) | Str(Arc<str>)` (`state.rs:1-6`).

Where each binding is consumed:

- `value` on `value_fill{}` / `scrub{}` → clamped 0..1 scalar (`render.rs:116-121`).
- `value` on `text{}` → string (`render.rs:124-129`).
- `collection` on `list{}` → `host.rows(collection)` (`engine.rs:216-222`).
- `selected` + `highlight` on `list{}` → scalar index vs row index for the highlight bar (`engine.rs:236-268`).

Binding names are **host-defined** — the engine treats them opaquely. The demo music-player host provides, for example: `playing` (Bool), `current_index` (Scalar), `position` (Scalar 0..1), `track_title` (Str), `time` (Str), `viz_<i>` (Scalar 0..1), and a `playlist` collection with `now`/`title`/`duration` cells (`crates/carapace-demo/src/music_player_host.rs`). When authoring, target whatever keys your host exposes; the previewer lets you inject arbitrary keys to test.

## Host actions (`host.<action>()`)

`host` is a real Lua table built from the host's action allowlist (`Host::actions()` → `ActionSpec { name }`, `host.rs:34-44`). It installs one enqueue-shim per allowlisted name (`script.rs:169-182`); calling any name not on the allowlist is a Lua error. A shim doesn't mutate the host directly — it enqueues a `Command::HostAction { action, args }`, decoupling Lua from host mutation.

Actions are wired three ways:

1. **From a primitive's `on_press`** — a closure calling `host.<action>(...)`:
   ```lua
   on_press = function() host.toggle_play() end
   ```
2. **By name string** on `list{ on_select = }` / `scrub{ on_seek = }` — the engine fires the action when a row/seek is clicked, passing the row index / seek fraction automatically.
3. **Directly from a Rust primitive** via `ctx.host_action(action, args)` — no Lua glue (used by `transport{}` etc.).

Action names are host-defined. The demo hosts use `begin_drag` / `minimize` / `close` (window ops), and `toggle_play` / `stop` / `next` / `prev` / `seek` / `play_index` (music). See `crates/carapace-demo/src/{window,music_player_host}.rs`.

## The `skin.toml` manifest

Schema: `Manifest` (`skin.rs:20-39`), loaded/validated by `load_dir` (`skin.rs:65-85`).

| field | type | required | meaning |
|---|---|---|---|
| `schema` | u32 | yes | must equal `1` |
| `id` | string | yes | skin identifier |
| `name` | string | yes | display name |
| `engine` | string | yes | must equal `"^0.1"` (exact string match) |
| `canvas` | `{width,height}` | yes | design-resolution size |
| `entry` | string | yes | Lua entry filename, relative to the skin dir |
| `asset_dir` | string | no | assets subdir; default `"assets"` |
| `resizable` | bool | no | resizable window (frame-skin archetype); default `false` |
| `min_size` | `[w,h]` | no | minimum window size (logical px) |
| `max_size` | `[w,h]` | no | maximum window size (logical px) |
| `transition` | `{kind,duration_ms}` | no | swap-in dissolve; see [below](#the-transition-table) |

```toml
# fixed-size skin
schema = 1
id = "headspace"
name = "Headspace (reference homage)"
engine = "^0.1"
canvas = { width = 342, height = 394 }
entry = "skin.lua"
```

```toml
# resizable frame skin
schema = 1
id = "frame"
name = "Frame"
engine = "^0.1"
entry = "skin.lua"
canvas = { width = 480, height = 320 }
resizable = true
min_size = [320, 220]
asset_dir = "assets"
```

### The `[transition]` table

Schema: `Transition`/`TransitionKind` (`skin.rs:26-52`). Declares how *this* skin dissolves in when a host swaps another skin *to* it (via `carapace_swap_skin`/`carapace_swap_skin_resized`) — it's a property of the incoming skin, not the outgoing one.

| field | type | required | meaning |
|---|---|---|---|
| `kind` | `"cut"` \| `"crossfade"` | no | dissolve style; default `"crossfade"` |
| `duration_ms` | u32 | no | dissolve duration in ms; default `250`, clamped to `≤ 5000` on load |

An absent `[transition]` table is equivalent to `{ kind = "crossfade", duration_ms = 250 }`. `kind = "cut"` swaps instantly — still stall-free, since the incoming skin is warmed off the render thread before it's presented.

```toml
# explicit fast cut
schema = 1
id = "terminal"
name = "Terminal"
engine = "^0.1"
canvas = { width = 480, height = 320 }
entry = "skin.lua"

[transition]
kind = "cut"
```

```toml
# explicit slow crossfade (clamped to 5000 if higher)
[transition]
kind = "crossfade"
duration_ms = 600
```

## The Lua sandbox

Each skin runs in a fresh `Lua::new()` VM against a custom `_ENV` table (not the default globals) — `script.rs:115-240`. The Lua body executes **once** at load to build the static scene graph plus a table of handler closures; interaction replays stored closures (`LoadedSkin::fire`, `script.rs:272-287`), it does not re-run the body.

**Available in the sandbox:**

- One constructor per registered primitive (the base 9 + host extensions).
- The `host` table (one shim per allowlisted action).
- Shape helpers: `rect`, `circle`, `rounded_rect`.
- `math` — the complete standard library (pure, capability-free).

**Blocked** (referencing any is a Lua "unknown global" error; verified by `script.rs:354-367`):

- `io` (no filesystem), `os` (no process/clock/env), `require` (no modules), `load` (no dynamic eval), `debug` (not injected).
- Any unregistered primitive or unknown global; any un-allowlisted `host.*` call.

**Documented subtlety:** Lua wires the *string* metatable at VM startup, so string methods on literals (`('x'):upper()`) remain reachable inside the sandbox. This is intentional — string methods are pure and capability-free — while `io`/`os`/`require`/`load` stay fully blocked (`script.rs:236-240`, tests `script.rs:555-594`).

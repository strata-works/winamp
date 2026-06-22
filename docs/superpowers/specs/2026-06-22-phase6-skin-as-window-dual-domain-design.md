# Phase 6 — Skin-as-Window, Dual-Domain — Design

**Date:** 2026-06-22
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Part of:** Phase 6 — the capstone. Folds **total window replacement** (the founding motivation,
de-risked by the window spike, PR #11) together with **cross-domain validation** (media player +
system monitor on one engine). Builds on Phases 2–5 (engine, vocab, assets, ergonomics, host
extensions).

## Purpose

Turn the demo into a real, **borderless skin-as-window** app that hosts **two different domains**
— a media player and a live system monitor — on the *exact same* engine, switchable at runtime.
That one artifact proves both founding theses at once:

1. **Total window replacement** — the skin *is* the window: no OS chrome, transparent background,
   shaped silhouette, draggable, with skin-drawn minimize/close.
2. **Zero domain knowledge** — the same engine + base vocabulary runs a media skin and a
   system-monitor skin; pressing one key swaps the whole domain.

**Neutrality proof (success criterion):** the only change inside `crates/carapace/src/` is a
domain-*neutral* rendering capability — a configurable transparent base color on the renderer (the
`render_offscreen` test changes only as a caller of the new field). All domain logic, all
window-chrome logic, and the new dependency live in `carapace-demo`.

### Two task groups

| Group | Adds |
|---|---|
| **1 — total window replacement** | transparent `base_color` (engine); borderless/transparent winit host; window-control host actions via a shared outbox; skin self-shaping |
| **2 — dual-domain validation** | `SysmonHost` (`sysinfo`); `gauge{}` extension; sysmon skin; live `H`-key host-switch |

## Scope

**In scope:**
- **Engine:** a `base_color: Color` (with alpha, default opaque) on `RenderTarget`, replacing the
  hardcoded opaque-black `RenderParams.base_color`. The *only* engine-crate change.
- **Host (demo):** a borderless + transparent winit window (`with_decorations(false)`,
  `with_transparent(true)`, `PostMultiplied` surface alpha, transparent vello base) — the spike's
  proven macOS/Metal path, productionized.
- **Window controls:** `begin_drag` / `minimize` / `close` as **host actions** recorded into a
  shared `WindowOutbox` that the App drains after `update()` and applies to the winit `Window`.
- **Skin shape:** vector skins self-shape via a `rounded_rect` backdrop over the transparent base;
  each demo skin gains a drag region + minimize/close glyphs wired to the window-control actions.
- **`SysmonHost`** (real metrics via `sysinfo`, added to `carapace-demo` via `sfw`): `cpu`/`mem`/
  `swap` scalars in `0..1` + `cpu_pct`/`mem_used` readouts; read-only domain; also exposes the
  window-control actions.
- **`gauge{}`** extension (demo crate) composing a vertical `value_fill` + `text` label + frame; a
  **sysmon skin** of `gauge{}`s.
- **Live host-switch:** one registry unions `base() + transport + gauge`; the **`H`** key issues
  `Command::SwitchHost`; **Tab** cycles skins within the active domain.

**Out of scope (later / noted):**
- **Alpha-shaping the Headspace *bitmap*** — the `reference` skin renders in a borderless but
  **rectangular** window (its PNG has no alpha; a clean alpha asset is a follow-on). Vector skins
  get true shaped silhouettes; the bitmap floats as a chrome-less rectangle.
- **Per-pixel click-through** — whole-window only is available via winit; per-pixel needs native
  shaping (the spike's known-hard unknown). Not built.
- **Linux/Windows transparency** — the spike validated macOS/Metal; the host codes defensively for
  the alpha mode but other platforms are best-effort.
- **Engine changes beyond `base_color`** — if any other engine change proves necessary, that is a
  finding to discuss, not a silent addition (it would dent the neutrality proof).

## 1. Architecture & invariants

The headless/GPU split and host-agnostic seam hold. The engine learns **nothing** about windows or
domains; window chrome and both domains live entirely in `carapace-demo`.

- **The one engine change is domain-neutral.** A transparent base color is a rendering capability
  (like gradients or text), carrying no media/sysmon/window meaning. "Zero domain knowledge" is
  intact; the proof is that `crates/carapace/src/` changes only `render.rs` (the `base_color`
  field), the `render_offscreen` test changing only to pass it.
- **Window control rides the existing command boundary.** `begin_drag`/`minimize`/`close` are host
  actions; `host.invoke` records a `WindowOp` into a shared outbox; the App (which owns the winit
  `Window`) drains the outbox and acts. The engine never touches the window — exactly the spike's
  recommendation ("through the Host/command boundary, not direct winit calls").
- **One engine, many domains.** A single `Engine` holds a registry unioning both extensions
  (`transport` + `gauge`); `SwitchHost` replaces the host + skin. A media skin's `toggle_play`
  fired on the sysmon host is dropped at the drain (existing allowlist rule) — but you only show a
  domain's skin on its own host.
- **Scene-as-projection, sandbox, closed `Node`, transactional swap** — all unchanged.

```
ENGINE (only domain-neutral change):
  render.rs   # RenderTarget gains `base_color: Color`; RenderParams uses it (was hardcoded opaque)

DEMO (everything else):
  carapace-demo/Cargo.toml          # + sysinfo (via sfw)
  src/window.rs        (new) # WindowOp, WindowOutbox, window-control action names + invoke helper
  src/demo_host.rs           # DemoHost gains window-control actions + an outbox handle
  src/sysmon_host.rs   (new) # SysmonHost (sysinfo) + window-control actions
  src/gauge.rs         (new) # gauge{} extension primitive
  src/transport.rs           # (unchanged; may gain min/close glyphs in its skin, not the prim)
  src/lib.rs                 # pub mod window/sysmon_host/gauge
  src/main.rs                # borderless/transparent window; transparent base_color; drain WindowOutbox;
                             #   H-key SwitchHost (media<->sysmon); registry = base()+transport+gauge
  skins/*/skin.lua           # rounded_rect backdrops + drag region + min/close glyphs
  skins/sysmon/        (new) # gauge row
  tests/...                  # host-switch, gauge build, window-op, transparent-base GPU test
```

## 2. Engine: transparent base color (`render.rs`)

```rust
pub struct RenderTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub base_color: crate::scene::Color,   // NEW; alpha-aware. Opaque black = today's behavior.
}
```

`draw()` uses it: `base_color: VColor::from_rgba8(t.base_color.r, t.base_color.g, t.base_color.b,
t.base_color.a)` instead of the hardcoded `from_rgba8(0,0,0,255)`. Existing callers (the
`render_offscreen` GPU tests) pass `Color { r:0,g:0,b:0,a:255 }` to preserve current behavior; the
demo passes `Color { 0,0,0,0 }` for window replacement. No other engine change in the phase.

## 3. Borderless / transparent host (`main.rs`)

Productionize the spike's proven setup:
- Window attrs: `Window::default_attributes().with_decorations(false).with_transparent(true)` (drop
  the title). Keep canvas×scale sizing.
- Surface `alpha_mode`: prefer `PreMultiplied`, else `PostMultiplied`, else `caps.alpha_modes[0]`
  (PreMultiplied is unavailable on macOS Metal → PostMultiplied is used; code handles either).
- Pass `base_color: Color { r:0, g:0, b:0, a:0 }` in `RenderTarget` so undrawn pixels are
  transparent and the desktop shows through.
- The existing Rgba8Unorm-intermediate → `TextureBlitter` → surface path already copies alpha
  faithfully (spike-confirmed); no blitter change needed.

## 4. Window controls (`window.rs`, both hosts, `main.rs`)

```rust
// window.rs (demo)
pub enum WindowOp { BeginDrag, Minimize, Close }
pub type WindowOutbox = std::rc::Rc<std::cell::RefCell<Vec<WindowOp>>>;
pub const WINDOW_ACTIONS: &[carapace::host::ActionSpec] = &[
    ActionSpec { name: "begin_drag" }, ActionSpec { name: "minimize" }, ActionSpec { name: "close" },
];
/// Returns true if `action` was a window-control action (and records the op).
pub fn handle_window_action(action: &str, out: &WindowOutbox) -> bool { /* match -> push -> true */ }
```

- Each demo host stores a `WindowOutbox` clone and returns `actions()` = its domain actions **++**
  `WINDOW_ACTIONS` (built once in `new`), and in `invoke()` calls `handle_window_action` first,
  falling back to its domain logic.
- `main.rs`: the App owns the outbox (shared into whichever host it constructs). After
  `engine.update(dt)`, it drains the outbox and applies each op to the `Window`:
  `BeginDrag → window.drag_window()`, `Minimize → window.set_minimized(true)`, `Close →
  event_loop.exit()`. (Drag is initiated within the press-event handling, as the spike confirmed.)

## 5. Skin self-shaping (`skins/*`)

Over the transparent base, a skin's **backdrop fill is its silhouette**:
- Vector skins (`classic`, `minimal`, `transport`, `sysmon`) use a **`rounded_rect` backdrop**
  (5d) → a rounded, floating window; outside the backdrop is desktop.
- Each gains a **drag region** (a `region{ on_press = host.begin_drag() }` over the backdrop —
  topmost interactive hotspots win via `Scene::hit`'s reverse iteration) and small **minimize /
  close glyphs** (`fill` + `region` → `host.minimize()` / `host.close()`), e.g. a `text{}` `_`/`x`
  or two small shapes in the top-right.
- The `reference` (Headspace bitmap) skin renders borderless but **rectangular** (no alpha); it
  still gets the drag region + min/close glyphs.

## 6. `SysmonHost` (`sysmon_host.rs`, `sysinfo`)

```rust
pub struct SysmonHost { sys: sysinfo::System, /* cached cpu/mem/swap */ window: WindowOutbox }
```
- `new(outbox)` builds a `System`, does an initial refresh.
- `tick(dt)`: `refresh_cpu_usage()` + `refresh_memory()`, recompute `cpu = global_cpu_usage()/100`,
  `mem = used_memory/total_memory`, `swap = used_swap/total_swap` (guard divide-by-zero → 0).
- `get(key)`: `cpu`/`mem`/`swap` → `Scalar(0..1)`; `cpu_pct` → `Str("NN%")`; `mem_used` →
  `Str("… MiB")`; else `None`.
- `actions()` = `WINDOW_ACTIONS` (read-only domain — no sysmon actions); `invoke` →
  `handle_window_action` (window ops only).
- First CPU reading may be 0 until the second refresh (sysinfo deltas); acceptable.

`sysinfo` is added via **`sfw cargo add sysinfo -p carapace-demo`** (demo-only; engine unaffected).

## 7. `gauge{}` extension (`gauge.rs`) + sysmon skin

`gauge{ x, y, value, label }` (demo `Primitive`, like `transport`): composes
- a frame `fill` (e.g. `rounded_rect`),
- a vertical `value_fill{ direction = Up }` bound to `value`,
- a `text{}` `label` beneath,
returning `Vec<Node>`. Uses only carapace's public API (`carapace::mlua::Table`, `shape`, `scene`,
`vocab`). Read-only (no `host_action`).

`skins/sysmon/skin.lua`: a `rounded_rect` backdrop + drag/min/close + a row of
`gauge{ value="cpu" label="CPU" }`, `gauge{ value="mem" … }`, `gauge{ value="swap" … }`.

## 8. Live host-switch (`main.rs`)

- The registry is built once: `base()` + `register(TransportPrim)` + `register(GaugePrim)`.
- The App tracks the active domain and a per-domain skin list (media: classic/minimal/reference/
  transport; sysmon: sysmon). `H` issues `Command::SwitchHost { host: <other domain's host>, skin:
  <that domain's first skin> }` (new host constructed with the shared outbox). `Tab` cycles within
  the active domain's list.
- On switch, the engine replaces host + rebuilds the skin against the same registry; the floating
  window becomes the other domain.

## 9. Testing

**Engine (gated GPU):** a transparent-base test — render an empty/partial scene with
`base_color = {0,0,0,0}`; an undrawn pixel reads alpha 0 (productionizes the spike's transparency
proof). Existing sentinels pass with explicit opaque `base_color`.

**Demo (headless / integration):**
- **Cross-domain on one `Engine`:** start on `DemoHost` + a media skin; `handle_command(SwitchHost
  { SysmonHost::new(outbox), sysmon source })`; `update`; the sysmon scene builds and `state("cpu")`
  is a `Scalar` in `[0,1]`.
- **`gauge{}`** builds the expected nodes (`ValueFill{ direction: Up }`, a `Text`, fills).
- **`SysmonHost`**: after a tick, `get("cpu")` is `Scalar` in `[0,1]` (range, not exact).
- **Window ops:** `handle_window_action("minimize", &out)` pushes `WindowOp::Minimize` and returns
  true; a domain action returns false; a host's `invoke("close")` records `Close`.

**Structural proof:** Phase 6's `git diff --stat` shows the only `crates/carapace/src/` change is
`render.rs` (the `base_color` field); the only other engine-crate change is the `render_offscreen`
test passing that field. No domain code in the engine — verified by inspection at review.

**Human:** `cargo run -p carapace-demo` → a borderless, shaped, draggable window floats on the
desktop; the skin's glyphs minimize/close it; **Tab** cycles skins; **`H`** switches the whole
window between the media player and the live system monitor on the same engine.

## 10. Error handling

- Malformed `gauge{}`/skin → `BuildError` → transactional swap keeps the prior scene.
- Window-control action on a host that lacks it → dropped at drain (existing rule).
- `sysinfo` returning zero/unavailable metrics → guarded to `0.0` (no panic / no divide-by-zero).
- An unavailable surface alpha mode → falls back per §3; the window still renders (opaque if no
  alpha mode supports transparency on the platform).

## Definition of done (Phase 6)

The demo runs as a **borderless, transparent, shaped, draggable** skin-as-window with skin-drawn
minimize/close; the **`H`** key live-switches the whole window between a **media player** and a
**real system monitor** (`sysinfo`) on one `Engine`; `gauge{}` and `transport{}` are both
host-registered extensions; the **only `crates/carapace/src/` change is the renderer's `base_color`**;
the headless boundary, the fast `check` CI job (incl. `clippy -D warnings`, both feature sets),
`fmt`, and the snapshot/GPU harnesses are all green. Phases 0–6 complete — the founding theses
(total window replacement + zero domain knowledge) are demonstrated end to end.

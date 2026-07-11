# Weather App Showcase — Design (sub-project 2)

**North star:** a native macOS weather app whose *skin is the window* — an animated WGSL
`shader{}` background whose look (and, later, whose very silhouette) reflects the current
weather, with the weather UI rendered by the carapace engine over it. The skin **wraps the
app**: a borderless, shaped, draggable window with no native title bar. It consumes the
`shader{}` primitive (sub-project 1, merged) and the existing ABI-3.x host bridge, so it
needs **zero engine-crate changes**.

This document details **Milestone 1** (app shell + static skin) and embeds a roadmap for
Milestones 2–4. Each later milestone gets its own spec + plan when reached.

## Decisions (from brainstorming, 2026-07-10)

- **Home:** a brand-new standalone macOS app (a `weather/` SwiftPM package parallel to
  `showcase/`), NOT an extension of the existing Showcase app or the demo.
- **Skin-as-window (total window replacement):** the engine-rendered surface IS the whole
  borderless window; there is no native toolbar/chrome. Draggable via an in-skin
  `region{ role='drag' }`; real macOS traffic-light buttons overlaid (M1), possibly dropped
  later for the flowing-silhouette aesthetic.
- **One über-shader:** a single `shader{}` whose WGSL `switch`es on a `condition` f32
  uniform (0–5), plus `is_day`/`temp`/`intensity`/`season` uniforms. Swift only updates
  uniform values — no skin swapping, instant condition changes.
- **Data (M2):** Swift `URLSession` → Open-Meteo (no API key) for current+hourly+daily, with
  a bundled `mock.json` fallback and a `→` demo-cycle key overriding the live condition.
- **Location:** default city (Accra 5.55,−0.20) + a search field, realized as a native
  `NSTextField` overlaid on a `view{}` cutout (approach A) in M4.
- **UI:** hero (location, big temp, condition, hi/lo, feels) + horizontal hourly strip +
  vertical daily list.
- **Flowing bottom (M3):** only the **bottom band (~lowest 15%)** of the window is an
  animated, condition-reactive silhouette (alpha ripples off the bottom edge); the top and
  sides stay a stable rounded rectangle that holds the UI and traffic lights.

## Architecture

```
The weather SKIN is the borderless/shaped window (the app IS the skin)
weather/ (new SwiftPM macOS app)
  WeatherApp (@main)                 window bootstrap
  SkinWindow  (NSWindow, .borderless, transparent, shaped)   ← copied from showcase/
  SkinView    (NSView; layer.contents = IOSurface; input)    ← copied from showcase/
  CarapaceBridge (engine create, IOSurface pool, render thread, frame_ready) ← copied
  HostCallbacks  (the CarapaceHostVTable Swift builds)        ← adapted from showcase/
  WeatherHost    (the vtable ctx: answers get_num/get_str/rows)   ← NEW
  skins/weather/{skin.toml, skin.lua, assets/weather.wgsl}       ← NEW
        │ carapace-ffi (ABI 3.x cdylib + carapace.h) — UNCHANGED
        ▼
  engine renders skins/weather (shader{} 4-stage + 2D UI) → IOSurface → SkinView
```

**Zero engine changes.** Everything is app + skin + shader authoring. The FFI render path
already threads `engine.elapsed_secs()` into `RenderTarget.time`, so shader animation works
through the FFI unchanged.

### The display path (concrete)

The window's visible content — and its silhouette — is the alpha mask of the IOSurface the
engine renders each frame:

1. The `carapace-ffi` **render thread** renders `skins/weather` (shader 4-stage + 2D) into
   one of a **pool of 3 BGRA IOSurfaces** the host created.
2. The C **`frame_ready(ctx, index, frame_id)`** callback fires → global frame sink →
   `onFrame(surface, index)` on the main thread.
3. `SkinView.show(surface:)` sets **`layer.contents = surface`** — `IOSurface` is directly
   CALayer-contents-compatible, so CoreAnimation composites it with **no copy**.
   `wantsLayer = true`, `isOpaque = false`, clear background ⇒ per-pixel alpha ⇒ the window
   is transparent/shaped where the skin renders alpha < 1.
4. **Input** flows back through `SkinView` (`mouseDown`/`mouseDragged`/`keyDown` → canvas
   coords → carapace hit-test / host actions).

### Build / link (mirrors showcase/Package.swift)

- SwiftPM package, `swift-tools-version:6.0`, `platforms: [.macOS(.v13)]`.
- `.systemLibrary(name: "CCarapace", path: "Sources/CCarapace")` wrapping a copy of the
  generated `crates/carapace-ffi/include/carapace.h` via `module.modulemap`.
- Executable target links the Rust cdylib: `-L ../target/debug -lcarapace_ffi` + rpath;
  `swiftSettings: -Xcc -DCARAPACE_APPLE -Xcc -std=c23` (so the C importer sees the
  Apple-gated header and C23 self-referential enum typedefs).
- Requires `cargo build -p carapace-ffi` first to produce `libcarapace_ffi.dylib`.

## The über-shader (`skins/weather/assets/weather.wgsl`)

One `shader{}`, full-canvas, fragment-only (the engine supplies `vs`/`VsOut` + generates
`struct U`). Host-bound uniforms (all `f32`, resolved raw/unclamped via the FFI `get_num`):

| uniform key   | meaning                                                        |
|---------------|----------------------------------------------------------------|
| `wx_condition`| 0 clear · 1 cloud · 2 rain · 3 snow · 4 storm · 5 fog (`switch`)|
| `wx_is_day`   | 0 night / 1 day (palette + sun/moon)                            |
| `wx_temp`     | current temperature °C (raw; tints warm/cool)                  |
| `wx_intensity`| 0–1 precipitation/cloud intensity (motion/density)             |
| `wx_season`   | 0 winter · 1 spring · 2 summer · 3 autumn (from month)         |

Plus the engine built-ins `u.time` (seconds) and `u.res`. `fs()` dispatches on
`i32(u.condition)` to a per-condition helper (`clear()`, `cloud()`, `rain()`, `snow()`,
`storm()`, `fog()`) sharing noise/gradient utilities. **M1 authors a first pass of the six
helpers** (recognizable, animated); polish + day/night + season tinting is M3. **M3** also
adds the bottom-band flowing alpha (see Roadmap). Output is **premultiplied** color so
Stage-3 compositing and future soft alpha edges don't fringe.

**WMO code → condition bucket** (Swift-side mapping, used M2; documented here for the shader
contract): 0–1→clear · 2–3→cloud · 45,48→fog · 51–67,80–82→rain · 71–77,85–86→snow ·
95–99→storm.

## Host-data contract (Swift ⇄ skin)

The skin reads these keys; `WeatherHost` answers them through the vtable. This is the stable
interface between the Swift host and the skin, independent of where the data comes from
(mock in M1, live in M2).

**Numeric (`get_num`)** — shader uniforms + hourly cells:
`wx_condition`, `wx_is_day`, `wx_temp`, `wx_intensity`, `wx_season`,
and `wx_hour_{i}_temp` for `i` in `0..12` (the hourly strip).

**String (`get_str`)** — hero + hourly labels:
`location`, `condition_text`, `temp_now` ("27°"), `hi_lo` ("H:31° L:24°"), `feels`
("Feels 30°"), and `wx_hour_{i}_time` ("13h") for `i` in `0..12`.

**Rows (`row_count`/`get_row_str`/`get_row_num`)** — the daily forecast, collection
`"daily"`, ~7 rows, fields: `day` (str "Mon"), `hi` (str "31°"), `lo` (str "24°"),
`glyph` (str, a condition symbol).

**Actions (`invoke`)** — M2+: `next_condition` (demo cycle); M4: location submit.

## Skin layout (`skins/weather/skin.lua`)

Canvas **~400×680 portrait** (adjustable). Draw order = shader background, then 2D UI:

- `shader{ src="weather.wgsl", x=0,y=0,w=400,h=680, uniforms={ condition="wx_condition",
  is_day="wx_is_day", temp="wx_temp", intensity="wx_intensity", season="wx_season" } }`
- `region{ path=rect{full}, role='drag', on_press=function() host.begin_drag() end }` (drag).
- **Hero:** `text{}` bound to `location`, `condition_text`, `temp_now` (large), `hi_lo`,
  `feels`.
- **Hourly strip:** `for i=0,11 do text{ value="wx_hour_"..i.."_time", ... }
  text{ value="wx_hour_"..i.."_temp", ... } end` laid out horizontally.
- **Daily list:** `list{ collection="daily", ... template={ day, glyph, hi, lo cells } }`.
- All content sits in the guaranteed-opaque zone (above the future bottom-flow band).

## Milestone 1 — App shell + static skin (this spec's implementable slice)

**Goal:** the new app launches as a borderless, draggable, skin-shaped macOS window
rendering the weather skin (animated über-shader + hero/hourly/daily UI) driven by a
**static/mock** `WeatherHost` — no network. Proves the skin-as-window pipeline end to end.

**Deliverables:**
1. `weather/` SwiftPM package: `Package.swift`, `Sources/CCarapace` (module.modulemap +
   copied `carapace.h`), `Sources/Weather` with `WeatherApp`, and the copied/adapted
   `SkinWindow`, `SkinView`, `CarapaceBridge`, `HostCallbacks`.
2. `weather/skins/weather/{skin.toml, skin.lua, assets/weather.wgsl}` — the über-shader
   (first pass of all six condition helpers) + hero/hourly/daily UI + drag region.
3. `WeatherModel` (Swift struct) with a hardcoded `.sample` value (e.g. Accra, partly
   cloudy, 27°, a plausible 12-hour + 7-day forecast), and `WeatherHost` (the vtable ctx)
   that answers the full host-data contract from a `WeatherModel`. M1 wires
   `WeatherHost(model: .sample)` — no separate mock class; M2 swaps `.sample` for a
   `WeatherService`-derived model.
4. Traffic-light overlay (close/minimize) as in the Showcase.

**Acceptance (verify):** `cargo build -p carapace-ffi` then `swift build` in `weather/`
succeeds; launching the app shows a borderless, shaped, draggable window with the animated
shader background and the hero/hourly/daily UI populated from `WeatherModel.sample`; a
temporary debug key in `SkinView.keyDown` cycles the mock `wx_condition` so each of the six
backgrounds can be eyeballed; the window drags; traffic lights close/minimize. (Driven from
a background session via the `launchctl asuser` + window-id-capture technique proven for the
Showcase.) The formal `next_condition` demo-cycle action is M2; M1's key is throwaway
verification scaffolding.

**Explicitly NOT in M1:** live fetch, geocoding, the search cutout, the bottom-flowing
silhouette, condition crossfades.

## Component isolation & testing

- `WeatherModel` (pure struct + formatting) — unit-testable; M1 hand-builds a sample, M2
  derives it from decoded Open-Meteo.
- `WeatherHost` (vtable glue over a `WeatherModel`) — unit-testable: assert get_num/get_str/
  row_* return the model's values for the contract keys.
- `WeatherService` (M2, pure: JSON → `WeatherModel`; mock fallback) — unit-testable with
  fixture JSON.
- `CarapaceBridge`/`SkinView`/`SkinWindow` — integration; verified by launching + eyeballing
  (and a `SwapGate`-style logic test where applicable).
- `weather.wgsl` — naga-validated at skin load (a bad shader fails `carapace_create` /
  skin load); visual eyeball via the app.

## Roadmap (later milestones, each its own spec + plan)

- **M2 — Live data.** `WeatherService` (Open-Meteo current+hourly+daily fetch/decode + mock
  fallback) + `WeatherModel` derivation (WMO→condition, season from month, is_day from
  local time, wx_* scalars, formatted strings, hourly/daily rows). Real weather drives the
  skin. Add the `→` demo-cycle key (`next_condition`) overriding the live condition.
- **M3 — Condition tour + bottom-flowing silhouette + polish.** Finish the six condition
  shaders; day/night + season tinting; the **bottom-band flowing alpha** (condition-reactive
  silhouette — gentle waves clear, drips/jagged rain+storm, dissolve fog, crystalline snow),
  with `hasShadow=false` and (likely) traffic lights hidden for the flowing aesthetic.
- **M4 — Location search cutout.** `view{ id="search" }` in the skin + an overlaid native
  `NSTextField` (approach A) at the cutout rect + Open-Meteo geocoding (name→coords) →
  refetch.

## Non-goals / deferrals

Condition crossfade blending; CoreLocation; wind/humidity/UV extra uniforms; precipitation
particle systems beyond what the shader draws; southern-hemisphere season inversion;
app-icon/notarization/packaging; multi-window.

## Constraints

- **Zero engine-crate changes.** All work is in `weather/` (+ a copied `carapace.h`). If
  something seems to need an engine change, stop and re-scope — the FFI + shader{} are
  expected to be sufficient.
- **Base:** branch `weather-app-showcase` off `main` (which now includes the shader
  primitive, commit 37ede2a).
- **Local gate before push:** `cargo build -p carapace-ffi` (the dylib the app links);
  `swift build` + `swift test` in `weather/`; and if any Rust changed (it should not),
  the engine gate. `swift-format`/`swiftlint` if the repo uses them.
- **Git identity** Daniel Agbemava <danagbemava@gmail.com>; no Claude attribution in
  commits/PRs; no direct push to `main`.

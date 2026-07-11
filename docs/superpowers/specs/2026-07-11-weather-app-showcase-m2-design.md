# Weather App Showcase — Milestone 2: Live Data — Design

**Sub-project 2, Milestone 2.** Builds on M1 (merged, PR #42), which shipped the borderless
skin-as-window app rendering `skins/weather` from a static `WeatherHost(model: .sample)`.

**Goal:** replace the static sample with **real Open-Meteo weather** for a fixed location
(Accra), auto-refreshing on a timer with a manual refresh key, an offline fallback that
reuses the live decode path, and a presenter **demo-cycle** that tours the six shader looks
without disturbing the live data.

## Guiding property: M2 is almost entirely Swift-side

M1 established a stable host-data contract (the keys the skin reads) and the skin already
reads every one of them. **M2 changes NO skin files (`skin.toml`/`skin.lua`/`weather.wgsl`)
and NO host-contract keys.** It only changes *where* `WeatherHost`'s `WeatherModel` comes
from (a live `WeatherService` instead of `.sample`) and adds refresh + demo-cycle control in
`App.swift`. This keeps the change surface small and the M1 skin/shader untouched.

**Zero engine-crate changes** (unchanged constraint): no edits under `crates/*`. The FFI +
`shader{}` primitive are sufficient.

## Decisions (from brainstorming, 2026-07-11)

- **Refresh lifecycle:** fetch on launch, auto-refresh on a **15-minute `Timer`**, and a
  manual **`R`** refresh key. On any fetch failure, fall back to the bundled fixture and
  retry on the next tick.
- **Demo cycle:** the **`→`** key advances a cycle `live → 0 clear → 1 cloud → 2 rain →
  3 snow → 4 storm → 5 fog → live`, `←` reverses. It sets a `conditionOverride` that forces
  **only the shader `wx_condition` uniform**; the hero/hourly/daily text stays the real live
  weather. Auto-refresh updates the underlying data but **never clobbers an active
  override**. The `live` slot returns to real conditions. This replaces M1's throwaway debug
  key.
- **Offline fallback:** a real Open-Meteo response is bundled as **`mock.json`** (a SwiftPM
  resource). On fetch failure it is decoded through the **same** `WeatherService.decode`
  path that live responses use, so the decoder runs even offline. The same JSON is the
  decoder's unit-test fixture.
- **Location:** fixed **Accra (latitude 5.55, longitude -0.20)**. Geocoding / the search
  cutout remain **M4**.

## Architecture

```
App.swift (@main / AppDelegate)
  ├─ init WeatherHost with WeatherService.decode(bundled mock.json)   ← no empty first frame
  ├─ Task { host.model = await service.fetch() }                      ← launch fetch
  ├─ Timer(15 min) → Task { host.model = await service.fetch() }      ← auto-refresh
  ├─ keyDown R  → manual refetch (same Task)
  └─ keyDown →/← → advance/retreat conditionOverride cycle

WeatherService (NEW)                     WeatherHost (M1, + conditionOverride)
  ├─ Codable response structs              num("wx_condition") = conditionOverride ?? model.condition
  ├─ decode(Data) -> WeatherModel          (all other keys unchanged; model set on MAIN thread,
  └─ fetch() async -> WeatherModel          read on RENDER thread — M1's NSLock guards this)
        │  URLSession → Open-Meteo
        │  failure → decode(bundled mock.json)
        ▼
  WeatherModel (M1 struct, unchanged shape)  ← the skin reads it via the unchanged contract
```

**Threading:** `fetch()` runs off the main thread (URLSession `async`). The derived
`WeatherModel` is assigned to `host.model` on the **main thread**. The engine reads
`host.model` on the **render thread** through the vtable. M1 already made `WeatherHost.model`
`NSLock`-guarded with per-method single-snapshot reads precisely for this — so the live
model swap is atomic w.r.t. render-thread reads. **This is the payoff of the M1
thread-safety fix; M2 introduces no new concurrency mechanism.**

## Components

### `WeatherService` (new) — fetch + decode + map + fallback

Pure decode/map is fully unit-testable; `fetch()` is the only side-effecting part.

**Open-Meteo request** (no API key required):
```
https://api.open-meteo.com/v1/forecast
  ?latitude=5.55&longitude=-0.20
  &current=temperature_2m,is_day,weather_code,apparent_temperature,precipitation,cloud_cover
  &hourly=temperature_2m,weather_code
  &daily=weather_code,temperature_2m_max,temperature_2m_min
  &timezone=auto&forecast_days=7
```

**`Codable` response structs** mirror exactly those fields (`current`, `hourly` arrays,
`daily` arrays, plus `hourly.time` / `daily.time` ISO strings and `current.time`).

**`decode(Data) throws -> WeatherModel`** derives the M1 `WeatherModel`:

| WeatherModel field | Derivation |
|--------------------|-----------|
| `condition` (Double 0–5) | `wmoBucket(current.weather_code)` — see mapping below |
| `conditionText` (String) | human label for the bucket (e.g. "Partly cloudy", "Rain") |
| `isDay` (Double 0/1) | `current.is_day` |
| `temp` (Double, raw °C) | `current.temperature_2m` |
| `intensity` (Double 0–1) | heuristic below |
| `season` (Double 0–3) | from the month of `current.time` (local), N-hemisphere |
| `tempNow` (String) | `"\(round(current.temperature_2m))°"` |
| `hiLo` (String) | `"H:\(round(daily.max[0]))° L:\(round(daily.min[0]))°"` |
| `feels` (String) | `"Feels \(round(current.apparent_temperature))°"` |
| `location` (String) | `"Accra"` (constant for M2; M4 makes it dynamic) |
| `hours` (12 × HourCell) | next 12 hourly entries from the current local hour; `time="\(hour)h"` (24h), `temp="\(round(t))°"` (**string**, matching the shipped M1 contract) |
| `days` (7 × DayRow) | `day` = weekday abbrev of `daily.time[i]`; `glyph` = condition symbol for `wmoBucket(daily.weather_code[i])`; `hi`/`lo` = `"\(round(max/min))°"` |

**WMO code → condition bucket** (from the M1 shader contract): `0–1 → 0 clear` · `2–3 →
1 cloud` · `45,48 → 5 fog` · `51–67,80–82 → 2 rain` · `71–77,85–86 → 3 snow` ·
`95–99 → 4 storm`. Any unmapped code → `1 cloud` (safe default).

**`intensity` heuristic** (gives the shader motion/density; deliberately simple):
- rain/snow/storm buckets → `clamp(current.precipitation / 8.0, 0.15, 1.0)`
- cloud/fog buckets → `clamp(current.cloud_cover / 100.0, 0.0, 1.0)`
- clear bucket → `clamp(current.cloud_cover / 100.0 * 0.3, 0.0, 0.3)`

**Hourly window:** find the first index in `hourly.time` at or after `current.time` (the
current local hour) and take the next 12. If fewer than 12 remain (edge of the array), take
the last 12 available.

**`season` from month** (N-hemisphere; southern inversion is a non-goal): Dec–Feb → 0
winter · Mar–May → 1 spring · Jun–Aug → 2 summer · Sep–Nov → 3 autumn.

**`fetch() async -> WeatherModel`:** GET the URL via `URLSession`; on success
`decode(data)`. On **any** thrown error (network, non-200, decode) load the bundled
`mock.json` and `decode` it. `fetch()` therefore always returns a valid model (never
throws to the caller) — the caller just assigns it to `host.model`.

### `mock.json` (new resource + test fixture)

A real captured Open-Meteo response for Accra with all requested fields, bundled via
`resources: [.copy("mock.json")]` (or a `Resources/` dir) in `Package.swift`. Loaded with
`Bundle.module`. It is decoded through `WeatherService.decode` both as the offline fallback
and as the unit-test input, so one artifact validates both paths.

### `WeatherHost` (M1, + `conditionOverride`)

Add `var conditionOverride: Double?` (default `nil`). The only behavior change:
`num("wx_condition")` returns `conditionOverride ?? model.condition`. All other `num`/`str`/
row methods are unchanged and continue to read the live `model`. The `conditionOverride`
field is mutated only from the main thread (the `→`/`←` keys); it is a single `Double?`,
independent of the lock-guarded `model`.

### `App.swift` (M1, + live wiring)

- Construct `WeatherService`; build the initial model synchronously from the bundled
  fixture (`try? service.decodeBundledFixture()`), init `WeatherHost` with it (so the first
  frame shows plausible data, not blank), set `hostBox.host`.
- `refresh()` helper: `Task { let m = await service.fetch(); await MainActor.run { host.model = m } }`.
- Call `refresh()` once after the window is up (launch fetch).
- Start a repeating 15-minute `Timer` calling `refresh()`.
- `handleKey(_:)` (replacing M1's throwaway debug cycle):
  - `→` (124): advance the override cycle (`nil → 0 → 1 → 2 → 3 → 4 → 5 → nil`).
  - `←` (123): reverse the cycle.
  - `R` (15): `refresh()` now.
- The cycle helper is a small pure function `nextOverride(_ current: Double?) -> Double?`
  (and `prevOverride`) so it is unit-testable without the app.

## Host-data contract

**Unchanged from M1.** Documented in the M1 design; M2 populates the same keys from live
data. The one internal addition is `WeatherHost.conditionOverride`, which is invisible to the
skin (it still just reads `wx_condition`).

## Component isolation & testing

- **`WeatherService.decode`** (pure) — unit-tested against `mock.json`: assert the WMO→bucket
  mapping across representative codes (0,2,48,61,71,95, an unmapped code), the formatted
  strings (`tempNow`/`hiLo`/`feels`), 12 hourly cells with the correct start hour and string
  temps, 7 daily rows, `season`/`isDay`, and `intensity` bounds per bucket.
- **`WeatherService.fetch` fallback** — unit-tested by pointing decode at the bundled fixture
  (the network path itself is exercised only in the live eyeball; `fetch` is thin over
  `decode` + `Bundle.module`).
- **`WeatherHost.conditionOverride`** — override wins for `wx_condition`, `nil` = live model,
  all other keys ignore the override.
- **`nextOverride`/`prevOverride`** — the cycle advances `nil→0…5→nil` and reverses.
- **Existing `WeatherHostTests`** — stay green (contract unchanged).
- **Live verification** — launch via `launchctl asuser 501` + window-id screencapture:
  confirm real Accra weather renders (temp/condition/hourly/daily populated from the API),
  `→` tours the six shader looks while the text stays live, `R` refetches, and an
  offline launch (or a forced failure) falls back to the bundled fixture.

## Explicitly NOT in M2

- Geocoding / location search / the `view{}` search cutout (**M4**).
- The bottom-flowing silhouette, finished six condition shaders, day/night + season shader
  polish, condition crossfades (**M3**).
- Formalizing `next_condition` as a skin-invokable host `invoke` action — there is no skin
  control that calls it in M2; the `→` keyDown is sufficient. Deferred until a skin control
  needs it (YAGNI).
- Wind/humidity/UV extra uniforms; CoreLocation; multi-location; southern-hemisphere season
  inversion; packaging/notarization.

## Constraints

- **Zero engine-crate changes** and **zero skin changes**. All work is in `weather/Sources`
  (+ `weather/mock.json` + `Package.swift` resource wiring + tests). If something seems to
  need an engine or skin change, stop and re-scope.
- **Base:** branch `weather-app-showcase-m2` off `main` (commit 7453318, which includes the
  merged M1 weather app).
- **Build order:** `cargo build -p carapace-ffi` (the dylib the app links) before
  `swift build`/`swift test` in `weather/`.
- **Local gate before push:** `cargo build -p carapace-ffi`; `swift build` + `swift test` in
  `weather/`. (No Rust changes expected.)
- **Git identity** Daniel Agbemava <danagbemava@gmail.com>; no Claude attribution in
  commits/PRs; no direct push to `main`.

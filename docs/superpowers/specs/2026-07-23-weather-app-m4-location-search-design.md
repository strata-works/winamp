# Weather App M4 — Location Search — Design

**Date:** 2026-07-23
**Status:** Approved
**Scope:** `weather/` app only — Swift host (`weather/Sources/Weather/`) + skin (`weather/skins/weather/skin.lua`). **Zero engine changes.**

## Motivation

The weather app is feature-complete through the shader revamp + tsunami/high-winds demo
conditions (PR #45), but its location is frozen: `WeatherService` hardcodes Accra
(`latitude: 5.55, longitude: -0.20, locationName: "Accra"`). There is no way to see any other
city's sky. M4 unfreezes location with an **in-skin search overlay** — the demo story becomes
"type any city, watch its sky." This milestone was explicitly parked as separate in the
shader-revamp spec's Out-of-scope.

## Approach

**In-skin overlay** (chosen over a native `NSTextField`/Spotlight panel and a native
`NSSearchField`). The search field, results list, and hints are drawn *by the skin itself*,
gated on a host binding, over the still-animating live scene. This preserves the app's founding
"the skin is the entire window" character (borderless, transparent, shaped, shader-is-the-whole-
window) and matches the cinematic aesthetic — no native chrome. Cost: the host must accumulate
typed text, geocode it, and expose the query + results to the skin as bindings; the skin renders
them. That cost is accepted.

**Scope: search & switch only** (chosen over search + saved favorites). One location at a time.
Pick a city → it becomes current, refetches, and is remembered on next launch. No favorites list,
no cycle-between-cities key. YAGNI: this is a cinematic showcase, not a daily-driver client.

## Interaction flow

- Press **`L`** → enter *search mode*. The live scene keeps animating behind a skin-drawn
  overlay; the scene is visually dimmed by the overlay card.
- **Type** a city name → the host accumulates the query, debounces ~250 ms, then geocodes it.
- **`↑ / ↓`** move the highlighted result (selection clamps at the ends).
- **`⏎`** commits the highlighted result → it becomes the current location, triggers a refetch
  (same code path as the existing `R` key), and the overlay closes.
- **`Esc`** cancels — the overlay closes and nothing changes.
- The committed location is persisted and restored on next launch.
- While in search mode, the existing tour keys (`→ ← D S R`) are **ignored** (input is routed to
  the search state machine, not the tour controller).

### Overlay layout (drawn by `skin.lua`, gated on `search_active`)

```
      ┌─────────────────────────────────────┐
      │  🔍  cape town▏                       │   ← query + blinking cursor
      ├─────────────────────────────────────┤
      │ ▸ Cape Town, Western Cape, South Af… │   ← highlighted result
      │   Cape Coral, Florida, United States │
      │   Capetown, …                        │
      ├─────────────────────────────────────┤
      │        ↑↓ select · ⏎ go · esc cancel │
      └─────────────────────────────────────┘
```

Centered card over the dimmed scene. States:
- **Empty query** → a prompt hint (e.g. "Type a city…"), no results rows.
- **Querying** → a status line (e.g. "…").
- **No matches / network error** → a status line ("No matches" / "Offline") in the card, no rows.
- **Results** → up to 5 rows, one highlighted; label format **"City, Admin1, Country"**
  (Admin1 omitted when the API returns none).

## Architecture

All host-side + skin. **Zero engine changes.**

### Input routing — `SkinView` / `App`

`SkinView.keyDown` today forwards only `e.keyCode` via `onKey`. Extend the hook to forward enough
of the event to recover typed characters (the full `NSEvent`, or `(keyCode, characters)`). The
host's key handler routes on mode:

- **Not in search mode:** current tour behavior (`→ ← D S R`, plus `L` now enters search mode).
- **In search mode:** printable input comes from `event.characters`; special keys by keycode —
  delete/backspace (51), return (36), escape (53), up (126), down (125). All other keys ignored.

### `LocationSearch` (new) — pure state machine

Owns: `query: String`, `results: [GeoResult]`, `selected: Int`, `status` (idle/querying/empty/
error/results). Methods for: type a character, backspace, move selection (clamped), set results,
commit (returns the selected `GeoResult` or nil), cancel/reset. **No AppKit import** — fully
unit-testable. Debounce lives in the host layer that drives it (a cancellable timer), not in the
pure machine.

### `Geocoder` (new) — protocol + Open-Meteo implementation

Mirrors `WeatherService`'s existing protocol + bundled-mock pattern so tests run offline.
- Endpoint: `https://geocoding-api.open-meteo.com/v1/search?name=<q>&count=5&language=en&format=json`.
- Decodes results → `[GeoResult]` (`name`, `admin1?`, `country?`, `latitude`, `longitude`).
- A mock implementation backed by a canned JSON fixture drives tests.
- Chosen over other geocoders because it matches the existing Open-Meteo forecast source and
  needs no API key.

### `WeatherHost` — new bindings the skin reads

- `search_active` (num, 0/1)
- `search_query` (str)
- `search_status` (str — human-readable state / status line)
- `search_count` (num — number of result rows)
- `search_sel` (num — highlighted row index)
- `search_row_<i>_label` (str — "City, Admin1, Country"), for `i` in `0..<count`

Follows the existing `wx_hour_<i>_<suffix>` indexed-binding convention already parsed in
`WeatherHost`.

### `WeatherService` + persistence

- The hardcoded `init` default (Accra) stays as the **fallback** only.
- On commit, the host rebuilds `WeatherService` with the picked `latitude/longitude/locationName`
  and calls `refresh()` (the existing fetch-and-swap-model path).
- **Persistence:** `UserDefaults` keys for lat/lon/name. Written on commit; read at launch —
  present → use it; absent → today's Accra default.

## Testing (TDD, no engine diff)

- **`LocationSearch` state machine:** typing appends; backspace on empty is a no-op; selection
  clamps at both ends; commit returns the highlighted result; escape/reset clears query, results,
  and selection.
- **`Geocoder` mapping:** decode a canned Open-Meteo JSON fixture → correct `[GeoResult]`,
  including the Admin1-absent case and the empty-results case.
- **Persistence round-trip:** save a location, reload, get it back; absent → Accra default.
- **Gate:** `swift test` all green; `git diff main --stat` touches no `crates/` (zero engine
  changes).

## Out of scope

- Saved favorites / multiple locations / a cycle-between-cities key (rejected — scope A).
- Native input UI of any kind (rejected — in-skin overlay chosen).
- Reverse geocoding / CoreLocation "use my location" (not requested).
- Engine changes of any kind.
- Fuzzy-matching or ranking beyond what Open-Meteo's geocoding returns.

## Verification

- Live: launch, press `L`, type a city, `↑↓` to pick, `⏎` → scene refetches to the new city;
  `Esc` cancels cleanly; tour keys resume after exit; relaunch restores the last-picked city.
- Gate: `swift test` green; `git diff main --stat | grep crates` empty.

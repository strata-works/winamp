# Weather App Showcase — Milestone 3: Shader Polish + Flowing Silhouette — Design

**Sub-project 2, Milestone 3.** Builds on M2 (merged, PR #43), which shipped live Open-Meteo
data driving the skin. M3 is the visual-polish milestone: finish the six condition shaders,
add day/night and season tinting, and — the signature feature — a **bottom-flowing,
condition-reactive silhouette** that makes the transparent window take an animated organic
shape.

**Goal:** the weather app looks finished and distinctive: each of the six conditions is
recognizable and reads well day and night, the palette shifts subtly by season, and the
window's bottom edge flows as an animated silhouette shaped by the shader's own alpha — the
founding "skin IS the window" aesthetic, now with a non-rectangular, living outline.

## The signature mechanism: alpha-shaped window

The M1/M2 `weather.wgsl` fragment returns opaque `alpha = 1` everywhere, so the window is a
solid 400×680 rectangle. M3 makes `fs()` compute a **per-pixel alpha**:

- `alpha = 1` above the bottom band (the whole UI region stays fully opaque).
- Within the bottom band (`uv.y` ∈ ~`[0.82, 1.0]`; note `uv.y = 0` is the TOP, `1` the
  BOTTOM, per the existing `sky()`), alpha ramps from `1` down through an **animated,
  condition-reactive edge** to `0` off the bottom.
- Output becomes **premultiplied**: `return vec4(col * alpha, alpha)`.

Because the window is already borderless, `isOpaque = false`, and clear-backgrounded, a
sub-1 alpha in the rendered IOSurface makes those pixels transparent under CoreAnimation
compositing — so **the window automatically takes the silhouette's shape; no windowing/masking
code is needed.** `hasShadow` is set to `false` so no rectangular drop shadow betrays the
underlying frame.

**Why this is zero-engine:** the FFI shader path renders the shader as stage 1 of the 4-stage
composite (shader background → transparent vello offscreen → premultiplied composite → view).
The shader stage writes its RGBA (including alpha) to the offscreen; the 2D UI draws nothing
in the bottom band; the premultiplied composite carries the sub-1 alpha through to the view
surface. No `crates/*` change is expected.

### Risk + first validation (Task 1 spike)

The whole silhouette rests on the assumption that **shader-authored `alpha < 1` survives the
4-stage composite all the way to the window surface** (i.e. nothing in the chain forces the
final surface opaque). This is plausible from the M1 findings (shader stage uses non-blended
replace; composite uses premultiplied-alpha blending) but is **not yet proven**. So **M3
Task 1 is a throwaway spike**: temporarily make `fs()` return `alpha = 0` for the bottom 15%
and confirm, by launching over a contrasting background, that the window is genuinely
see-through there.

- **GO:** the window is transparent in that band → proceed with the real silhouette.
- **NO-GO:** if the engine flattens the alpha (window stays opaque), **STOP and escalate** —
  the bottom-flow would need an engine change, which is out of M3's zero-engine scope, and the
  milestone must be re-scoped (e.g. ship the shader/day-night/season polish without the
  silhouette).

## The shader work (`weather.wgsl` full rewrite)

One fragment-only shader, same uniform contract as today (`u.time`, `u.res`, `u.condition`,
`u.is_day`, `u.temp`, `u.intensity`, `u.season`). Structure: shared noise/gradient helpers →
per-condition color helpers → a per-condition **silhouette-edge** helper → `fs()` that
dispatches, applies the temperature tint + season tint, then computes and premultiplies alpha.

### Bottom-flowing silhouette (per condition)

A helper `silhouette_alpha(uv, t, condition, intensity) -> f32` returns the window alpha,
`1` above the band and ramping to `0` below an animated edge line near `uv.y ≈ 0.82`. The
edge's character is condition-reactive:

- **clear (0):** gentle, slow sine waves — a calm liquid horizon.
- **cloud (1):** soft, low, rounded swells.
- **rain (2):** downward **drips** — vertical tendrils that stretch and pinch off below the edge.
- **storm (4):** **jagged/erratic** edge — sharp, fast, noisy peaks.
- **snow (3):** **crystalline scallops** — rounded, slightly faceted lobes, slow.
- **fog (5):** **soft dissolve** — no hard edge; alpha fades out with `fbm` noise so the
  bottom melts away rather than cutting.

Amplitude/speed scale gently with `intensity`. The edge stays within the bottom band so the
UI region above is always fully opaque.

### Finish the six condition looks

Refine each helper from the M1 "first pass" so it is clearly recognizable AND reads well at
night (not just day): e.g. clear night gets faint star flecks + the moon disc already
present; rain/storm get darker, moodier night palettes; snow reads moonlit blue at night;
fog stays luminous but dim at night. Keep the existing animation ideas (drifting sun/moon,
scrolling cloud fbm, rain streaks, snow flakes, lightning flash) and polish their color and
motion. Continue to honor `intensity` for density/motion.

### Day/night

Polish the existing `sky()` day↔night blend and make every condition consume `day`
consistently so the whole scene — not just the sky — dims and shifts cool at night.

### Season tinting

A **subtle** final-color tint from `wx_season`, applied after the condition color and
temperature tint, mixed at low strength (≈0.06–0.10) so it never overrides the condition:
- `0 winter` → cool, slightly desaturated (blue-white lift).
- `1 spring` → fresh, faintly green.
- `2 summer` → warm, a touch more saturated/vivid.
- `3 autumn` → amber/gold warmth.

A `season_tint(season) -> vec3<f32>` helper returns the tint color; `fs()` does
`col = mix(col, col * tint, strength)` (or an additive equivalent) at low strength.

## App / skin / host changes (small, zero engine)

### `WeatherHost` — two more overrides

Following M2's `conditionOverride` pattern exactly (lock-guarded, reusing the same `NSLock`):
- `var isDayOverride: Double?` — `num("wx_is_day")` returns `isDayOverride ?? model.isDay`.
- `var seasonOverride: Double?` — `num("wx_season")` returns `seasonOverride ?? model.season`.

Overrides force **only** their shader uniform; all string/row data stays live. Refresh sets
`model` only, so overrides survive a refresh (same guarantee as M2's condition override).

### Cycle helper — generalize

Generalize M2's `ConditionCycle` (hardcoded to 5) into a bounded cycle used by all three
overrides: `next(current, upTo:)` / `prev(current, upTo:)` walking `nil → 0 … upTo → nil`.
Condition uses `upTo: 5`, season `upTo: 3`, day/night `upTo: 1`. M2's `→`/`←` keep their
behavior via `upTo: 5`.

### `App.swift`

- **Hide the traffic lights** — stop installing them (the window is chrome-free). Quit via
  ⌘Q / the app menu (the existing menu already has Quit). The whole canvas stays draggable
  via the skin's drag region.
- **`hasShadow = false`** — so the flowing bottom edge reads cleanly with no rectangular
  shadow.
- **`handleKey`** gains, alongside M2's `→`(124)/`←`(123)/`R`(15):
  - **`D`** (keycode 2) — cycle `isDayOverride` via the generalized cycle (`upTo: 1`):
    `live → night(0) → day(1) → live`.
  - **`S`** (keycode 1) — cycle `seasonOverride` (`upTo: 3`): `live → 0 → 1 → 2 → 3 → live`.

### `skin.lua`

Tighten the daily list so all content ends **above** the bottom silhouette band: e.g.
`row_height` 40→36 and/or nudge the list up so the 7 rows end by ~`y = 555`, leaving the
lowest ~`120 px` (~17%) clear for the flow. The shader `x/y/w/h` still covers the full
400×680 canvas (the silhouette lives in the shader's own bottom region).

## Host-data contract

**Unchanged.** M3 adds `isDayOverride`/`seasonOverride` inside `WeatherHost` (invisible to the
skin — it still just reads `wx_is_day`/`wx_season`). No new keys.

## Component isolation & testing

- **`WeatherHost.isDayOverride`/`seasonOverride`** — unit-tested: override wins for its own
  key, `nil` = live, other keys ignore it (mirrors M2's `conditionOverride` tests).
- **Generalized cycle** — unit-tested across bounds: `next(nil, upTo:1)=0`, `next(1, upTo:1)=nil`;
  `next(3, upTo:3)=nil`; `prev(nil, upTo:3)=3`; and the existing condition `upTo:5` cases stay
  green.
- **`weather.wgsl`** — naga-validated at skin load (a bad shader fails `carapace_create`);
  visual eyeball via the app across conditions × day/night × season, and the transparency
  spike (Task 1) proves the alpha-shaped window.
- **Verification** — launch via `launchctl asuser 501` + window-id/region screencapture:
  confirm each of the six conditions in day and night (`→` + `D`), season cycling (`S`), and —
  the key check — that the **bottom edge is genuinely transparent/shaped** (capture over a
  contrasting background so the silhouette and see-through edge are visible, not a rectangle).

## Explicitly NOT in M3

- The location search cutout / geocoding (**M4**).
- Condition crossfade blending (a non-goal); overrides snap instantly.
- Foreground precipitation particle systems beyond what the shader draws.
- Any change to the host-data contract, the data layer (WeatherService), or the refresh logic.
- Southern-hemisphere season inversion; CoreLocation; packaging.

## Constraints

- **Zero engine-crate changes.** All work is in `weather/Sources/Weather`, `weather/Tests`,
  and `weather/skins/weather` (`weather.wgsl` + `skin.lua`). If the Task 1 spike shows the
  silhouette needs an engine change, **STOP and escalate / re-scope** — do not edit `crates/*`.
- **Base:** branch `weather-app-showcase-m3` off `main` (commit `bb84bc5`, which includes the
  merged M2).
- **Build order:** `cargo build -p carapace-ffi` before `swift build`/`swift test` in `weather/`.
- **Local gate before push:** `cargo build -p carapace-ffi`; `swift build` + `swift test` in
  `weather/`.
- **Git identity** Daniel Agbemava <danagbemava@gmail.com>; no Claude attribution in
  commits/PRs; no direct push to `main`.

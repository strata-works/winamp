# Weather App Demo Conditions: Tsunami + High Winds — Design

**Date:** 2026-07-16
**Status:** Approved
**Scope:** `weather/` app only — shader, Swift host. **Zero engine changes.**
**Branch/PR:** continues `weather-app-shader-revamp` / PR #45.

## Motivation

Two demo-spectacle conditions that push the theatrical-window thesis past what forecast data
can express. Both are **demo-only** (user decision A): reachable via the →/← tour and
`WX_COND=6/7`, never selected by live data (`wmoBucket` is untouched — Open-Meteo cannot
forecast a tsunami, and honesty beats fake alerts).

## Condition 6 — High winds ("clear gale", full flex)

**Scene:** hard-blue `sky_dome`; shredded racing clouds — a short `march_clouds` pass with an
x-stretched noise domain and ~8× drift speed (torn streaks, not puffs); two debris layers
(the `snow_layer2` grid trick: horizontal, fast, leaf-ochre tinted) whose speed + slant surge
with `moment()`-scheduled gusts.

**Window theatrics (`window_alpha` case 6):**
- **Tremble** — constant fine hash jitter of the mask coordinate (~1–2 px).
- **Gust jolts** — the whole silhouette shoved ~6 px downwind with a snap-back envelope,
  moment-synced with the debris surges.
- **Top-edge fabric flap** — a traveling ripple (`sin(x·k − t·fast)`, gust-enveloped) luffing
  the top edge.
- **Debris impacts** (~every 8–20 s): a `moment()` channel picks an edge point + time; a
  debris streak converges on that point over the final ~0.3 s; on contact the window takes a
  **localized dent** that springs back with a damped wobble (2–3 overshoot oscillations).
  Streak and dent are synthesized independently from the same hash — no particle tracking.

## Condition 7 — Tsunami (full engulf + window deformation)

A looping **32 s** arc driven by **`u.cond_age`** — NOT shader time — so the host computes the
identical phase from its own clock (the snow-burial sync trick at full scale). Condition entry
resets `wx_cond_age` → the → tour always starts at calm.

| Phase | Window | Scene |
|---|---|---|
| 0.00–0.45 calm → swell | normal | dome sky; 4 parallax fbm wave bands low on screen; horizon line visibly rises |
| 0.45–0.60 rise | impact bulge begins | wave wall grows up the frame; foam crest; 2D spray particles |
| 0.60–0.74 **crash + engulf** | bulge peaks, then **water sheets off all edges** | foam wall sweeps down-frame, then full-screen underwater: deep teal, caustic shimmer (2D fbm), rising bubbles — **the forecast drowns (host blanks all UI)** |
| 0.74–1.00 recede | edges shed water streams | water drains down-frame; sky returns; bands settle |

**Water rendering (approach decision):** layered 2D ocean — 3–4 parallax fbm-displaced wave
band silhouettes with foam crests; crash spray = cheap 2D particles. (Ray-marched heightfield
rejected: second expensive marcher, no visible payoff at 400×680.)

## Host coordination (Swift, TDD)

New pure `Tsunami` enum mirroring the shader cycle — one threshold constant on each side,
changed together:

```swift
enum Tsunami {
    static let period: Double = 32
    static let engulfStart: Double = 0.60   // phase fraction
    static let engulfEnd: Double = 0.74
    static func phase(age: Double) -> Double        // (age mod 32) / 32
    static func isEngulfed(age: Double) -> Bool
}
```

While the effective condition is 7 AND `isEngulfed(conditionAge())`:
- `str()` returns `""` for every display key (empty strings skip rendering — `scene.rs`
  drops them).
- `rowCount()` returns 0.

Hero, hourly, and daily all vanish underwater and return on recede. Tests: `phase(0) == 0`,
engulf boundary values, wraparound at 32 s, blanking active only for condition 7.

## Plumbing

- `ConditionCycle` 1-arg helpers: `upTo: 5` → `upTo: 7` (existing wrap tests updated).
- `wmoBucket`: untouched. `WX_COND=6/7`: works with no changes.
- Shader `switch` + `window_alpha` gain cases 6/7; `default` stays clear.
- `condition_text` stays live/model-bound (demo conditions are visual-only; no label changes).
- Snow pile, cracks, fog erosion, rain outline: untouched.

## Out of scope

- Live wind data / `wx_wind` modifier (rejected with trigger-model A).
- Real tsunami alert feeds (NOAA/GDACS).
- Engine changes of any kind.

## Verification

- Env-forced captures: winds — burst for tremble/jolt/flap + an impact-dent GIF; tsunami —
  full 32 s real-time GIF with the engulf showing blanked UI, plus `WX_AGE` jumps straight to
  crash-phase stills.
- Perf gate re-run on conditions 6/7 (expected cheap: 2D water + short stretched march);
  p50 within the ~17.6 ms pacing baseline tolerance (≤ ~18.5 ms).
- All existing tests + new `Tsunami` tests green; zero `crates/` diff.

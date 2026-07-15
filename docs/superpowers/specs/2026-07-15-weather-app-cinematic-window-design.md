# Weather App Cinematic Sky + Theatrical Window — Design

**Date:** 2026-07-15
**Status:** Approved
**Scope:** `weather/` app only — shader, skin, Swift host. **Zero engine changes.**
**Branch/PR:** continues `weather-app-shader-revamp` / PR #45 (nothing lands until the wow is in).

## Motivation

Two revamp rounds produced technically-better procedural wallpaper with no wow factor. User
direction: **A + C** —

- **A. Cinematic realism** — ray-marched volumetric clouds, real light scattering; the gasp of
  "how is this real."
- **C. The window IS the weather** — theatrical use of the one capability no other weather app
  has (total window replacement): weather that deforms, cracks, buries, and erodes the window
  itself.

Rejected directions: pure stylization (B), incremental polish (D-alone).

## Hard constraints

1. **Zero engine changes** — no diffs under `crates/`.
2. **The shader can never draw over UI text** — the engine composites vello text OVER the
   shader. All "invasion" effects work through window-alpha shaping plus host-coordinated UI
   changes (the host hides rows; the shader shapes alpha). This is load-bearing for §Theatrical.
3. **`shader{}` has no texture bindings** — all volumetric noise is pure-ALU 3D value noise.
   This is the entire perf question; hence the spike.
4. **60 fps hard gate** (probe-measured frame_ready inter-arrival, p50 within pacing baseline
   ≈17.7 ms). Degrade order on pressure: march steps → march-region height → noise octaves.

## Part A — Cinematic core

### Analytic scattering sky (all 6 conditions)

Replaces `mesh_gradient` as the base layer. Each pixel = a view ray into a sky dome (elevation
from `uv.y`, azimuth fixed; sun elevation from `wx_sun`):

- Rayleigh-style zenith→horizon gradient (deep blue up, pale at horizon; indigo night).
- Mie forward-scatter halo hugging the sun direction; sun disc with limb glow.
- Golden-hour banding emerges from the model when the sun is low (replaces hand-tuned
  `sky_grade` horizon logic; `Sky` struct interface is preserved for consumers, derived from
  the new model).
- Night keeps the existing starfield/moon-with-craters/shooting-star, now under the scattering
  model.

### Bounded volumetric clouds (clear / cloud / storm)

True ray-march through a horizontal cloud slab:

- Analytic slab entry/exit per ray; **16–24 steps**, per-pixel dithered start (grain pass hides
  stepping); early-exit when transmittance < ~0.05.
- Density: 3-octave ALU 3D value noise, wind-drifted; coverage/type from condition +
  `intensity`.
- Lighting per step: one short sun-ward density tap → Beer's law + powder term;
  Henyey-Greenstein phase (forward lobe) for **silver linings**.
- **Storm interior flash:** during a strike, a point light at the bolt position illuminates the
  marched volume from inside (distance-falloff added to the per-step light term, gated on the
  strike env). The signature shot.

Per-condition treatment:

| Condition | Volumetrics | Notes |
|---|---|---|
| Clear | Sparse wisps, short 6–8 step march | Sun disc + mie halo dominate; night unchanged |
| Cloud | Broken cumulus field, full march | Silver linings; cloud-break = real coverage gap sweeping through + light shaft |
| Storm | Heavy dark low cell, fast churn, full march | Lit from inside on strikes; bolt/shockwave/edge-jolt carry forward |
| Rain / Snow / Fog | None (2D overcast tint layer) | Keep existing motif systems (streaks/flakes/banks) on the new analytic sky + shared light model |

### GO/NO-GO spike (first task)

Storm volumetric at full res (800×1360): must hold the 60 fps gate AND pass the gasp eyeball.
**NO-GO fallback (written):** faux-lumetric — 4–6 parallax noise slices with derivative-lit
shading (~70 % of the look, guaranteed frame rate). A NO-GO switches Part A's cloud sections to
that approach; everything else in this spec stands.

## Part C — Theatrical window

One generalized `window_alpha(uv, t, cond, intensity)` replaces `silhouette_alpha` +
`corner_alpha`: rounded-rect base mask, deformed on **all four edges** per condition.

| Condition | Deformation |
|---|---|
| Rain | Whole outline undulates (sheeting water; amplitude ∝ intensity, gust-synced); bottom drip streams; moment-gated **droplet detach** — a blob of alpha separates from a bottom corner and falls away |
| Storm | **Window cracks** on strike: 3–5 jagged transparent fracture lines (reuse `bolt_path`, steep amplitude) radiate from the impact for ~0.5 s, then heal. Transient text crossing is accepted theater. Edge jolt + shockwave carry forward |
| Snow | **The pile**: opaque snow mound accumulates on the bottom edge over ~2–3 min (fbm-profiled height field driven by `wx_cond_age`), laps the last daily row → host hides that row (burial). Condition change resets → replayable demo |
| Fog | **Erosion**: edge noise eats inward on all edges, breathing with fog-roll moments; peak = soft-edged ghost window (~30 px), interior/text zones untouched |
| Clear | Gentle wave (restraint is the contrast) |
| Cloud | Top edge takes soft cumulus-profile bumps — silhouette cut by the clouds |

## Host changes (Swift, TDD)

- **New uniform `wx_cond_age`**: seconds since the effective condition last changed (live change
  or override change both reset it). Lock-guarded like other host state; computed on read from
  a stored change timestamp.
- **Snow burial coordination:** a pure, unit-tested `SnowPile.buriedRows(age:) -> Int`
  (0/1/2…) mirrors the shader's pile-height formula thresholds; `WeatherHost.rowCount()`
  subtracts it while the effective condition is snow. Shader and Swift evaluate the same
  formula of the same clock — the row vanishes as the mound covers it.
- No other host-data contract changes.

## Out of scope

- Engine changes of any kind (incl. texture bindings for shader{}, post-composite text warping).
- Layout rework; rain/snow/fog volumetrics; azimuth-accurate sun paths.
- M4 location search.

## Verification

- Spike gate first (60 fps probe + gasp test), then per-condition eyeball loops with burst
  captures (silver linings, interior flash).
- Re-run the 6 × 4 condition/sun matrix.
- New GIFs: storm interior-flash strike + crack · snow burial time-lapse · fog ghost · rain
  outline + droplet detach.
- Frame-time probe across all 6 conditions at the end (60 fps gate).
- `swift test` green including new `wx_cond_age` + `SnowPile` tests; zero `crates/` diff.

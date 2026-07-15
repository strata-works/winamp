# Weather App Shader Revamp — Design

**Date:** 2026-07-15
**Status:** Approved
**Scope:** `weather/` app only — shader (`weather/skins/weather/assets/weather.wgsl`), skin (`weather/skins/weather/skin.lua`), Swift host. **Zero engine changes.**

## Motivation

The M3/redesign shaders (Apple Weather × paper.design mesh-gradient blend, PR #44) set the right
direction, but the scenes read as looping wallpaper: flat depth, metronomic animation, binary
day/night, and visible banding/blowout artifacts. This revamp **refines the current direction**
— it does not replace it. Priorities, in order:

1. **Depth & atmosphere** — scenes read as a *space*, not a flat gradient.
2. **Episodic moments** — irregular events break the loop.
3. **Slow evolution** — the sky tracks the real sun; never the same twice.
4. **Micro-detail & material polish** — banding, blowout, grain, glints.

## Approach

**Shared systems + bespoke motifs** (chosen over in-place upgrades and a full rewrite): build
four global systems every condition consumes, keep the six bespoke condition functions —
upgraded in place and rewired onto the shared systems. Consistency where it matters (light,
grading, event rhythm), distinctness where it matters (motifs).

## Shared system 1 — Sun-elevation palette engine

- **New uniform `wx_sun` ∈ [-1, 1]** replaces binary `wx_is_day`. Continuous solar-elevation
  proxy: 1 = solar noon, 0 = sunrise/sunset, -1 = deep night (midpoint of the night arc).
  Piecewise-linear over the day/night arcs; dawn and dusk are visually identical in v1
  (elevation only, no azimuth).
- **Host computes it on read** in `num("wx_sun")` from `Date()` + today's sunrise/sunset —
  the sky evolves continuously with zero timers.
- **Shader:** a `sky_grade(sun)` function derives one global light state — key-light color
  (horizon gold → noon white → silver moonlight), ambient level, horizon warmth — consumed by
  every condition in place of its hand-rolled `mix(nightColor, dayColor, day)` pairs. A real
  golden-hour band appears around `sun ≈ 0`.
- **Light disc position tracks elevation:** low near the horizon at dawn/dusk, high at noon;
  moon at night.
- **D key:** 4-stop override cycle dawn → noon → dusk → night → live (replaces the binary
  day/night toggle).

## Shared system 2 — Moment scheduler

Generalize `storm_strike`'s hash-gated envelope into a reusable primitive:

```wgsl
// moment(t, rate, prob, channel) -> (env, phase, seed, active)
// Irregular episodic events: hash-gated occurrence at `rate` slots/sec with
// probability `prob`, smooth attack/decay envelope, independent channels.
```

Per-condition moments:

| Condition | Moment(s) |
|---|---|
| Clear (day) | Sun-flare pulse — brief bloom + ray surge |
| Clear (night) | Shooting star — streak with fading tail |
| Cloud | Cloud-break — a gap aligns and a god-ray shaft sweeps through |
| Rain | Wind gust — slant angle + fall speed + streak brightness surge (~3 s) |
| Snow | Flurry — density + rotational swirl surge |
| Storm | Existing strike refactored onto `moment()`; occasional double-strike (second bolt ~0.15 s later, offset x); distant flash (env without bolt) |
| Fog | Fog roll — a dense bank drifts through; visibility drops then recovers |

## Shared system 3 — Depth model

- **Standard far/mid/near parallax pattern** for all condition content (scale/speed/softness
  gradients per plane).
- **Atmospheric perspective:** far layers fade toward the sky palette (one shared mix helper).
- **Vertical depth grade:** subtle luminance/saturation shift down the canvas.

## Shared system 4 — Final grade pass

One post pass after the condition color, before `ui_scrim`:

- **Filmic-ish soft-shoulder tone curve** — structurally fixes the known "additive glows blow
  out to white" failure mode.
- **Subtle vignette.**
- **Blue-noise grain/dither** — kills mesh-gradient banding (highest-leverage premium cue on
  smooth gradients).
- Existing temperature-warmth and season tints fold into this pass.

`ui_scrim` stays on top of the grade.

## Per-condition treatments

Each condition = shared systems wired in + bespoke motif upgrades:

| Cond | Depth (A) | Moments (B) | Polish (D) |
|---|---|---|---|
| **Clear** | 2-layer starfield (far dim/dense, near bright/sparse); horizon glow band; soft halo gradient on the disc | Sun-flare pulse (day); shooting star (night) | Rays stay subtle post-tone-curve; faint crater-noise texture on the moon |
| **Cloud** | Keep 3 planes; sun-side rim lighting on cloud edges; atmospheric fade on far plane | Cloud-break god-ray sweep | Second warp octave so edges billow instead of blobbing |
| **Rain** | New far sheet-rain layer (soft, slow, misty) behind main streaks; near layer with occasional large soft drops | Wind gust | Streak refraction strengthened slightly; pooling ripples get grade-pass sparkle |
| **Snow** | Keep 3 flake layers; near layer bigger/softer with per-flake sinusoidal x-sway | Flurry | Per-flake size jitter; faint ground-glow near the silhouette band |
| **Storm** | Churning cloud planes (2 warped fbm layers); rain sheets in depth | Strike on shared scheduler + double-strike + distant flash | Bolt afterglow decay; shockwave/edge-jolt carried forward unchanged |
| **Fog** | 3 counter-scrolling banks at distinct scales/speeds/blurs; distant content fades hardest | Fog roll | Faint light-diffusion halo where the sun would be |

Carried forward unchanged: bottom-flowing silhouette band (per-condition edges), corner mask,
full-window drag region. Storm's silhouette strike-jolt reads the shared scheduler.

## UI sympathy pass (skin.lua only; layout structure unchanged)

- **Two-tier color system:** primary near-white; secondary ~78 % toward the sky tone; applied
  consistently across hero, hourly, and daily.
- Fix the known low-contrast daily "lo" temps (M3 open minor).
- Spacing nudges: hero-block breathing room, hourly-strip baseline alignment.
- **`ui_scrim` retuned** for the new palettes — softer, more gradient-shaped, so it disappears
  against the richer backgrounds.

## Host changes (Swift, TDD)

- Open-Meteo daily fetch += `sunrise,sunset`; `mock.json` fixture gains matching fields.
- `WeatherModel` stores today's sunrise/sunset.
- Pure `sunElevation(now:sunrise:sunset:) -> Double` in [-1, 1], unit-tested: noon → 1,
  sunrise/sunset → 0, deep night → -1, before-sunrise/after-sunset edge cases.
- `WeatherHost` binds `wx_sun` computed on read (lock-guarded like the other fields).
- `isDayOverride` → `sunOverride`; D key cycles the 4 stops → live.
- `skin.lua` uniforms: `is_day = "wx_is_day"` → `sun = "wx_sun"` in the same commit.

## Performance

Parallax multiplies `fbm` calls — the cost center. Budget rules:

- Far planes drop to 3 octaves (main fbm stays 5).
- Bounded total noise evaluations per pixel (target ≈ a dozen fbm-equivalents).
- Before/after frame-time measurement; **60 fps sustained is the hard gate.**

## Verification

- Shader work is a **creative tuning loop** (launch via `launchctl asuser` → screenshot →
  tune), eyeballed across 6 conditions × 4 sun stops.
- Moments captured as GIFs: gust, flurry, cloud-break, shooting star, double-strike, fog roll.
- Frame-time before/after measurement (60 fps gate).
- Swift gate green: existing 18 tests + new sun-elevation tests.

## Out of scope

- Engine changes of any kind (including post-composite UI-text warping — declined previously).
- Layout rework (hero/hourly/daily structure stays).
- Dawn/dusk azimuth distinction (elevation-only sun in v1).
- M4 location search (separate milestone).

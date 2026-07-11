# Weather App Showcase — Shader Redesign (Apple Weather × paper.design)

> Revision of Milestone 3's shaders. Supersedes the visual look defined in
> `2026-07-11-weather-app-showcase-m3-design.md`; the M3 mechanics (transparent
> alpha-shaped window, `D`/`S`/`→`/`R` presenter keys, the host-data contract, the
> bottom-flowing silhouette) are unchanged and carried forward.

## Motivation

Post-M3 review feedback: the app content reads **soft** and the six condition shaders
are **not imaginative or representative enough** — they lean on generic low-frequency
fbm haze, so conditions don't feel unmistakably like *that* weather, and the UI text
sits on a busy, low-contrast background without legibility treatment.

## Direction

**Apple Weather × paper.design, balanced 50/50.** A flowing **mesh-gradient color field**
(paper.design's signature — organic, ever-shifting, premium color blending) is the
atmospheric *base* of every condition. On top rides **Apple Weather's layered depth,
directional light, and a clear, legible signature motif per condition**. "Balanced"
means the mesh base is always clearly present *and* each condition carries a confident,
instantly-readable identifying element — imaginative **and** representative.

The "sharpness" complaint is **legibility, not resolution**: the render pipeline is
already retina-correct (400×680 canvas → 800×1360 surface at 2× backing scale, displayed
1:1). The fix is contrast/scrim treatment, done in the shader.

## Constraints (carried from M3)

- **Zero engine-crate changes.** Only `weather/skins/weather/assets/weather.wgsl` is
  rewritten, plus a one-line color bump in `weather/skins/weather/skin.lua` (the daily
  "lo" temp color). No edits to `crates/*`, `showcase/*`, or the Swift host.
- **Host-data contract UNCHANGED.** Same uniforms consumed by the shader:
  `u.time`, `u.res`, `u.condition`, `u.is_day`, `u.temp`, `u.intensity`, `u.season`
  (the engine's `shader{}` primitive injects the uniform struct + `VsOut`; the shader
  never declares them). No new keys; `WeatherHost`/`WeatherService` untouched.
- **Presenter keys unchanged:** `→`/`←` condition tour, `D` day/night, `S` season,
  `R` refetch. Overrides still force only their uniform; hero/hourly/daily text stays live.
- **Silhouette unchanged (mechanism):** the bottom-flowing, per-condition, premultiplied-
  alpha edge in `uv.y ∈ ~[0.82, 1.0]` remains the signature and the window's shape.
- **uv orientation:** `uv.y = 0` TOP, `uv.y = 1` BOTTOM. Six conditions
  `0 clear · 1 cloud · 2 rain · 3 snow · 4 storm · 5 fog`. Season `0 winter · 1 spring ·
  2 summer · 3 autumn`.
- **Single GPU fragment pass, all procedural** (no raymarching, no multi-pass) — 60fps
  is never at risk; the current shader barely uses the GPU, so there is ample headroom.

## Architecture — shared foundation, then per-condition layers

All in `weather.wgsl`. The existing `hash21`/`noise2`/`fbm` helpers are kept and extended.

1. **Mesh-gradient base — `mesh_gradient(uv, t, palette) -> vec3`.** ~4–5 color control
   points, each drifting along a slow noise/sinusoidal path, blended with smooth
   inverse-distance-power (or gaussian) weighting into an organic color field. The sample
   coordinate is **domain-warped** by fbm so the field flows rather than sliding rigidly.
   Every condition supplies its own palette (a small set of colors) into this one helper.
   This is the paper.design DNA and the source of the "premium, alive" quality.

2. **A single shared directional light** — a sun/moon position (drifting slightly), reused
   by clear (god-rays), cloud (plane lighting), and storm (flash illumination). One light
   that everything agrees on is what produces the Apple "real depth / real light" cohesion
   across the six looks.

3. **Per-condition composition.** `fs()` dispatches on `i32(u.condition)` (0–5 + default→
   clear) to a per-condition function that: builds its mesh base → stacks atmospheric depth
   (parallax planes / layered fbm) → adds its signature motif → returns color. The
   temperature warm/cool tint and the subtle season tint (mixed low, ~0.08) are applied
   after dispatch, as today.

4. **Legibility scrim — `ui_scrim(uv)`.** Because the shader renders *under* the 2D text
   and the engine has no text-shadow/filled-scrim primitive (adding one would be an engine
   change), legibility is baked into the shader: a soft luminance knock-down behind the UI
   zones — the upper-left hero block, the hourly strip, and the daily column — plus keeping
   the whole upper region calmer and lower-contrast, pushing brightness and drama to
   mid/lower screen and into motion. Text then reads crisply against a quiet ground. The
   one-line `skin.lua` change lifts the daily "lo" color (currently `{190,198,214}`, low
   contrast on a bright day sky) to a brighter value.

5. **Silhouette.** The per-condition bottom alpha edge is retained and harmonized with each
   motif (e.g. rain ripples pooling at the edge, storm's jagged edge). Final output stays
   premultiplied: `return vec4(col * a, a)` with `a = silhouette_alpha(...)`.

## The six condition looks (balanced: mesh base + legible motif)

- **Clear (0)** — warm luminous mesh (day: sky-blue → warm horizon; night: deep indigo
  with drifting, twinkling stars) + a soft sun/moon disc with **volumetric god-rays**
  (radial samples) and a faint heat shimmer.
- **Cloud (1)** — airy mesh + **2–3 parallax fbm cloud layers** drifting at different
  depths/speeds/opacities, shaded by the shared light for volume.
- **Rain (2)** — cool desaturated mesh + **streaks running down like wet glass** (with a
  slight refraction offset of the background) + an overall wet sheen + **ripples pooling**
  at the flowing bottom silhouette.
- **Snow (3)** — soft pale mesh + **3 parallax flake layers** (near: large/soft-blurred;
  far: small/crisp) drifting and swaying + a gentle cool bloom.
- **Storm (4)** — dark, churning, higher-contrast mesh (faster domain warp) + rain sheets
  + **real forked/branching lightning** (procedural bolt, time-gated) with a screen flash
  that briefly illuminates the clouds.
- **Fog (5)** — muted low-contrast mesh + **rolling volumetric fog banks** (layered
  scrolling fbm) that reduce visibility toward a hazy horizon; the softest, no-hard-edge
  silhouette dissolve.

## Day/night & season

Day/night selects each condition's palette (luminous vs. moonlit) and light color via the
existing `u.is_day`; stars appear on clear/less-obscured night skies. Season applies a
subtle temperature/hue bias to the palettes (winter cool → summer warm → autumn amber),
mixed at low strength so it reads as a mood shift, not a filter. Both remain driven by the
`D` and `S` presenter keys and the existing override plumbing.

## Window shape (reference)

The window's visible shape is authored **entirely by the shader**, not macOS: the borderless
`NSWindow` (`isOpaque=false`, `backgroundColor=.clear`, `hasShadow=false`) simply honors
per-pixel alpha, and the shader's premultiplied `alpha` defines what is opaque vs.
see-through. The top and sides are square only because the shader emits `alpha=1` there;
the whole outline is a free shader-side lever if a future revision wants rounded corners or
a different silhouette. This redesign keeps the current outline (square top/sides,
condition-reactive flowing bottom band).

## Performance envelope

Cheap and used freely: multi-octave fbm (≤5 octaves), domain warping, layered/parallax
planes, procedural rain/snow particles (small loops, ≤4 iterations), glow/bloom, radial
god-rays, refraction offsets, time-gated lightning. Avoided/faked: true volumetric
raymarching, heavy multi-pass blur, fluid sim. One full-screen fragment pass; naga-validated
at skin load (launch), as today.

## Verification

Rebuild (`cargo build -p carapace-ffi && swift build`) and launch over a **bright,
contrasting backdrop** so transparency and the mesh colors read true. Screenshot and judge:
all six conditions across **day and night** and a couple of **seasons** — each must be
unmistakably its weather (representative), feel alive and premium (imaginative), keep the
flowing transparent silhouette, and render the hero/hourly/daily **text crisply** (the
legibility goal). Full local gate stays green (`swift test` — the existing 18 tests are
unaffected since the host contract is unchanged).

## Where it lands

On the existing **`weather-app-showcase-m3`** branch, folded into the still-draft,
unreviewed **PR #44**, so Milestone 3 ships with these shaders rather than merging the
weaker version and opening a separate follow-up.

## Out of scope / deferred

- No engine primitive for text shadows / scrims (legibility is shader-baked instead).
- No change to the window outline beyond the existing flowing bottom (rounded corners etc.
  noted as a future shader-side option).
- M4 (location search cutout + geocoding) remains the next milestone, unchanged.

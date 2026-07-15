# Weather Shader Redesign (Apple Weather × paper.design) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Note on verification:** WGSL shaders are not unit-testable. The "test" for each task is (a) **naga validation at skin load** — the app `fatalError`s with the naga/skin error if the WGSL is invalid, so a clean launch that stays alive proves validity — and (b) a **controller visual eyeball** over a bright backdrop (screenshot, judge against the target look, tune constants). Because the creative quality needs a launch→screenshot→tune loop the implementer can't self-assess, tasks are best executed **inline** (executing-plans) with the controller judging each screenshot; the WGSL in each task is a concrete, naga-valid **starting point** to be visually tuned in that task's verification step, not frozen transcription.

**Goal:** Rewrite the six weather-condition shaders as an Apple Weather × paper.design blend — a flowing mesh-gradient base with clear per-condition signature motifs — and fix UI text legibility, all with zero engine/host changes.

**Architecture:** One file, `weather/skins/weather/assets/weather.wgsl`, gains a shared `mesh_gradient()` (paper.design flowing color field, domain-warped fbm), a shared directional `light_pos()`, per-condition palettes (day/night), and a `ui_scrim()` legibility knock-down. Each condition function stacks its signature motif on the mesh base. The existing per-condition `silhouette_alpha()` (the transparent window shape) and premultiplied-alpha output are carried forward unchanged. A one-line `skin.lua` bump lifts the low-contrast daily "lo" color.

**Tech Stack:** WGSL (naga-validated at skin load), the engine's `shader{}` primitive (injects the uniform struct + `VsOut`), Swift 6 host (unchanged), Lua skin.

## Global Constraints

- **Zero engine-crate changes.** Only `weather/skins/weather/assets/weather.wgsl` (rewrite) and `weather/skins/weather/skin.lua` (one-line color bump) change. Do NOT edit `crates/*`, `showcase/*`, or `weather/Sources/**`.
- **Real shader path** is `weather/skins/weather/assets/weather.wgsl` (NOT `weather/skins/weather/weather.wgsl`).
- **Host-data contract UNCHANGED.** The shader consumes exactly `u.time`, `u.res`, `u.condition`, `u.is_day`, `u.temp`, `u.intensity`, `u.season`. It must NOT declare the uniform struct, `@group`/`@binding`, or `VsOut` — the engine's `shader{}` primitive injects them. No new host keys; no Swift changes. The existing 18 Swift tests stay green (untouched).
- **uv orientation:** `uv.y = 0` TOP, `uv.y = 1` BOTTOM. Six conditions `0 clear · 1 cloud · 2 rain · 3 snow · 4 storm · 5 fog` via `switch (i32(u.condition))` with a `default`. Season `0 winter · 1 spring · 2 summer · 3 autumn`.
- **Silhouette unchanged (mechanism):** keep `silhouette_alpha()` and the premultiplied return `vec4(col * a, a)`; the band is `uv.y ∈ [0.82, 1.0]`.
- **Single fragment pass, all procedural** — no raymarching, no multi-pass. Keep fbm ≤ 5 octaves and motif loops ≤ 24 iterations so 60fps holds.
- **Build order:** `cargo build -p carapace-ffi` before `swift build`. Naga validates at launch, not at `swift build`.
- **Launch (controller eyeball):**
  ```bash
  cd /Users/nexus/projects/experiments/winamp/weather
  pkill -f 'arm64-apple-macosx/debug/Weather' 2>/dev/null; sleep 1
  launchctl asuser 501 /bin/zsh -lc 'cd /Users/nexus/projects/experiments/winamp/weather && exec .build/arm64-apple-macosx/debug/Weather' >/tmp/wx.log 2>&1 &
  sleep 5; pgrep -fl 'arm64-apple-macosx/debug/Weather' || echo "DIED — shader failed naga validation"; cat /tmp/wx.log
  ```
  For transparency/color truth, capture over a **bright backdrop** (a solid-magenta PNG in Preview behind the window; see the M3 findings doc for the technique). Drive conditions with `→` (key code 124), day/night with `D` (2), season with `S` (1) via `osascript … key code N` while the Weather process is frontmost (`set frontmost of process "Weather" to true`, then `perform action "AXRaise"`).
- **Base:** branch `weather-app-showcase-m3` (folds into the still-draft, unreviewed PR #44). Never commit to `main`.
- **Git identity:** Daniel Agbemava <danagbemava@gmail.com>. No Claude attribution in commits/PRs.

## File Structure

- `weather/skins/weather/assets/weather.wgsl` — **rewrite (Task 1)** then per-condition motif replacements (Tasks 3–5) and the scrim edit (Task 2). Sections, top to bottom: noise/warp helpers → `mesh_gradient` → `light_pos` → six `*_c` condition functions → `season_tint` → `ui_scrim` → `silhouette_alpha` → `fs`.
- `weather/skins/weather/skin.lua` — **modify (Task 2)**: one line, the daily "lo" color.

---

### Task 1: Foundation — mesh-gradient base, directional light, per-condition palettes, day/night

Replace the whole shader with the new foundation: shared helpers, a flowing 4-point mesh gradient, a shared light, and six condition functions that (for now) render only their **mesh palette** (motifs land in Tasks 3–5). Carry the M3 `silhouette_alpha`, `season_tint`, temp tint, and premultiplied output forward so the window shape and presenter keys keep working.

**Files:**
- Rewrite: `weather/skins/weather/assets/weather.wgsl`

**Interfaces:**
- Consumes: injected `u.*` uniforms + `VsOut` (from the `shader{}` primitive).
- Produces (used by later tasks): `hash21`, `noise2`, `fbm`, `rot`, `warp`, `mesh_gradient(uv,t,c0,c1,c2,c3)`, `light_pos(t)`, `clear_c/cloud_c/rain_c/snow_c/storm_c/fog_c(uv,t,day,intensity)`, `season_tint(season)`, `silhouette_alpha(uv,t,cond,intensity)`, `fs`.

- [ ] **Step 1: Replace the entire file with the foundation shader**

Write to `weather/skins/weather/assets/weather.wgsl`:
```wgsl
// ---- Noise / warp helpers ----
fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i + vec2<f32>(0.0, 0.0));
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0; var amp = 0.5; var q = p;
    for (var k = 0; k < 5; k = k + 1) { v = v + amp * noise2(q); q = q * 2.0; amp = amp * 0.5; }
    return v;
}
fn rot(a: f32) -> mat2x2<f32> {
    let s = sin(a); let c = cos(a);
    return mat2x2<f32>(c, -s, s, c);
}
// fbm domain warp — bends the sample coordinate so gradients flow organically.
fn warp(p: vec2<f32>, t: f32) -> vec2<f32> {
    let q = vec2<f32>(fbm(p + vec2<f32>(0.0, t * 0.05)),
                      fbm(p + vec2<f32>(5.2, 1.3 - t * 0.04)));
    return p + 0.6 * q;
}

// ---- paper.design-style flowing 4-point mesh gradient ----
// Four color anchors near the corners, each drifting on a slow path; blended by
// inverse-distance-power weights over a domain-warped coordinate.
fn mesh_gradient(uv: vec2<f32>, t: f32, c0: vec3<f32>, c1: vec3<f32>, c2: vec3<f32>, c3: vec3<f32>) -> vec3<f32> {
    let w = warp(uv * 1.5, t);
    let p0 = vec2<f32>(0.20 + 0.10 * sin(t * 0.11),        0.20 + 0.10 * cos(t * 0.13));
    let p1 = vec2<f32>(0.80 + 0.10 * sin(t * 0.10 + 1.7),  0.25 + 0.10 * cos(t * 0.09 + 2.1));
    let p2 = vec2<f32>(0.25 + 0.10 * sin(t * 0.08 + 3.1),  0.80 + 0.10 * cos(t * 0.12 + 0.7));
    let p3 = vec2<f32>(0.80 + 0.10 * sin(t * 0.09 + 4.2),  0.80 + 0.10 * cos(t * 0.10 + 1.1));
    let e = 2.0;
    let d0 = 1.0 / (pow(distance(w, p0), e) + 0.03);
    let d1 = 1.0 / (pow(distance(w, p1), e) + 0.03);
    let d2 = 1.0 / (pow(distance(w, p2), e) + 0.03);
    let d3 = 1.0 / (pow(distance(w, p3), e) + 0.03);
    let acc = c0 * d0 + c1 * d1 + c2 * d2 + c3 * d3;
    return acc / (d0 + d1 + d2 + d3);
}

// Shared directional light (sun by day / moon by night), drifting slightly.
fn light_pos(t: f32) -> vec2<f32> { return vec2<f32>(0.72, 0.26 + 0.02 * sin(t * 0.3)); }

// Faint stars for clear/less-obscured night skies.
fn stars(uv: vec2<f32>, t: f32) -> f32 {
    let g = hash21(floor(uv * 120.0));
    let tw = 0.5 + 0.5 * sin(t * 3.0 + g * 40.0);
    return step(0.985, g) * tw;
}

// ---- Condition bases (mesh palette only; motifs added in Tasks 3-5) ----
fn clear_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.16), vec3<f32>(0.26, 0.52, 0.92), day);
    let c1 = mix(vec3<f32>(0.06, 0.08, 0.22), vec3<f32>(0.40, 0.66, 0.97), day);
    let c2 = mix(vec3<f32>(0.10, 0.09, 0.22), vec3<f32>(0.70, 0.83, 0.98), day);
    let c3 = mix(vec3<f32>(0.16, 0.11, 0.20), vec3<f32>(0.98, 0.86, 0.68), day);
    return mesh_gradient(uv, t, c0, c1, c2, c3);
}
fn cloud_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.10, 0.11, 0.16), vec3<f32>(0.55, 0.62, 0.74), day);
    let c1 = mix(vec3<f32>(0.13, 0.14, 0.20), vec3<f32>(0.68, 0.73, 0.82), day);
    let c2 = mix(vec3<f32>(0.16, 0.17, 0.23), vec3<f32>(0.80, 0.83, 0.89), day);
    let c3 = mix(vec3<f32>(0.12, 0.13, 0.18), vec3<f32>(0.60, 0.66, 0.78), day);
    return mesh_gradient(uv, t, c0, c1, c2, c3);
}
fn rain_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.06, 0.09, 0.14), vec3<f32>(0.30, 0.40, 0.52), day);
    let c1 = mix(vec3<f32>(0.08, 0.11, 0.17), vec3<f32>(0.38, 0.48, 0.60), day);
    let c2 = mix(vec3<f32>(0.10, 0.13, 0.19), vec3<f32>(0.46, 0.56, 0.68), day);
    let c3 = mix(vec3<f32>(0.05, 0.08, 0.13), vec3<f32>(0.28, 0.38, 0.50), day);
    return mesh_gradient(uv, t, c0, c1, c2, c3);
}
fn snow_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.16, 0.19, 0.28), vec3<f32>(0.74, 0.80, 0.90), day);
    let c1 = mix(vec3<f32>(0.20, 0.23, 0.32), vec3<f32>(0.82, 0.87, 0.95), day);
    let c2 = mix(vec3<f32>(0.24, 0.27, 0.36), vec3<f32>(0.90, 0.93, 0.99), day);
    let c3 = mix(vec3<f32>(0.18, 0.21, 0.30), vec3<f32>(0.78, 0.84, 0.93), day);
    return mesh_gradient(uv, t, c0, c1, c2, c3);
}
fn storm_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    // Faster, higher-contrast churn (extra warp) + darker palette.
    let w2 = warp(uv * 2.2 + vec2<f32>(t * 0.08, 0.0), t);
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.09), vec3<f32>(0.20, 0.24, 0.32), day);
    let c1 = mix(vec3<f32>(0.07, 0.08, 0.13), vec3<f32>(0.30, 0.34, 0.42), day);
    let c2 = mix(vec3<f32>(0.05, 0.06, 0.11), vec3<f32>(0.24, 0.28, 0.36), day);
    let c3 = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.14, 0.17, 0.24), day);
    return mesh_gradient(w2, t, c0, c1, c2, c3);
}
fn fog_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.16, 0.17, 0.19), vec3<f32>(0.66, 0.68, 0.71), day);
    let c1 = mix(vec3<f32>(0.19, 0.20, 0.22), vec3<f32>(0.74, 0.76, 0.79), day);
    let c2 = mix(vec3<f32>(0.17, 0.18, 0.20), vec3<f32>(0.70, 0.72, 0.75), day);
    let c3 = mix(vec3<f32>(0.14, 0.15, 0.17), vec3<f32>(0.62, 0.64, 0.67), day);
    return mesh_gradient(uv, t, c0, c1, c2, c3);
}

// ---- Subtle season tint (multiplier, mixed at low strength) ----
fn season_tint(season: f32) -> vec3<f32> {
    let s = i32(round(clamp(season, 0.0, 3.0)));
    if (s == 0) { return vec3<f32>(0.86, 0.93, 1.06); }   // winter: cool
    if (s == 1) { return vec3<f32>(0.93, 1.05, 0.95); }   // spring: fresh green
    if (s == 2) { return vec3<f32>(1.08, 1.00, 0.90); }   // summer: warm
    return vec3<f32>(1.08, 0.95, 0.80);                    // autumn: amber
}

// ---- Bottom-flowing silhouette (window alpha). Carried forward from M3. ----
fn silhouette_alpha(uv: vec2<f32>, t: f32, cond: i32, intensity: f32) -> f32 {
    let band_top = 0.82;
    if (uv.y < band_top) { return 1.0; }
    let x = uv.x;
    let b = (uv.y - band_top) / (1.0 - band_top);
    let amp = 0.10 + 0.10 * intensity;
    var edge = 0.4;
    var soft = 0.10;
    if (cond == 0) {
        edge = 0.42 + amp * sin(x * 8.0 + t * 0.8);
    } else if (cond == 1) {
        edge = 0.46 + amp * 0.7 * sin(x * 5.0 + t * 0.5);
    } else if (cond == 2) {
        let drip = fbm(vec2<f32>(x * 12.0, t * 0.6));
        edge = 0.30 + amp * 1.4 * drip;
        soft = 0.05;
    } else if (cond == 3) {
        edge = 0.42 + amp * 0.8 * abs(sin(x * 10.0 + t * 0.3));
        soft = 0.08;
    } else if (cond == 4) {
        let j = fbm(vec2<f32>(x * 20.0 + t * 1.5, t));
        edge = 0.35 + amp * 1.6 * (j - 0.5) * 2.0;
        soft = 0.03;
    } else {
        let n = fbm(vec2<f32>(x * 4.0 + t * 0.2, uv.y * 6.0));
        return clamp(1.0 - b * (0.7 + 0.6 * n), 0.0, 1.0);
    }
    return 1.0 - smoothstep(edge - soft, edge + soft, b);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let day = clamp(u.is_day, 0.0, 1.0);
    let intensity = clamp(u.intensity, 0.0, 1.0);
    let cond = i32(u.condition);
    var col: vec3<f32>;
    switch (cond) {
        case 0: { col = clear_c(uv, t, day, intensity); }
        case 1: { col = cloud_c(uv, t, day, intensity); }
        case 2: { col = rain_c(uv, t, day, intensity); }
        case 3: { col = snow_c(uv, t, day, intensity); }
        case 4: { col = storm_c(uv, t, day, intensity); }
        case 5: { col = fog_c(uv, t, day, intensity); }
        default: { col = clear_c(uv, t, day, intensity); }
    }
    // Warm/cool tint from temperature (raw °C).
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.12;
    // Subtle season tint.
    col = mix(col, col * season_tint(u.season), 0.08);
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    let a = silhouette_alpha(uv, t, cond, intensity);
    return vec4<f32>(col * a, a);
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p carapace-ffi && (cd weather && swift build)`
Expected: `Build complete!` (naga validates at launch, next step).

- [ ] **Step 3: Launch + eyeball (naga gate + base look)**

Launch over a bright backdrop (Global Constraints → Launch). Confirm: process stays alive, clean log (a naga error → fatalError; fix the WGSL). Then verify: all six conditions (`→` tour) show **distinct, flowing mesh-gradient palettes** (not the old fbm haze); the palettes read as the right mood (clear bright/warm, rain cool-blue, storm dark, snow pale, fog grey, cloud airy); **`D`** visibly shifts each between luminous day and dark night; the **transparent bottom silhouette still flows**; text is present. Tune the palette constants / warp amounts until the mesh flow looks premium (paper.design-like) before committing.

- [ ] **Step 4: Commit**

```bash
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -am "feat(weather): shader foundation — flowing mesh-gradient base + shared light + day/night palettes

Claude-Session: https://claude.ai/code/session_01EZrJQopFpwWq2uLNwikoH5"
```
(`-am` is fine: only `weather.wgsl` is dirty.)

---

### Task 2: Legibility — shader-baked UI scrim + skin.lua "lo" color

Make the hero/hourly/daily text read crisply by darkening the shader behind the UI zones, and lift the low-contrast daily "lo" color.

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (add `ui_scrim`, call it in `fs`)
- Modify: `weather/skins/weather/skin.lua` (one line)

**Interfaces:**
- Consumes: `fs` from Task 1.
- Produces: `ui_scrim(uv) -> f32` (a 0..1 luminance multiplier; 1 = untouched).

- [ ] **Step 1: Add the `ui_scrim` helper**

Insert this function just above `silhouette_alpha` in `weather.wgsl`:
```wgsl
// Softly darkens the shader behind the 2D UI so text stays legible. The engine has
// no text-shadow/scrim primitive, so legibility is baked here. Zones (canvas 400x680,
// uv normalized): hero top-left, hourly strip band, daily column. Returns a luminance
// multiplier in [~0.55, 1.0].
fn ui_scrim(uv: vec2<f32>) -> f32 {
    var s = 1.0;
    // Hero block (top-left ~ x<0.62, y<0.30): strongest darkening.
    s = s - 0.42 * smoothstep(0.62, 0.30, uv.x) * smoothstep(0.34, 0.02, uv.y);
    // Hourly strip band (~ y 0.33..0.40, full width): gentle.
    s = s - 0.22 * smoothstep(0.32, 0.36, uv.y) * smoothstep(0.42, 0.38, uv.y);
    // Daily column labels + temps (left third and right third, y 0.45..0.82): gentle.
    let band = smoothstep(0.44, 0.48, uv.y) * smoothstep(0.83, 0.79, uv.y);
    s = s - 0.20 * band * (smoothstep(0.32, 0.06, uv.x) + smoothstep(0.68, 0.95, uv.x));
    return clamp(s, 0.5, 1.0);
}
```

- [ ] **Step 2: Apply the scrim in `fs`**

In `fs`, immediately after the `col = clamp(col, ...)` line and before `let a = silhouette_alpha(...)`, add:
```wgsl
    col = col * ui_scrim(uv);
```

- [ ] **Step 3: Lift the daily "lo" color in skin.lua**

In `weather/skins/weather/skin.lua`, in the daily `list{}` template, change the `lo` row from
`{ bind = "lo",    right = 10,   y = 8, size = 15, halign = "right", color = { r = 190, g = 198, b = 214 } },`
to a brighter, higher-contrast value:
```lua
        { bind = "lo",    right = 10,   y = 8, size = 15, halign = "right", color = { r = 214, g = 222, b = 236 } },
```

- [ ] **Step 4: Build + eyeball (legibility)**

Run: `cargo build -p carapace-ffi && (cd weather && swift build)`; launch over a bright backdrop. Confirm across a bright condition (e.g. clear-day via `→` then `D`): the hero "Accra 25°", the hourly strip, and the daily rows (incl. "lo") all read **crisply** against a visibly-calmer ground; the scrim is subtle (no hard rectangle edges) and doesn't muddy the weather motif in the open areas. Tune the scrim strengths/zone bounds until text is sharp but the scrim is invisible-as-a-shape.

- [ ] **Step 5: Commit**

```bash
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -am "feat(weather): shader-baked UI legibility scrim + lift daily lo color

Claude-Session: https://claude.ai/code/session_01EZrJQopFpwWq2uLNwikoH5"
```

---

### Task 3: Sky conditions — clear (sun/moon + god-rays + stars) & cloud (parallax planes)

Add the signature motifs for the two sky conditions on top of their mesh bases.

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (replace `clear_c` and `cloud_c`; add `god_rays`)

**Interfaces:**
- Consumes: `mesh_gradient`, `light_pos`, `stars`, `fbm` (Task 1).
- Produces: updated `clear_c`, `cloud_c`; new `god_rays(uv, lp, t)`.

- [ ] **Step 1: Add the `god_rays` helper** (place above `clear_c`):
```wgsl
// Radial god-rays: march from the pixel toward the light, accumulating brightness.
fn god_rays(uv: vec2<f32>, lp: vec2<f32>, t: f32) -> f32 {
    var s = uv;
    let stepv = (lp - uv) / 24.0;
    var decay = 1.0;
    var acc = 0.0;
    for (var i = 0; i < 24; i = i + 1) {
        s = s + stepv;
        acc = acc + smoothstep(0.45, 0.0, distance(s, lp)) * decay;
        decay = decay * 0.92;
    }
    return acc / 24.0;
}
```

- [ ] **Step 2: Replace `clear_c`** with the mesh base + sun/moon disc + god-rays + night stars:
```wgsl
fn clear_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.16), vec3<f32>(0.26, 0.52, 0.92), day);
    let c1 = mix(vec3<f32>(0.06, 0.08, 0.22), vec3<f32>(0.40, 0.66, 0.97), day);
    let c2 = mix(vec3<f32>(0.10, 0.09, 0.22), vec3<f32>(0.70, 0.83, 0.98), day);
    let c3 = mix(vec3<f32>(0.16, 0.11, 0.20), vec3<f32>(0.98, 0.86, 0.68), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Night stars.
    col = col + vec3<f32>(0.9, 0.92, 1.0) * stars(uv, t) * (1.0 - day) * 0.8;
    // Sun (day) / moon (night) disc + glow.
    let lp = light_pos(t);
    let d = distance(uv, lp);
    let disc = smoothstep(0.11, 0.075, d);
    let glow = smoothstep(0.55, 0.0, d) * 0.4;
    let discCol = mix(vec3<f32>(0.86, 0.90, 1.0), vec3<f32>(1.0, 0.96, 0.80), day);
    col = col + discCol * (disc + glow);
    // Volumetric god-rays from the light, stronger by day.
    col = col + discCol * god_rays(uv, lp, t) * (0.5 + 0.6 * day);
    return col;
}
```

- [ ] **Step 3: Replace `cloud_c`** with the mesh base + parallax fbm cloud planes lit by the shared light:
```wgsl
fn cloud_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.10, 0.11, 0.16), vec3<f32>(0.55, 0.62, 0.74), day);
    let c1 = mix(vec3<f32>(0.13, 0.14, 0.20), vec3<f32>(0.68, 0.73, 0.82), day);
    let c2 = mix(vec3<f32>(0.16, 0.17, 0.23), vec3<f32>(0.80, 0.83, 0.89), day);
    let c3 = mix(vec3<f32>(0.12, 0.13, 0.18), vec3<f32>(0.60, 0.66, 0.78), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let lp = light_pos(t);
    let litd = mix(vec3<f32>(0.20, 0.22, 0.28), vec3<f32>(0.92, 0.94, 0.98), day);
    let shad = mix(vec3<f32>(0.10, 0.11, 0.15), vec3<f32>(0.52, 0.56, 0.64), day);
    // Three parallax planes at increasing scale/speed/coverage.
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let sc = 2.0 + fk * 1.6;
        let sp = 0.04 + fk * 0.03;
        let n = fbm(uv * vec2<f32>(sc, sc * 0.7) + vec2<f32>(t * sp, fk * 3.1));
        let cover = smoothstep(0.55, 0.85, n) * (0.35 + 0.25 * fk) * (0.6 + 0.5 * intensity);
        // Fake lighting: brighter toward the light side.
        let lit = mix(shad, litd, clamp(0.5 + (lp.x - uv.x) * 0.8, 0.0, 1.0));
        col = mix(col, lit, cover);
    }
    return col;
}
```

- [ ] **Step 4: Build + eyeball (clear & cloud)**

Build; launch over a bright backdrop. `→` to clear: confirm day shows a warm sun with **god-rays** + shimmerless clean sky over the flowing mesh; `D` to night shows a moon + **stars**. `→` to cloud: confirm **layered clouds drift at different depths/speeds** with directional light, over the mesh. Tune disc size/glow, god-ray strength, cloud coverage/scale until each reads clearly and premium.

- [ ] **Step 5: Commit**

```bash
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -am "feat(weather): clear (sun/moon + god-rays + stars) & cloud (parallax planes) motifs

Claude-Session: https://claude.ai/code/session_01EZrJQopFpwWq2uLNwikoH5"
```

---

### Task 4: Precipitation — rain (glass streaks + sheen + pooling) & snow (parallax flakes)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (replace `rain_c` and `snow_c`; add `rain_streaks`, `snow_layer`)

**Interfaces:**
- Consumes: `mesh_gradient`, `fbm`, `hash21` (Task 1).
- Produces: updated `rain_c`, `snow_c`; new `rain_streaks(uv,t,intensity)`, `snow_layer(uv,t,scale,speed,seed)`.

- [ ] **Step 1: Add particle helpers** (place above `rain_c`):
```wgsl
// Vertical rain streaks scrolling down, per-column randomized (glass-run look).
fn rain_streaks(uv: vec2<f32>, t: f32, intensity: f32) -> f32 {
    let cols = 55.0;
    let x = uv.x * cols;
    let col = floor(x);
    let fx = fract(x) - 0.5;
    let speed = 0.7 + hash21(vec2<f32>(col, 1.0)) * 0.9;
    let y = fract(uv.y * 2.2 + t * speed + hash21(vec2<f32>(col, 3.0)));
    let line = smoothstep(0.14, 0.0, abs(fx));
    let head = smoothstep(0.0, 0.18, y) * smoothstep(1.0, 0.5, y);
    return line * head * (0.4 + 0.7 * intensity);
}
// One parallax snow layer: soft round flakes on a drifting grid.
fn snow_layer(uv: vec2<f32>, t: f32, scale: f32, speed: f32, seed: f32) -> f32 {
    let p = uv * scale + vec2<f32>(sin(t * 0.3 + seed) * 0.5, t * speed);
    let g = floor(p);
    let f = fract(p) - 0.5;
    let h = hash21(g + seed);
    return smoothstep(0.14, 0.0, length(f)) * step(0.86, h);
}
```

- [ ] **Step 2: Replace `rain_c`** — cool mesh + refracted streaks + wet sheen + pooling at the bottom:
```wgsl
fn rain_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.06, 0.09, 0.14), vec3<f32>(0.30, 0.40, 0.52), day);
    let c1 = mix(vec3<f32>(0.08, 0.11, 0.17), vec3<f32>(0.38, 0.48, 0.60), day);
    let c2 = mix(vec3<f32>(0.10, 0.13, 0.19), vec3<f32>(0.46, 0.56, 0.68), day);
    let c3 = mix(vec3<f32>(0.05, 0.08, 0.13), vec3<f32>(0.28, 0.38, 0.50), day);
    // Slight refraction: sample the mesh at a streak-perturbed coord for a wet-glass warp.
    let streak = rain_streaks(uv, t, intensity);
    let ruv = uv + vec2<f32>(streak * 0.01, 0.0);
    var col = mesh_gradient(ruv, t, c0, c1, c2, c3);
    // Bright streak highlight.
    col = col + vec3<f32>(0.65, 0.74, 0.88) * streak * 0.3;
    // Overall wet sheen toward the bottom.
    col = col + vec3<f32>(0.10, 0.13, 0.18) * smoothstep(0.4, 1.0, uv.y) * (0.4 + 0.4 * day);
    // Ripples pooling near the silhouette band.
    let pool = smoothstep(0.78, 0.9, uv.y) * (0.5 + 0.5 * sin(uv.x * 40.0 - t * 4.0 + fbm(uv * 8.0) * 6.0));
    col = col + vec3<f32>(0.5, 0.6, 0.75) * pool * 0.12 * (0.5 + intensity);
    return col;
}
```

- [ ] **Step 3: Replace `snow_c`** — pale mesh + 3 parallax flake layers + cool bloom:
```wgsl
fn snow_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.16, 0.19, 0.28), vec3<f32>(0.74, 0.80, 0.90), day);
    let c1 = mix(vec3<f32>(0.20, 0.23, 0.32), vec3<f32>(0.82, 0.87, 0.95), day);
    let c2 = mix(vec3<f32>(0.24, 0.27, 0.36), vec3<f32>(0.90, 0.93, 0.99), day);
    let c3 = mix(vec3<f32>(0.18, 0.21, 0.30), vec3<f32>(0.78, 0.84, 0.93), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Far (small/sharp) -> near (big/soft) parallax flake layers.
    var flakes = 0.0;
    flakes = flakes + snow_layer(uv, t, 22.0, 0.10, 1.0) * 0.6;
    flakes = flakes + snow_layer(uv, t, 15.0, 0.16, 2.0) * 0.8;
    flakes = flakes + snow_layer(uv, t,  9.0, 0.24, 3.0) * 1.0;
    let bloom = mix(0.75, 1.0, day);
    col = col + vec3<f32>(1.0) * flakes * (0.35 + 0.4 * intensity) * bloom;
    return col;
}
```

- [ ] **Step 4: Build + eyeball (rain & snow)**

Build; launch over a bright backdrop. `→` to rain: confirm **streaks run down like glass**, a wet sheen, and ripples pooling at the flowing bottom — over the cool mesh. `→` to snow: confirm **layered flakes at different depths** drift/sway with a soft bloom. Tune streak density/refraction/pool amplitude and flake sizes/counts until each reads clearly.

- [ ] **Step 5: Commit**

```bash
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -am "feat(weather): rain (glass streaks + sheen + pooling) & snow (parallax flakes) motifs

Claude-Session: https://claude.ai/code/session_01EZrJQopFpwWq2uLNwikoH5"
```

---

### Task 5: Drama — storm (forked lightning + rain sheets) & fog (rolling banks)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (replace `storm_c` and `fog_c`; add `lightning`, `fog_banks`)

**Interfaces:**
- Consumes: `mesh_gradient`, `warp`, `fbm`, `hash21`, `rain_streaks` (Task 4) (place these functions after `rain_streaks` so it's in scope).
- Produces: updated `storm_c`, `fog_c`; new `lightning(uv,t)`, `fog_banks(uv,t)`.

- [ ] **Step 1: Add drama helpers** (place above `storm_c`, after `rain_streaks`/`snow_layer` from Task 4):
```wgsl
// Time-gated forked lightning: a noise-perturbed vertical bolt + a brief screen flash.
fn lightning(uv: vec2<f32>, t: f32) -> f32 {
    let seed = floor(t * 0.7);
    let active = step(0.72, hash21(vec2<f32>(seed, 11.0)));
    let life = fract(t * 0.7);
    let env = active * smoothstep(0.0, 0.04, life) * smoothstep(0.55, 0.08, life);
    // Main bolt path perturbed by fbm along y; a fork branches off partway down.
    let bx = 0.32 + 0.4 * hash21(vec2<f32>(seed, 7.0));
    let path = bx + (fbm(vec2<f32>(uv.y * 4.5, seed)) - 0.5) * 0.28;
    let main = smoothstep(0.02, 0.0, abs(uv.x - path)) * step(uv.y, 0.72);
    let fork = smoothstep(0.015, 0.0, abs(uv.x - (path + (uv.y - 0.4) * 0.6)))
             * step(0.4, uv.y) * step(uv.y, 0.66);
    return env * (main + fork * 0.7) + env * 0.12; // bolt + ambient flash
}
// Rolling volumetric fog bands (two counter-scrolling fbm layers).
fn fog_banks(uv: vec2<f32>, t: f32) -> f32 {
    let n1 = fbm(uv * vec2<f32>(3.0, 1.6) + vec2<f32>(t * 0.06, 0.0));
    let n2 = fbm(uv * vec2<f32>(2.0, 1.1) + vec2<f32>(-t * 0.04, 1.7));
    return 0.5 * (n1 + n2);
}
```

- [ ] **Step 2: Replace `storm_c`** — dark churning mesh + rain sheets + forked lightning flash:
```wgsl
fn storm_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let w2 = warp(uv * 2.2 + vec2<f32>(t * 0.08, 0.0), t);
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.09), vec3<f32>(0.20, 0.24, 0.32), day);
    let c1 = mix(vec3<f32>(0.07, 0.08, 0.13), vec3<f32>(0.30, 0.34, 0.42), day);
    let c2 = mix(vec3<f32>(0.05, 0.06, 0.11), vec3<f32>(0.24, 0.28, 0.36), day);
    let c3 = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.14, 0.17, 0.24), day);
    var col = mesh_gradient(w2, t, c0, c1, c2, c3);
    // Driving rain sheets (reuse the streak field, denser & fainter).
    col = col + vec3<f32>(0.35, 0.40, 0.52) * rain_streaks(uv, t, 1.0) * 0.18;
    // Forked lightning + flash illuminating the clouds.
    let f = lightning(uv, t);
    col = col + vec3<f32>(0.85, 0.88, 1.0) * f;
    return col;
}
```

- [ ] **Step 3: Replace `fog_c`** — muted mesh + rolling banks reducing visibility:
```wgsl
fn fog_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let c0 = mix(vec3<f32>(0.16, 0.17, 0.19), vec3<f32>(0.66, 0.68, 0.71), day);
    let c1 = mix(vec3<f32>(0.19, 0.20, 0.22), vec3<f32>(0.74, 0.76, 0.79), day);
    let c2 = mix(vec3<f32>(0.17, 0.18, 0.20), vec3<f32>(0.70, 0.72, 0.75), day);
    let c3 = mix(vec3<f32>(0.14, 0.15, 0.17), vec3<f32>(0.62, 0.64, 0.67), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let fogc = mix(vec3<f32>(0.55, 0.57, 0.60), vec3<f32>(0.86, 0.88, 0.90), day);
    let banks = fog_banks(uv, t);
    // Denser fog low and rolling; reduces visibility toward the horizon.
    let dens = clamp(banks * (0.5 + 0.7 * intensity) + smoothstep(0.3, 1.0, uv.y) * 0.35, 0.0, 0.85);
    col = mix(col, fogc, dens);
    return col;
}
```

- [ ] **Step 4: Build + eyeball (storm & fog)**

Build; launch over a bright backdrop. `→` to storm: confirm dark churning sky, driving rain sheets, and **forked lightning bolts that flash and illuminate** (wait a few seconds for a strike; the gate fires intermittently). `→` to fog: confirm **rolling fog banks** that soften the scene and thicken toward the horizon. Tune lightning frequency/thickness (the `0.72` gate and `0.7` rate) and fog density until each reads clearly and isn't over/under-done.

- [ ] **Step 5: Commit**

```bash
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -am "feat(weather): storm (forked lightning + rain sheets) & fog (rolling banks) motifs

Claude-Session: https://claude.ai/code/session_01EZrJQopFpwWq2uLNwikoH5"
```

---

### Task 6: Full gate + final eyeball + push

**Files:** none (verification + push).

- [ ] **Step 1: Full local gate**

Run:
```bash
cargo build -p carapace-ffi
cd weather && swift build && swift test
```
Expected: dylib built; `Build complete!`; **18/18 Swift tests pass** (unchanged — the host contract wasn't touched).

- [ ] **Step 2: Final combined eyeball**

Launch over a bright backdrop and tour the whole matrix: all six conditions × `D` day/night × a couple of `S` seasons. Confirm the whole set reads as **Apple Weather × paper.design** — flowing mesh base everywhere, each condition unmistakable via its motif (god-rays / parallax clouds / glass rain + pooling / parallax snow / forked lightning / rolling fog), the transparent flowing silhouette intact, text crisp via the scrim, and `text` staying the live weather. Capture representative screenshots (clear-day, rain-night, storm, fog).

- [ ] **Step 3: Push (PR #44 already open)**

```bash
git push origin weather-app-showcase-m3
```
No new PR — the draft PR #44 updates automatically. Optionally add a PR comment noting the shader redesign landed.

---

## Self-Review

**Spec coverage:**
- Apple×paper.design balanced blend → mesh-gradient base (Task 1) + per-condition motifs (Tasks 3–5). ✓
- Six representative looks (god-rays/parallax clouds/glass rain+pool/parallax snow/forked lightning/rolling fog) → Tasks 3–5. ✓
- Shared directional light for cohesion → `light_pos` (Task 1), used by clear/cloud/storm. ✓
- Day/night palettes + stars → Task 1 palettes + Task 3 stars/disc. ✓
- Subtle season tint → `season_tint` carried forward (Task 1). ✓
- Legibility = shader-baked scrim + skin.lua lo bump → Task 2. ✓
- Silhouette + premultiplied alpha unchanged → carried in Task 1, untouched after. ✓
- Zero engine/host changes; only weather.wgsl + one skin.lua line → File Structure + Global Constraints. ✓
- Performance (single pass, ≤5 fbm octaves, ≤24-iter loops) → Global Constraints; god_rays 24, cloud 3, snow 3, streak/flake O(1). ✓
- Lands on m3 branch / PR #44 → Task 6. ✓

**Placeholder scan:** No TBD/TODO. Every code step ships complete, naga-plausible WGSL. Verification steps are visual-tuning loops (inherent to shader work), not placeholders — the code is complete before tuning.

**Type consistency:** All condition fns are `(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32>` in Task 1 and in the Task 3–5 replacements. `mesh_gradient(uv,t,c0,c1,c2,c3)`, `light_pos(t)`, `god_rays(uv,lp,t)`, `rain_streaks(uv,t,intensity)`, `snow_layer(uv,t,scale,speed,seed)`, `lightning(uv,t)`, `fog_banks(uv,t)`, `silhouette_alpha(uv,t,cond,intensity)`, `season_tint(season)`, `ui_scrim(uv)` — signatures are consistent across all call sites. Task 5 helpers reference `rain_streaks` from Task 4 (ordering noted). `fs` dispatch + premultiplied return match M3's contract. Uniform names (`u.time/res/condition/is_day/temp/intensity/season`) unchanged; no struct/VsOut declared.

**WGSL ordering note (for the executor):** WGSL requires a function be declared before use. Keep the file order: helpers → `mesh_gradient`/`light_pos`/`stars` → `god_rays` → condition fns (`clear_c`…`fog_c`, with `rain_streaks`/`snow_layer` before `rain_c`/`snow_c`, and `lightning`/`fog_banks` before `storm_c`/`fog_c`) → `season_tint` → `ui_scrim` → `silhouette_alpha` → `fs`. When a task adds a helper "above" a function, ensure it also sits above every function that calls it.

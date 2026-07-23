# Weather App Cinematic Sky + Theatrical Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the procedural-wallpaper look with an analytic scattering sky + bounded ray-marched volumetric clouds (clear/cloud/storm), and make the window itself theatrical — cracking on strikes, buried by snow, eroded by fog, warped by rain.

**Architecture:** Pure-ALU volumetrics inside the existing `shader{}` fragment (no texture bindings exist): analytic sky dome per view ray, a horizontal cloud slab ray-marched 16–24 dithered steps with 1-tap sun shadowing, storm cell lit from inside during strikes. A single generalized `window_alpha()` replaces `silhouette_alpha`+`corner_alpha` for all-edge deformation. A new `wx_cond_age` uniform (host clock since condition change) drives snow-pile growth in the shader and row burial in the host via one shared threshold.

**Tech Stack:** WGSL (naga-parsed at `carapace_create`), Lua skin, Swift/SwiftPM host, XCTest.

**Spec:** `docs/superpowers/specs/2026-07-15-weather-app-cinematic-window-design.md`

## Global Constraints

- **Zero engine changes** — no diffs under `crates/`. STOP and escalate if a task seems to need one.
- **The shader can never draw over UI text** (engine composites vello text OVER the shader). Invasion = window-alpha shaping + host-coordinated UI changes only.
- **`shader{}` has no texture bindings** — all noise is pure-ALU.
- **60 fps hard gate**: probe p50 must stay at the pacing baseline (~17.7 ms measured pre-rework; the loop sleeps ~16.7 ms, so p50 materially above ~18 ms = shader-bound). Degrade order: march steps → march-region height → noise octaves.
- **Task 1 is GO/NO-GO.** NO-GO fallback (written in spec): faux-lumetric parallax slices; Tasks 4–5 then swap `march_clouds` for the fallback, everything else stands.
- Shader visual constants are **starting points** — every shader task ends in a launch→screenshot→tune eyeball loop; structure (functions, signatures, uniforms, thresholds shared with Swift) must NOT drift.
- Swift work is TDD. `cd weather && swift test` (26 pre-existing tests stay green).
- Branch `weather-app-shader-revamp`, PR #45. Commit identity: `git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit ...`. Never push `main`.
- GUI verify from background shell: `launchctl asuser $(id -u) /bin/zsh -lc "cd <repo>/weather && exec .build/arm64-apple-macosx/debug/Weather"`; keys via osascript (→=124 ←=123 D=2 S=1 R=15); region-screenshot via window bounds; `pkill -f "arm64-apple-macosx/debug/Weather"` to stop. Requires unlocked screen (`ioreg -n Root -d1 -a | grep IOConsoleLocked`).
- Perf probe (temporary, in `SkinView.show`): record `CACurrentMediaTime()` inter-arrival deltas, every 600 frames write p50/p95 to `FileHandle.standardError` (Swift `print` to a redirected stdout never flushes). Remove before final commit.
- WGSL gotchas: `active` is reserved; reversed-edge `smoothstep(hi, lo, x)` works via the clamp formula; parse errors surface as `fatalError` + `carapace_last_error` at launch.

---

### Task 1: Storm volumetric GO/NO-GO spike

Prove pure-ALU ray-marched clouds hold 60 fps at 800×1360 and actually gasp. Work directly in `weather.wgsl` on the storm condition; the spike code IS the foundation (not throwaway) if GO.

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (add 3D noise + march; rewire `storm_c`)
- Modify (temp): `weather/Sources/Weather/SkinView.swift` (perf probe, Global Constraints snippet)

**Interfaces:**
- Produces (Tasks 4–5 rely on): `fn hash13(p: vec3<f32>) -> f32`, `fn noise3(p: vec3<f32>) -> f32`, `fn fbm3d(p: vec3<f32>) -> f32` (3 octaves), `struct CloudParams { coverage: f32, dark: f32, speed: f32, scale: f32, steps: i32 }`, `fn cloud_density(p: vec3<f32>, t: f32, cp: CloudParams) -> f32`, `fn march_clouds(uv: vec2<f32>, rd: vec3<f32>, t: f32, sd: vec3<f32>, key_col: vec3<f32>, amb_col: vec3<f32>, cp: CloudParams, flash_pos: vec3<f32>, flash_amt: f32) -> vec4<f32>` (rgb = premultiplied cloud light, a = opacity), `fn view_ray(uv: vec2<f32>) -> vec3<f32>`, `fn sun_dir(sun: f32) -> vec3<f32>`.

- [ ] **Step 1: Add 3D noise + camera/ray helpers to weather.wgsl** (after the 2D noise block)

```wgsl
// ---- 3D value noise (pure ALU — shader{} has no texture bindings) ----
fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q = q + dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u3 = f * f * (3.0 - 2.0 * f);
    let n000 = hash13(i);
    let n100 = hash13(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash13(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash13(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash13(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash13(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash13(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash13(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(mix(mix(n000, n100, u3.x), mix(n010, n110, u3.x), u3.y),
               mix(mix(n001, n101, u3.x), mix(n011, n111, u3.x), u3.y), u3.z);
}
fn fbm3d(p: vec3<f32>) -> f32 {
    var v = 0.0; var amp = 0.5; var q = p;
    for (var k = 0; k < 3; k = k + 1) { v = v + amp * noise3(q); q = q * 2.15; amp = amp * 0.5; }
    return v;
}

// ---- View camera: each pixel is a ray into a sky dome. Camera near the ground, looking
// forward+up; azimuth spreads across the window width. uv.y=0 is the TOP of the canvas. ----
fn view_ray(uv: vec2<f32>) -> vec3<f32> {
    let el = mix(0.85, 0.02, uv.y);          // radians: top of window looks well up
    let az = (uv.x - 0.5) * 0.9;
    return normalize(vec3<f32>(cos(el) * sin(az), sin(el), cos(el) * cos(az)));
}
fn sun_dir(sun: f32) -> vec3<f32> {
    let el = clamp(sun, -1.0, 1.0) * 1.1;
    let az = 0.32;                            // fixed right-of-center, like the old light_pos
    return normalize(vec3<f32>(cos(el) * sin(az), sin(el), cos(el) * cos(az)));
}
```

- [ ] **Step 2: Add the cloud slab march**

```wgsl
// ---- Bounded volumetric cloud march. Slab y ∈ [1.5, 3.6] world units, camera at y=0.2.
// rgb returned premultiplied by opacity; caller composites: col = sky * (1-a) + rgb. ----
struct CloudParams {
    coverage: f32,   // 0..1 how much of the field is cloud
    dark: f32,       // 0 = white cumulus, 1 = storm-black albedo
    speed: f32,      // wind drift
    scale: f32,      // noise domain scale
    steps: i32,      // march steps (perf knob #1)
}
fn cloud_density(p: vec3<f32>, t: f32, cp: CloudParams) -> f32 {
    let q = p * cp.scale + vec3<f32>(t * cp.speed, 0.0, t * cp.speed * 0.35);
    let base = fbm3d(q);
    // Vertical profile: densest mid-slab, feathered top/bottom.
    let hprof = smoothstep(1.5, 1.9, p.y) * smoothstep(3.6, 2.7, p.y);
    return clamp((base - (1.0 - cp.coverage)) * 2.4, 0.0, 1.0) * hprof;
}
fn march_clouds(uv: vec2<f32>, rd: vec3<f32>, t: f32, sd: vec3<f32>,
                key_col: vec3<f32>, amb_col: vec3<f32>, cp: CloudParams,
                flash_pos: vec3<f32>, flash_amt: f32) -> vec4<f32> {
    if (rd.y < 0.03) { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }
    let ro = vec3<f32>(0.0, 0.2, 0.0);
    let t0 = (1.5 - ro.y) / rd.y;
    let t1 = (3.6 - ro.y) / rd.y;
    let n = cp.steps;
    let dt = (t1 - t0) / f32(n);
    // Per-pixel dithered start (grain pass hides the stepping).
    var tt = t0 + dt * hash21(uv * u.res);
    var trans = 1.0;
    var acc = vec3<f32>(0.0);
    let albedo = mix(vec3<f32>(1.0), vec3<f32>(0.22, 0.24, 0.30), cp.dark);
    let mu = clamp(dot(rd, sd), 0.0, 1.0);
    let phase = 0.55 + 1.1 * pow(mu, 8.0);   // HG-ish forward lobe -> silver linings
    for (var i = 0; i < n; i = i + 1) {
        if (trans < 0.05) { break; }
        let p = ro + rd * tt;
        let dens = cloud_density(p, t, cp);
        if (dens > 0.012) {
            // 1-tap sun shadow (Beer's law) + powder darkening in thick cores.
            let ldens = cloud_density(p + sd * 0.5, t, cp);
            let shadow = exp(-ldens * 2.4);
            let powder = 1.0 - exp(-dens * 4.5);
            var lit = albedo * (key_col * shadow * phase * powder + amb_col * 0.4);
            // Storm interior flash: point light inside the cell during a strike.
            if (flash_amt > 0.001) {
                let fd = p - flash_pos;
                lit = lit + vec3<f32>(0.85, 0.88, 1.0) * flash_amt / (1.0 + dot(fd, fd) * 1.6);
            }
            let a = 1.0 - exp(-dens * dt * 5.5);
            acc = acc + trans * a * lit;
            trans = trans * (1.0 - a);
        }
        tt = tt + dt;
    }
    return vec4<f32>(acc, 1.0 - trans);
}
```

- [ ] **Step 3: Rewire storm_c onto the march (spike wiring)**

Replace `storm_c`'s two `fbm3`-plane churn loop with a march call; keep everything else (mesh base for now, rain sheets, bolts, shockwave):

```wgsl
    // Volumetric storm cell, lit from inside by strikes.
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    let cp = CloudParams(0.85, 0.9, 0.14, 0.55, 22);
    let flash_pos = vec3<f32>((st.y - 0.5) * 5.0, 2.4, 7.0);
    let cl = march_clouds(uv, rd, t, sd, sky.key, mix(vec3<f32>(0.06, 0.07, 0.11), vec3<f32>(0.30, 0.33, 0.42), day), cp, flash_pos, (st.x + st2.x) * 3.0);
    col = col * (1.0 - cl.a) + cl.rgb;
```

(Insert after the mesh_gradient base, replacing the `for (var k = 0; k < 2 ...)` churn-plane loop.)

- [ ] **Step 4: Re-add the perf probe** (Global Constraints snippet) to `SkinView.show`, `swift build`.

- [ ] **Step 5: Measure + eyeball**

Launch; → ×5 to storm; run ≥60 s capturing stderr. Record p50/p95. Burst-capture 30 frames @0.25 s; review: does the cell read as a boiling volumetric mass with silver-lit edges, and does a strike visibly light it from inside?
- **GO** = p50 ≤ ~18 ms AND the eyeball gasps → proceed.
- Perf fail → knobs in order (steps 22→16, slab 3.6→3.0, fbm3d 3→2 octaves), re-measure; if still >18 ms → **NO-GO**: STOP, report numbers, switch Tasks 4–5 to the spec's faux-lumetric fallback after discussing with the user.

- [ ] **Step 6: Remove probe, commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): volumetric storm cell spike — ALU ray-march w/ interior strike flash (GO)"
```
(Include measured p50/p95 numbers in the commit body.)

---

### Task 2: `wx_cond_age` uniform + SnowPile burial (host, TDD)

**Files:**
- Create: `weather/Sources/Weather/SnowPile.swift`
- Create: `weather/Tests/WeatherTests/CondAgeTests.swift`
- Modify: `weather/Sources/Weather/WeatherHost.swift`
- Modify: `weather/skins/weather/skin.lua` (uniform list only)

**Interfaces:**
- Produces: `SnowPile.buryAgeLastRow: Double == 135`, `SnowPile.buriedRows(age: Double) -> Int` (0 below 135, 1 at/above); `WeatherHost.conditionAge(now: Date) -> Double`; uniform `wx_cond_age` (skin name `cond_age`, so the shader reads `u.cond_age`); `rowCount()` subtracts burial while effective condition is snow (3).
- Consumes: existing `WeatherHost` lock/override structure.

- [ ] **Step 1: Failing tests**

`weather/Tests/WeatherTests/CondAgeTests.swift`:

```swift
import XCTest
@testable import Weather

final class CondAgeTests: XCTestCase {
    func testSnowPileThreshold() {
        XCTAssertEqual(SnowPile.buriedRows(age: 0), 0)
        XCTAssertEqual(SnowPile.buriedRows(age: 134.9), 0)
        XCTAssertEqual(SnowPile.buriedRows(age: 135), 1)
        XCTAssertEqual(SnowPile.buriedRows(age: 10_000), 1)
    }

    func testConditionAgeResetsOnOverrideChange() {
        let host = WeatherHost(model: .sample)
        let t0 = Date()
        XCTAssertLessThan(host.conditionAge(now: t0), 1.0)            // fresh host ≈ 0
        // Simulate time passing without changes: age grows.
        XCTAssertEqual(host.conditionAge(now: t0.addingTimeInterval(60)), 60, accuracy: 1.0)
        // Changing the effective condition resets the clock.
        host.conditionOverride = 3
        XCTAssertLessThan(host.conditionAge(now: Date()), 1.0)
        // Setting the SAME override again must NOT reset (no-op change).
        let mid = Date().addingTimeInterval(30)
        host.conditionOverride = 3
        XCTAssertEqual(host.conditionAge(now: mid), 30, accuracy: 1.5)
    }

    func testRowCountBuriesLastRowInSnowOnly() {
        let host = WeatherHost(model: .sample)          // sample condition = 1 (cloud)
        host.conditionOverride = 3                      // snow
        XCTAssertEqual(host.rowCount(now: Date().addingTimeInterval(200)), 6)  // 7 - 1 buried
        XCTAssertEqual(host.rowCount(now: Date().addingTimeInterval(10)), 7)   // too young
        host.conditionOverride = 4                      // storm: no burial
        XCTAssertEqual(host.rowCount(now: Date().addingTimeInterval(200)), 7)
    }

    func testWxCondAgeUniformIsNumeric() {
        let host = WeatherHost(model: .sample)
        let v = host.num("wx_cond_age")
        XCTAssertNotNil(v)
        XCTAssertGreaterThanOrEqual(v!, 0)
    }
}
```

- [ ] **Step 2: Run — expect compile failure** (`cannot find 'SnowPile'`, no `conditionAge`).

- [ ] **Step 3: Implement**

`weather/Sources/Weather/SnowPile.swift`:

```swift
import Foundation

/// Snow-pile burial coordination. The SHADER grows an opaque snow mound from
/// `wx_cond_age`; the HOST hides daily rows the mound has lapped. Both sides evaluate the
/// same threshold so the row vanishes exactly as the mound covers its position.
/// weather.wgsl `pile_height` must stay in sync with `buryAgeLastRow` (see Task 6).
enum SnowPile {
    /// Age (seconds in snow) at which the pile laps the last daily row.
    static let buryAgeLastRow: Double = 135
    static func buriedRows(age: Double) -> Int { age >= buryAgeLastRow ? 1 : 0 }
}
```

`WeatherHost.swift` — add state + logic:

```swift
    private var _condChangedAt = Date()
```

Track the effective condition: add a private helper and call it from BOTH the `conditionOverride` setter and the `model` setter (inside the lock, before assignment):

```swift
    /// Reset the condition-age clock when the EFFECTIVE condition value changes.
    /// Callers must hold `lock`.
    private func noteEffectiveCondition(from old: Double, to new: Double) {
        if old != new { _condChangedAt = Date() }
    }
```

In `var conditionOverride` setter (replace body):

```swift
        set {
            lock.lock()
            let old = _conditionOverride ?? _model.condition
            let new = newValue ?? _model.condition
            noteEffectiveCondition(from: old, to: new)
            _conditionOverride = newValue
            lock.unlock()
        }
```

In `var model` setter (replace body):

```swift
        set {
            lock.lock()
            let old = _conditionOverride ?? _model.condition
            let new = _conditionOverride ?? newValue.condition
            noteEffectiveCondition(from: old, to: new)
            _model = newValue
            lock.unlock()
        }
```

Public age + row count:

```swift
    /// Seconds since the effective condition last changed. `now` injected for testability.
    func conditionAge(now: Date = Date()) -> Double {
        lock.lock(); defer { lock.unlock() }
        return now.timeIntervalSince(_condChangedAt)
    }

    func rowCount(now: Date = Date()) -> Int {
        let m = model
        let cond = conditionOverride ?? m.condition
        let buried = cond == 3 ? SnowPile.buriedRows(age: conditionAge(now: now)) : 0
        return max(0, m.days.count - buried)
    }
```

(Delete the old `func rowCount() -> Int { model.days.count }`; the default argument keeps the vtable call site `rowCount()` compiling unchanged.) In `num(_:)` add:

```swift
        case "wx_cond_age": return conditionAge()
```

`skin.lua` uniforms line gains `cond_age = "wx_cond_age"`:

```lua
        uniforms = { condition = "wx_condition", sun = "wx_sun", cond_age = "wx_cond_age",
                     temp = "wx_temp", intensity = "wx_intensity", season = "wx_season" } }
```

(An unused `u.cond_age` struct field is valid WGSL — the shader starts consuming it in Task 6.)

- [ ] **Step 4: Run tests — all green** (`swift test`: 26 + 4 new = 30). Launch once to confirm the skin still parses with the new uniform.

- [ ] **Step 5: Commit**

```bash
git add weather/Sources/Weather/SnowPile.swift weather/Sources/Weather/WeatherHost.swift weather/Tests/WeatherTests/CondAgeTests.swift weather/skins/weather/skin.lua
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): wx_cond_age uniform + SnowPile row-burial coordination (TDD)"
```

---

### Task 3: Analytic scattering sky (all six conditions)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl`

**Interfaces:**
- Consumes: `view_ray`, `sun_dir` (Task 1); `Sky`/`sky_grade` (existing — unchanged).
- Produces: `fn sky_dome(rd: vec3<f32>, sd: vec3<f32>, sun: f32) -> vec3<f32>` — the base field for every condition (replaces `mesh_gradient` in all six `*_c` functions; `mesh_gradient`/`warp` get deleted once no caller remains).

- [ ] **Step 1: Add sky_dome**

```wgsl
// ---- Analytic scattering sky: Rayleigh-ish gradient + mie halo + sun disc.
// The base field for every condition (replaces the mesh-gradient wallpaper). ----
fn sky_dome(rd: vec3<f32>, sd: vec3<f32>, sun: f32) -> vec3<f32> {
    let day = smoothstep(-0.12, 0.35, sun);
    let up = clamp(rd.y, 0.0, 1.0);
    // Zenith -> horizon gradients, day and night.
    let day_col = mix(vec3<f32>(0.58, 0.72, 0.94), vec3<f32>(0.10, 0.34, 0.80), pow(up, 0.55));
    let ngt_col = mix(vec3<f32>(0.05, 0.05, 0.13), vec3<f32>(0.012, 0.018, 0.055), pow(up, 0.7));
    var col = mix(ngt_col, day_col, day);
    // Golden hour: warm band hugging the horizon, strongest when the sun is low.
    let golden = (1.0 - smoothstep(0.0, 0.4, abs(sun))) * exp(-max(rd.y, 0.0) * 5.0);
    col = mix(col, vec3<f32>(1.0, 0.52, 0.24), golden * 0.55);
    // Mie forward-scatter halo around the sun + the disc itself (tonemap shoulders it).
    let mu = clamp(dot(rd, sd), 0.0, 1.0);
    col = col + vec3<f32>(1.0, 0.82, 0.55) * pow(mu, 32.0) * mix(0.06, 0.30, day);
    col = col + vec3<f32>(1.0, 0.95, 0.85) * smoothstep(0.99930, 0.99985, mu) * day * 2.0;
    return col;
}
```

- [ ] **Step 2: Rewire all six condition bases**

In each `*_c` function, replace the `let c0..c3 = mix(...)` + `mesh_gradient(...)` base with:

```wgsl
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    var col = sky_dome(rd, sd, u.sun);
```

then apply the condition's mood on top (starting values; tune in the loop):
- `clear_c`: nothing extra (the dome IS the scene); keep stars/moon/shooting-star/halo/god-rays. **Delete the screen-space day sun disc** (dome draws it); keep the night moon disc via `light_pos` as today.
- `cloud_c`: `col = mix(col, col * vec3<f32>(0.9, 0.92, 0.95), 0.35 * intensity);` (slight grey-down; the real clouds arrive in Task 4).
- `rain_c`: `col = col * mix(vec3<f32>(1.0), vec3<f32>(0.45, 0.52, 0.62), 0.75) + vec3<f32>(0.02, 0.03, 0.05);` — overcast wash; keep all motifs (streaks/sheen/pool). Storm similarly darker: `col * vec3<f32>(0.30, 0.32, 0.38)`.
- `snow_c`: `col = mix(col, vec3<f32>(0.78, 0.82, 0.90), 0.55 * mix(0.4, 1.0, day));` bright overcast; keep flakes/glow.
- `fog_c`: keep the fogc mix logic, but its base is now the dome.
- `storm_c`: dome base + the Task-1 march already composites over `col`; delete the leftover mesh base + `w2` warp.
- Rain keeps its refraction: sample the dome at `view_ray(ruv)` instead of `mesh_gradient(ruv, ...)`.

Delete `mesh_gradient` and `warp` once callers are gone (`grep -n "mesh_gradient\|warp(" weather.wgsl` must return nothing).

- [ ] **Step 3: Eyeball loop**

Launch; all 6 conditions × D stops. Checklist: clear noon reads as a *physical* sky (deep zenith → pale horizon), golden hour is a model output not a tint, sun halo hugs the disc, night sky deep and clean under stars; no condition regressed to grey mush. Tune gradient anchors/mie strength.

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): analytic scattering sky replaces mesh-gradient base across all conditions"
```

---

### Task 4: Volumetric cloud + clear wisps

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (`cloud_c`, `clear_c`)

**Interfaces:**
- Consumes: `march_clouds`, `CloudParams`, `view_ray`, `sun_dir`, `sky_dome`, `moment`, `god_rays`.
- Produces: nothing new.

- [ ] **Step 1: cloud_c gets the full march**

Replace the Task-3 grey-down + the old 3-plane parallax loop entirely:

```wgsl
fn cloud_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    var col = sky_dome(rd, sd, u.sun);
    // Cloud-break moment: a real coverage gap sweeps through (drives coverage down),
    // letting a god-ray shaft cross the scene.
    let mb = moment(t, 0.05, 0.6, 9.0);
    let cover = clamp(0.45 + 0.35 * intensity - mb.x * 0.25, 0.05, 0.9);
    let cp = CloudParams(cover, 0.12, 0.05, 0.42, 20);
    let amb = mix(vec3<f32>(0.10, 0.11, 0.17), vec3<f32>(0.55, 0.62, 0.75), day);
    let cl = march_clouds(uv, rd, t, sd, sky.key, amb, cp, vec3<f32>(0.0), 0.0);
    col = col * (1.0 - cl.a) + cl.rgb;
    col = col + sky.key * god_rays(uv, vec2<f32>(0.2 + 0.6 * mb.y, 0.18), t) * mb.x * 0.28 * day;
    return col;
}
```

- [ ] **Step 2: clear_c gets sparse wisps**

After the dome base in `clear_c` (rd/sd already in scope from Task 3's base), add a short cheap march:

```wgsl
    // Sparse fair-weather wisps: short march, low coverage.
    let cpw = CloudParams(0.18 + intensity * 0.3, 0.05, 0.03, 0.5, 8);
    let clw = march_clouds(uv, rd, t, sd, sky.key, mix(vec3<f32>(0.08, 0.09, 0.15), vec3<f32>(0.6, 0.68, 0.8), day), cpw, vec3<f32>(0.0, 0.0, 0.0), 0.0);
    col = col * (1.0 - clw.a) + clw.rgb;
```

(Place BEFORE stars/moon so night celestial bodies draw over thin wisps.)

- [ ] **Step 3: Eyeball loop**

Cloud noon: cumulus with bright sun-side rims and shadowed undersides; slow believable drift; a cloud-break every ~30 s visibly opens the field + shaft. Clear: wisps read as depth, sky still dominates. Night versions stay moody (key = moonlight). Tune coverage/scale/phase.

- [ ] **Step 4: Perf spot-check** — probe back in temporarily OR trust Task 7's full gate if frame feel is smooth; if cloud_c chugs, steps 20→16.

- [ ] **Step 5: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): volumetric cumulus for cloud + clear wisps, real cloud-break coverage gap"
```

---

### Task 5: Storm cinematic completion

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (`storm_c`)

**Interfaces:**
- Consumes: Task-1 march wiring (already in `storm_c`), `storm_strike`/`storm_strike2`/`lightning`, `rain_streaks`, `sky_dome`.
- Produces: nothing new.

- [ ] **Step 1: Tighten the composition**

Order inside `storm_c` (delete remnants that duplicate): dome base (very dark: `col * vec3<f32>(0.30, 0.32, 0.38)` from Task 3) → volumetric cell (Task 1 wiring; verify `cp.dark = 0.9`, coverage 0.85) → rain sheets ×2 → shockwave ring + ambient flashes → bolts (primary, double, afterglow). Distant-flash moment `df` also feeds `flash_amt`: `(st.x + st2.x) * 3.0 + df.x * 0.8` so even boltless sheet flashes glow inside the cell.

- [ ] **Step 2: Eyeball loop (burst captures)**

30-frame bursts: cell boils; strike lights the volume from INSIDE (bright core in the cloud mass around the bolt, falling off with distance); double-strikes read; silhouette jolt still fires. The gasp test: show the best frame — would a stranger say "wait, that's a live shader?"

- [ ] **Step 3: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): cinematic storm — volumetric cell interior-lit by strikes"
```

---

### Task 6: Theatrical window_alpha

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (new `window_alpha` + `pile_height`; delete `silhouette_alpha` + `corner_alpha`; `fs()` + snow pile color)

**Interfaces:**
- Consumes: `bolt_path`, `storm_strike`, `moment`, `fbm`, `u.cond_age` (Task 2).
- Produces: `fn pile_height(x: f32, age: f32) -> f32`; `fn window_alpha(uv: vec2<f32>, t: f32, cond: i32, intensity: f32) -> f32`. **Sync contract:** pile laps the last daily row at age 135 s = `SnowPile.buryAgeLastRow` (row-7 bottom ≈ uv.y 0.809; mean pile height must cross 0.191 at age 135).

- [ ] **Step 1: pile_height (shared-threshold formula)**

```wgsl
// Snow pile height in uv units at column x. Grows linearly to full size over 150s.
// SYNC: mean height (0.21 * growth) crosses the last daily row's bottom (uv 0.809,
// i.e. height 0.191) at age ≈ 135s == SnowPile.buryAgeLastRow in Swift. Change together.
fn pile_height(x: f32, age: f32) -> f32 {
    let growth = clamp(age / 150.0, 0.0, 1.0);
    return growth * (0.16 + 0.10 * fbm(vec2<f32>(x * 3.5, 7.7)));
}
```

- [ ] **Step 2: window_alpha**

```wgsl
// ---- Theatrical window mask: rounded-rect base, deformed per condition on ALL edges.
// Replaces silhouette_alpha + corner_alpha. ----
fn base_mask(uv: vec2<f32>, inset: f32) -> f32 {
    let asp = u.res.y / u.res.x;
    let p = (uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(1.0, asp);
    let b = vec2<f32>(0.5 - inset, (0.5 - inset) * asp);
    let r = 0.065;
    let q = abs(p) - b + vec2<f32>(r, r);
    let sd = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    return smoothstep(0.006, -0.006, sd);
}
fn window_alpha(uv: vec2<f32>, t: f32, cond: i32, intensity: f32) -> f32 {
    var a = base_mask(uv, 0.0);
    let x = uv.x;
    if (cond == 0) {
        // Clear: gentle bottom wave (restraint is the contrast).
        let b = smoothstep(0.86, 1.0, uv.y);
        a = a * (1.0 - smoothstep(0.42 + 0.08 * sin(x * 8.0 + t * 0.8), 1.0, b));
    } else if (cond == 1) {
        // Cloud: TOP edge takes soft cumulus-profile bumps.
        let bump = fbm(vec2<f32>(x * 5.0 + t * 0.05, 3.3)) * 0.045;
        a = a * smoothstep(0.0, 0.012, uv.y - (0.012 + bump) * smoothstep(0.2, 0.0, uv.y));
    } else if (cond == 2) {
        // Rain: whole outline undulates (sheeting water), drips at the bottom,
        // moment-gated droplet detaching from the bottom-right corner.
        let g = moment(t, 0.18, 0.45, 10.0);
        let amp = (0.006 + 0.010 * intensity) * (1.0 + g.x);
        let wob = fbm(vec2<f32>(uv.y * 9.0 + t * 1.2, x * 9.0 - t)) - 0.5;
        a = a * base_mask(uv + vec2<f32>(wob, wob) * amp, -0.004);
        let drip = fbm(vec2<f32>(x * 12.0, t * 0.6));
        a = a * (1.0 - smoothstep(0.30 + 0.24 * drip, 1.0, smoothstep(0.85, 1.0, uv.y) * 1.4));
        let md = moment(t, 0.10, 0.5, 14.0);
        if (md.w > 0.5) {
            let c = vec2<f32>(0.88, 0.965 + md.y * 0.08);
            let asp2 = u.res.y / u.res.x;
            let dd = length((uv - c) * vec2<f32>(1.0, asp2));
            a = max(a, smoothstep(0.016, 0.010, dd) * (1.0 - md.y));   // detaching blob
        }
    } else if (cond == 3) {
        // Snow: soft scalloped bottom + the accumulating pile (alpha=1 inside the mound).
        let b = (uv.y - 0.86) / 0.14;
        a = a * (1.0 - smoothstep(0.35 + 0.12 * abs(sin(x * 10.0 + t * 0.3)), 1.2, max(b, 0.0)));
        let ph = pile_height(x, u.cond_age);
        if (uv.y > 1.0 - ph) { a = base_mask(uv, 0.0); }   // pile restores full mask alpha
    } else if (cond == 4) {
        // Storm: jagged churned bottom + strike edge-jolt + WINDOW CRACKS from the impact.
        let st = storm_strike(t);
        let b = (uv.y - 0.84) / 0.16;
        let j = fbm(vec2<f32>(x * 14.0 + t * 1.5, t));
        var edge = 0.38 + 0.22 * (j - 0.5) * 2.0;
        edge = edge - st.x * smoothstep(0.24, 0.0, abs(x - st.y)) * 0.42;
        a = a * (1.0 - smoothstep(edge, edge + 0.06, max(b, 0.0)));
        // Cracks: 4 jagged transparent fractures radiating from the impact, healing with the
        // strike envelope (~0.7s visible).
        let crack_env = st.x;
        if (crack_env > 0.02) {
            let asp2 = u.res.y / u.res.x;
            let impact = vec2<f32>(st.y, 0.70);
            for (var k = 0; k < 4; k = k + 1) {
                let fk = f32(k);
                let ang = (hash21(vec2<f32>(st.w, 30.0 + fk)) - 0.5) * 2.6 - 1.57;
                let dirv = vec2<f32>(cos(ang), sin(ang) / asp2);
                let rel = uv - impact;
                let along = dot(rel, normalize(dirv));
                let across = abs(rel.x * normalize(dirv).y - rel.y * normalize(dirv).x);
                let jag = (fbm(vec2<f32>(along * 18.0, st.w + fk)) - 0.5) * 0.02;
                let reach = 0.05 + 0.30 * hash21(vec2<f32>(st.w, 40.0 + fk));
                let on_line = smoothstep(0.0035, 0.0012, abs(across + jag)) * step(0.0, along) * step(along, reach) * (1.0 - along / reach);
                a = a * (1.0 - on_line * crack_env);
            }
        }
    } else {
        // Fog: erosion — edge noise eats inward on ALL edges; peak = ghost window.
        let mr = moment(t, 0.06, 0.5, 13.0);
        let breathe = 0.5 + 0.5 * sin(t * 0.15) + mr.x;
        let edge_d = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
        let eat = (0.02 + 0.030 * breathe) * fbm(vec2<f32>(uv.x * 6.0 + t * 0.1, uv.y * 6.0 - t * 0.07));
        a = a * smoothstep(0.0, 0.02, edge_d - eat);
    }
    return clamp(a, 0.0, 1.0);
}
```

- [ ] **Step 3: fs() wiring + pile color**

Two edits in `fs()`:

1. Immediately AFTER the condition `switch` (before `depth_grade`/`grade`, so the pile is
   graded like scenery), insert:

```wgsl
    // Snow pile is drawn scenery: bright mound with faint sparkle, graded with the scene.
    if (cond == 3) {
        let ph = pile_height(uv.x, u.cond_age);
        if (uv.y > 1.0 - ph) {
            let depth_in = (uv.y - (1.0 - ph)) / max(ph, 0.001);
            col = mix(vec3<f32>(0.92, 0.94, 0.99), vec3<f32>(0.70, 0.75, 0.86), depth_in);
            col = col + vec3<f32>(1.0) * step(0.985, hash21(floor(uv * u.res / 3.0))) * 0.25;
        }
    }
```

2. Replace the final `silhouette_alpha(...) * corner_alpha(...)` line with:

```wgsl
    let a = window_alpha(uv, t, cond, intensity);
    return vec4<f32>(col * a, a);
```

Delete `silhouette_alpha` and `corner_alpha` (grep: no remaining references).

- [ ] **Step 4: Eyeball loop per condition**

- Rain: outline visibly sheets/wobbles; drips; catch a droplet detach (burst capture).
- Storm: burst-capture a strike — cracks radiate + heal in ~0.5 s; edge jolt intact.
- Snow: set condition to snow, wait/observe growth (tip: temporarily test with `u.cond_age * 10.0` then REVERT); at ~135 s the mound covers the last row's position AND the row text vanishes (host burial — verify visually).
- Fog: edges erode and breathe; peak roll = ghost window; interior text zone untouched.
- Clear/cloud: wave + cumulus-cut top edge read as intentional.

- [ ] **Step 5: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): theatrical window_alpha — cracks, snow-pile burial, fog erosion, rain outline + droplet"
```

---

### Task 7: Full verification + PR update

**Files:**
- Modify (temp): `weather/Sources/Weather/SkinView.swift` (probe; removed again)

- [ ] **Step 1: Perf gate** — probe in, `swift build`, measure all 6 conditions ≥2 windows each. Every condition p50 ≤ ~18 ms (pacing baseline). Fail → degrade knobs on the offending condition, re-measure, note in commit. Remove probe (`git diff` on SkinView must be empty).

- [ ] **Step 2: Matrix** — 6 × 4 sun stops, review for coherence + text legibility (worst case: snow noon).

- [ ] **Step 3: GIFs** — storm interior-flash strike + crack · snow burial time-lapse (record ≥150 s, then speed 10× via `ffmpeg -vf "setpts=PTS/10,fps=12,scale=400:-1..."`) · fog ghost breathing · rain outline + droplet detach. Send to the user.

- [ ] **Step 4: Gate** — `swift test` 30/30 green; `git diff main --stat | grep crates` empty; `git status` clean.

- [ ] **Step 5: Commit final tuning, push, update PR #45** — comment summarizing the cinematic rework + refreshed description ("shaders-as-skin weather app with volumetric skies and a window the weather can break"). No Claude-attribution footer.

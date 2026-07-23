# Tsunami + High Winds Demo Conditions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Two demo-only conditions on the → tour — high winds (gale + buffeting/flapping/dented window) and tsunami (32 s arc that engulfs the window and drowns the forecast UI).

**Architecture:** Extends the existing per-condition switch: shader gains `wind_c`/`tsunami_c` + `window_alpha` cases 6/7; the tsunami cycle runs off `u.cond_age` so a pure Swift `Tsunami` enum computes the identical phase and blanks all UI strings during the engulf (the snow-burial sync pattern). Live data never selects 6/7 (`wmoBucket` untouched); `ConditionCycle` tour extends to 7. All water/debris is cheap 2D — no new ray-march passes.

**Tech Stack:** WGSL, Swift/SwiftPM, XCTest.

**Spec:** `docs/superpowers/specs/2026-07-16-weather-app-tsunami-winds-design.md`

## Global Constraints

- **Zero engine changes** — no diffs under `crates/`.
- Shader can never draw over UI text — the engulf works by the HOST blanking strings (empty strings skip rendering in `scene.rs`).
- **Perf gate:** conditions 6/7 p50 ≤ ~18.5 ms (17.6 ms pacing baseline) via the temp SkinView probe (stderr; remove before final commit).
- Shader constants are starting points (eyeball loop tunes); structure + the shared 32 s / [0.60, 0.74] engulf thresholds must NOT drift independently on either side.
- Swift TDD; `cd weather && swift test` (30 existing tests stay green, some cycle tests updated by design).
- Branch `weather-app-shader-revamp` / PR #45. Commit identity `Daniel Agbemava <danagbemava@gmail.com>`; never push main.
- Verification via env launches: `WX_COND=6/7 WX_SUN=… WX_AGE=… WX_POS='1080,60'`, capture region `1080,60,400,680` (confirm via the `WX_FRAME` stderr line). `WX_SHY=1` when the user is active.
- **`window_alpha` structural hazard:** the current final `else` branch is FOG — conditions 6/7 would fall into fog erosion. The fog branch must become an explicit `else if (cond == 5)` before adding 6/7.

---

### Task 1: Host — Tsunami sync + tour extension (TDD)

**Files:**
- Create: `weather/Sources/Weather/Tsunami.swift`
- Create: `weather/Tests/WeatherTests/TsunamiTests.swift`
- Modify: `weather/Sources/Weather/WeatherHost.swift` (str/rowCount blanking)
- Modify: `weather/Sources/Weather/ConditionCycle.swift` (1-arg helpers upTo 7)
- Modify: `weather/Tests/WeatherTests/ConditionOverrideTests.swift` (wrap-at-7 updates)

**Interfaces:**
- Produces: `Tsunami.period == 32`, `.engulfStart == 0.60`, `.engulfEnd == 0.74`, `Tsunami.phase(age:) -> Double` in [0,1), `Tsunami.isEngulfed(age:) -> Bool`; `WeatherHost.str`/`rowCount` blank while effective condition is 7 and engulfed; `ConditionCycle.next(nil) … next(7) == nil`.
- Consumes: `conditionAge(now:)`, `backdateConditionChange` (existing).

- [ ] **Step 1: Failing tests**

`weather/Tests/WeatherTests/TsunamiTests.swift`:

```swift
import XCTest
@testable import Weather

final class TsunamiTests: XCTestCase {
    func testPhaseWrapsAtPeriod() {
        XCTAssertEqual(Tsunami.phase(age: 0), 0, accuracy: 1e-9)
        XCTAssertEqual(Tsunami.phase(age: 16), 0.5, accuracy: 1e-9)
        XCTAssertEqual(Tsunami.phase(age: 32), 0, accuracy: 1e-9)
        XCTAssertEqual(Tsunami.phase(age: 48), 0.5, accuracy: 1e-9)
    }

    func testEngulfWindow() {
        XCTAssertFalse(Tsunami.isEngulfed(age: 0.60 * 32 - 0.1))
        XCTAssertTrue(Tsunami.isEngulfed(age: 0.60 * 32 + 0.1))
        XCTAssertTrue(Tsunami.isEngulfed(age: 0.74 * 32 - 0.1))
        XCTAssertFalse(Tsunami.isEngulfed(age: 0.74 * 32 + 0.1))
        XCTAssertTrue(Tsunami.isEngulfed(age: 32 + 0.60 * 32 + 0.1))   // second cycle
    }

    func testEngulfBlanksAllUIOnlyForTsunami() {
        let host = WeatherHost(model: .sample)
        host.conditionOverride = 7
        host.backdateConditionChange(seconds: 0.65 * 32)     // mid-engulf
        XCTAssertEqual(host.str("location"), "")
        XCTAssertEqual(host.str("temp_now"), "")
        XCTAssertEqual(host.str("wx_hour_0_time"), "")
        XCTAssertEqual(host.rowCount(), 0)
        host.backdateConditionChange(seconds: 0.30 * 32)     // calm phase
        XCTAssertEqual(host.str("location"), WeatherModel.sample.location)
        XCTAssertEqual(host.rowCount(), 7)
        host.conditionOverride = 4                            // storm never blanks
        host.backdateConditionChange(seconds: 0.65 * 32)
        XCTAssertEqual(host.str("location"), WeatherModel.sample.location)
    }

    func testTourReachesDemoConditions() {
        XCTAssertEqual(ConditionCycle.next(5), 6)
        XCTAssertEqual(ConditionCycle.next(6), 7)
        XCTAssertNil(ConditionCycle.next(7))                  // 7 -> live
        XCTAssertEqual(ConditionCycle.prev(nil), 7)
        XCTAssertEqual(ConditionCycle.prev(7), 6)
    }
}
```

- [ ] **Step 2: Run — expect compile failure** (`cannot find 'Tsunami'`).

- [ ] **Step 3: Implement**

`weather/Sources/Weather/Tsunami.swift`:

```swift
import Foundation

/// Tsunami demo-cycle coordination. The SHADER renders the 32 s arc from `wx_cond_age`;
/// the HOST computes the identical phase from its own clock and blanks the entire UI while
/// the window is engulfed. weather.wgsl `tsunami_phase`/engulf constants must stay in sync.
enum Tsunami {
    static let period: Double = 32
    static let engulfStart = 0.60   // phase fraction
    static let engulfEnd = 0.74

    static func phase(age: Double) -> Double {
        let m = age.truncatingRemainder(dividingBy: period)
        return (m < 0 ? m + period : m) / period
    }
    static func isEngulfed(age: Double) -> Bool {
        let p = phase(age: age)
        return p >= engulfStart && p < engulfEnd
    }
}
```

`WeatherHost.swift` — add a private helper and gate `str`/`rowCount`:

```swift
    /// True while the tsunami demo condition has the window engulfed — the whole UI drowns.
    private var uiDrowned: Bool {
        (conditionOverride ?? model.condition) == 7 && Tsunami.isEngulfed(age: conditionAge())
    }
```

At the top of `func str(_ key: String) -> String?` insert:

```swift
        if uiDrowned { return "" }   // empty strings skip rendering — the forecast is underwater
```

In `rowCount(now:)`, add after the `buried` line:

```swift
        if uiDrowned { return 0 }
```

`ConditionCycle.swift` — the 1-arg helpers:

```swift
    static func next(_ current: Double?) -> Double? { next(current, upTo: 7) }
    static func prev(_ current: Double?) -> Double? { prev(current, upTo: 7) }
```

Update the doc comment ("condition (`upTo 7` — 0–5 live buckets + 6 winds + 7 tsunami demo)").

`ConditionOverrideTests.swift` — update the two wrap assertions:
- `testCycleForwardWrapsThroughLive`: `next(4) == 5`, `next(5) == 6`, `next(7) == nil`.
- `testCycleBackwardWrapsThroughLive`: `prev(nil) == 7`, `prev(7) == 6`.
- `testGeneralizedCycleBounds`: the two 1-arg lines become `next(7) == nil` / `next(nil) == 0`.

- [ ] **Step 4: swift test — all green** (30 adjusted + 4 new = 34).

- [ ] **Step 5: Commit**

```bash
git add weather/Sources/Weather/Tsunami.swift weather/Sources/Weather/WeatherHost.swift weather/Sources/Weather/ConditionCycle.swift weather/Tests/WeatherTests/TsunamiTests.swift weather/Tests/WeatherTests/ConditionOverrideTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): Tsunami sync enum + UI drowning + tour extended to demo conditions (TDD)"
```

---

### Task 2: Shader — high winds (condition 6)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (new `wind_c`, `debris_layer`, `impact_event`; `fs()` case 6; `window_alpha` case 6 + fog-branch fix)

**Interfaces:**
- Produces: `fn impact_event(t: f32) -> vec4<f32>` — (spring_amt, edge_x01, edge_y, converge_phase) shared by the scene streak and the window dent; `fn wind_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32>`.
- Consumes: `sky_dome`, `view_ray`, `sun_dir`, `moment`, `fbm`, `rain_streaks`, `base_mask`, `hash21`.

- [ ] **Step 1: Impact event helper** (before `window_alpha`)

```wgsl
// Debris-impact event (winds): a moment channel picks an edge point; the scene draws a
// streak CONVERGING on it, then the window takes a dent with a damped spring-back.
// Returns (spring, edge_x, edge_y, converge) — spring > 0 only just after contact (phase
// 0.70 of the slot); converge ∈ (0,1] only during the approach (phase 0.55..0.70).
fn impact_event(t: f32) -> vec4<f32> {
    let m = moment(t, 0.08, 0.7, 21.0);
    if (m.w < 0.5) { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }
    let side = step(0.5, hash21(vec2<f32>(m.z, 2.0)));            // 0 = left edge, 1 = right
    let ey = 0.15 + 0.60 * hash21(vec2<f32>(m.z, 3.0));
    var spring = 0.0;
    if (m.y > 0.70) {
        let p = m.y - 0.70;
        spring = exp(-p * 9.0) * sin(p * 55.0);                    // damped wobble
    }
    let converge = smoothstep(0.55, 0.70, m.y) * step(m.y, 0.70);
    return vec4<f32>(spring, side, ey, converge);
}
```

- [ ] **Step 2: wind_c scene**

```wgsl
fn wind_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let rd = view_ray(uv);
    let sd = sun_dir(u.sun);
    var col = sky_dome(rd, sd, u.sun);
    let g = moment(t, 0.15, 0.6, 20.0);        // gusts (shared with window_alpha case 6)
    // Shredded racing clouds: horizontally-elongated 2D noise scrolling FAST.
    let shred = fbm(vec2<f32>(uv.x * 1.4 - t * (0.9 + g.x * 0.6), uv.y * 7.0));
    let cover = smoothstep(0.55, 0.75, shred) * smoothstep(0.75, 0.15, uv.y);
    let cloudc = mix(vec3<f32>(0.35, 0.38, 0.45), vec3<f32>(0.95, 0.96, 0.99), day);
    col = mix(col, cloudc, cover * 0.75);
    // Debris: rain_streaks TRANSPOSED (columns -> rows, y-scroll -> x-scroll), ochre-tinted,
    // two depths, speed surging with gusts.
    let spd = 2.2 * (1.0 + g.x * 1.5);
    let d1 = rain_streaks(vec2<f32>(uv.y, uv.x), t, 0.8, 0.15, spd);
    let d2 = rain_streaks(vec2<f32>(uv.y * 1.6, uv.x * 1.4), t * 0.7, 0.8, 0.10, spd);
    col = col + vec3<f32>(0.55, 0.45, 0.28) * d1 * 0.30 * mix(0.5, 1.0, day);
    col = col + vec3<f32>(0.45, 0.38, 0.25) * d2 * 0.18 * mix(0.5, 1.0, day);
    // Converging impact streak: a bright dash flying toward the strike point.
    let ie = impact_event(t);
    if (ie.w > 0.001) {
        let target = vec2<f32>(ie.y, ie.z);
        let from = vec2<f32>(1.0 - ie.y, ie.z - 0.25);            // enters from the far side, above
        let pos = mix(from, target, ie.w);
        let asp = u.res.y / u.res.x;
        let dd = length((uv - pos) * vec2<f32>(1.0, asp));
        col = col + vec3<f32>(0.75, 0.62, 0.40) * smoothstep(0.020, 0.004, dd);
    }
    return col;
}
```

- [ ] **Step 3: fs() + window_alpha wiring**

`fs()` switch gains:

```wgsl
        case 6: { col = wind_c(uv, t, sky, intensity); }
        case 7: { col = tsunami_c(uv, t, sky, intensity); }
```

(Add `tsunami_c` as a stub returning `sky_dome(view_ray(uv), sun_dir(u.sun), u.sun)` in THIS task so the switch compiles; Task 3 replaces it.)

`window_alpha`: change the fog `} else {` to `} else if (cond == 5) {`, then add:

```wgsl
    } else if (cond == 6) {
        // High winds: tremble + gust jolts + top-edge fabric flap + debris-impact dents.
        let g = moment(t, 0.15, 0.6, 20.0);
        let jit = (hash21(vec2<f32>(floor(t * 30.0), 1.0)) - 0.5) * 0.003;
        let jolt = g.x * 0.008;                                    // shove downwind (-x)
        let uvw = uv + vec2<f32>(jit - jolt, jit * 0.6);
        a = base_mask(uvw, 0.0);
        // Top edge luffs like fabric: a traveling ripple, gust-enveloped.
        let flap = (0.5 + 0.5 * sin(uv.x * 30.0 - t * 18.0)) * 0.010 * (0.3 + g.x);
        a = a * smoothstep(0.0, 0.010, uvw.y - flap * smoothstep(0.15, 0.0, uv.y));
        // Bottom: gentle clear-style wave.
        let b = smoothstep(0.86, 1.0, uv.y);
        a = a * (1.0 - smoothstep(0.45 + 0.06 * sin(x * 7.0 + t * 1.2), 1.0, b));
        // Impact dent with damped spring-back.
        let ie = impact_event(t);
        if (abs(ie.x) > 0.001) {
            let asp2 = u.res.y / u.res.x;
            let dd = length((uv - vec2<f32>(ie.y, ie.z)) * vec2<f32>(1.0, asp2));
            a = a * (1.0 - clamp(ie.x, 0.0, 1.0) * 0.9 * smoothstep(0.06, 0.0, dd));
        }
    } else if (cond == 7) {
        // Tsunami window work arrives in Task 3; keep the base mask until then.
    }
```

The branch chain ends up: clear 0 / cloud 1 / rain 2 / snow 3 / storm 4 / fog `else if (cond == 5)` / winds 6 / tsunami 7 / final `else {}` (safe default: plain base mask, deliberately empty).

- [ ] **Step 4: Eyeball loop**

`WX_COND=6 WX_SUN=1.0 WX_INT=0.6` launch; burst 24 frames @0.3 s. Checklist: sky reads *windy* (racing shreds, not cumulus); debris streams with visible gust surges; window trembles constantly; a gust visibly shoves it; top edge ripples; within ~40 s an impact lands — streak converges, dent pops, springs back with wobble. Night variant (`WX_SUN=-1.0`) still legible. Tune amplitudes.

- [ ] **Step 5: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): high-winds demo condition — gale scene, buffeting window, debris impact dents"
```

---

### Task 3: Shader — tsunami (condition 7)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (real `tsunami_c`, `tsunami_phase`, `window_alpha` case 7)

**Interfaces:**
- Produces: `fn tsunami_phase() -> f32` (fract(u.cond_age / 32.0) — the 32 and [0.60, 0.74] window MUST equal `Tsunami.period`/`engulfStart`/`engulfEnd` in Swift).
- Consumes: `sky_dome`, `fbm`, `snow_layer2` (bubbles), `base_mask`, `moment`.

- [ ] **Step 1: tsunami_phase + tsunami_c**

```wgsl
// SYNC: 32s period and the 0.60..0.74 engulf window mirror Tsunami.swift. Change together.
fn tsunami_phase() -> f32 { return fract(u.cond_age / 32.0); }

fn tsunami_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let ph = tsunami_phase();
    let day = sky.daylight;
    var col = sky_dome(view_ray(uv), sun_dir(u.sun), u.sun);
    // Sea level (uv.y of the surface): calm 0.80 -> swell 0.70 -> wall to 0.05 -> restore.
    var level = 0.80;
    level = level - smoothstep(0.0, 0.45, ph) * 0.10;
    level = level - smoothstep(0.45, 0.62, ph) * 0.75;
    level = level + smoothstep(0.74, 0.95, ph) * 0.85;
    // 4 parallax wave bands stacked below `level`, each fbm-displaced, nearer = darker + wilder.
    let chop = 1.0 + smoothstep(0.30, 0.60, ph) * 2.0;            // seas roughen as it builds
    for (var k = 0; k < 4; k = k + 1) {
        let fk = f32(k);
        let wob = (fbm(vec2<f32>(uv.x * (2.0 + fk * 1.3) + t * (0.25 + fk * 0.18), fk * 3.7)) - 0.5) * chop;
        let surf = level + fk * 0.030 + wob * (0.025 + fk * 0.012);
        let m = smoothstep(surf, surf + 0.012, uv.y);
        let water = mix(vec3<f32>(0.10, 0.34, 0.44), vec3<f32>(0.02, 0.13, 0.21), fk / 3.0);
        col = mix(col, water * mix(0.35, 1.0, day), m * 0.85);
        // Foam crest on each band, brightest when seas are rough.
        col = col + vec3<f32>(0.90, 0.95, 1.0)
                  * smoothstep(0.010, 0.0, abs(uv.y - surf)) * (0.25 + 0.5 * chop * abs(wob));
    }
    // Spray during rise/crash: fast upward-streaking particles above the surface.
    let spray_amt = smoothstep(0.45, 0.60, ph) * (1.0 - smoothstep(0.74, 0.80, ph));
    if (spray_amt > 0.0) {
        let sp = snow_layer2(vec2<f32>(uv.x, 1.0 - uv.y), t * 2.5, 16.0, 0.8, 6.0, 0.05, 0.06);
        col = col + vec3<f32>(0.85, 0.92, 0.97) * sp * spray_amt * 0.6;
    }
    // Engulf: full-screen underwater — deep teal, caustic shimmer, rising bubbles.
    let uw = smoothstep(0.58, 0.62, ph) * (1.0 - smoothstep(0.72, 0.76, ph));
    if (uw > 0.0) {
        let caust = fbm(uv * 6.0 + vec2<f32>(t * 0.8, -t * 0.5))
                  * fbm(uv * 9.0 - vec2<f32>(t * 0.6, t * 0.4));
        var deep = vec3<f32>(0.03, 0.17, 0.25) + vec3<f32>(0.18, 0.45, 0.50) * caust;
        let bub = snow_layer2(vec2<f32>(uv.x, 1.0 - uv.y), t, 14.0, 0.5, 9.0, 0.05, 0.0);
        deep = deep + vec3<f32>(0.7, 0.85, 0.9) * bub * 0.5;
        col = mix(col, deep * mix(0.5, 1.0, day), uw);
    }
    return col;
}
```

- [ ] **Step 2: window_alpha case 7**

Replace the Task-2 placeholder:

```wgsl
    } else if (cond == 7) {
        let ph = tsunami_phase();
        // Impact bulge: the window swells outward as the wall arrives, peaking at the crash.
        let bulge = smoothstep(0.45, 0.62, ph) * (1.0 - smoothstep(0.66, 0.74, ph)) * 0.012;
        a = base_mask(uv, -bulge);
        // Recede: water sheets off — heavy drip streams below the bottom edge + side streams.
        let shed = smoothstep(0.74, 0.78, ph) * (1.0 - smoothstep(0.92, 1.0, ph));
        if (shed > 0.0) {
            let stream = fbm(vec2<f32>(x * 10.0, t * 1.4));
            let ext = smoothstep(0.9, 1.0, uv.y) * smoothstep(0.25, 0.75, stream) * shed;
            a = max(a, ext * base_mask(vec2<f32>(uv.x, 0.5), 0.0));   // streams hang below
            let side_d = min(uv.x, 1.0 - uv.x);
            a = max(a, shed * smoothstep(0.012, 0.0, side_d)
                        * smoothstep(0.35, 0.75, fbm(vec2<f32>(uv.y * 8.0, t + x))) * 0.8);
        }
    }
```

- [ ] **Step 3: Eyeball loop**

- Full cycle: `WX_COND=7 WX_SUN=1.0` fresh launch, watch 35 s. Checklist: calm ocean reads (bands + foam, horizon rising); wall climbs; crash sweeps; **UI text vanishes underwater** (host blanking — hero/hourly/daily gone), caustics + bubbles; recede sheds water off edges; text returns; loop repeats from calm.
- Phase jumps: `WX_AGE=15` (swell), `WX_AGE=17` (rise), `WX_AGE=20` (engulf still — verify blanked UI + underwater in one frame), `WX_AGE=25` (recede).
- Tune levels/colors until the crash gasps.

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): tsunami demo condition — 32s arc, host-synced UI drowning, bulge + water-shed window"
```

---

### Task 4: Verification + ship

- [ ] **Step 1: Perf gate** — probe into `SkinView.show` (Global Constraints snippet from the previous plan), measure `WX_COND=6` and `WX_COND=7` (24 s each; for 7 use `WX_AGE=18` to catch the busy crash phase). p50 ≤ ~18.5 ms. Remove probe; `git diff` on SkinView empty.
- [ ] **Step 2: GIFs** — winds: 45 s record, cut 9 s around an impact (frame-diff detection); tsunami: full 32 s real-time from `WX_AGE=0` launch (`setpts=PTS/2` → 16 s GIF, the engulf + blanked UI must be visible). Send both to the user.
- [ ] **Step 3: Gate** — `swift test` 34/34; `git diff main --stat | grep crates` empty; tour keys → through 7 → live verified once by hand-driving or env relaunch.
- [ ] **Step 4: Commit any tuning, push, PR update** — comment on #45 describing the two demo conditions. No Claude-attribution footer.

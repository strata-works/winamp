# Weather App Shader Revamp Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refine the weather app's 6-condition shader with four shared systems (continuous sun-elevation palette, episodic moment scheduler, standardized depth model, final grade pass) plus per-condition depth/moment/polish upgrades and a sympathetic UI pass.

**Architecture:** Shared systems + bespoke motifs. Four global WGSL systems every condition consumes; the six bespoke `*_c()` condition functions stay, upgraded in place. Host (Swift) gains a continuous `wx_sun` uniform computed on read from real sunrise/sunset. **Zero engine changes** — everything lives in `weather/skins/weather/assets/weather.wgsl`, `weather/skins/weather/skin.lua`, and `weather/Sources/Weather/`.

**Tech Stack:** WGSL (parsed by naga inside the engine at `carapace_create`), Lua skin, Swift/SwiftPM host app, XCTest.

**Spec:** `docs/superpowers/specs/2026-07-15-weather-app-shader-revamp-design.md`

## Global Constraints

- **Zero engine changes** — no diffs under `crates/`. If a task seems to need one, STOP and escalate.
- **60 fps sustained is the hard perf gate** (measured in Task 7; far parallax planes use 3-octave fbm).
- Shader visual code in this plan is a **starting point**; each shader task ends in an eyeball tuning loop (launch → screenshot → tune) and values may drift. Structure (functions, signatures, uniforms) must NOT drift.
- Swift work is TDD. Run tests with: `cd weather && swift test` (18 pre-existing tests must stay green).
- Commit identity: `git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit ...`
- Never push to `main`; work stays on branch `weather-app-shader-revamp`.
- GUI verification from a background shell: build then
  `launchctl asuser $(id -u) /bin/zsh -lc "cd <repo>/weather && exec .build/arm64-apple-macosx/debug/Weather"`
  Drive keys via `osascript -e 'tell application "System Events" to key code <n>'` (→=124 ←=123 D=2 S=1 R=15) after setting the process frontmost. Capture: query window bounds via AppleScript, then `screencapture -x -R"x,y,w,h" out.png` while frontmost. Cleanup: `pkill -f "arm64-apple-macosx/debug/Weather"`.
- The engine reports shader parse errors at `carapace_create` → the app `fatalError`s with `carapace_last_error`; a failed launch + that message = WGSL syntax/uniform mismatch.

---

### Task 1: Sun-elevation plumbing (host TDD + skin + minimal shader consumption)

Replaces binary `wx_is_day` with continuous `wx_sun` ∈ [-1, 1], end to end, in one green commit.

**Files:**
- Create: `weather/Sources/Weather/SunMath.swift`
- Create: `weather/Tests/WeatherTests/SunMathTests.swift`
- Modify: `weather/Sources/Weather/WeatherService.swift` (query, `Response`, `map`, new `parseLocal`)
- Modify: `weather/Sources/Weather/WeatherModel.swift` (drop `isDay`, add `sunrise`/`sunset`)
- Modify: `weather/Sources/Weather/WeatherHost.swift` (`sunOverride`, `wx_sun`, drop `wx_is_day`)
- Modify: `weather/Sources/Weather/ConditionCycle.swift` (stops-array cycle)
- Modify: `weather/Sources/Weather/App.swift` (D key)
- Modify: `weather/Sources/Weather/mock.json` (`utc_offset_seconds`, `daily.sunrise`, `daily.sunset`)
- Modify: `weather/skins/weather/skin.lua` (uniform rename)
- Modify: `weather/skins/weather/assets/weather.wgsl` (derive `day` from `u.sun` — minimal)
- Modify tests: `weather/Tests/WeatherTests/WeatherServiceTests.swift`, `WeatherHostTests.swift`, `ConditionOverrideTests.swift`

**Interfaces:**
- Produces: `SunMath.sunElevation(now: Date, sunrise: Date, sunset: Date) -> Double` in [-1, 1]; `SunMath.presenterStops: [Double]` == `[0.1, 1.0, -0.1, -1.0]` (dawn/noon/dusk/night); `WeatherModel.sunrise/.sunset: Date`; `WeatherHost.sunOverride: Double?`; uniform `wx_sun`; skin uniform name `sun` (shader reads `u.sun`). `ConditionCycle.next(_ current: Double?, stops: [Double]) -> Double?`.
- Consumes: nothing new.

- [ ] **Step 1: Write failing SunMath tests**

`weather/Tests/WeatherTests/SunMathTests.swift`:

```swift
import XCTest
@testable import Weather

final class SunMathTests: XCTestCase {
    /// Fixed-date helper (UTC, 2026-07-15 unless day overridden).
    private func date(_ hour: Int, _ minute: Int = 0, day: Int = 15) -> Date {
        var c = DateComponents()
        c.year = 2026; c.month = 7; c.day = day; c.hour = hour; c.minute = minute
        c.timeZone = TimeZone(secondsFromGMT: 0)
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(secondsFromGMT: 0)!
        return cal.date(from: c)!
    }
    // 12h day: sunrise 06:00, sunset 18:00.
    private var sr: Date { date(6) }
    private var ss: Date { date(18) }

    func testNoonIsOne() {
        XCTAssertEqual(SunMath.sunElevation(now: date(12), sunrise: sr, sunset: ss), 1.0, accuracy: 1e-9)
    }
    func testSunriseAndSunsetAreZero() {
        XCTAssertEqual(SunMath.sunElevation(now: date(6), sunrise: sr, sunset: ss), 0.0, accuracy: 1e-9)
        XCTAssertEqual(SunMath.sunElevation(now: date(18), sunrise: sr, sunset: ss), 0.0, accuracy: 1e-9)
    }
    func testMidMorningIsLinear() {
        XCTAssertEqual(SunMath.sunElevation(now: date(9), sunrise: sr, sunset: ss), 0.5, accuracy: 1e-9)
    }
    func testNightMidpointIsMinusOne() {
        // Night arc 18:00 -> next 06:00; midpoint = 00:00 next day.
        XCTAssertEqual(SunMath.sunElevation(now: date(0, day: 16), sunrise: sr, sunset: ss), -1.0, accuracy: 1e-9)
    }
    func testBeforeSunriseIsSmallNegative() {
        // 05:00 same day = 11h after sunset in the wrapped 12h night arc -> -(1-|2*(11/12)-1|) = -1/6.
        XCTAssertEqual(SunMath.sunElevation(now: date(5), sunrise: sr, sunset: ss), -1.0 / 6.0, accuracy: 1e-9)
    }
    func testAfterSunsetIsNegative() {
        // 21:00 = 3h into the 12h night arc -> g=0.25 -> -0.5.
        XCTAssertEqual(SunMath.sunElevation(now: date(21), sunrise: sr, sunset: ss), -0.5, accuracy: 1e-9)
    }
    func testDegenerateDayReturnsZero() {
        XCTAssertEqual(SunMath.sunElevation(now: date(12), sunrise: sr, sunset: sr), 0.0)
    }
    func testPresenterStopsCycle() {
        var v: Double? = nil
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, 0.1)   // dawn
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, 1.0)   // noon
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, -0.1)  // dusk
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, -1.0)  // night
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertNil(v)          // -> live
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd weather && swift test 2>&1 | tail -5`
Expected: compile FAILURE — `cannot find 'SunMath' in scope`.

- [ ] **Step 3: Implement SunMath + ConditionCycle stops**

`weather/Sources/Weather/SunMath.swift`:

```swift
import Foundation

/// Continuous solar-elevation proxy in [-1, 1] from today's sunrise/sunset.
/// Piecewise-linear triangles: 1 at solar noon (day-arc midpoint), 0 at sunrise/sunset,
/// -1 at the night-arc midpoint. Dawn and dusk are symmetric (elevation only, no azimuth).
enum SunMath {
    static func sunElevation(now: Date, sunrise: Date, sunset: Date) -> Double {
        let dayLen = sunset.timeIntervalSince(sunrise)
        guard dayLen > 0, dayLen < 86_400 else { return 0 }   // degenerate/garbled -> horizon
        let nightLen = 86_400 - dayLen
        if now >= sunrise && now <= sunset {
            let f = now.timeIntervalSince(sunrise) / dayLen
            return 1 - abs(2 * f - 1)
        }
        // Night: position in the sunset -> next-sunrise arc, wrapping any date into [0, 86400).
        var ns = now.timeIntervalSince(sunset).truncatingRemainder(dividingBy: 86_400)
        if ns < 0 { ns += 86_400 }
        let g = min(ns / nightLen, 1)
        return -(1 - abs(2 * g - 1))
    }

    /// D-key presenter stops: dawn -> noon -> dusk -> night (then back to live).
    static let presenterStops: [Double] = [0.1, 1.0, -0.1, -1.0]
}
```

Append to `weather/Sources/Weather/ConditionCycle.swift` (inside the enum):

```swift
    /// Cycle through an explicit stops array: nil (live) -> stops[0] -> ... -> last -> nil.
    /// Used by the D key over `SunMath.presenterStops`. Matches on exact stored values.
    static func next(_ current: Double?, stops: [Double]) -> Double? {
        guard let c = current else { return stops.first }
        guard let i = stops.firstIndex(of: c), i + 1 < stops.count else { return nil }
        return stops[i + 1]
    }
```

- [ ] **Step 4: Run SunMath tests — pass**

Run: `cd weather && swift test --filter SunMathTests 2>&1 | tail -3`
Expected: `Executed 8 tests, with 0 failures`.

- [ ] **Step 5: Thread sunrise/sunset through model + service + fixtures**

`weather/Sources/Weather/WeatherModel.swift` — replace `var isDay: Double // 0 night / 1 day` with:

```swift
    var sunrise: Date       // today's sunrise (location-local instant)
    var sunset: Date        // today's sunset
```

and update `.sample` (it becomes a computed-once closure so it can reference "today"):

```swift
    static let sample: WeatherModel = {
        let cal = Calendar.current
        let sr = cal.date(bySettingHour: 6, minute: 0, second: 0, of: Date()) ?? Date()
        let ss = cal.date(bySettingHour: 18, minute: 0, second: 0, of: Date()) ?? Date()
        return WeatherModel(
            location: "Accra",
            conditionText: "Partly cloudy",
            condition: 1, temp: 27, intensity: 0.4, season: 2,
            sunrise: sr, sunset: ss,
            tempNow: "27°", hiLo: "H:31° L:24°", feels: "Feels 30°",
            hours: (0..<12).map { i in
                HourCell(time: "\(12 + i)h", temp: "\(27 + (i % 4))°")
            },
            days: [
                DayRow(day: "Mon", glyph: "☀", hi: "31°", lo: "24°"),
                DayRow(day: "Tue", glyph: "⛅", hi: "30°", lo: "23°"),
                DayRow(day: "Wed", glyph: "☔", hi: "29°", lo: "24°"),
                DayRow(day: "Thu", glyph: "⛈", hi: "28°", lo: "23°"),
                DayRow(day: "Fri", glyph: "⛅", hi: "30°", lo: "24°"),
                DayRow(day: "Sat", glyph: "☀", hi: "32°", lo: "25°"),
                DayRow(day: "Sun", glyph: "☁", hi: "29°", lo: "23°"),
            ]
        )
    }()
```

(Reorder the `WeatherModel` stored properties to match: `location, conditionText, condition, temp, intensity, season, sunrise, sunset, tempNow, hiLo, feels, hours, days` — the memberwise init order changes with it.)

`weather/Sources/Weather/WeatherService.swift`:

1. `Response` gains a top-level field (Open-Meteo always returns it):

```swift
        let utc_offset_seconds: Int
```

2. `Response.Daily` gains:

```swift
            let sunrise: [String]
            let sunset: [String]
```

3. The daily query line becomes:

```swift
            .init(name: "daily", value: "weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset"),
```

4. New pure helper (near `hourLabel`):

```swift
    /// "2026-07-11T05:52" in the location's utc-offset -> Date. Open-Meteo emits offset-free
    /// local timestamps when timezone=auto; the top-level utc_offset_seconds locates them.
    static func parseLocal(_ isoTime: String, offsetSeconds: Int) -> Date? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(secondsFromGMT: offsetSeconds)
        f.dateFormat = "yyyy-MM-dd'T'HH:mm"
        return f.date(from: isoTime)
    }
```

5. In `map(...)`, replace `isDay: Double(r.current.is_day),` with sunrise/sunset (add before the `return`):

```swift
        // Degenerate fallback (missing/unparseable): sunrise==sunset -> sunElevation returns 0 (horizon).
        let sunrise = parseLocal(r.daily.sunrise.first ?? "", offsetSeconds: r.utc_offset_seconds) ?? Date()
        let sunset = parseLocal(r.daily.sunset.first ?? "", offsetSeconds: r.utc_offset_seconds) ?? Date()
```

and pass `sunrise: sunrise, sunset: sunset` in the `WeatherModel(...)` call (matching the new property order; `is_day` stays in the `current` query + `Response.Current` — decoded but unused).

`weather/Sources/Weather/mock.json`: add top-level `"utc_offset_seconds": 0,` (after `"timezone"`), and inside `"daily"` add (7 entries each, matching `daily.time` dates 2026-07-11..17, Accra-ish times):

```json
    "sunrise": ["2026-07-11T05:58", "2026-07-12T05:58", "2026-07-13T05:59", "2026-07-14T05:59", "2026-07-15T05:59", "2026-07-16T05:59", "2026-07-17T06:00"],
    "sunset":  ["2026-07-11T18:13", "2026-07-12T18:13", "2026-07-13T18:13", "2026-07-14T18:13", "2026-07-15T18:13", "2026-07-16T18:12", "2026-07-17T18:12"]
```

- [ ] **Step 6: Rewire WeatherHost + App key**

`weather/Sources/Weather/WeatherHost.swift`: rename `_isDayOverride`/`isDayOverride` to `_sunOverride`/`sunOverride` (update the doc comment: "Presenter override for the shader sun-elevation uniform (the `D` key cycles dawn/noon/dusk/night). Forces only `wx_sun`."). In `num(_:)` replace the `wx_is_day` case with:

```swift
        case "wx_sun":
            return sunOverride ?? SunMath.sunElevation(now: Date(), sunrise: model.sunrise, sunset: model.sunset)
```

(`Date()` on every read = the sky evolves continuously with zero timers.)

`weather/Sources/Weather/App.swift`: replace the D-key line and update the comment block:

```swift
    //   →/← tour condition · D cycles dawn/noon/dusk/night · S cycles season · R refetches.
        case 2:   host.sunOverride = ConditionCycle.next(host.sunOverride, stops: SunMath.presenterStops) // D
```

- [ ] **Step 7: Rename the uniform in skin + shader (minimal)**

`weather/skins/weather/skin.lua` — the `shader{}` uniforms line becomes:

```lua
        uniforms = { condition = "wx_condition", sun = "wx_sun",
                     temp = "wx_temp", intensity = "wx_intensity", season = "wx_season" } }
```

`weather/skins/weather/assets/weather.wgsl` — in `fs()`, replace `let day = clamp(u.is_day, 0.0, 1.0);` with:

```wgsl
    // Temporary until Task 2 introduces sky_grade: soft day factor from continuous elevation.
    let day = smoothstep(-0.12, 0.3, u.sun);
```

(The prelude's uniform struct is generated from the skin.lua names — any remaining `u.is_day` reference fails naga parse at launch. `grep -n "is_day" weather/skins/weather/assets/weather.wgsl` must return nothing.)

- [ ] **Step 8: Update the three existing test files**

`WeatherServiceTests.swift`:
- In `testDecodeScalarsAndStrings`, replace `XCTAssertEqual(m.isDay, 1)` with:

```swift
        XCTAssertEqual(m.sunrise, WeatherService.parseLocal("2026-07-11T05:58", offsetSeconds: 0))
        XCTAssertEqual(m.sunset, WeatherService.parseLocal("2026-07-11T18:13", offsetSeconds: 0))
```

- In `testUrlHasExpectedQuery`, the daily assertion becomes:

```swift
        XCTAssertTrue(u.contains("daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset"))
```

`WeatherHostTests.swift` line 9: replace the `wx_is_day` assertion with a range check (live value depends on wall-clock):

```swift
        let sun = host.num("wx_sun")!
        XCTAssertTrue(sun >= -1 && sun <= 1)
```

`ConditionOverrideTests.swift` lines 34–40: replace the isDay block with:

```swift
        let host = WeatherHost(model: .sample)
        host.sunOverride = -1.0
        XCTAssertEqual(host.num("wx_sun"), -1.0)          // override wins
        host.sunOverride = nil
        let live = host.num("wx_sun")!
        XCTAssertTrue(live >= -1 && live <= 1)             // back to live
```

- [ ] **Step 9: Full gate + visual smoke**

Run: `cd weather && swift build && swift test 2>&1 | tail -3`
Expected: all tests pass (18 pre-existing, adjusted, + 8 new = 26).

Launch (Global Constraints command), confirm the app renders (fixture first frame, then live) and D cycles four distinct-brightness looks then returns to live. Screenshot once. `pkill -f "arm64-apple-macosx/debug/Weather"`.

- [ ] **Step 10: Commit**

```bash
git add weather/ docs/
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): continuous wx_sun elevation uniform replaces binary wx_is_day (host+skin+shader plumbing)"
```

---

### Task 2: Shared shader systems (sky_grade, moment, depth helpers, final grade)

All-WGSL structural pass: introduce the four shared systems and mechanically rewire all six conditions onto them. No per-condition motif work yet — the scene should look *similar but graded* after this task.

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl`

**Interfaces:**
- Consumes: `u.sun` (Task 1).
- Produces (used by Tasks 3–5):
  - `struct Sky { key: vec3<f32>, ambient: f32, horizon: vec3<f32>, daylight: f32 }`
  - `fn sky_grade(sun: f32) -> Sky`
  - `fn moment(t: f32, rate: f32, prob: f32, channel: f32) -> vec4<f32>` — returns `(env, phase, seed, active)`
  - `fn fbm3(p: vec2<f32>) -> f32` (3-octave budget variant)
  - `fn atmo(col: vec3<f32>, sky_col: vec3<f32>, depth: f32) -> vec3<f32>`
  - `fn depth_grade(col: vec3<f32>, uv: vec2<f32>) -> vec3<f32>`
  - `fn grade(col_in: vec3<f32>, uv: vec2<f32>, t: f32) -> vec3<f32>` (tone curve + vignette + grain + temp/season tints)
  - `fn light_pos(t: f32, sun: f32) -> vec2<f32>` (elevation-tracking; signature change)
  - All six condition signatures become `fn <cond>_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32>`
  - `storm_strike(t)` unchanged shape `(env, bolt_x, life, seed)`, now gated via `moment()`.

- [ ] **Step 1: Add the shared systems to weather.wgsl**

After the existing noise helpers, add:

```wgsl
// 3-octave fbm for far/cheap layers (perf budget: far planes never use the 5-octave fbm).
fn fbm3(p: vec2<f32>) -> f32 {
    var v = 0.0; var amp = 0.5; var q = p;
    for (var k = 0; k < 3; k = k + 1) { v = v + amp * noise2(q); q = q * 2.0; amp = amp * 0.5; }
    return v;
}

// ---- Sky grade: one global light state from continuous sun elevation ----
// sun ∈ [-1,1]: 1 noon · 0 horizon (golden hour) · -1 deep night.
struct Sky {
    key: vec3<f32>,      // key-light color (disc, rims, rays)
    ambient: f32,        // scene ambient level
    horizon: vec3<f32>,  // horizon-band tint (gold at low sun)
    daylight: f32,       // soft 0 night .. 1 day (replaces the old binary is_day)
}
fn sky_grade(sun: f32) -> Sky {
    let daylight = smoothstep(-0.12, 0.35, sun);
    let gold  = vec3<f32>(1.0, 0.72, 0.42);
    let noonw = vec3<f32>(1.0, 0.96, 0.90);
    let moon  = vec3<f32>(0.72, 0.78, 0.92);
    let golden = 1.0 - smoothstep(0.0, 0.45, abs(sun));   // peaks at the horizon
    var key = mix(moon, noonw, daylight);
    key = mix(key, gold, golden * 0.75);
    let ambient = mix(0.16, 1.0, daylight);
    let horizon = mix(vec3<f32>(0.20, 0.16, 0.24), gold, golden) * mix(0.35, 1.0, daylight);
    return Sky(key, ambient, horizon, daylight);
}

// ---- Moment scheduler: irregular episodic events (generalizes storm_strike) ----
// Time is cut into slots of 1/rate seconds; each slot fires with probability `prob`
// (hash-gated, per `channel`). Returns (env, phase, seed, active): env is a smooth
// attack/decay envelope over the slot; phase ∈ [0,1) is slot progress; seed is stable
// per slot for randomizing the event's parameters.
fn moment(t: f32, rate: f32, prob: f32, channel: f32) -> vec4<f32> {
    let slot = floor(t * rate);
    let phase = fract(t * rate);
    let seed = hash21(vec2<f32>(slot, 17.0 + channel * 31.0));
    let active = step(1.0 - prob, seed);
    let env = active * smoothstep(0.0, 0.15, phase) * smoothstep(1.0, 0.45, phase);
    return vec4<f32>(env, phase, seed, active);
}

// ---- Depth helpers ----
// Atmospheric perspective: fade layer content toward the sky color by depth (0 near, 1 far).
fn atmo(col: vec3<f32>, sky_col: vec3<f32>, depth: f32) -> vec3<f32> {
    return mix(col, sky_col, depth * 0.55);
}
// Vertical depth grade: gently darken + desaturate down the canvas so the field reads as space.
fn depth_grade(col: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let d = smoothstep(0.15, 1.0, uv.y);
    let lum = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    return mix(col, mix(col, vec3<f32>(lum), 0.18) * 0.88, d);
}

// ---- Final grade: tints + soft-shoulder tone curve + vignette + grain ----
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    // Reinhard-style shoulder: keeps mids, stops additive glows blowing out to flat white.
    return c / (c + vec3<f32>(0.35)) * 1.35;
}
fn grade(col_in: vec3<f32>, uv: vec2<f32>, t: f32) -> vec3<f32> {
    var col = max(col_in, vec3<f32>(0.0));
    // Temperature warmth + season tint (moved here from fs()).
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.12;
    col = mix(col, col * season_tint(u.season), 0.08);
    col = tonemap(col);
    // Vignette (aspect-corrected).
    let asp = u.res.y / u.res.x;
    let p = (uv - vec2<f32>(0.5, 0.5)) * vec2<f32>(1.0, asp);
    col = col * (1.0 - 0.22 * smoothstep(0.35, 0.85, length(p)));
    // Animated hash grain (~±1.5/255) dithers mesh-gradient banding away.
    let g = hash21(uv * u.res + vec2<f32>(fract(t) * 61.7, 0.0)) - 0.5;
    col = col + vec3<f32>(g * 0.012);
    return clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
}
```

- [ ] **Step 2: Elevation-tracking light + moment-gated strike**

Replace `light_pos`:

```wgsl
// Shared directional light. Elevation maps to screen height: horizon -> low, noon/deep night -> high.
// (uv.y = 0 is the TOP of the canvas.)
fn light_pos(t: f32, sun: f32) -> vec2<f32> {
    let h = mix(0.34, 0.12, clamp(abs(sun), 0.0, 1.0));
    return vec2<f32>(0.72, h + 0.02 * sin(t * 0.3));
}
```

Replace `storm_strike` (same return shape; now a `moment()` client — channel 4, rate/prob match the old feel):

```wgsl
// Shared lightning-strike state so the bolt, shockwave, and window-edge jolt fire together.
// Returns (flash_env, bolt_x, life, seed); env is 0 when no strike. Sharper attack than the
// generic moment envelope, so it reshapes env from the raw phase.
fn storm_strike(t: f32) -> vec4<f32> {
    let m = moment(t, 0.7, 0.28, 4.0);
    let env = m.w * smoothstep(0.0, 0.04, m.y) * smoothstep(0.55, 0.08, m.y);
    let slot = floor(t * 0.7);
    let bx = 0.32 + 0.4 * hash21(vec2<f32>(slot, 7.0));
    return vec4<f32>(env, bx, m.y, slot);
}
```

- [ ] **Step 3: Mechanical rewire of the six conditions + fs()**

- Every condition signature: `fn clear_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32>` (same for cloud/rain/snow/storm/fog). First line of each body: `let day = sky.daylight;` — bodies otherwise unchanged this task.
- `clear_c`/`cloud_c` callers of `light_pos(t)` become `light_pos(t, u.sun)`.
- `fs()` becomes:

```wgsl
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let sky = sky_grade(clamp(u.sun, -1.0, 1.0));
    let intensity = clamp(u.intensity, 0.0, 1.0);
    let cond = i32(u.condition);
    var col: vec3<f32>;
    switch (cond) {
        case 0: { col = clear_c(uv, t, sky, intensity); }
        case 1: { col = cloud_c(uv, t, sky, intensity); }
        case 2: { col = rain_c(uv, t, sky, intensity); }
        case 3: { col = snow_c(uv, t, sky, intensity); }
        case 4: { col = storm_c(uv, t, sky, intensity); }
        case 5: { col = fog_c(uv, t, sky, intensity); }
        default: { col = clear_c(uv, t, sky, intensity); }
    }
    col = depth_grade(col, uv);
    col = grade(col, uv, t);
    col = col * ui_scrim(uv);
    let a = silhouette_alpha(uv, t, cond, intensity) * corner_alpha(uv);
    return vec4<f32>(col * a, a);
}
```

(The old temp-warmth/season lines in `fs()` are deleted — they live in `grade()` now.)

- [ ] **Step 4: Build + launch + eyeball loop**

`cd weather && swift build` then launch. A parse failure at launch prints the naga error via `fatalError` — fix and relaunch. Eyeball checklist (→/← over all 6 conditions, D over 4 stops):
- No banding visible in smooth gradient areas (grain working).
- Storm flash and clear-sky glow no longer clip to flat white (tonemap working); overall exposure comparable to before — retune `tonemap`'s `0.35/1.35` knobs if the scene got muddy or washed out.
- Dawn/dusk (D stops 1 and 3) show a warm golden cast globally (sky_grade feeding `day`).
- Storm still strikes with the same rhythm (moment-gated `storm_strike` regression check).
Screenshot 6 conditions at noon + clear at all 4 stops.

- [ ] **Step 5: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): shared shader systems — sky_grade, moment scheduler, depth helpers, final grade pass"
```

---

### Task 3: Clear + Cloud (depth, moments, polish)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (`clear_c`, `cloud_c`, new `shooting_star`)

**Interfaces:**
- Consumes: `Sky`, `moment()`, `fbm3`, `atmo`, `light_pos(t, sun)`, `god_rays`, `stars` (Task 2).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Rewrite clear_c + shooting_star**

```wgsl
// Shooting star: rare, brief streak with a fading tail across the upper sky (night only).
fn shooting_star(uv: vec2<f32>, t: f32) -> f32 {
    let m = moment(t, 0.12, 0.5, 7.0);           // a chance roughly every ~8s
    if (m.w < 0.5) { return 0.0; }
    let s0 = vec2<f32>(0.15 + 0.6 * hash21(vec2<f32>(m.z, 3.0)),
                       0.08 + 0.15 * hash21(vec2<f32>(m.z, 5.0)));
    let dir = normalize(vec2<f32>(0.8, 0.35));
    let head = s0 + dir * m.y * 0.5;
    let to_head = uv - head;
    let along = dot(to_head, dir);                       // negative behind the head
    let across = abs(to_head.x * dir.y - to_head.y * dir.x);
    var tail = 0.0;
    if (along < 0.0 && along > -0.18) { tail = exp(along * 20.0); }
    let width = smoothstep(0.004, 0.0, across);
    let head_glow = smoothstep(0.015, 0.0, length(to_head));
    let vis = smoothstep(0.0, 0.1, m.y) * smoothstep(1.0, 0.6, m.y);
    return (head_glow + width * tail * 0.8) * vis;
}

fn clear_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    // Palette anchors; the horizon anchor comes from the sky grade (golden hour lives there).
    let c0 = mix(vec3<f32>(0.03, 0.04, 0.14), vec3<f32>(0.22, 0.48, 0.90), day);
    let c1 = mix(vec3<f32>(0.05, 0.07, 0.20), vec3<f32>(0.36, 0.62, 0.95), day);
    let c2 = mix(vec3<f32>(0.09, 0.08, 0.20), vec3<f32>(0.66, 0.80, 0.97), day);
    let c3 = mix(vec3<f32>(0.15, 0.10, 0.19), sky.horizon, max(day, 0.35));
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Horizon glow band (strongest at golden hour).
    col = col + sky.horizon * smoothstep(0.45, 0.95, uv.y) * 0.18;
    // Two-layer starfield: far = dim + dense (offset/scaled grid), near = bright + sparse.
    let starvis = smoothstep(0.15, -0.25, u.sun);
    col = col + vec3<f32>(0.85, 0.88, 1.0) * stars(uv * 1.9 + vec2<f32>(3.7, 1.3), t * 0.7) * starvis * 0.35;
    col = col + vec3<f32>(0.92, 0.94, 1.0) * stars(uv, t) * starvis * 0.75;
    col = col + vec3<f32>(1.0) * shooting_star(uv, t) * starvis;
    // Sun/moon disc, elevation-tracking, aspect-corrected.
    let lp = light_pos(t, u.sun);
    let asp = u.res.y / u.res.x;
    let pd = length((uv - lp) * vec2<f32>(1.0, asp));
    var disc = smoothstep(0.070, 0.045, pd);
    // Moon surface: faint crater noise, night only.
    disc = disc * mix(0.92 + 0.14 * fbm3(uv * 30.0), 1.0, day);
    // Halo ring + broad glow; sun-flare pulse moment surges both by day.
    let flare = moment(t, 0.08, 0.5, 8.0).x * day;
    let glow = smoothstep(0.40, 0.0, pd) * mix(0.16, 0.24, day) * (1.0 + flare * 0.8);
    let halo = smoothstep(0.16, 0.0, pd) * 0.10;
    col = col + sky.key * (disc + glow + halo);
    // God-rays (subtle; surge with the flare pulse).
    col = col + sky.key * god_rays(uv, lp, t) * (0.10 + 0.20 * day) * (1.0 + flare);
    return col;
}
```

- [ ] **Step 2: Rewrite cloud_c**

```wgsl
fn cloud_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let c0 = mix(vec3<f32>(0.10, 0.11, 0.16), vec3<f32>(0.55, 0.62, 0.74), day);
    let c1 = mix(vec3<f32>(0.13, 0.14, 0.20), vec3<f32>(0.68, 0.73, 0.82), day);
    let c2 = mix(vec3<f32>(0.16, 0.17, 0.23), vec3<f32>(0.80, 0.83, 0.89), day);
    let c3 = mix(vec3<f32>(0.12, 0.13, 0.18), vec3<f32>(0.60, 0.66, 0.78), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let skybg = col;
    let lp = light_pos(t, u.sun);
    let to_light = normalize(lp - uv + vec2<f32>(0.0001, 0.0001));
    let litd = mix(vec3<f32>(0.20, 0.22, 0.28), vec3<f32>(0.92, 0.94, 0.98), day);
    let shad = mix(vec3<f32>(0.10, 0.11, 0.15), vec3<f32>(0.52, 0.56, 0.64), day);
    // Three parallax planes, far -> near. Far plane: 3-octave fbm + atmospheric fade.
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let far = 1.0 - fk / 2.0;                 // 1 far .. 0 near
        let sc = 2.0 + fk * 1.6;
        let sp = 0.04 + fk * 0.03;
        let q = uv * vec2<f32>(sc, sc * 0.7) + vec2<f32>(t * sp, fk * 3.1);
        // Billow: light second-octave warp of the sample coordinate.
        let bw = vec2<f32>(fbm3(q * 1.7 + vec2<f32>(t * 0.02, 0.0)), fbm3(q * 1.7 + vec2<f32>(2.7, 1.1)));
        var n = 0.0;
        if (k == 0) { n = fbm3(q + 0.35 * bw); } else { n = fbm(q + 0.35 * bw); }
        let cover = smoothstep(0.55, 0.85, n) * (0.35 + 0.25 * fk) * (0.6 + 0.5 * intensity);
        // Directional lighting + sun-side rim (gradient of the field toward the light).
        let nlit = fbm3(q + 0.35 * bw + to_light * 0.10);
        let rim = clamp(n - nlit, 0.0, 1.0) * 2.2;
        var plane = mix(shad, litd, clamp(0.5 + (lp.x - uv.x) * 0.8, 0.0, 1.0));
        plane = atmo(plane, skybg, far);
        col = mix(col, plane, cover);
        col = col + sky.key * rim * cover * 0.35;
    }
    // Cloud-break moment: a god-ray shaft sweeps across during the event (day only).
    let mb = moment(t, 0.05, 0.6, 9.0);
    col = col + sky.key * god_rays(uv, vec2<f32>(0.2 + 0.6 * mb.y, 0.18), t) * mb.x * 0.5 * day;
    return col;
}
```

- [ ] **Step 3: Build + launch + eyeball loop**

Checklist (clear + cloud, ×4 D stops):
- Clear night: two visibly distinct star layers; a shooting star appears within ~60 s of watching; moon has faint surface texture.
- Clear day: sun flare pulse visibly breathes glow/rays occasionally; golden hour (dawn/dusk stops) tints the horizon band gold; disc rides lower at dawn/dusk than noon.
- Cloud: edges billow (no round blobs); sun-side rims visible; far plane hazier than near; a god-ray shaft sweeps through occasionally by day.
- Tune constants until it reads premium; structure fixed.

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): clear & cloud revamp — starfield depth, golden hour, flare/shooting-star/cloud-break moments, cloud rims"
```

---

### Task 4: Rain + Snow (depth, moments, polish)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (`rain_streaks`, `rain_c`, `snow_layer`, `snow_c`)

**Interfaces:**
- Consumes: `Sky`, `moment()`, `rot` (Task 2 / existing helpers).
- Produces: `rain_streaks(uv, t, intensity, slant, speedm)` — Task 5's storm consumes this 5-arg form.

- [ ] **Step 1: Parameterize rain_streaks + rewrite rain_c**

```wgsl
// Falling rain streaks. slant = diagonal lean (gusts push it), speedm = fall-speed multiplier.
fn rain_streaks(uv: vec2<f32>, t: f32, intensity: f32, slant: f32, speedm: f32) -> f32 {
    let sl = uv + vec2<f32>(uv.y * slant, 0.0);
    let cols = 55.0;
    let x = sl.x * cols;
    let col = floor(x);
    let fx = fract(x) - 0.5;
    let speed = (0.8 + hash21(vec2<f32>(col, 1.0)) * 1.0) * speedm;
    // -t so streaks scroll DOWN (uv.y = 0 is the top of the canvas).
    let y = fract(uv.y * 3.4 - t * speed + hash21(vec2<f32>(col, 3.0)));
    let line = smoothstep(0.09, 0.0, abs(fx));
    let head = smoothstep(0.85, 0.30, y) * smoothstep(0.0, 0.10, y);
    return line * head * (0.4 + 0.7 * intensity);
}

fn rain_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    // Wind gust: slant + speed + brightness surge together over the event.
    let g = moment(t, 0.18, 0.45, 10.0);
    let slant = 0.06 + g.x * 0.22;
    let speedm = 1.0 + g.x * 0.9;
    let c0 = mix(vec3<f32>(0.06, 0.09, 0.14), vec3<f32>(0.30, 0.40, 0.52), day);
    let c1 = mix(vec3<f32>(0.08, 0.11, 0.17), vec3<f32>(0.38, 0.48, 0.60), day);
    let c2 = mix(vec3<f32>(0.10, 0.13, 0.19), vec3<f32>(0.46, 0.56, 0.68), day);
    let c3 = mix(vec3<f32>(0.05, 0.08, 0.13), vec3<f32>(0.28, 0.38, 0.50), day);
    // Depth: far misty rain sheet behind the glass streaks.
    let far = rain_streaks(uv * vec2<f32>(1.7, 1.4), t * 0.55, intensity, slant * 0.7, speedm) * 0.5;
    let streak = rain_streaks(uv, t, intensity, slant, speedm);
    // Wet-glass refraction, slightly stronger than before.
    let ruv = uv + vec2<f32>(streak * 0.014, 0.0);
    var col = mesh_gradient(ruv, t, c0, c1, c2, c3);
    col = col + vec3<f32>(0.45, 0.54, 0.68) * far * 0.15;
    col = col + vec3<f32>(0.65, 0.74, 0.88) * streak * 0.3 * (1.0 + g.x * 0.6);
    // Near depth: occasional large soft drops streaking past.
    let big = snow_layer(uv, t * 3.2, 5.0, 0.9, 5.0);
    col = col + vec3<f32>(0.55, 0.64, 0.80) * big * 0.18;
    // Wet sheen + pooling ripples near the silhouette band.
    col = col + vec3<f32>(0.10, 0.13, 0.18) * smoothstep(0.4, 1.0, uv.y) * (0.4 + 0.4 * day);
    let pool = smoothstep(0.78, 0.9, uv.y) * (0.5 + 0.5 * sin(uv.x * 40.0 - t * 4.0 + fbm(uv * 8.0) * 6.0));
    col = col + vec3<f32>(0.5, 0.6, 0.75) * pool * 0.12 * (0.5 + intensity);
    return col;
}
```

**⚠️ Call-site fix in the same step:** `storm_c` still calls the old 3-arg form. Update its one call to `rain_streaks(uv, t, 1.0, 0.06, 1.0)` (neutral slant/speed — Task 5 replaces it with the gust-locked values). naga rejects the whole shader on an arg-count mismatch, so this task's launch would otherwise fail.

- [ ] **Step 2: Upgrade snow_layer + rewrite snow_c**

```wgsl
// One parallax snow layer: soft round flakes with coherent sway and per-flake size jitter.
// -t*speed falls DOWN (uv.y = 0 is the top). `boost` lowers the density threshold (flurries).
fn snow_layer2(uv: vec2<f32>, t: f32, scale: f32, speed: f32, seed: f32, soft: f32, boost: f32) -> f32 {
    var p = uv * scale + vec2<f32>(0.0, -t * speed);
    p.x = p.x + sin(t * (0.4 + seed * 0.2) + p.y * 1.5) * 0.35;   // coherent sway
    let g = floor(p);
    let f = fract(p) - 0.5;
    let h = hash21(g + seed);
    let sz = mix(0.10, 0.10 + soft, hash21(g + seed + 7.0));       // per-flake size jitter
    return smoothstep(sz, 0.0, length(f)) * step(0.84 - boost, h);
}
// Legacy 5-arg form kept for rain's "big drops" layer.
fn snow_layer(uv: vec2<f32>, t: f32, scale: f32, speed: f32, seed: f32) -> f32 {
    return snow_layer2(uv, t, scale, speed, seed, 0.06, 0.0);
}

fn snow_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let c0 = mix(vec3<f32>(0.16, 0.19, 0.28), vec3<f32>(0.74, 0.80, 0.90), day);
    let c1 = mix(vec3<f32>(0.20, 0.23, 0.32), vec3<f32>(0.82, 0.87, 0.95), day);
    let c2 = mix(vec3<f32>(0.24, 0.27, 0.36), vec3<f32>(0.90, 0.93, 0.99), day);
    let c3 = mix(vec3<f32>(0.18, 0.21, 0.30), vec3<f32>(0.78, 0.84, 0.93), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    // Flurry moment: density surge + gentle swirl of the whole field.
    let f = moment(t, 0.14, 0.4, 11.0);
    let suv = (uv - vec2<f32>(0.5, 0.5)) * rot(f.x * 0.35 * sin(t * 0.8)) + vec2<f32>(0.5, 0.5);
    let boost = f.x * 0.06;
    // Far (small/sharp) -> near (big/soft/swaying) parallax layers.
    var flakes = 0.0;
    flakes = flakes + snow_layer2(suv, t, 22.0, 0.10, 1.0, 0.04, boost) * 0.6;
    flakes = flakes + snow_layer2(suv, t, 15.0, 0.16, 2.0, 0.06, boost) * 0.8;
    flakes = flakes + snow_layer2(suv, t,  6.0, 0.26, 3.0, 0.16, boost) * 1.0;
    let bloom = mix(0.75, 1.0, day);
    col = col + vec3<f32>(1.0) * flakes * (0.35 + 0.4 * intensity) * bloom;
    // Faint ground-glow where snow gathers near the silhouette band.
    col = col + vec3<f32>(0.9, 0.93, 1.0) * smoothstep(0.75, 0.98, uv.y) * 0.08 * (0.4 + 0.6 * intensity);
    return col;
}
```

- [ ] **Step 3: Build + launch + eyeball loop**

Checklist (rain + snow, day/night):
- Rain: far misty sheet reads *behind* the crisp streaks; gusts visibly lean + accelerate the rain every ~10–30 s; occasional big near drops.
- Snow: near flakes big/soft and swaying, far flakes small/sharp; flurries visibly thicken + swirl; faint glow at the bottom band.
- Rain and snow still fall DOWN (regression: the `-t` signs).

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): rain & snow revamp — sheet-rain depth, gust/flurry moments, flake sway + size jitter"
```

---

### Task 5: Storm + Fog (depth, moments, polish)

**Files:**
- Modify: `weather/skins/weather/assets/weather.wgsl` (`storm_c`, `lightning`, `fog_banks`, `fog_c`)

**Interfaces:**
- Consumes: `moment()`, `storm_strike` (Task 2), `rain_streaks` 5-arg (Task 4), `fbm3`, `Sky`, `light_pos`.
- Produces: nothing consumed later.

- [ ] **Step 1: Storm — churn planes, double strike, distant flash, afterglow**

Add after `lightning`:

```wgsl
// Second bolt of an occasional double-strike: same slot as the primary, delayed ~10% of the
// slot, offset x. Gated on the PRIMARY slot being active (m.w) so a double never fires alone.
fn storm_strike2(t: f32) -> vec4<f32> {
    let m = moment(t, 0.7, 0.28, 4.0);            // same channel as storm_strike's gate
    let slot = floor(t * 0.7);
    let dbl = m.w * step(0.6, hash21(vec2<f32>(slot, 23.0)));
    let phase2 = m.y - 0.10;                      // smoothstep(0, 0.04, x) is 0 for x < 0
    let env = dbl * smoothstep(0.0, 0.04, phase2) * smoothstep(0.45, 0.06, phase2);
    let bx = clamp(0.32 + 0.4 * hash21(vec2<f32>(slot, 7.0)) + 0.10, 0.05, 0.95);
    return vec4<f32>(env, bx, phase2, slot + 101.0);
}
```

Rewrite `storm_c`:

```wgsl
fn storm_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let st = storm_strike(t);
    let st2 = storm_strike2(t);
    let flash = st.x + st2.x;
    // Shockwave ring from the primary strike's ground contact deforms the background.
    let asp = u.res.y / u.res.x;
    let impact = vec2<f32>(st.y, 0.70);
    let dvec = (uv - impact) * vec2<f32>(1.0, asp);
    let dist = length(dvec);
    let ring = smoothstep(0.09, 0.0, abs(dist - st.z * 1.1));
    let disp = normalize(dvec + vec2<f32>(0.0001, 0.0001)) * ring * st.x * 0.06;
    let duv = uv + disp;
    let w2 = warp(duv * 2.2 + vec2<f32>(t * 0.08, 0.0), t);
    let c0 = mix(vec3<f32>(0.04, 0.05, 0.09), vec3<f32>(0.20, 0.24, 0.32), day);
    let c1 = mix(vec3<f32>(0.07, 0.08, 0.13), vec3<f32>(0.30, 0.34, 0.42), day);
    let c2 = mix(vec3<f32>(0.05, 0.06, 0.11), vec3<f32>(0.24, 0.28, 0.36), day);
    let c3 = mix(vec3<f32>(0.02, 0.03, 0.07), vec3<f32>(0.14, 0.17, 0.24), day);
    var col = mesh_gradient(w2, t, c0, c1, c2, c3);
    // Churning cloud planes in the upper sky, lit by strikes from within.
    for (var k = 0; k < 2; k = k + 1) {
        let fk = f32(k);
        let n = fbm3(uv * vec2<f32>(2.5 + fk, 1.8) + vec2<f32>(t * (0.10 + 0.05 * fk), fk * 7.0));
        let cover = smoothstep(0.45, 0.8, n) * (0.5 + 0.3 * fk) * smoothstep(0.55, 0.1, uv.y);
        let dark = mix(vec3<f32>(0.05, 0.06, 0.10), vec3<f32>(0.16, 0.18, 0.24), day);
        col = mix(col, dark * (1.0 + flash * 0.8), cover);
    }
    // Driving rain sheets (gust-locked slant), in two depths.
    col = col + vec3<f32>(0.35, 0.40, 0.52) * rain_streaks(uv, t, 1.0, 0.10, 1.3) * 0.18;
    col = col + vec3<f32>(0.30, 0.34, 0.46) * rain_streaks(uv * vec2<f32>(1.6, 1.3), t * 0.6, 1.0, 0.08, 1.3) * 0.10;
    // Shockwave highlight + whole-sky flash (primary + distant sheet flashes).
    let df = moment(t, 0.5, 0.2, 12.0);
    col = col + vec3<f32>(0.55, 0.60, 0.75) * ring * st.x * 0.5;
    col = col + vec3<f32>(0.60, 0.63, 0.78) * (flash * 0.18 + df.x * 0.08);
    // Bolts: primary + occasional double, with a soft afterglow trailing the primary.
    col = col + vec3<f32>(0.90, 0.92, 1.0) * lightning(uv, t, st);
    col = col + vec3<f32>(0.85, 0.88, 1.0) * lightning(uv, t, st2) * 0.8;
    let after = storm_strike(t);
    col = col + vec3<f32>(0.70, 0.74, 0.95) * lightning(uv, t, vec4<f32>(sqrt(after.x) * 0.25, after.y, after.z, after.w));
    return col;
}
```

- [ ] **Step 2: Fog — three banks, roll moment, diffusion halo**

```wgsl
// Three counter-scrolling banks: sharp/fast -> soft/slow, at distinct scales.
fn fog_banks(uv: vec2<f32>, t: f32) -> f32 {
    let n1 = fbm(uv * vec2<f32>(3.0, 1.6) + vec2<f32>(t * 0.06, 0.0));
    let n2 = fbm3(uv * vec2<f32>(1.8, 1.0) + vec2<f32>(-t * 0.04, 1.7));
    let n3 = fbm3(uv * vec2<f32>(1.1, 0.7) + vec2<f32>(t * 0.02, 3.9));
    return 0.42 * n1 + 0.33 * n2 + 0.25 * n3;
}

fn fog_c(uv: vec2<f32>, t: f32, sky: Sky, intensity: f32) -> vec3<f32> {
    let day = sky.daylight;
    let c0 = mix(vec3<f32>(0.16, 0.17, 0.19), vec3<f32>(0.66, 0.68, 0.71), day);
    let c1 = mix(vec3<f32>(0.19, 0.20, 0.22), vec3<f32>(0.74, 0.76, 0.79), day);
    let c2 = mix(vec3<f32>(0.17, 0.18, 0.20), vec3<f32>(0.70, 0.72, 0.75), day);
    let c3 = mix(vec3<f32>(0.14, 0.15, 0.17), vec3<f32>(0.62, 0.64, 0.67), day);
    var col = mesh_gradient(uv, t, c0, c1, c2, c3);
    let fogc = mix(vec3<f32>(0.55, 0.57, 0.60), vec3<f32>(0.86, 0.88, 0.90), day);
    let banks = fog_banks(uv, t);
    // Fog-roll moment: a dense bank drifts across; visibility drops then recovers.
    let mr = moment(t, 0.06, 0.5, 13.0);
    let roll = mr.x * smoothstep(0.35, 0.0, abs(uv.x - (mr.y * 1.4 - 0.2))) * 0.35;
    // Denser low + toward the horizon; distant content fades hardest.
    var dens = banks * (0.5 + 0.7 * intensity) + smoothstep(0.3, 1.0, uv.y) * 0.35 + roll;
    dens = clamp(dens, 0.0, 0.92);
    col = mix(col, fogc, dens);
    // Light-diffusion halo where the sun sits behind the fog (day only).
    let lp = light_pos(t, u.sun);
    let asp = u.res.y / u.res.x;
    let pd = length((uv - lp) * vec2<f32>(1.0, asp));
    col = col + sky.key * smoothstep(0.5, 0.0, pd) * 0.10 * day;
    return col;
}
```

- [ ] **Step 3: Build + launch + eyeball loop**

Checklist (storm + fog):
- Storm: churn planes visibly boil in the upper sky and light up from within on strikes; occasional double-strike (two bolts ~0.15 s apart); distant sheet flashes without bolts; afterglow softens the cutoff; shockwave + window-edge jolt still fire (silhouette regression).
- Fog: three distinguishable drift speeds; every ~15–30 s a roll sweeps through and visibly drops visibility; soft bright halo marks the hidden sun by day.

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): storm & fog revamp — churn planes, double-strike/distant-flash, layered banks + fog-roll moment"
```

---

### Task 6: UI sympathy pass (skin.lua + ui_scrim retune)

**Files:**
- Modify: `weather/skins/weather/skin.lua`
- Modify: `weather/skins/weather/assets/weather.wgsl` (`ui_scrim` only)

**Interfaces:**
- Consumes: nothing new. Layout structure (hero/hourly/daily positions, list template shape) does NOT change.
- Produces: final text colors/sizes; retuned `ui_scrim`.

- [ ] **Step 1: Two-tier color system in skin.lua**

Define at the top and use throughout (starting values; tuned against the new palettes in Step 3):

```lua
-- Two-tier text colors: primary = near-white, secondary = softened toward the sky.
local PRI = { r = 252, g = 253, b = 255 }
local SEC = { r = 208, g = 218, b = 236 }
```

- Hero: `location` 26 PRI · `condition_text` 14 SEC · `temp_now` 72 PRI · `hi_lo`/`feels` 14 SEC. Nudge `condition_text` y 74 → 76 and `hi_lo`/`feels` y 196 → 198 (breathing room).
- Hourly: `_time` 11 SEC · `_temp` 13 PRI (same positions).
- Daily template: `day` 15 PRI · `glyph` unchanged · `hi` 15 PRI · `lo` 15 `{ r = 228, g = 234, b = 246 }` (the M3 low-contrast fix — brighter than SEC).

- [ ] **Step 2: Retune ui_scrim (softer, gradient-shaped)**

```wgsl
// Softly darkens the shader behind the 2D UI so text stays legible (the engine has no
// text-shadow/scrim primitive). Retuned for the graded palettes: shallower, wider falloffs
// so the scrim disappears into the scene. Zones (canvas 400x680, uv normalized).
fn ui_scrim(uv: vec2<f32>) -> f32 {
    var s = 1.0;
    // Hero block (top-left): strongest, with a long soft tail.
    s = s - 0.34 * smoothstep(0.66, 0.24, uv.x) * smoothstep(0.36, 0.0, uv.y);
    // Hourly strip band: gentle.
    s = s - 0.16 * smoothstep(0.30, 0.355, uv.y) * smoothstep(0.43, 0.375, uv.y);
    // Daily columns (left + right thirds).
    let band = smoothstep(0.43, 0.49, uv.y) * smoothstep(0.84, 0.78, uv.y);
    s = s - 0.14 * band * (smoothstep(0.34, 0.04, uv.x) + smoothstep(0.66, 0.96, uv.x));
    return clamp(s, 0.55, 1.0);
}
```

- [ ] **Step 3: Build + launch + eyeball loop**

All 6 conditions × 4 D stops, checking ONLY text: every string readable on its worst-case background (bright noon clear sky for `lo` temps is the known killer; storm flash moments for hero text). Tune PRI/SEC and scrim strengths until legible everywhere without visible scrim rectangles.

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/skin.lua weather/skins/weather/assets/weather.wgsl
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): UI sympathy pass — two-tier text colors, lo-temp contrast fix, softer gradient ui_scrim"
```

---

### Task 7: Performance gate + full verification matrix

**Files:**
- Modify (temporarily): `weather/Sources/Weather/SkinView.swift` (perf probe — removed before commit)

**Interfaces:** none.

- [ ] **Step 1: Frame-time probe**

Add temporarily inside `SkinView`'s frame-present path (the method `CarapaceBridge.onFrame` calls, `show(surface:index:)`):

```swift
    // TEMP perf probe: frame_ready inter-arrival. The render thread free-runs at 60fps;
    // sustained arrivals > 16.7ms mean the shader is the bottleneck.
    private static var perfLast = CACurrentMediaTime()
    private static var perfDeltas: [Double] = []
    // ... at the top of show(surface:index:):
    let now = CACurrentMediaTime()
    Self.perfDeltas.append(now - Self.perfLast)
    Self.perfLast = now
    if Self.perfDeltas.count == 600 {
        let s = Self.perfDeltas.sorted()
        print(String(format: "frame p50 %.2fms p95 %.2fms", s[300] * 1000, s[570] * 1000))
        Self.perfDeltas.removeAll()
    }
```

- [ ] **Step 2: Measure every condition**

Build, launch from a terminal that captures stdout. For each of the 6 conditions (→ key), let it run ≥ 2 windows (20 s). Record p50/p95 per condition.
Expected: p95 ≤ 17 ms on every condition (60 fps gate). **If a condition fails:** reduce its cost in this order — far-plane fbm→fbm3 swaps, drop god-ray march count 24→16, remove the billow warp on the far cloud plane — re-measure, and note the trade in the commit message.

- [ ] **Step 3: Remove the probe**

Delete the probe code. `git diff weather/Sources/Weather/SkinView.swift` must be empty.

- [ ] **Step 4: Full matrix eyeball + GIF captures**

- Screenshot matrix: 6 conditions × 4 D stops = 24 shots (drive via osascript key codes, capture via window-bounds `screencapture`). Review each: distinct condition identity, golden-hour warmth at dawn/dusk stops, no banding, no blowouts, text legible.
- GIFs (screen-record ~10 s with `screencapture -v`, convert via ffmpeg, as in the M3 redesign): rain gust, snow flurry, cloud-break, shooting star (clear night), storm double-strike, fog roll. Send to the user.
- Regression: →/← cycles + returns to live; S season stops; R refetch crash-free; window drag works; silhouette band flows per condition; storm edge-jolt fires.

- [ ] **Step 5: Full gate + docs check**

```bash
cd weather && swift build && swift test 2>&1 | tail -3   # all green
git status --porcelain                                    # ONLY intended files
git diff main --stat | grep crates && echo "ENGINE DIFF — STOP" || echo "zero engine changes ✓"
grep -rn "is_day\|isDay" weather/ --include="*.swift" --include="*.lua" --include="*.wgsl" | grep -v "Response\|query\|mock.json"   # no stale refs (Response.Current.is_day + the current= query line are the only survivors)
```

Check `docs/api/` skin-authoring shader docs: if any example references `is_day` as a weather uniform, update it (the `shader{}` primitive docs are engine-level and likely untouched).

- [ ] **Step 6: Commit any final tuning + push branch**

```bash
git add weather/ docs/
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(weather): perf-gate the revamp at 60fps + final tuning"
git push -u origin weather-app-shader-revamp
```

Then open a draft PR (no Claude attribution footer) targeting `main`.

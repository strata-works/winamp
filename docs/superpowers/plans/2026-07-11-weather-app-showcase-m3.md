# Weather App Showcase — Milestone 3 (Shader Polish + Flowing Silhouette) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the weather app look finished and distinctive — six polished condition shaders that read well day and night, a subtle season tint, and the signature **bottom-flowing, condition-reactive silhouette** that shapes the transparent window via the shader's own alpha. Plus presenter keys (`D` day/night, `S` season) and a chrome-free window (no traffic lights).

**Architecture:** Almost entirely `weather/skins/weather/weather.wgsl` (the shader rewrite) plus small Swift changes: `WeatherHost` gains `isDayOverride`/`seasonOverride` (lock-guarded, like M2's `conditionOverride`); a generalized cycle helper drives all three overrides; `App.swift` hides the traffic lights, sets `hasShadow=false`, and adds `D`/`S` keys; `skin.lua` tightens the daily list to clear the bottom silhouette band. **Zero engine-crate changes** — the transparent window is shaped automatically by sub-1 shader alpha through the existing 4-stage composite (validated by a Task 1 spike).

**Tech Stack:** WGSL (naga-validated at skin load), Swift 6 (AppKit), Lua (skin), carapace-ffi (C ABI 3.x — unchanged).

## Global Constraints

- **Zero engine-crate changes.** All work is in `weather/Sources/Weather`, `weather/Tests`, and `weather/skins/weather` (`weather.wgsl` + `skin.lua`). Do NOT edit `crates/*`, `showcase/*`. If the Task 1 spike shows the silhouette needs an engine change, **STOP and escalate / re-scope** — do not edit `crates/*`.
- **Host-data contract UNCHANGED.** M3 adds `isDayOverride`/`seasonOverride` inside `WeatherHost` (invisible to the skin — it still reads `wx_is_day`/`wx_season`). No new keys. The data layer (`WeatherService`) and refresh logic are untouched.
- **uv orientation:** in `weather.wgsl`, `uv.y = 0` is the TOP of the canvas, `uv.y = 1` the BOTTOM (per the existing `sky()` gradient). The silhouette band is the bottom: `uv.y` ∈ ~`[0.82, 1.0]`.
- **Six conditions:** `0 clear · 1 cloud · 2 rain · 3 snow · 4 storm · 5 fog` (shader `switch` on `i32(u.condition)`).
- **Season:** `0 winter · 1 spring · 2 summer · 3 autumn`.
- **Overrides force ONLY their shader uniform** (`wx_condition`/`wx_is_day`/`wx_season`); string/row data stays live; a refresh sets `model` only, so overrides survive it.
- **Cycle order:** `nil (live) → 0 … upTo → nil`. Condition `upTo 5`, season `upTo 3`, day/night `upTo 1` (`live → night(0) → day(1) → live`).
- **macOS keycodes:** `→` 124, `←` 123, `R` 15, `D` 2, `S` 1.
- **Build order:** `cargo build -p carapace-ffi` before `swift build`/`swift test` in `weather/`.
- **Base:** branch `weather-app-showcase-m3` off `main` (commit `bb84bc5`, includes merged M2). Never commit to `main`.
- **Git identity:** Daniel Agbemava <danagbemava@gmail.com>. No Claude attribution in commits/PRs.

## File Structure

- `weather/skins/weather/weather.wgsl` — **rewrite** (Task 4): shared helpers → 6 polished condition colors (day/night) → `season_tint` → `silhouette_alpha` (per-condition edge) → `fs()` that dispatches, tints, and premultiplies alpha.
- `weather/skins/weather/skin.lua` — **modify** (Task 3): tighten the daily list so content ends above the bottom band.
- `weather/Sources/Weather/WeatherHost.swift` — **modify** (Task 2): add `isDayOverride` + `seasonOverride`; `num()` applies them to `wx_is_day`/`wx_season`.
- `weather/Sources/Weather/ConditionCycle.swift` — **modify** (Task 2): add generalized `next(_:upTo:)`/`prev(_:upTo:)`; keep the 1-arg condition helpers delegating to `upTo: 5`.
- `weather/Sources/Weather/App.swift` — **modify** (Task 3): remove traffic lights, `hasShadow=false`, add `D`/`S` keys.
- `weather/Tests/WeatherTests/ConditionOverrideTests.swift` — **modify** (Task 2): add is_day/season override + generalized-cycle tests.

---

### Task 1: Transparency spike — prove shader alpha shapes the window (GO/NO-GO)

Before authoring the real silhouette, prove the engine's 4-stage composite carries a shader-authored `alpha < 1` all the way to a transparent window region. **This is a throwaway spike**: the temporary edits are reverted at the end; only a findings note is committed.

**Files:**
- Temporarily edit (then REVERT): `weather/skins/weather/weather.wgsl`, `weather/Sources/Weather/App.swift`
- Create: `docs/superpowers/specs/2026-07-11-weather-app-showcase-m3-findings.md`

**Interfaces:** none (produces a GO/NO-GO decision).

- [ ] **Step 1: Temporarily punch a transparent hole in the shader**

In `weather/skins/weather/weather.wgsl`, change the final `return` of `fs()` from
`return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);` to a spike that makes the bottom 15% fully transparent:
```wgsl
    let spike_a = select(1.0, 0.0, in.uv.y > 0.85);
    let spike_col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)) * spike_a;
    return vec4<f32>(spike_col, spike_a);
```

- [ ] **Step 2: Temporarily disable the window shadow**

In `weather/Sources/Weather/App.swift`, change `window.hasShadow = true` to `window.hasShadow = false` (so a rectangular shadow doesn't mask the test).

- [ ] **Step 3: Build + launch over a contrasting background**

```bash
cargo build -p carapace-ffi && (cd weather && swift build)
pkill -f 'arm64-apple-macosx/debug/Weather' 2>/dev/null; sleep 1
launchctl asuser 501 /bin/zsh -lc 'cd /Users/nexus/projects/experiments/winamp/weather && exec .build/arm64-apple-macosx/debug/Weather' &
sleep 4
```
Bring `Weather` frontmost over a bright/contrasting window (e.g. a white window or the desktop), region-capture its 400×680 frame to `/tmp/m3-spike.png`.

- [ ] **Step 4: Evaluate GO/NO-GO (controller judges the screenshot)**

- **GO:** the bottom 15% of the window is genuinely see-through (the background shows through, not a black/opaque rectangle). Record GO.
- **NO-GO:** the bottom stays opaque (black or filled) → the composite flattens shader alpha. **STOP and escalate to the human** — the silhouette needs an engine change, which is out of M3 scope; the milestone must be re-scoped.

- [ ] **Step 5: Revert the temporary edits**

Restore `weather.wgsl`'s `fs()` return to the original `return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);` and (if you want the spike commit clean) leave `App.swift`'s `hasShadow` as it was; the real `hasShadow=false` lands in Task 3. Confirm `git diff` shows NO changes to `weather.wgsl`/`App.swift` after reverting.
```bash
git checkout -- weather/skins/weather/weather.wgsl weather/Sources/Weather/App.swift
```

- [ ] **Step 6: Record the finding + commit**

Create `docs/superpowers/specs/2026-07-11-weather-app-showcase-m3-findings.md` with the verdict (GO/NO-GO), the screenshot path, and one line on what was observed.
```bash
git add docs/superpowers/specs/2026-07-11-weather-app-showcase-m3-findings.md
git commit -m "spike(weather): confirm shader alpha shapes the transparent window (M3 GO/NO-GO)"
```

---

### Task 2: WeatherHost is_day/season overrides + generalized cycle (TDD)

Add two more presenter overrides mirroring M2's `conditionOverride`, and generalize the cycle helper so all three overrides share it.

**Files:**
- Modify: `weather/Sources/Weather/WeatherHost.swift`
- Modify: `weather/Sources/Weather/ConditionCycle.swift`
- Test: `weather/Tests/WeatherTests/ConditionOverrideTests.swift`

**Interfaces:**
- Consumes: `WeatherHost`, `ConditionCycle`, `WeatherModel` (existing).
- Produces:
  - `WeatherHost.isDayOverride: Double?`, `WeatherHost.seasonOverride: Double?` (thread-safe get/set).
  - `ConditionCycle.next(_ current: Double?, upTo max: Double) -> Double?` and `prev(_:upTo:)`; existing `next(_:)`/`prev(_:)` retained (delegate to `upTo: 5`).

- [ ] **Step 1: Write the failing tests**

Append to `weather/Tests/WeatherTests/ConditionOverrideTests.swift` (inside the existing `ConditionOverrideTests` class):
```swift
    func testIsDayOverrideForcesOnlyWxIsDay() {
        let host = WeatherHost(model: .sample)   // sample.isDay == 1
        XCTAssertEqual(host.num("wx_is_day"), 1)
        host.isDayOverride = 0
        XCTAssertEqual(host.num("wx_is_day"), 0)          // override wins
        XCTAssertEqual(host.num("wx_condition"), WeatherModel.sample.condition) // others live
        host.isDayOverride = nil
        XCTAssertEqual(host.num("wx_is_day"), 1)          // back to live
    }

    func testSeasonOverrideForcesOnlyWxSeason() {
        let host = WeatherHost(model: .sample)   // sample.season == 2
        XCTAssertEqual(host.num("wx_season"), 2)
        host.seasonOverride = 0
        XCTAssertEqual(host.num("wx_season"), 0)
        XCTAssertEqual(host.num("wx_temp"), WeatherModel.sample.temp)  // others live
        host.seasonOverride = nil
        XCTAssertEqual(host.num("wx_season"), 2)
    }

    func testGeneralizedCycleBounds() {
        XCTAssertEqual(ConditionCycle.next(nil, upTo: 1), 0)
        XCTAssertEqual(ConditionCycle.next(0, upTo: 1), 1)
        XCTAssertEqual(ConditionCycle.next(1, upTo: 1), nil)   // day/night wraps at 1
        XCTAssertEqual(ConditionCycle.next(3, upTo: 3), nil)   // season wraps at 3
        XCTAssertEqual(ConditionCycle.prev(nil, upTo: 3), 3)
        XCTAssertEqual(ConditionCycle.prev(0, upTo: 3), nil)
        // Existing 1-arg condition cycle still works (upTo 5):
        XCTAssertEqual(ConditionCycle.next(5), nil)
        XCTAssertEqual(ConditionCycle.next(nil), 0)
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo build -p carapace-ffi && (cd weather && swift test)`
Expected: FAIL to compile (`isDayOverride`/`seasonOverride`/`next(_:upTo:)` undefined).

- [ ] **Step 3: Generalize the cycle**

Replace the body of `weather/Sources/Weather/ConditionCycle.swift` with:
```swift
/// The presenter demo cycle over a shader override: `nil` (live) → 0 → 1 … → `upTo` → nil …
/// (`prev` reverses). Used for the condition (`upTo 5`), season (`upTo 3`), and day/night
/// (`upTo 1`) overrides. The 1-arg helpers are the condition cycle (`upTo 5`).
enum ConditionCycle {
    static func next(_ current: Double?, upTo max: Double) -> Double? {
        switch current {
        case .none: return 0
        case .some(let c) where c >= max: return nil
        case .some(let c): return c + 1
        }
    }

    static func prev(_ current: Double?, upTo max: Double) -> Double? {
        switch current {
        case .none: return max
        case .some(let c) where c <= 0: return nil
        case .some(let c): return c - 1
        }
    }

    static func next(_ current: Double?) -> Double? { next(current, upTo: 5) }
    static func prev(_ current: Double?) -> Double? { prev(current, upTo: 5) }
}
```

- [ ] **Step 4: Add the two overrides to `WeatherHost`**

Edit `weather/Sources/Weather/WeatherHost.swift`:

1. Add backing fields beside `_conditionOverride` (after line `private var _conditionOverride: Double?`):
```swift
    private var _isDayOverride: Double?
    private var _seasonOverride: Double?
```

2. Add two lock-guarded computed properties right after the existing `conditionOverride` property (reusing the same `lock`):
```swift
    /// Presenter override for the shader day/night uniform only (the `D` key). Lock-guarded like
    /// `model`; `nil` = live. Forces only `wx_is_day`.
    var isDayOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _isDayOverride }
        set { lock.lock(); _isDayOverride = newValue; lock.unlock() }
    }

    /// Presenter override for the shader season uniform only (the `S` key). Lock-guarded like
    /// `model`; `nil` = live. Forces only `wx_season`.
    var seasonOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _seasonOverride }
        set { lock.lock(); _seasonOverride = newValue; lock.unlock() }
    }
```

3. Change the `wx_is_day` and `wx_season` cases in `num(_:)` to apply the overrides:
```swift
        case "wx_is_day":    return isDayOverride ?? model.isDay
        case "wx_temp":      return model.temp
        case "wx_intensity": return model.intensity
        case "wx_season":    return seasonOverride ?? model.season
```
(Leave `wx_condition`, `wx_temp`, `wx_intensity`, the default, and all of `str`/`rowCount`/`rowString` untouched.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo build -p carapace-ffi && (cd weather && swift test)`
Expected: PASS (new override + cycle tests green; existing `ConditionOverrideTests`, `WeatherServiceTests`, `WeatherHostTests` still green).

- [ ] **Step 6: Commit**

```bash
git add weather/Sources/Weather/WeatherHost.swift weather/Sources/Weather/ConditionCycle.swift weather/Tests/WeatherTests/ConditionOverrideTests.swift
git commit -m "feat(weather): is_day + season presenter overrides + generalized cycle (TDD)"
```

---

### Task 3: App chrome (no traffic lights, no shadow, D/S keys) + skin layout

Make the window chrome-free and add the day/night + season presenter keys; tighten the daily list to clear the bottom silhouette band.

**Files:**
- Modify: `weather/Sources/Weather/App.swift`
- Modify: `weather/skins/weather/skin.lua`

**Interfaces:**
- Consumes: `WeatherHost.isDayOverride`/`seasonOverride` + `ConditionCycle.next(_:upTo:)` (Task 2).

- [ ] **Step 1: Hide the traffic lights**

Edit `weather/Sources/Weather/App.swift`:

1. Remove the `installTrafficLights()` call (the line `installTrafficLights()` inside `applicationDidFinishLaunching`).
2. Delete the entire `installTrafficLights()` method.
3. Delete the now-unused stored property `private var trafficLightButtons: [NSButton] = []`.

- [ ] **Step 2: Drop the window shadow**

In `applicationDidFinishLaunching`, change `window.hasShadow = true` to `window.hasShadow = false`.

- [ ] **Step 3: Add the D/S presenter keys**

Replace the `handleKey(_:)` method body with (adds `D`/`S`; migrates nothing else):
```swift
    // Presenter controls (overrides force only the shader; hero/hourly/daily text stays live):
    //   →/← tour condition · D toggles day/night · S cycles season · R refetches.
    private func handleKey(_ code: UInt16) {
        switch code {
        case 124: host.conditionOverride = ConditionCycle.next(host.conditionOverride)          // →
        case 123: host.conditionOverride = ConditionCycle.prev(host.conditionOverride)          // ←
        case 2:   host.isDayOverride = ConditionCycle.next(host.isDayOverride, upTo: 1)          // D
        case 1:   host.seasonOverride = ConditionCycle.next(host.seasonOverride, upTo: 3)        // S
        case 15:  refresh()                                                                       // R
        default:  break
        }
    }
```

- [ ] **Step 4: Tighten the daily list to clear the bottom band**

In `weather/skins/weather/skin.lua`, replace the `list{ ... }` block (the "Vertical daily forecast list" at the end) with a shorter one that ends by ~`y = 550` (the shader's silhouette band starts at `uv.y = 0.82` ≈ `y = 558`):
```lua
-- Vertical daily forecast list (collection = "daily"). Ends above the shader's bottom
-- silhouette band (uv.y > 0.82 ≈ y 558); shorter row_height keeps all 7 rows in the opaque zone.
list{ collection = "daily", x = 24, y = 312, w = W - 48, h = 238, row_height = 34,
      template = {
        { bind = "day",   x = 8,        y = 8, size = 15, color = { r = 240, g = 244, b = 252 } },
        { bind = "glyph", x = 120,      y = 6, size = 17, color = { r = 245, g = 240, b = 220 } },
        { bind = "hi",    right = 70,   y = 8, size = 15, color = { r = 245, g = 247, b = 252 } },
        { bind = "lo",    right = 10,   y = 8, size = 15, halign = "right", color = { r = 190, g = 198, b = 214 } },
      } }
```
(7 rows × 34 = 238, from `y = 312` → rows occupy `y 312…550`, leaving `550…680` for the flow.)

- [ ] **Step 5: Build**

Run: `cargo build -p carapace-ffi && (cd weather && swift build)`
Expected: `Build complete!` (skin still loads with the current opaque shader — the silhouette lands in Task 4).

- [ ] **Step 6: Launch + quick eyeball**

Launch (as in Task 1 Step 3). Confirm: no traffic-light buttons appear; the window still renders and drags; `⌘Q` quits; pressing `D`/`S`/`→` does not crash (the background may not visibly change until Task 4's shader reads the overrides differently, but the app must stay alive). The daily list sits above the bottom ~130 px.

- [ ] **Step 7: Commit**

```bash
git add weather/Sources/Weather/App.swift weather/skins/weather/skin.lua
git commit -m "feat(weather): chrome-free window (no traffic lights/shadow) + D/S presenter keys + list layout"
```

---

### Task 4: weather.wgsl rewrite — polished conditions + day/night + season tint + flowing silhouette

The creative core: rewrite the shader with refined per-condition looks, day/night depth, a subtle season tint, and the per-condition bottom-flowing alpha silhouette. Naga-validated at skin load; visually tuned in verification.

**Files:**
- Rewrite: `weather/skins/weather/weather.wgsl`

**Interfaces:**
- Consumes: the uniform contract (`u.time`, `u.res`, `u.condition`, `u.is_day`, `u.temp`, `u.intensity`, `u.season`).

- [ ] **Step 1: Rewrite the shader**

Replace the entire contents of `weather/skins/weather/weather.wgsl` with:
```wgsl
// ---- Noise helpers (value noise + fbm) ----
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
    for (var k = 0; k < 4; k = k + 1) { v = v + amp * noise2(q); q = q * 2.0; amp = amp * 0.5; }
    return v;
}

// ---- Sky gradient tinted by day/night ----
fn sky(uv: vec2<f32>, day: f32) -> vec3<f32> {
    let top_day = vec3<f32>(0.30, 0.55, 0.90);
    let bot_day = vec3<f32>(0.75, 0.85, 0.95);
    let top_night = vec3<f32>(0.02, 0.03, 0.10);
    let bot_night = vec3<f32>(0.06, 0.08, 0.18);
    let top = mix(top_night, top_day, day);
    let bot = mix(bot_night, bot_day, day);
    return mix(top, bot, uv.y);
}

// Faint stars for clear/less-cloudy night skies.
fn stars(uv: vec2<f32>, t: f32) -> f32 {
    let g = hash21(floor(uv * 120.0));
    let tw = 0.5 + 0.5 * sin(t * 3.0 + g * 40.0);
    return step(0.985, g) * tw;
}

fn clear_c(uv: vec2<f32>, t: f32, day: f32) -> vec3<f32> {
    var col = sky(uv, day);
    col = col + vec3<f32>(0.9, 0.92, 1.0) * stars(uv, t) * (1.0 - day) * 0.8;
    // Sun by day / moon by night, drifting slightly.
    let c = vec2<f32>(0.72, 0.24 + 0.02 * sin(t * 0.3));
    let d = distance(uv, c);
    let disc = smoothstep(0.14, 0.10, d);
    let glow = smoothstep(0.5, 0.0, d) * 0.35;
    let sun = mix(vec3<f32>(0.85, 0.90, 1.0), vec3<f32>(1.0, 0.95, 0.75), day);
    col = col + sun * (disc + glow);
    return col;
}

fn cloud_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = sky(uv, day) * 0.9;
    let n = fbm(uv * vec2<f32>(3.0, 2.0) + vec2<f32>(t * 0.05, 0.0));
    let cover = smoothstep(0.4, 0.8, n) * (0.5 + 0.5 * intensity);
    let cloud_day = vec3<f32>(0.82, 0.85, 0.90);
    let cloud_night = vec3<f32>(0.18, 0.20, 0.26);
    let cloud = mix(cloud_night, cloud_day, day);
    return mix(col, cloud, cover);
}

fn rain_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = cloud_c(uv, t, day, 0.8) * mix(0.55, 0.8, day);
    let sc = uv * vec2<f32>(60.0, 30.0) + vec2<f32>(uv.y * 8.0, -t * 12.0);
    let line = fract(sc.x + floor(sc.y) * 0.5);
    let streak = smoothstep(0.96, 1.0, 1.0 - abs(line - 0.5) * 2.0) * (0.3 + intensity);
    col = col + vec3<f32>(0.6, 0.7, 0.85) * streak * 0.28;
    return col;
}

fn snow_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let tintv = mix(vec3<f32>(0.55, 0.62, 0.85), vec3<f32>(0.9, 0.93, 1.0), day);
    var col = cloud_c(uv, t, day, 0.5) * tintv;
    var flakes = 0.0;
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let p = uv * (10.0 + fk * 6.0) + vec2<f32>(sin(t * 0.5 + fk) * 0.5, t * (0.15 + fk * 0.05));
        let g = hash21(floor(p));
        let f = fract(p) - 0.5;
        flakes = flakes + smoothstep(0.08, 0.0, length(f)) * step(0.85, g);
    }
    return col + vec3<f32>(1.0) * flakes * (0.4 + intensity) * mix(0.7, 1.0, day);
}

fn storm_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = rain_c(uv, t, day, 1.0) * 0.5;
    let strike = step(0.985, hash21(vec2<f32>(floor(t * 2.0), 3.0)));
    let flash = strike * (0.5 + 0.5 * sin(t * 40.0)) * 0.6;
    col = col + vec3<f32>(0.9, 0.9, 1.0) * flash;
    return col;
}

fn fog_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let base = mix(vec3<f32>(0.30, 0.32, 0.36), vec3<f32>(0.80, 0.82, 0.85), day);
    let n = fbm(uv * 2.0 + vec2<f32>(t * 0.03, t * 0.01));
    return mix(base * 0.9, base, n);
}

// ---- Subtle season tint (multiplier, mixed at low strength) ----
fn season_tint(season: f32) -> vec3<f32> {
    let s = i32(round(clamp(season, 0.0, 3.0)));
    if (s == 0) { return vec3<f32>(0.86, 0.93, 1.06); }   // winter: cool
    if (s == 1) { return vec3<f32>(0.93, 1.05, 0.95); }   // spring: fresh green
    if (s == 2) { return vec3<f32>(1.08, 1.00, 0.90); }   // summer: warm
    return vec3<f32>(1.08, 0.95, 0.80);                    // autumn: amber
}

// ---- Bottom-flowing silhouette: window alpha 1 above the band, ramping to 0 below an
//      animated, condition-reactive edge. ----
fn silhouette_alpha(uv: vec2<f32>, t: f32, cond: i32, intensity: f32) -> f32 {
    let band_top = 0.82;
    if (uv.y < band_top) { return 1.0; }
    let x = uv.x;
    let b = (uv.y - band_top) / (1.0 - band_top);   // 0 at band top, 1 at canvas bottom
    let amp = 0.10 + 0.10 * intensity;
    var edge = 0.4;
    var soft = 0.10;
    if (cond == 0) {                                 // clear: gentle sine waves
        edge = 0.42 + amp * sin(x * 8.0 + t * 0.8);
    } else if (cond == 1) {                          // cloud: soft low swells
        edge = 0.46 + amp * 0.7 * sin(x * 5.0 + t * 0.5);
    } else if (cond == 2) {                          // rain: downward drips
        let drip = fbm(vec2<f32>(x * 12.0, t * 0.6));
        edge = 0.30 + amp * 1.4 * drip;
        soft = 0.05;
    } else if (cond == 3) {                          // snow: crystalline scallops
        edge = 0.42 + amp * 0.8 * abs(sin(x * 10.0 + t * 0.3));
        soft = 0.08;
    } else if (cond == 4) {                          // storm: jagged/erratic
        let j = fbm(vec2<f32>(x * 20.0 + t * 1.5, t));
        edge = 0.35 + amp * 1.6 * (j - 0.5) * 2.0;
        soft = 0.03;
    } else {                                         // fog (5): soft dissolve, no hard edge
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
        case 0: { col = clear_c(uv, t, day); }
        case 1: { col = cloud_c(uv, t, day, intensity); }
        case 2: { col = rain_c(uv, t, day, intensity); }
        case 3: { col = snow_c(uv, t, day, intensity); }
        case 4: { col = storm_c(uv, t, day, intensity); }
        case 5: { col = fog_c(uv, t, day, intensity); }
        default: { col = clear_c(uv, t, day); }
    }
    // Warm/cool tint from temperature (raw °C).
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.15;
    // Subtle season tint.
    col = mix(col, col * season_tint(u.season), 0.08);
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    // Bottom-flowing silhouette → premultiplied alpha shapes the transparent window.
    let a = silhouette_alpha(uv, t, cond, intensity);
    return vec4<f32>(col * a, a);
}
```

- [ ] **Step 2: Build (naga validates the shader at skin load in the eyeball step)**

Run: `cargo build -p carapace-ffi && (cd weather && swift build)`
Expected: `Build complete!` (swift build does not load the skin; validation happens at launch).

- [ ] **Step 3: Launch + full visual eyeball**

Launch (as in Task 1 Step 3). If `carapace_create` fails, the fatalError prints the naga/skin error — fix the WGSL and rebuild. Then verify over a contrasting background so transparency is visible:
- **The bottom edge is a transparent, animated silhouette** (the window is see-through and flowing there, not a rectangle) — the signature check.
- Press **→** through all six conditions: each is recognizable and the silhouette edge changes character (waves/swells/drips/scallops/jagged/dissolve).
- Press **D**: the scene toggles live→night→day→live — night is visibly darker/cooler across conditions.
- Press **S**: the palette shifts subtly through the four seasons.
- The hero/hourly/daily **text stays the live weather** through all of the above.
Capture screenshots of a few representative looks (e.g. clear-day, rain-night, storm) showing the flowing bottom.

- [ ] **Step 4: Commit**

```bash
git add weather/skins/weather/weather.wgsl
git commit -m "feat(weather): shader — polished conditions + day/night + season tint + flowing silhouette"
```

---

### Task 5: Milestone-3 gate + PR

**Files:** none (verification + push).

- [ ] **Step 1: Full local gate**

Run:
```bash
cargo build -p carapace-ffi
cd weather && swift build && swift test
```
Expected: dylib built; `Build complete!`; all tests pass (`ConditionOverrideTests` incl. the new override/cycle tests, `WeatherServiceTests`, `WeatherHostTests`).

- [ ] **Step 2: Final eyeball**

Re-run the Task 4 launch/eyeball on the built binary: the flowing silhouette is transparent and condition-reactive, all six conditions read well, `D`/`S` tour day/night + season with live text, `R` refetches, the window drags, `⌘Q` quits.

- [ ] **Step 3: Push + draft PR**

```bash
git push -u origin weather-app-showcase-m3
gh pr create --draft --base main --head weather-app-showcase-m3 \
  --title "feat(weather): weather app showcase — Milestone 3 (shader polish + flowing silhouette)" \
  --body "Implements Milestone 3 of docs/superpowers/specs/2026-07-11-weather-app-showcase-m3-design.md: the six condition shaders are polished for day and night, a subtle season tint is added, and the window's bottom edge is now an animated, condition-reactive **silhouette** shaped by the shader's own alpha (waves/swells/drips/scallops/jagged/dissolve per condition). The window is chrome-free (no traffic lights, hasShadow=false; ⌘Q to quit) and gains presenter keys D (day/night) and S (season) alongside M2's →/← and R. A Task-1 spike confirmed the engine's 4-stage composite carries shader alpha<1 through to a transparent window (zero engine changes). Host-data contract, data layer, and refresh logic unchanged. Follow-up: M4 location search cutout + geocoding."
```

---

## Self-Review

**Spec coverage:**
- Alpha-shaped window + Task-1 spike GO/NO-GO → Task 1. ✓
- Bottom-flowing silhouette, per-condition edges (waves/swells/drips/scallops/jagged/dissolve) → Task 4 `silhouette_alpha`. ✓
- Finish six condition looks + day/night depth → Task 4 (refined helpers, night palettes, stars). ✓
- Subtle season tinting → Task 4 `season_tint` mixed at 0.08. ✓
- `isDayOverride`/`seasonOverride` (lock-guarded, force only their uniform) + generalized cycle → Task 2. ✓
- Hide traffic lights, `hasShadow=false`, `D`/`S` keys → Task 3. ✓
- skin.lua layout clears the bottom band → Task 3. ✓
- Host-data contract / data layer unchanged → no `WeatherService`/contract edits in any task. ✓
- Zero engine changes; escalate if spike NO-GO → Task 1 Step 4. ✓
- Deferred (M4): search cutout, geocoding → not in any task. ✓

**Placeholder scan:** No "TBD/TODO/handle appropriately". Task 4 ships a complete shader (visually tuned in Step 3, which is verification, not a placeholder). Task 1's spike edits are explicitly reverted.

**Type consistency:** `WeatherHost.isDayOverride`/`seasonOverride: Double?` (Task 2) match `host.isDayOverride`/`host.seasonOverride` in App.swift (Task 3) and the tests. `ConditionCycle.next(_:upTo:)`/`prev(_:upTo:)` (Task 2) match Task 3's `handleKey` (`upTo: 1` for D, `upTo: 3` for S) and the tests; the 1-arg `next(_:)`/`prev(_:)` retained for condition still match App's existing `→`/`←` calls. `num()`'s `wx_is_day`/`wx_season` cases (Task 2) apply the overrides; `wx_condition` keeps M2's override; all string/row methods untouched. Shader uniform names (`u.condition/is_day/temp/intensity/season`) and the skin's `uniforms{}` bindings are unchanged. Keycodes D=2/S=1/R=15/→124/←123 consistent between Task 3 and the constraints.

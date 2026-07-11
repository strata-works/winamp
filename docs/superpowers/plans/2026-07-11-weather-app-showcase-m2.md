# Weather App Showcase — Milestone 2 (Live Data) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the weather app's static `WeatherHost(model: .sample)` with **live Open-Meteo weather** for a fixed location (Accra) — fetched on launch, auto-refreshed every 15 min plus a manual key, with a bundled-JSON offline fallback that reuses the live decode path, and a presenter demo-cycle (`→`/`←`) that tours the six shader looks without disturbing the live text.

**Architecture:** A new pure-ish `WeatherService` (Codable Open-Meteo structs + `decode`/`map` + `fetch()`/fallback) turns JSON into the existing `WeatherModel`. `WeatherHost` gains a lock-guarded `conditionOverride: Double?` so only the shader's `wx_condition` uniform can be overridden while all other keys stay live. `App.swift` wires the async fetch, a 15-min `Timer`, an `R` refresh key, and the `→`/`←` override cycle (replacing M1's throwaway debug key). **Zero engine-crate changes; zero skin changes; the host-data contract is unchanged.**

**Tech Stack:** Swift 6 (SwiftPM, AppKit), `URLSession` async, `Codable`, `Bundle.module` resources, XCTest, carapace-ffi (C ABI 3.x — unchanged).

## Global Constraints

- **Zero engine-crate changes** and **zero skin changes.** All work is under `weather/Sources/Weather`, `weather/Tests`, one bundled `weather/Sources/Weather/mock.json`, and `weather/Package.swift` (resource wiring). Do NOT edit `crates/*`, `showcase/*`, or any file under `weather/skins/`. If something seems to require an engine or skin change, STOP and escalate.
- **Host-data contract is UNCHANGED from M1.** The skin already reads every key; M2 only changes where `WeatherHost`'s `WeatherModel` comes from and adds `conditionOverride`. Hourly temp cells stay **strings** (`get_str`, e.g. `"28°"`) — do NOT switch them to numeric.
- **Base:** branch `weather-app-showcase-m2` off `main` (commit `7453318`, which includes the merged M1 weather app). Never commit to `main`.
- **Build order:** `cargo build -p carapace-ffi` (produces `target/debug/libcarapace_ffi.dylib` the app links) BEFORE `swift build`/`swift test` in `weather/`.
- **Location:** fixed **Accra, latitude `5.55`, longitude `-0.20`**, display name `"Accra"`. No geocoding (that is M4).
- **WMO code → condition bucket:** `0–1 → 0` clear · `2–3 → 1` cloud · `45,48 → 5` fog · `51–67,80–82 → 2` rain · `71–77,85–86 → 3` snow · `95–99 → 4` storm · anything else → `1` cloud (safe default).
- **`intensity` heuristic:** rain/snow/storm (buckets 2,3,4) → `clamp(precipitation / 8.0, 0.15, 1.0)`; cloud/fog (1,5) → `clamp(cloud_cover / 100.0, 0.0, 1.0)`; clear (0) → `clamp(cloud_cover / 100.0 * 0.3, 0.0, 0.3)`.
- **`season` from month** (N-hemisphere): 12,1,2 → `0` winter · 3,4,5 → `1` spring · 6,7,8 → `2` summer · 9,10,11 → `3` autumn.
- **Demo-cycle order:** `nil (live) → 0 → 1 → 2 → 3 → 4 → 5 → nil …`; `←` reverses.
- **macOS keycodes:** `→` = 124, `←` = 123, `R` = 15.
- **Git identity:** Daniel Agbemava <danagbemava@gmail.com>. No Claude attribution in commits/PRs.

## File Structure

- `weather/Sources/Weather/WeatherService.swift` — **new.** Codable Open-Meteo response structs, pure `map`/`decode`, static helpers (`wmoBucket`, `intensity`, `seasonForMonth`, `conditionText`, `glyph`, `weekdayAbbrev`, `hourLabel`), `loadBundledFixture()`, and `fetch() async`.
- `weather/Sources/Weather/mock.json` — **new.** A real Open-Meteo response for Accra; bundled resource AND the decoder test fixture.
- `weather/Sources/Weather/ConditionCycle.swift` — **new.** Pure `next`/`prev` cycle functions for the demo override.
- `weather/Sources/Weather/WeatherHost.swift` — **modify.** Add lock-guarded `conditionOverride: Double?`; `num("wx_condition")` returns `conditionOverride ?? model.condition`.
- `weather/Sources/Weather/App.swift` — **modify.** Live wiring: init from fixture, launch fetch, 15-min timer, `R` refresh, `→`/`←` override cycle (replace the debug `handleKey`).
- `weather/Package.swift` — **modify.** Add `resources: [.copy("mock.json")]` to the `Weather` target.
- `weather/Tests/WeatherTests/WeatherServiceTests.swift` — **new.** Decode-the-fixture + pure-helper tests.
- `weather/Tests/WeatherTests/ConditionOverrideTests.swift` — **new.** `conditionOverride` behavior + cycle tests.
- `weather/Tests/WeatherTests/WeatherHostTests.swift` — **unchanged** (must stay green).

---

### Task 1: WeatherService — Open-Meteo decode/map + bundled fixture + tests (TDD)

The pure data layer: Codable structs for the Open-Meteo response, a `map` that derives the existing `WeatherModel`, static mapping helpers, a bundled `mock.json` (fixture + offline fallback), and `fetch()`. Fully unit-testable except the live network path.

**Files:**
- Create: `weather/Sources/Weather/WeatherService.swift`
- Create: `weather/Sources/Weather/mock.json`
- Modify: `weather/Package.swift` (add the resource)
- Test: `weather/Tests/WeatherTests/WeatherServiceTests.swift`

**Interfaces:**
- Consumes: `WeatherModel`, `HourCell`, `DayRow` (existing, `weather/Sources/Weather/WeatherModel.swift`).
- Produces:
  - `struct WeatherService { init(latitude:Double=5.55, longitude:Double=-0.20, locationName:String="Accra"); var url: URL; func decode(_ data: Data) throws -> WeatherModel; func loadBundledFixture() throws -> WeatherModel; func fetch() async -> WeatherModel }`
  - static helpers: `WeatherService.wmoBucket(_ code:Int) -> Double`, `.intensity(bucket:Double, precipitation:Double, cloudCover:Double) -> Double`, `.seasonForMonth(_ month:Int) -> Double`, `.conditionText(bucket:Double) -> String`, `.glyph(bucket:Double) -> String`.

- [ ] **Step 1: Create the bundled fixture**

Create `weather/Sources/Weather/mock.json` (deterministic — the tests assert against these exact values):
```json
{
  "latitude": 5.55,
  "longitude": -0.20,
  "timezone": "Africa/Accra",
  "current": {
    "time": "2026-07-11T12:00",
    "temperature_2m": 27.0,
    "is_day": 1,
    "weather_code": 3,
    "apparent_temperature": 30.0,
    "precipitation": 0.0,
    "cloud_cover": 75
  },
  "hourly": {
    "time": ["2026-07-11T00:00","2026-07-11T01:00","2026-07-11T02:00","2026-07-11T03:00","2026-07-11T04:00","2026-07-11T05:00","2026-07-11T06:00","2026-07-11T07:00","2026-07-11T08:00","2026-07-11T09:00","2026-07-11T10:00","2026-07-11T11:00","2026-07-11T12:00","2026-07-11T13:00","2026-07-11T14:00","2026-07-11T15:00","2026-07-11T16:00","2026-07-11T17:00","2026-07-11T18:00","2026-07-11T19:00","2026-07-11T20:00","2026-07-11T21:00","2026-07-11T22:00","2026-07-11T23:00"],
    "temperature_2m": [24,24,23,23,24,25,25,26,26,27,27,27,27,28,28,29,29,28,27,26,26,25,25,24],
    "weather_code": [3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3]
  },
  "daily": {
    "time": ["2026-07-11","2026-07-12","2026-07-13","2026-07-14","2026-07-15","2026-07-16","2026-07-17"],
    "weather_code": [3,61,0,95,71,45,2],
    "temperature_2m_max": [31,30,33,28,5,20,30],
    "temperature_2m_min": [24,23,25,22,-1,12,23]
  }
}
```

- [ ] **Step 2: Wire the resource into Package.swift**

Edit `weather/Package.swift` — add a `resources:` argument to the `Weather` executable target (between `dependencies:` and `swiftSettings:`):
```swift
        .executableTarget(
            name: "Weather",
            dependencies: ["CCarapace"],
            resources: [.copy("mock.json")],
            swiftSettings: [
```
(Leave the rest of the target unchanged.)

- [ ] **Step 3: Write the failing tests**

Create `weather/Tests/WeatherTests/WeatherServiceTests.swift`:
```swift
import XCTest
@testable import Weather

final class WeatherServiceTests: XCTestCase {
    private let service = WeatherService()

    // Decodes the bundled fixture through the SAME path used for live data.
    private func decoded() throws -> WeatherModel { try service.loadBundledFixture() }

    func testDecodeScalarsAndStrings() throws {
        let m = try decoded()
        XCTAssertEqual(m.location, "Accra")
        XCTAssertEqual(m.condition, 1)          // code 3 -> cloud
        XCTAssertEqual(m.conditionText, "Partly cloudy")
        XCTAssertEqual(m.isDay, 1)
        XCTAssertEqual(m.temp, 27, accuracy: 0.001)
        XCTAssertEqual(m.tempNow, "27°")
        XCTAssertEqual(m.feels, "Feels 30°")
        XCTAssertEqual(m.hiLo, "H:31° L:24°")
        XCTAssertEqual(m.intensity, 0.75, accuracy: 0.001)   // cloud -> cloud_cover/100
        XCTAssertEqual(m.season, 2)             // July -> summer
    }

    func testDecodeHourlyWindowStartsAtCurrentHour() throws {
        let m = try decoded()
        XCTAssertEqual(m.hours.count, 12)
        XCTAssertEqual(m.hours[0].time, "12h")  // current.time is 12:00
        XCTAssertEqual(m.hours[0].temp, "27°")
        XCTAssertEqual(m.hours[11].time, "23h")
        XCTAssertEqual(m.hours[11].temp, "24°")
    }

    func testDecodeDailyRows() throws {
        let m = try decoded()
        XCTAssertEqual(m.days.count, 7)
        XCTAssertEqual(m.days[0].day, "Sat")    // 2026-07-11 is a Saturday
        XCTAssertEqual(m.days[0].glyph, "⛅")    // code 3 -> cloud
        XCTAssertEqual(m.days[0].hi, "31°")
        XCTAssertEqual(m.days[0].lo, "24°")
        XCTAssertEqual(m.days[4].glyph, "❄")     // code 71 -> snow
        XCTAssertEqual(m.days[4].hi, "5°")
        XCTAssertEqual(m.days[4].lo, "-1°")      // negative rounds correctly
    }

    func testWmoBucketMapping() {
        XCTAssertEqual(WeatherService.wmoBucket(0), 0)
        XCTAssertEqual(WeatherService.wmoBucket(1), 0)
        XCTAssertEqual(WeatherService.wmoBucket(2), 1)
        XCTAssertEqual(WeatherService.wmoBucket(3), 1)
        XCTAssertEqual(WeatherService.wmoBucket(45), 5)
        XCTAssertEqual(WeatherService.wmoBucket(48), 5)
        XCTAssertEqual(WeatherService.wmoBucket(61), 2)
        XCTAssertEqual(WeatherService.wmoBucket(82), 2)
        XCTAssertEqual(WeatherService.wmoBucket(71), 3)
        XCTAssertEqual(WeatherService.wmoBucket(86), 3)
        XCTAssertEqual(WeatherService.wmoBucket(95), 4)
        XCTAssertEqual(WeatherService.wmoBucket(99), 4)
        XCTAssertEqual(WeatherService.wmoBucket(1234), 1)   // unmapped -> cloud
    }

    func testIntensityHeuristic() {
        XCTAssertEqual(WeatherService.intensity(bucket: 2, precipitation: 16, cloudCover: 50), 1.0, accuracy: 0.001)
        XCTAssertEqual(WeatherService.intensity(bucket: 2, precipitation: 0.4, cloudCover: 50), 0.15, accuracy: 0.001)
        XCTAssertEqual(WeatherService.intensity(bucket: 1, precipitation: 0, cloudCover: 75), 0.75, accuracy: 0.001)
        XCTAssertEqual(WeatherService.intensity(bucket: 0, precipitation: 0, cloudCover: 100), 0.3, accuracy: 0.001)
    }

    func testSeasonForMonth() {
        XCTAssertEqual(WeatherService.seasonForMonth(1), 0)
        XCTAssertEqual(WeatherService.seasonForMonth(4), 1)
        XCTAssertEqual(WeatherService.seasonForMonth(7), 2)
        XCTAssertEqual(WeatherService.seasonForMonth(10), 3)
        XCTAssertEqual(WeatherService.seasonForMonth(12), 0)
    }

    func testUrlHasExpectedQuery() {
        let u = service.url.absoluteString
        XCTAssertTrue(u.contains("latitude=5.55"))
        XCTAssertTrue(u.contains("longitude=-0.2"))
        XCTAssertTrue(u.contains("current=temperature_2m,is_day,weather_code,apparent_temperature,precipitation,cloud_cover"))
        XCTAssertTrue(u.contains("hourly=temperature_2m,weather_code"))
        XCTAssertTrue(u.contains("daily=weather_code,temperature_2m_max,temperature_2m_min"))
        XCTAssertTrue(u.contains("timezone=auto"))
        XCTAssertTrue(u.contains("forecast_days=7"))
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo build -p carapace-ffi && (cd weather && swift test)`
Expected: FAIL to compile (`WeatherService` undefined).

- [ ] **Step 5: Implement `WeatherService`**

Create `weather/Sources/Weather/WeatherService.swift`:
```swift
import Foundation

/// Fetches Open-Meteo weather for a fixed location and maps it to `WeatherModel`.
/// Pure `map`/`decode` + static helpers are unit-testable; only `fetch()` hits the network.
struct WeatherService {
    let latitude: Double
    let longitude: Double
    let locationName: String

    init(latitude: Double = 5.55, longitude: Double = -0.20, locationName: String = "Accra") {
        self.latitude = latitude
        self.longitude = longitude
        self.locationName = locationName
    }

    // MARK: Open-Meteo response (property names match the JSON keys verbatim — do NOT use
    // convertFromSnakeCase; the digit in `temperature_2m` doesn't round-trip cleanly).
    struct Response: Decodable {
        struct Current: Decodable {
            let time: String
            let temperature_2m: Double
            let is_day: Int
            let weather_code: Int
            let apparent_temperature: Double
            let precipitation: Double
            let cloud_cover: Double
        }
        struct Hourly: Decodable {
            let time: [String]
            let temperature_2m: [Double]
            let weather_code: [Int]
        }
        struct Daily: Decodable {
            let time: [String]
            let weather_code: [Int]
            let temperature_2m_max: [Double]
            let temperature_2m_min: [Double]
        }
        let current: Current
        let hourly: Hourly
        let daily: Daily
    }

    // MARK: Request

    var url: URL {
        var c = URLComponents(string: "https://api.open-meteo.com/v1/forecast")!
        c.queryItems = [
            .init(name: "latitude", value: String(latitude)),
            .init(name: "longitude", value: String(longitude)),
            .init(name: "current", value: "temperature_2m,is_day,weather_code,apparent_temperature,precipitation,cloud_cover"),
            .init(name: "hourly", value: "temperature_2m,weather_code"),
            .init(name: "daily", value: "weather_code,temperature_2m_max,temperature_2m_min"),
            .init(name: "timezone", value: "auto"),
            .init(name: "forecast_days", value: "7"),
        ]
        return c.url!
    }

    // MARK: Decode + fetch

    func decode(_ data: Data) throws -> WeatherModel {
        let r = try JSONDecoder().decode(Response.self, from: data)
        return Self.map(r, locationName: locationName)
    }

    func loadBundledFixture() throws -> WeatherModel {
        guard let u = Bundle.module.url(forResource: "mock", withExtension: "json") else {
            throw NSError(domain: "WeatherService", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "bundled mock.json not found"])
        }
        return try decode(Data(contentsOf: u))
    }

    /// Always returns a valid model: live on success, bundled fixture on any failure, and the
    /// hardcoded `.sample` only if even the fixture is unreadable.
    func fetch() async -> WeatherModel {
        do {
            let (data, resp) = try await URLSession.shared.data(from: url)
            guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
                throw NSError(domain: "WeatherService", code: 2,
                              userInfo: [NSLocalizedDescriptionKey: "non-200 response"])
            }
            return try decode(data)
        } catch {
            return (try? loadBundledFixture()) ?? .sample
        }
    }

    // MARK: Mapping (pure)

    static func map(_ r: Response, locationName: String) -> WeatherModel {
        let bucket = wmoBucket(r.current.weather_code)
        let month = monthOf(r.current.time)
        let hours = hourlyCells(r.hourly, currentTime: r.current.time)
        let days = dailyRows(r.daily)
        return WeatherModel(
            location: locationName,
            conditionText: conditionText(bucket: bucket),
            condition: bucket,
            isDay: Double(r.current.is_day),
            temp: r.current.temperature_2m,
            intensity: intensity(bucket: bucket,
                                 precipitation: r.current.precipitation,
                                 cloudCover: r.current.cloud_cover),
            season: seasonForMonth(month),
            tempNow: "\(round(r.current.temperature_2m))°",
            hiLo: "H:\(round(r.daily.temperature_2m_max.first ?? 0))° L:\(round(r.daily.temperature_2m_min.first ?? 0))°",
            feels: "Feels \(round(r.current.apparent_temperature))°",
            hours: hours,
            days: days
        )
    }

    static func hourlyCells(_ h: Response.Hourly, currentTime: String) -> [HourCell] {
        let n = min(h.time.count, min(h.temperature_2m.count, 12_000))
        guard n > 0 else { return [] }
        // First hourly slot at/after the current hour; clamp so a 12-wide window always fits.
        let firstAtOrAfter = h.time.firstIndex(where: { $0 >= currentTime }) ?? 0
        let start = min(firstAtOrAfter, max(0, n - 12))
        let end = min(start + 12, n)
        return (start..<end).map { i in
            HourCell(time: "\(hourLabel(h.time[i]))h", temp: "\(round(h.temperature_2m[i]))°")
        }
    }

    static func dailyRows(_ d: Response.Daily) -> [DayRow] {
        let n = min(d.time.count, min(d.weather_code.count,
                     min(d.temperature_2m_max.count, d.temperature_2m_min.count)))
        return (0..<n).map { i in
            let b = wmoBucket(d.weather_code[i])
            return DayRow(day: weekdayAbbrev(d.time[i]),
                          glyph: glyph(bucket: b),
                          hi: "\(round(d.temperature_2m_max[i]))°",
                          lo: "\(round(d.temperature_2m_min[i]))°")
        }
    }

    // MARK: Static helpers (pure)

    static func wmoBucket(_ code: Int) -> Double {
        switch code {
        case 0, 1: return 0                       // clear
        case 2, 3: return 1                       // cloud
        case 45, 48: return 5                      // fog
        case 51...67, 80...82: return 2            // rain
        case 71...77, 85, 86: return 3             // snow
        case 95...99: return 4                     // storm
        default: return 1                          // safe default: cloud
        }
    }

    static func intensity(bucket: Double, precipitation: Double, cloudCover: Double) -> Double {
        switch bucket {
        case 2, 3, 4: return min(max(precipitation / 8.0, 0.15), 1.0)
        case 0:       return min(max(cloudCover / 100.0 * 0.3, 0.0), 0.3)
        default:      return min(max(cloudCover / 100.0, 0.0), 1.0)   // cloud/fog
        }
    }

    static func seasonForMonth(_ month: Int) -> Double {
        switch month {
        case 12, 1, 2: return 0    // winter
        case 3, 4, 5:  return 1    // spring
        case 6, 7, 8:  return 2    // summer
        default:       return 3    // autumn (9,10,11)
        }
    }

    static func conditionText(bucket: Double) -> String {
        switch Int(bucket) {
        case 0: return "Clear"
        case 1: return "Partly cloudy"
        case 2: return "Rain"
        case 3: return "Snow"
        case 4: return "Thunderstorm"
        case 5: return "Fog"
        default: return "Partly cloudy"
        }
    }

    static func glyph(bucket: Double) -> String {
        switch Int(bucket) {
        case 0: return "☀"
        case 1: return "⛅"
        case 2: return "☔"
        case 3: return "❄"
        case 4: return "⛈"
        case 5: return "🌫"
        default: return "⛅"
        }
    }

    /// "2026-07-11T14:00" -> 14 (the hour as an Int; drops leading zero).
    static func hourLabel(_ isoTime: String) -> Int {
        guard let tRange = isoTime.range(of: "T") else { return 0 }
        let after = isoTime[tRange.upperBound...]           // "14:00"
        let hh = after.prefix(2)                            // "14"
        return Int(hh) ?? 0
    }

    /// "2026-07-11" -> "Sat".
    static func weekdayAbbrev(_ isoDate: String) -> String {
        let inFmt = DateFormatter()
        inFmt.locale = Locale(identifier: "en_US_POSIX")
        inFmt.timeZone = TimeZone(identifier: "UTC")
        inFmt.dateFormat = "yyyy-MM-dd"
        guard let date = inFmt.date(from: isoDate) else { return "" }
        let outFmt = DateFormatter()
        outFmt.locale = Locale(identifier: "en_US_POSIX")
        outFmt.timeZone = TimeZone(identifier: "UTC")
        outFmt.dateFormat = "EEE"
        return outFmt.string(from: date)
    }

    /// "2026-07-11T12:00" -> 7 (the month as an Int).
    static func monthOf(_ isoTime: String) -> Int {
        // "2026-07-11..." -> chars 5..6 are the month.
        let s = Array(isoTime)
        guard s.count >= 7 else { return 1 }
        return Int(String(s[5...6])) ?? 1
    }

    private static func round(_ x: Double) -> Int { Int(x.rounded()) }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo build -p carapace-ffi && (cd weather && swift test)`
Expected: PASS (all `WeatherServiceTests` green; existing `WeatherHostTests` still green).

- [ ] **Step 7: Commit**

```bash
git add weather/Sources/Weather/WeatherService.swift weather/Sources/Weather/mock.json weather/Package.swift weather/Tests/WeatherTests/WeatherServiceTests.swift
git commit -m "feat(weather): WeatherService — Open-Meteo decode/map + bundled fixture (TDD)"
```

---

### Task 2: WeatherHost.conditionOverride + ConditionCycle (TDD)

Add the presenter override to `WeatherHost` (lock-guarded, like `model`) and the pure cycle helpers. Only the shader `wx_condition` uniform is affected; everything else stays live.

**Files:**
- Modify: `weather/Sources/Weather/WeatherHost.swift`
- Create: `weather/Sources/Weather/ConditionCycle.swift`
- Test: `weather/Tests/WeatherTests/ConditionOverrideTests.swift`

**Interfaces:**
- Consumes: `WeatherHost` (existing), `WeatherModel` (existing).
- Produces:
  - `WeatherHost.conditionOverride: Double?` (thread-safe get/set).
  - `enum ConditionCycle { static func next(_ current: Double?) -> Double?; static func prev(_ current: Double?) -> Double? }`

- [ ] **Step 1: Write the failing tests**

Create `weather/Tests/WeatherTests/ConditionOverrideTests.swift`:
```swift
import XCTest
@testable import Weather

final class ConditionOverrideTests: XCTestCase {
    func testOverrideForcesOnlyWxCondition() {
        let host = WeatherHost(model: .sample)   // sample.condition == 1
        XCTAssertEqual(host.num("wx_condition"), 1)   // no override -> live model

        host.conditionOverride = 4
        XCTAssertEqual(host.num("wx_condition"), 4)   // override wins
        // Every other key ignores the override:
        XCTAssertEqual(host.num("wx_temp"), WeatherModel.sample.temp)
        XCTAssertEqual(host.str("condition_text"), WeatherModel.sample.conditionText)

        host.conditionOverride = nil
        XCTAssertEqual(host.num("wx_condition"), 1)   // back to live
    }

    func testCycleForwardWrapsThroughLive() {
        XCTAssertEqual(ConditionCycle.next(nil), 0)
        XCTAssertEqual(ConditionCycle.next(0), 1)
        XCTAssertEqual(ConditionCycle.next(4), 5)
        XCTAssertEqual(ConditionCycle.next(5), nil)   // 5 -> live
    }

    func testCycleBackwardWrapsThroughLive() {
        XCTAssertEqual(ConditionCycle.prev(nil), 5)
        XCTAssertEqual(ConditionCycle.prev(5), 4)
        XCTAssertEqual(ConditionCycle.prev(1), 0)
        XCTAssertEqual(ConditionCycle.prev(0), nil)   // 0 -> live
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo build -p carapace-ffi && (cd weather && swift test)`
Expected: FAIL to compile (`conditionOverride` / `ConditionCycle` undefined).

- [ ] **Step 3: Add `conditionOverride` to `WeatherHost`**

Edit `weather/Sources/Weather/WeatherHost.swift`:

1. Add the backing field beside `_model` (just after the `private var _model: WeatherModel` line):
```swift
    private var _conditionOverride: Double?
```

2. Add a lock-guarded computed property right after the existing `model` computed property (reuse the same `lock`):
```swift
    /// Presenter demo override for the shader condition only. Set from the MAIN thread
    /// (the →/← keys); read from the RENDER thread in `num("wx_condition")`. Lock-guarded
    /// like `model`. `nil` = show the live condition.
    var conditionOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _conditionOverride }
        set { lock.lock(); _conditionOverride = newValue; lock.unlock() }
    }
```

3. Change ONLY the `wx_condition` case in `num(_:)` from `return model.condition` to:
```swift
        case "wx_condition": return conditionOverride ?? model.condition
```
(Leave every other case and method untouched.)

- [ ] **Step 4: Create `ConditionCycle`**

Create `weather/Sources/Weather/ConditionCycle.swift`:
```swift
/// The presenter demo cycle over the shader condition override:
/// `nil` (live) → 0 → 1 → 2 → 3 → 4 → 5 → nil …  (`prev` reverses).
enum ConditionCycle {
    static func next(_ current: Double?) -> Double? {
        switch current {
        case .none: return 0
        case .some(let c) where c >= 5: return nil
        case .some(let c): return c + 1
        }
    }

    static func prev(_ current: Double?) -> Double? {
        switch current {
        case .none: return 5
        case .some(let c) where c <= 0: return nil
        case .some(let c): return c - 1
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo build -p carapace-ffi && (cd weather && swift test)`
Expected: PASS (`ConditionOverrideTests` green; `WeatherHostTests` + `WeatherServiceTests` still green).

- [ ] **Step 6: Commit**

```bash
git add weather/Sources/Weather/WeatherHost.swift weather/Sources/Weather/ConditionCycle.swift weather/Tests/WeatherTests/ConditionOverrideTests.swift
git commit -m "feat(weather): WeatherHost.conditionOverride + demo cycle (TDD)"
```

---

### Task 3: App wiring — live fetch, refresh timer, refresh + demo-cycle keys

Replace M1's throwaway debug key and static model with the live service: init from the bundled fixture, fetch on launch, auto-refresh every 15 min, `R` to refetch, `→`/`←` to drive the override cycle.

**Files:**
- Modify: `weather/Sources/Weather/App.swift`

**Interfaces:**
- Consumes: `WeatherService` (Task 1), `WeatherHost.conditionOverride` + `ConditionCycle` (Task 2).

- [ ] **Step 1: Add the service + refresh plumbing to `AppDelegate`**

Edit `weather/Sources/Weather/App.swift`:

1. Add stored properties beside the existing ones (after `private var bridge: CarapaceBridge!`):
```swift
    private let service = WeatherService()
    private var refreshTimer: Timer?
```

2. Replace the model init line. Change:
```swift
        host = WeatherHost(model: .sample)
```
to:
```swift
        // First frame shows the bundled fixture immediately; the launch fetch replaces it.
        host = WeatherHost(model: (try? service.loadBundledFixture()) ?? .sample)
```

- [ ] **Step 2: Kick off the launch fetch + refresh timer**

In `applicationDidFinishLaunching`, immediately after `NSApp.activate(ignoringOtherApps: true)` (the last line of the method), add:
```swift

        refresh()  // launch fetch
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 15 * 60, repeats: true) { [weak self] _ in
            self?.refresh()
        }
```

- [ ] **Step 3: Add the `refresh()` helper**

Add this method to `AppDelegate` (e.g. right after `applicationDidFinishLaunching`):
```swift
    /// Fetch live weather off-main, then swap it into the host on the main thread. The host's
    /// `model` is lock-guarded, so the render thread's reads stay consistent across the swap.
    private func refresh() {
        Task { [weak self] in
            guard let self else { return }
            let model = await self.service.fetch()
            await MainActor.run { self.host.model = model }
        }
    }
```

- [ ] **Step 4: Replace the debug key handler with the M2 controls**

Replace the entire `handleKey(_:)` method (the M1 debug version, including its `// Debug (M1 verification scaffolding):` comment) with:
```swift
    // Presenter controls: →/← tour the six shader looks via a condition override (text stays
    // live); R refetches now.
    private func handleKey(_ code: UInt16) {
        switch code {
        case 124: host.conditionOverride = ConditionCycle.next(host.conditionOverride)  // →
        case 123: host.conditionOverride = ConditionCycle.prev(host.conditionOverride)  // ←
        case 15:  refresh()                                                             // R
        default:  break
        }
    }
```

- [ ] **Step 5: Build**

Run: `cargo build -p carapace-ffi && (cd weather && swift build)`
Expected: `Build complete!`

- [ ] **Step 6: Launch and eyeball (live data)**

The app opens a GUI window, so launch it into the Aqua session:
```bash
cargo build -p carapace-ffi && (cd weather && swift build)
pkill -f 'arm64-apple-macosx/debug/Weather' 2>/dev/null
launchctl asuser 501 /bin/zsh -lc 'cd /Users/nexus/projects/experiments/winamp/weather && exec .build/arm64-apple-macosx/debug/Weather' &
```
Then verify (bring the `Weather` process frontmost, then region-capture its window bounds):
- The window shows **real Accra weather** (temperature, condition text, hi/lo, hourly strip, and 7 daily rows populated from the API — values differ from the M1 sample). If offline, it shows the bundled fixture (Accra, 27°, partly cloudy) — that is the expected fallback, not a failure.
- Press **→** several times: the background tours `clear → cloud → rain → snow → storm → fog → live`, while the hero/hourly/daily **text stays the live weather** (only the shader changes).
- Press **←**: it steps back through the cycle.
- Press **R**: the app refetches (no visible change if the weather is unchanged; confirm it does not crash).

If `carapace_create` fails or the app crashes, capture the error and fix before committing.

- [ ] **Step 7: Commit**

```bash
git add weather/Sources/Weather/App.swift
git commit -m "feat(weather): live fetch + 15-min refresh + R key + →/← demo cycle"
```

---

### Task 4: Milestone-2 gate + PR

**Files:** none (verification + push).

- [ ] **Step 1: Full local gate**

Run:
```bash
cargo build -p carapace-ffi
cd weather && swift build && swift test
```
Expected: dylib built; `Build complete!`; all tests pass (`WeatherServiceTests`, `ConditionOverrideTests`, `WeatherHostTests`).

- [ ] **Step 2: Final eyeball**

Re-run the Task 3 launch/eyeball once more on the built binary: confirm live data renders, `→`/`←` tour the shaders with live text, `R` refetches, and the window drags + closes cleanly.

- [ ] **Step 3: Push + draft PR**

```bash
git push -u origin weather-app-showcase-m2
gh pr create --draft --base main --head weather-app-showcase-m2 \
  --title "feat(weather): weather app showcase — Milestone 2 (live Open-Meteo data)" \
  --body "Implements Milestone 2 of docs/superpowers/specs/2026-07-11-weather-app-showcase-m2-design.md: the weather app now renders real Open-Meteo weather for Accra instead of the static sample. New WeatherService (Codable Open-Meteo structs + pure decode/map + fetch with a bundled mock.json fallback that reuses the live decode path). WeatherHost gains a lock-guarded conditionOverride so the →/← keys tour the six shader looks while the hero/hourly/daily text stays live; R refetches; a 15-min timer auto-refreshes. Zero engine-crate changes, zero skin changes, host-data contract unchanged. Follow-ups: M3 condition-shader polish + bottom-flowing silhouette; M4 location search cutout + geocoding."
```

---

## Self-Review

**Spec coverage:**
- `WeatherService` (Open-Meteo current+hourly+daily fetch/decode + mock fallback) → Task 1. ✓
- `WeatherModel` derivation (WMO→condition, season from month, is_day from API, wx_* scalars, formatted strings, hourly/daily rows) → Task 1 `map`/helpers. ✓
- Bundled `mock.json` as resource + decoder fixture (same decode path) → Task 1 (`loadBundledFixture` used by both fallback and tests). ✓
- Refresh lifecycle: launch + 15-min timer + `R` key; failure → fixture, retry next tick → Task 3 (`refresh()`, `Timer`, `fetch()` swallows errors to the fixture). ✓
- Demo cycle `nil→0…5→nil`, override forces only `wx_condition`, refresh can't clobber → Task 2 (`conditionOverride`, `ConditionCycle`) + Task 3 (keys). Refresh sets `model`, never `conditionOverride`, so an active override survives a refresh. ✓
- Fixed Accra location; geocoding is M4 → Task 1 (`WeatherService` defaults). ✓
- Zero engine/skin changes; contract unchanged; hourly temp stays string → constraints honored (no skin/`crates` files touched; `WeatherModel` shape unchanged; `HourCell.temp` stays `String`). ✓
- Threading via M1's lock → Task 3 sets `host.model` on main; `WeatherHost.model`/`conditionOverride` lock-guarded. ✓
- Deferred (M3/M4): shader polish, bottom-flow, search cutout, `next_condition` as a skin action → not in any task (correctly out of scope). ✓

**Placeholder scan:** No "TBD/TODO/handle appropriately". Every code step shows complete code; `mock.json` is fully specified; the eyeball step (Task 3 Step 6) is a manual verification, not a placeholder.

**Type consistency:** `WeatherService.fetch() -> WeatherModel`, `decode(Data) -> WeatherModel`, `loadBundledFixture() -> WeatherModel` are used consistently in App.swift (Task 3) and tests (Task 1). Static helpers `wmoBucket(Int)->Double`, `intensity(bucket:precipitation:cloudCover:)->Double`, `seasonForMonth(Int)->Double`, `conditionText(bucket:)->String`, `glyph(bucket:)->String` match their test call sites (Task 1) exactly. `WeatherHost.conditionOverride: Double?` (Task 2) matches `host.conditionOverride` reads/writes in App.swift (Task 3) and the tests. `ConditionCycle.next/prev(Double?)->Double?` (Task 2) matches Task 3's `handleKey`. `WeatherModel`/`HourCell`/`DayRow` field names (`location/conditionText/condition/isDay/temp/intensity/season/tempNow/hiLo/feels/hours/days`, `time/temp`, `day/glyph/hi/lo`) match the existing struct in `WeatherModel.swift`. The host-data keys the skin reads are unchanged, so no skin edit is needed.

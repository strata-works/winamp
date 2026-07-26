# Weather App M4 — Location Search — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user search any city from an in-skin overlay (`L` to open, type, `↑↓` to pick, `⏎` to switch, `Esc` to cancel), refetch that city's weather, and remember it across launches — replacing the hardcoded Accra location.

**Architecture:** All work is host-side Swift + the Lua skin; **zero engine changes**. A pure `LocationSearch` state machine holds query/results/selection/status. A `Geocoder` (Open-Meteo, protocol + offline-decodable) resolves queries. `LocationStore` persists the chosen city in `UserDefaults`. `WeatherHost` exposes a `search_*` binding surface the skin reads each frame. The skin draws a scrim + card with `value_fill{}` gated on the host bool `search_active`, and all overlay text via `value=` bindings that return `""` when inactive. `AppDelegate` routes keystrokes by mode, debounces the geocode, and on commit rebuilds `WeatherService` and refetches.

**Tech Stack:** Swift 6 toolchain / language mode v5, AppKit, XCTest, Open-Meteo geocoding API, the carapace Lua skin DSL (`value_fill`, `rounded_rect`, `text`).

## Global Constraints

- **Zero engine changes.** `git diff main --stat` must touch no `crates/` paths. Verify every task.
- **Swift language mode v5**, platform **macOS 13** (set in `weather/Package.swift`; do not change).
- **Work only under `weather/`** (`Sources/Weather/`, `Tests/WeatherTests/`, `skins/weather/`) plus `docs/`. Do not edit `weather/skins/weather/assets/weather.wgsl` — the overlay needs no shader change.
- **Overlay gating rule:** the skin scene tree is built once at load; the only per-frame host levers are `text{ value = "<key>" }` (empty string skips drawing) and `value_fill{ value = "<key>" }` (0/false/missing draws nothing, 1/true fills the path). Text color and fill alpha are baked at load and **cannot** be host-driven. Never use a static `text{ text = "…" }` or a literal `fill{}` in the overlay — both would draw on every frame.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>` (use `git -c user.name=… -c user.email=…`).
- **Branch:** `weather-app-m4-location-search` (already created off `main`, already holds the design spec commit).
- **Gate before pushing:** `cd weather && swift build && swift test` all green; `git diff main --stat | grep crates` empty.
- **PR:** target `main`, no "Generated with Claude Code" footer.
- All shell commands below assume the working directory is `weather/` unless a path says otherwise.

---

### Task 1: `Geocoder` — Open-Meteo geocoding (GeoResult + decode + url)

**Files:**
- Create: `weather/Sources/Weather/Geocoder.swift`
- Test: `weather/Tests/WeatherTests/GeocoderTests.swift`

**Interfaces:**
- Consumes: nothing (leaf).
- Produces:
  - `struct GeoResult: Equatable { let name: String; let admin1: String?; let country: String?; let latitude: Double; let longitude: Double; var label: String }`
  - `protocol Geocoder { func search(_ query: String) async throws -> [GeoResult] }`
  - `struct OpenMeteoGeocoder: Geocoder` with `func url(_ query: String) -> URL` and `static func decode(_ data: Data) throws -> [GeoResult]`.

- [ ] **Step 1: Write the failing test**

Create `weather/Tests/WeatherTests/GeocoderTests.swift`:

```swift
import XCTest
@testable import Weather

final class GeocoderTests: XCTestCase {
    // Trimmed Open-Meteo /v1/search shape: one fully-populated row, one with no admin1.
    private let json = """
    { "results": [
        { "id": 1, "name": "Cape Town", "latitude": -33.9258, "longitude": 18.4232,
          "country": "South Africa", "admin1": "Western Cape" },
        { "id": 2, "name": "Singapore", "latitude": 1.2897, "longitude": 103.8501,
          "country": "Singapore" }
    ] }
    """.data(using: .utf8)!

    func testDecodeMapsRows() throws {
        let r = try OpenMeteoGeocoder.decode(json)
        XCTAssertEqual(r.count, 2)
        XCTAssertEqual(r[0], GeoResult(name: "Cape Town", admin1: "Western Cape",
                                       country: "South Africa", latitude: -33.9258, longitude: 18.4232))
        XCTAssertNil(r[1].admin1)
    }

    func testLabelFormatting() {
        XCTAssertEqual(GeoResult(name: "Cape Town", admin1: "Western Cape",
                                 country: "South Africa", latitude: 0, longitude: 0).label,
                       "Cape Town, Western Cape, South Africa")
        XCTAssertEqual(GeoResult(name: "Singapore", admin1: nil,
                                 country: "Singapore", latitude: 0, longitude: 0).label,
                       "Singapore, Singapore")
        XCTAssertEqual(GeoResult(name: "Nowhere", admin1: nil, country: nil,
                                 latitude: 0, longitude: 0).label, "Nowhere")
    }

    func testEmptyResponseDecodesToEmptyArray() throws {
        // Open-Meteo omits "results" entirely when there are no matches.
        let empty = #"{ "generationtime_ms": 0.5 }"#.data(using: .utf8)!
        XCTAssertEqual(try OpenMeteoGeocoder.decode(empty), [])
    }

    func testUrlHasExpectedQuery() {
        let u = OpenMeteoGeocoder().url("Cape Town").absoluteString
        XCTAssertTrue(u.hasPrefix("https://geocoding-api.open-meteo.com/v1/search"))
        XCTAssertTrue(u.contains("name=Cape%20Town"))
        XCTAssertTrue(u.contains("count=5"))
        XCTAssertTrue(u.contains("language=en"))
        XCTAssertTrue(u.contains("format=json"))
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `swift test --filter GeocoderTests`
Expected: FAIL — `cannot find 'OpenMeteoGeocoder' / 'GeoResult' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `weather/Sources/Weather/Geocoder.swift`:

```swift
import Foundation

/// One geocoding hit. `label` is what the search overlay shows: "City, Admin1, Country"
/// with absent/empty parts dropped.
struct GeoResult: Equatable {
    let name: String
    let admin1: String?
    let country: String?
    let latitude: Double
    let longitude: Double

    var label: String {
        [name, admin1, country]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
    }
}

/// Resolves a free-text query to candidate locations. `search` throws on transport/HTTP failure
/// (→ "Offline" in the UI) and returns `[]` for a valid empty response (→ "No matches").
protocol Geocoder {
    func search(_ query: String) async throws -> [GeoResult]
}

/// Open-Meteo geocoding (no API key; same provider as the forecast source).
struct OpenMeteoGeocoder: Geocoder {
    struct Response: Decodable {
        struct Row: Decodable {
            let name: String
            let latitude: Double
            let longitude: Double
            let country: String?
            let admin1: String?
        }
        let results: [Row]?
    }

    func url(_ query: String) -> URL {
        var c = URLComponents(string: "https://geocoding-api.open-meteo.com/v1/search")!
        c.queryItems = [
            .init(name: "name", value: query),
            .init(name: "count", value: "5"),
            .init(name: "language", value: "en"),
            .init(name: "format", value: "json"),
        ]
        return c.url!
    }

    static func decode(_ data: Data) throws -> [GeoResult] {
        let r = try JSONDecoder().decode(Response.self, from: data)
        return (r.results ?? []).map {
            GeoResult(name: $0.name, admin1: $0.admin1, country: $0.country,
                      latitude: $0.latitude, longitude: $0.longitude)
        }
    }

    func search(_ query: String) async throws -> [GeoResult] {
        let (data, resp) = try await URLSession.shared.data(from: url(query))
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
            throw NSError(domain: "Geocoder", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "non-200 response"])
        }
        return try Self.decode(data)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `swift test --filter GeocoderTests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/Weather/Geocoder.swift Tests/WeatherTests/GeocoderTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(weather): Open-Meteo geocoder (GeoResult + decode + url)"
```

---

### Task 2: `LocationSearch` — pure state machine

**Files:**
- Create: `weather/Sources/Weather/LocationSearch.swift`
- Test: `weather/Tests/WeatherTests/LocationSearchTests.swift`

**Interfaces:**
- Consumes: `GeoResult` (Task 1).
- Produces: `struct LocationSearch` (value type, no AppKit) with:
  - stored: `active: Bool`, `query: String`, `results: [GeoResult]`, `selected: Int`, `status: Status`
  - `enum Status { case idle, querying, results, empty, error }`
  - mutating: `enter()`, `exit()`, `typed(_ s: String)`, `backspace()`, `moveSelection(_ delta: Int)`, `setResults(_ r: [GeoResult], forQuery q: String)`, `setError(forQuery q: String)`
  - read-only: `func commit() -> GeoResult?`, `var statusLine: String`
  - `init()` yields an inactive machine (`active == false`).

- [ ] **Step 1: Write the failing test**

Create `weather/Tests/WeatherTests/LocationSearchTests.swift`:

```swift
import XCTest
@testable import Weather

final class LocationSearchTests: XCTestCase {
    private func r(_ name: String) -> GeoResult {
        GeoResult(name: name, admin1: nil, country: nil, latitude: 0, longitude: 0)
    }

    func testDefaultIsInactive() {
        let s = LocationSearch()
        XCTAssertFalse(s.active)
        XCTAssertEqual(s.status, .idle)
    }

    func testEnterActivatesAndResets() {
        var s = LocationSearch()
        s.enter()
        XCTAssertTrue(s.active)
        XCTAssertEqual(s.query, "")
        XCTAssertEqual(s.status, .idle)
    }

    func testTypedAppendsAndQuerying() {
        var s = LocationSearch(); s.enter()
        s.typed("c"); s.typed("a")
        XCTAssertEqual(s.query, "ca")
        XCTAssertEqual(s.status, .querying)
    }

    func testBackspaceToEmptyReturnsIdle() {
        var s = LocationSearch(); s.enter()
        s.typed("a")
        s.setResults([r("A")], forQuery: "a")
        s.backspace()
        XCTAssertEqual(s.query, "")
        XCTAssertEqual(s.results, [])
        XCTAssertEqual(s.status, .idle)
    }

    func testMoveSelectionClamps() {
        var s = LocationSearch(); s.enter()
        s.typed("x")
        s.setResults([r("A"), r("B")], forQuery: "x")
        XCTAssertEqual(s.selected, 0)
        s.moveSelection(-1)                 // clamp low
        XCTAssertEqual(s.selected, 0)
        s.moveSelection(1)
        XCTAssertEqual(s.selected, 1)
        s.moveSelection(1)                  // clamp high
        XCTAssertEqual(s.selected, 1)
    }

    func testStaleResultsIgnored() {
        var s = LocationSearch(); s.enter()
        s.typed("c"); s.typed("a"); s.typed("p")     // query == "cap"
        s.setResults([r("Cairo")], forQuery: "ca")   // stale (debounced earlier keystroke)
        XCTAssertEqual(s.results, [])
        XCTAssertEqual(s.status, .querying)
    }

    func testEmptyResultsSetEmptyStatus() {
        var s = LocationSearch(); s.enter()
        s.typed("z"); s.setResults([], forQuery: "z")
        XCTAssertEqual(s.status, .empty)
    }

    func testSetErrorStatus() {
        var s = LocationSearch(); s.enter()
        s.typed("z"); s.setError(forQuery: "z")
        XCTAssertEqual(s.status, .error)
        XCTAssertEqual(s.results, [])
    }

    func testCommitReturnsSelectedOnlyWhenResults() {
        var s = LocationSearch(); s.enter()
        s.typed("x")
        XCTAssertNil(s.commit())                          // querying, no results
        s.setResults([r("A"), r("B")], forQuery: "x")
        s.moveSelection(1)
        XCTAssertEqual(s.commit()?.name, "B")
        s.setResults([], forQuery: "x")
        XCTAssertNil(s.commit())                          // empty
    }

    func testStatusLineText() {
        var s = LocationSearch(); s.enter()
        XCTAssertEqual(s.statusLine, "Type a city…")
        s.typed("z"); XCTAssertEqual(s.statusLine, "Searching…")
        s.setResults([], forQuery: "z"); XCTAssertEqual(s.statusLine, "No matches")
        s.setError(forQuery: "z"); XCTAssertEqual(s.statusLine, "Offline — check connection")
        s.setResults([r("A")], forQuery: "z"); XCTAssertEqual(s.statusLine, "")
    }

    func testExitDeactivates() {
        var s = LocationSearch(); s.enter(); s.typed("a")
        s.exit()
        XCTAssertFalse(s.active)
        XCTAssertEqual(s.query, "")
        XCTAssertEqual(s.results, [])
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `swift test --filter LocationSearchTests`
Expected: FAIL — `cannot find 'LocationSearch' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `weather/Sources/Weather/LocationSearch.swift`:

```swift
import Foundation

/// Pure, AppKit-free state for the location-search overlay. `AppDelegate` mutates a copy on the
/// main thread and publishes it into `WeatherHost` (lock-guarded) for the render thread to read.
struct LocationSearch: Equatable {
    enum Status: Equatable { case idle, querying, results, empty, error }

    private(set) var active: Bool = false
    private(set) var query: String = ""
    private(set) var results: [GeoResult] = []
    private(set) var selected: Int = 0
    private(set) var status: Status = .idle

    mutating func enter() {
        active = true; query = ""; results = []; selected = 0; status = .idle
    }

    mutating func exit() {
        active = false; query = ""; results = []; selected = 0; status = .idle
    }

    /// Append already-validated printable text (caller filters control keys).
    mutating func typed(_ s: String) {
        query += s
        status = query.isEmpty ? .idle : .querying
    }

    mutating func backspace() {
        if !query.isEmpty { query.removeLast() }
        if query.isEmpty { results = []; selected = 0; status = .idle }
        else { status = .querying }
    }

    mutating func moveSelection(_ delta: Int) {
        guard !results.isEmpty else { return }
        selected = min(max(selected + delta, 0), results.count - 1)
    }

    /// Apply geocoder output only if it still matches the current query (drops stale async replies).
    mutating func setResults(_ r: [GeoResult], forQuery q: String) {
        guard q == query else { return }
        results = r; selected = 0; status = r.isEmpty ? .empty : .results
    }

    mutating func setError(forQuery q: String) {
        guard q == query else { return }
        results = []; selected = 0; status = .error
    }

    /// The pick to commit, or nil when there is nothing selectable.
    func commit() -> GeoResult? {
        guard status == .results, selected >= 0, selected < results.count else { return nil }
        return results[selected]
    }

    /// Card body line shown when no rows are listed ("" while results are shown).
    var statusLine: String {
        switch status {
        case .idle:     return "Type a city…"
        case .querying: return "Searching…"
        case .empty:    return "No matches"
        case .error:    return "Offline — check connection"
        case .results:  return ""
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `swift test --filter LocationSearchTests`
Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/Weather/LocationSearch.swift Tests/WeatherTests/LocationSearchTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(weather): LocationSearch pure state machine (query/results/selection/status)"
```

---

### Task 3: `LocationStore` — UserDefaults persistence

**Files:**
- Create: `weather/Sources/Weather/LocationStore.swift`
- Test: `weather/Tests/WeatherTests/LocationStoreTests.swift`

**Interfaces:**
- Consumes: nothing (leaf).
- Produces:
  - `struct StoredLocation: Equatable { let latitude: Double; let longitude: Double; let name: String }`
  - `struct LocationStore { init(defaults: UserDefaults = .standard); func load() -> StoredLocation?; func save(_ loc: StoredLocation) }`

- [ ] **Step 1: Write the failing test**

Create `weather/Tests/WeatherTests/LocationStoreTests.swift`:

```swift
import XCTest
@testable import Weather

final class LocationStoreTests: XCTestCase {
    private var defaults: UserDefaults!
    private let suite = "wx.tests.\(UUID().uuidString)"

    override func setUp() {
        defaults = UserDefaults(suiteName: suite)!
    }
    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
    }

    func testLoadNilWhenEmpty() {
        XCTAssertNil(LocationStore(defaults: defaults).load())
    }

    func testSaveThenLoadRoundTrips() {
        let store = LocationStore(defaults: defaults)
        let loc = StoredLocation(latitude: -33.9258, longitude: 18.4232, name: "Cape Town")
        store.save(loc)
        XCTAssertEqual(store.load(), loc)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `swift test --filter LocationStoreTests`
Expected: FAIL — `cannot find 'LocationStore' in scope`.

- [ ] **Step 3: Write minimal implementation**

Create `weather/Sources/Weather/LocationStore.swift`:

```swift
import Foundation

struct StoredLocation: Equatable {
    let latitude: Double
    let longitude: Double
    let name: String
}

/// Persists the last chosen location so it survives relaunch. Absent → the app keeps its
/// built-in default (Accra).
struct LocationStore {
    private let defaults: UserDefaults
    private let kLat = "wx.location.lat"
    private let kLon = "wx.location.lon"
    private let kName = "wx.location.name"

    init(defaults: UserDefaults = .standard) { self.defaults = defaults }

    func load() -> StoredLocation? {
        guard defaults.object(forKey: kLat) != nil,
              defaults.object(forKey: kLon) != nil,
              let name = defaults.string(forKey: kName) else { return nil }
        return StoredLocation(latitude: defaults.double(forKey: kLat),
                              longitude: defaults.double(forKey: kLon),
                              name: name)
    }

    func save(_ loc: StoredLocation) {
        defaults.set(loc.latitude, forKey: kLat)
        defaults.set(loc.longitude, forKey: kLon)
        defaults.set(loc.name, forKey: kName)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `swift test --filter LocationStoreTests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/Weather/LocationStore.swift Tests/WeatherTests/LocationStoreTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(weather): LocationStore (UserDefaults persistence for chosen city)"
```

---

### Task 4: `WeatherHost` — `search_*` binding surface

**Files:**
- Modify: `weather/Sources/Weather/WeatherHost.swift`
- Test: `weather/Tests/WeatherTests/WeatherHostSearchTests.swift`

**Interfaces:**
- Consumes: `LocationSearch` (Task 2), `GeoResult` (Task 1).
- Produces on `WeatherHost`:
  - lock-guarded `var search: LocationSearch` (get/set, mirrors `model`'s locking).
  - `num("search_active")` → `1`/`0`.
  - `str("search_query")`, `str("search_status")`, `str("search_hint")`, `str("search_row_<i>_label")` — all `""` when inactive; `search_row_<i>_label` carries a `"▸  "`/`"    "` selection-marker prefix (row color can't be host-driven).
  - private generic `indexBetween(_:prefix:suffix:)` (also refactors the existing hour-index parse).

**Selection-highlight note:** because text color is baked at load, the highlighted row is marked with a leading `"▸  "` glyph on the selected label and `"    "` (four spaces) on the others — not a color change.

- [ ] **Step 1: Write the failing test**

Create `weather/Tests/WeatherTests/WeatherHostSearchTests.swift`:

```swift
import XCTest
@testable import Weather

final class WeatherHostSearchTests: XCTestCase {
    private func activeSearch() -> LocationSearch {
        var s = LocationSearch()
        s.enter()
        s.typed("c"); s.typed("a"); s.typed("p")
        s.setResults([
            GeoResult(name: "Cape Town", admin1: "Western Cape", country: "South Africa",
                      latitude: -33.9, longitude: 18.4),
            GeoResult(name: "Cape Coral", admin1: "Florida", country: "United States",
                      latitude: 26.5, longitude: -81.9),
        ], forQuery: "cap")
        s.moveSelection(1)   // select "Cape Coral"
        return s
    }

    func testInactiveHidesEverything() {
        let host = WeatherHost(model: .sample)          // default search is inactive
        XCTAssertEqual(host.num("search_active"), 0)
        XCTAssertEqual(host.str("search_query"), "")
        XCTAssertEqual(host.str("search_status"), "")
        XCTAssertEqual(host.str("search_hint"), "")
        XCTAssertEqual(host.str("search_row_0_label"), "")
    }

    func testActiveExposesQueryStatusHint() {
        let host = WeatherHost(model: .sample)
        host.search = activeSearch()
        XCTAssertEqual(host.num("search_active"), 1)
        XCTAssertEqual(host.str("search_query"), "cap")
        XCTAssertEqual(host.str("search_status"), "")        // results shown → no status line
        XCTAssertEqual(host.str("search_hint"), "↑↓ select · ⏎ go · esc cancel")
    }

    func testRowLabelsCarrySelectionMarker() {
        let host = WeatherHost(model: .sample)
        host.search = activeSearch()                          // selected index 1
        XCTAssertEqual(host.str("search_row_0_label"), "    Cape Town, Western Cape, South Africa")
        XCTAssertEqual(host.str("search_row_1_label"), "▸  Cape Coral, Florida, United States")
        XCTAssertEqual(host.str("search_row_2_label"), "")   // only 2 results
    }

    func testStatusLineWhenNoResults() {
        let host = WeatherHost(model: .sample)
        var s = LocationSearch(); s.enter(); s.typed("z"); s.setResults([], forQuery: "z")
        host.search = s
        XCTAssertEqual(host.str("search_status"), "No matches")
        XCTAssertEqual(host.str("search_row_0_label"), "")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `swift test --filter WeatherHostSearchTests`
Expected: FAIL — `value of type 'WeatherHost' has no member 'search'`.

- [ ] **Step 3: Write minimal implementation**

In `weather/Sources/Weather/WeatherHost.swift`:

(a) Add the backing store + accessor. After the `_intensityOverride` line (currently `private var _intensityOverride: Double?`, ~line 84), add:

```swift
    private var _search = LocationSearch()

    /// The location-search overlay state. Set from the MAIN thread (key handler / debounce);
    /// read from the RENDER thread via `num`/`str`. Lock-guarded like `model`.
    var search: LocationSearch {
        get { lock.lock(); defer { lock.unlock() }; return _search }
        set { lock.lock(); _search = newValue; lock.unlock() }
    }
```

(b) Add the generic index parser and refactor `hourIndex` to use it. Replace the existing `hourIndex` method (currently ~lines 91-99) with:

```swift
    /// Parse the integer `i` out of "<prefix><i><suffix>", or nil.
    private func indexBetween(_ key: String, prefix: String, suffix: String) -> Int? {
        guard key.hasPrefix(prefix), key.hasSuffix(suffix) else { return nil }
        let start = key.index(key.startIndex, offsetBy: prefix.count)
        let end = key.index(key.endIndex, offsetBy: -suffix.count)
        guard start <= end else { return nil }
        return Int(key[start..<end])
    }

    /// Parse the `i` out of "wx_hour_<i>_<suffix>", or nil.
    private func hourIndex(_ key: String, suffix: String) -> Int? {
        indexBetween(key, prefix: "wx_hour_", suffix: suffix)
    }
```

(c) In `num(_:)`, add a case before `default:`:

```swift
        case "search_active": return search.active ? 1 : 0
```

(d) In `str(_:)`, handle search keys **before** the `if uiDrowned { return "" }` guard so the overlay works in any condition. Replace the opening of `str` (currently the `if uiDrowned { return "" }` line down to `let m = model`) so it reads:

```swift
    func str(_ key: String) -> String? {
        // Search overlay first — must render regardless of the tsunami "drowned" blanking below.
        let s = search
        switch key {
        case "search_query":  return s.active ? s.query : ""
        case "search_status": return s.active ? s.statusLine : ""
        case "search_hint":   return s.active ? "↑↓ select · ⏎ go · esc cancel" : ""
        default:
            if let i = indexBetween(key, prefix: "search_row_", suffix: "_label") {
                guard s.active, i >= 0, i < s.results.count else { return "" }
                let marker = (i == s.selected) ? "▸  " : "    "
                return marker + s.results[i].label
            }
        }

        if uiDrowned { return "" }   // empty strings skip rendering — the forecast is underwater
        // Snapshot once so the count-check and the index read below see the SAME model (TOCTOU).
        let m = model
        switch key {
```

(Leave the rest of `str` — the `location`/`condition_text`/hourly cases and the final `return nil` — unchanged.)

- [ ] **Step 4: Run test to verify it passes**

Run: `swift test --filter WeatherHostSearchTests` then the full suite `swift test`
Expected: PASS (4 new tests); full suite still green (existing `WeatherHostTests` unaffected).

- [ ] **Step 5: Commit**

```bash
git add Sources/Weather/WeatherHost.swift Tests/WeatherTests/WeatherHostSearchTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(weather): WeatherHost search_* binding surface for the overlay"
```

---

### Task 5: Input routing + `AppDelegate` wiring

**Files:**
- Modify: `weather/Sources/Weather/SkinView.swift:75-78` (keyDown forwards characters)
- Modify: `weather/Sources/Weather/App.swift` (mode routing, debounce, commit, launch-load)

**Interfaces:**
- Consumes: `LocationSearch` (T2), `Geocoder`/`OpenMeteoGeocoder`/`GeoResult` (T1), `LocationStore`/`StoredLocation` (T3), `WeatherHost.search` (T4).
- Produces: no new public types. Behavior — `L` opens search; typing/`↑↓`/`⏎`/`Esc` drive it; commit rebuilds `WeatherService` + persists + refetches; launch restores the saved city. This task has **no unit tests** (AppKit `AppDelegate` / `NSView`); it is validated by `swift build` and by the live check in Task 7. Keep the diff minimal and mechanical.

**Reference — macOS virtual keycodes used:** `L`=37, Return=36, keypad-Enter=76, Esc=53, Delete/Backspace=51, Up=126, Down=125, Left=123, Right=124 (existing: `D`=2, `S`=1, `R`=15).

- [ ] **Step 1: Widen the key hook in `SkinView.swift`**

Replace lines 75-78:

```swift
    override func keyDown(with e: NSEvent) {
        onKey?(e.keyCode)
    }
    var onKey: ((UInt16) -> Void)?
```

with:

```swift
    override func keyDown(with e: NSEvent) {
        onKey?(e.keyCode, e.characters)
    }
    var onKey: ((UInt16, String?) -> Void)?
```

- [ ] **Step 2: Add search state + collaborators to `AppDelegate` in `App.swift`**

Change `private let service = WeatherService()` (line 16) to `private var service = WeatherService()`, and immediately after `private var refreshTimer: Timer?` (line 17) add:

```swift
    private let store = LocationStore()
    private let geocoder: Geocoder = OpenMeteoGeocoder()
    private var search = LocationSearch()
    private var searchDebounce: Timer?
```

- [ ] **Step 3: Restore the saved location at launch**

In `applicationDidFinishLaunching`, immediately **before** the `host = WeatherHost(...)` line (currently line 27), add:

```swift
        // Restore the last-chosen city (falls back to the WeatherService default = Accra).
        if let loc = store.load() {
            service = WeatherService(latitude: loc.latitude, longitude: loc.longitude,
                                     locationName: loc.name)
        }
```

- [ ] **Step 4: Widen the `onKey` closure**

Replace line 33:

```swift
        view.onKey = { [weak self] code in self?.handleKey(code) }
```

with:

```swift
        view.onKey = { [weak self] code, chars in self?.handleKey(code, chars) }
```

- [ ] **Step 5: Route keys by mode**

Replace the existing `handleKey` method (currently lines 110-119) with:

```swift
    // Presenter controls (overrides force only the shader; hero/hourly/daily text stays live):
    //   →/← tour condition · D cycles dawn/noon/dusk/night · S cycles season · R refetches ·
    //   L opens location search. While searching, keys drive the overlay (see handleSearchKey).
    private func handleKey(_ code: UInt16, _ chars: String?) {
        if search.active { handleSearchKey(code, chars); return }
        switch code {
        case 124: host.conditionOverride = ConditionCycle.next(host.conditionOverride)          // →
        case 123: host.conditionOverride = ConditionCycle.prev(host.conditionOverride)          // ←
        case 2:   host.sunOverride = ConditionCycle.next(host.sunOverride, stops: SunMath.presenterStops) // D
        case 1:   host.seasonOverride = ConditionCycle.next(host.seasonOverride, upTo: 3)        // S
        case 15:  refresh()                                                                       // R
        case 37:  search.enter(); publishSearch()                                                 // L
        default:  break
        }
    }

    private func publishSearch() { host.search = search }

    private func handleSearchKey(_ code: UInt16, _ chars: String?) {
        switch code {
        case 53:      search.exit(); publishSearch(); searchDebounce?.invalidate()   // Esc
        case 36, 76:  commitSearch()                                                  // Return / Enter
        case 51:      search.backspace(); publishSearch(); scheduleGeocode()          // Delete
        case 126:     search.moveSelection(-1); publishSearch()                       // Up
        case 125:     search.moveSelection(1); publishSearch()                        // Down
        case 123, 124: break                                                          // ←/→ ignored
        default:
            if let s = chars, isPrintable(s) {
                search.typed(s); publishSearch(); scheduleGeocode()
            }
        }
    }

    /// Accept only visible characters (letters, digits, space, punctuation) — never control/
    /// function keys, which arrive as non-printable scalars or nil.
    private func isPrintable(_ s: String) -> Bool {
        !s.isEmpty && s.unicodeScalars.allSatisfy { $0.value >= 0x20 && $0.value != 0x7F }
    }

    /// Debounce the network geocode ~250 ms after the last keystroke.
    private func scheduleGeocode() {
        searchDebounce?.invalidate()
        let q = search.query
        guard !q.isEmpty else { return }   // backspace-to-empty already reset status to .idle
        searchDebounce = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: false) { [weak self] _ in
            self?.runGeocode(q)
        }
    }

    private func runGeocode(_ q: String) {
        Task { [weak self] in
            guard let self else { return }
            do {
                let results = try await self.geocoder.search(q)
                await MainActor.run { self.search.setResults(results, forQuery: q); self.publishSearch() }
            } catch {
                await MainActor.run { self.search.setError(forQuery: q); self.publishSearch() }
            }
        }
    }

    private func commitSearch() {
        guard let r = search.commit() else { return }   // Enter with nothing selectable → ignore
        service = WeatherService(latitude: r.latitude, longitude: r.longitude, locationName: r.name)
        store.save(StoredLocation(latitude: r.latitude, longitude: r.longitude, name: r.name))
        search.exit(); publishSearch(); searchDebounce?.invalidate()
        refresh()
    }
```

- [ ] **Step 6: Build**

Run: `swift build`
Expected: builds clean (no warnings introduced). The overlay won't be visible yet (skin not updated), but `L` now mutates host search state without crashing.

- [ ] **Step 7: Commit**

```bash
git add Sources/Weather/SkinView.swift Sources/Weather/App.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(weather): route keys by mode — L opens search, debounced geocode, commit→refetch"
```

---

### Task 6: Skin overlay in `skin.lua`

**Files:**
- Modify: `weather/skins/weather/skin.lua` (append the overlay block)

**Interfaces:**
- Consumes host bindings from Task 4: `search_active` (num, `value_fill` gate), `search_query`, `search_status`, `search_hint`, `search_row_0.._4_label` (str).
- Produces: no code interface — visual overlay. Validated live.

**Gating rule (from Global Constraints):** scrim + card are `value_fill{ value = "search_active" }` (draw nothing at 0). Every text line uses `value = "<key>"` (empty string when inactive). No static `text{ text=… }`, no literal `fill{}`.

- [ ] **Step 1: Append the overlay block to `skin.lua`**

Add at the end of `weather/skins/weather/skin.lua`:

```lua
-- ── Location search overlay (M4) ──────────────────────────────────────────────
-- Everything here is gated on the host bool `search_active`: the two value_fill
-- panels draw nothing when it is 0, and every text line binds a host string that
-- is "" while inactive. Declared last, so it composites on top of the scene.
local SEARCH_ROWS = 5
local cardX, cardY = 28, 150
local cardW, cardH = W - 56, 320

-- Full-scene dim, then the card. value_fill draws nothing at value 0, full color at 1.
value_fill{ path = rect{ x = 0, y = 0, w = W, h = H }, value = "search_active",
            color = { r = 6, g = 9, b = 16, a = 150 }, direction = "down" }
value_fill{ path = rounded_rect{ x = cardX, y = cardY, w = cardW, h = cardH, radius = 16 },
            value = "search_active",
            color = { r = 14, g = 18, b = 28, a = 235 }, direction = "down" }

-- Query line.
text{ value = "search_query", font = F_PRI, x = cardX + 22, y = cardY + 20, size = 22, color = PRI }
-- Status line (Type a city… / Searching… / No matches / Offline). "" while results show.
text{ value = "search_status", font = F_SEC, x = cardX + 22, y = cardY + 74, size = 14, color = SEC }
-- Result rows; the selected one carries a "▸" marker baked into the label host-side.
local searchRowY0 = cardY + 70
for i = 0, SEARCH_ROWS - 1 do
  text{ value = "search_row_" .. i .. "_label", font = F_SEC,
        x = cardX + 18, y = searchRowY0 + i * 32, size = 15, color = PRI }
end
-- Navigation hint pinned near the card bottom.
text{ value = "search_hint", font = F_SEC, x = cardX + 22, y = cardY + cardH - 30,
      size = 12, halign = "left", color = SEC }
```

- [ ] **Step 2: Live check — overlay opens, searches, commits, cancels**

Ensure the engine dylib is built (the app links `../target/debug/libcarapace_ffi.dylib`):

```bash
# from repo root, only if the dylib is stale/missing:
cargo build -p carapace-ffi
```

Launch the app so its window shows without stealing focus, positioned on-screen:

```bash
cd weather && WX_SHY=1 WX_POS="200,120" swift run 2>/tmp/wx.err &
```

Then verify by hand-driving (or via `osascript` key codes) and eyeballing:
- Press **L** → scrim dims the scene, card appears, status reads "Type a city…".
- Type **"cape town"** → after ~¼s, rows appear ("Cape Town, Western Cape, South Africa" …); first row shows the **▸** marker.
- **↓ ↓** moves the marker down (clamps at the last row); **↑** moves it up.
- **⏎** on a row → card closes, scene refetches to that city, the hero location text updates.
- Press **L**, type nonsense (**"zzzzz"**) → "No matches".
- Press **L**, then **Esc** → card closes, nothing changed; tour keys (**→ ← D S R**) work again.

Kill the app when done: `kill %1 2>/dev/null` (or `pkill -f 'swift run'`). Adjust card geometry / row spacing / colors in `skin.lua` until it reads cleanly, then re-launch to confirm.

- [ ] **Step 3: Commit**

```bash
git add skins/weather/skin.lua
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(weather): in-skin location-search overlay (scrim + card + results)"
```

---

### Task 7: Verification, docs, and ship

**Files:**
- Modify (if present): `weather/README.md` (document the `L` key)
- No code changes beyond doc/tuning.

- [ ] **Step 1: Full gate**

```bash
cd weather && swift build && swift test 2>&1 | tail -5     # all green
cd /Users/nexus/projects/experiments/winamp
git diff main --stat | grep crates && echo "ENGINE DIFF — STOP" || echo "zero engine changes ✓"
git status --porcelain                                     # only intended weather/ + docs/ files
```
Expected: tests green; "zero engine changes ✓"; no stray files.

- [ ] **Step 2: Docs**

If `weather/README.md` exists and lists the presenter keys (`→ ← D S R`), add `L` — "press **L** to search for a city; type, `↑↓` to choose, `⏎` to switch, `Esc` to cancel; the choice is remembered next launch." Skip if there is no such README section (do not invent one). Do not touch `docs/api/` — this milestone adds no engine/skin-authoring primitive.

- [ ] **Step 3: Live GIF for the user**

With the app running (as in Task 6 Step 2), screen-record ~10 s of: press L → type a city → ↑↓ → ⏎ → scene refetches. Convert to GIF (the same `screencapture -v` → `ffmpeg` path used in prior weather milestones) and send it to the user with `SendUserFile`.

- [ ] **Step 4: Commit any final tuning + push**

```bash
git add -A weather/ docs/
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs(weather): note the L location-search key" --allow-empty
git push -u origin weather-app-m4-location-search
```

- [ ] **Step 5: Open the PR**

Open a PR targeting `main`, title `feat(weather): M4 location search — in-skin city search`, body summarizing the overlay + persistence and stating **zero engine changes**. **No "Generated with Claude Code" footer.**

---

## Self-Review

**Spec coverage** (against `docs/superpowers/specs/2026-07-23-weather-app-m4-location-search-design.md`):
- In-skin overlay, gated on `search_active` → Task 4 (binding) + Task 6 (value_fill). ✓
- `L` opens, type, `↑↓`, `⏎` commit + refetch, `Esc` cancel → Task 5. ✓
- Tour keys ignored while searching → Task 5 (`handleSearchKey` early return). ✓
- Persist + restore last city → Task 3 + Task 5 (launch-load, commit-save). ✓
- Debounced (~250 ms) geocode → Task 5. ✓
- `Geocoder` protocol + Open-Meteo + offline fixture test → Task 1. ✓
- `LocationSearch` pure state machine + tests → Task 2. ✓
- Bindings `search_active/query/status/row_<i>_label` → Task 4. ✓ (Deviations from the spec's binding list, both driven by the primitive investigation: `search_count`/`search_sel` are **not** exposed — selection is folded into the row label's marker prefix because text color can't be host-driven; a `search_hint` binding is **added** because the hint line can't be a static `text{}`.)
- Empty / no-match / error states → Task 2 (`statusLine`) + Task 6. ✓
- Zero engine changes; `swift test` green → Task 7 gate. ✓

**Placeholder scan:** no TBD/TODO; every code step shows complete code; every test shows real assertions. ✓

**Type consistency:** `GeoResult` (name, admin1?, country?, latitude, longitude, label) used identically across Tasks 1/2/4/5. `LocationSearch` method names (`enter/exit/typed/backspace/moveSelection/setResults/setError/commit/statusLine/active/query/results/selected`) match across Tasks 2/4/5. `StoredLocation`(latitude, longitude, name) + `LocationStore.load/save` match across Tasks 3/5. `onKey: ((UInt16, String?) -> Void)?` set in Task 5 Step 1 and consumed in Step 4. Host keys (`search_active/query/status/hint/row_<i>_label`) match between Task 4 (producer) and Task 6 (consumer). ✓

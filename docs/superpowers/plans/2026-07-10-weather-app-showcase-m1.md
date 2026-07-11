# Weather App Showcase — Milestone 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a new standalone macOS app whose *skin is the window* — a borderless, draggable, skin-shaped window rendering a carapace weather skin (animated WGSL `shader{}` background + hero/hourly/daily UI) driven by a static mock `WeatherHost`. No network. Proves the skin-as-window pipeline end to end.

**Architecture:** A new SwiftPM package `weather/` (parallel to `showcase/`) links the existing `carapace-ffi` cdylib + a copied `carapace.h`. It reuses the Showcase's proven display scaffold — a borderless `NSWindow`, a layer-backed `SkinView` whose `CALayer.contents` is the engine's IOSurface (zero-copy), and `CarapaceBridge` (engine create + IOSurface pool + `frame_ready`). The engine renders `skins/weather` (a `shader{}` 4-stage background + 2D UI) into the pool; Swift displays each frame and feeds host data (the `wx_*` shader uniforms + display strings + daily rows) through the `CarapaceHostVTable`. **Zero engine-crate changes.**

**Tech Stack:** Swift 6 (SwiftPM, AppKit), IOSurface/CoreAnimation, carapace-ffi (C ABI 3.x), WGSL (naga-validated at skin load), Lua (skin).

## Global Constraints

- **Zero engine-crate changes.** All work is under `weather/` plus a copied `carapace.h`. No edits to any `crates/*`. If something seems to require an engine change, STOP and escalate — the FFI + `shader{}` are expected to be sufficient.
- **Base:** branch `weather-app-showcase` off `main` (commit `37ede2a`, which includes the merged `shader{}` primitive). Never commit to `main`.
- **Reuse the Showcase scaffold by copying named files**, then adapting exactly as shown. Do not restructure the copied files beyond the shown edits.
- **Build order:** `cargo build -p carapace-ffi` (produces `target/debug/libcarapace_ffi.dylib` the app links) BEFORE `swift build`/`swift test` in `weather/`.
- **Canvas:** 400×680 logical (portrait). Backing scale from `NSScreen.main.backingScaleFactor` (2 on Retina) → 800×1360 surface pixels.
- **Six conditions + WMO mapping** (shader `switch` on `wx_condition`): 0 clear (WMO 0–1) · 1 cloud (2–3) · 2 rain (51–67,80–82) · 3 snow (71–77,85–86) · 4 storm (95–99) · 5 fog (45,48).
- **Host-data contract** (the skin reads these; `WeatherHost` answers them):
  - **Numeric (`get_num`)** — shader uniforms only: `wx_condition`, `wx_is_day`, `wx_temp`, `wx_intensity`, `wx_season`.
  - **String (`get_str`)** — hero + hourly labels: `location`, `condition_text`, `temp_now`, `hi_lo`, `feels`, and `wx_hour_{i}_time` + `wx_hour_{i}_temp` for `i` in `0..12` (display strings like `"13h"` / `"28°"`).
  - **Rows (`row_count`/`get_row_str`)** — collection `"daily"`, ~7 rows, string fields `day`, `glyph`, `hi`, `lo`. `get_row_num` returns false (no numeric daily fields in M1).
  - *Refinement vs spec:* hourly temp cells are **strings** (`get_str`, `"28°"`) because `text{ value=… }` binds host strings; the shader's numeric uniforms remain `get_num`.
- **Git identity:** Daniel Agbemava <danagbemava@gmail.com>. No Claude attribution in commits/PRs.
- **Milestone 1 is mock-only:** no `URLSession`, no geocoding, no `view{}` search cutout, no bottom-flowing silhouette, no condition crossfades. Those are M2–M4.

## File Structure

New package `weather/`:
- `weather/Package.swift` — SwiftPM manifest (links `carapace_ffi`).
- `weather/Sources/CCarapace/module.modulemap` + `include/carapace.h` — the C module (copied header).
- `weather/Sources/Weather/SkinWindow.swift` — borderless key-capable `NSWindow` (copied verbatim).
- `weather/Sources/Weather/SkinView.swift` — IOSurface display + input (copied, adapted).
- `weather/Sources/Weather/CarapaceBridge.swift` — engine create + pool + `frame_ready` (copied, trimmed).
- `weather/Sources/Weather/HostCallbacks.swift` — the vtable, backed by `WeatherHost` (adapted).
- `weather/Sources/Weather/WeatherModel.swift` — the data struct + `.sample` (new).
- `weather/Sources/Weather/WeatherHost.swift` — answers the host-data contract from a `WeatherModel` (new).
- `weather/Sources/Weather/App.swift` — `@main` bootstrap: borderless window, bridge, traffic lights, debug key (new, modeled on Showcase `App.swift`).
- `weather/skins/weather/skin.toml` + `skin.lua` + `assets/weather.wgsl` — the weather skin (new).
- `weather/Tests/WeatherTests/WeatherHostTests.swift` — unit tests for the host-data contract (new).

---

### Task 1: SwiftPM package scaffold that links carapace-ffi

Stand up the package so `swift build` compiles and links the Rust cdylib. Deliverable: an empty-but-linking executable.

**Files:**
- Create: `weather/Package.swift`
- Create: `weather/Sources/CCarapace/module.modulemap`
- Create: `weather/Sources/CCarapace/include/carapace.h` (copied)
- Create: `weather/Sources/Weather/main_placeholder.swift` (temporary, deleted in Task 5)

**Interfaces:**
- Produces: a buildable `Weather` executable target depending on `CCarapace`.

- [ ] **Step 1: Build the Rust dylib the app links**

Run: `cargo build -p carapace-ffi`
Expected: `Finished` and `target/debug/libcarapace_ffi.dylib` exists (`ls target/debug/libcarapace_ffi.dylib`).

- [ ] **Step 2: Copy the generated C header into the package**

Run:
```bash
mkdir -p weather/Sources/CCarapace/include
cp crates/carapace-ffi/include/carapace.h weather/Sources/CCarapace/include/carapace.h
```

- [ ] **Step 3: Write the module map**

Create `weather/Sources/CCarapace/module.modulemap`:
```
module CCarapace {
    header "include/carapace.h"
    link "carapace_ffi"
    export *
}
```

- [ ] **Step 4: Write `Package.swift`**

Create `weather/Package.swift`:
```swift
// swift-tools-version:6.0
import PackageDescription

let repoTarget = "../target/debug"  // libcarapace_ffi.dylib location relative to this package

let package = Package(
    name: "CarapaceWeather",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CCarapace", path: "Sources/CCarapace"),
        .executableTarget(
            name: "Weather",
            dependencies: ["CCarapace"],
            swiftSettings: [
                .swiftLanguageMode(.v5),
                // carapace.h gates its Apple-only API behind `#if defined(CARAPACE_APPLE)`; the C
                // importer needs the same define. C23 makes each `typedef enum Foo Foo;` self-
                // referential (avoids the Swift "ambiguous for type lookup" on enum type names).
                .unsafeFlags(["-Xcc", "-DCARAPACE_APPLE", "-Xcc", "-std=c23"]),
            ],
            linkerSettings: [
                .unsafeFlags([
                    "-L", repoTarget, "-lcarapace_ffi",
                    "-Xlinker", "-rpath", "-Xlinker", repoTarget,
                ])
            ]
        ),
        .testTarget(name: "WeatherTests", dependencies: ["Weather"]),
    ]
)
```

- [ ] **Step 5: Add a temporary placeholder main**

Create `weather/Sources/Weather/main_placeholder.swift`:
```swift
import CCarapace

// Temporary: proves the package links carapace-ffi. Replaced by App.swift in Task 5.
@main
struct Placeholder {
    static func main() {
        var buf = [CChar](repeating: 0, count: 16)
        _ = carapace_last_error(&buf, UInt(buf.count)) // any exported symbol proves linkage
        print("weather: linked carapace-ffi")
    }
}
```

- [ ] **Step 6: Build to verify linkage**

Run: `cd weather && swift build`
Expected: `Build complete!` (no linker errors about `carapace_ffi`).

- [ ] **Step 7: Commit**

```bash
git add weather/Package.swift weather/Sources/CCarapace weather/Sources/Weather/main_placeholder.swift
git commit -m "feat(weather): scaffold SwiftPM app linking carapace-ffi"
```

---

### Task 2: WeatherModel + WeatherHost + unit tests (TDD)

The pure data layer: a `WeatherModel` with a hardcoded `.sample`, and a `WeatherHost` that answers the host-data contract. No engine dependency — fully unit-testable.

**Files:**
- Create: `weather/Sources/Weather/WeatherModel.swift`
- Create: `weather/Sources/Weather/WeatherHost.swift`
- Test: `weather/Tests/WeatherTests/WeatherHostTests.swift`

**Interfaces:**
- Produces:
  - `struct HourCell { let time: String; let temp: String }`
  - `struct DayRow { let day: String; let glyph: String; let hi: String; let lo: String }`
  - `struct WeatherModel` with fields: `location: String`, `conditionText: String`, `condition: Double`, `isDay: Double`, `temp: Double`, `intensity: Double`, `season: Double`, `tempNow: String`, `hiLo: String`, `feels: String`, `hours: [HourCell]`, `days: [DayRow]`; and `static let sample: WeatherModel`.
  - `final class WeatherHost { init(model: WeatherModel); var model: WeatherModel; func num(_ key: String) -> Double?; func str(_ key: String) -> String?; func rowCount() -> Int; func rowString(_ index: Int, field: String) -> String? }`

- [ ] **Step 1: Write the failing tests**

Create `weather/Tests/WeatherTests/WeatherHostTests.swift`:
```swift
import XCTest
@testable import Weather

final class WeatherHostTests: XCTestCase {
    private let host = WeatherHost(model: .sample)

    func testShaderUniformsAreNumeric() {
        XCTAssertEqual(host.num("wx_condition"), WeatherModel.sample.condition)
        XCTAssertEqual(host.num("wx_is_day"), WeatherModel.sample.isDay)
        XCTAssertEqual(host.num("wx_temp"), WeatherModel.sample.temp)
        XCTAssertEqual(host.num("wx_intensity"), WeatherModel.sample.intensity)
        XCTAssertEqual(host.num("wx_season"), WeatherModel.sample.season)
        XCTAssertNil(host.num("location"))          // strings are not numeric
        XCTAssertNil(host.num("nonsense_key"))
    }

    func testHeroStrings() {
        XCTAssertEqual(host.str("location"), WeatherModel.sample.location)
        XCTAssertEqual(host.str("condition_text"), WeatherModel.sample.conditionText)
        XCTAssertEqual(host.str("temp_now"), WeatherModel.sample.tempNow)
        XCTAssertEqual(host.str("hi_lo"), WeatherModel.sample.hiLo)
        XCTAssertEqual(host.str("feels"), WeatherModel.sample.feels)
        XCTAssertNil(host.str("wx_condition"))      // numerics are not strings
    }

    func testHourlyCells() {
        XCTAssertEqual(host.str("wx_hour_0_time"), WeatherModel.sample.hours[0].time)
        XCTAssertEqual(host.str("wx_hour_0_temp"), WeatherModel.sample.hours[0].temp)
        XCTAssertEqual(host.str("wx_hour_11_temp"), WeatherModel.sample.hours[11].temp)
        XCTAssertNil(host.str("wx_hour_99_temp"))   // out of range
    }

    func testDailyRows() {
        XCTAssertEqual(host.rowCount(), WeatherModel.sample.days.count)
        let d = WeatherModel.sample.days[0]
        XCTAssertEqual(host.rowString(0, field: "day"), d.day)
        XCTAssertEqual(host.rowString(0, field: "glyph"), d.glyph)
        XCTAssertEqual(host.rowString(0, field: "hi"), d.hi)
        XCTAssertEqual(host.rowString(0, field: "lo"), d.lo)
        XCTAssertNil(host.rowString(0, field: "bogus"))
        XCTAssertNil(host.rowString(99, field: "day"))   // out of range
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd weather && swift test`
Expected: FAIL to compile (`WeatherHost` / `WeatherModel` undefined).

- [ ] **Step 3: Implement `WeatherModel`**

Create `weather/Sources/Weather/WeatherModel.swift`:
```swift
struct HourCell {
    let time: String   // "13h"
    let temp: String   // "28°"
}

struct DayRow {
    let day: String    // "Mon"
    let glyph: String   // "☀"
    let hi: String     // "31°"
    let lo: String     // "24°"
}

/// The full weather state the skin renders. M1 uses `.sample`; M2 derives it from Open-Meteo.
struct WeatherModel {
    var location: String
    var conditionText: String
    var condition: Double   // 0 clear·1 cloud·2 rain·3 snow·4 storm·5 fog
    var isDay: Double       // 0 night / 1 day
    var temp: Double        // °C
    var intensity: Double   // 0..1
    var season: Double      // 0 winter·1 spring·2 summer·3 autumn
    var tempNow: String     // "27°"
    var hiLo: String        // "H:31° L:24°"
    var feels: String       // "Feels 30°"
    var hours: [HourCell]   // 12 cells
    var days: [DayRow]      // 7 rows

    static let sample = WeatherModel(
        location: "Accra",
        conditionText: "Partly cloudy",
        condition: 1, isDay: 1, temp: 27, intensity: 0.4, season: 2,
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
}
```

- [ ] **Step 4: Implement `WeatherHost`**

Create `weather/Sources/Weather/WeatherHost.swift`:
```swift
/// Answers the host-data contract (numeric shader uniforms, display strings, daily rows) from a
/// `WeatherModel`. Held by the HostCallbacks vtable; mutate `model` to change what the skin shows.
final class WeatherHost {
    var model: WeatherModel
    init(model: WeatherModel) { self.model = model }

    /// Parse the `i` out of "wx_hour_<i>_<suffix>", or nil.
    private func hourIndex(_ key: String, suffix: String) -> Int? {
        let prefix = "wx_hour_"
        guard key.hasPrefix(prefix), key.hasSuffix(suffix) else { return nil }
        let start = key.index(key.startIndex, offsetBy: prefix.count)
        let end = key.index(key.endIndex, offsetBy: -suffix.count)
        return Int(key[start..<end])
    }

    func num(_ key: String) -> Double? {
        switch key {
        case "wx_condition": return model.condition
        case "wx_is_day":    return model.isDay
        case "wx_temp":      return model.temp
        case "wx_intensity": return model.intensity
        case "wx_season":    return model.season
        default:             return nil
        }
    }

    func str(_ key: String) -> String? {
        switch key {
        case "location":       return model.location
        case "condition_text": return model.conditionText
        case "temp_now":       return model.tempNow
        case "hi_lo":          return model.hiLo
        case "feels":          return model.feels
        default:
            if let i = hourIndex(key, suffix: "_time"), i >= 0, i < model.hours.count {
                return model.hours[i].time
            }
            if let i = hourIndex(key, suffix: "_temp"), i >= 0, i < model.hours.count {
                return model.hours[i].temp
            }
            return nil
        }
    }

    func rowCount() -> Int { model.days.count }

    func rowString(_ index: Int, field: String) -> String? {
        guard index >= 0, index < model.days.count else { return nil }
        let d = model.days[index]
        switch field {
        case "day":   return d.day
        case "glyph": return d.glyph
        case "hi":    return d.hi
        case "lo":    return d.lo
        default:      return nil
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd weather && swift test`
Expected: PASS (all `WeatherHostTests` green).

- [ ] **Step 6: Commit**

```bash
git add weather/Sources/Weather/WeatherModel.swift weather/Sources/Weather/WeatherHost.swift weather/Tests/WeatherTests/WeatherHostTests.swift
git commit -m "feat(weather): WeatherModel.sample + WeatherHost host-data contract (TDD)"
```

---

### Task 3: Copy the display scaffold (SkinWindow, SkinView, CarapaceBridge, HostCallbacks)

Bring in the proven IOSurface display + input + engine-bridge code, adapted to `WeatherHost` and this app's shape. No swap/dither/music.

**Files:**
- Create: `weather/Sources/Weather/SkinWindow.swift` (verbatim copy)
- Create: `weather/Sources/Weather/SkinView.swift` (copy + adapt)
- Create: `weather/Sources/Weather/CarapaceBridge.swift` (copy + trim)
- Create: `weather/Sources/Weather/HostCallbacks.swift` (copy + adapt)

**Interfaces:**
- Consumes: `WeatherHost` (Task 2), `CCarapace` (Task 1).
- Produces:
  - `final class SkinWindow: NSWindow` (key-capable borderless).
  - `final class SkinView: NSView { var bridge: CarapaceBridge?; var canvasW/H: Double; func show(surface:index:); var onKey: ((UInt16) -> Void)? }`.
  - `final class CarapaceBridge { init?(skinDir:width:height:onFrame:); func hitTest(x:y:) -> CarapaceHitKind; func pointer(x:y:); func releaseSurface(_:) }`.
  - `let hostBox: HostBox` (holds `weak var host: WeatherHost?`), `let windowBox: WindowBox`, `func makeVTable(frameReady:) -> CarapaceHostVTable`.

- [ ] **Step 1: Copy `SkinWindow.swift` verbatim**

Run: `cp showcase/Sources/Showcase/SkinWindow.swift weather/Sources/Weather/SkinWindow.swift`
(It is 7 lines, app-agnostic — no edits needed.)

- [ ] **Step 2: Copy and adapt `SkinView.swift`**

Run: `cp showcase/Sources/Showcase/SkinView.swift weather/Sources/Weather/SkinView.swift`

Then edit `weather/Sources/Weather/SkinView.swift`:
1. Change the default canvas size to portrait:
```swift
    var canvasW: Double = 400
    var canvasH: Double = 680
```
(replace the `= 420` / `= 660` defaults).
2. Replace the `keyDown` + `onTab` block at the bottom with a generic key hook (M1 uses it for the debug condition-cycle):
```swift
    override func keyDown(with e: NSEvent) {
        onKey?(e.keyCode)
    }
    var onKey: ((UInt16) -> Void)?
```
(remove the old `if e.keyCode == 48 { onTab?() } else { super.keyDown… }` and the `var onTab`.)

- [ ] **Step 3: Copy and trim `CarapaceBridge.swift`**

Run: `cp showcase/Sources/Showcase/CarapaceBridge.swift weather/Sources/Weather/CarapaceBridge.swift`

Then edit `weather/Sources/Weather/CarapaceBridge.swift`:
1. In `init?`, drop the `contentSurface` parameter and the `content_surface` field (M1 has no cutouts). Change the signature to:
```swift
    init?(skinDir: String, width: Int, height: Int,
          onFrame: @escaping (IOSurface, UInt32) -> Void) {
```
and in the `CarapaceCreateDesc(...)` literal set:
```swift
                    content_surface: nil,
```
(remove the `contentSurface.map { … } ?? nil` expression).
2. Delete the methods the weather app doesn't use: `swap(skinDir:)`, `swapResized(...)`, and `setContentSurface(...)` (everything from `func swap(` to the end of `setContentSurface`), keeping `pointer`, `hitTest`, `releaseSurface`, and `deinit` (which calls `carapace_destroy`). If `releaseSurface`/`deinit` are below the deleted block, keep them.

Run to see what remains and confirm `releaseSurface` + `deinit` are intact:
```bash
grep -n "func \|deinit" weather/Sources/Weather/CarapaceBridge.swift
```
Expected: `init?`, `pointer`, `hitTest`, `releaseSurface`, `deinit` — and NOT `swap`/`swapResized`/`setContentSurface`.

- [ ] **Step 4: Copy and adapt `HostCallbacks.swift`**

Run: `cp showcase/Sources/Showcase/HostCallbacks.swift weather/Sources/Weather/HostCallbacks.swift`

Then edit `weather/Sources/Weather/HostCallbacks.swift`:
1. Retype the host box to `WeatherHost`:
```swift
final class HostBox { weak var host: WeatherHost? }
```
2. `hostGetNum` / `hostGetStr` already call `h.num(...)` / `h.str(...)` — `WeatherHost` has those, no change.
3. Change the collection name in `hostRowCount` and `hostGetRowStr` from `"playlist"` to `"daily"`:
```swift
func hostRowCount(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?) -> UInt32 {
    guard let col = col, let h = hostBox.host, String(cString: col) == "daily" else { return 0 }
    return UInt32(h.rowCount())
}
func hostGetRowStr(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?, _ index: UInt32, _ field: UnsafePointer<CChar>?, _ buf: UnsafeMutablePointer<CChar>?, _ cap: UInt) -> Bool {
    guard let col = col, let field = field, let buf = buf, let h = hostBox.host,
          String(cString: col) == "daily" else { return false }
    guard let v = h.rowString(Int(index), field: String(cString: field)) else { return false }
    return writeCString(v, buf, cap)
}
```
4. Replace the whole `hostInvoke` body's music/`invoke_arg` cases with just window actions (M1 has no host actions beyond minimize/close; `begin_drag` is view-handled):
```swift
func hostInvoke(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?) {
    guard let action = action else { return }
    switch String(cString: action) {
    case "minimize": DispatchQueue.main.async { windowBox.window?.miniaturize(nil) }
    case "close": DispatchQueue.main.async { NSApp.terminate(nil) }
    case "begin_drag": break // window drag is handled from the view's mouse events
    default: break
    }
}
```
5. Simplify `hostInvokeArg` to a no-op (no numeric actions in M1):
```swift
func hostInvokeArg(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?, _ arg: Double) {}
```
(`hostGetRowNum` already returns `false` — keep it.)

- [ ] **Step 5: Build to verify the scaffold compiles**

Run: `cargo build -p carapace-ffi && cd weather && swift build`
Expected: `Build complete!` (the placeholder main still owns `@main`; the scaffold just compiles alongside it).

- [ ] **Step 6: Commit**

```bash
git add weather/Sources/Weather/SkinWindow.swift weather/Sources/Weather/SkinView.swift weather/Sources/Weather/CarapaceBridge.swift weather/Sources/Weather/HostCallbacks.swift
git commit -m "feat(weather): display scaffold (SkinWindow/SkinView/CarapaceBridge/HostCallbacks)"
```

---

### Task 4: The weather skin (manifest + Lua UI + über-shader)

Author the skin: the `shader{}` über-background (six condition helpers, first pass), the hero/hourly/daily 2D UI, and a full-canvas drag region. The WGSL is naga-validated at skin load, so a syntax error fails `carapace_create` — Task 5 surfaces that.

**Files:**
- Create: `weather/skins/weather/skin.toml`
- Create: `weather/skins/weather/skin.lua`
- Create: `weather/skins/weather/assets/weather.wgsl`

**Interfaces:**
- Consumes: the host-data contract keys (Global Constraints).
- Produces: a loadable skin dir at `weather/skins/weather`.

- [ ] **Step 1: Write the manifest**

Create `weather/skins/weather/skin.toml`:
```toml
schema = 1
id = "weather"
name = "Weather"
engine = "^0.1"
canvas = { width = 400, height = 680 }
entry = "skin.lua"
```

- [ ] **Step 2: Write the über-shader**

Create `weather/skins/weather/assets/weather.wgsl` (fragment-only; the engine supplies `vs`/`VsOut` and generates `struct U { time, res, condition, is_day, temp, intensity, season }`). First-pass, recognizable, animated; M3 polishes:
```wgsl
// Hash/noise helpers (value noise).
fn hash21(p: vec2<f32>) -> f32 {
    var h = dot(p, vec2<f32>(127.1, 311.7));
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

// Sky gradient tinted by day/night.
fn sky(uv: vec2<f32>, day: f32) -> vec3<f32> {
    let top_day = vec3<f32>(0.30, 0.55, 0.9);
    let bot_day = vec3<f32>(0.75, 0.85, 0.95);
    let top_night = vec3<f32>(0.03, 0.04, 0.12);
    let bot_night = vec3<f32>(0.08, 0.10, 0.20);
    let top = mix(top_night, top_day, day);
    let bot = mix(bot_night, bot_day, day);
    return mix(top, bot, uv.y);
}

fn clear_c(uv: vec2<f32>, t: f32, day: f32) -> vec3<f32> {
    var col = sky(uv, day);
    // A soft sun/moon disc drifting slightly.
    let c = vec2<f32>(0.72, 0.24 + 0.02 * sin(t * 0.3));
    let d = distance(uv, c);
    let disc = smoothstep(0.14, 0.10, d);
    let glow = smoothstep(0.5, 0.0, d) * 0.35;
    let sun = mix(vec3<f32>(1.0, 0.95, 0.75), vec3<f32>(0.85, 0.9, 1.0), 1.0 - day);
    col = col + sun * (disc + glow);
    return col;
}

fn cloud_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = sky(uv, day) * 0.9;
    let n = fbm(uv * vec2<f32>(3.0, 2.0) + vec2<f32>(t * 0.05, 0.0));
    let cover = smoothstep(0.4, 0.8, n) * (0.5 + 0.5 * intensity);
    let cloud = mix(vec3<f32>(0.6, 0.62, 0.68), vec3<f32>(0.85, 0.87, 0.9), day);
    return mix(col, cloud, cover);
}

fn rain_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = cloud_c(uv, t, day, 0.8) * 0.8;
    // Streaks: repeated diagonal lines scrolling down.
    let sc = uv * vec2<f32>(60.0, 30.0) + vec2<f32>(uv.y * 8.0, -t * 12.0);
    let line = fract(sc.x + floor(sc.y) * 0.5);
    let streak = smoothstep(0.96, 1.0, 1.0 - abs(line - 0.5) * 2.0) * (0.3 + intensity);
    col = col + vec3<f32>(0.6, 0.7, 0.85) * streak * 0.25;
    return col;
}

fn snow_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = cloud_c(uv, t, day, 0.5) * vec3<f32>(0.9, 0.93, 1.0);
    var flakes = 0.0;
    for (var k = 0; k < 3; k = k + 1) {
        let fk = f32(k);
        let p = uv * (10.0 + fk * 6.0) + vec2<f32>(sin(t * 0.5 + fk) * 0.5, t * (0.15 + fk * 0.05));
        let g = hash21(floor(p));
        let f = fract(p) - 0.5;
        flakes = flakes + smoothstep(0.08, 0.0, length(f)) * step(0.85, g);
    }
    return col + vec3<f32>(1.0) * flakes * (0.4 + intensity);
}

fn storm_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    var col = rain_c(uv, t, day, 1.0) * 0.5;
    // Occasional lightning flash: a fast pulse gating on a time hash.
    let strike = step(0.985, hash21(vec2<f32>(floor(t * 2.0), 3.0)));
    let flash = strike * (0.5 + 0.5 * sin(t * 40.0)) * 0.6;
    col = col + vec3<f32>(0.9, 0.9, 1.0) * flash;
    return col;
}

fn fog_c(uv: vec2<f32>, t: f32, day: f32, intensity: f32) -> vec3<f32> {
    let base = mix(vec3<f32>(0.5, 0.52, 0.55), vec3<f32>(0.8, 0.82, 0.85), day);
    let n = fbm(uv * 2.0 + vec2<f32>(t * 0.03, t * 0.01));
    return mix(base * 0.9, base, n);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.time;
    let day = clamp(u.is_day, 0.0, 1.0);
    let intensity = clamp(u.intensity, 0.0, 1.0);
    var col: vec3<f32>;
    switch (i32(u.condition)) {
        case 0: { col = clear_c(uv, t, day); }
        case 1: { col = cloud_c(uv, t, day, intensity); }
        case 2: { col = rain_c(uv, t, day, intensity); }
        case 3: { col = snow_c(uv, t, day, intensity); }
        case 4: { col = storm_c(uv, t, day, intensity); }
        case 5: { col = fog_c(uv, t, day, intensity); }
        default: { col = clear_c(uv, t, day); }
    }
    // Warm/cool tint from temperature (raw °C): cold → blue, hot → amber.
    let warmth = clamp((u.temp - 10.0) / 25.0, -0.3, 0.3);
    col = col + vec3<f32>(warmth, 0.0, -warmth) * 0.15;
    // Opaque background (premultiplied by alpha = 1).
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
```

- [ ] **Step 3: Write the skin Lua (shader bg + drag + hero + hourly + daily)**

Create `weather/skins/weather/skin.lua`:
```lua
local W, H = 400, 680

-- Animated WGSL background (renders UNDER the 2D UI). Host-bound numeric uniforms.
shader{ src = "weather.wgsl", x = 0, y = 0, w = W, h = H,
        uniforms = { condition = "wx_condition", is_day = "wx_is_day",
                     temp = "wx_temp", intensity = "wx_intensity", season = "wx_season" } }

-- Whole-window drag (the skin IS the window). Controls drawn later win hit-testing.
region{ path = rect{ x = 0, y = 0, w = W, h = H }, role = "drag",
        on_press = function() host.begin_drag() end }

-- Hero block.
text{ value = "location", x = 28, y = 40, size = 26, color = { r = 245, g = 247, b = 252 } }
text{ value = "condition_text", x = 28, y = 74, size = 14, color = { r = 210, g = 216, b = 230 } }
text{ value = "temp_now", x = 28, y = 108, size = 72, color = { r = 255, g = 255, b = 255 } }
text{ value = "hi_lo", x = 30, y = 196, size = 14, color = { r = 225, g = 230, b = 240 } }
text{ value = "feels", x = 160, y = 196, size = 14, color = { r = 225, g = 230, b = 240 } }

-- Horizontal hourly strip: 12 cells (time above, temp below), evenly spaced.
local hourly_y = 250
local n = 12
local pad = 20
local step = (W - pad * 2) / n
for i = 0, n - 1 do
  local cx = pad + step * i + step / 2
  text{ value = "wx_hour_" .. i .. "_time", x = cx, y = hourly_y, size = 11,
        halign = "center", color = { r = 205, g = 212, b = 226 } }
  text{ value = "wx_hour_" .. i .. "_temp", x = cx, y = hourly_y + 20, size = 13,
        halign = "center", color = { r = 245, g = 247, b = 252 } }
end

-- Vertical daily forecast list (collection = "daily").
list{ collection = "daily", x = 24, y = 320, w = W - 48, h = 300, row_height = 40,
      template = {
        { bind = "day",   x = 8,        y = 10, size = 16, color = { r = 240, g = 244, b = 252 } },
        { bind = "glyph", x = 120,      y = 8,  size = 18, color = { r = 245, g = 240, b = 220 } },
        { bind = "hi",    right = 70,   y = 10, size = 16, color = { r = 245, g = 247, b = 252 } },
        { bind = "lo",    right = 10,   y = 10, size = 16, halign = "right", color = { r = 190, g = 198, b = 214 } },
      } }
```

- [ ] **Step 4: (Verification happens in Task 5.)**

The skin can only be validated by loading it through the engine, which Task 5 does when it creates the bridge. If `carapace_create` fails, Task 5's launch prints the naga/skin error via `carapace_last_error`. No separate step here.

- [ ] **Step 5: Commit**

```bash
git add weather/skins/weather
git commit -m "feat(weather): weather skin — über-shader + hero/hourly/daily UI"
```

---

### Task 5: App bootstrap — borderless skin-as-window, wired to the mock host

Replace the placeholder main with the real `@main` app: a borderless window whose content view is the `SkinView`, a `CarapaceBridge` loading `skins/weather` with `WeatherHost(.sample)`, real traffic-light buttons, and a debug key that cycles `wx_condition` to eyeball all six backgrounds.

**Files:**
- Delete: `weather/Sources/Weather/main_placeholder.swift`
- Create: `weather/Sources/Weather/App.swift`

**Interfaces:**
- Consumes: `SkinWindow`, `SkinView`, `CarapaceBridge`, `WeatherHost`, `hostBox`, `windowBox` (Tasks 2–3); the `skins/weather` dir (Task 4).

- [ ] **Step 1: Remove the placeholder**

Run: `git rm weather/Sources/Weather/main_placeholder.swift`

- [ ] **Step 2: Write the app bootstrap**

Create `weather/Sources/Weather/App.swift`:
```swift
import SwiftUI
import AppKit

@main
struct WeatherApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    var body: some Scene { Settings { EmptyView() } } // AppDelegate owns the skin window
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var window: SkinWindow!
    private var view: SkinView!
    private var host: WeatherHost!
    private var bridge: CarapaceBridge!
    private var trafficLightButtons: [NSButton] = []

    private let canvasW = 400
    private let canvasH = 680

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)
        installMainMenu()

        host = WeatherHost(model: .sample)
        hostBox.host = host

        view = SkinView(frame: NSRect(x: 0, y: 0, width: canvasW, height: canvasH))
        view.canvasW = Double(canvasW)
        view.canvasH = Double(canvasH)
        view.onKey = { [weak self] code in self?.handleKey(code) }

        window = SkinWindow(contentRect: NSRect(x: 200, y: 200, width: canvasW, height: canvasH),
                            styleMask: [.borderless, .closable, .miniaturizable],
                            backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = view
        windowBox.window = window

        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())
        guard let b = CarapaceBridge(skinDir: skinDir(), width: canvasW * scale, height: canvasH * scale,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            var msg = [CChar](repeating: 0, count: 256)
            _ = carapace_last_error(&msg, UInt(msg.count))
            fatalError("weather: bridge/skin load failed: \(String(cString: msg))")
        }
        bridge = b
        view.bridge = b

        installTrafficLights()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Absolute path to weather/skins/weather (App.swift is weather/Sources/Weather/App.swift).
    private func skinDir() -> String {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()   // Weather
            .deletingLastPathComponent()   // Sources
            .deletingLastPathComponent()   // weather (package root)
            .appendingPathComponent("skins/weather").path
    }

    // Debug (M1 verification scaffolding): → / ← cycle the mock condition to eyeball all six shaders.
    private func handleKey(_ code: UInt16) {
        let delta: Double
        switch code {
        case 124: delta = 1   // right arrow
        case 123: delta = -1  // left arrow
        default: return
        }
        var c = host.model.condition + delta
        if c > 5 { c = 0 }; if c < 0 { c = 5 }
        host.model.condition = c
    }

    private func installTrafficLights() {
        let mask: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable]
        let specs: [(NSWindow.ButtonType, Selector?)] = [
            (.closeButton, #selector(NSWindow.performClose(_:))),
            (.miniaturizeButton, #selector(NSWindow.miniaturize(_:))),
            (.zoomButton, nil),  // greyed: a fixed-canvas borderless skin can't zoom
        ]
        for (type, action) in specs {
            guard let b = NSWindow.standardWindowButton(type, for: mask) else { continue }
            if let action { b.target = window; b.action = action } else { b.isEnabled = false }
            b.autoresizingMask = []
            view.addSubview(b)
            trafficLightButtons.append(b)
        }
        let ox: CGFloat = 16, oy: CGFloat = 14
        for (i, b) in trafficLightButtons.enumerated() {
            b.setFrameOrigin(NSPoint(x: ox + CGFloat(i) * 20,
                                     y: view.bounds.height - oy - b.frame.height))
        }
    }

    private func installMainMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit CarapaceWeather",
                        action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)
        NSApp.mainMenu = main
    }
}
```

- [ ] **Step 3: Build**

Run: `cargo build -p carapace-ffi && cd weather && swift build`
Expected: `Build complete!` (now `WeatherApp` owns `@main`; the placeholder is gone).

- [ ] **Step 4: Launch and eyeball (skin-as-window with mock data)**

The app opens a GUI window, so launch it into the Aqua session (a plain background launch shows "0 windows"). Run:
```bash
cargo build -p carapace-ffi && (cd weather && swift build)
launchctl asuser 501 /bin/zsh -lc 'exec weather/.build/debug/Weather' &
```
Then verify (screenshot the window by its window-server id, as done for the demo):
- A **borderless, portrait** window appears with the animated shader background and the hero (`Accra`, `Partly cloudy`, `27°`, `H:31° L:24°`, `Feels 30°`), the 12-cell hourly strip, and the 7-row daily list.
- Press **→** several times: the background cycles through all six conditions (clear/cloud/rain/snow/storm/fog), each visibly different and animated.
- **Drag** the window by its background; the window moves.
- The **traffic lights** (top-left) close / minimize.

If `carapace_create` failed, the fatalError prints the skin/naga error — fix the WGSL/Lua in Task 4 and rebuild.

- [ ] **Step 5: Commit**

```bash
git add weather/Sources/Weather/App.swift
git rm weather/Sources/Weather/main_placeholder.swift 2>/dev/null || true
git commit -m "feat(weather): app bootstrap — borderless skin-as-window on WeatherModel.sample"
```

---

### Task 6: Milestone-1 gate + PR

**Files:** none (verification + push).

- [ ] **Step 1: Full local gate**

Run:
```bash
cargo build -p carapace-ffi
cd weather && swift build && swift test
```
Expected: dylib built; `Build complete!`; `WeatherHostTests` all pass.

- [ ] **Step 2: Final eyeball**

Re-run the Task 5 launch/eyeball once more on the built binary to confirm the window renders, cycles conditions, drags, and closes cleanly.

- [ ] **Step 3: Push + draft PR**

```bash
git push -u origin weather-app-showcase
gh pr create --draft --base main --head weather-app-showcase \
  --title "feat(weather): weather app showcase — Milestone 1 (skin-as-window app shell)" \
  --body "Implements Milestone 1 of docs/superpowers/specs/2026-07-10-weather-app-showcase-design.md: a new standalone macOS app (weather/) whose skin IS the borderless window. Renders skins/weather (animated shader{} background + hero/hourly/daily UI) driven by a static WeatherHost(.sample) — no network. Reuses the Showcase display scaffold; zero engine-crate changes. Follow-ups: M2 live Open-Meteo data, M3 condition tour + bottom-flowing silhouette, M4 location search cutout."
```

---

## Self-Review

**Spec coverage:**
- New standalone macOS SwiftPM app parallel to showcase/ → Task 1. ✓
- Skin-as-window (borderless/transparent/shaped/draggable, total window replacement) → Task 5 (borderless window + clear bg + drag region in Task 4 Lua). ✓
- Reuse Showcase display scaffold (SkinWindow/SkinView/CarapaceBridge/HostCallbacks), zero engine changes → Task 3. ✓
- IOSurface display path (frame_ready → layer.contents) → Task 3 (copied bridge + view). ✓
- One über-shader, condition switch + is_day/temp/intensity/season uniforms → Task 4 weather.wgsl. ✓
- Host-data contract (numeric uniforms, hero/hourly strings, daily rows) → Task 2 (WeatherHost) + Task 3 (HostCallbacks wiring) + Task 4 (skin bindings). ✓
- Hero + hourly strip + daily list UI → Task 4 skin.lua. ✓
- WeatherModel.sample / WeatherHost, mock-only (no network) → Task 2. ✓
- Real traffic lights + drag → Task 5 + Task 4. ✓
- Build/link mirrors showcase/Package.swift (CARAPACE_APPLE, C23, -lcarapace_ffi) → Task 1. ✓
- Verification via launchctl asuser + window-id capture → Task 5/6. ✓
- Deferred (M2–M4): live fetch, geocoding, search cutout, bottom-flow, crossfades → not in any task (correctly out of scope). ✓

**Placeholder scan:** No "TBD/TODO/handle appropriately". Task 4 Step 4 intentionally has no action (validation is inherent to Task 5's load) — stated explicitly, not a placeholder. All code blocks are complete and compile-ready.

**Type consistency:** `WeatherHost.{num,str,rowCount,rowString}` defined in Task 2 are exactly the methods `HostCallbacks` calls in Task 3 (`h.num`, `h.str`, `h.rowCount`, `h.rowString`). `hostBox.host: WeatherHost?` (Task 3) matches `WeatherHost` (Task 2). `CarapaceBridge.init?(skinDir:width:height:onFrame:)` (Task 3, contentSurface dropped) matches the call in Task 5. `SkinView.onKey: ((UInt16) -> Void)?` (Task 3) matches Task 5's `view.onKey = …` and `handleKey(_ code: UInt16)`. The skin's host keys (Task 4: `wx_condition/is_day/temp/intensity/season`, `location/condition_text/temp_now/hi_lo/feels`, `wx_hour_i_time/temp`, daily `day/glyph/hi/lo`) are exactly the keys `WeatherHost` answers (Task 2) and the collection name `"daily"` matches HostCallbacks (Task 3). Canvas 400×680 consistent across Task 4 (skin.toml/Lua) and Task 5 (window/view).

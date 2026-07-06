# Showcase SwiftUI App (Sub-project B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A native macOS SwiftUI app — the first real consumer of `carapace-ffi` — that embeds the carapace engine, renders a skin zero-copy via an IOSurface pool into a borderless/transparent/draggable window, with a Swift-owned music host (AVAudioPlayer) whose state survives skin hot-swaps.

**Architecture:** SwiftPM package `showcase/`. A `CCarapace` systemLibrary links `libcarapace_ffi.dylib`. Swift is the host: a `MusicHost` (playlist + AVAudioPlayer + state) is exposed to the engine through top-level C vtable callbacks (weak-box pattern). A `CarapaceBridge` owns the engine handle + IOSurface pool and drives display via `frame_ready`. An AppKit-owned borderless `SkinWindow`/`SkinView` displays frames and routes input through `carapace_hit_test` → drag/`carapace_pointer`. SwiftUI provides the `@main App` entry.

**Tech Stack:** Swift 6 (language mode 5), SwiftPM, AppKit, AVFoundation, IOSurface, Swift Testing, carapace-ffi (ABI 3.0).

## Global Constraints

- macOS only, `.macOS(.v13)`, Swift tools 6.0, `.swiftLanguageMode(.v5)`.
- Links `carapace-ffi` ABI 3.0 (`carapace_abi_version() == 3 << 16`). Build order: `cargo build -p carapace-ffi` THEN `swift build` (the dylib must exist at `target/debug/libcarapace_ffi.dylib`).
- Not CI-gated (no Swift in CI). Verification is `swift test` (host logic) + an agent-device / manual run.
- Git identity: `git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' commit ...`.
- All skins share ONE design canvas; the app is written for a fixed design canvas `W=420, H=660` (points) rendered into a backing-scale IOSurface pool. Window is sized `W×H` points.
- Host key/action surface (served by `MusicHost`): state `track_title`,`artist`,`time`,`position`,`volume`,`playing`,`current_index`,`viz_0..15`; collection `playlist` fields `now`,`title`,`artist`,`duration`; actions `toggle_play`,`stop`,`next`,`prev`,`begin_drag`,`minimize`,`close` (parameterless) and `seek`,`set_volume`,`play_index` (numeric via `invoke_arg`).
- Vtable is the 9-field v3 `CarapaceHostVTable`; create via `CarapaceCreateDesc`; display via `frame_ready` + `carapace_release_surface`; input via `carapace_hit_test` (Passthrough=0/Control=1/Drag=2) + `carapace_pointer(x,y,Press=0)`; hot-swap via `carapace_swap_skin`.

---

### Task 1: SwiftPM package + CCarapace link + ABI smoke test

**Files:**
- Create: `showcase/Package.swift`
- Create: `showcase/Sources/CCarapace/module.modulemap`
- Create: `showcase/Sources/CCarapace/include/carapace.h` (copied from carapace-ffi)
- Create: `showcase/Sources/CCarapace/include/dummy.h` (empty, umbrella not needed) — omit if module.modulemap points at carapace.h directly
- Create: `showcase/Sources/Showcase/main.swift` (temporary smoke `main`, replaced in Task 5)
- Create: `showcase/.gitignore` (`.build/`)

**Interfaces:**
- Produces: a buildable package whose `Showcase` executable links `carapace_ffi` and can call every `carapace_*` symbol + reference `CarapaceCreateDesc`/`CarapaceHostVTable`.

- [ ] **Step 1: Build the Rust dylib**

Run: `cargo build -p carapace-ffi`
Expected: `target/debug/libcarapace_ffi.dylib` exists (`ls target/debug/libcarapace_ffi.dylib`).

- [ ] **Step 2: Copy the header**

Run: `mkdir -p showcase/Sources/CCarapace/include && cp crates/carapace-ffi/include/carapace.h showcase/Sources/CCarapace/include/carapace.h`

- [ ] **Step 3: Write the module map**

`showcase/Sources/CCarapace/module.modulemap`:
```
module CCarapace {
    header "include/carapace.h"
    link "carapace_ffi"
    export *
}
```

- [ ] **Step 4: Write Package.swift**

`showcase/Package.swift`:
```swift
// swift-tools-version:6.0
import PackageDescription

let repoTarget = "../target/debug"  // dylib location relative to this package

let package = Package(
    name: "CarapaceShowcase",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CCarapace", path: "Sources/CCarapace"),
        .executableTarget(
            name: "Showcase",
            dependencies: ["CCarapace"],
            swiftSettings: [.swiftLanguageMode(.v5)],
            linkerSettings: [
                .unsafeFlags([
                    "-L", repoTarget, "-lcarapace_ffi",
                    "-Xlinker", "-rpath", "-Xlinker", repoTarget,
                ])
            ]
        ),
        .testTarget(name: "ShowcaseTests", dependencies: ["Showcase"]),
    ]
)
```

- [ ] **Step 5: Write the smoke main**

`showcase/Sources/Showcase/main.swift`:
```swift
import CCarapace

let v = carapace_abi_version()
let major = v >> 16
print("[showcase] carapace ABI \(major).\(v & 0xFFFF)")
precondition(major == 3, "expected carapace ABI major 3, got \(major)")
print("[showcase] linkage OK")
```

- [ ] **Step 6: Build + run to verify linkage**

Run (from `showcase/`): `swift build && swift run Showcase`
Expected: prints `carapace ABI 3.0` and `linkage OK`, exit 0. (If the dylib isn't found at runtime, confirm the rpath and that Step 1 ran.)

- [ ] **Step 7: Commit**

```bash
git add showcase/Package.swift showcase/Sources/CCarapace showcase/Sources/Showcase/main.swift showcase/.gitignore
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): SwiftPM package linking carapace-ffi (ABI 3.0 smoke)"
```

---

### Task 2: Track + MusicHost logic (TDD)

**Files:**
- Create: `showcase/Sources/Showcase/MusicHost.swift`
- Create: `showcase/Tests/ShowcaseTests/MusicHostTests.swift`

**Interfaces:**
- Produces:
  - `struct Track { let title: String; let artist: String; let url: URL; let duration: TimeInterval }`
  - `final class MusicHost` with: `init(playlist: [Track])`; `private(set) var current: Int`; `var volume: Double` (0–1); `private(set) var playing: Bool`; methods `togglePlay()`, `stop()`, `next()`, `prev()`, `play(index: Int)`, `seek(_ f: Double)`, `setVolume(_ f: Double)`; readers `positionFraction() -> Double`, `timeString() -> String`, `viz(_ i: Int) -> Double`, `rowCount() -> Int`, `rowString(_ i: Int, field: String) -> String?`, `str(_ key: String) -> String?`, `num(_ key: String) -> Double?`.
  - Playback is via an injected `AudioPlayer` protocol so logic is unit-testable without real audio.
- Consumes: nothing (Task 1 only provides the package).

- [ ] **Step 1: Write failing tests**

`showcase/Tests/ShowcaseTests/MusicHostTests.swift`:
```swift
import Testing
import Foundation
@testable import Showcase

private func demoPlaylist() -> [Track] {
    [
        Track(title: "One", artist: "Alpha", url: URL(fileURLWithPath: "/tmp/1.wav"), duration: 100),
        Track(title: "Two", artist: "Beta",  url: URL(fileURLWithPath: "/tmp/2.wav"), duration: 200),
        Track(title: "Three", artist: "Gamma", url: URL(fileURLWithPath: "/tmp/3.wav"), duration: 300),
    ]
}

@Test func next_prev_clamp_at_ends() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    #expect(h.current == 0)
    h.next(); #expect(h.current == 1)
    h.next(); #expect(h.current == 2)
    h.next(); #expect(h.current == 2)          // clamps at last
    h.prev(); #expect(h.current == 1)
    h.prev(); h.prev(); #expect(h.current == 0) // clamps at first
}

@Test func play_index_sets_current_and_starts() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.play(index: 2)
    #expect(h.current == 2)
    #expect(h.playing == true)
    #expect(h.play(index: 99) == ())          // out of range: no-op
    #expect(h.current == 2)
}

@Test func volume_and_seek_clamp_0_1() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.setVolume(1.5); #expect(h.volume == 1.0)
    h.setVolume(-1);  #expect(h.volume == 0.0)
    h.setVolume(0.3); #expect(abs(h.volume - 0.3) < 1e-9)
    h.seek(2.0)                                // clamps; no crash with fake player
    #expect(h.num("volume") == 0.3)
}

@Test func rows_expose_now_marker_and_fields() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    h.play(index: 1)
    #expect(h.rowCount() == 3)
    #expect(h.rowString(1, field: "now") == "▶")
    #expect(h.rowString(0, field: "now") == "")
    #expect(h.rowString(2, field: "title") == "Three")
    #expect(h.rowString(2, field: "artist") == "Gamma")
    #expect(h.rowString(1, field: "duration") == "3:20")
}

@Test func str_and_num_keys() {
    let h = MusicHost(playlist: demoPlaylist(), player: FakeAudioPlayer())
    #expect(h.str("track_title") == "One")
    #expect(h.str("artist") == "Alpha")
    #expect(h.num("playing") == 0.0)
    h.togglePlay(); #expect(h.num("playing") == 1.0)
    #expect(h.num("current_index") == 0.0)
    #expect(h.num("viz_0") != nil)             // some level
    #expect(h.str("nope") == nil)
    #expect(h.num("nope") == nil)
}

final class FakeAudioPlayer: AudioPlayer {
    var isPlaying = false
    var volume: Float = 1.0
    var currentTime: TimeInterval = 0
    var duration: TimeInterval = 0
    func load(_ url: URL, duration: TimeInterval) { self.duration = duration; currentTime = 0 }
    func play() { isPlaying = true }
    func pause() { isPlaying = false }
    func stop() { isPlaying = false; currentTime = 0 }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run (from `showcase/`): `swift test`
Expected: FAIL to compile — `MusicHost`/`Track`/`AudioPlayer` undefined.

- [ ] **Step 3: Implement MusicHost + Track + AudioPlayer**

`showcase/Sources/Showcase/MusicHost.swift`:
```swift
import Foundation

struct Track {
    let title: String
    let artist: String
    let url: URL
    let duration: TimeInterval
}

/// Playback backend abstraction so MusicHost logic is testable without real audio.
protocol AudioPlayer: AnyObject {
    var isPlaying: Bool { get }
    var volume: Float { get set }
    var currentTime: TimeInterval { get set }
    var duration: TimeInterval { get }
    func load(_ url: URL, duration: TimeInterval)
    func play()
    func pause()
    func stop()
}

private func fmtMMSS(_ t: TimeInterval) -> String {
    let s = Int(t.rounded(.down))
    return "\(s / 60):" + String(format: "%02d", s % 60)
}

/// Swift-owned music host — the single source of truth exposed to the engine over the vtable.
/// Survives skin swaps (the engine never owns this state).
final class MusicHost {
    private let playlist: [Track]
    private let player: AudioPlayer
    private(set) var current: Int = 0
    private(set) var playing: Bool = false
    private(set) var volume: Double = 0.8

    init(playlist: [Track], player: AudioPlayer) {
        self.playlist = playlist
        self.player = player
        self.player.volume = Float(volume)
        if let t = playlist.first { self.player.load(t.url, duration: t.duration) }
    }

    // MARK: actions
    private func loadCurrent(autoplay: Bool) {
        guard playlist.indices.contains(current) else { return }
        let t = playlist[current]
        player.load(t.url, duration: t.duration)
        if autoplay { player.play(); playing = true }
    }
    func togglePlay() {
        if playing { player.pause(); playing = false }
        else { player.play(); playing = true }
    }
    func stop() { player.stop(); playing = false }
    func next() { if current + 1 < playlist.count { current += 1; loadCurrent(autoplay: playing) } }
    func prev() { if current > 0 { current -= 1; loadCurrent(autoplay: playing) } }
    func play(index: Int) {
        guard playlist.indices.contains(index) else { return }
        current = index; loadCurrent(autoplay: true)
    }
    func seek(_ f: Double) {
        let frac = min(max(f, 0), 1)
        player.currentTime = frac * player.duration
    }
    func setVolume(_ f: Double) {
        volume = min(max(f, 0), 1)
        player.volume = Float(volume)
    }

    // MARK: readers
    func positionFraction() -> Double {
        let d = player.duration
        return d > 0 ? min(max(player.currentTime / d, 0), 1) : 0
    }
    func timeString() -> String { "\(fmtMMSS(player.currentTime)) / \(fmtMMSS(player.duration))" }
    func viz(_ i: Int) -> Double {
        guard playing else { return 0.05 }
        let t = player.currentTime
        let fi = Double(i)
        let base = (1 - fi / 16) * 0.45 + 0.18
        let wobble = 0.55 * sin(t * (4 + fi * 0.6) + fi) + 0.30 * sin(t * (9 + fi * 0.27))
        return min(max(base + wobble * 0.4, 0.05), 1)
    }

    // MARK: collection
    func rowCount() -> Int { playlist.count }
    func rowString(_ i: Int, field: String) -> String? {
        guard playlist.indices.contains(i) else { return nil }
        let t = playlist[i]
        switch field {
        case "now": return i == current ? "▶" : ""
        case "title": return t.title
        case "artist": return t.artist
        case "duration": return fmtMMSS(t.duration)
        default: return nil
        }
    }

    // MARK: state keys
    func str(_ key: String) -> String? {
        switch key {
        case "track_title": return playlist.indices.contains(current) ? playlist[current].title : ""
        case "artist": return playlist.indices.contains(current) ? playlist[current].artist : ""
        case "time": return timeString()
        default: return nil
        }
    }
    func num(_ key: String) -> Double? {
        switch key {
        case "position": return positionFraction()
        case "volume": return volume
        case "playing": return playing ? 1 : 0
        case "current_index": return Double(current)
        default:
            if key.hasPrefix("viz_"), let i = Int(key.dropFirst(4)) { return viz(i) }
            return nil
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run (from `showcase/`): `swift test`
Expected: PASS — all `MusicHostTests` green, output pristine.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/MusicHost.swift showcase/Tests/ShowcaseTests/MusicHostTests.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): MusicHost state/actions/rows over the host surface (TDD)"
```

---

### Task 3: Vtable callbacks (weak-box) → MusicHost

**Files:**
- Create: `showcase/Sources/Showcase/HostCallbacks.swift`
- Create: `showcase/Tests/ShowcaseTests/HostCallbacksTests.swift`

**Interfaces:**
- Consumes: `MusicHost` (Task 2).
- Produces:
  - `final class HostBox { weak var host: MusicHost? }` and `let hostBox = HostBox()` (global, set once at startup).
  - Top-level C funcs matching the vtable: `hostGetNum`, `hostGetStr`, `hostRowCount`, `hostGetRowStr`, `hostGetRowNum`, `hostInvoke`, `hostInvokeArg` — signatures exactly matching `CarapaceHostVTable`.
  - `func makeVTable(frameReady: @escaping @convention(c) (UnsafeMutableRawPointer?, UInt32, UInt64) -> Void) -> CarapaceHostVTable`.

- [ ] **Step 1: Write failing tests**

`showcase/Tests/ShowcaseTests/HostCallbacksTests.swift`:
```swift
import Testing
import Foundation
@testable import Showcase

private func withHost(_ body: (MusicHost) -> Void) {
    let h = MusicHost(playlist: [
        Track(title: "One", artist: "Alpha", url: URL(fileURLWithPath: "/tmp/1.wav"), duration: 100),
        Track(title: "Two", artist: "Beta",  url: URL(fileURLWithPath: "/tmp/2.wav"), duration: 200),
    ], player: FakeAudioPlayer())
    hostBox.host = h
    defer { hostBox.host = nil }
    body(h)
}

@Test func get_num_reads_volume() {
    withHost { h in
        h.setVolume(0.5)
        var out = 0.0
        let ok = "volume".withCString { hostGetNum(nil, $0, &out) }
        #expect(ok); #expect(out == 0.5)
        let miss = "nope".withCString { hostGetNum(nil, $0, &out) }
        #expect(!miss)
    }
}

@Test func get_str_writes_title_nul_terminated() {
    withHost { _ in
        var buf = [CChar](repeating: 0, count: 64)
        let ok = "track_title".withCString { hostGetStr(nil, $0, &buf, buf.count) }
        #expect(ok)
        #expect(String(cString: buf) == "One")
    }
}

@Test func rows_via_callbacks() {
    withHost { h in
        h.play(index: 1)
        #expect("playlist".withCString { hostRowCount(nil, $0) } == 2)
        var buf = [CChar](repeating: 0, count: 32)
        let ok = "playlist".withCString { col in "now".withCString { f in
            hostGetRowStr(nil, col, 1, f, &buf, buf.count) } }
        #expect(ok); #expect(String(cString: buf) == "▶")
    }
}

@Test func invoke_and_invoke_arg_route_to_host() {
    withHost { h in
        "toggle_play".withCString { hostInvoke(nil, $0) }
        #expect(h.playing == true)
        "set_volume".withCString { hostInvokeArg(nil, $0, 0.25) }
        #expect(h.volume == 0.25)
        "play_index".withCString { hostInvokeArg(nil, $0, 1) }
        #expect(h.current == 1)
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run (from `showcase/`): `swift test --filter HostCallbacks`
Expected: FAIL to compile — `hostBox`/`hostGetNum`/etc. undefined.

- [ ] **Step 3: Implement HostCallbacks.swift**

`showcase/Sources/Showcase/HostCallbacks.swift`:
```swift
import Foundation
import CCarapace

/// Weak box so the top-level C callbacks (no captured context) can reach the live host.
final class HostBox { weak var host: MusicHost? }
let hostBox = HostBox()

private func writeCString(_ s: String, _ buf: UnsafeMutablePointer<CChar>, _ cap: Int) -> Bool {
    guard cap > 0 else { return false }
    let bytes = Array(s.utf8)
    let n = min(bytes.count, cap - 1)
    for i in 0..<n { buf[i] = CChar(bitPattern: bytes[i]) }
    buf[n] = 0
    return true
}

func hostGetNum(_ ctx: UnsafeMutableRawPointer?, _ key: UnsafePointer<CChar>?, _ out: UnsafeMutablePointer<Double>?) -> Bool {
    guard let key = key, let out = out, let h = hostBox.host else { return false }
    guard let v = h.num(String(cString: key)) else { return false }
    out.pointee = v
    return true
}

func hostGetStr(_ ctx: UnsafeMutableRawPointer?, _ key: UnsafePointer<CChar>?, _ buf: UnsafeMutablePointer<CChar>?, _ cap: Int) -> Bool {
    guard let key = key, let buf = buf, let h = hostBox.host else { return false }
    guard let v = h.str(String(cString: key)) else { return false }
    return writeCString(v, buf, cap)
}

func hostRowCount(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?) -> UInt32 {
    guard let col = col, let h = hostBox.host, String(cString: col) == "playlist" else { return 0 }
    return UInt32(h.rowCount())
}

func hostGetRowStr(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?, _ index: UInt32, _ field: UnsafePointer<CChar>?, _ buf: UnsafeMutablePointer<CChar>?, _ cap: Int) -> Bool {
    guard let col = col, let field = field, let buf = buf, let h = hostBox.host,
          String(cString: col) == "playlist" else { return false }
    guard let v = h.rowString(Int(index), field: String(cString: field)) else { return false }
    return writeCString(v, buf, cap)
}

func hostGetRowNum(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?, _ index: UInt32, _ field: UnsafePointer<CChar>?, _ out: UnsafeMutablePointer<Double>?) -> Bool {
    // playlist has no numeric fields today; string fields only.
    return false
}

func hostInvoke(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?) {
    guard let action = action, let h = hostBox.host else { return }
    switch String(cString: action) {
    case "toggle_play": h.togglePlay()
    case "stop": h.stop()
    case "next": h.next()
    case "prev": h.prev()
    case "minimize": DispatchQueue.main.async { windowBox.window?.miniaturize(nil) }
    case "close": DispatchQueue.main.async { NSApp.terminate(nil) }
    case "begin_drag": break // window drag is handled from the view's mouse events
    default: break
    }
}

func hostInvokeArg(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?, _ arg: Double) {
    guard let action = action, let h = hostBox.host else { return }
    switch String(cString: action) {
    case "seek": h.seek(arg)
    case "set_volume": h.setVolume(arg)
    case "play_index": h.play(index: Int(arg))
    default: break
    }
}

/// Assemble the v3 vtable. `frame_ready` is supplied by the bridge (Task 4).
func makeVTable(frameReady: @escaping @convention(c) (UnsafeMutableRawPointer?, UInt32, UInt64) -> Void) -> CarapaceHostVTable {
    CarapaceHostVTable(
        ctx: nil,
        get_num: hostGetNum,
        get_str: hostGetStr,
        invoke: hostInvoke,
        frame_ready: frameReady,
        row_count: hostRowCount,
        get_row_str: hostGetRowStr,
        get_row_num: hostGetRowNum,
        invoke_arg: hostInvokeArg
    )
}
```

Note: `windowBox` and `NSApp` are referenced for `minimize`/`close`. Add `import AppKit` to this file and declare the window weak box here so it exists before Task 5:
```swift
import AppKit
final class WindowBox { weak var window: NSWindow? }
let windowBox = WindowBox()
```
(Task 5 sets `windowBox.window`.)

- [ ] **Step 4: Run tests to verify they pass**

Run (from `showcase/`): `swift test --filter HostCallbacks`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/HostCallbacks.swift showcase/Tests/ShowcaseTests/HostCallbacksTests.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): v3 vtable callbacks bridging MusicHost over the C ABI"
```

---

### Task 4: CarapaceBridge — IOSurface pool + create/display/input/swap

**Files:**
- Create: `showcase/Sources/Showcase/CarapaceBridge.swift`

**Interfaces:**
- Consumes: `makeVTable` (Task 3), `carapace_*` (CCarapace), the `SkinView` display hook (Task 5) via a callback.
- Produces:
  - `final class CarapaceBridge` with `init?(skinDir: String, width: Int, height: Int, onFrame: @escaping (IOSurface, UInt32) -> Void)` (returns nil on create failure), and methods `pointer(x: Double, y: Double)`, `hitTest(x: Double, y: Double) -> CarapaceHitKind`, `swap(skinDir: String) -> Bool`, `releaseSurface(_ index: UInt32)`, `deinit` → `carapace_destroy`.
  - `width`/`height` are SURFACE pixels (design canvas × backing scale).
  - Holds `surfaces: [IOSurface]` (pool of 3) kept alive for the engine's lifetime.

- [ ] **Step 1: Implement CarapaceBridge.swift**

`showcase/Sources/Showcase/CarapaceBridge.swift`:
```swift
import Foundation
import IOSurface
import CCarapace

/// Global frame sink so the C `frame_ready` callback (no captured context) can deliver frames.
/// Set by the single live CarapaceBridge. Fired on the render thread → we hop to main.
final class FrameSink { var onFrame: ((IOSurface, UInt32) -> Void)? ; var surfaces: [IOSurface] = [] }
let frameSink = FrameSink()

private func onFrameReady(_ ctx: UnsafeMutableRawPointer?, _ index: UInt32, _ frameID: UInt64) {
    // Runs on carapace's render thread. Must NOT call any carapace_* here. Hop to main to display.
    let idx = Int(index)
    guard idx < frameSink.surfaces.count else { return }
    let surface = frameSink.surfaces[idx]
    DispatchQueue.main.async {
        frameSink.onFrame?(surface, index)
    }
}

final class CarapaceBridge {
    private var engine: OpaquePointer?
    private let surfaces: [IOSurface]
    let width: Int
    let height: Int

    init?(skinDir: String, width: Int, height: Int, onFrame: @escaping (IOSurface, UInt32) -> Void) {
        self.width = width
        self.height = height
        // Pool of 3 BGRA IOSurfaces at surface pixel size.
        var pool: [IOSurface] = []
        for _ in 0..<3 {
            guard let s = IOSurface(properties: [
                .width: width, .height: height, .bytesPerElement: 4,
                .pixelFormat: 0x42475241 as UInt32, // 'BGRA'
            ]) else { return nil }
            pool.append(s)
        }
        self.surfaces = pool
        frameSink.surfaces = pool
        frameSink.onFrame = onFrame

        let vt = makeVTable(frameReady: onFrameReady)
        // surfaces array as [IOSurfaceRef] for the create desc.
        var refs: [IOSurfaceRef] = pool.map { $0 as IOSurfaceRef }
        let ok = refs.withUnsafeBufferPointer { buf -> Bool in
            skinDir.withCString { dir -> Bool in
                var desc = CarapaceCreateDesc(
                    skin_dir: dir,
                    vtable: vt,
                    surfaces: buf.baseAddress,
                    surface_count: UInt32(buf.count),
                    content_surface: nil,
                    w: UInt32(width), h: UInt32(height)
                )
                var out: OpaquePointer?
                let status = carapace_create(&desc, &out)
                if status == Ok, let e = out { self.engine = e; return true }
                return false
            }
        }
        if !ok {
            var msg = [CChar](repeating: 0, count: 256)
            _ = carapace_last_error(&msg, msg.count)
            print("[showcase] carapace_create failed: \(String(cString: msg))")
            return nil
        }
    }

    func pointer(x: Double, y: Double) {
        guard let e = engine else { return }
        _ = carapace_pointer(e, x, y, Press) // Press = 0
    }

    func hitTest(x: Double, y: Double) -> CarapaceHitKind {
        guard let e = engine else { return Passthrough }
        var kind = Passthrough
        _ = carapace_hit_test(e, x, y, &kind)
        return kind
    }

    func swap(skinDir: String) -> Bool {
        guard let e = engine else { return false }
        return skinDir.withCString { carapace_swap_skin(e, $0) } == Ok
    }

    func releaseSurface(_ index: UInt32) {
        guard let e = engine else { return }
        _ = carapace_release_surface(e, index)
    }

    deinit {
        if let e = engine { carapace_destroy(e) }
    }
}
```

- [ ] **Step 2: Build to verify it compiles against the ABI**

Run (from `showcase/`): `swift build`
Expected: builds (the enum constants `Ok`, `Press`, `Passthrough`, and structs `CarapaceCreateDesc`/`CarapaceHostVTable` resolve from CCarapace). If `Ok`/`Press`/`Passthrough` don't resolve as bare names, qualify them as the imported C enum constants (`CarapaceStatus(0)` etc.) — check the generated interface and adjust; do not invent values (Ok=0, Press=0, Passthrough=0 per the header).

- [ ] **Step 3: Commit**

```bash
git add showcase/Sources/Showcase/CarapaceBridge.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): CarapaceBridge — IOSurface pool, create, frame_ready display, swap"
```

---

### Task 5: SkinWindow + SkinView + SwiftUI app entry

**Files:**
- Create: `showcase/Sources/Showcase/SkinWindow.swift`
- Create: `showcase/Sources/Showcase/SkinView.swift`
- Create: `showcase/Sources/Showcase/App.swift`
- Delete: `showcase/Sources/Showcase/main.swift` (replaced by `App.swift`'s `@main`)

**Interfaces:**
- Consumes: `CarapaceBridge` (Task 4), `MusicHost`/`hostBox` (Tasks 2–3), `windowBox` (Task 3), the playlist builder (Task 6 provides real tracks; Task 5 may use a temporary one-track list).
- Produces: a launching app that creates the host + bridge, shows a borderless window, displays frames, and routes input.
- Design canvas constants: `let CANVAS_W = 420`, `let CANVAS_H = 660`.

- [ ] **Step 1: Implement SkinWindow.swift**

```swift
import AppKit

/// Borderless windows can't become key/main by default; override so the skin receives input.
final class SkinWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
```

- [ ] **Step 2: Implement SkinView.swift**

```swift
import AppKit
import IOSurface

let CANVAS_W = 420
let CANVAS_H = 660

/// Layer-backed view that displays carapace IOSurface frames and routes input via hit-test.
final class SkinView: NSView {
    var bridge: CarapaceBridge?
    private var lastShown: UInt32?
    private var dragOrigin: NSPoint?
    private var didDrag = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.isOpaque = false
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.contentsGravity = .resizeAspect
    }
    required init?(coder: NSCoder) { fatalError() }
    override var isFlipped: Bool { false }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var acceptsFirstResponder: Bool { true }

    /// Called (on main) by the bridge when a frame lands. Rotate CALayer contents + release prev.
    func show(surface: IOSurface, index: UInt32) {
        guard let l = layer else { return }
        l.contents = surface
        let sel = Selector(("setContentsChanged"))
        if l.responds(to: sel) { l.perform(sel) } // force refresh of same-identity IOSurface
        if let prev = lastShown, prev != index { bridge?.releaseSurface(prev) }
        lastShown = index
    }

    private func canvasPoint(_ e: NSEvent) -> (Double, Double) {
        let p = convert(e.locationInWindow, from: nil)
        let cx = Double(p.x) * Double(CANVAS_W) / Double(bounds.width)
        let cy = (Double(bounds.height) - Double(p.y)) * Double(CANVAS_H) / Double(bounds.height)
        return (cx, cy)
    }

    override func mouseDown(with e: NSEvent) {
        window?.makeKey()
        let (cx, cy) = canvasPoint(e)
        switch bridge?.hitTest(x: cx, y: cy) {
        case .some(Drag):
            dragOrigin = window?.frame.origin
            dragStartMouse = NSEvent.mouseLocation
            didDrag = false
        case .some(Control):
            bridge?.pointer(x: cx, y: cy) // engine dispatches the control's action synchronously
        default:
            break // Passthrough
        }
    }
    private var dragStartMouse: NSPoint?
    override func mouseDragged(with e: NSEvent) {
        guard let origin = dragOrigin, let start = dragStartMouse else { return }
        let now = NSEvent.mouseLocation
        window?.setFrameOrigin(NSPoint(x: origin.x + (now.x - start.x), y: origin.y + (now.y - start.y)))
    }
    override func mouseUp(with e: NSEvent) { dragOrigin = nil; dragStartMouse = nil }

    override func keyDown(with e: NSEvent) {
        if e.keyCode == 48 { // Tab → hot-swap (wired in App.swift via a closure)
            onTab?()
        } else {
            super.keyDown(with: e)
        }
    }
    var onTab: (() -> Void)?
}
```

- [ ] **Step 3: Implement App.swift (SwiftUI @main + AppDelegate)**

```swift
import SwiftUI
import AppKit

@main
struct ShowcaseApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    var body: some Scene {
        Settings { EmptyView() } // no default window; AppDelegate owns the skin window
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    var window: SkinWindow!
    var view: SkinView!
    var host: MusicHost!
    var bridge: CarapaceBridge!
    private var skinDirs: [String] = []
    private var skinIndex = 0

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)

        // Host (Task 6 supplies the real playlist; here a placeholder that Task 6 replaces).
        host = makePlaceholderHost()
        hostBox.host = host

        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())
        let sw = CANVAS_W * scale, sh = CANVAS_H * scale

        view = SkinView(frame: NSRect(x: 0, y: 0, width: CANVAS_W, height: CANVAS_H))
        skinDirs = resolveSkinDirs()
        guard let b = CarapaceBridge(skinDir: skinDirs[0], width: sw, height: sh,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            print("[showcase] bridge init failed"); NSApp.terminate(nil); return
        }
        bridge = b
        view.bridge = b
        view.onTab = { [weak self] in self?.cycleSkin() }

        window = SkinWindow(contentRect: NSRect(x: 200, y: 200, width: CANVAS_W, height: CANVAS_H),
                            styleMask: [.borderless], backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = view
        windowBox.window = window
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func cycleSkin() {
        skinIndex = (skinIndex + 1) % skinDirs.count
        _ = bridge.swap(skinDir: skinDirs[skinIndex]) // MusicHost state persists
    }
}
```

Add temporary helpers at the bottom of `App.swift` (Task 6 replaces `makePlaceholderHost`/`resolveSkinDirs` with real ones):
```swift
extension AppDelegate {
    func makePlaceholderHost() -> MusicHost {
        MusicHost(playlist: [Track(title: "Placeholder", artist: "—",
                                   url: URL(fileURLWithPath: "/dev/null"), duration: 1)],
                  player: RealAudioPlayer())
    }
    func resolveSkinDirs() -> [String] {
        // Task 6 replaces this with [starter, reference]. Temporary: one existing base-vocab skin.
        let repo = URL(fileURLWithPath: #filePath) // .../showcase/Sources/Showcase/App.swift
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent() // → repo root
        return [repo.appendingPathComponent("crates/carapace-demo/skins/reference").path]
    }
}
```

- [ ] **Step 4: Implement RealAudioPlayer (AVFoundation)**

Create `showcase/Sources/Showcase/RealAudioPlayer.swift`:
```swift
import Foundation
import AVFoundation

/// AVFoundation-backed AudioPlayer. Nil-safe: if a file can't load, playback is a no-op.
final class RealAudioPlayer: AudioPlayer {
    private var player: AVAudioPlayer?
    var isPlaying: Bool { player?.isPlaying ?? false }
    var volume: Float = 0.8 { didSet { player?.volume = volume } }
    var currentTime: TimeInterval {
        get { player?.currentTime ?? 0 }
        set { player?.currentTime = newValue }
    }
    var duration: TimeInterval { player?.duration ?? 0 }
    func load(_ url: URL, duration: TimeInterval) {
        player = try? AVAudioPlayer(contentsOf: url)
        player?.prepareToPlay()
        player?.volume = volume
    }
    func play() { player?.play() }
    func pause() { player?.pause() }
    func stop() { player?.stop(); player?.currentTime = 0 }
}
```

- [ ] **Step 5: Build + launch to verify the window renders a skin**

Run: `cargo build -p carapace-ffi && (cd showcase && swift build && swift run Showcase)`
Expected: a borderless window appears showing the `reference` skin rendered by carapace (transparent margins, draggable by a drag region, controls clickable). Ctrl-C to quit. If the window is black: check the printed create/last-error output and that the skin path resolves.

- [ ] **Step 6: Commit**

```bash
git rm showcase/Sources/Showcase/main.swift
git add showcase/Sources/Showcase/SkinWindow.swift showcase/Sources/Showcase/SkinView.swift showcase/Sources/Showcase/App.swift showcase/Sources/Showcase/RealAudioPlayer.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): borderless SkinWindow/SkinView + SwiftUI app entry, live render"
```

---

### Task 6: Starter skin + real playlist + hot-swap cycle

**Files:**
- Create: `showcase/skins/starter/skin.toml`
- Create: `showcase/skins/starter/skin.lua`
- Create: `showcase/Resources/audio/` (copy 2 CC0 tone WAVs from carapace-demo)
- Modify: `showcase/Sources/Showcase/App.swift` (`makePlaceholderHost` → real playlist; `resolveSkinDirs` → `[starter, reference]`)
- Modify: `showcase/Package.swift` (add `.copy("Resources")` to the Showcase target — convert to `.executableTarget` resources; note SwiftPM requires resources on a regular target, so add `resources: [.copy("Resources")]`)

**Interfaces:**
- Consumes: the host key surface (Tasks 2–3), `CANVAS_W/H` (Task 5).
- Produces: a working starter skin + a 2-entry swap cycle over a real playlist.

- [ ] **Step 1: Author the starter skin manifest**

`showcase/skins/starter/skin.toml` (exact schema, verified against `crates/carapace-demo/skins/minimal/skin.toml`):
```toml
schema = 1
id = "starter"
name = "Starter"
engine = "^0.1"
canvas = { width = 420, height = 660 }
entry = "skin.lua"
```

- [ ] **Step 2: Author the starter skin Lua**

`showcase/skins/starter/skin.lua` — bind every host key on the 420×660 canvas. Base vocab only:
```lua
-- Shaped body (rounded rect) over the transparent window; whole-body drag region.
fill{ path = rounded_rect{x=0, y=0, w=420, h=660, radius=18}, color = {r=18, g=22, b=32} }
region{ path = rounded_rect{x=0, y=0, w=420, h=660, radius=18}, role='drag',
        on_press = function() host.begin_drag() end }

-- window buttons
text{ text="_", x=384, y=8, size=16, color={r=200,g=200,b=210} }
region{ path=rect{x=380,y=8,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=402, y=8, size=16, color={r=230,g=140,b=140} }
region{ path=rect{x=398,y=8,w=16,h=18}, on_press=function() host.close() end }

-- now playing
text{ value="track_title", x=24, y=40, size=22, color={r=235,g=240,b=250} }
text{ value="artist", x=24, y=72, size=15, color={r=150,g=165,b=190} }
text{ value="time", x=24, y=96, size=13, color={r=120,g=135,b=160} }

-- seek scrub (position -> seek)
scrub{ value="position", on_seek="seek", x=24, y=128, w=372, h=10,
       direction='horizontal', color={r=92,g=255,b=154} }

-- visualizer bars (viz_0..viz_11)
value_fill{ path=rect{x=24,  y=160, w=28, h=60}, value="viz_0", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=56,  y=160, w=28, h=60}, value="viz_1", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=88,  y=160, w=28, h=60}, value="viz_2", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=120, y=160, w=28, h=60}, value="viz_3", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=152, y=160, w=28, h=60}, value="viz_4", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=184, y=160, w=28, h=60}, value="viz_5", direction='up', color={r=77,g=215,b=255} }

-- transport row
fill{ path=rect{x=24,  y=240, w=80, h=44}, color={r=48,g=58,b=78},
      on_press=function() host.prev() end }
text{ text="prev", x=44, y=254, size=14, color={r=210,g=220,b=235} }
fill{ path=rect{x=112, y=240, w=110, h=44}, color={r=88,g=255,b=173},
      on_press=function() host.toggle_play() end }
text{ text="play/pause", x=124, y=254, size=13, color={r=8,g=30,b=18} }
fill{ path=rect{x=230, y=240, w=80, h=44}, color={r=48,g=58,b=78},
      on_press=function() host.next() end }
text{ text="next", x=250, y=254, size=14, color={r=210,g=220,b=235} }

-- volume scrub (volume -> set_volume)
text{ text="vol", x=24, y=300, size=13, color={r=150,g=165,b=190} }
scrub{ value="volume", on_seek="set_volume", x=64, y=302, w=332, h=10,
       direction='horizontal', color={r=255,g=200,b=87} }

-- playlist
list{ collection="playlist", x=24, y=336, w=372, h=300, row_height=34,
      on_select="play_index", highlight={r=40,g=52,b=44}, selected="current_index",
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='artist', x=210, y=8, size=15, color={r=150,g=165,b=190} },
        { bind='duration', right=10, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
```
(Vocab arg names verified against `crates/carapace-demo/skins/reference/skin.lua` + `vocab.rs`: `role='drag'` classifies the whole-body hotspot as Drag for `hit_test` (controls declared AFTER win as Control since the topmost hotspot wins); `value_fill`/`scrub` take `direction`/`value`/`on_seek`; `list` takes `selected`/`highlight`/`on_select`/`template`; a `RowCell` uses `x` (from left) OR `right` (from right) — not both. Build/run is the final check.)

- [ ] **Step 3: Bundle audio + wire the real playlist**

Copy tones: `mkdir -p showcase/Resources/audio && cp crates/carapace-demo/skins/reference/assets/audio/track-01.wav crates/carapace-demo/skins/reference/assets/audio/track-02.wav showcase/Resources/audio/`

In `showcase/Package.swift`, add to the `Showcase` target: `resources: [.copy("Resources")]`. (Resources on an executable target are accessed via `Bundle.module`.)

Replace `makePlaceholderHost()` and `resolveSkinDirs()` in `App.swift`:
```swift
extension AppDelegate {
    func makePlaceholderHost() -> MusicHost {
        func tone(_ name: String, _ title: String, _ artist: String) -> Track? {
            guard let url = Bundle.module.url(forResource: "audio/\(name)", withExtension: "wav") else { return nil }
            return Track(title: title, artist: artist, url: url, duration: 4)
        }
        let tracks = [
            tone("track-01", "Neon Drive", "Atlas Minor"),
            tone("track-02", "Low Orbit", "Atlas Minor"),
        ].compactMap { $0 }
        return MusicHost(playlist: tracks, player: RealAudioPlayer())
    }
    func resolveSkinDirs() -> [String] {
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let starter = repo.appendingPathComponent("showcase/skins/starter").path
        let reference = repo.appendingPathComponent("crates/carapace-demo/skins/reference").path
        return [starter, reference]
    }
}
```
(Rename `makePlaceholderHost` → keep the name to avoid touching the call site, or rename both together.)

- [ ] **Step 4: Build + run + verify hot-swap keeps state**

Run: `cargo build -p carapace-ffi && (cd showcase && swift build && swift run Showcase)`
Expected: starter skin renders with title/artist/time/scrub/viz/transport/volume/playlist. Click play → audio + viz animate. Click a playlist row → that track plays (highlight moves). Drag the volume scrub → volume changes. Press Tab → swaps to `reference`; playback/position/volume/selection continue. Press Tab again → back to starter, still playing.

- [ ] **Step 5: Commit**

```bash
git add showcase/skins/starter showcase/Resources showcase/Package.swift showcase/Sources/Showcase/App.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): starter skin + real playlist + Tab hot-swap (starter <-> reference)"
```

---

### Task 7: README + agent-device verification run

**Files:**
- Create: `showcase/README.md`

**Interfaces:** none (docs + verification).

- [ ] **Step 1: Write the README**

`showcase/README.md`:
```markdown
# CarapaceShowcase (macOS)

Native SwiftUI app embedding the carapace engine via `carapace-ffi` (ABI 3.0), rendering skins
zero-copy through an IOSurface pool into a borderless, draggable window. Swift owns the music host
(playlist + AVFoundation), so skin hot-swaps preserve playback, position, volume, and selection.

## Build & run

    cargo build -p carapace-ffi     # from repo root — produces target/debug/libcarapace_ffi.dylib
    cd showcase && swift run Showcase

Press **Tab** to hot-swap skins (starter ↔ reference). Drag the body to move the window;
the min/close glyphs and all transport/scrub/playlist controls are the skin's own.

## Tests

    cd showcase && swift test        # MusicHost + vtable-callback unit tests

## Notes

- Sub-project B of the "one host, three skins" showcase. The three concept skins are Sub-project C.
- `viz_*` is a time-driven animation, not a real FFT.
- Not built in CI (no Swift toolchain there); verified by `swift test` + a manual/agent run.
```

- [ ] **Step 2: Agent-device verification run**

Run the app and drive it with `agent-device` (confirm macOS targeting first: `agent-device devices`; if it can't target this macOS app, fall back to `swift run Showcase` + `screencapture -o /tmp/showcase.png`). Capture: (a) starter skin playing, (b) after a playlist-row click, (c) after Tab swap to reference still playing. Confirm playback/position/volume/selection persist across the swap. Record the outcome (and screenshot paths) in the report — this is the acceptance gate.

- [ ] **Step 3: Commit**

```bash
git add showcase/README.md
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "docs(showcase): build/run/test README for the macOS showcase app"
```

---

### Final verification (after all tasks)

- [ ] `cargo build -p carapace-ffi && (cd showcase && swift build && swift test)` — package builds, all unit tests pass.
- [ ] A run (agent-device or manual) confirms: borderless window renders the starter skin; audio plays; transport/scrub/volume/playlist all drive the host; Tab hot-swaps starter↔reference with state intact. Screenshots captured.
- [ ] No Swift warnings in `swift build` output (pristine).

## Self-review notes (reconciled)

- **TDD applies to Tasks 2–3** (MusicHost + callbacks are unit-tested); Tasks 4–6 are FFI/window/GUI glue verified by build + run (no meaningful unit test — stated honestly).
- **ABI enum bare names** (`Ok`, `Press`, `Passthrough`, `Drag`, `Control`): the header defines them as C enum constants; Task 4/5 note to qualify them if Swift's C importer doesn't expose bare names, using the documented values (all the "0" variants are 0).
- **Skin vocab arg names**: the starter skin (Task 6) must be validated against `carapace-demo/skins/reference` + `vocab.rs`; build/run is the check, and Task 6 says so.
- **Type consistency**: `MusicHost` methods, the vtable callback names (`hostGetNum` etc.), `CarapaceBridge` API, and `frameSink`/`hostBox`/`windowBox` globals are used identically across tasks.

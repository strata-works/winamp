import AppKit
import IOSurface
import IOKit.ps
import CCarapace

// Fix A: unbuffer stdout so runtime prints flush immediately to a non-TTY pipe.
setvbuf(stdout, nil, _IONBF, 0)

let W: UInt32 = 342, H: UInt32 = 394

// ---------------------------------------------------------------------------
// Swift-owned host state
// ---------------------------------------------------------------------------

final class HostState {
    var paused = false
    let start = CACurrentMediaTime()

    /// Playback position 0..1 sweeping over ~60 s; frozen when paused.
    func position() -> Double {
        if paused { return positionAtPause }
        let elapsed = CACurrentMediaTime() - startOfPlay
        return (elapsed / 60.0).truncatingRemainder(dividingBy: 1.0)
    }

    /// Fake visualiser bar i: animated spectrum, near-0/flat when paused.
    func viz(_ i: Int) -> Double {
        if paused {
            return abs(sin(positionAtPause * 2.5 + Double(i) * 0.6)) *
                   (0.4 + 0.6 * abs(sin(positionAtPause * 0.7 + Double(i)))) * 0.1
        }
        let now = CACurrentMediaTime() - start
        let v = abs(sin(now * 2.5 + Double(i) * 0.6)) *
                (0.4 + 0.6 * abs(sin(now * 0.7 + Double(i))))
        return min(max(v, 0.0), 1.0)
    }

    /// "mm:ss / 3:45" derived from position over a fake 225s total.
    func timeString() -> String {
        let total = 225.0        // 3:45
        let elapsed = position() * total
        let m  = Int(elapsed) / 60
        let s  = Int(elapsed) % 60
        let ss = s < 10 ? "0\(s)" : "\(s)"
        return "\(m):\(ss) / 3:45"
    }

    // Internal: track pause-time so position/viz freeze correctly.
    private var positionAtPause: Double = 0.0
    private var startOfPlay: Double     // adjusted origin so position is continuous

    init() {
        startOfPlay = CACurrentMediaTime()
    }

    func togglePlay() {
        if paused {
            // Resume: shift startOfPlay so position() picks up where we paused.
            startOfPlay = CACurrentMediaTime() - positionAtPause * 60.0
            paused = false
        } else {
            positionAtPause = position()
            paused = true
        }
    }
}

/// Read the current battery charge fraction (0..1) via IOKit Power Sources.
func batteryFraction() -> Double? {
    let blob = IOPSCopyPowerSourcesInfo().takeRetainedValue()
    let list = IOPSCopyPowerSourcesList(blob).takeRetainedValue() as [CFTypeRef]
    for src in list {
        if let info = IOPSGetPowerSourceDescription(blob, src)
                        .takeUnretainedValue() as? [String: Any],
           let cur = info[kIOPSCurrentCapacityKey] as? Int,
           let max = info[kIOPSMaxCapacityKey] as? Int,
           max > 0
        {
            return Double(cur) / Double(max)
        }
    }
    return nil
}

let state = HostState()
print("[host] battery fraction (native read):", batteryFraction().map { String($0) } ?? "n/a")

// ---------------------------------------------------------------------------
// Host vtable callbacks — top-level C-compatible functions
// ---------------------------------------------------------------------------

func getNum(
    _ ctx: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<CChar>?,
    _ out: UnsafeMutablePointer<Double>?
) -> Bool {
    guard let key = key, let out = out else { return false }
    let k = String(cString: key)
    switch k {
    case "position":
        out.pointee = state.position()
        return true
    case "current_index":
        out.pointee = 0.0
        return true
    default:
        // viz_0 .. viz_11
        if k.hasPrefix("viz_"), let idx = Int(k.dropFirst(4)), idx >= 0 && idx <= 11 {
            out.pointee = state.viz(idx)
            return true
        }
        return false
    }
}

func getStr(
    _ ctx: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<CChar>?,
    _ buf: UnsafeMutablePointer<CChar>?,
    _ cap: Int
) -> Bool {
    guard let key = key, let buf = buf, cap > 0 else { return false }
    let k = String(cString: key)
    let value: String
    switch k {
    case "track_title": value = "Headspace · Ambient Demo"
    case "time":        value = state.timeString()
    default:            return false
    }
    // Copy at most cap-1 UTF-8 bytes + NUL terminator.
    let bytes = Array(value.utf8)
    let toCopy = min(bytes.count, cap - 1)
    for i in 0..<toCopy {
        buf[i] = Int8(bitPattern: bytes[i])
    }
    buf[toCopy] = 0
    return true
}

func invokeAction(
    _ ctx: UnsafeMutableRawPointer?,
    _ action: UnsafePointer<CChar>?
) {
    guard let action = action else { return }
    let name = String(cString: action)
    print("[host] invoke: \(name)")
    if name == "toggle_play" {
        state.togglePlay()
        print("[host] paused =", state.paused)
    }
}

// ---------------------------------------------------------------------------
// AppKit view backed by carapace
// ---------------------------------------------------------------------------

final class SkinView: NSView {
    var engine: OpaquePointer?
    var surface: IOSurface!
    var last = CACurrentMediaTime()

    // Accept the first click even when the window is not yet key,
    // and make the view the first responder so keyboard/mouse events land here.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

        surface = IOSurface(properties: [
            .width: Int(W),
            .height: Int(H),
            .bytesPerElement: 4,
            .pixelFormat: 0x42475241 as UInt32   // 'BGRA'
        ])!

        // Build vtable
        let vt = CarapaceHostVTable(
            ctx: nil,
            get_num: getNum,
            get_str: getStr,
            invoke: invokeAction
        )

        // Derive skin path relative to this source file.
        // #filePath  = .../crates/embed-spike/macos-sample/Sources/EmbedSpike/main.swift
        // hop 1 → strip main.swift         → .../Sources/EmbedSpike/
        // hop 2 → strip EmbedSpike/        → .../Sources/
        // hop 3 → strip Sources/           → .../macos-sample/
        // hop 4 → strip macos-sample/      → .../embed-spike/
        // hop 5 → strip embed-spike/       → .../crates/
        // + carapace-demo/skins/reference  → real Headspace skin dir ✓
        let thisFile = URL(fileURLWithPath: #filePath)
        let skinURL = thisFile
            .deletingLastPathComponent()   // → .../Sources/EmbedSpike/
            .deletingLastPathComponent()   // → .../Sources/
            .deletingLastPathComponent()   // → .../macos-sample/
            .deletingLastPathComponent()   // → .../embed-spike/
            .deletingLastPathComponent()   // → .../crates/
            .appendingPathComponent("carapace-demo/skins/reference")
        let skinDir = skinURL.path
        print("[carapace] skin dir:", skinDir)
        let skinExists = FileManager.default.fileExists(atPath: skinDir)
        print("[carapace] skin dir exists:", skinExists)
        if !skinExists {
            print("[carapace] WARNING: skin directory not found — engine will fail to load")
        }

        engine = skinDir.withCString { carapace_create($0, vt, surface, W, H) }
        if engine == nil {
            print("[carapace] ERROR: carapace_create returned nil — check skin path and dylib")
        } else {
            let tier = carapace_active_tier(engine)
            print("[carapace] active tier:", tier, tier == 1 ? "(Readback)" : tier == 2 ? "(Shared/Metal)" : "(unknown)")
        }
    }

    required init?(coder: NSCoder) { fatalError() }

    func tick() {
        let now = CACurrentMediaTime()
        let dt = now - last
        last = now
        carapace_tick(engine, dt)
        layer?.contents = surface          // zero-copy hand-off to CA
        layer?.contentsGravity = .resize
    }

    override func mouseDown(with e: NSEvent) {
        let p = convert(e.locationInWindow, from: nil)
        // AppKit y is bottom-up; canvas y is top-down.
        let cx = Double(p.x) * Double(W) / Double(bounds.width)
        let cy = (Double(bounds.height) - Double(p.y)) * Double(H) / Double(bounds.height)
        print("[input] pointer press at canvas (\(Int(cx)), \(Int(cy)))")
        carapace_pointer(engine, cx, cy, 0)
    }

    deinit {
        if let e = engine { carapace_destroy(e) }
    }
}

// ---------------------------------------------------------------------------
// App bootstrap
// ---------------------------------------------------------------------------

let app = NSApplication.shared
app.setActivationPolicy(.regular)

let win = NSWindow(
    contentRect: NSRect(x: 200, y: 200, width: Int(W), height: Int(H)),
    styleMask: [.titled, .closable, .miniaturizable],
    backing: .buffered,
    defer: false
)
win.title = "embed-spike"

let view = SkinView(frame: win.contentLayoutRect)
view.autoresizingMask = [.width, .height]
win.contentView = view
win.makeKeyAndOrderFront(nil)
win.makeFirstResponder(view)    // route events directly to SkinView

// LAG FIX: Replace CVDisplayLink (which dispatched async onto main, building a queue backlog)
// with a main-run-loop Timer. Coalesces naturally; fires during mouse tracking (.common mode).
let timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { _ in
    view.tick()
}
RunLoop.main.add(timer, forMode: .common)

app.activate(ignoringOtherApps: true)
app.run()

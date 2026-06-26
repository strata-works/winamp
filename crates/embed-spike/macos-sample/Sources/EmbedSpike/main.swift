import AppKit
import CoreVideo
import IOSurface
import IOKit.ps
import CCarapace

let W: UInt32 = 240, H: UInt32 = 80

// ---------------------------------------------------------------------------
// Swift-owned host state
// ---------------------------------------------------------------------------

final class HostState {
    var paused = false
    var lastLevel: Double = 0.5

    func level() -> Double {
        if paused { return lastLevel }
        if let frac = batteryFraction() {
            lastLevel = frac
            return frac
        }
        // Wall-clock sweep: 0→1 over 10 seconds
        let sweep = Date().timeIntervalSince1970.truncatingRemainder(dividingBy: 10.0) / 10.0
        lastLevel = sweep
        return sweep
    }
}

/// Read the current battery charge fraction (0..1) via IOKit Power Sources.
/// Returns nil when no battery is present (e.g. desktop Macs or while on AC
/// with no battery charge info available).
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

// ---------------------------------------------------------------------------
// Host vtable callbacks — top-level C-compatible functions
// ---------------------------------------------------------------------------

func getNum(
    _ ctx: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<CChar>?,
    _ out: UnsafeMutablePointer<Double>?
) -> Bool {
    guard let key = key, let out = out else { return false }
    if String(cString: key) == "level" {
        out.pointee = state.level()
        return true
    }
    return false
}

func getStr(
    _ ctx: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<CChar>?,
    _ buf: UnsafeMutablePointer<CChar>?,
    _ cap: Int
) -> Bool {
    return false
}

func invokeAction(
    _ ctx: UnsafeMutableRawPointer?,
    _ action: UnsafePointer<CChar>?
) {
    guard let action = action else { return }
    let name = String(cString: action)
    print("[host] invoke: \(name)")
    if name == "toggle" {
        state.paused.toggle()
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
        // #filePath  = .../embed-spike/macos-sample/Sources/EmbedSpike/main.swift
        // hop 1 → strip main.swift   → .../Sources/EmbedSpike/
        // hop 2 → strip EmbedSpike/  → .../Sources/
        // hop 3 → strip Sources/     → .../macos-sample/
        // hop 4 → strip macos-sample/→ .../embed-spike/
        // + "skin"                   → .../embed-spike/skin  ✓  (verified empirically)
        let thisFile = URL(fileURLWithPath: #filePath)
        let skinURL = thisFile
            .deletingLastPathComponent()   // → .../Sources/EmbedSpike/
            .deletingLastPathComponent()   // → .../Sources/
            .deletingLastPathComponent()   // → .../macos-sample/
            .deletingLastPathComponent()   // → .../embed-spike/
            .appendingPathComponent("skin")
        let skinDir = skinURL.path
        print("[carapace] skin dir:", skinDir)

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
    contentRect: NSRect(x: 200, y: 200, width: 480, height: 160),
    styleMask: [.titled, .closable, .miniaturizable],
    backing: .buffered,
    defer: false
)
win.title = "embed-spike"

let view = SkinView(frame: win.contentLayoutRect)
view.autoresizingMask = [.width, .height]
win.contentView = view
win.makeKeyAndOrderFront(nil)

// CVDisplayLink drives ticks at the display refresh rate
var displayLink: CVDisplayLink?
CVDisplayLinkCreateWithActiveCGDisplays(&displayLink)
if let dl = displayLink {
    CVDisplayLinkSetOutputHandler(dl) { _, _, _, _, _ in
        DispatchQueue.main.async { view.tick() }
        return kCVReturnSuccess
    }
    CVDisplayLinkStart(dl)
}

app.activate(ignoringOtherApps: true)
app.run()

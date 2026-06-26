import AppKit
import CoreText
import IOSurface
import IOKit.ps
import CCarapace

// Fix A: unbuffer stdout so runtime prints flush immediately to a non-TTY pipe.
setvbuf(stdout, nil, _IONBF, 0)

let W: UInt32 = 342, H: UInt32 = 394
// The host-content IOSurface for the skin's view{ id = "host" } cutout (70,60..272,206 → 202×146).
// Rendered at 2× for crispness; the engine samples it into the cutout rect.
let CW: Int = 404, CH: Int = 292

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

// ---------------------------------------------------------------------------
// Weak-reference box so C closures can call back into the window.
// ---------------------------------------------------------------------------

final class WeakWindowBox {
    weak var window: SkinWindow?
    init(_ w: SkinWindow) { window = w }
}

// Will be set after the window is created; the C callback captures it.
var windowBox: WeakWindowBox? = nil

func invokeAction(
    _ ctx: UnsafeMutableRawPointer?,
    _ action: UnsafePointer<CChar>?
) {
    guard let action = action else { return }
    let name = String(cString: action)
    print("[host] invoke: \(name)")
    switch name {
    case "toggle_play":
        state.togglePlay()
        print("[host] paused =", state.paused)
    case "minimize":
        DispatchQueue.main.async {
            windowBox?.window?.miniaturize(nil)
        }
    case "close":
        DispatchQueue.main.async {
            NSApp.terminate(nil)
        }
    default:
        // begin_drag and anything else: just log (window drag handled manually).
        break
    }
}

// ---------------------------------------------------------------------------
// Borderless window subclass — .borderless windows cannot become key/main
// by default; override to restore click-to-focus and keyboard focus.
// ---------------------------------------------------------------------------

final class SkinWindow: NSWindow {
    override var canBecomeKey:  Bool { true }
    override var canBecomeMain: Bool { true }
}

// ---------------------------------------------------------------------------
// AppKit view backed by carapace
// ---------------------------------------------------------------------------

final class SkinView: NSView {
    var engine: OpaquePointer?
    var surface: IOSurface!
    var content: IOSurface!            // Swift draws its OWN live content here each tick.
    let contentStart = CACurrentMediaTime()
    var last = CACurrentMediaTime()

    // Drag state
    private var dragStartMouse:  NSPoint?
    private var dragStartOrigin: NSPoint?
    private var didDrag = false

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

        // Second IOSurface holding the host app's OWN live content for the view{} cutout.
        content = IOSurface(properties: [
            .width: CW,
            .height: CH,
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

        // Derive skin path relative to this source file. The view{}-cutout skin lives under
        // embed-spike now (NOT carapace-demo): 4 hops to .../crates/embed-spike/, then skin-headspace.
        // #filePath  = .../crates/embed-spike/macos-sample/Sources/EmbedSpike/main.swift
        // hop 1 → strip main.swift         → .../Sources/EmbedSpike/
        // hop 2 → strip EmbedSpike/        → .../Sources/
        // hop 3 → strip Sources/           → .../macos-sample/
        // hop 4 → strip macos-sample/      → .../embed-spike/
        // + skin-headspace                 → view{} cutout Headspace skin ✓
        let thisFile = URL(fileURLWithPath: #filePath)
        let skinURL = thisFile
            .deletingLastPathComponent()   // → .../Sources/EmbedSpike/
            .deletingLastPathComponent()   // → .../Sources/
            .deletingLastPathComponent()   // → .../macos-sample/
            .deletingLastPathComponent()   // → .../embed-spike/
            .appendingPathComponent("skin-headspace")
        let skinDir = skinURL.path
        print("[carapace] skin dir:", skinDir)
        let skinExists = FileManager.default.fileExists(atPath: skinDir)
        print("[carapace] skin dir exists:", skinExists)
        if !skinExists {
            print("[carapace] WARNING: skin directory not found — engine will fail to load")
        }

        engine = skinDir.withCString { carapace_create($0, vt, surface, content, W, H) }
        if engine == nil {
            print("[carapace] ERROR: carapace_create returned nil — check skin path and dylib")
        } else {
            let tier = carapace_active_tier(engine)
            print("[carapace] active tier:", tier, tier == 1 ? "(Readback)" : tier == 2 ? "(Shared/Metal)" : "(unknown)")
        }
    }

    required init?(coder: NSCoder) { fatalError() }

    /// Draw the host app's OWN live content into the content IOSurface via Core Graphics.
    /// A dark translucent panel, a bright accent header bar, a LIVE digital clock (HH:MM:SS),
    /// a "Swift host content" label, and a moving bar that follows the playback sweep. The engine
    /// samples this BGRA surface straight into the skin's view{ id = "host" } cutout.
    func drawHostContent() {
        content.lock(options: [], seed: nil)
        defer { content.unlock(options: [], seed: nil) }

        let base = content.baseAddress
        let bpr = content.bytesPerRow
        // BGRA premultiplied (byteOrder32Little + premultipliedFirst) — matches what the engine
        // imports as Bgra8Unorm, so colours land correct.
        let bitmapInfo = CGImageAlphaInfo.premultipliedFirst.rawValue
            | CGBitmapInfo.byteOrder32Little.rawValue
        guard let ctx = CGContext(
            data: base,
            width: CW,
            height: CH,
            bitsPerComponent: 8,
            bytesPerRow: bpr,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: bitmapInfo
        ) else { return }

        // CGContext origin is bottom-left; the engine samples top-down. Flip so our drawing is
        // upright in the cutout.
        ctx.translateBy(x: 0, y: CGFloat(CH))
        ctx.scaleBy(x: 1, y: -1)

        let wF = CGFloat(CW), hF = CGFloat(CH)

        // 1. Dark translucent background panel.
        ctx.setFillColor(red: 0.04, green: 0.06, blue: 0.10, alpha: 0.92)
        ctx.fill(CGRect(x: 0, y: 0, width: wF, height: hF))

        // 2. Bright accent header bar (Headspace orange/amber).
        ctx.setFillColor(red: 1.0, green: 0.62, blue: 0.16, alpha: 1.0)
        ctx.fill(CGRect(x: 0, y: 0, width: wF, height: hF * 0.16))

        // 3. Moving element: a bar whose width follows the same 0..1 sweep as `position`.
        let pos = CGFloat(state.position())
        ctx.setFillColor(red: 0.20, green: 0.85, blue: 0.55, alpha: 1.0)
        ctx.fill(CGRect(x: 0, y: hF - hF * 0.10, width: wF * pos, height: hF * 0.10))

        // Helper: draw a string with CoreText at (x, y) measured from the TOP (we flip y inside).
        func drawText(_ s: String, x: CGFloat, topY: CGFloat, size: CGFloat,
                      color: CGColor, font: String = "Menlo") {
            let ctFont = CTFontCreateWithName(font as CFString, size, nil)
            let attrs: [NSAttributedString.Key: Any] = [
                .font: ctFont,
                .foregroundColor: color,
            ]
            let line = CTLineCreateWithAttributedString(
                NSAttributedString(string: s, attributes: attrs))
            // Convert top-origin y to CG bottom-origin baseline.
            ctx.textPosition = CGPoint(x: x, y: hF - topY - size)
            CTLineDraw(line, ctx)
        }

        // 4. Header label inside the accent bar.
        drawText("Swift host content", x: 14, topY: 10, size: 20,
                 color: CGColor(red: 0.05, green: 0.04, blue: 0.02, alpha: 1.0),
                 font: "HelveticaNeue-Bold")

        // 5. LIVE digital clock (HH:MM:SS), updated every tick.
        let df = DateFormatter()
        df.dateFormat = "HH:mm:ss"
        let clock = df.string(from: Date())
        drawText(clock, x: 14, topY: hF * 0.16 + 16, size: 56,
                 color: CGColor(red: 0.90, green: 0.97, blue: 1.0, alpha: 1.0),
                 font: "Menlo-Bold")

        // 6. A secondary line proving it's live: elapsed seconds since launch.
        let elapsed = Int(CACurrentMediaTime() - contentStart)
        drawText("live · up \(elapsed)s · pos \(Int(pos * 100))%",
                 x: 14, topY: hF * 0.16 + 90, size: 22,
                 color: CGColor(red: 0.55, green: 0.80, blue: 0.95, alpha: 1.0),
                 font: "Menlo")
    }

    func tick() {
        let now = CACurrentMediaTime()
        let dt = now - last
        last = now
        // Draw the host's own live content BEFORE ticking so the engine composites this frame.
        drawHostContent()
        carapace_tick(engine, dt)
        // Keep layer transparent so desktop shows through clear pixels.
        layer?.isOpaque = false
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.contents = surface          // zero-copy hand-off to CA
        layer?.contentsGravity = .resize
    }

    // MARK: - Mouse events for drag + tap dispatch

    override func mouseDown(with e: NSEvent) {
        // Record screen-space anchor; do NOT forward to the engine yet —
        // we only forward on mouseUp if the gesture was a tap (no drag).
        dragStartMouse  = NSEvent.mouseLocation
        dragStartOrigin = window?.frame.origin
        didDrag = false
    }

    override func mouseDragged(with e: NSEvent) {
        guard let start  = dragStartMouse,
              let origin = dragStartOrigin else { return }
        let now = NSEvent.mouseLocation
        let dx = now.x - start.x
        let dy = now.y - start.y
        if abs(dx) > 3 || abs(dy) > 3 {
            didDrag = true
        }
        window?.setFrameOrigin(NSPoint(x: origin.x + dx, y: origin.y + dy))
    }

    override func mouseUp(with e: NSEvent) {
        if !didDrag {
            // It was a tap — forward to the engine as a press+release.
            let p  = convert(e.locationInWindow, from: nil)
            let cx = Double(p.x) * Double(W) / Double(bounds.width)
            let cy = (Double(bounds.height) - Double(p.y)) * Double(H) / Double(bounds.height)
            print("[input] pointer tap at canvas (\(Int(cx)), \(Int(cy)))")
            carapace_pointer(engine, cx, cy, 0)
        }
        // If didDrag: window already moved; nothing more to do.
        dragStartMouse  = nil
        dragStartOrigin = nil
        didDrag = false
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

// 1. Borderless + transparent + shaped window
let win = SkinWindow(
    contentRect: NSRect(x: 200, y: 200, width: Int(W), height: Int(H)),
    styleMask: [.borderless],
    backing: .buffered,
    defer: false
)
win.isOpaque = false
win.backgroundColor = .clear
win.hasShadow = true   // drop-shadow follows the shaped silhouette

// Wire up the weak box so invokeAction can reach the window.
windowBox = WeakWindowBox(win)

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

import AppKit
import AVFoundation
import CoreText
import IOSurface
import IOKit.ps
import CCarapace

// Fix A: unbuffer stdout so runtime prints flush immediately to a non-TTY pipe.
setvbuf(stdout, nil, _IONBF, 0)

// Design canvas for skin-paper-surround (skin.toml canvas = 480×300).
let W: UInt32 = 480, H: UInt32 = 300
let CW: Int = 480, CH: Int = 300
// The host-content IOSurface for the skin's view{ id = "content" } cutout
// (24,24..456,276 → 432×252, 1:1 with design coords — no 2× multiplier this time).
// The engine samples it into the cutout rect; the "paper" cutout behind it is
// rendered directly by the engine's transpiled mesh-gradient shader.
let CONTENT_W: Int = 432, CONTENT_H: Int = 252

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

// Same pattern as WeakWindowBox: invokeAction is a top-level C-compatible function with
// no captured context, so transport actions reach the real AVAudioPlayer (owned by
// SkinView) through a weak box set once the view exists.
final class WeakViewBox {
    weak var view: SkinView?
    init(_ v: SkinView) { view = v }
}

var viewBox: WeakViewBox? = nil

func invokeAction(
    _ ctx: UnsafeMutableRawPointer?,
    _ action: UnsafePointer<CChar>?
) {
    guard let action = action else { return }
    let name = String(cString: action)
    print("[host] invoke: \(name)")
    switch name {
    case "toggle_play":
        if let v = viewBox?.view {
            if v.player?.isPlaying == true { v.player?.pause() } else { v.player?.play() }
        }
    case "prev":
        if let p = viewBox?.view?.player {
            p.currentTime = max(0, p.currentTime - 15)
        }
    case "next":
        if let p = viewBox?.view?.player {
            p.currentTime = min((p.duration) - 0.1, p.currentTime + 15)
        }
    case "scrub":
        // The actual seek is computed in mouseUp against the click x that produced this
        // very hit-test (carapace_pointer → engine hit-test → this callback, all synchronous
        // within the same call), so just flag it here.
        viewBox?.view?.scrubPending = true
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

    // Real playback: an actual AVAudioPlayer, routed to from invokeAction via viewBox.
    var player: AVAudioPlayer?
    var scrubPending = false

    // Zoom state for scroll/pinch resize (aspect-locked, 0.5…3.0×).
    private var zoom: CGFloat = 1.0
    private let baseW: CGFloat = 480
    private let baseH: CGFloat = 300

    // Drag state
    private var dragStartMouse:  NSPoint?
    private var dragStartOrigin: NSPoint?
    private var dragStartZoom:   CGFloat = 1.0
    private var didDrag = false

    // Accept the first click even when the window is not yet key,
    // and make the view the first responder so keyboard/mouse events land here.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true

        // RETINA SHARPNESS: render into a backing-scale (e.g. 2×) IOSurface while keeping the
        // window/view in POINTS (W×H). The CALayer then maps sw×sh surface pixels 1:1 onto the
        // backing pixels of the W×H-point view → no CoreAnimation upscale → crisp.
        // Layout/hit-testing stay at the W×H DESIGN canvas (Rust lays out at scene.canvas and
        // scales up to fill the surface).
        let scale = NSScreen.main?.backingScaleFactor ?? 2.0
        let sw = Int((CGFloat(W) * scale).rounded())
        let sh = Int((CGFloat(H) * scale).rounded())
        print("[carapace] backing scale:", scale, "surface px:", sw, "×", sh)

        surface = IOSurface(properties: [
            .width: sw,
            .height: sh,
            .bytesPerElement: 4,
            .pixelFormat: 0x42475241 as UInt32   // 'BGRA'
        ])!

        // Second IOSurface holding the host app's OWN live content for the view{} cutout.
        content = IOSurface(properties: [
            .width: CONTENT_W,
            .height: CONTENT_H,
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
        // embed-spike now (NOT carapace-demo): 4 hops to .../crates/embed-spike/, then
        // skin-paper-surround (Phase 2: gradient surround + real player content cutout).
        // #filePath  = .../crates/embed-spike/macos-sample/Sources/EmbedSpike/main.swift
        // hop 1 → strip main.swift         → .../Sources/EmbedSpike/
        // hop 2 → strip EmbedSpike/        → .../Sources/
        // hop 3 → strip Sources/           → .../macos-sample/
        // hop 4 → strip macos-sample/      → .../embed-spike/
        // + skin-paper-surround            → view{} cutout paper-surround skin ✓
        let thisFile = URL(fileURLWithPath: #filePath)
        let skinURL = thisFile
            .deletingLastPathComponent()   // → .../Sources/EmbedSpike/
            .deletingLastPathComponent()   // → .../Sources/
            .deletingLastPathComponent()   // → .../macos-sample/
            .deletingLastPathComponent()   // → .../embed-spike/
            .appendingPathComponent("skin-paper-surround")
        let skinDir = skinURL.path
        print("[carapace] skin dir:", skinDir)
        let skinExists = FileManager.default.fileExists(atPath: skinDir)
        print("[carapace] skin dir exists:", skinExists)
        if !skinExists {
            print("[carapace] WARNING: skin directory not found — engine will fail to load")
        }

        // Pass the SURFACE pixel size (sw×sh) — the engine sizes its offscreen/IOSurface textures
        // to this and renders the W×H design canvas scaled up to fill it. Hit-testing still uses
        // the design canvas (Rust reads scene.canvas internally).
        engine = skinDir.withCString { carapace_create($0, vt, surface, content, UInt32(sw), UInt32(sh)) }
        if engine == nil {
            print("[carapace] ERROR: carapace_create returned nil — check skin path and dylib")
        } else {
            let tier = carapace_active_tier(engine)
            print("[carapace] active tier:", tier, tier == 1 ? "(Readback)" : tier == 2 ? "(Shared/Metal)" : "(unknown)")
        }

        // Pinch-to-zoom gesture recognizer.
        let pinch = NSMagnificationGestureRecognizer(target: self, action: #selector(handlePinch(_:)))
        addGestureRecognizer(pinch)

        setupAudio()
    }

    /// Load the bundled sample clip into a real AVAudioPlayer (looped; the demo has no
    /// playlist, just one track that repeats).
    private func setupAudio() {
        guard let url = Bundle.module.url(forResource: "sample", withExtension: "m4a") else {
            print("[carapace] sample.m4a not found in bundle"); return
        }
        player = try? AVAudioPlayer(contentsOf: url)
        player?.numberOfLoops = -1
        player?.prepareToPlay()
    }

    @objc private func zoomIn()  { applyZoomDelta(1.1) }
    @objc private func zoomOut() { applyZoomDelta(1.0 / 1.1) }

    required init?(coder: NSCoder) { fatalError() }

    /// Draw the host app's OWN live content into the content IOSurface via a flipped
    /// NSGraphicsContext so text and layout use top-left origin (y grows DOWN) — no manual
    /// CTM flip needed, and NSString/NSAttributedString render upright and correctly-handed.
    func drawHostContent() {
        content.lock(options: [], seed: nil)
        defer { content.unlock(options: [], seed: nil) }

        let base   = content.baseAddress
        let stride = content.bytesPerRow
        let cw     = CONTENT_W
        let ch     = CONTENT_H

        let cs  = CGColorSpaceCreateDeviceRGB()
        let bmp = CGImageAlphaInfo.premultipliedFirst.rawValue
                | CGBitmapInfo.byteOrder32Little.rawValue
        guard let cg = CGContext(
            data:             base,
            width:            cw,
            height:           ch,
            bitsPerComponent: 8,
            bytesPerRow:      stride,
            space:            cs,
            bitmapInfo:       bmp
        ) else { return }

        // Flip the raw bitmap context (which is y-up / bottom-left origin) to a top-left
        // origin so BOTH the layout (y grows down) AND AppKit text render upright and
        // un-mirrored. Verified in isolation via a PNG harness: translate+scale(1,-1) THEN
        // NSGraphicsContext(flipped:true). (flipped:true alone, with no CTM flip, 180°-inverts.)
        cg.translateBy(x: 0, y: CGFloat(ch))
        cg.scaleBy(x: 1, y: -1)
        let ns = NSGraphicsContext(cgContext: cg, flipped: true)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = ns

        let wF = CGFloat(cw), hF = CGFloat(ch)
        let dur = player?.duration ?? 1
        let pos = CGFloat((player?.currentTime ?? 0) / max(dur, 0.001))
        let playing = player?.isPlaying ?? false

        // Fill the content rect with a STATIC gradient in paper's palette, then draw the
        // rounded card on top — so the card's rounded corners read as gradient, not black.
        // (The engine's view-compositor overwrites host content with blend:None, so true
        // see-through corners onto the LIVE paper layer need an engine alpha-blend change —
        // deferred as a post-spike follow-up; see findings.)
        let cornerGrad = NSGradient(colors: [
            NSColor(red: 0.94, green: 0.28, blue: 0.44, alpha: 1),
            NSColor(red: 0.99, green: 0.76, blue: 0.18, alpha: 1),
            NSColor(red: 0.11, green: 0.78, blue: 0.55, alpha: 1),
            NSColor(red: 0.15, green: 0.39, blue: 0.92, alpha: 1),
        ])
        cornerGrad?.draw(in: NSRect(x: 0, y: 0, width: wF, height: hF), angle: -35)
        // Card background (near-white, real macOS look) — rounded to match the window corners.
        NSColor(white: 0.98, alpha: 1.0).setFill()
        NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: wF, height: hF),
                     xRadius: 12, yRadius: 12).fill()

        // Window controls: macOS traffic lights at the card's top-left (the skin's invisible
        // hotspots align to these). Drawn by the host because the engine composites view{} over
        // the vello layer, so carapace vector can't paint over the full-bleed paper shader.
        let controls: [(CGFloat, NSColor)] = [
            (20, NSColor(red: 1.00, green: 0.37, blue: 0.34, alpha: 1)),  // close (red)
            (40, NSColor(red: 1.00, green: 0.74, blue: 0.18, alpha: 1)),  // minimize (yellow)
            (60, NSColor(red: 0.16, green: 0.79, blue: 0.25, alpha: 1)),  // zoom (green, cosmetic)
        ]
        for (dx, color) in controls {
            color.setFill()
            NSBezierPath(ovalIn: NSRect(x: dx - 6, y: 12, width: 12, height: 12)).fill()
        }

        // Album art (rounded square, left).
        let art = NSRect(x: 22, y: hF*0.5 - 62, width: 124, height: 124)
        let grad = NSGradient(colors: [NSColor(red:0.93,green:0.35,blue:0.57,alpha:1),
                                       NSColor(red:0.23,green:0.43,blue:0.94,alpha:1)])
        NSBezierPath(roundedRect: art, xRadius: 16, yRadius: 16).addClip()
        grad?.draw(in: art, angle: -60)
        NSGraphicsContext.current?.cgContext.resetClip()

        // Title + artist.
        let titleAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 26, weight: .bold),
            .foregroundColor: NSColor(white: 0.08, alpha: 1)]
        ("Cascade" as NSString).draw(at: NSPoint(x: 168, y: hF*0.5 - 52), withAttributes: titleAttrs)
        let subAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 14, weight: .regular),
            .foregroundColor: NSColor(white: 0.42, alpha: 1)]
        ("paper.design — Mesh Sessions" as NSString)
            .draw(at: NSPoint(x: 168, y: hF*0.5 - 18), withAttributes: subAttrs)

        // Scrubber track + fill.
        let trackY = hF - 72
        NSColor(white: 0.88, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 168, y: trackY, width: 240, height: 5), xRadius: 2.5, yRadius: 2.5).fill()
        NSColor(red:0.93,green:0.35,blue:0.57,alpha:1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 168, y: trackY, width: 240*pos, height: 5), xRadius: 2.5, yRadius: 2.5).fill()

        // Transport glyphs (drawn; hotspots live in the skin).
        let glyphAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 22, weight: .medium),
            .foregroundColor: NSColor(white: 0.15, alpha: 1)]
        ("\u{23EE}" as NSString).draw(at: NSPoint(x: 214, y: hF - 44), withAttributes: glyphAttrs)
        ((playing ? "\u{23F8}" : "\u{25B6}") as NSString).draw(at: NSPoint(x: 246, y: hF - 46), withAttributes: glyphAttrs)
        ("\u{23ED}" as NSString).draw(at: NSPoint(x: 278, y: hF - 44), withAttributes: glyphAttrs)

        NSGraphicsContext.restoreGraphicsState()
    }

    func tick() {
        let now = CACurrentMediaTime()
        let dt = now - last
        last = now
        // Draw the host's own live content BEFORE ticking so the engine composites this frame.
        drawHostContent()
        carapace_tick(engine, dt)
        // Hand the freshly-composited surface to CA. CRITICAL: assigning the SAME IOSurface
        // object every frame is cached by CoreAnimation by object identity — the picture
        // freezes even though the surface's pixels changed. Explicitly flag the contents as
        // changed (the documented mechanism for live IOSurface-backed layers).
        if let l = layer {
            l.isOpaque = false
            l.backgroundColor = NSColor.clear.cgColor
            l.contents = surface
            l.contentsGravity = .resizeAspect
            // Round the borderless window's corners to the SAME radius as the inner content
            // card (12pt) so the two read as concentric, and the gradient surround looks like
            // a real shaped window rather than a hard rectangle (drop shadow follows the mask).
            l.cornerRadius = 12
            l.masksToBounds = true
            let sel = Selector(("setContentsChanged"))
            if l.responds(to: sel) { l.perform(sel) }
        }
    }

    // MARK: - Mouse events for drag + tap dispatch

    override func mouseDown(with e: NSEvent) {
        // acceptsFirstMouse delivers clicks WITHOUT making the window key, so keyDown never
        // fires. Explicitly make it key here so +/- keyboard zoom works after a click.
        window?.makeKey()
        // Record screen-space anchor; do NOT forward to the engine yet —
        // we only forward on mouseUp if the gesture was a tap (no drag).
        dragStartMouse  = NSEvent.mouseLocation
        dragStartOrigin = window?.frame.origin
        dragStartZoom   = zoom
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
        if e.modifierFlags.contains(.option) {
            // ⌥-drag = RESIZE. Drag DOWN/RIGHT grows it, aspect-locked.
            setZoom(dragStartZoom * (1 + (dx - dy) * 0.004))
        } else {
            // Plain drag = MOVE the window.
            window?.setFrameOrigin(NSPoint(x: origin.x + dx, y: origin.y + dy))
        }
    }

    override func mouseUp(with e: NSEvent) {
        if !didDrag {
            // It was a tap — forward to the engine as a press+release.
            let p  = convert(e.locationInWindow, from: nil)
            let cx = Double(p.x) * Double(W) / Double(bounds.width)
            let cy = (Double(bounds.height) - Double(p.y)) * Double(H) / Double(bounds.height)
            print("[input] pointer tap at canvas (\(Int(cx)), \(Int(cy)))")
            carapace_pointer(engine, cx, cy, 0)
            // carapace_pointer hit-tests synchronously and, if the tap landed on the skin's
            // scrub region, calls invokeAction("scrub") → viewBox.view.scrubPending = true
            // BEFORE returning here — so cx (this same tap's design-space x) is exactly the
            // click position the scrub should seek to.
            if scrubPending, let p = player {
                // Content view{} spans design x 24..456; scrub strip spans canvas x 180..408.
                let localX = min(max(cx - 180, 0), 228)
                p.currentTime = (localX / 228) * p.duration
                scrubPending = false
            }
        }
        // If didDrag: window already moved; nothing more to do.
        dragStartMouse  = nil
        dragStartOrigin = nil
        didDrag = false
    }

    // MARK: - Zoom (scroll, pinch, or +/- keys) — aspect-locked, anchored top-left.

    /// Set the absolute zoom (clamped 0.5…3.0) and resize the window via explicit setFrame,
    /// keeping the top edge fixed so it grows downward and stays on screen.
    private func setZoom(_ z: CGFloat) {
        let nz = max(0.5, min(3.0, z))
        guard let win = window else { zoom = nz; return }
        zoom = nz
        let newW = baseW * zoom, newH = baseH * zoom
        let f = win.frame
        let top = f.origin.y + f.size.height
        win.setFrame(NSRect(x: f.origin.x, y: top - newH, width: newW, height: newH), display: true)
    }

    private func applyZoomDelta(_ factor: CGFloat) { setZoom(zoom * factor) }

    override func scrollWheel(with e: NSEvent) {
        // Some devices report scrollingDeltaY≈0 (trackpad phase events), so step a FIXED ±notch
        // past a deadzone rather than scaling by the (tiny) delta.
        let dy = e.scrollingDeltaY
        let factor: CGFloat = dy > 0.5 ? 1.05 : (dy < -0.5 ? 0.95 : 1.0)
        if factor != 1.0 { applyZoomDelta(factor) }
    }

    /// +/- keyboard zoom (works once the window is key — see makeKey() in mouseDown).
    override func keyDown(with e: NSEvent) {
        switch e.charactersIgnoringModifiers ?? "" {
        case "+", "=": applyZoomDelta(1.1)
        case "-", "_": applyZoomDelta(1.0 / 1.1)
        default:       super.keyDown(with: e)
        }
    }

    @objc func handlePinch(_ r: NSMagnificationGestureRecognizer) {
        guard r.state == .changed else { return }
        applyZoomDelta(1 + r.magnification)
        r.magnification = 0          // reset so deltas don't accumulate across .changed events
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
viewBox = WeakViewBox(view)
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

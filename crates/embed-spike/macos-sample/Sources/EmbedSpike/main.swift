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
// (24,40..456,276 → 432×236). The card is inset 40px at the top so the floating window
// controls have a gradient "titlebar" strip; the bottom stays at canvas y=276.
// The IOSurface is allocated at the backing scale (2× on Retina) so the player draws SHARP;
// the engine samples it into the cutout rect. The "paper" cutout behind it is rendered
// directly by the engine's transpiled mesh-gradient shader.
let CONTENT_W: Int = 432, CONTENT_H: Int = 236

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
        // Allocated at the backing scale so the player renders SHARP on Retina (the engine
        // upscales this surface into the cutout; a 1× surface looked blurry at 2×).
        let cScale = Int(scale.rounded())
        content = IOSurface(properties: [
            .width: CONTENT_W * cScale,
            .height: CONTENT_H * cScale,
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

        installTitlebarControls()
        setupAudio()
    }

    /// Floating window controls (AppKit overlays) over the gradient "titlebar" strip: a
    /// dot + "paper_mesh" label top-left, and a rounded pill with minimize / zoom / close
    /// top-right. AppKit overlays because the engine composites view{} OVER the vello layer,
    /// so carapace vector can't paint on top of the full-bleed paper shader.
    private func installTitlebarControls() {
        let top = CGFloat(H)   // view is non-flipped: top edge is at y = height.

        // Top-left: white dot + monospace label.
        let dot = NSView(frame: NSRect(x: 16, y: top - 25, width: 11, height: 11))
        dot.wantsLayer = true
        dot.layer?.backgroundColor = NSColor.white.cgColor
        dot.layer?.cornerRadius = 5.5
        dot.layer?.opacity = 0.9
        dot.autoresizingMask = [.maxXMargin, .minYMargin]
        addSubview(dot)

        let label = NSTextField(labelWithString: "paper_mesh")
        label.font = NSFont.monospacedSystemFont(ofSize: 13, weight: .medium)
        label.textColor = .white
        label.frame = NSRect(x: 34, y: top - 28, width: 170, height: 18)
        label.autoresizingMask = [.maxXMargin, .minYMargin]
        addSubview(label)

        // Top-right: rounded translucent pill with three glyph buttons.
        let pillW: CGFloat = 104, pillH: CGFloat = 28
        let pill = NSView(frame: NSRect(x: CGFloat(W) - pillW - 12, y: top - pillH - 8,
                                        width: pillW, height: pillH))
        pill.wantsLayer = true
        pill.layer?.backgroundColor = NSColor(white: 0.04, alpha: 0.30).cgColor
        pill.layer?.cornerRadius = 14
        pill.layer?.borderWidth = 1
        pill.layer?.borderColor = NSColor(white: 1, alpha: 0.16).cgColor
        pill.autoresizingMask = [.minXMargin, .minYMargin]
        addSubview(pill)

        let items: [(String, Selector)] = [
            ("\u{2013}", #selector(hostMinimize)),  // – minimize
            ("\u{25A2}", #selector(hostZoom)),      // ▢ zoom
            ("\u{2715}", #selector(hostClose)),     // ✕ close
        ]
        for (i, (title, sel)) in items.enumerated() {
            let b = NSButton(title: title, target: self, action: sel)
            b.isBordered = false
            b.font = NSFont.systemFont(ofSize: 14, weight: .medium)
            b.contentTintColor = .white
            b.frame = NSRect(x: 6 + CGFloat(i) * 32, y: 0, width: 30, height: pillH)
            pill.addSubview(b)
        }
    }

    @objc private func hostMinimize() { window?.miniaturize(nil) }
    @objc private func hostZoom()     { applyZoomDelta(1.12) }
    @objc private func hostClose()    { NSApp.terminate(nil) }

    /// Load the bundled sample clip into a real AVAudioPlayer (looped; the demo has no
    /// playlist, just one track that repeats).
    private func setupAudio() {
        // Prefer a LOCAL, git-ignored track (macos-sample/local-track.m4a) if present — this lets
        // the demo play a real song WITHOUT ever committing copyrighted audio. It lives outside the
        // SwiftPM target dir so it isn't a bundled resource and can't be staged. The committed,
        // bundled `sample.m4a` is a synth-tone placeholder used when no local track is present.
        let localURL = URL(fileURLWithPath: #filePath)   // .../macos-sample/Sources/EmbedSpike/main.swift
            .deletingLastPathComponent()                 // → Sources/EmbedSpike/
            .deletingLastPathComponent()                 // → Sources/
            .deletingLastPathComponent()                 // → macos-sample/
            .appendingPathComponent("local-track.m4a")
        let url: URL? = FileManager.default.fileExists(atPath: localURL.path)
            ? localURL
            : Bundle.module.url(forResource: "sample", withExtension: "m4a")
        guard let url else { print("[carapace] no audio file found"); return }
        player = try? AVAudioPlayer(contentsOf: url)
        player?.numberOfLoops = -1
        player?.prepareToPlay()
    }

    @objc private func zoomIn()  { applyZoomDelta(1.1) }
    @objc private func zoomOut() { applyZoomDelta(1.0 / 1.1) }

    required init?(coder: NSCoder) { fatalError() }

    /// Draw an SF Symbol (the macOS iconography) centered at (cx, cy) in the current flipped
    /// content context. Template symbols render black — reads correctly on the near-white card.
    private func drawSymbol(_ name: String, cx: CGFloat, cy: CGFloat, size: CGFloat) {
        let cfg = NSImage.SymbolConfiguration(pointSize: size, weight: .semibold)
        guard let img = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(cfg) else { return }
        let s = img.size
        img.draw(in: NSRect(x: cx - s.width / 2, y: cy - s.height / 2, width: s.width, height: s.height))
    }

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
        // The surface is allocated at the backing scale; derive it so we draw in LOGICAL
        // CONTENT_W×CONTENT_H units but render at full surface resolution (sharp on Retina).
        let cScale = max(1, content.width / CONTENT_W)

        let cs  = CGColorSpaceCreateDeviceRGB()
        let bmp = CGImageAlphaInfo.premultipliedFirst.rawValue
                | CGBitmapInfo.byteOrder32Little.rawValue
        guard let cg = CGContext(
            data:             base,
            width:            cw * cScale,
            height:           ch * cScale,
            bitsPerComponent: 8,
            bytesPerRow:      stride,
            space:            cs,
            bitmapInfo:       bmp
        ) else { return }

        // Scale to the logical CONTENT_W×CONTENT_H space (Retina sharpness), THEN flip the
        // raw bitmap context (y-up / bottom-left origin) to a top-left origin so BOTH the
        // layout (y grows down) AND AppKit text render upright and un-mirrored. Verified in
        // isolation: translate+scale(1,-1) THEN NSGraphicsContext(flipped:true).
        cg.scaleBy(x: CGFloat(cScale), y: CGFloat(cScale))
        cg.translateBy(x: 0, y: CGFloat(ch))
        cg.scaleBy(x: 1, y: -1)
        let ns = NSGraphicsContext(cgContext: cg, flipped: true)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = ns

        let wF = CGFloat(cw), hF = CGFloat(ch)
        let dur = player?.duration ?? 1
        let pos = CGFloat((player?.currentTime ?? 0) / max(dur, 0.001))
        let playing = player?.isPlaying ?? false

        // Clear to transparent so the card reveals the LIVE paper gradient behind it — the
        // engine alpha-blends this content over the paper layer.
        NSGraphicsContext.current?.cgContext.clear(CGRect(x: 0, y: 0, width: wF, height: hF))
        // TRANSLUCENT card: a light frosted panel (~62% white) so the soft gradient shines
        // through the whole card, not just the corners (macOS-vibrancy feel; no real blur —
        // the content pass has no access to the paper pixels, but the gradient is soft enough).
        NSColor(white: 0.99, alpha: 0.62).setFill()
        NSBezierPath(roundedRect: NSRect(x: 0, y: 0, width: wF, height: hF),
                     xRadius: 12, yRadius: 12).fill()


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

        // Scrubber track + fill + macOS slider thumb.
        let trackY = hF - 72
        NSColor(white: 0.88, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 168, y: trackY, width: 240, height: 5), xRadius: 2.5, yRadius: 2.5).fill()
        NSColor(red:0.93,green:0.35,blue:0.57,alpha:1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 168, y: trackY, width: 240*pos, height: 5), xRadius: 2.5, yRadius: 2.5).fill()
        let thumbX = 168 + 240 * pos
        NSColor.white.setFill()
        let thumb = NSBezierPath(ovalIn: NSRect(x: thumbX - 7, y: trackY + 2.5 - 7, width: 14, height: 14))
        thumb.fill()
        NSColor(white: 0, alpha: 0.14).setStroke()
        thumb.lineWidth = 0.5
        thumb.stroke()

        // Transport controls as SF Symbols (the macOS iconography). Centers align to the skin's
        // transport hotspots; SF Symbols render black (template) on the near-white card.
        drawSymbol("backward.fill",                        cx: 225, cy: hF - 33, size: 15)
        drawSymbol(playing ? "pause.fill" : "play.fill",   cx: 257, cy: hF - 33, size: 19)
        drawSymbol("forward.fill",                         cx: 289, cy: hF - 33, size: 15)

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
                // The scrubber track is DRAWN at content-local x=168 w=240; the content view{}
                // origin is canvas x=24, so the visible track spans canvas x 192..432 (w=240).
                // Map the click into that exact span so the seek matches where the thumb sits.
                let localX = min(max(cx - 192, 0), 240)
                p.currentTime = (localX / 240) * p.duration
                scrubPending = false
            }
            // Tapping the album art cycles the paper surround shader (also bound to the 's' key).
            // Handled here (not via a skin action) because it drives the engine's paper renderer.
            if cx >= 46, cx <= 170, cy >= 96, cy <= 220 {
                carapace_cycle_shader(engine)
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
        case "s", "S": carapace_cycle_shader(engine)   // cycle the paper surround shader
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

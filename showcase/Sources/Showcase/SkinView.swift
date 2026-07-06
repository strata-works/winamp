import AppKit
import IOSurface
import CCarapace

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

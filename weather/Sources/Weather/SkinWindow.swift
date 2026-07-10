import AppKit

/// Borderless windows can't become key/main by default; override so the skin receives input.
final class SkinWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

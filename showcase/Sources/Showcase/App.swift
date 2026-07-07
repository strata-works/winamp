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
        host = makePlaceholderHost()
        hostBox.host = host
        skinDirs = resolveSkinDirs()

        // Create the window + view once; applySkin sizes them and builds the first bridge.
        view = SkinView(frame: NSRect(x: 0, y: 0, width: 420, height: 660))
        view.onTab = { [weak self] in self?.cycleSkin() }
        window = SkinWindow(contentRect: NSRect(x: 200, y: 200, width: 420, height: 660),
                            styleMask: [.borderless], backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = view
        windowBox.window = window

        applySkin(dir: skinDirs[skinIndex])
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Point the app at `dir`: tear down the current engine, resize the window to the skin's
    /// canvas, and create a fresh bridge/pool at that size. MusicHost/audio are untouched, so
    /// playback + state persist across the swap.
    private func applySkin(dir: String) {
        let (w, h) = SkinManifest.canvas(atDir: dir, fallback: (420, 660))
        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())

        // 1. Destroy the current engine FIRST (its render thread joins in carapace_destroy),
        //    so no stale frame can fire after we re-point the frame sink.
        bridge = nil

        // 2. Resize the borderless window to the new skin's canvas, preserving the top-left corner.
        let topY = window.frame.origin.y + window.frame.height
        window.setContentSize(NSSize(width: w, height: h))
        var origin = window.frame.origin
        origin.y = topY - window.frame.height
        window.setFrameOrigin(origin)
        view.frame = NSRect(x: 0, y: 0, width: w, height: h)
        view.canvasW = Double(w)
        view.canvasH = Double(h)

        // 3. Build a fresh bridge/pool at the new size.
        guard let b = CarapaceBridge(skinDir: dir, width: w * scale, height: h * scale,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            print("[showcase] bridge init failed for \(dir)"); NSApp.terminate(nil); return
        }
        bridge = b
        view.bridge = b
    }

    private func cycleSkin() {
        skinIndex = (skinIndex + 1) % skinDirs.count
        applySkin(dir: skinDirs[skinIndex]) // window resizes to the next skin; MusicHost persists
    }
}

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
        // TEMP (Task 6 replaces with [faceplate, studio, cassette]): two different-sized skins to
        // exercise per-skin resize — starter (420×660) and the demo `reference` skin (342×394).
        let starter = repo.appendingPathComponent("showcase/skins/starter").path
        let reference = repo.appendingPathComponent("crates/carapace-demo/skins/reference").path
        return [starter, reference]
    }
}

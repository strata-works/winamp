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
        let alt = repo.appendingPathComponent("showcase/skins/alt").path
        return [starter, alt]
    }
}

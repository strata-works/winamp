import SwiftUI
import AppKit
import UniformTypeIdentifiers

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
    private var dither: DitherRenderer?
    private var ditherTimer: Timer?
    private var ditherStart: TimeInterval = 0

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)
        installMainMenu()
        host = makePlaceholderHost()
        hostBox.host = host
        skinDirs = resolveSkinDirs()

        // Create the window + view once; applySkin sizes them and builds the first bridge.
        view = SkinView(frame: NSRect(x: 0, y: 0, width: 420, height: 660))
        view.onTab = { [weak self] in self?.cycleSkin() }
        // Borderless (the skin IS the window) but keep close/miniaturize capabilities so the
        // real native traffic-light buttons work.
        window = SkinWindow(contentRect: NSRect(x: 200, y: 200, width: 420, height: 660),
                            styleMask: [.borderless, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = view
        windowBox.window = window

        applySkin(dir: skinDirs[skinIndex])
        installTrafficLights()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private var trafficLightButtons: [NSButton] = []

    /// Top-left origin (skin/top-origin coords) of the traffic-light cluster for the given skin;
    /// the three buttons march right from here at 20pt spacing. Each skin's baked chrome reserves a
    /// different clear spot (Faceplate bezel, Studio LCD's left, Cassette's dark top strip), so the
    /// app repositions the cluster on every skin swap rather than using one fixed offset.
    private func trafficLightOrigin(forDir dir: String) -> CGPoint {
        switch (dir as NSString).lastPathComponent {
        case "studio": return CGPoint(x: 16, y: 24)  // dark control strip left of the inset LCD
        default: return CGPoint(x: 16, y: 12)         // faceplate bezel / cassette cleared corner
        }
    }

    /// Add the real macOS traffic-light buttons (close / minimize / zoom). They render, hover, and
    /// behave natively — no baked glyphs. Created once; `positionTrafficLights` places them for the
    /// current skin and is re-run on every swap.
    private func installTrafficLights() {
        let mask: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable]
        let specs: [(NSWindow.ButtonType, Selector?)] = [
            (.closeButton, #selector(NSWindow.performClose(_:))),
            (.miniaturizeButton, #selector(NSWindow.miniaturize(_:))),
            (.zoomButton, nil),  // greyed: a fixed-canvas borderless skin can't zoom
        ]
        for (type, action) in specs {
            guard let b = NSWindow.standardWindowButton(type, for: mask) else { continue }
            if let action {
                b.target = window
                b.action = action
            } else {
                b.isEnabled = false
            }
            view.addSubview(b)
            trafficLightButtons.append(b)
        }
        positionTrafficLights(forDir: skinDirs[skinIndex])
    }

    /// Place the traffic-light cluster at the current skin's reserved spot. The view is bottom-origin
    /// (isFlipped == false), so convert the skin-space top offset to a bottom offset.
    private func positionTrafficLights(forDir dir: String) {
        guard !trafficLightButtons.isEmpty else { return }  // not yet created (first applySkin)
        let o = trafficLightOrigin(forDir: dir)
        for (i, b) in trafficLightButtons.enumerated() {
            b.setFrameOrigin(NSPoint(x: o.x + CGFloat(i) * 20,
                                     y: view.bounds.height - o.y - b.frame.height))
        }
    }

    /// Point the app at `dir`: tear down the current engine, resize the window to the skin's
    /// canvas, and create a fresh bridge/pool at that size. MusicHost/audio are untouched, so
    /// playback + state persist across the swap.
    private func applySkin(dir: String) {
        let (w, h) = SkinManifest.canvas(atDir: dir, fallback: (420, 660))
        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())

        // 1. Destroy the current engine FIRST (its render thread joins in carapace_destroy),
        //    so no stale frame can fire after we re-point the global frameSink. BOTH strong refs
        //    to the old bridge must be dropped here — `view.bridge` also retains it, so nil-ing
        //    only `bridge` would keep the old render thread alive past the new bridge's frameSink
        //    repoint (a cross-pool race). Drop view.bridge first, then bridge → old bridge deinits
        //    → carapace_destroy joins the old thread before we build the new one.
        view.bridge = nil
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
        positionTrafficLights(forDir: dir)  // re-place the cluster for this skin's chrome

        // 3. Build a fresh bridge/pool at the new size.
        let content = ditherSurface(forDir: dir, width: w * scale, height: h * scale)
        guard let b = CarapaceBridge(skinDir: dir, width: w * scale, height: h * scale,
                                     contentSurface: content,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            print("[showcase] bridge init failed for \(dir)"); NSApp.terminate(nil); return
        }
        bridge = b
        view.bridge = b
    }

    /// Start/stop the dither loop for the given skin. Only Studio declares a view{ id="host" }
    /// cutout, so we render (and pay GPU) only there; returns the content surface to hand the
    /// bridge (nil for other skins).
    private func ditherSurface(forDir dir: String, width: Int, height: Int) -> IOSurface? {
        stopDither()
        guard (dir as NSString).lastPathComponent == "studio",
              let r = DitherRenderer(width: width, height: height) else { return nil }
        dither = r
        ditherStart = Date().timeIntervalSinceReferenceDate
        let t = Timer(timeInterval: 1.0/60.0, repeats: true) { [weak self] _ in
            guard let self, let r = self.dither else { return }
            let time = Float(Date().timeIntervalSinceReferenceDate - self.ditherStart)
            r.render(time: time, level: Float(self.host.level()))
        }
        RunLoop.main.add(t, forMode: .common)   // keep ticking during window drags
        ditherTimer = t
        return r.surface
    }

    private func stopDither() {
        ditherTimer?.invalidate(); ditherTimer = nil; dither = nil
    }

    /// Live-swap to `dir`: the engine crossfades the incoming skin in over its declared duration
    /// while the old skin keeps animating (no teardown of the bridge/pool). After the fade,
    /// animate the borderless window to the new skin's canvas size (top-left corner fixed, same
    /// as `applySkin`'s resize) so the settle reads as a deliberate follow-through, not a snap.
    private func swapSkin(dir: String) {
        guard bridge.swap(skinDir: dir) else {
            // Live swap rejected (e.g. incompatible pool) — fall back to the full rebuild.
            applySkin(dir: dir)
            return
        }
        positionTrafficLights(forDir: dir)  // re-place chrome for the incoming skin now
        let ms = SkinManifest.durationMs(atDir: dir)
        let (w, h) = SkinManifest.canvas(atDir: dir, fallback: (420, 660))
        // Resize AFTER the crossfade completes so the seamless dissolve isn't disturbed; the
        // fixed IOSurface pool scales to fit during the brief settle (pool re-fit at the exact
        // new size is deferred).
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(ms)) { [weak self] in
            guard let self else { return }
            let window = self.window!
            let topY = window.frame.origin.y + window.frame.height
            var frame = window.frame
            frame.size = NSSize(width: w, height: h)
            frame.origin.y = topY - CGFloat(h)
            NSAnimationContext.runAnimationGroup { ctx in
                ctx.duration = 0.15
                window.animator().setFrame(frame, display: true)
            }
            self.view.canvasW = Double(w)
            self.view.canvasH = Double(h)
        }
    }

    private func cycleSkin() {
        skinIndex = (skinIndex + 1) % skinDirs.count
        swapSkin(dir: skinDirs[skinIndex]) // live crossfade; window settles to new size after the fade
    }

    /// Minimal main menu so ⌘O (Open Music…) works and is discoverable. The skin window itself
    /// stays borderless; this only adds the menu bar + Quit.
    private func installMainMenu() {
        let main = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit CarapaceShowcase",
                        action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        let fileItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        let open = NSMenuItem(title: "Open Music…", action: #selector(openMusic(_:)), keyEquivalent: "o")
        open.target = self
        fileMenu.addItem(open)
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        NSApp.mainMenu = main
    }

    /// Present an app-modal open panel (files and/or folders), import audio, and append to the
    /// playlist. Current playback/selection/volume are preserved by MusicHost.addTracks.
    @objc private func openMusic(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        panel.allowedContentTypes = [.audio]
        panel.prompt = "Add to Playlist"
        panel.message = "Choose audio files or folders to add to the playlist"
        guard panel.runModal() == .OK else { return }
        let urls = panel.urls
        Task { @MainActor in
            let tracks = await TrackImporter.importTracks(from: urls)
            host.addTracks(tracks)
        }
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
        return ["faceplate", "studio", "cassette"].map {
            repo.appendingPathComponent("showcase/skins/\($0)").path
        }
    }
}

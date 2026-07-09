import SwiftUI
import AppKit
import QuartzCore // CAMediaTimingFunction for the animated-resize transition
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
    private var dither: DitherRenderer?      // fills view{ id="host" }
    private var vizDither: DitherRenderer?   // fills the second view{ id="viz" } cutout
    private var ditherTimer: Timer?
    private var ditherStart: TimeInterval = 0
    // Studio's second cutout (`view{ id="viz" }`) in logical canvas px; its dither surface is sized
    // to these × backing scale.
    private let vizCutoutW = 270
    private let vizCutoutH = 48
    // Amber front color for the viz strip so the two cutouts read as distinct live regions.
    private let vizFront: (Float, Float, Float) = (255.0/255, 180.0/255, 90.0/255)

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
            // Never let AppKit's autoresize move these: `swapSkin` animates their origin explicitly in
            // lockstep with the window resize, and a flexible margin would fight that animation.
            b.autoresizingMask = []
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
        let content = ditherSurface(forDir: dir, width: w * scale, height: h * scale, scale: scale)
        guard let b = CarapaceBridge(skinDir: dir, width: w * scale, height: h * scale,
                                     contentSurface: content,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            print("[showcase] bridge init failed for \(dir)"); NSApp.terminate(nil); return
        }
        bridge = b
        view.bridge = b
        // 4. Attach the studio cutouts' content via the registry API. The "host" attach is redundant
        //    with the create seed above but keeps both entry paths (applySkin/swapSkin) uniform; the
        //    "viz" attach is the second region. No-ops for non-studio skins (dithers are nil). This is
        //    UAF-safe without a blocking clear: applySkin destroyed the OLD engine (joining its render
        //    thread) at step 1 BEFORE ditherSurface freed the old dithers, so nothing reads a freed
        //    surface here — these calls attach the freshly built dithers to the NEW engine `b`.
        if let host = dither { b.setContentSurface("host", host.surface, host.surface.width, host.surface.height) }
        if let viz = vizDither { b.setContentSurface("viz", viz.surface, viz.surface.width, viz.surface.height) }
    }

    /// Start/stop the dither loop for the given skin. Only Studio declares view{} cutouts, so we
    /// render (and pay GPU) only there; builds BOTH the host and viz dithers and a single 60fps timer
    /// that renders both. Returns the HOST content surface to hand the create/swap `"host"` seed (nil
    /// for other skins); the viz surface is attached separately via `setContentSurface` by the caller.
    private func ditherSurface(forDir dir: String, width: Int, height: Int, scale: Int) -> IOSurface? {
        stopDither()
        guard (dir as NSString).lastPathComponent == "studio",
              let host = DitherRenderer(width: width, height: height) else { return nil }
        dither = host
        // Second cutout: a smaller amber strip. Sized to the viz cutout's pixels × scale; the engine
        // derives content dims from the surface, so any size composites into the view{ id="viz" } rect.
        vizDither = DitherRenderer(width: vizCutoutW * scale, height: vizCutoutH * scale,
                                   cutoutW: Float(vizCutoutW), cutoutH: Float(vizCutoutH), front: vizFront)
        ditherStart = Date().timeIntervalSinceReferenceDate
        let t = Timer(timeInterval: 1.0/60.0, repeats: true) { [weak self] _ in
            guard let self else { return }
            let time = Float(Date().timeIntervalSinceReferenceDate - self.ditherStart)
            let level = Float(self.host.level())
            self.dither?.render(time: time, level: level)
            self.vizDither?.render(time: time, level: level)
        }
        RunLoop.main.add(t, forMode: .common)   // keep ticking during window drags
        ditherTimer = t
        return host.surface
    }

    private func stopDither() {
        ditherTimer?.invalidate(); ditherTimer = nil; dither = nil; vizDither = nil
    }

    /// Live-swap to `dir` at the incoming skin's NATIVE size: the engine adopts a new pool sized to
    /// the new skin, crossfades the incoming skin in at native resolution (the outgoing skin scales
    /// out during the fade), and the window is ANIMATED to the new size over the crossfade window so
    /// the geometry morphs in lockstep with the dissolve instead of snapping in one frame.
    private func swapSkin(dir: String) {
        let (cw, ch) = SkinManifest.canvas(atDir: dir, fallback: (420, 660))
        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())
        // The engine's render thread CPU-copies each content (dither) IOSurface every frame via a raw,
        // non-retained pointer. `ditherSurface` below tears down the previous dither renderers, which
        // own the sole strong refs to the OUTGOING skin's surfaces — releasing one unmaps its pages.
        // Unlike `applySkin` (which destroys the engine, JOINING the render thread, before freeing),
        // `swapResized` keeps the render thread running.
        //
        // A1 (content registry persists across a resized swap): a `swapResized` with a NULL content
        // surface now KEEPS the existing "host" entry instead of clearing it — so on a swap AWAY from
        // Studio the render thread would keep reading the outgoing surfaces after we free them. The
        // fix is an EXPLICIT blocking clear of EVERY content entry on the CURRENT engine BEFORE any
        // code frees the dithers those surfaces belong to: `setContentSurface(id, nil)` removes the
        // entry and blocks until the render thread dropped its ContentTex, so the outgoing surfaces are
        // safe to unmap. Clearing an absent id is a harmless no-op, so this is safe on any outgoing
        // skin (faceplate→studio, studio→cassette, …). The `withExtendedLifetime` pins below are kept
        // as defense-in-depth. See docs/superpowers/specs/2026-07-08-…-swap-design.md.
        if let b = bridge {
            _ = b.setContentSurface("host", nil, 0, 0)
            _ = b.setContentSurface("viz", nil, 0, 0)
        }
        let outgoingDither = dither
        let outgoingViz = vizDither
        let content = ditherSurface(forDir: dir, width: cw * scale, height: ch * scale, scale: scale)
        guard let b = bridge,
              b.swapResized(skinDir: dir, width: cw * scale, height: ch * scale, contentSurface: content)
        else {
            // Swap rejected → the engine kept the OLD (now-cleared) registry and its render thread is
            // still running. `applySkin` destroys the engine (joining the render thread) before it
            // frees; keep the outgoing dithers mapped until AFTER that join.
            applySkin(dir: dir) // fall back to full rebuild if the resized swap is rejected
            withExtendedLifetime(outgoingDither) {}
            withExtendedLifetime(outgoingViz) {}
            return
        }
        // swapResized returned Ok → the render thread has adopted the new pool + content. Attach the
        // new studio cutouts' content (no-op when the incoming skin has no dithers). The "host" attach
        // is redundant with the swap seed above but keeps both entry paths uniform; "viz" is the second
        // region. These reference the NEW dithers built by ditherSurface — never the outgoing ones.
        if let host = dither { b.setContentSurface("host", host.surface, host.surface.width, host.surface.height) }
        if let viz = vizDither { b.setContentSurface("viz", viz.surface, viz.surface.width, viz.surface.height) }
        // The outgoing surfaces were made safe by the blocking clears above (the engine dropped their
        // ContentTex); release the pins now, strictly after the engine stopped reading them.
        withExtendedLifetime(outgoingDither) {}
        withExtendedLifetime(outgoingViz) {}
        // Resize the borderless window to the new native size (top-left anchored). We ANIMATE the
        // resize over the same window the engine crossfades in, rather than snapping in one frame:
        // the old code hard-cut the window to the new size at swap start while the GPU dissolve ran
        // for another ~250 ms, so a hard geometry jump sat next to a soft fade — the transition's
        // dominant "jitter". Animating setFrame over the crossfade duration makes the geometry morph
        // in lockstep with the dissolve. The new pool is already native size, so the layer's
        // `.resizeAspect` gravity scales that native surface into the interpolating frame each step.
        let window = self.window!
        let topY = window.frame.origin.y + window.frame.height
        var frame = window.frame
        frame.size = NSSize(width: cw, height: ch)
        frame.origin.y = topY - CGFloat(ch)
        // Update the logical canvas immediately so hit-testing maps against the new skin from frame 1;
        // the view (contentView) autoresizes with the window as the animation drives it.
        view.canvasW = Double(cw)
        view.canvasH = Double(ch)
        // Traffic lights slide to the new skin's reserved spot in lockstep. Their target y derives
        // from the FINAL view height (ch), so they land correctly whatever the interpolating height is
        // mid-morph — matching `positionTrafficLights`' math but animated instead of snapped.
        let o = trafficLightOrigin(forDir: dir)
        let finalH = CGFloat(ch)
        NSAnimationContext.runAnimationGroup { ctx in
            // Match the engine's crossfade window: the incoming skin's declared `[transition]`
            // duration_ms (default 250 ms). All showcase skins use the default today.
            ctx.duration = Double(SkinManifest.durationMs(atDir: dir)) / 1000.0
            ctx.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            window.animator().setFrame(frame, display: true)
            for (i, b) in trafficLightButtons.enumerated() {
                b.animator().setFrameOrigin(NSPoint(x: o.x + CGFloat(i) * 20,
                                                    y: finalH - o.y - b.frame.height))
            }
        }
    }

    private func cycleSkin() {
        skinIndex = (skinIndex + 1) % skinDirs.count
        swapSkin(dir: skinDirs[skinIndex]) // live crossfade; window morphs to the new size during the fade
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

import SwiftUI
import AppKit
import CCarapace

@main
struct WeatherApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    var body: some Scene { Settings { EmptyView() } } // AppDelegate owns the skin window
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var window: SkinWindow!
    private var view: SkinView!
    private var host: WeatherHost!
    private var bridge: CarapaceBridge!
    private let service = WeatherService()
    private var refreshTimer: Timer?

    private let canvasW = 400
    private let canvasH = 680

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)
        installMainMenu()

        // First frame shows the bundled fixture immediately; the launch fetch replaces it.
        host = WeatherHost(model: (try? service.loadBundledFixture()) ?? .sample)
        hostBox.host = host

        view = SkinView(frame: NSRect(x: 0, y: 0, width: canvasW, height: canvasH))
        view.canvasW = Double(canvasW)
        view.canvasH = Double(canvasH)
        view.onKey = { [weak self] code in self?.handleKey(code) }

        window = SkinWindow(contentRect: NSRect(x: 200, y: 200, width: canvasW, height: canvasH),
                            styleMask: [.borderless, .closable, .miniaturizable],
                            backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.contentView = view
        windowBox.window = window

        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())
        guard let b = CarapaceBridge(skinDir: skinDir(), width: canvasW * scale, height: canvasH * scale,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            var msg = [CChar](repeating: 0, count: 256)
            _ = carapace_last_error(&msg, UInt(msg.count))
            fatalError("weather: bridge/skin load failed: \(String(cString: msg))")
        }
        bridge = b
        view.bridge = b

        // Launch-time presenter/automation overrides (also used by scripted verification):
        //   WX_COND=0..5 forces the condition · WX_SUN=-1..1 forces sun elevation ·
        //   WX_POS="x,y" positions the window (screen points, top-left origin).
        let env = ProcessInfo.processInfo.environment
        if let c = env["WX_COND"].flatMap(Double.init) { host.conditionOverride = c }
        if let s = env["WX_SUN"].flatMap(Double.init) { host.sunOverride = s }
        if let i = env["WX_INT"].flatMap(Double.init) { host.intensityOverride = i }
        if let a = env["WX_AGE"].flatMap(Double.init) { host.backdateConditionChange(seconds: a) }
        if let p = env["WX_POS"] {
            let parts = p.split(separator: ",").compactMap { Double($0) }
            if parts.count == 2, let screen = NSScreen.main {
                let topLeftY = screen.frame.maxY - parts[1] - CGFloat(canvasH)
                window.setFrameOrigin(NSPoint(x: parts[0], y: topLeftY))
            }
        }

        // Report the final window frame (global bottom-left coords) + main-screen height so
        // scripted verification can compute exact capture regions instead of guessing.
        let sh = NSScreen.screens.first?.frame.height ?? 0
        FileHandle.standardError.write("WX_FRAME \(window.frame.origin.x),\(window.frame.origin.y),\(window.frame.width),\(window.frame.height) SCREEN_H \(sh)\n".data(using: .utf8)!)

        if env["WX_SHY"] != nil {
            // Verification/automation mode: show the window without stealing focus.
            window.orderFrontRegardless()
        } else {
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
        }

        refresh()  // launch fetch
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 15 * 60, repeats: true) { [weak self] _ in
            self?.refresh()
        }
    }

    /// Fetch live weather off-main, then swap it into the host on the main thread. The host's
    /// `model` is lock-guarded, so the render thread's reads stay consistent across the swap.
    private func refresh() {
        Task { [weak self] in
            guard let self else { return }
            let model = await self.service.fetch()
            await MainActor.run { self.host.model = model }
        }
    }

    /// Absolute path to weather/skins/weather (App.swift is weather/Sources/Weather/App.swift).
    private func skinDir() -> String {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()   // Weather
            .deletingLastPathComponent()   // Sources
            .deletingLastPathComponent()   // weather (package root)
            .appendingPathComponent("skins/weather").path
    }

    // Presenter controls (overrides force only the shader; hero/hourly/daily text stays live):
    //   →/← tour condition · D cycles dawn/noon/dusk/night · S cycles season · R refetches.
    private func handleKey(_ code: UInt16) {
        switch code {
        case 124: host.conditionOverride = ConditionCycle.next(host.conditionOverride)          // →
        case 123: host.conditionOverride = ConditionCycle.prev(host.conditionOverride)          // ←
        case 2:   host.sunOverride = ConditionCycle.next(host.sunOverride, stops: SunMath.presenterStops) // D
        case 1:   host.seasonOverride = ConditionCycle.next(host.seasonOverride, upTo: 3)        // S
        case 15:  refresh()                                                                       // R
        default:  break
        }
    }

    private func installMainMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit CarapaceWeather",
                        action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)
        NSApp.mainMenu = main
    }
}

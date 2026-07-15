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

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)

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
    //   →/← tour condition · D toggles day/night · S cycles season · R refetches.
    private func handleKey(_ code: UInt16) {
        switch code {
        case 124: host.conditionOverride = ConditionCycle.next(host.conditionOverride)          // →
        case 123: host.conditionOverride = ConditionCycle.prev(host.conditionOverride)          // ←
        case 2:   host.isDayOverride = ConditionCycle.next(host.isDayOverride, upTo: 1)          // D
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

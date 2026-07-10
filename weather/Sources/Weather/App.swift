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
    private var trafficLightButtons: [NSButton] = []

    private let canvasW = 400
    private let canvasH = 680

    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)
        installMainMenu()

        host = WeatherHost(model: .sample)
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
        window.hasShadow = true
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

        installTrafficLights()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Absolute path to weather/skins/weather (App.swift is weather/Sources/Weather/App.swift).
    private func skinDir() -> String {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()   // Weather
            .deletingLastPathComponent()   // Sources
            .deletingLastPathComponent()   // weather (package root)
            .appendingPathComponent("skins/weather").path
    }

    // Debug (M1 verification scaffolding): → / ← cycle the mock condition to eyeball all six shaders.
    private func handleKey(_ code: UInt16) {
        let delta: Double
        switch code {
        case 124: delta = 1   // right arrow
        case 123: delta = -1  // left arrow
        default: return
        }
        var c = host.model.condition + delta
        if c > 5 { c = 0 }; if c < 0 { c = 5 }
        host.model.condition = c
    }

    private func installTrafficLights() {
        let mask: NSWindow.StyleMask = [.titled, .closable, .miniaturizable, .resizable]
        let specs: [(NSWindow.ButtonType, Selector?)] = [
            (.closeButton, #selector(NSWindow.performClose(_:))),
            (.miniaturizeButton, #selector(NSWindow.miniaturize(_:))),
            (.zoomButton, nil),  // greyed: a fixed-canvas borderless skin can't zoom
        ]
        for (type, action) in specs {
            guard let b = NSWindow.standardWindowButton(type, for: mask) else { continue }
            if let action { b.target = window; b.action = action } else { b.isEnabled = false }
            b.autoresizingMask = []
            view.addSubview(b)
            trafficLightButtons.append(b)
        }
        let ox: CGFloat = 16, oy: CGFloat = 14
        for (i, b) in trafficLightButtons.enumerated() {
            b.setFrameOrigin(NSPoint(x: ox + CGFloat(i) * 20,
                                     y: view.bounds.height - oy - b.frame.height))
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

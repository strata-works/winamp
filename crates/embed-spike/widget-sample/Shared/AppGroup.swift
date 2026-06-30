import Foundation

enum AppGroup {
    static let id = "group.carapace.spike"

    static var container: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id)!
    }
    /// The rendered "Now Playing" skin PNG the widget displays.
    static var renderURL: URL { container.appendingPathComponent("nowplaying.png") }
    /// Staged breadcrumbs the widget extension writes while probing an in-extension render,
    /// read back by the app — lets us see how far the extension got before any jetsam kill.
    static var probeLogURL: URL { container.appendingPathComponent("ext-probe.log") }
}

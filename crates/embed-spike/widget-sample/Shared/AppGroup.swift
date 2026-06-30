import Foundation

enum AppGroup {
    static let id = "group.carapace.spike"

    static var container: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id)!
    }
    /// The rendered "Now Playing" skin PNG the widget displays.
    static var renderURL: URL { container.appendingPathComponent("nowplaying.png") }
}

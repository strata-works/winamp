import Foundation

enum AppGroup {
    static let id = "group.carapace.spike"

    static var container: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id)!
    }
    /// The rendered Headspace faceplate (shaped, transparent background) the widget displays.
    static var faceplateURL: URL { container.appendingPathComponent("faceplate.png") }
}

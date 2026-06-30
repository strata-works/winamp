import Foundation

enum RenderMode: String {
    case live = "live GPU render"
    case seeded = "seeded (Simulator fallback)"
    case failed = "failed"
}

enum CarapaceBridge {
    /// The live "now playing" data the skin renders. On a real device this would come from the
    /// player; here it is sample data passed straight into carapace_render_info.
    static let nowPlaying: [String: String] = [
        "track": "Carapace Live \u{2713}",
        "artist": "device data probe",
        "time": "9:13 / 9:99",
        "position": "0.20",
    ]

    /// Render the "Now Playing" skin (with live data) into the App Group.
    ///
    /// Tries the live carapace GPU render first (works on a real device). In the iOS Simulator
    /// the render fails (Vello needs GPU INDIRECT_EXECUTION, which the Simulator's Metal lacks),
    /// so we fall back to the host-rendered `Seeded/nowplaying.png`.
    @discardableResult
    static func render(width: UInt32 = 640, height: UInt32 = 280) -> RenderMode {
        if renderLive(width: width, height: height) { return .live }
        if seedFromBundle() { return .seeded }
        return .failed
    }

    /// Live path: render the skin with the now-playing data via carapace_render_info.
    private static func renderLive(width: UInt32, height: UInt32) -> Bool {
        guard let skin = Bundle.main.url(forResource: "skin-nowplaying", withExtension: nil)?.path
        else { return false }
        let keys = Array(nowPlaying.keys)
        let vals = keys.map { nowPlaying[$0]! }
        // Bridge [String] -> const char* const* for the C ABI.
        return keys.withCStringArray { kptr in
            vals.withCStringArray { vptr in
                carapace_render_info(skin, width, height, UInt32(keys.count),
                                     kptr, vptr, AppGroup.renderURL.path)
            }
        }
    }

    /// Fallback: copy the bundled host-rendered render into the App Group.
    private static func seedFromBundle() -> Bool {
        guard let src = Bundle.main.url(forResource: "nowplaying", withExtension: "png",
                                        subdirectory: "Seeded")
        else { return false }
        let dst = AppGroup.renderURL
        let fm = FileManager.default
        try? fm.removeItem(at: dst)
        do { try fm.copyItem(at: src, to: dst); return true } catch { return false }
    }
}

private extension Array where Element == String {
    /// Call `body` with a `[UnsafePointer<CChar>?]` view of this array of strings.
    func withCStringArray<R>(_ body: (UnsafePointer<UnsafePointer<CChar>?>) -> R) -> R {
        let cstrings = map { strdup($0) }
        defer { cstrings.forEach { free($0) } }
        let ptrs = cstrings.map { UnsafePointer($0) }
        return ptrs.withUnsafeBufferPointer { body($0.baseAddress!) }
    }
}

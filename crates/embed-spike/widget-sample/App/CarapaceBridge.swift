import Foundation

enum RenderMode: String {
    case live = "live GPU render"
    case seeded = "seeded (Simulator fallback)"
    case failed = "failed"
}

enum CarapaceBridge {
    /// Render the Headspace faceplate into the App Group.
    ///
    /// Tries the live carapace GPU render first (works on a real device). In the iOS Simulator
    /// the render fails (Vello needs GPU INDIRECT_EXECUTION, which the Simulator's Metal lacks),
    /// so we fall back to the host-rendered `Seeded/faceplate.png`. Either way the App Group ends
    /// up with the shaped, transparent faceplate the widget floats.
    @discardableResult
    static func renderFaceplate(width: UInt32 = 684, height: UInt32 = 788) -> RenderMode {
        if let skin = Bundle.main.url(forResource: "skin-headspace", withExtension: nil)?.path,
           carapace_render_png(skin, width, height, 0.0, AppGroup.faceplateURL.path) {
            return .live
        }
        if seedFromBundle() { return .seeded }
        return .failed
    }

    /// Fallback: copy the bundled host-rendered faceplate into the App Group.
    private static func seedFromBundle() -> Bool {
        guard let src = Bundle.main.url(forResource: "faceplate", withExtension: "png",
                                        subdirectory: "Seeded")
        else { return false }
        let dst = AppGroup.faceplateURL
        let fm = FileManager.default
        try? fm.removeItem(at: dst)
        do { try fm.copyItem(at: src, to: dst); return true } catch { return false }
    }
}

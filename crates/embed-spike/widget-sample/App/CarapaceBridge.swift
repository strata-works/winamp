import Foundation

enum RenderMode: String {
    case live = "live GPU render"
    case seeded = "seeded (Simulator fallback)"
    case failed = "failed"
}

enum CarapaceBridge {
    /// Populate the App Group with a PNG per state.
    ///
    /// Tries the live carapace GPU render first. On a real device this draws every state with
    /// the engine. In the iOS Simulator the render fails (Vello needs GPU INDIRECT_EXECUTION,
    /// which the Simulator's Metal lacks), so we fall back to the host-rendered PNGs bundled
    /// under `Seeded/`. Either way the App Group ends up with state-0…N png for the widget.
    @discardableResult
    static func populateStates(width: UInt32 = 240, height: UInt32 = 80) -> RenderMode {
        if renderAllStatesLive(width: width, height: height) { return .live }
        if seedFromBundle() { return .seeded }
        return .failed
    }

    /// Live path: render every state with the engine straight into the App Group.
    private static func renderAllStatesLive(width: UInt32, height: UInt32) -> Bool {
        guard let skinDir = Bundle.main.url(forResource: "skin", withExtension: nil)?.path
        else { return false }
        for i in 0..<AppGroup.stateCount {
            let level = Double(i) / Double(AppGroup.stateCount - 1)   // 0.0 … 1.0
            let out = AppGroup.pngURL(state: i).path
            if !carapace_render_png(skinDir, width, height, level, out) { return false }
        }
        return true
    }

    /// Fallback path: copy the bundled host-rendered PNGs into the App Group.
    private static func seedFromBundle() -> Bool {
        let fm = FileManager.default
        for i in 0..<AppGroup.stateCount {
            guard let src = Bundle.main.url(forResource: "state-\(i)", withExtension: "png",
                                            subdirectory: "Seeded")
            else { return false }
            let dst = AppGroup.pngURL(state: i)
            try? fm.removeItem(at: dst)
            do { try fm.copyItem(at: src, to: dst) } catch { return false }
        }
        return true
    }
}

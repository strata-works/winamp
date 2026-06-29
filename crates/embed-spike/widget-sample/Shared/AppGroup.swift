import Foundation

enum AppGroup {
    static let id = "group.carapace.spike"
    static let stateCount = 4   // states 0..3

    static var container: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id)!
    }
    static func pngURL(state: Int) -> URL {
        container.appendingPathComponent("state-\(state).png")
    }
    static var stateFile: URL { container.appendingPathComponent("state.txt") }

    static func currentState() -> Int {
        (try? String(contentsOf: stateFile))
            .flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) } ?? 0
    }
    static func setState(_ s: Int) {
        try? String(s).write(to: stateFile, atomically: true, encoding: .utf8)
    }
}

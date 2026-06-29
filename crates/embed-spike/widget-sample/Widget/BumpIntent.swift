import AppIntents
import WidgetKit
import os

private let log = Logger(subsystem: "com.carapace.spike", category: "bump")

struct BumpIntent: AppIntent {
    static var title: LocalizedStringResource = "Bump"

    func perform() async throws -> some IntentResult {
        let cur = AppGroup.currentState()
        let next = (cur + 1) % AppGroup.stateCount
        AppGroup.setState(next)
        log.log("BumpIntent: \(cur) -> \(next) (container=\(AppGroup.container.path))")
        WidgetCenter.shared.reloadTimelines(ofKind: "CarapaceWidget")
        return .result()
    }
}

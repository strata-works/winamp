import XCTest
@testable import Weather

final class ConditionOverrideTests: XCTestCase {
    func testOverrideForcesOnlyWxCondition() {
        let host = WeatherHost(model: .sample)   // sample.condition == 1
        XCTAssertEqual(host.num("wx_condition"), 1)   // no override -> live model

        host.conditionOverride = 4
        XCTAssertEqual(host.num("wx_condition"), 4)   // override wins
        // Every other key ignores the override:
        XCTAssertEqual(host.num("wx_temp"), WeatherModel.sample.temp)
        XCTAssertEqual(host.str("condition_text"), WeatherModel.sample.conditionText)

        host.conditionOverride = nil
        XCTAssertEqual(host.num("wx_condition"), 1)   // back to live
    }

    func testCycleForwardWrapsThroughLive() {
        XCTAssertEqual(ConditionCycle.next(nil), 0)
        XCTAssertEqual(ConditionCycle.next(0), 1)
        XCTAssertEqual(ConditionCycle.next(4), 5)
        XCTAssertEqual(ConditionCycle.next(5), nil)   // 5 -> live
    }

    func testCycleBackwardWrapsThroughLive() {
        XCTAssertEqual(ConditionCycle.prev(nil), 5)
        XCTAssertEqual(ConditionCycle.prev(5), 4)
        XCTAssertEqual(ConditionCycle.prev(1), 0)
        XCTAssertEqual(ConditionCycle.prev(0), nil)   // 0 -> live
    }
}

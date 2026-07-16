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
        XCTAssertEqual(ConditionCycle.next(5), 6)     // into the demo conditions
        XCTAssertEqual(ConditionCycle.next(7), nil)   // 7 -> live
    }

    func testCycleBackwardWrapsThroughLive() {
        XCTAssertEqual(ConditionCycle.prev(nil), 7)
        XCTAssertEqual(ConditionCycle.prev(7), 6)
        XCTAssertEqual(ConditionCycle.prev(1), 0)
        XCTAssertEqual(ConditionCycle.prev(0), nil)   // 0 -> live
    }

    func testSunOverrideForcesOnlyWxSun() {
        let host = WeatherHost(model: .sample)
        host.sunOverride = -1.0
        XCTAssertEqual(host.num("wx_sun"), -1.0)           // override wins
        XCTAssertEqual(host.num("wx_condition"), WeatherModel.sample.condition) // others live
        host.sunOverride = nil
        let live = host.num("wx_sun")!
        XCTAssertTrue(live >= -1 && live <= 1)             // back to live (wall-clock value)
    }

    func testSeasonOverrideForcesOnlyWxSeason() {
        let host = WeatherHost(model: .sample)   // sample.season == 2
        XCTAssertEqual(host.num("wx_season"), 2)
        host.seasonOverride = 0
        XCTAssertEqual(host.num("wx_season"), 0)
        XCTAssertEqual(host.num("wx_temp"), WeatherModel.sample.temp)  // others live
        host.seasonOverride = nil
        XCTAssertEqual(host.num("wx_season"), 2)
    }

    func testGeneralizedCycleBounds() {
        XCTAssertEqual(ConditionCycle.next(nil, upTo: 1), 0)
        XCTAssertEqual(ConditionCycle.next(0, upTo: 1), 1)
        XCTAssertEqual(ConditionCycle.next(1, upTo: 1), nil)   // day/night wraps at 1
        XCTAssertEqual(ConditionCycle.next(3, upTo: 3), nil)   // season wraps at 3
        XCTAssertEqual(ConditionCycle.prev(nil, upTo: 3), 3)
        XCTAssertEqual(ConditionCycle.prev(0, upTo: 3), nil)
        // 1-arg condition cycle spans the demo conditions (upTo 7):
        XCTAssertEqual(ConditionCycle.next(7), nil)
        XCTAssertEqual(ConditionCycle.next(nil), 0)
    }
}

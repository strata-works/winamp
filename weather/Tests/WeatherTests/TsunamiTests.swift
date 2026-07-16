import XCTest
@testable import Weather

final class TsunamiTests: XCTestCase {
    func testPhaseWrapsAtPeriod() {
        XCTAssertEqual(Tsunami.phase(age: 0), 0, accuracy: 1e-9)
        XCTAssertEqual(Tsunami.phase(age: 16), 0.5, accuracy: 1e-9)
        XCTAssertEqual(Tsunami.phase(age: 32), 0, accuracy: 1e-9)
        XCTAssertEqual(Tsunami.phase(age: 48), 0.5, accuracy: 1e-9)
    }

    func testEngulfWindow() {
        XCTAssertFalse(Tsunami.isEngulfed(age: 0.60 * 32 - 0.1))
        XCTAssertTrue(Tsunami.isEngulfed(age: 0.60 * 32 + 0.1))
        XCTAssertTrue(Tsunami.isEngulfed(age: 0.74 * 32 - 0.1))
        XCTAssertFalse(Tsunami.isEngulfed(age: 0.74 * 32 + 0.1))
        XCTAssertTrue(Tsunami.isEngulfed(age: 32 + 0.60 * 32 + 0.1))   // second cycle
    }

    func testEngulfBlanksAllUIOnlyForTsunami() {
        let host = WeatherHost(model: .sample)
        host.conditionOverride = 7
        host.backdateConditionChange(seconds: 0.65 * 32)     // mid-engulf
        XCTAssertEqual(host.str("location"), "")
        XCTAssertEqual(host.str("temp_now"), "")
        XCTAssertEqual(host.str("wx_hour_0_time"), "")
        XCTAssertEqual(host.rowCount(), 0)
        host.backdateConditionChange(seconds: 0.30 * 32)     // calm phase
        XCTAssertEqual(host.str("location"), WeatherModel.sample.location)
        XCTAssertEqual(host.rowCount(), 7)
        host.conditionOverride = 4                            // storm never blanks
        host.backdateConditionChange(seconds: 0.65 * 32)
        XCTAssertEqual(host.str("location"), WeatherModel.sample.location)
    }

    func testTourReachesDemoConditions() {
        XCTAssertEqual(ConditionCycle.next(5), 6)
        XCTAssertEqual(ConditionCycle.next(6), 7)
        XCTAssertNil(ConditionCycle.next(7))                  // 7 -> live
        XCTAssertEqual(ConditionCycle.prev(nil), 7)
        XCTAssertEqual(ConditionCycle.prev(7), 6)
    }
}

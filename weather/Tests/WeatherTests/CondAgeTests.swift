import XCTest
@testable import Weather

final class CondAgeTests: XCTestCase {
    func testSnowPileThreshold() {
        XCTAssertEqual(SnowPile.buriedRows(age: 0), 0)
        XCTAssertEqual(SnowPile.buriedRows(age: 134.9), 0)
        XCTAssertEqual(SnowPile.buriedRows(age: 135), 1)
        XCTAssertEqual(SnowPile.buriedRows(age: 10_000), 1)
    }

    func testConditionAgeResetsOnOverrideChange() {
        let host = WeatherHost(model: .sample)
        let t0 = Date()
        XCTAssertLessThan(host.conditionAge(now: t0), 1.0)            // fresh host ≈ 0
        // Simulate time passing without changes: age grows.
        XCTAssertEqual(host.conditionAge(now: t0.addingTimeInterval(60)), 60, accuracy: 1.0)
        // Changing the effective condition resets the clock.
        host.conditionOverride = 3
        XCTAssertLessThan(host.conditionAge(now: Date()), 1.0)
        // Setting the SAME override again must NOT reset (no-op change).
        let mid = Date().addingTimeInterval(30)
        host.conditionOverride = 3
        XCTAssertEqual(host.conditionAge(now: mid), 30, accuracy: 1.5)
    }

    func testRowCountBuriesLastRowInSnowOnly() {
        let host = WeatherHost(model: .sample)          // sample condition = 1 (cloud)
        host.conditionOverride = 3                      // snow
        XCTAssertEqual(host.rowCount(now: Date().addingTimeInterval(200)), 6)  // 7 - 1 buried
        XCTAssertEqual(host.rowCount(now: Date().addingTimeInterval(10)), 7)   // too young
        host.conditionOverride = 4                      // storm: no burial
        XCTAssertEqual(host.rowCount(now: Date().addingTimeInterval(200)), 7)
    }

    func testWxCondAgeUniformIsNumeric() {
        let host = WeatherHost(model: .sample)
        let v = host.num("wx_cond_age")
        XCTAssertNotNil(v)
        XCTAssertGreaterThanOrEqual(v!, 0)
    }
}

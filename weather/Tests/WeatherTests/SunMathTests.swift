import XCTest
@testable import Weather

final class SunMathTests: XCTestCase {
    /// Fixed-date helper (UTC, 2026-07-15 unless day overridden).
    private func date(_ hour: Int, _ minute: Int = 0, day: Int = 15) -> Date {
        var c = DateComponents()
        c.year = 2026; c.month = 7; c.day = day; c.hour = hour; c.minute = minute
        c.timeZone = TimeZone(secondsFromGMT: 0)
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(secondsFromGMT: 0)!
        return cal.date(from: c)!
    }
    // 12h day: sunrise 06:00, sunset 18:00.
    private var sr: Date { date(6) }
    private var ss: Date { date(18) }

    func testNoonIsOne() {
        XCTAssertEqual(SunMath.sunElevation(now: date(12), sunrise: sr, sunset: ss), 1.0, accuracy: 1e-9)
    }
    func testSunriseAndSunsetAreZero() {
        XCTAssertEqual(SunMath.sunElevation(now: date(6), sunrise: sr, sunset: ss), 0.0, accuracy: 1e-9)
        XCTAssertEqual(SunMath.sunElevation(now: date(18), sunrise: sr, sunset: ss), 0.0, accuracy: 1e-9)
    }
    func testMidMorningIsLinear() {
        XCTAssertEqual(SunMath.sunElevation(now: date(9), sunrise: sr, sunset: ss), 0.5, accuracy: 1e-9)
    }
    func testNightMidpointIsMinusOne() {
        // Night arc 18:00 -> next 06:00; midpoint = 00:00 next day.
        XCTAssertEqual(SunMath.sunElevation(now: date(0, day: 16), sunrise: sr, sunset: ss), -1.0, accuracy: 1e-9)
    }
    func testBeforeSunriseIsSmallNegative() {
        // 05:00 same day = 11h after sunset in the wrapped 12h night arc -> -(1-|2*(11/12)-1|) = -1/6.
        XCTAssertEqual(SunMath.sunElevation(now: date(5), sunrise: sr, sunset: ss), -1.0 / 6.0, accuracy: 1e-9)
    }
    func testAfterSunsetIsNegative() {
        // 21:00 = 3h into the 12h night arc -> g=0.25 -> -0.5.
        XCTAssertEqual(SunMath.sunElevation(now: date(21), sunrise: sr, sunset: ss), -0.5, accuracy: 1e-9)
    }
    func testDegenerateDayReturnsZero() {
        XCTAssertEqual(SunMath.sunElevation(now: date(12), sunrise: sr, sunset: sr), 0.0)
    }
    func testPresenterStopsCycle() {
        var v: Double? = nil
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, 0.1)   // dawn
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, 1.0)   // noon
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, -0.1)  // dusk
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertEqual(v, -1.0)  // night
        v = ConditionCycle.next(v, stops: SunMath.presenterStops); XCTAssertNil(v)          // -> live
    }
}

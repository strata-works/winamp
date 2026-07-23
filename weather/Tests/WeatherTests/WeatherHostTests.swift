import XCTest
@testable import Weather

final class WeatherHostTests: XCTestCase {
    private let host = WeatherHost(model: .sample)

    func testShaderUniformsAreNumeric() {
        XCTAssertEqual(host.num("wx_condition"), WeatherModel.sample.condition)
        let sun = host.num("wx_sun")!            // live value depends on wall-clock
        XCTAssertTrue(sun >= -1 && sun <= 1)
        XCTAssertEqual(host.num("wx_temp"), WeatherModel.sample.temp)
        XCTAssertEqual(host.num("wx_intensity"), WeatherModel.sample.intensity)
        XCTAssertEqual(host.num("wx_season"), WeatherModel.sample.season)
        XCTAssertNil(host.num("location"))          // strings are not numeric
        XCTAssertNil(host.num("nonsense_key"))
    }

    func testHeroStrings() {
        XCTAssertEqual(host.str("location"), WeatherModel.sample.location)
        XCTAssertEqual(host.str("condition_text"), WeatherModel.sample.conditionText)
        XCTAssertEqual(host.str("temp_now"), WeatherModel.sample.tempNow)
        XCTAssertEqual(host.str("hi_lo"), WeatherModel.sample.hiLo)
        XCTAssertEqual(host.str("feels"), WeatherModel.sample.feels)
        XCTAssertNil(host.str("wx_condition"))      // numerics are not strings
    }

    func testHourlyCells() {
        XCTAssertEqual(host.str("wx_hour_0_time"), WeatherModel.sample.hours[0].time)
        XCTAssertEqual(host.str("wx_hour_0_temp"), WeatherModel.sample.hours[0].temp)
        XCTAssertEqual(host.str("wx_hour_11_temp"), WeatherModel.sample.hours[11].temp)
        XCTAssertNil(host.str("wx_hour_99_temp"))   // out of range
    }

    func testMalformedHourKeysReturnNil() {
        XCTAssertNil(host.str("wx_hour_time"))   // no index → nil, must not crash
        XCTAssertNil(host.str("wx_hour_temp"))
        XCTAssertNil(host.str("wx_hour__time"))  // empty index → nil
    }

    func testDailyRows() {
        XCTAssertEqual(host.rowCount(), WeatherModel.sample.days.count)
        let d = WeatherModel.sample.days[0]
        XCTAssertEqual(host.rowString(0, field: "day"), d.day)
        XCTAssertEqual(host.rowString(0, field: "glyph"), d.glyph)
        XCTAssertEqual(host.rowString(0, field: "hi"), d.hi)
        XCTAssertEqual(host.rowString(0, field: "lo"), d.lo)
        XCTAssertNil(host.rowString(0, field: "bogus"))
        XCTAssertNil(host.rowString(99, field: "day"))   // out of range
    }
}

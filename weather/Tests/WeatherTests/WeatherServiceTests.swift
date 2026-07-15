import XCTest
@testable import Weather

final class WeatherServiceTests: XCTestCase {
    private let service = WeatherService()

    // Decodes the bundled fixture through the SAME path used for live data.
    private func decoded() throws -> WeatherModel { try service.loadBundledFixture() }

    func testDecodeScalarsAndStrings() throws {
        let m = try decoded()
        XCTAssertEqual(m.location, "Accra")
        XCTAssertEqual(m.condition, 1)          // code 3 -> cloud
        XCTAssertEqual(m.conditionText, "Partly cloudy")
        XCTAssertEqual(m.sunrise, WeatherService.parseLocal("2026-07-11T05:58", offsetSeconds: 0))
        XCTAssertEqual(m.sunset, WeatherService.parseLocal("2026-07-11T18:13", offsetSeconds: 0))
        XCTAssertEqual(m.temp, 27, accuracy: 0.001)
        XCTAssertEqual(m.tempNow, "27°")
        XCTAssertEqual(m.feels, "Feels 30°")
        XCTAssertEqual(m.hiLo, "H:31° L:24°")
        XCTAssertEqual(m.intensity, 0.75, accuracy: 0.001)   // cloud -> cloud_cover/100
        XCTAssertEqual(m.season, 2)             // July -> summer
    }

    func testDecodeHourlyWindowStartsAtCurrentHour() throws {
        let m = try decoded()
        XCTAssertEqual(m.hours.count, 12)
        XCTAssertEqual(m.hours[0].time, "12h")  // current.time is 12:00
        XCTAssertEqual(m.hours[0].temp, "27°")
        XCTAssertEqual(m.hours[11].time, "23h")
        XCTAssertEqual(m.hours[11].temp, "24°")
    }

    func testDecodeDailyRows() throws {
        let m = try decoded()
        XCTAssertEqual(m.days.count, 7)
        XCTAssertEqual(m.days[0].day, "Sat")    // 2026-07-11 is a Saturday
        XCTAssertEqual(m.days[0].glyph, "⛅")    // code 3 -> cloud
        XCTAssertEqual(m.days[0].hi, "31°")
        XCTAssertEqual(m.days[0].lo, "24°")
        XCTAssertEqual(m.days[4].glyph, "❄")     // code 71 -> snow
        XCTAssertEqual(m.days[4].hi, "5°")
        XCTAssertEqual(m.days[4].lo, "-1°")      // negative rounds correctly
    }

    func testWmoBucketMapping() {
        XCTAssertEqual(WeatherService.wmoBucket(0), 0)
        XCTAssertEqual(WeatherService.wmoBucket(1), 0)
        XCTAssertEqual(WeatherService.wmoBucket(2), 1)
        XCTAssertEqual(WeatherService.wmoBucket(3), 1)
        XCTAssertEqual(WeatherService.wmoBucket(45), 5)
        XCTAssertEqual(WeatherService.wmoBucket(48), 5)
        XCTAssertEqual(WeatherService.wmoBucket(61), 2)
        XCTAssertEqual(WeatherService.wmoBucket(82), 2)
        XCTAssertEqual(WeatherService.wmoBucket(71), 3)
        XCTAssertEqual(WeatherService.wmoBucket(86), 3)
        XCTAssertEqual(WeatherService.wmoBucket(95), 4)
        XCTAssertEqual(WeatherService.wmoBucket(99), 4)
        XCTAssertEqual(WeatherService.wmoBucket(1234), 1)   // unmapped -> cloud
    }

    func testIntensityHeuristic() {
        XCTAssertEqual(WeatherService.intensity(bucket: 2, precipitation: 16, cloudCover: 50), 1.0, accuracy: 0.001)
        XCTAssertEqual(WeatherService.intensity(bucket: 2, precipitation: 0.4, cloudCover: 50), 0.15, accuracy: 0.001)
        XCTAssertEqual(WeatherService.intensity(bucket: 1, precipitation: 0, cloudCover: 75), 0.75, accuracy: 0.001)
        XCTAssertEqual(WeatherService.intensity(bucket: 0, precipitation: 0, cloudCover: 100), 0.3, accuracy: 0.001)
    }

    func testSeasonForMonth() {
        XCTAssertEqual(WeatherService.seasonForMonth(1), 0)
        XCTAssertEqual(WeatherService.seasonForMonth(4), 1)
        XCTAssertEqual(WeatherService.seasonForMonth(7), 2)
        XCTAssertEqual(WeatherService.seasonForMonth(10), 3)
        XCTAssertEqual(WeatherService.seasonForMonth(12), 0)
    }

    func testUrlHasExpectedQuery() {
        let u = service.url.absoluteString
        XCTAssertTrue(u.contains("latitude=5.55"))
        XCTAssertTrue(u.contains("longitude=-0.2"))
        XCTAssertTrue(u.contains("current=temperature_2m,is_day,weather_code,apparent_temperature,precipitation,cloud_cover"))
        XCTAssertTrue(u.contains("hourly=temperature_2m,weather_code"))
        XCTAssertTrue(u.contains("daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset"))
        XCTAssertTrue(u.contains("timezone=auto"))
        XCTAssertTrue(u.contains("forecast_days=7"))
    }
}

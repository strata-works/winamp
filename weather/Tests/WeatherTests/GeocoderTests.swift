import XCTest
@testable import Weather

final class GeocoderTests: XCTestCase {
    // Trimmed Open-Meteo /v1/search shape: one fully-populated row, one with no admin1.
    private let json = """
    { "results": [
        { "id": 1, "name": "Cape Town", "latitude": -33.9258, "longitude": 18.4232,
          "country": "South Africa", "admin1": "Western Cape" },
        { "id": 2, "name": "Singapore", "latitude": 1.2897, "longitude": 103.8501,
          "country": "Singapore" }
    ] }
    """.data(using: .utf8)!

    func testDecodeMapsRows() throws {
        let r = try OpenMeteoGeocoder.decode(json)
        XCTAssertEqual(r.count, 2)
        XCTAssertEqual(r[0], GeoResult(name: "Cape Town", admin1: "Western Cape",
                                       country: "South Africa", latitude: -33.9258, longitude: 18.4232))
        XCTAssertNil(r[1].admin1)
    }

    func testLabelFormatting() {
        XCTAssertEqual(GeoResult(name: "Cape Town", admin1: "Western Cape",
                                 country: "South Africa", latitude: 0, longitude: 0).label,
                       "Cape Town, Western Cape, South Africa")
        XCTAssertEqual(GeoResult(name: "Singapore", admin1: nil,
                                 country: "Singapore", latitude: 0, longitude: 0).label,
                       "Singapore, Singapore")
        XCTAssertEqual(GeoResult(name: "Nowhere", admin1: nil, country: nil,
                                 latitude: 0, longitude: 0).label, "Nowhere")
    }

    func testEmptyResponseDecodesToEmptyArray() throws {
        // Open-Meteo omits "results" entirely when there are no matches.
        let empty = #"{ "generationtime_ms": 0.5 }"#.data(using: .utf8)!
        XCTAssertEqual(try OpenMeteoGeocoder.decode(empty), [])
    }

    func testUrlHasExpectedQuery() {
        let u = OpenMeteoGeocoder().url("Cape Town").absoluteString
        XCTAssertTrue(u.hasPrefix("https://geocoding-api.open-meteo.com/v1/search"))
        XCTAssertTrue(u.contains("name=Cape%20Town"))
        XCTAssertTrue(u.contains("count=5"))
        XCTAssertTrue(u.contains("language=en"))
        XCTAssertTrue(u.contains("format=json"))
    }
}

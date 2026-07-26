import XCTest
@testable import Weather

final class WeatherHostSearchTests: XCTestCase {
    private func activeSearch() -> LocationSearch {
        var s = LocationSearch()
        s.enter()
        s.typed("c"); s.typed("a"); s.typed("p")
        s.setResults([
            GeoResult(name: "Cape Town", admin1: "Western Cape", country: "South Africa",
                      latitude: -33.9, longitude: 18.4),
            GeoResult(name: "Cape Coral", admin1: "Florida", country: "United States",
                      latitude: 26.5, longitude: -81.9),
        ], forQuery: "cap")
        s.moveSelection(1)   // select "Cape Coral"
        return s
    }

    func testInactiveHidesEverything() {
        let host = WeatherHost(model: .sample)          // default search is inactive
        XCTAssertEqual(host.num("search_active"), 0)
        XCTAssertEqual(host.str("search_query"), "")
        XCTAssertEqual(host.str("search_status"), "")
        XCTAssertEqual(host.str("search_hint"), "")
        XCTAssertEqual(host.str("search_row_0_label"), "")
    }

    func testActiveExposesQueryStatusHint() {
        let host = WeatherHost(model: .sample)
        host.search = activeSearch()
        XCTAssertEqual(host.num("search_active"), 1)
        XCTAssertEqual(host.str("search_query"), "cap")
        XCTAssertEqual(host.str("search_status"), "")        // results shown → no status line
        XCTAssertEqual(host.str("search_hint"), "↑↓ select · ⏎ go · esc cancel")
    }

    func testRowLabelsCarrySelectionMarker() {
        let host = WeatherHost(model: .sample)
        host.search = activeSearch()                          // selected index 1
        XCTAssertEqual(host.str("search_row_0_label"), "    Cape Town, Western Cape, South Africa")
        XCTAssertEqual(host.str("search_row_1_label"), "‣  Cape Coral, Florida, United States")
        XCTAssertEqual(host.str("search_row_2_label"), "")   // only 2 results
    }

    func testStatusLineWhenNoResults() {
        let host = WeatherHost(model: .sample)
        var s = LocationSearch(); s.enter(); s.typed("z"); s.setResults([], forQuery: "z")
        host.search = s
        XCTAssertEqual(host.str("search_status"), "No matches")
        XCTAssertEqual(host.str("search_row_0_label"), "")
    }
}

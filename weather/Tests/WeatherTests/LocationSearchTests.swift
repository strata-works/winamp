import XCTest
@testable import Weather

final class LocationSearchTests: XCTestCase {
    private func r(_ name: String) -> GeoResult {
        GeoResult(name: name, admin1: nil, country: nil, latitude: 0, longitude: 0)
    }

    func testDefaultIsInactive() {
        let s = LocationSearch()
        XCTAssertFalse(s.active)
        XCTAssertEqual(s.status, .idle)
    }

    func testEnterActivatesAndResets() {
        var s = LocationSearch()
        s.enter()
        XCTAssertTrue(s.active)
        XCTAssertEqual(s.query, "")
        XCTAssertEqual(s.status, .idle)
    }

    func testTypedAppendsAndQuerying() {
        var s = LocationSearch(); s.enter()
        s.typed("c"); s.typed("a")
        XCTAssertEqual(s.query, "ca")
        XCTAssertEqual(s.status, .querying)
    }

    func testBackspaceToEmptyReturnsIdle() {
        var s = LocationSearch(); s.enter()
        s.typed("a")
        s.setResults([r("A")], forQuery: "a")
        s.backspace()
        XCTAssertEqual(s.query, "")
        XCTAssertEqual(s.results, [])
        XCTAssertEqual(s.status, .idle)
    }

    func testMoveSelectionClamps() {
        var s = LocationSearch(); s.enter()
        s.typed("x")
        s.setResults([r("A"), r("B")], forQuery: "x")
        XCTAssertEqual(s.selected, 0)
        s.moveSelection(-1)                 // clamp low
        XCTAssertEqual(s.selected, 0)
        s.moveSelection(1)
        XCTAssertEqual(s.selected, 1)
        s.moveSelection(1)                  // clamp high
        XCTAssertEqual(s.selected, 1)
    }

    func testStaleResultsIgnored() {
        var s = LocationSearch(); s.enter()
        s.typed("c"); s.typed("a"); s.typed("p")     // query == "cap"
        s.setResults([r("Cairo")], forQuery: "ca")   // stale (debounced earlier keystroke)
        XCTAssertEqual(s.results, [])
        XCTAssertEqual(s.status, .querying)
    }

    func testEmptyResultsSetEmptyStatus() {
        var s = LocationSearch(); s.enter()
        s.typed("z"); s.setResults([], forQuery: "z")
        XCTAssertEqual(s.status, .empty)
    }

    func testSetErrorStatus() {
        var s = LocationSearch(); s.enter()
        s.typed("z"); s.setError(forQuery: "z")
        XCTAssertEqual(s.status, .error)
        XCTAssertEqual(s.results, [])
    }

    func testCommitReturnsSelectedOnlyWhenResults() {
        var s = LocationSearch(); s.enter()
        s.typed("x")
        XCTAssertNil(s.commit())                          // querying, no results
        s.setResults([r("A"), r("B")], forQuery: "x")
        s.moveSelection(1)
        XCTAssertEqual(s.commit()?.name, "B")
        s.setResults([], forQuery: "x")
        XCTAssertNil(s.commit())                          // empty
    }

    func testStatusLineText() {
        var s = LocationSearch(); s.enter()
        XCTAssertEqual(s.statusLine, "Type a city…")
        s.typed("z"); XCTAssertEqual(s.statusLine, "Searching…")
        s.setResults([], forQuery: "z"); XCTAssertEqual(s.statusLine, "No matches")
        s.setError(forQuery: "z"); XCTAssertEqual(s.statusLine, "Offline — check connection")
        s.setResults([r("A")], forQuery: "z"); XCTAssertEqual(s.statusLine, "")
    }

    func testExitDeactivates() {
        var s = LocationSearch(); s.enter(); s.typed("a")
        s.exit()
        XCTAssertFalse(s.active)
        XCTAssertEqual(s.query, "")
        XCTAssertEqual(s.results, [])
    }
}

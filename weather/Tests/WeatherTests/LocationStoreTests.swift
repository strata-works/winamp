import XCTest
@testable import Weather

final class LocationStoreTests: XCTestCase {
    private var defaults: UserDefaults!
    private let suite = "wx.tests.\(UUID().uuidString)"

    override func setUp() {
        defaults = UserDefaults(suiteName: suite)!
    }
    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
    }

    func testLoadNilWhenEmpty() {
        XCTAssertNil(LocationStore(defaults: defaults).load())
    }

    func testSaveThenLoadRoundTrips() {
        let store = LocationStore(defaults: defaults)
        let loc = StoredLocation(latitude: -33.9258, longitude: 18.4232, name: "Cape Town")
        store.save(loc)
        XCTAssertEqual(store.load(), loc)
    }
}

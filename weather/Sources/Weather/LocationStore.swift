import Foundation

struct StoredLocation: Equatable {
    let latitude: Double
    let longitude: Double
    let name: String
}

/// Persists the last chosen location so it survives relaunch. Absent → the app keeps its
/// built-in default (Accra).
struct LocationStore {
    private let defaults: UserDefaults
    private let kLat = "wx.location.lat"
    private let kLon = "wx.location.lon"
    private let kName = "wx.location.name"

    init(defaults: UserDefaults = .standard) { self.defaults = defaults }

    func load() -> StoredLocation? {
        guard defaults.object(forKey: kLat) != nil,
              defaults.object(forKey: kLon) != nil,
              let name = defaults.string(forKey: kName) else { return nil }
        return StoredLocation(latitude: defaults.double(forKey: kLat),
                              longitude: defaults.double(forKey: kLon),
                              name: name)
    }

    func save(_ loc: StoredLocation) {
        defaults.set(loc.latitude, forKey: kLat)
        defaults.set(loc.longitude, forKey: kLon)
        defaults.set(loc.name, forKey: kName)
    }
}

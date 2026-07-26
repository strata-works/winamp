import Foundation

/// One geocoding hit. `label` is what the search overlay shows: "City, Admin1, Country"
/// with absent/empty parts dropped.
struct GeoResult: Equatable {
    let name: String
    let admin1: String?
    let country: String?
    let latitude: Double
    let longitude: Double

    var label: String {
        [name, admin1, country]
            .compactMap { $0 }
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
    }
}

/// Resolves a free-text query to candidate locations. `search` throws on transport/HTTP failure
/// (→ "Offline" in the UI) and returns `[]` for a valid empty response (→ "No matches").
protocol Geocoder {
    func search(_ query: String) async throws -> [GeoResult]
}

/// Open-Meteo geocoding (no API key; same provider as the forecast source).
struct OpenMeteoGeocoder: Geocoder {
    struct Response: Decodable {
        struct Row: Decodable {
            let name: String
            let latitude: Double
            let longitude: Double
            let country: String?
            let admin1: String?
        }
        let results: [Row]?
    }

    func url(_ query: String) -> URL {
        var c = URLComponents(string: "https://geocoding-api.open-meteo.com/v1/search")!
        c.queryItems = [
            .init(name: "name", value: query),
            .init(name: "count", value: "5"),
            .init(name: "language", value: "en"),
            .init(name: "format", value: "json"),
        ]
        return c.url!
    }

    static func decode(_ data: Data) throws -> [GeoResult] {
        let r = try JSONDecoder().decode(Response.self, from: data)
        return (r.results ?? []).map {
            GeoResult(name: $0.name, admin1: $0.admin1, country: $0.country,
                      latitude: $0.latitude, longitude: $0.longitude)
        }
    }

    func search(_ query: String) async throws -> [GeoResult] {
        let (data, resp) = try await URLSession.shared.data(from: url(query))
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
            throw NSError(domain: "Geocoder", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "non-200 response"])
        }
        return try Self.decode(data)
    }
}

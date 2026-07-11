import Foundation

/// Fetches Open-Meteo weather for a fixed location and maps it to `WeatherModel`.
/// Pure `map`/`decode` + static helpers are unit-testable; only `fetch()` hits the network.
struct WeatherService {
    let latitude: Double
    let longitude: Double
    let locationName: String

    init(latitude: Double = 5.55, longitude: Double = -0.20, locationName: String = "Accra") {
        self.latitude = latitude
        self.longitude = longitude
        self.locationName = locationName
    }

    // MARK: Open-Meteo response (property names match the JSON keys verbatim — do NOT use
    // convertFromSnakeCase; the digit in `temperature_2m` doesn't round-trip cleanly).
    struct Response: Decodable {
        struct Current: Decodable {
            let time: String
            let temperature_2m: Double
            let is_day: Int
            let weather_code: Int
            let apparent_temperature: Double
            let precipitation: Double
            let cloud_cover: Double
        }
        struct Hourly: Decodable {
            let time: [String]
            let temperature_2m: [Double]
            let weather_code: [Int]
        }
        struct Daily: Decodable {
            let time: [String]
            let weather_code: [Int]
            let temperature_2m_max: [Double]
            let temperature_2m_min: [Double]
        }
        let current: Current
        let hourly: Hourly
        let daily: Daily
    }

    // MARK: Request

    var url: URL {
        var c = URLComponents(string: "https://api.open-meteo.com/v1/forecast")!
        c.queryItems = [
            .init(name: "latitude", value: String(latitude)),
            .init(name: "longitude", value: String(longitude)),
            .init(name: "current", value: "temperature_2m,is_day,weather_code,apparent_temperature,precipitation,cloud_cover"),
            .init(name: "hourly", value: "temperature_2m,weather_code"),
            .init(name: "daily", value: "weather_code,temperature_2m_max,temperature_2m_min"),
            .init(name: "timezone", value: "auto"),
            .init(name: "forecast_days", value: "7"),
        ]
        return c.url!
    }

    // MARK: Decode + fetch

    func decode(_ data: Data) throws -> WeatherModel {
        let r = try JSONDecoder().decode(Response.self, from: data)
        return Self.map(r, locationName: locationName)
    }

    func loadBundledFixture() throws -> WeatherModel {
        guard let u = Bundle.module.url(forResource: "mock", withExtension: "json") else {
            throw NSError(domain: "WeatherService", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "bundled mock.json not found"])
        }
        return try decode(Data(contentsOf: u))
    }

    /// Always returns a valid model: live on success, bundled fixture on any failure, and the
    /// hardcoded `.sample` only if even the fixture is unreadable.
    func fetch() async -> WeatherModel {
        do {
            let (data, resp) = try await URLSession.shared.data(from: url)
            guard (resp as? HTTPURLResponse)?.statusCode == 200 else {
                throw NSError(domain: "WeatherService", code: 2,
                              userInfo: [NSLocalizedDescriptionKey: "non-200 response"])
            }
            return try decode(data)
        } catch {
            return (try? loadBundledFixture()) ?? .sample
        }
    }

    // MARK: Mapping (pure)

    static func map(_ r: Response, locationName: String) -> WeatherModel {
        let bucket = wmoBucket(r.current.weather_code)
        let month = monthOf(r.current.time)
        let hours = hourlyCells(r.hourly, currentTime: r.current.time)
        let days = dailyRows(r.daily)
        return WeatherModel(
            location: locationName,
            conditionText: conditionText(bucket: bucket),
            condition: bucket,
            isDay: Double(r.current.is_day),
            temp: r.current.temperature_2m,
            intensity: intensity(bucket: bucket,
                                 precipitation: r.current.precipitation,
                                 cloudCover: r.current.cloud_cover),
            season: seasonForMonth(month),
            tempNow: "\(round(r.current.temperature_2m))°",
            hiLo: "H:\(round(r.daily.temperature_2m_max.first ?? 0))° L:\(round(r.daily.temperature_2m_min.first ?? 0))°",
            feels: "Feels \(round(r.current.apparent_temperature))°",
            hours: hours,
            days: days
        )
    }

    static func hourlyCells(_ h: Response.Hourly, currentTime: String) -> [HourCell] {
        let n = min(h.time.count, h.temperature_2m.count)
        guard n > 0 else { return [] }
        // First hourly slot at/after the current hour, then clamp so a 12-wide window always fits.
        // The lexicographic compare is exact only because Open-Meteo emits identical offset-free,
        // minute-resolution ISO timestamps ("2026-07-11T12:00") for both current.time and hourly.time.
        let firstAtOrAfter = h.time.firstIndex(where: { $0 >= currentTime }) ?? 0
        let start = min(firstAtOrAfter, max(0, n - 12))
        let end = min(start + 12, n)
        return (start..<end).map { i in
            HourCell(time: "\(hourLabel(h.time[i]))h", temp: "\(round(h.temperature_2m[i]))°")
        }
    }

    static func dailyRows(_ d: Response.Daily) -> [DayRow] {
        let n = min(d.time.count, min(d.weather_code.count,
                     min(d.temperature_2m_max.count, d.temperature_2m_min.count)))
        return (0..<n).map { i in
            let b = wmoBucket(d.weather_code[i])
            return DayRow(day: weekdayAbbrev(d.time[i]),
                          glyph: glyph(bucket: b),
                          hi: "\(round(d.temperature_2m_max[i]))°",
                          lo: "\(round(d.temperature_2m_min[i]))°")
        }
    }

    // MARK: Static helpers (pure)

    static func wmoBucket(_ code: Int) -> Double {
        switch code {
        case 0, 1: return 0                       // clear
        case 2, 3: return 1                       // cloud
        case 45, 48: return 5                      // fog
        case 51...67, 80...82: return 2            // rain
        case 71...77, 85, 86: return 3             // snow
        case 95...99: return 4                     // storm
        default: return 1                          // safe default: cloud
        }
    }

    static func intensity(bucket: Double, precipitation: Double, cloudCover: Double) -> Double {
        switch bucket {
        case 2, 3, 4: return min(max(precipitation / 8.0, 0.15), 1.0)
        case 0:       return min(max(cloudCover / 100.0 * 0.3, 0.0), 0.3)
        default:      return min(max(cloudCover / 100.0, 0.0), 1.0)   // cloud/fog
        }
    }

    static func seasonForMonth(_ month: Int) -> Double {
        switch month {
        case 12, 1, 2: return 0    // winter
        case 3, 4, 5:  return 1    // spring
        case 6, 7, 8:  return 2    // summer
        default:       return 3    // autumn (9,10,11)
        }
    }

    static func conditionText(bucket: Double) -> String {
        switch Int(bucket) {
        case 0: return "Clear"
        case 1: return "Partly cloudy"
        case 2: return "Rain"
        case 3: return "Snow"
        case 4: return "Thunderstorm"
        case 5: return "Fog"
        default: return "Partly cloudy"
        }
    }

    static func glyph(bucket: Double) -> String {
        switch Int(bucket) {
        case 0: return "☀"
        case 1: return "⛅"
        case 2: return "☔"
        case 3: return "❄"
        case 4: return "⛈"
        case 5: return "🌫"
        default: return "⛅"
        }
    }

    /// "2026-07-11T14:00" -> 14 (the hour as an Int; drops leading zero).
    static func hourLabel(_ isoTime: String) -> Int {
        guard let tRange = isoTime.range(of: "T") else { return 0 }
        let after = isoTime[tRange.upperBound...]           // "14:00"
        let hh = after.prefix(2)                            // "14"
        return Int(hh) ?? 0
    }

    /// "2026-07-11" -> "Sat".
    static func weekdayAbbrev(_ isoDate: String) -> String {
        let inFmt = DateFormatter()
        inFmt.locale = Locale(identifier: "en_US_POSIX")
        inFmt.timeZone = TimeZone(identifier: "UTC")
        inFmt.dateFormat = "yyyy-MM-dd"
        guard let date = inFmt.date(from: isoDate) else { return "" }
        let outFmt = DateFormatter()
        outFmt.locale = Locale(identifier: "en_US_POSIX")
        outFmt.timeZone = TimeZone(identifier: "UTC")
        outFmt.dateFormat = "EEE"
        return outFmt.string(from: date)
    }

    /// "2026-07-11T12:00" -> 7 (the month as an Int).
    static func monthOf(_ isoTime: String) -> Int {
        // "2026-07-11..." -> chars 5..6 are the month.
        let s = Array(isoTime)
        guard s.count >= 7 else { return 1 }
        return Int(String(s[5...6])) ?? 1
    }

    private static func round(_ x: Double) -> Int { Int(x.rounded()) }
}

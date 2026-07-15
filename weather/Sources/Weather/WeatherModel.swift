import Foundation

struct HourCell {
    let time: String   // "13h"
    let temp: String   // "28°"
}

struct DayRow {
    let day: String    // "Mon"
    let glyph: String   // "☀"
    let hi: String     // "31°"
    let lo: String     // "24°"
}

/// The full weather state the skin renders. M1 uses `.sample`; M2 derives it from Open-Meteo.
struct WeatherModel {
    var location: String
    var conditionText: String
    var condition: Double   // 0 clear·1 cloud·2 rain·3 snow·4 storm·5 fog
    var temp: Double        // °C
    var intensity: Double   // 0..1
    var season: Double      // 0 winter·1 spring·2 summer·3 autumn
    var sunrise: Date       // today's sunrise (location-local instant)
    var sunset: Date        // today's sunset
    var tempNow: String     // "27°"
    var hiLo: String        // "H:31° L:24°"
    var feels: String       // "Feels 30°"
    var hours: [HourCell]   // 12 cells
    var days: [DayRow]      // 7 rows

    static let sample: WeatherModel = {
        let cal = Calendar.current
        let sr = cal.date(bySettingHour: 6, minute: 0, second: 0, of: Date()) ?? Date()
        let ss = cal.date(bySettingHour: 18, minute: 0, second: 0, of: Date()) ?? Date()
        return WeatherModel(
            location: "Accra",
            conditionText: "Partly cloudy",
            condition: 1, temp: 27, intensity: 0.4, season: 2,
            sunrise: sr, sunset: ss,
            tempNow: "27°", hiLo: "H:31° L:24°", feels: "Feels 30°",
            hours: (0..<12).map { i in
                HourCell(time: "\(12 + i)h", temp: "\(27 + (i % 4))°")
            },
            days: [
                DayRow(day: "Mon", glyph: "☀", hi: "31°", lo: "24°"),
                DayRow(day: "Tue", glyph: "⛅", hi: "30°", lo: "23°"),
                DayRow(day: "Wed", glyph: "☔", hi: "29°", lo: "24°"),
                DayRow(day: "Thu", glyph: "⛈", hi: "28°", lo: "23°"),
                DayRow(day: "Fri", glyph: "⛅", hi: "30°", lo: "24°"),
                DayRow(day: "Sat", glyph: "☀", hi: "32°", lo: "25°"),
                DayRow(day: "Sun", glyph: "☁", hi: "29°", lo: "23°"),
            ]
        )
    }()
}

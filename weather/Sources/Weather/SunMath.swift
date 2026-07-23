import Foundation

/// Continuous solar-elevation proxy in [-1, 1] from today's sunrise/sunset.
/// Piecewise-linear triangles: 1 at solar noon (day-arc midpoint), 0 at sunrise/sunset,
/// -1 at the night-arc midpoint. Dawn and dusk are symmetric (elevation only, no azimuth).
enum SunMath {
    static func sunElevation(now: Date, sunrise: Date, sunset: Date) -> Double {
        let dayLen = sunset.timeIntervalSince(sunrise)
        guard dayLen > 0, dayLen < 86_400 else { return 0 }   // degenerate/garbled -> horizon
        let nightLen = 86_400 - dayLen
        if now >= sunrise && now <= sunset {
            let f = now.timeIntervalSince(sunrise) / dayLen
            return 1 - abs(2 * f - 1)
        }
        // Night: position in the sunset -> next-sunrise arc, wrapping any date into [0, 86400).
        var ns = now.timeIntervalSince(sunset).truncatingRemainder(dividingBy: 86_400)
        if ns < 0 { ns += 86_400 }
        let g = min(ns / nightLen, 1)
        return -(1 - abs(2 * g - 1))
    }

    /// D-key presenter stops: dawn -> noon -> dusk -> night (then back to live).
    static let presenterStops: [Double] = [0.1, 1.0, -0.1, -1.0]
}

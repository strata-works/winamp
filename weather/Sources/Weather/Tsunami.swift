import Foundation

/// Tsunami demo-cycle coordination. The SHADER renders the 32 s arc from `wx_cond_age`;
/// the HOST computes the identical phase from its own clock and blanks the entire UI while
/// the window is engulfed. weather.wgsl `tsunami_phase`/engulf constants must stay in sync.
enum Tsunami {
    static let period: Double = 32
    static let engulfStart = 0.60   // phase fraction
    static let engulfEnd = 0.74

    static func phase(age: Double) -> Double {
        let m = age.truncatingRemainder(dividingBy: period)
        return (m < 0 ? m + period : m) / period
    }
    static func isEngulfed(age: Double) -> Bool {
        let p = phase(age: age)
        return p >= engulfStart && p < engulfEnd
    }
}

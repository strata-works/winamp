import Foundation

/// Snow-pile burial coordination. The SHADER grows an opaque snow mound from
/// `wx_cond_age`; the HOST hides daily rows the mound has lapped. Both sides evaluate the
/// same threshold so the row vanishes exactly as the mound covers its position.
/// weather.wgsl `pile_height` must stay in sync with `buryAgeLastRow`.
enum SnowPile {
    /// Age (seconds in snow) at which the pile laps the last daily row.
    static let buryAgeLastRow: Double = 135
    static func buriedRows(age: Double) -> Int { age >= buryAgeLastRow ? 1 : 0 }
}

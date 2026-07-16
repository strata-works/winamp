/// The presenter demo cycle over a shader override: `nil` (live) → 0 → 1 … → `upTo` → nil …
/// (`prev` reverses). Used for the condition (`upTo 7` — 0–5 live buckets + 6 winds +
/// 7 tsunami demo) and season (`upTo 3`) overrides. The 1-arg helpers are the condition cycle.
enum ConditionCycle {
    static func next(_ current: Double?, upTo max: Double) -> Double? {
        switch current {
        case .none: return 0
        case .some(let c) where c >= max: return nil
        case .some(let c): return c + 1
        }
    }

    static func prev(_ current: Double?, upTo max: Double) -> Double? {
        switch current {
        case .none: return max
        case .some(let c) where c <= 0: return nil
        case .some(let c): return c - 1
        }
    }

    static func next(_ current: Double?) -> Double? { next(current, upTo: 7) }
    static func prev(_ current: Double?) -> Double? { prev(current, upTo: 7) }

    /// Cycle through an explicit stops array: nil (live) -> stops[0] -> ... -> last -> nil.
    /// Used by the D key over `SunMath.presenterStops`. Matches on exact stored values.
    static func next(_ current: Double?, stops: [Double]) -> Double? {
        guard let c = current else { return stops.first }
        guard let i = stops.firstIndex(of: c), i + 1 < stops.count else { return nil }
        return stops[i + 1]
    }
}

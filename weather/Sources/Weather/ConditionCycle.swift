/// The presenter demo cycle over a shader override: `nil` (live) → 0 → 1 … → `upTo` → nil …
/// (`prev` reverses). Used for the condition (`upTo 5`), season (`upTo 3`), and day/night
/// (`upTo 1`) overrides. The 1-arg helpers are the condition cycle (`upTo 5`).
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

    static func next(_ current: Double?) -> Double? { next(current, upTo: 5) }
    static func prev(_ current: Double?) -> Double? { prev(current, upTo: 5) }
}

/// The presenter demo cycle over the shader condition override:
/// `nil` (live) → 0 → 1 → 2 → 3 → 4 → 5 → nil …  (`prev` reverses).
enum ConditionCycle {
    static func next(_ current: Double?) -> Double? {
        switch current {
        case .none: return 0
        case .some(let c) where c >= 5: return nil
        case .some(let c): return c + 1
        }
    }

    static func prev(_ current: Double?) -> Double? {
        switch current {
        case .none: return 5
        case .some(let c) where c <= 0: return nil
        case .some(let c): return c - 1
        }
    }
}

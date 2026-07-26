import Foundation

/// Pure, AppKit-free state for the location-search overlay. `AppDelegate` mutates a copy on the
/// main thread and publishes it into `WeatherHost` (lock-guarded) for the render thread to read.
struct LocationSearch: Equatable {
    enum Status: Equatable { case idle, querying, results, empty, error }

    private(set) var active: Bool = false
    private(set) var query: String = ""
    private(set) var results: [GeoResult] = []
    private(set) var selected: Int = 0
    private(set) var status: Status = .idle

    mutating func enter() {
        active = true; query = ""; results = []; selected = 0; status = .idle
    }

    mutating func exit() {
        active = false; query = ""; results = []; selected = 0; status = .idle
    }

    /// Append already-validated printable text (caller filters control keys).
    mutating func typed(_ s: String) {
        query += s
        status = query.isEmpty ? .idle : .querying
    }

    mutating func backspace() {
        if !query.isEmpty { query.removeLast() }
        if query.isEmpty { results = []; selected = 0; status = .idle }
        else { status = .querying }
    }

    mutating func moveSelection(_ delta: Int) {
        guard !results.isEmpty else { return }
        selected = min(max(selected + delta, 0), results.count - 1)
    }

    /// Apply geocoder output only if it still matches the current query (drops stale async replies).
    mutating func setResults(_ r: [GeoResult], forQuery q: String) {
        guard q == query else { return }
        results = r; selected = 0; status = r.isEmpty ? .empty : .results
    }

    mutating func setError(forQuery q: String) {
        guard q == query else { return }
        results = []; selected = 0; status = .error
    }

    /// The pick to commit, or nil when there is nothing selectable.
    func commit() -> GeoResult? {
        guard status == .results, selected >= 0, selected < results.count else { return nil }
        return results[selected]
    }

    /// Card body line shown when no rows are listed ("" while results are shown).
    var statusLine: String {
        switch status {
        case .idle:     return "Type a city…"
        case .querying: return "Searching…"
        case .empty:    return "No matches"
        case .error:    return "Offline — check connection"
        case .results:  return ""
        }
    }
}

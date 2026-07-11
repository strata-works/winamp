import Foundation

/// Answers the host-data contract (numeric shader uniforms, display strings, daily rows) from a
/// `WeatherModel`. Held by the HostCallbacks vtable; mutate `model` to change what the skin shows.
final class WeatherHost {
    private var _model: WeatherModel
    private var _conditionOverride: Double?
    private let lock = NSLock()
    init(model: WeatherModel) { self._model = model }

    /// The current weather state. Thread-safe: the engine reads via the host vtable on the
    /// RENDER thread (num/str/rowCount/rowString) while the app mutates from the MAIN thread
    /// (the live WeatherService refresh swaps the whole model). All access is lock-guarded,
    /// so a full-model swap is atomic w.r.t. the render thread's reads. The →/← demo cycle
    /// uses `conditionOverride` (below), not this, so it survives a refresh.
    var model: WeatherModel {
        get { lock.lock(); defer { lock.unlock() }; return _model }
        set { lock.lock(); _model = newValue; lock.unlock() }
    }

    /// Presenter demo override for the shader condition only. Set from the MAIN thread
    /// (the →/← keys); read from the RENDER thread in `num("wx_condition")`. Lock-guarded
    /// like `model`. `nil` = show the live condition.
    var conditionOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _conditionOverride }
        set { lock.lock(); _conditionOverride = newValue; lock.unlock() }
    }

    /// Parse the `i` out of "wx_hour_<i>_<suffix>", or nil.
    private func hourIndex(_ key: String, suffix: String) -> Int? {
        let prefix = "wx_hour_"
        guard key.hasPrefix(prefix), key.hasSuffix(suffix) else { return nil }
        let start = key.index(key.startIndex, offsetBy: prefix.count)
        let end = key.index(key.endIndex, offsetBy: -suffix.count)
        guard start <= end else { return nil }
        return Int(key[start..<end])
    }

    func num(_ key: String) -> Double? {
        switch key {
        case "wx_condition": return conditionOverride ?? model.condition
        case "wx_is_day":    return model.isDay
        case "wx_temp":      return model.temp
        case "wx_intensity": return model.intensity
        case "wx_season":    return model.season
        default:             return nil
        }
    }

    func str(_ key: String) -> String? {
        // Snapshot once so the count-check and the index read below see the SAME model — two
        // separate `model` reads could observe different snapshots (TOCTOU → out-of-bounds) once
        // M2 mutates array lengths from another thread.
        let m = model
        switch key {
        case "location":       return m.location
        case "condition_text": return m.conditionText
        case "temp_now":       return m.tempNow
        case "hi_lo":          return m.hiLo
        case "feels":          return m.feels
        default:
            if let i = hourIndex(key, suffix: "_time"), i >= 0, i < m.hours.count {
                return m.hours[i].time
            }
            if let i = hourIndex(key, suffix: "_temp"), i >= 0, i < m.hours.count {
                return m.hours[i].temp
            }
            return nil
        }
    }

    func rowCount() -> Int { model.days.count }

    func rowString(_ index: Int, field: String) -> String? {
        // Single snapshot so the bounds check and the index read below can't race (see `str`).
        let m = model
        guard index >= 0, index < m.days.count else { return nil }
        let d = m.days[index]
        switch field {
        case "day":   return d.day
        case "glyph": return d.glyph
        case "hi":    return d.hi
        case "lo":    return d.lo
        default:      return nil
        }
    }
}

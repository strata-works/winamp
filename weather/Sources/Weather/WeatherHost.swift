import Foundation

/// Answers the host-data contract (numeric shader uniforms, display strings, daily rows) from a
/// `WeatherModel`. Held by the HostCallbacks vtable; mutate `model` to change what the skin shows.
final class WeatherHost {
    private var _model: WeatherModel
    private var _conditionOverride: Double?
    private var _sunOverride: Double?
    private var _seasonOverride: Double?
    private var _condChangedAt = Date()
    private let lock = NSLock()
    init(model: WeatherModel) { self._model = model }

    /// Reset the condition-age clock when the EFFECTIVE condition value changes.
    /// Callers must hold `lock`.
    private func noteEffectiveCondition(from old: Double, to new: Double) {
        if old != new { _condChangedAt = Date() }
    }

    /// Seconds since the effective condition last changed. `now` injected for testability.
    func conditionAge(now: Date = Date()) -> Double {
        lock.lock(); defer { lock.unlock() }
        return now.timeIntervalSince(_condChangedAt)
    }

    /// Presenter/automation: pretend the current condition started `seconds` ago (WX_AGE env).
    /// Lets demos/verification jump straight to grown snow piles etc.
    func backdateConditionChange(seconds: Double) {
        lock.lock(); _condChangedAt = Date().addingTimeInterval(-seconds); lock.unlock()
    }

    /// The current weather state. Thread-safe: the engine reads via the host vtable on the
    /// RENDER thread (num/str/rowCount/rowString) while the app mutates from the MAIN thread
    /// (the live WeatherService refresh swaps the whole model). All access is lock-guarded,
    /// so a full-model swap is atomic w.r.t. the render thread's reads. The →/← demo cycle
    /// uses `conditionOverride` (below), not this, so it survives a refresh.
    var model: WeatherModel {
        get { lock.lock(); defer { lock.unlock() }; return _model }
        set {
            lock.lock()
            let old = _conditionOverride ?? _model.condition
            let new = _conditionOverride ?? newValue.condition
            noteEffectiveCondition(from: old, to: new)
            _model = newValue
            lock.unlock()
        }
    }

    /// Presenter demo override for the shader condition only. Set from the MAIN thread
    /// (the →/← keys); read from the RENDER thread in `num("wx_condition")`. Lock-guarded
    /// like `model`. `nil` = show the live condition.
    var conditionOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _conditionOverride }
        set {
            lock.lock()
            let old = _conditionOverride ?? _model.condition
            let new = newValue ?? _model.condition
            noteEffectiveCondition(from: old, to: new)
            _conditionOverride = newValue
            lock.unlock()
        }
    }

    /// Presenter override for the shader sun-elevation uniform only (the `D` key cycles
    /// dawn/noon/dusk/night). Lock-guarded like `model`; `nil` = live. Forces only `wx_sun`.
    var sunOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _sunOverride }
        set { lock.lock(); _sunOverride = newValue; lock.unlock() }
    }

    /// Presenter override for the shader season uniform only (the `S` key). Lock-guarded like
    /// `model`; `nil` = live. Forces only `wx_season`.
    var seasonOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _seasonOverride }
        set { lock.lock(); _seasonOverride = newValue; lock.unlock() }
    }

    /// Presenter/automation override for the shader intensity uniform only (WX_INT env).
    /// Lock-guarded like `model`; `nil` = live. Forces only `wx_intensity`.
    var intensityOverride: Double? {
        get { lock.lock(); defer { lock.unlock() }; return _intensityOverride }
        set { lock.lock(); _intensityOverride = newValue; lock.unlock() }
    }
    private var _intensityOverride: Double?

    /// True while the tsunami demo condition has the window engulfed — the whole UI drowns.
    private var uiDrowned: Bool {
        (conditionOverride ?? model.condition) == 7 && Tsunami.isEngulfed(age: conditionAge())
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
        case "wx_sun":
            // `Date()` on every read: the sky evolves continuously with zero timers.
            return sunOverride ?? SunMath.sunElevation(now: Date(), sunrise: model.sunrise, sunset: model.sunset)
        case "wx_temp":      return model.temp
        case "wx_intensity": return intensityOverride ?? model.intensity
        case "wx_season":    return seasonOverride ?? model.season
        case "wx_cond_age":  return conditionAge()
        default:             return nil
        }
    }

    func str(_ key: String) -> String? {
        if uiDrowned { return "" }   // empty strings skip rendering — the forecast is underwater
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

    /// Daily row count, minus any rows the snow pile has buried (snow condition only).
    /// The default `now` keeps the vtable call site (`rowCount()`) unchanged.
    func rowCount(now: Date = Date()) -> Int {
        if uiDrowned { return 0 }
        let m = model
        let cond = conditionOverride ?? m.condition
        let buried = cond == 3 ? SnowPile.buriedRows(age: conditionAge(now: now)) : 0
        return max(0, m.days.count - buried)
    }

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

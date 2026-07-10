/// Answers the host-data contract (numeric shader uniforms, display strings, daily rows) from a
/// `WeatherModel`. Held by the HostCallbacks vtable; mutate `model` to change what the skin shows.
final class WeatherHost {
    var model: WeatherModel
    init(model: WeatherModel) { self.model = model }

    /// Parse the `i` out of "wx_hour_<i>_<suffix>", or nil.
    private func hourIndex(_ key: String, suffix: String) -> Int? {
        let prefix = "wx_hour_"
        guard key.hasPrefix(prefix), key.hasSuffix(suffix) else { return nil }
        let start = key.index(key.startIndex, offsetBy: prefix.count)
        let end = key.index(key.endIndex, offsetBy: -suffix.count)
        return Int(key[start..<end])
    }

    func num(_ key: String) -> Double? {
        switch key {
        case "wx_condition": return model.condition
        case "wx_is_day":    return model.isDay
        case "wx_temp":      return model.temp
        case "wx_intensity": return model.intensity
        case "wx_season":    return model.season
        default:             return nil
        }
    }

    func str(_ key: String) -> String? {
        switch key {
        case "location":       return model.location
        case "condition_text": return model.conditionText
        case "temp_now":       return model.tempNow
        case "hi_lo":          return model.hiLo
        case "feels":          return model.feels
        default:
            if let i = hourIndex(key, suffix: "_time"), i >= 0, i < model.hours.count {
                return model.hours[i].time
            }
            if let i = hourIndex(key, suffix: "_temp"), i >= 0, i < model.hours.count {
                return model.hours[i].temp
            }
            return nil
        }
    }

    func rowCount() -> Int { model.days.count }

    func rowString(_ index: Int, field: String) -> String? {
        guard index >= 0, index < model.days.count else { return nil }
        let d = model.days[index]
        switch field {
        case "day":   return d.day
        case "glyph": return d.glyph
        case "hi":    return d.hi
        case "lo":    return d.lo
        default:      return nil
        }
    }
}

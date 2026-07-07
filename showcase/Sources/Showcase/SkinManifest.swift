import Foundation

/// Reads a skin's design canvas from its `skin.toml`. Deliberately tiny — scans for the two
/// integers in `canvas = { width = W, height = H }` rather than pulling in a TOML dependency.
enum SkinManifest {
    static func parseCanvas(fromTOML toml: String) -> (w: Int, h: Int)? {
        func intAfter(_ key: String) -> Int? {
            // match e.g. `width = 380` (any whitespace), taking the first occurrence.
            guard let r = toml.range(of: "\(key)\\s*=\\s*([0-9]+)", options: .regularExpression) else { return nil }
            let digits = toml[r].drop(while: { !$0.isNumber })
            return Int(digits)
        }
        guard let w = intAfter("width"), let h = intAfter("height") else { return nil }
        return (w, h)
    }

    static func canvas(atDir dir: String, fallback: (Int, Int)) -> (Int, Int) {
        let path = (dir as NSString).appendingPathComponent("skin.toml")
        guard let toml = try? String(contentsOfFile: path, encoding: .utf8),
              let c = parseCanvas(fromTOML: toml) else { return fallback }
        return (c.w, c.h)
    }
}

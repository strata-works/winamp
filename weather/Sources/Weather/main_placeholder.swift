import CCarapace

// Temporary: proves the package links carapace-ffi. Replaced by App.swift in Task 5.
@main
struct Placeholder {
    static func main() {
        var buf = [CChar](repeating: 0, count: 16)
        _ = carapace_last_error(&buf, UInt(buf.count)) // any exported symbol proves linkage
        print("weather: linked carapace-ffi")
    }
}

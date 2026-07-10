import Foundation
import AppKit
import CCarapace

/// Weak box so the top-level C callbacks (no captured context) can reach the live host.
final class HostBox { weak var host: WeatherHost? }
let hostBox = HostBox()

/// Weak box for the app's window, so `minimize`/`close` actions can reach it.
/// Task 5 sets `windowBox.window`.
final class WindowBox { weak var window: NSWindow? }
let windowBox = WindowBox()

private func writeCString(_ s: String, _ buf: UnsafeMutablePointer<CChar>, _ cap: UInt) -> Bool {
    guard cap > 0 else { return false }
    let bytes = Array(s.utf8)
    let n = min(bytes.count, Int(cap) - 1)
    for i in 0..<n { buf[i] = CChar(bitPattern: bytes[i]) }
    buf[n] = 0
    return true
}

func hostGetNum(_ ctx: UnsafeMutableRawPointer?, _ key: UnsafePointer<CChar>?, _ out: UnsafeMutablePointer<Double>?) -> Bool {
    guard let key = key, let out = out, let h = hostBox.host else { return false }
    guard let v = h.num(String(cString: key)) else { return false }
    out.pointee = v
    return true
}

func hostGetStr(_ ctx: UnsafeMutableRawPointer?, _ key: UnsafePointer<CChar>?, _ buf: UnsafeMutablePointer<CChar>?, _ cap: UInt) -> Bool {
    guard let key = key, let buf = buf, let h = hostBox.host else { return false }
    guard let v = h.str(String(cString: key)) else { return false }
    return writeCString(v, buf, cap)
}

func hostRowCount(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?) -> UInt32 {
    guard let col = col, let h = hostBox.host, String(cString: col) == "daily" else { return 0 }
    return UInt32(h.rowCount())
}

func hostGetRowStr(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?, _ index: UInt32, _ field: UnsafePointer<CChar>?, _ buf: UnsafeMutablePointer<CChar>?, _ cap: UInt) -> Bool {
    guard let col = col, let field = field, let buf = buf, let h = hostBox.host,
          String(cString: col) == "daily" else { return false }
    guard let v = h.rowString(Int(index), field: String(cString: field)) else { return false }
    return writeCString(v, buf, cap)
}

func hostGetRowNum(_ ctx: UnsafeMutableRawPointer?, _ col: UnsafePointer<CChar>?, _ index: UInt32, _ field: UnsafePointer<CChar>?, _ out: UnsafeMutablePointer<Double>?) -> Bool {
    // daily has no numeric fields today; string fields only.
    return false
}

func hostInvoke(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?) {
    guard let action = action else { return }
    switch String(cString: action) {
    case "minimize": DispatchQueue.main.async { windowBox.window?.miniaturize(nil) }
    case "close": DispatchQueue.main.async { NSApp.terminate(nil) }
    case "begin_drag": break // window drag is handled from the view's mouse events
    default: break
    }
}

func hostInvokeArg(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?, _ arg: Double) {}

/// Assemble the v3 vtable. `frame_ready` is supplied by the bridge (Task 4).
func makeVTable(frameReady: @escaping @convention(c) (UnsafeMutableRawPointer?, UInt32, UInt64) -> Void) -> CarapaceHostVTable {
    CarapaceHostVTable(
        ctx: nil,
        get_num: hostGetNum,
        get_str: hostGetStr,
        invoke: hostInvoke,
        frame_ready: frameReady,
        row_count: hostRowCount,
        get_row_str: hostGetRowStr,
        get_row_num: hostGetRowNum,
        invoke_arg: hostInvokeArg
    )
}

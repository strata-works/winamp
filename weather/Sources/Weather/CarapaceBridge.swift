import Foundation
import IOSurface
import CCarapace

/// Global frame sink so the C `frame_ready` callback (no captured context) can deliver frames.
/// Set by the single live CarapaceBridge. Fired on the render thread → we hop to main.
/// `surfaces` is written once in `init?` (main thread) and read here in `onFrameReady` (render
/// thread); this app has no live pool swap, but the lock is kept as cheap insurance against a
/// torn read of the array.
final class FrameSink {
    var onFrame: ((IOSurface, UInt32) -> Void)?
    var surfaces: [IOSurface] = []
}
let frameSink = FrameSink()

private func onFrameReady(_ ctx: UnsafeMutableRawPointer?, _ index: UInt32, _ frameID: UInt64) {
    // Runs on carapace's render thread. Must NOT call any carapace_* here. Hop to main to display.
    let idx = Int(index)
    objc_sync_enter(frameSink)
    let surface: IOSurface? = idx < frameSink.surfaces.count ? frameSink.surfaces[idx] : nil
    objc_sync_exit(frameSink)
    guard let surface else { return }
    DispatchQueue.main.async {
        frameSink.onFrame?(surface, index)
    }
}

final class CarapaceBridge {
    private var engine: OpaquePointer?
    private(set) var surfaces: [IOSurface]
    private(set) var width: Int
    private(set) var height: Int

    init?(skinDir: String, width: Int, height: Int,
          onFrame: @escaping (IOSurface, UInt32) -> Void) {
        self.width = width
        self.height = height
        // Pool of 3 BGRA IOSurfaces at surface pixel size.
        var pool: [IOSurface] = []
        for _ in 0..<3 {
            guard let s = IOSurface(properties: [
                .width: width, .height: height, .bytesPerElement: 4,
                .pixelFormat: 0x42475241 as UInt32, // 'BGRA'
            ]) else { return nil }
            pool.append(s)
        }
        self.surfaces = pool
        objc_sync_enter(frameSink)
        frameSink.surfaces = pool
        objc_sync_exit(frameSink)
        frameSink.onFrame = onFrame

        let vt = makeVTable(frameReady: onFrameReady)
        // `const IOSurfaceRef *` imports as `UnsafePointer<Unmanaged<IOSurfaceRef>?>` — IOSurfaceRef
        // is a CF type, so an array of them for a borrowed C pointer needs `Unmanaged`, not a plain
        // bridge cast.
        let refs: [Unmanaged<IOSurfaceRef>?] = pool.map { Unmanaged.passUnretained($0 as IOSurfaceRef) }
        let ok = refs.withUnsafeBufferPointer { buf -> Bool in
            skinDir.withCString { dir -> Bool in
                var desc = CarapaceCreateDesc(
                    skin_dir: dir,
                    vtable: vt,
                    surfaces: buf.baseAddress,
                    surface_count: UInt32(buf.count),
                    content_surface: nil,
                    w: UInt32(width), h: UInt32(height)
                )
                var out: OpaquePointer?
                let status = carapace_create(&desc, &out)
                if status == Ok, let e = out { self.engine = e; return true }
                return false
            }
        }
        if !ok {
            var msg = [CChar](repeating: 0, count: 256)
            _ = carapace_last_error(&msg, UInt(msg.count))
            print("[weather] carapace_create failed: \(String(cString: msg))")
            return nil
        }
    }

    func pointer(x: Double, y: Double) {
        guard let e = engine else { return }
        _ = carapace_pointer(e, x, y, Press) // Press = 0
    }

    func hitTest(x: Double, y: Double) -> CarapaceHitKind {
        guard let e = engine else { return Passthrough }
        var kind = Passthrough
        _ = carapace_hit_test(e, x, y, &kind)
        return kind
    }

    func releaseSurface(_ index: UInt32) {
        guard let e = engine else { return }
        _ = carapace_release_surface(e, index)
    }

    deinit {
        if let e = engine { carapace_destroy(e) }
    }
}

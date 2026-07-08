import Foundation
import IOSurface
import CCarapace

/// Global frame sink so the C `frame_ready` callback (no captured context) can deliver frames.
/// Set by the single live CarapaceBridge. Fired on the render thread → we hop to main.
/// `surfaces` is read here (render thread) and written from `swapResized` (main thread), so both
/// sides take the same lock to avoid a torn read of the array during a live pool swap.
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

    init?(skinDir: String, width: Int, height: Int, contentSurface: IOSurface?,
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
                    content_surface: contentSurface.map { Unmanaged.passUnretained($0 as IOSurfaceRef) } ?? nil,
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
            print("[showcase] carapace_create failed: \(String(cString: msg))")
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

    func swap(skinDir: String) -> Bool {
        guard let e = engine else { return false }
        return skinDir.withCString { carapace_swap_skin(e, $0) } == Ok
    }

    /// Live-swap to `skinDir` AND adopt a new pool at `width`×`height` (the incoming skin's native
    /// size). Returns true on success; on failure the current skin+pool are unchanged.
    func swapResized(skinDir: String, width: Int, height: Int, contentSurface: IOSurface?) -> Bool {
        guard let e = engine else { return false }
        var pool: [IOSurface] = []
        for _ in 0..<3 {
            guard let s = IOSurface(properties: [
                .width: width, .height: height, .bytesPerElement: 4,
                .pixelFormat: 0x42475241 as UInt32,
            ]) else { return false }
            pool.append(s)
        }
        // `surfaces`/`content_surface` here are `const void *const *`/`const void *` (not
        // `IOSurfaceRef`-typed like `carapace_create`'s desc), so they import as raw pointers —
        // unwrap each IOSurfaceRef to its opaque pointer rather than an `Unmanaged` wrapper.
        let refs: [UnsafeRawPointer?] = pool.map {
            UnsafeRawPointer(Unmanaged.passUnretained($0 as IOSurfaceRef).toOpaque())
        }
        let content = contentSurface.map {
            UnsafeRawPointer(Unmanaged.passUnretained($0 as IOSurfaceRef).toOpaque())
        }
        let ok = refs.withUnsafeBufferPointer { buf -> Bool in
            skinDir.withCString { dir -> Bool in
                carapace_swap_skin_resized(e, dir, buf.baseAddress, UInt32(buf.count),
                                           UInt32(width), UInt32(height), content) == Ok
            }
        }
        guard ok else { return false }
        // The C call blocked until the render thread switched pools, so no old-pool frame will fire
        // after this. Adopt the new pool for future frame_ready lookups.
        self.surfaces = pool
        self.width = width
        self.height = height
        objc_sync_enter(frameSink)
        frameSink.surfaces = pool
        objc_sync_exit(frameSink)
        return true
    }

    func releaseSurface(_ index: UInt32) {
        guard let e = engine else { return }
        _ = carapace_release_surface(e, index)
    }

    deinit {
        if let e = engine { carapace_destroy(e) }
    }
}

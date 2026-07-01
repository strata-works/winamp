import Flutter
import UIKit
import IOSurface
import CoreVideo

/// Bridges the carapace engine to a Flutter external texture.
///
/// Owns an IOSurface-backed BGRA8 `CVPixelBuffer`, hands its `IOSurfaceRef` to `carapace_create`
/// as the engine's render target (Tier-1: the engine CPU-copies each frame into the surface), and
/// serves that same `CVPixelBuffer` back to Flutter via `FlutterTexture.copyPixelBuffer`. A
/// `CADisplayLink` drives `carapace_tick` + `textureFrameAvailable`. This object IS the carapace
/// Host: it owns `level` and the `toggle` action the spike skin binds — the engine knows no Swift.
final class CarapaceBridge: NSObject, FlutterTexture {
    private var engine: OpaquePointer?            // CarapaceEngine*
    private var pixelBuffer: CVPixelBuffer?
    private let width: Int
    private let height: Int
    private var link: CADisplayLink?
    private weak var registry: FlutterTextureRegistry?
    private var textureId: Int64 = 0

    // Host state the door skin binds: number "lit" (0/1) lights the wall torch; "toggle" flips it.
    private(set) var lit: Double = 1

    init?(width: Int, height: Int, skinDir: String, registry: FlutterTextureRegistry) {
        self.width = width
        self.height = height
        self.registry = registry
        super.init()

        // 1. IOSurface-backed BGRA8 pixel buffer — the shared surface between engine and Flutter.
        let attrs: [CFString: Any] = [
            kCVPixelBufferIOSurfacePropertiesKey: [:],
            kCVPixelBufferPixelFormatTypeKey: Int(kCVPixelFormatType_32BGRA),
            kCVPixelBufferWidthKey: width,
            kCVPixelBufferHeightKey: height,
        ]
        var pb: CVPixelBuffer?
        guard CVPixelBufferCreate(kCFAllocatorDefault, width, height,
                                  kCVPixelFormatType_32BGRA, attrs as CFDictionary, &pb) == kCVReturnSuccess,
              let pb = pb, let io = CVPixelBufferGetIOSurface(pb)?.takeUnretainedValue() else {
            NSLog("[carapace] CVPixelBuffer/IOSurface creation failed")
            return nil
        }
        self.pixelBuffer = pb

        // 2. Host vtable — non-capturing C callbacks that route through `ctx` (this object).
        let vtable = CarapaceHostVTable(
            ctx: Unmanaged.passUnretained(self).toOpaque(),
            get_num: { ctx, key, out in
                guard let ctx = ctx, let key = key, let out = out else { return false }
                let me = Unmanaged<CarapaceBridge>.fromOpaque(ctx).takeUnretainedValue()
                if String(cString: key) == "lit" { out.pointee = me.lit; return true }
                return false
            },
            get_str: nil,
            invoke: { ctx, action in
                guard let ctx = ctx, let action = action else { return }
                let me = Unmanaged<CarapaceBridge>.fromOpaque(ctx).takeUnretainedValue()
                if String(cString: action) == "toggle" {
                    me.lit = me.lit > 0.5 ? 0 : 1   // extinguish / relight the torch
                }
            })

        // 3. Create the engine targeting the IOSurface. No host-content surface.
        guard let e = skinDir.withCString({ carapace_create($0, vtable, io, nil,
                                                             UInt32(width), UInt32(height)) }) else {
            NSLog("[carapace] carapace_create returned NULL")
            return nil
        }
        engine = e
        NSLog("[carapace] engine created, active tier = \(carapace_active_tier(e))")
    }

    // MARK: FlutterTexture
    func copyPixelBuffer() -> Unmanaged<CVPixelBuffer>? {
        guard let pb = pixelBuffer else { return nil }
        return Unmanaged.passRetained(pb)
    }

    // MARK: on-demand render loop
    // Tier-1 render+readback is synchronous on the main thread, so rendering EVERY frame at this
    // surface size starves Flutter (the first frame never draws). This skin is static except on a
    // tap, so we render on demand: mark `needsRender` at init + after each pointer event, and the
    // CADisplayLink only does the expensive tick when the flag is set.
    private var needsRender = true

    func start(textureId: Int64) {
        self.textureId = textureId
        let l = CADisplayLink(target: self, selector: #selector(onFrame(_:)))
        l.add(to: .main, forMode: .common)
        link = l
    }

    @objc private func onFrame(_ l: CADisplayLink) {
        guard needsRender, let e = engine else { return }
        needsRender = false
        carapace_tick(e, 1.0 / 60.0)
        registry?.textureFrameAvailable(textureId)
    }

    // Host-driven state: Flutter pushes the torch flame level (0..1) in time with the music.
    func setLit(_ v: Double) {
        lit = max(0, min(1, v))
        needsRender = true
    }

    // MARK: input — canvas coords (mapping done in Dart). kind 0 = press.
    func pointer(x: Double, y: Double) {
        guard let e = engine else { return }
        carapace_pointer(e, x, y, 0)   // may flip host state via the skin's region hit-test
        needsRender = true             // re-render to reflect it
    }

    deinit {
        link?.invalidate()
        if let e = engine { carapace_destroy(e) }
    }
}

import Foundation
import Metal
import IOSurface

/// Renders paper.design's dithering effect (Bayer ordered-dither between two colors over an
/// animated field, music-reactive via `level`) into a BGRA IOSurface the engine composites into
/// a `view{ id="host" }` cutout. One job: own the pipeline + surface, draw a frame on demand.
final class DitherRenderer {
    let surface: IOSurface
    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let pipeline: MTLRenderPipelineState
    private let texture: MTLTexture
    // Studio's viz-panel opening (logical px) — the cutout the content is stretched into.
    private let cutoutW: Float = 474
    private let cutoutH: Float = 214

    init?(width: Int, height: Int) {
        guard let dev = MTLCreateSystemDefaultDevice(),
              let q = dev.makeCommandQueue(),
              let s = IOSurface(properties: [
                  .width: width, .height: height, .bytesPerElement: 4,
                  .pixelFormat: 0x42475241 as UInt32,   // 'BGRA'
              ]) else { return nil }
        self.device = dev; self.queue = q; self.surface = s

        // BGRA texture aliasing the IOSurface (zero-copy).
        let td = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm, width: width, height: height, mipmapped: false)
        td.usage = [.renderTarget, .shaderRead]
        td.storageMode = .shared
        guard let tex = dev.makeTexture(descriptor: td, iosurface: s, plane: 0) else { return nil }
        self.texture = tex

        guard let lib = try? dev.makeLibrary(source: DitherRenderer.shaderSource, options: nil),
              let vfn = lib.makeFunction(name: "dither_vs"),
              let ffn = lib.makeFunction(name: "dither_fs") else { return nil }
        let pd = MTLRenderPipelineDescriptor()
        pd.vertexFunction = vfn
        pd.fragmentFunction = ffn
        pd.colorAttachments[0].pixelFormat = .bgra8Unorm
        guard let ps = try? dev.makeRenderPipelineState(descriptor: pd) else { return nil }
        self.pipeline = ps
    }

    func render(time: Float, level: Float) {
        var u = makeDitherUniforms(time: time, level: level, width: cutoutW, height: cutoutH)
        let rp = MTLRenderPassDescriptor()
        rp.colorAttachments[0].texture = texture
        rp.colorAttachments[0].loadAction = .clear
        rp.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        rp.colorAttachments[0].storeAction = .store
        guard let cb = queue.makeCommandBuffer(),
              let enc = cb.makeRenderCommandEncoder(descriptor: rp) else { return }
        enc.setRenderPipelineState(pipeline)
        withUnsafeBytes(of: &u) { enc.setFragmentBytes($0.baseAddress!, length: $0.count, index: 0) }
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        enc.endEncoding()
        cb.commit()
    }

    private static let shaderSource = """
    #include <metal_stdlib>
    using namespace metal;

    // Field order matches DitherUniforms (Task 2): float4s first (16-byte aligned), then float2,
    // then scalars. Metal rounds the struct to 64 bytes, matching the Swift value exactly.
    struct Uniforms {
        float4 colorBack;    // 0
        float4 colorFront;   // 16
        float2 resolution;   // 32
        float time;          // 40
        float level;         // 44
        float pxSize;        // 48
    };

    struct VOut { float4 pos [[position]]; float2 uv; };

    vertex VOut dither_vs(uint vid [[vertex_id]]) {
        float2 p[3] = { float2(-1,-1), float2(3,-1), float2(-1,3) };
        VOut o;
        o.pos = float4(p[vid], 0, 1);
        o.uv = p[vid] * 0.5 + 0.5;   // 0..1
        return o;
    }

    // Normalized 8x8 Bayer ordered-dither matrix.
    constant float bayer[64] = {
         0.5/64,32.5/64, 8.5/64,40.5/64, 2.5/64,34.5/64,10.5/64,42.5/64,
        48.5/64,16.5/64,56.5/64,24.5/64,50.5/64,18.5/64,58.5/64,26.5/64,
        12.5/64,44.5/64, 4.5/64,36.5/64,14.5/64,46.5/64, 6.5/64,38.5/64,
        60.5/64,28.5/64,52.5/64,20.5/64,62.5/64,30.5/64,54.5/64,22.5/64,
         3.5/64,35.5/64,11.5/64,43.5/64, 1.5/64,33.5/64, 9.5/64,41.5/64,
        51.5/64,19.5/64,59.5/64,27.5/64,49.5/64,17.5/64,57.5/64,25.5/64,
        15.5/64,47.5/64, 7.5/64,39.5/64,13.5/64,45.5/64, 5.5/64,37.5/64,
        63.5/64,31.5/64,55.5/64,23.5/64,61.5/64,29.5/64,53.5/64,21.5/64
    };

    fragment float4 dither_fs(VOut in [[stage_in]], constant Uniforms& u [[buffer(0)]]) {
        float2 uv = in.uv;
        // Animated field: a warped diagonal sweep. Amplitude/contrast rise with the audio level.
        float warp = 0.15 * sin(uv.y * 6.2831 + u.time * 0.6);
        float field = 0.5 + 0.5 * sin((uv.x + warp) * 6.2831 * 1.5 - u.time * 0.9);
        float coverage = clamp(field * (0.45 + 1.0 * u.level), 0.0, 1.0);
        // Bayer threshold at this pixel (cutout-space pixels ⇒ square cells after the stretch).
        float2 px = uv * u.resolution;
        int2 cell = int2(px / max(u.pxSize, 1.0));
        int bi = (cell.y & 7) * 8 + (cell.x & 7);
        float on = step(bayer[bi], coverage);
        float4 c = mix(u.colorBack, u.colorFront, on);
        c.rgb *= (0.75 + 0.5 * u.level);   // front brightens with level
        return c;
    }
    """
}

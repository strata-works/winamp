import Foundation

/// Fixed-layout mirror of the MSL `Uniforms` struct (Task 3). Field ORDER matters: the two float4
/// colors come first (16-byte aligned), then the float2 resolution (8-byte), then the scalars, then
/// trailing pad to a clean 64 bytes — this avoids Metal's vec3/vec4 mid-struct alignment surprises so
/// the Swift value uploads byte-for-byte. Plain Floats (no SIMD) keep the layout explicit + testable.
struct DitherUniforms {
    var backR: Float; var backG: Float; var backB: Float; var backA: Float   // 0..15
    var frontR: Float; var frontG: Float; var frontB: Float; var frontA: Float // 16..31
    var resX: Float; var resY: Float   // 32..39 (matches MSL float2 resolution)
    var time: Float                    // 40
    var level: Float                   // 44
    var pxSize: Float                  // 48
    var _pad0: Float = 0; var _pad1: Float = 0; var _pad2: Float = 0  // 52..63 → total 64
}

/// Studio-palette dither uniforms for a given cutout size, clamping `level` to 0...1.
func makeDitherUniforms(time: Float, level: Float, width: Float, height: Float,
                        front: (Float, Float, Float) = (77.0/255, 160.0/255, 240.0/255)) -> DitherUniforms {
    let l = max(0, min(1, level))
    return DitherUniforms(
        backR: 0.02, backG: 0.03, backB: 0.05, backA: 1,
        frontR: front.0, frontG: front.1, frontB: front.2, frontA: 1,
        resX: width, resY: height, time: time, level: l, pxSize: 3)
}

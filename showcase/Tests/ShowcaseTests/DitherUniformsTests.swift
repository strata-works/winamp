import Testing
@testable import Showcase

@Test func make_uniforms_sets_resolution_level_and_studio_colors() {
    let u = makeDitherUniforms(time: 1.5, level: 2.0, width: 474, height: 214)
    #expect(u.time == 1.5)
    #expect(u.level == 1.0)                 // clamped to 1
    #expect(u.resX == 474 && u.resY == 214) // cutout size, not full canvas
    #expect(u.pxSize == 3)
    // Studio blue front (77,160,240)/255, opaque
    #expect(abs(u.frontR - 77.0/255) < 1e-5)
    #expect(abs(u.frontG - 160.0/255) < 1e-5)
    #expect(abs(u.frontB - 240.0/255) < 1e-5)
    #expect(u.frontA == 1)
    // near-black back
    #expect(u.backR < 0.05 && u.backA == 1)
}

@Test func make_uniforms_clamps_negative_level() {
    #expect(makeDitherUniforms(time: 0, level: -1, width: 1, height: 1).level == 0)
}

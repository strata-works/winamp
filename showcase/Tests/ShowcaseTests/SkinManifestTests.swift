import Testing
@testable import Showcase

@Test func parses_canvas_width_height() {
    let toml = """
    schema = 1
    id = "x"
    canvas = { width = 380, height = 560 }
    entry = "skin.lua"
    """
    let c = SkinManifest.parseCanvas(fromTOML: toml)
    #expect(c?.w == 380)
    #expect(c?.h == 560)
}

@Test func malformed_returns_nil() {
    #expect(SkinManifest.parseCanvas(fromTOML: "id = \"x\"") == nil)
    #expect(SkinManifest.parseCanvas(fromTOML: "canvas = { width = 380 }") == nil) // missing height
}

@Test func canvas_atDir_falls_back_when_missing() {
    let c = SkinManifest.canvas(atDir: "/no/such/dir", fallback: (420, 660))
    #expect(c == (420, 660))
}

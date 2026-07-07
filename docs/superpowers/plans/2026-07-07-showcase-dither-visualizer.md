# Studio Dither Visualizer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Studio Deck's fake viz bars with a live, music-reactive dither field — paper.design's dithering effect rendered by the Swift host (Metal) into an IOSurface the engine composites into a `view{}` cutout, driven by real AVAudioPlayer metering.

**Architecture:** `RealAudioPlayer` gains real metering (`level`). A new `DitherRenderer` (Metal) renders a Bayer ordered-dither shader into a full-canvas BGRA IOSurface each frame (main-thread `Timer`, ~60fps), driven by wall-clock time + level. `CarapaceBridge` passes that IOSurface as `content_surface`; the engine composites it into `view{ id="host" }` declared over Studio's viz panel. Gated to Studio only. Zero engine changes.

**Tech Stack:** Swift 5 / AppKit, Metal (runtime-compiled MSL), IOSurface, AVFoundation metering, carapace-ffi (unchanged), swift-testing.

## Global Constraints

- **Zero `crates/` changes** — reuses `view{}` + `content_surface` + alpha compositing already on `main`.
- All work under `showcase/`.
- Commit identity: `Daniel Agbemava <danagbemava@gmail.com>`; no "Generated with Claude Code" footer.
- Tests are swift-testing (`import Testing` / `@Test` / `#expect`); run `cd showcase && swift test`.
- `content_surface` **must** match the main surface pixel size (`carapace.h:267-275`): Studio is 720×480 logical × backing scale (2 ⇒ 1440×960).
- The compositor samples the whole content texture (UV 0→1) into the cutout dest, so the shader takes the **cutout** resolution as a uniform for correct (undistorted) Bayer cells.
- Dither is **Studio-only**; Faceplate/Cassette pass `content_surface = nil` and run no dither loop.
- macOS 13 deployment floor (no CADisplayLink-on-NSView; use `Timer`).

---

### Task 1: RealAudioPlayer metering + level normalization

**Files:**
- Modify: `showcase/Sources/Showcase/RealAudioPlayer.swift`
- Modify: `showcase/Sources/Showcase/MusicHost.swift`
- Test: `showcase/Tests/ShowcaseTests/AudioLevelTests.swift` (create)

**Interfaces:**
- Consumes: existing `RealAudioPlayer`, `AudioPlayer` protocol, `MusicHost`.
- Produces:
  - free function `normalizeDB(_ db: Float) -> Float` (maps −60→0, 0→1, clamps).
  - `AudioPlayer.level: Float { get }` (protocol requirement; 0…1 smoothed).
  - `MusicHost.level() -> Double` (pass-through of `player.level`).

- [ ] **Step 1: Write the failing tests**

Create `showcase/Tests/ShowcaseTests/AudioLevelTests.swift`:

```swift
import Testing
import Foundation
@testable import Showcase

@Test func normalize_db_maps_range_and_clamps() {
    #expect(normalizeDB(0) == 1.0)
    #expect(normalizeDB(-60) == 0.0)
    #expect(abs(normalizeDB(-30) - 0.5) < 1e-6)
    #expect(normalizeDB(10) == 1.0)     // clamps above 0 dB
    #expect(normalizeDB(-120) == 0.0)   // clamps below floor
}

@Test func fake_player_level_defaults_to_zero() {
    // FakeAudioPlayer (test double) must satisfy the new protocol requirement.
    let p = FakeAudioPlayer()
    #expect(p.level == 0.0)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd showcase && swift test 2>&1 | tail -20`
Expected: compile error — `normalizeDB` not found, `FakeAudioPlayer` has no `level`.

- [ ] **Step 3: Implement metering + normalization**

In `showcase/Sources/Showcase/RealAudioPlayer.swift`, add the free function above the class and wire metering:

```swift
/// Map an AVAudioPlayer average-power reading (dBFS, ~-60...0) to 0...1, clamped.
func normalizeDB(_ db: Float) -> Float {
    let floorDB: Float = -60
    return max(0, min(1, (db - floorDB) / (0 - floorDB)))
}
```

Add to the `AudioPlayer` protocol in `MusicHost.swift` (a new read-only requirement):

```swift
    var level: Float { get }
```

Implement it in `RealAudioPlayer` (enable metering on load; smooth with an EMA):

```swift
    private var smoothedLevel: Float = 0
    var level: Float {
        guard let p = player, p.isPlaying else { smoothedLevel *= 0.85; return smoothedLevel }
        p.updateMeters()
        let target = normalizeDB(p.averagePower(forChannel: 0))
        smoothedLevel += (target - smoothedLevel) * 0.35   // EMA toward target
        return smoothedLevel
    }
```

And enable metering in `load(_:duration:)` (after `prepareToPlay()`):

```swift
        player?.isMeteringEnabled = true
```

In `MusicHost.swift`, add a reader in the `// MARK: readers` section:

```swift
    func level() -> Double { Double(player.level) }
```

- [ ] **Step 4: Update the test double**

In `showcase/Tests/ShowcaseTests/MusicHostTests.swift`, add `level` to `FakeAudioPlayer`:

```swift
    var level: Float = 0
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd showcase && swift test 2>&1 | tail -20`
Expected: all pass (existing + 2 new).

- [ ] **Step 6: Commit**

```bash
git add showcase/Sources/Showcase/RealAudioPlayer.swift showcase/Sources/Showcase/MusicHost.swift \
        showcase/Tests/ShowcaseTests/AudioLevelTests.swift showcase/Tests/ShowcaseTests/MusicHostTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(showcase): real audio metering + level normalization"
```

---

### Task 2: DitherUniforms + mapping

**Files:**
- Create: `showcase/Sources/Showcase/DitherUniforms.swift`
- Test: `showcase/Tests/ShowcaseTests/DitherUniformsTests.swift`

**Interfaces:**
- Produces:
  - `struct DitherUniforms` — a fixed-layout value matching the MSL `Uniforms` (Task 3): `time: Float`, `level: Float`, `resolution: (Float, Float)` as two `Float`s, `pxSize: Float`, `colorBack: (Float,Float,Float,Float)`, `colorFront: (Float,Float,Float,Float)`.
  - `static func makeDitherUniforms(time: Float, level: Float, width: Float, height: Float) -> DitherUniforms` — sets Studio colors + `pxSize = 3`, clamps level to 0…1.

**Note:** keep this a plain `struct` of `Float`s in declared order (no SIMD) so the memory layout is explicit and matches the MSL struct byte-for-byte; the renderer copies it with `withUnsafeBytes`.

- [ ] **Step 1: Write the failing test**

Create `showcase/Tests/ShowcaseTests/DitherUniformsTests.swift`:

```swift
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd showcase && swift test 2>&1 | tail -20`
Expected: compile error — `makeDitherUniforms`/`DitherUniforms` not found.

- [ ] **Step 3: Implement**

Create `showcase/Sources/Showcase/DitherUniforms.swift`:

```swift
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
func makeDitherUniforms(time: Float, level: Float, width: Float, height: Float) -> DitherUniforms {
    let l = max(0, min(1, level))
    return DitherUniforms(
        backR: 0.02, backG: 0.03, backB: 0.05, backA: 1,
        frontR: 77.0/255, frontG: 160.0/255, frontB: 240.0/255, frontA: 1,
        resX: width, resY: height, time: time, level: l, pxSize: 3)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd showcase && swift test 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/DitherUniforms.swift showcase/Tests/ShowcaseTests/DitherUniformsTests.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(showcase): DitherUniforms + Studio-palette mapping"
```

---

### Task 3: dither.metal shader + DitherRenderer

**Files:**
- Create: `showcase/Sources/Showcase/DitherRenderer.swift`

**Interfaces:**
- Consumes: `DitherUniforms`, `makeDitherUniforms` (Task 2).
- Produces:
  - `final class DitherRenderer` with `init?(width: Int, height: Int)` (surface pixel size = full canvas), `let surface: IOSurface`, and `func render(time: Float, level: Float)`.
  - The `render` call packs uniforms with the **cutout** size (474×214) and draws one dither frame into `surface`'s aliased Metal texture.

**Note:** no unit test (GPU render). The gate is a clean build; visual behavior is verified in Task 5. The MSL is compiled at runtime from an embedded string (avoids SPM metallib build setup).

- [ ] **Step 1: Create the renderer + embedded shader**

Create `showcase/Sources/Showcase/DitherRenderer.swift`:

```swift
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
```

- [ ] **Step 2: Build**

Run: `cd /Users/nexus/projects/experiments/winamp/showcase && swift build 2>&1 | tail -20`
Expected: builds clean (the renderer is unused until Task 5).

- [ ] **Step 3: Commit**

```bash
git add showcase/Sources/Showcase/DitherRenderer.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(showcase): DitherRenderer — Metal Bayer dither into an IOSurface"
```

---

### Task 4: CarapaceBridge content_surface parameter

**Files:**
- Modify: `showcase/Sources/Showcase/CarapaceBridge.swift`

**Interfaces:**
- Consumes: nothing new.
- Produces: `CarapaceBridge.init?(skinDir:width:height:contentSurface:onFrame:)` — new `contentSurface: IOSurface?` param, passed into `CarapaceCreateDesc.content_surface`.

- [ ] **Step 1: Add the parameter**

In `showcase/Sources/Showcase/CarapaceBridge.swift`, change the initializer signature (line 26):

```swift
    init?(skinDir: String, width: Int, height: Int, contentSurface: IOSurface?,
          onFrame: @escaping (IOSurface, UInt32) -> Void) {
```

And replace `content_surface: nil,` (line 54) with:

```swift
                    content_surface: contentSurface as IOSurfaceRef?,
```

(If the compiler rejects the cast because the field imports as `Unmanaged<IOSurfaceRef>?`, use `contentSurface.map { Unmanaged.passUnretained($0 as IOSurfaceRef) } ?? nil` instead — the field is a borrowed ref for the call, same pattern as the `surfaces` array on line 46.)

- [ ] **Step 2: Update the one existing caller so it compiles**

In `showcase/Sources/Showcase/App.swift`, the `CarapaceBridge(...)` call in `applySkin` must pass the new argument. For now pass `nil` (Task 5 makes it conditional):

```swift
        guard let b = CarapaceBridge(skinDir: dir, width: w * scale, height: h * scale,
                                     contentSurface: nil,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
```

- [ ] **Step 3: Build**

Run: `cd /Users/nexus/projects/experiments/winamp/showcase && swift build 2>&1 | tail -20`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add showcase/Sources/Showcase/CarapaceBridge.swift showcase/Sources/Showcase/App.swift
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(showcase): CarapaceBridge accepts an optional content_surface"
```

---

### Task 5: Wire the Studio-gated dither loop + skin cutout

**Files:**
- Modify: `showcase/skins/studio/skin.lua`
- Modify: `showcase/Sources/Showcase/App.swift`
- Modify: `showcase/README.md`

**Interfaces:**
- Consumes: `DitherRenderer` (Task 3), `CarapaceBridge(...contentSurface:...)` (Task 4), `MusicHost.level()` (Task 1).

- [ ] **Step 1: Swap Studio's viz bars for a `view{}` cutout**

In `showcase/skins/studio/skin.lua`, delete the 8 `value_fill{ value="viz_N" ... }` lines (the `viz_0`…`viz_7` block) and replace them with a single cutout over the baked viz panel:

```lua
-- live music-reactive dither field (host-rendered) fills the baked viz-glass opening
view{ id="host", x=20, y=74, w=474, h=214 }
```

- [ ] **Step 2: Add the dither loop to AppDelegate (gated to Studio)**

In `showcase/Sources/Showcase/App.swift`, add stored properties to `AppDelegate`:

```swift
    private var dither: DitherRenderer?
    private var ditherTimer: Timer?
    private var ditherStart: TimeInterval = 0
```

Add these methods:

```swift
    /// Start/stop the dither loop for the given skin. Only Studio declares a view{ id="host" }
    /// cutout, so we render (and pay GPU) only there; returns the content surface to hand the
    /// bridge (nil for other skins).
    private func ditherSurface(forDir dir: String, width: Int, height: Int) -> IOSurface? {
        stopDither()
        guard (dir as NSString).lastPathComponent == "studio",
              let r = DitherRenderer(width: width, height: height) else { return nil }
        dither = r
        ditherStart = Date().timeIntervalSinceReferenceDate
        let t = Timer(timeInterval: 1.0/60.0, repeats: true) { [weak self] _ in
            guard let self, let r = self.dither else { return }
            let time = Float(Date().timeIntervalSinceReferenceDate - self.ditherStart)
            r.render(time: time, level: Float(self.host.level()))
        }
        RunLoop.main.add(t, forMode: .common)   // keep ticking during window drags
        ditherTimer = t
        return r.surface
    }

    private func stopDither() {
        ditherTimer?.invalidate(); ditherTimer = nil; dither = nil
    }
```

Update the `CarapaceBridge(...)` call in `applySkin` to feed the Studio surface (replace the Task-4 interim `contentSurface: nil`):

```swift
        let content = ditherSurface(forDir: dir, width: w * scale, height: h * scale)
        guard let b = CarapaceBridge(skinDir: dir, width: w * scale, height: h * scale,
                                     contentSurface: content,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
```

Place the `let content = ...` line **before** the `guard let b = ...` (after `view.canvasH = ...` and `positionTrafficLights(forDir: dir)`), so the dither surface exists when the bridge is created.

- [ ] **Step 3: Build**

Run: `cd /Users/nexus/projects/experiments/winamp && cargo build -p carapace-ffi 2>&1 | tail -3 && cd showcase && swift build 2>&1 | tail -20`
Expected: both clean.

- [ ] **Step 4: Manual verification**

Run: `cd showcase && swift run Showcase`
- Press **Tab** to Studio → the viz panel shows a moving **dither field** (Studio-blue on near-black), not the old bars.
- **Play** a track → the dither visibly **brightens/densifies with the audio** (pulses with the music); pausing settles it down.
- Tab to Faceplate/Cassette → unchanged; Tab back to Studio → dither resumes; playback/selection persist across swaps.

- [ ] **Step 5: Update README**

In `showcase/README.md`, add to the manual-verification checklist (after the DSEG7 clock item) and Notes:
- Checklist: "Press **Tab** to Studio and **play** — confirm the viz panel is a live dither field that pulses with the audio (real AVAudioPlayer metering)."
- Notes: "Studio's visualizer is paper.design's **dithering** effect, host-rendered (Metal) into a `view{ id="host" }` cutout and driven by real audio level. `viz_*` is still the time-driven fallback used by Faceplate."

- [ ] **Step 6: Commit**

```bash
git add showcase/skins/studio/skin.lua showcase/Sources/Showcase/App.swift showcase/README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "feat(showcase): live music-reactive dither visualizer on Studio"
```

---

## Final verification

- [ ] `cd showcase && swift test` — all unit tests pass (metering + uniforms + existing 20).
- [ ] `cargo build -p carapace-ffi && cd showcase && swift run Showcase` — walk the README checklist: Studio dither renders + reacts to audio; Faceplate/Cassette unchanged; hot-swap preserves playback; ⌘O + DSEG7 + traffic lights still fine.
- [ ] `git log --oneline` shows the design + 5 task commits on `showcase-dither`.

## Notes / risks

- **No CI coverage** for Swift/Metal; `swift test` (pure logic) + manual run is the safety net, matching the showcase's practice.
- **`content_surface` bridging cast** (Task 4): if `IOSurfaceRef?` cast fails, fall back to the `Unmanaged` form (noted inline). The `surfaces` array on line 46 is the reference pattern.
- **Aspect**: the shader uses the cutout resolution (474×214) for the Bayer grid, so cells stay square after the full-canvas→cutout stretch. If cells still look stretched, the fix is in `dither_fs` (already accounts for it), not the compositor.
- **Perf**: the dither renders a full 1440×960 surface every frame though only the 948×428 cutout shows it (same tradeoff the paper spike measured). Fine at Tier 2; dirty-region optimization is out of scope.
- **Threading**: single writer (main-thread Timer) into the IOSurface; engine reads async on its render thread. Unsynchronized by design — invisible tearing for a soft dither. No locks.

# Showcase C1: Three Polished Skins + Per-Skin Sizing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the plain `starter`/`alt` skins with three polished, native-sized concept skins (Faceplate 380×560, Studio Deck 720×480, Cassette 600×400) that hot-swap over the same Swift-owned `MusicHost`, the window resizing to each skin.

**Architecture:** Enhance the app (`showcase/`) so each skin declares its own `canvas` in `skin.toml`; on Tab the app destroys the current `CarapaceBridge` and creates a new one at the next skin's size, resizing the borderless window. `MusicHost`/`AVAudioPlayer` are Swift-side so playback/state persist across the size-changing swap. Then author the three skins in Lua at their native sizes.

**Tech Stack:** Swift 6 (language mode 5), AppKit, IOSurface, carapace-ffi (unchanged), carapace skin Lua vocab.

## Global Constraints

- macOS only, `.swiftLanguageMode(.v5)`, git identity `Daniel Agbemava <danagbemava@gmail.com>`.
- NO `carapace`/`carapace-ffi` (Rust) changes — app (Swift) + skins (Lua) only.
- Each skin's size is its `skin.toml` `canvas = { width, height }`; the app reads it (never hardcodes per-skin sizes). Fallback if unreadable: 420×660.
- skin.toml schema: `schema = 1`, `id`, `name`, `engine = "^0.1"`, `canvas = { width = W, height = H }`, `entry = "skin.lua"`.
- Swap mechanism = destroy + recreate the bridge/pool at the new size (accepted: a brief GPU flash per swap). Old bridge destroyed BEFORE new is created.
- Skin vocab (verified against `crates/carapace/src/vocab.rs`): `fill{ path=<path>, color={r,g,b[,a]} | gradient=<grad>, [on_press=fn], [role='drag'|'passthrough'] }`; paths `rect{x,y,w,h}`, `rounded_rect{x,y,w,h,radius}`, `circle{cx,cy,r}`; gradient `{ type='linear'|'radial'|'sweep', from={x,y},to={x,y} (linear) | center={x,y},radius=N (radial) | center={x,y},start_deg=0,end_deg=360 (sweep), stops={ {at=0.0,color={r,g,b}}, {at=1.0,color={r,g,b}} } }` (stop key is `at`, ≥2 stops); `text{ value="key"|text="lit", x,y,size, color={..}|gradient=<grad>, [halign='right'] }`; `scrub{ value="key", on_seek="action", x,y,w,h, direction='right'|'left'|'up'|'down', color }`; `value_fill{ path=rect{}, value="key", direction='up'|..., color }`; `list{ collection="playlist", x,y,w,h, row_height, on_select="play_index", selected="current_index", highlight={r,g,b,a}, template={ {bind='now'|'title'|'artist'|'duration', x=<from left> | right=<from right>, y, size, color, [halign='right'] } } }`; `region{ path=<path>, role='drag', on_press=fn }`. A `RowCell` uses `x` XOR `right`. Topmost hotspot wins hit-test → declare the whole-body `role='drag'` region FIRST, controls after.
- Host surface every skin binds: state `track_title`/`artist`/`time`/`position`/`volume`/`playing`/`viz_0..15`; collection `playlist` (`now`/`title`/`artist`/`duration`); actions `toggle_play`/`stop`/`next`/`prev`/`seek`/`set_volume`/`play_index` + `begin_drag`/`minimize`/`close`.
- Build order: `cargo build -p carapace-ffi` (already built) then `swift build`; run/verify from `showcase/`. Not CI-gated.

---

### Task 1: skin.toml canvas parser (Swift, TDD)

**Files:**
- Create: `showcase/Sources/Showcase/SkinManifest.swift`
- Create: `showcase/Tests/ShowcaseTests/SkinManifestTests.swift`

**Interfaces:**
- Produces: `enum SkinManifest { static func parseCanvas(fromTOML toml: String) -> (w: Int, h: Int)? ; static func canvas(atDir dir: String, fallback: (Int, Int)) -> (Int, Int) }`

- [ ] **Step 1: Write the failing test**

`showcase/Tests/ShowcaseTests/SkinManifestTests.swift`:
```swift
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
```

- [ ] **Step 2: Run to verify it fails**

Run (from `showcase/`): `swift test --filter SkinManifest`
Expected: FAIL to compile — `SkinManifest` undefined.

- [ ] **Step 3: Implement**

`showcase/Sources/Showcase/SkinManifest.swift`:
```swift
import Foundation

/// Reads a skin's design canvas from its `skin.toml`. Deliberately tiny — scans for the two
/// integers in `canvas = { width = W, height = H }` rather than pulling in a TOML dependency.
enum SkinManifest {
    static func parseCanvas(fromTOML toml: String) -> (w: Int, h: Int)? {
        func intAfter(_ key: String) -> Int? {
            // match e.g. `width = 380` (any whitespace), taking the first occurrence.
            guard let r = toml.range(of: "\(key)\\s*=\\s*([0-9]+)", options: .regularExpression) else { return nil }
            let digits = toml[r].drop(while: { !$0.isNumber })
            return Int(digits)
        }
        guard let w = intAfter("width"), let h = intAfter("height") else { return nil }
        return (w, h)
    }

    static func canvas(atDir dir: String, fallback: (Int, Int)) -> (Int, Int) {
        let path = (dir as NSString).appendingPathComponent("skin.toml")
        guard let toml = try? String(contentsOfFile: path, encoding: .utf8),
              let c = parseCanvas(fromTOML: toml) else { return fallback }
        return (c.w, c.h)
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run (from `showcase/`): `swift test --filter SkinManifest`
Expected: PASS (3 tests). Also run full `swift test` — existing 9 still pass.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/SkinManifest.swift showcase/Tests/ShowcaseTests/SkinManifestTests.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): skin.toml canvas parser for per-skin sizing"
```

---

### Task 2: App per-skin-size refactor (destroy+recreate + resize)

**Files:**
- Modify: `showcase/Sources/Showcase/SkinView.swift` (replace global `CANVAS_W/H` with instance vars)
- Modify: `showcase/Sources/Showcase/App.swift` (`applySkin(dir:)`, startup, `cycleSkin`, `resolveSkinDirs`)

**Interfaces:**
- Consumes: `SkinManifest.canvas(atDir:fallback:)` (Task 1); `CarapaceBridge.init?(skinDir:width:height:onFrame:)` (existing).
- Produces: `SkinView.canvasW`/`SkinView.canvasH` instance vars; `AppDelegate.applySkin(dir:)`.

- [ ] **Step 1: Give SkinView per-skin canvas vars**

In `showcase/Sources/Showcase/SkinView.swift`: DELETE the globals `let CANVAS_W = 420` / `let CANVAS_H = 660`. Add instance vars on `SkinView` (near the top of the class):
```swift
    var canvasW: Double = 420
    var canvasH: Double = 660
```
And change `canvasPoint` to use them:
```swift
    private func canvasPoint(_ e: NSEvent) -> (Double, Double) {
        let p = convert(e.locationInWindow, from: nil)
        let cx = Double(p.x) * canvasW / Double(bounds.width)
        let cy = (Double(bounds.height) - Double(p.y)) * canvasH / Double(bounds.height)
        return (cx, cy)
    }
```

- [ ] **Step 2: Refactor App.swift for per-skin size + destroy/recreate swap**

Replace the body of `applicationDidFinishLaunching`, `cycleSkin`, and `resolveSkinDirs` in `showcase/Sources/Showcase/App.swift` with:

```swift
    func applicationDidFinishLaunching(_ note: Notification) {
        NSApp.setActivationPolicy(.regular)
        host = makePlaceholderHost()
        hostBox.host = host
        skinDirs = resolveSkinDirs()

        // Create the window + view once; applySkin sizes them and builds the first bridge.
        view = SkinView(frame: NSRect(x: 0, y: 0, width: 420, height: 660))
        view.onTab = { [weak self] in self?.cycleSkin() }
        window = SkinWindow(contentRect: NSRect(x: 200, y: 200, width: 420, height: 660),
                            styleMask: [.borderless], backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = view
        windowBox.window = window

        applySkin(dir: skinDirs[skinIndex])
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Point the app at `dir`: tear down the current engine, resize the window to the skin's
    /// canvas, and create a fresh bridge/pool at that size. MusicHost/audio are untouched, so
    /// playback + state persist across the swap.
    private func applySkin(dir: String) {
        let (w, h) = SkinManifest.canvas(atDir: dir, fallback: (420, 660))
        let scale = Int((NSScreen.main?.backingScaleFactor ?? 2).rounded())

        // 1. Destroy the current engine FIRST (its render thread joins in carapace_destroy),
        //    so no stale frame can fire after we re-point the frame sink.
        bridge = nil

        // 2. Resize the borderless window to the new skin's canvas, preserving the top-left corner.
        let topY = window.frame.origin.y + window.frame.height
        window.setContentSize(NSSize(width: w, height: h))
        var origin = window.frame.origin
        origin.y = topY - window.frame.height
        window.setFrameOrigin(origin)
        view.frame = NSRect(x: 0, y: 0, width: w, height: h)
        view.canvasW = Double(w)
        view.canvasH = Double(h)

        // 3. Build a fresh bridge/pool at the new size.
        guard let b = CarapaceBridge(skinDir: dir, width: w * scale, height: h * scale,
                                     onFrame: { [weak self] s, i in self?.view.show(surface: s, index: i) }) else {
            print("[showcase] bridge init failed for \(dir)"); NSApp.terminate(nil); return
        }
        bridge = b
        view.bridge = b
    }

    private func cycleSkin() {
        skinIndex = (skinIndex + 1) % skinDirs.count
        applySkin(dir: skinDirs[skinIndex]) // window resizes to the next skin; MusicHost persists
    }
```

And TEMPORARILY point `resolveSkinDirs` at two DIFFERENT-sized existing skins to prove the resize works (Task 6 switches it to the three concept skins):
```swift
    func resolveSkinDirs() -> [String] {
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        // TEMP (Task 6 replaces with [faceplate, studio, cassette]): two different-sized skins to
        // exercise per-skin resize — starter (420×660) and the demo `reference` skin (342×394).
        let starter = repo.appendingPathComponent("showcase/skins/starter").path
        let reference = repo.appendingPathComponent("crates/carapace-demo/skins/reference").path
        return [starter, reference]
    }
```

- [ ] **Step 3: Build + verify resize on swap**

Run: `cargo build -p carapace-ffi && (cd showcase && swift build && swift test)`
Expected: builds; `swift test` still 12/12 (9 + 3 SkinManifest).

Then a manual/backgrounded run (the app blocks — background + kill; the user may run it instead):
`(cd showcase && swift run Showcase > /tmp/c1-t2.log 2>&1 &) ; sleep 6 ; screencapture -x /tmp/c1-t2-a.png ; ...` then send Tab (or ask the user) and screencapture `/tmp/c1-t2-b.png`.
Expected: opens on `starter` at 420×660; on Tab the window RESIZES to `reference` (342×394) and renders it; playback/state persist. (If you cannot send Tab headlessly, verify the two skins load at their sizes by temporarily setting `skinDirs` to each alone, and rely on the user's manual Tab check.)

- [ ] **Step 4: Commit**

```bash
git add showcase/Sources/Showcase/SkinView.swift showcase/Sources/Showcase/App.swift
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): per-skin window sizing via destroy+recreate swap"
```

---

### Task 3: Faceplate skin (380×560)

**Files:**
- Create: `showcase/skins/faceplate/skin.toml`
- Create: `showcase/skins/faceplate/skin.lua`

- [ ] **Step 1: Manifest**

`showcase/skins/faceplate/skin.toml`:
```toml
schema = 1
id = "faceplate"
name = "Faceplate"
engine = "^0.1"
canvas = { width = 380, height = 560 }
entry = "skin.lua"
```

- [ ] **Step 2: Skin**

`showcase/skins/faceplate/skin.lua`:
```lua
-- shaped body + radial glow; whole-body drag (declared first so controls win hit-test)
fill{ path = rounded_rect{x=0, y=0, w=380, h=560, radius=30},
      gradient = { type='linear', from={x=0,y=0}, to={x=0,y=560},
        stops={ {at=0.0, color={r=48,g=44,b=84}}, {at=0.5, color={r=21,g=25,b=42}}, {at=1.0, color={r=8,g=11,b=18}} } } }
fill{ path = circle{cx=70, cy=70, r=120},
      gradient = { type='radial', center={x=70,y=70}, radius=120,
        stops={ {at=0.0, color={r=255,g=255,b=255, a=40}}, {at=1.0, color={r=255,g=255,b=255, a=0}} } } }
region{ path = rounded_rect{x=0, y=0, w=380, h=560, radius=30}, role='drag',
        on_press = function() host.begin_drag() end }

-- window buttons
text{ text="_", x=344, y=10, size=16, color={r=200,g=205,b=220} }
region{ path=rect{x=340,y=10,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=362, y=10, size=16, color={r=235,g=145,b=150} }
region{ path=rect{x=358,y=10,w=16,h=18}, on_press=function() host.close() end }

-- LCD panel
fill{ path = rounded_rect{x=24, y=44, w=332, h=132, radius=10}, color={r=3,g=16,b=8} }
fill{ path = rounded_rect{x=24, y=44, w=332, h=132, radius=10}, color={r=31,g=122,b=68, a=40} }
text{ value="track_title", x=40, y=62, size=22,
      gradient={ type='linear', from={x=0,y=0}, to={x=0,y=24},
        stops={ {at=0.0, color={r=141,g=255,b=173}}, {at=1.0, color={r=60,g=200,b=120}} } } }
text{ value="artist", x=40, y=96, size=15, color={r=110,g=200,b=150} }
text{ value="time", x=40, y=140, size=14, color={r=90,g=175,b=125} }

-- seek scrub
fill{ path=rounded_rect{x=24, y=190, w=332, h=12, radius=6}, color={r=31,g=41,b=55} }
scrub{ value="position", on_seek="seek", x=24, y=190, w=332, h=12, direction='right', color={r=92,g=255,b=154} }

-- transport row (prev / stop / play / next)
fill{ path=rounded_rect{x=40,  y=222, w=64, h=48, radius=8}, color={r=48,g=58,b=78},
      on_press=function() host.prev() end }
text{ text="<<", x=60, y=236, size=16, color={r=210,g=220,b=235} }
fill{ path=rounded_rect{x=112, y=222, w=64, h=48, radius=8}, color={r=48,g=58,b=78},
      on_press=function() host.stop() end }
text{ text="[]", x=134, y=236, size=15, color={r=210,g=220,b=235} }
fill{ path=rounded_rect{x=184, y=222, w=72, h=48, radius=8},
      gradient={ type='linear', from={x=0,y=222}, to={x=0,y=270},
        stops={ {at=0.0, color={r=120,g=255,b=173}}, {at=1.0, color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text=">", x=214, y=236, size=18, color={r=8,g=30,b=18} }
fill{ path=rounded_rect{x=264, y=222, w=64, h=48, radius=8}, color={r=48,g=58,b=78},
      on_press=function() host.next() end }
text{ text=">>", x=284, y=236, size=16, color={r=210,g=220,b=235} }

-- volume
text{ text="vol", x=24, y=288, size=13, color={r=150,g=165,b=190} }
fill{ path=rounded_rect{x=64, y=290, w=292, h=10, radius=5}, color={r=31,g=41,b=55} }
scrub{ value="volume", on_seek="set_volume", x=64, y=290, w=292, h=10, direction='right', color={r=255,g=200,b=87} }

-- queue drawer
fill{ path=rounded_rect{x=24, y=320, w=332, h=224, radius=12}, color={r=13,g=18,b=30} }
list{ collection="playlist", x=34, y=330, w=312, h=204, row_height=34,
      on_select="play_index", selected="current_index", highlight={r=40,g=52,b=44, a=200},
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='artist', x=170, y=8, size=14, color={r=150,g=165,b=190} },
        { bind='duration', right=10, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
```

- [ ] **Step 3: Verify it loads + renders**

Temporarily set `resolveSkinDirs` to return only this skin (`return [repo.appendingPathComponent("showcase/skins/faceplate").path]`), run backgrounded + screencapture `/tmp/c1-faceplate.png`, confirm the log has NO `carapace_create failed` and the screenshot shows the faceplate at 380×560 with the real playlist. REVERT `resolveSkinDirs` to Task 2's temp `[starter, reference]`. If a vocab arg is wrong (load fails), fix against `crates/carapace/src/vocab.rs` and re-run.

- [ ] **Step 4: Commit**

```bash
git add showcase/skins/faceplate
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): Faceplate skin (380x560)"
```

---

### Task 4: Studio Deck skin (720×480)

**Files:**
- Create: `showcase/skins/studio/skin.toml`
- Create: `showcase/skins/studio/skin.lua`

- [ ] **Step 1: Manifest**

`showcase/skins/studio/skin.toml`:
```toml
schema = 1
id = "studio"
name = "Studio Deck"
engine = "^0.1"
canvas = { width = 720, height = 480 }
entry = "skin.lua"
```

- [ ] **Step 2: Skin**

`showcase/skins/studio/skin.lua`:
```lua
fill{ path = rounded_rect{x=0, y=0, w=720, h=480, radius=16},
      gradient={ type='linear', from={x=0,y=0}, to={x=720,y=480},
        stops={ {at=0.0, color={r=38,g=48,b=68}}, {at=0.6, color={r=16,g=24,b=39}}, {at=1.0, color={r=9,g=13,b=22}} } } }
region{ path = rounded_rect{x=0, y=0, w=720, h=480, radius=16}, role='drag',
        on_press=function() host.begin_drag() end }

text{ text="_", x=684, y=10, size=16, color={r=200,g=205,b=220} }
region{ path=rect{x=680,y=10,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=702, y=10, size=16, color={r=235,g=145,b=150} }
region{ path=rect{x=698,y=10,w=16,h=18}, on_press=function() host.close() end }

-- title bar
fill{ path=rounded_rect{x=20, y=20, w=680, h=44, radius=8}, color={r=7,g=20,b=13} }
text{ value="track_title", x=34, y=30, size=20, color={r=141,g=255,b=173} }
text{ value="artist", x=300, y=34, size=15, color={r=150,g=165,b=190} }
text{ value="time", right=20, y=34, size=14, color={r=120,g=135,b=160}, halign='right' }

-- visualizer (left column)
fill{ path=rounded_rect{x=20, y=80, w=340, h=180, radius=10}, color={r=10,g=16,b=32} }
value_fill{ path=rect{x=36,  y=100, w=40, h=140}, value="viz_0", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=84,  y=100, w=40, h=140}, value="viz_1", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=132, y=100, w=40, h=140}, value="viz_2", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=180, y=100, w=40, h=140}, value="viz_3", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=228, y=100, w=40, h=140}, value="viz_4", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=276, y=100, w=40, h=140}, value="viz_5", direction='up', color={r=77,g=215,b=255} }

-- knobs (decoration) + volume
fill{ path=circle{cx=60, cy=310, r=26}, gradient={ type='radial', center={x=60,y=302}, radius=30,
        stops={ {at=0.0, color={r=120,g=130,b=150}}, {at=1.0, color={r=31,g=41,b=55}} } } }
fill{ path=circle{cx=130, cy=310, r=26}, gradient={ type='radial', center={x=130,y=302}, radius=30,
        stops={ {at=0.0, color={r=120,g=130,b=150}}, {at=1.0, color={r=31,g=41,b=55}} } } }
text{ text="vol", x=180, y=286, size=13, color={r=150,g=165,b=190} }
fill{ path=rounded_rect{x=180, y=304, w=180, h=12, radius=6}, color={r=31,g=41,b=55} }
scrub{ value="volume", on_seek="set_volume", x=180, y=304, w=180, h=12, direction='right', color={r=255,g=200,b=87} }

-- seek + transport
fill{ path=rounded_rect{x=20, y=350, w=340, h=12, radius=6}, color={r=31,g=41,b=55} }
scrub{ value="position", on_seek="seek", x=20, y=350, w=340, h=12, direction='right', color={r=92,g=255,b=154} }
fill{ path=rounded_rect{x=20,  y=380, w=70, h=44, radius=8}, color={r=48,g=58,b=78}, on_press=function() host.prev() end }
text{ text="<<", x=44, y=394, size=15, color={r=210,g=220,b=235} }
fill{ path=rounded_rect{x=100, y=380, w=90, h=44, radius=8},
      gradient={ type='linear', from={x=0,y=380}, to={x=0,y=424}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text="play", x=126, y=394, size=15, color={r=8,g=30,b=18} }
fill{ path=rounded_rect{x=200, y=380, w=70, h=44, radius=8}, color={r=48,g=58,b=78}, on_press=function() host.next() end }
text{ text=">>", x=224, y=394, size=15, color={r=210,g=220,b=235} }

-- playlist (right column)
fill{ path=rounded_rect{x=380, y=80, w=320, h=380, radius=10}, color={r=11,g=18,b=30} }
list{ collection="playlist", x=390, y=90, w=300, h=340, row_height=34,
      on_select="play_index", selected="current_index", highlight={r=36,g=112,b=66, a=200},
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='duration', right=10, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
```

- [ ] **Step 3: Verify** — same method as Task 3 Step 3 (temporarily point `resolveSkinDirs` at `showcase/skins/studio` only; screencapture `/tmp/c1-studio.png`; confirm loads at 720×480; REVERT).

- [ ] **Step 4: Commit**

```bash
git add showcase/skins/studio
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): Studio Deck skin (720x480)"
```

---

### Task 5: Cassette skin (600×400)

**Files:**
- Create: `showcase/skins/cassette/skin.toml`
- Create: `showcase/skins/cassette/skin.lua`

- [ ] **Step 1: Manifest**

`showcase/skins/cassette/skin.toml`:
```toml
schema = 1
id = "cassette"
name = "Cassette"
engine = "^0.1"
canvas = { width = 600, height = 400 }
entry = "skin.lua"
```

- [ ] **Step 2: Skin**

`showcase/skins/cassette/skin.lua`:
```lua
-- cassette body + drag
fill{ path = rounded_rect{x=0, y=0, w=600, h=400, radius=32},
      gradient={ type='linear', from={x=0,y=0}, to={x=600,y=400},
        stops={ {at=0.0, color={r=58,g=40,b=27}}, {at=0.45, color={r=23,g=16,b=11}}, {at=1.0, color={r=16,g=24,b=39}} } } }
region{ path = rounded_rect{x=0, y=0, w=600, h=400, radius=32}, role='drag', on_press=function() host.begin_drag() end }
text{ text="_", x=560, y=12, size=16, color={r=220,g=205,b=185} }
region{ path=rect{x=556,y=12,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=578, y=12, size=16, color={r=235,g=150,b=150} }
region{ path=rect{x=574,y=12,w=16,h=18}, on_press=function() host.close() end }

-- tape label
fill{ path=rounded_rect{x=140, y=40, w=320, h=70, radius=6},
      gradient={ type='linear', from={x=0,y=40}, to={x=0,y=110}, stops={ {at=0.0,color={r=255,g=214,b=107}}, {at=1.0,color={r=216,g=155,b=37}} } } }
text{ value="track_title", x=160, y=54, size=20, color={r=58,g=40,b=27} }
text{ value="artist", x=160, y=82, size=15, color={r=110,g=78,b=45} }

-- reels (sweep-gradient spokes + hub)
fill{ path=circle{cx=170, cy=250, r=64},
      gradient={ type='sweep', center={x=170,y=250}, start_deg=0, end_deg=360,
        stops={ {at=0.0,color={r=40,g=44,b=54}}, {at=0.25,color={r=17,g=24,b=39}}, {at=0.5,color={r=40,g=44,b=54}}, {at=0.75,color={r=17,g=24,b=39}}, {at=1.0,color={r=40,g=44,b=54}} } } }
fill{ path=circle{cx=170, cy=250, r=22}, color={r=217,g=168,b=92} }
fill{ path=circle{cx=170, cy=250, r=8}, color={r=15,g=23,b=42} }
fill{ path=circle{cx=430, cy=250, r=64},
      gradient={ type='sweep', center={x=430,y=250}, start_deg=0, end_deg=360,
        stops={ {at=0.0,color={r=40,g=44,b=54}}, {at=0.25,color={r=17,g=24,b=39}}, {at=0.5,color={r=40,g=44,b=54}}, {at=0.75,color={r=17,g=24,b=39}}, {at=1.0,color={r=40,g=44,b=54}} } } }
fill{ path=circle{cx=430, cy=250, r=22}, color={r=217,g=168,b=92} }
fill{ path=circle{cx=430, cy=250, r=8}, color={r=15,g=23,b=42} }

-- tape window between reels + time
fill{ path=rounded_rect{x=250, y=150, w=100, h=40, radius=4}, color={r=7,g=20,b=13} }
text{ value="time", x=262, y=162, size=13, color={r=141,g=255,b=173} }

-- keys (prev / play / stop / next)
fill{ path=rounded_rect{x=180, y=326, w=54, h=40, radius=6}, color={r=210,g=180,b=140}, on_press=function() host.prev() end }
text{ text="<<", x=196, y=338, size=14, color={r=58,g=40,b=27} }
fill{ path=rounded_rect{x=240, y=326, w=54, h=40, radius=6},
      gradient={ type='linear', from={x=0,y=326}, to={x=0,y=366}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text=">", x=262, y=338, size=16, color={r=8,g=30,b=18} }
fill{ path=rounded_rect{x=300, y=326, w=54, h=40, radius=6}, color={r=210,g=180,b=140}, on_press=function() host.stop() end }
text{ text="[]", x=318, y=338, size=13, color={r=58,g=40,b=27} }
fill{ path=rounded_rect{x=360, y=326, w=54, h=40, radius=6}, color={r=210,g=180,b=140}, on_press=function() host.next() end }
text{ text=">>", x=376, y=338, size=14, color={r=58,g=40,b=27} }

-- slim seek
fill{ path=rounded_rect{x=100, y=300, w=400, h=8, radius=4}, color={r=40,g=30,b=20} }
scrub{ value="position", on_seek="seek", x=100, y=300, w=400, h=8, direction='right', color={r=255,g=170,b=120} }
```

- [ ] **Step 3: Verify** — same method (temporarily point `resolveSkinDirs` at `showcase/skins/cassette` only; screencapture `/tmp/c1-cassette.png`; confirm loads at 600×400 with the sweep-gradient reels; REVERT). If the `sweep` gradient or any arg fails to load, fix against `vocab.rs` and re-run.

- [ ] **Step 4: Commit**

```bash
git add showcase/skins/cassette
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): Cassette skin (600x400) with sweep-gradient reels"
```

---

### Task 6: Wire the cycle to the three skins + cleanup

**Files:**
- Modify: `showcase/Sources/Showcase/App.swift` (`resolveSkinDirs` → the three concept skins)
- Delete: `showcase/skins/starter`, `showcase/skins/alt`
- Modify: `showcase/README.md` (usage: the three skins; Tab resizes)

- [ ] **Step 1: Point the cycle at the three skins**

In `showcase/Sources/Showcase/App.swift`, replace `resolveSkinDirs`'s temp body with:
```swift
    func resolveSkinDirs() -> [String] {
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        return ["faceplate", "studio", "cassette"].map {
            repo.appendingPathComponent("showcase/skins/\($0)").path
        }
    }
```

- [ ] **Step 2: Delete the retired placeholder skins**

Run: `git rm -r showcase/skins/starter showcase/skins/alt`

- [ ] **Step 3: Update the README**

In `showcase/README.md`, update the usage line to: "Press **Tab** to hot-swap Faceplate → Studio Deck → Cassette; the window resizes to each skin. Playback, position, volume, and selection persist across swaps." Update the manual-verification checklist to cycle all three and confirm the window resizes each time with state intact.

- [ ] **Step 4: Build + final verify all three**

Run: `cargo build -p carapace-ffi && (cd showcase && swift build && swift test)`
Expected: builds; `swift test` 12/12.
Then a run (user- or agent-assisted): launch (opens on Faceplate 380×560); Tab → Studio Deck (720×480) → Cassette (600×400) → back; each renders, the window resizes, and playback/position/volume/selection persist. Screenshot each (`/tmp/c1-final-{faceplate,studio,cassette}.png`). No `carapace_create failed` in the log for any skin.

- [ ] **Step 5: Commit**

```bash
git add showcase/Sources/Showcase/App.swift showcase/README.md
git -c user.name='Daniel Agbemava' -c user.email='danagbemava@gmail.com' \
  commit -m "feat(showcase): cycle Faceplate/Studio/Cassette; retire starter/alt"
```

---

### Final verification (after all tasks)

- [ ] `cargo build -p carapace-ffi && (cd showcase && swift build && swift test)` — builds, `swift test` 12/12, no Swift warnings.
- [ ] A run confirms all three skins load, the window resizes to each on Tab (380×560 / 720×480 / 600×400), and playback/position/volume/selection persist across the size-changing swaps. Screenshots captured.
- [ ] `carapace`/`carapace-ffi` unchanged (git diff shows only `showcase/` + docs).

## Self-review notes (reconciled)

- **TDD applies to Task 1** (canvas parser); Tasks 2–6 are app-refactor + skin-authoring verified by build + run (GUI/visual — no meaningful unit test), stated honestly.
- **Ordering:** Task 2 verifies the resize mechanism against two DIFFERENT-sized existing skins (`starter` 420×660 ↔ demo `reference` 342×394) so it's testable before the concept skins exist; Task 6 switches the cycle to the three and deletes the placeholders.
- **Skin vocab** (gradients `at`-keyed stops, `sweep` center/start_deg/end_deg, `role='drag'` first) is verified against `vocab.rs`; each skin task's Step 3 loads it (a bad arg fails `carapace_create`) and says to fix against `vocab.rs`.
- **Type consistency:** `SkinManifest.canvas`/`parseCanvas`, `SkinView.canvasW/canvasH`, `AppDelegate.applySkin(dir:)` used identically across tasks; `resolveSkinDirs` temp→final transition is explicit.

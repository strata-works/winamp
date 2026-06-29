# WidgetKit Offscreen-Bitmap Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove a carapace skin can render to an offscreen bitmap shown in an iOS home-screen widget, with an AppIntent button discretely swapping the displayed bitmap, in the iOS Simulator.

**Architecture:** Add a stateless one-shot `carapace_render_png` to the existing `embed-spike` C ABI (reusing the engine's offscreen-render + readback path, no IOSurface). A minimal SwiftUI container app calls it to pre-render PNGs for states `0..N` into a shared App Group container; a WidgetKit extension loads the current-state PNG into a SwiftUI `Image`; an `AppIntent` button updates the shared state and reloads the timeline so the widget swaps bitmaps. A stretch task attempts rendering inside the extension itself.

**Tech Stack:** Rust (wgpu 29, carapace engine, `image` for PNG), staticlib for `aarch64-apple-ios-sim`, Swift / SwiftUI / WidgetKit / AppIntents, Xcode 26.5.

**Spec:** `docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-design.md`

## Global Constraints

- **Zero engine diff.** No changes to `crates/carapace/src` or `crates/hittest/src`. `git diff --stat <base>...HEAD -- crates/carapace/src crates/hittest/src` MUST be empty at the end. All new code lives in `crates/embed-spike/` and the new sample.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>` (use `git -c user.name=... -c user.email=...` or a branch-local config).
- **Branch:** do this work on a spike branch off `main` (e.g. `widgetkit-bitmap-spike`), never directly on `main`.
- **Dependency fetches:** the first fetch of any new third-party crate must run through Socket Firewall: `sfw cargo add ...` (or `sfw cargo build` on first build that pulls a new crate).
- **Platform:** iOS Simulator only (Apple silicon). No physical-device build, no App Store entitlements. Tier-1/CPU-readback render only.
- **Skin under test:** reuse `crates/embed-spike/skin/` as-is (canvas 240×80, a `value_fill` bound to host key `"level"`). Do not author a new skin.
- **Render is stateless:** `carapace_render_png` takes the state scalar as a parameter; it does not use `carapace_create`/`carapace_tick` or the live vtable.

---

## File Structure

**Rust (`crates/embed-spike/`):**
- `Cargo.toml` — add `staticlib` crate-type; promote `image` to a runtime dependency (png only).
- `src/render.rs` — split platform gating: keep `init_gpu`, `GpuCtx`, `OffscreenTarget`, `new_offscreen`, `render_frame`, `readback_rgba` available on **all** targets; gate the IOSurface-only functions (`try_shared`, `upload_iosurface_to_texture`, `copy_into_iosurface`, and the `io_surface` import) to `cfg(target_os = "macos")`.
- `src/oneshot.rs` (new) — `OneShotHost` (a `carapace::host::Host` returning the passed-in `level`) and the `carapace_render_png` extern "C" function. Compiles on all targets.
- `src/lib.rs` — declare `pub mod oneshot;` unconditionally; keep the existing macOS-gated `ffi_impl` as-is.
- `carapace.h` — add the `carapace_render_png` prototype.
- `tests/render_png.rs` (new) — host-side (macOS) integration test proving the render works and that state changes the output.

**iOS sample (`crates/embed-spike/widget-sample/`, new Xcode project):**
- `CarapaceWidgetSpike.xcodeproj` — two targets: app + widget extension.
- `App/` — SwiftUI container app: `carapace.h` via bridging header, calls `carapace_render_png` for states `0..N`, writes PNGs to the App Group container; a debug `Image` view to eyeball the render in-app.
- `Widget/` — WidgetKit extension: `TimelineProvider`, the SwiftUI widget view, and the `BumpIntent` `AppIntent`.
- `Shared/AppGroup.swift` — the App Group id + helpers to compute PNG/state paths (used by both targets).
- `libembed_spike.a` — the iOS-sim staticlib, linked into both targets.

---

## Task 1: One-shot `carapace_render_png` (host-tested on macOS)

The whole render path, validated on the dev machine before any iOS work. This is the spike's biggest de-risk.

**Files:**
- Modify: `crates/embed-spike/Cargo.toml`
- Modify: `crates/embed-spike/src/render.rs` (platform gating only)
- Create: `crates/embed-spike/src/oneshot.rs`
- Modify: `crates/embed-spike/src/lib.rs:1-6` (add `pub mod oneshot;`)
- Create: `crates/embed-spike/tests/render_png.rs`

**Interfaces:**
- Consumes (from `carapace`): `skin::load_dir(&Path) -> Result<(Manifest, SkinSource), SkinError>`; `engine::Engine::new(Box<dyn Host>, VocabRegistry::base(), SkinSource) -> Result<Engine>`; `Engine::scene().canvas -> (u32, u32)`; `host::{Host, ActionSpec, Row, Value}`; `state::StateValue`.
- Consumes (from `render.rs`): `init_gpu() -> GpuCtx`; `new_offscreen(&Device, u32, u32) -> OffscreenTarget`; `render_frame(&mut Engine, &mut Renderer, &GpuCtx, &TextureView, u32, u32, Duration, bool, Option<(&str,&TextureView)>)`; `readback_rgba(&GpuCtx, &Texture, u32, u32) -> Vec<u8>` (tightly-packed RGBA8).
- Produces: `extern "C" fn carapace_render_png(skin_dir: *const c_char, w: u32, h: u32, state: f64, out_path: *const c_char) -> bool` and `struct OneShotHost { level: f32 }`.

- [ ] **Step 1: Add the staticlib crate-type and promote `image`**

Edit `crates/embed-spike/Cargo.toml`. Change the `[lib]` line and move `image` from dev-deps to deps:

```toml
[lib]
crate-type = ["cdylib", "rlib", "staticlib"]   # +staticlib: iOS links a static archive

[dependencies]
carapace = { path = "../carapace" }
libc = "0.2.186"
pollster = "0.4.0"
wgpu = "29.0.3"
wgpu-hal = "=29.0.3"
image = { version = "0.24", default-features = false, features = ["png"] }
```

Leave the `[target.'cfg(target_os = "macos")'.dependencies]` block unchanged. In `[dev-dependencies]`, remove the now-redundant `image = "0.24"` line (keep `carapace`).

- [ ] **Step 2: Re-gate `render.rs` so the neutral render fns build everywhere**

In `crates/embed-spike/src/render.rs`, the four functions `init_gpu`, `new_offscreen`, `render_frame`, `readback_rgba` (and the structs `GpuCtx`, `OffscreenTarget`, `Tier`) use only `wgpu` + `carapace` and must compile on iOS. Only the IOSurface functions need `io_surface`.

Make these edits:
1. Move the top-of-file `io_surface` import (lines 1-5) to sit **directly above** the first IOSurface function, and gate it:
```rust
// io-surface 0.16 is deprecated in favour of objc2-io-surface; we knowingly use it here.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
use io_surface::{
    IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceRef, IOSurfaceUnlock,
};
```
2. Add `#[cfg(target_os = "macos")]` immediately above each of: `pub unsafe fn try_shared`, `pub fn make_content_texture`, `pub unsafe fn upload_iosurface_to_texture`, and `pub unsafe fn copy_into_iosurface`. (Leave `init_gpu`, `new_offscreen`, `render_frame`, `readback_rgba`, `blit`, `GpuCtx`, `OffscreenTarget`, `Tier` ungated.)

- [ ] **Step 3: Keep `render` module available on iOS too**

In `crates/embed-spike/src/lib.rs`, change the render-module gate (line 3-4) from macOS-only to both Apple targets, and add the new module:

```rust
pub mod host;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod render;

pub mod oneshot;
```

(Leave the existing `#[cfg(target_os = "macos")] mod ffi_impl { ... }` block untouched.)

- [ ] **Step 4: Write the failing host-side test**

Create `crates/embed-spike/tests/render_png.rs`. It renders the spike skin at a low and a high state and asserts the bound bar covers more pixels at the high state.

```rust
use std::ffi::CString;
use std::path::PathBuf;

use embed_spike::oneshot::carapace_render_png;

// Counts pixels whose green channel dominates (the value_fill bar is bright green
// {r=120,g=230,b=80} over a near-black {r=18,g=20,b=26} background).
fn green_pixels(path: &std::path::Path) -> u64 {
    let img = image::open(path).unwrap().to_rgba8();
    img.pixels()
        .filter(|p| p[1] > 150 && p[0] < 160 && p[2] < 130)
        .count() as u64
}

#[test]
fn render_png_writes_a_file_and_state_drives_the_bar() {
    let skin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skin");
    let skin_c = CString::new(skin.to_str().unwrap()).unwrap();

    let dir = std::env::temp_dir();
    let low = dir.join("spike_low.png");
    let high = dir.join("spike_high.png");
    let low_c = CString::new(low.to_str().unwrap()).unwrap();
    let high_c = CString::new(high.to_str().unwrap()).unwrap();

    let ok_low =
        unsafe { carapace_render_png(skin_c.as_ptr(), 240, 80, 0.15, low_c.as_ptr()) };
    let ok_high =
        unsafe { carapace_render_png(skin_c.as_ptr(), 240, 80, 0.90, high_c.as_ptr()) };

    assert!(ok_low && ok_high, "render should succeed");
    assert!(low.exists() && high.exists(), "PNGs should be written");

    let (g_low, g_high) = (green_pixels(&low), green_pixels(&high));
    assert!(g_low > 0, "low state should still draw some bar");
    assert!(
        g_high > g_low * 2,
        "high state bar must be much larger: low={g_low} high={g_high}"
    );
}
```

- [ ] **Step 5: Run the test to confirm it fails to compile**

Run: `cargo test -p embed-spike --test render_png`
Expected: FAIL — `carapace_render_png` / `oneshot` module not found (it doesn't exist yet).

- [ ] **Step 6: Implement `oneshot.rs`**

Create `crates/embed-spike/src/oneshot.rs`:

```rust
use std::ffi::{c_char, CStr};
use std::path::PathBuf;
use std::time::Duration;

use carapace::engine::Engine;
use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::render::Renderer;
use carapace::state::StateValue;

use crate::render::{init_gpu, new_offscreen, readback_rgba, render_frame};

/// Minimal stateless host for one-shot renders: reports a single scalar under key "level".
/// Advertises `toggle` only so the spike skin's `region{ on_press = host.toggle }` resolves at
/// load; it is never invoked (one-shot render forwards no input).
pub struct OneShotHost {
    pub level: f32,
}

const ACTIONS: &[ActionSpec] = &[ActionSpec { name: "toggle" }];

impl Host for OneShotHost {
    fn name(&self) -> &str {
        "oneshot"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        if key == "level" {
            Some(StateValue::Scalar(self.level))
        } else {
            None
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, _action: &str, _args: &[Value]) {}
    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new()
    }
}

/// One-shot headless render of `skin_dir` at the given `state` (drives host key "level") into a
/// `w`×`h` PNG written to `out_path`. Stateless; no IOSurface; CPU-readback path. Returns true on
/// success, false on any failure (never panics across the FFI boundary).
///
/// # Safety
/// `skin_dir` and `out_path` must be valid NUL-terminated UTF-8 paths.
#[no_mangle]
pub unsafe extern "C" fn carapace_render_png(
    skin_dir: *const c_char,
    w: u32,
    h: u32,
    state: f64,
    out_path: *const c_char,
) -> bool {
    if skin_dir.is_null() || out_path.is_null() || w == 0 || h == 0 {
        return false;
    }
    let render = || -> Option<()> {
        let dir = PathBuf::from(unsafe { CStr::from_ptr(skin_dir) }.to_str().ok()?);
        let out = unsafe { CStr::from_ptr(out_path) }.to_str().ok()?.to_string();

        let (_manifest, source) = carapace::skin::load_dir(&dir).ok()?;
        let mut engine = Engine::new(
            Box::new(OneShotHost {
                level: state as f32,
            }),
            carapace::vocab::VocabRegistry::base(),
            source,
        )
        .ok()?;

        let gpu = init_gpu();
        let mut renderer = Renderer::new(&gpu.device);
        let off = new_offscreen(&gpu.device, w, h);
        // wait=true so readback sees completed GPU work; no host view{} content for this skin.
        render_frame(
            &mut engine,
            &mut renderer,
            &gpu,
            &off.view,
            w,
            h,
            Duration::ZERO,
            true,
            None,
        );
        let rgba = readback_rgba(&gpu, &off.tex, w, h);
        image::save_buffer(&out, &rgba, w, h, image::ColorType::Rgba8).ok()?;
        Some(())
    };
    // Convert any None (and isolate against unwind) into a clean `false`.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(render))
        .ok()
        .flatten()
        .is_some()
}
```

- [ ] **Step 7: Run the test to confirm it passes**

Run: `cargo test -p embed-spike --test render_png`
Expected: PASS. (If `init_gpu` panics for lack of a Metal adapter, you are not on the intended macOS host — note and stop.)

- [ ] **Step 8: Confirm the whole workspace still builds and is warning-clean**

Run: `cargo build -p embed-spike && cargo clippy -p embed-spike -- -D warnings`
Expected: builds, no clippy errors. (CI gates on `clippy -D warnings`.)

- [ ] **Step 9: Commit**

```bash
git add crates/embed-spike/Cargo.toml crates/embed-spike/src/render.rs \
        crates/embed-spike/src/lib.rs crates/embed-spike/src/oneshot.rs \
        crates/embed-spike/tests/render_png.rs
git commit -m "spike(widget): one-shot carapace_render_png + iOS-portable render path"
```

---

## Task 2: Cross-compile the iOS-simulator staticlib + header

Produce the artifact the Xcode targets link, and prove the crate compiles for iOS.

**Files:**
- Modify: `crates/embed-spike/carapace.h`

**Interfaces:**
- Consumes: `carapace_render_png` from Task 1.
- Produces: `carapace.h` with the new prototype; a built `libembed_spike.a` for `aarch64-apple-ios-sim`.

- [ ] **Step 1: Add the prototype to the C header**

In `crates/embed-spike/carapace.h`, just before the closing `#endif`, add:

```c
/* One-shot headless render of a skin to a PNG file (CPU readback path).
 * Stateless: `state` drives the skin's value_fill via host key "level".
 * w,h are the output pixel size. Returns true on success, false on any failure. */
bool carapace_render_png(const char* skin_dir, uint32_t w, uint32_t h,
                         double state, const char* out_path);
```

- [ ] **Step 2: Install the iOS-simulator Rust target**

Run: `rustup target add aarch64-apple-ios-sim`
Expected: target installed (or "up to date").

- [ ] **Step 3: Build the staticlib for the simulator (through Socket Firewall on first fetch)**

Run: `sfw cargo build -p embed-spike --target aarch64-apple-ios-sim --release`
Expected: completes; `target/aarch64-apple-ios-sim/release/libembed_spike.a` exists.
If a macOS-only dep (`io-surface`, `objc2-*`) tries to build, the re-gating in Task 1 Step 2 was incomplete — fix the `#[cfg(target_os = "macos")]` coverage so those are not referenced on iOS.

- [ ] **Step 4: Sanity-check the archive exposes the symbol**

Run: `nm target/aarch64-apple-ios-sim/release/libembed_spike.a 2>/dev/null | grep carapace_render_png | head`
Expected: at least one line showing `_carapace_render_png` (a `T` text symbol).

- [ ] **Step 5: Commit**

```bash
git add crates/embed-spike/carapace.h
git commit -m "spike(widget): declare carapace_render_png in C header; build iOS-sim staticlib"
```

---

## Task 3: SwiftUI container app that renders states into an App Group

The app process (full GPU) pre-renders PNGs for states `0..N` and writes them to the shared container.

**Files:**
- Create: `crates/embed-spike/widget-sample/` Xcode project (app target `CarapaceWidgetSpike`).
- Create: `widget-sample/Shared/AppGroup.swift`
- Create: `widget-sample/App/ContentView.swift`, `widget-sample/App/CarapaceWidgetSpikeApp.swift`
- Create: `widget-sample/App/CarapaceBridge.swift`
- Create bridging header referencing `carapace.h`.

**Interfaces:**
- Consumes: `libembed_spike.a` + `carapace.h` from Task 2; the skin dir `crates/embed-spike/skin/`.
- Produces: PNGs `state-0.png … state-3.png` and a `state.txt` (current index) in the App Group container; `AppGroup` Swift API used by Task 4/5.

- [ ] **Step 1: Create the Xcode project**

In Xcode: File ▸ New ▸ Project ▸ iOS ▸ App. Product name `CarapaceWidgetSpike`, interface **SwiftUI**, language **Swift**. Save into `crates/embed-spike/widget-sample/`. Select your **free personal team** under Signing (personal teams permit App Groups in the Simulator).

- [ ] **Step 2: Add the App Group capability**

App target ▸ Signing & Capabilities ▸ + Capability ▸ **App Groups** ▸ add `group.carapace.spike`. Confirm an `.entitlements` file is created listing that group.

- [ ] **Step 3: Vendor the staticlib, header, bridging header, and skin into the app**

- Copy `target/aarch64-apple-ios-sim/release/libembed_spike.a` and `crates/embed-spike/carapace.h` into `widget-sample/Vendor/`.
- App target ▸ Build Phases ▸ Link Binary With Libraries ▸ add `libembed_spike.a`.
- App target ▸ Build Settings ▸ **Objective-C Bridging Header** = `CarapaceWidgetSpike/Bridging-Header.h`; create that file containing `#import "carapace.h"` and add `Vendor/` to **Header Search Paths**.
- Add the skin folder as a **folder reference** (blue) so it ships in the app bundle: drag `crates/embed-spike/skin/` into the app target, choosing "Create folder references".

- [ ] **Step 4: Write the App Group helper**

Create `widget-sample/Shared/AppGroup.swift`:

```swift
import Foundation

enum AppGroup {
    static let id = "group.carapace.spike"
    static let stateCount = 4   // states 0..3

    static var container: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id)!
    }
    static func pngURL(state: Int) -> URL {
        container.appendingPathComponent("state-\(state).png")
    }
    static var stateFile: URL { container.appendingPathComponent("state.txt") }

    static func currentState() -> Int {
        (try? String(contentsOf: stateFile))
            .flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) } ?? 0
    }
    static func setState(_ s: Int) {
        try? String(s).write(to: stateFile, atomically: true, encoding: .utf8)
    }
}
```

- [ ] **Step 5: Write the render bridge**

Create `widget-sample/App/CarapaceBridge.swift`:

```swift
import Foundation

enum CarapaceBridge {
    /// Render every state PNG into the App Group container. Returns true if all succeeded.
    @discardableResult
    static func renderAllStates(width: UInt32 = 240, height: UInt32 = 80) -> Bool {
        guard let skinDir = Bundle.main.url(forResource: "skin", withExtension: nil)?.path
        else { return false }
        var allOK = true
        for i in 0..<AppGroup.stateCount {
            let level = Double(i) / Double(AppGroup.stateCount - 1)   // 0.0 … 1.0
            let out = AppGroup.pngURL(state: i).path
            let ok = carapace_render_png(skinDir, width, height, level, out)
            allOK = allOK && ok
        }
        return allOK
    }
}
```

- [ ] **Step 6: Call it at launch and show a debug preview**

Replace `ContentView.swift` body to render on appear and display the current-state PNG so you can eyeball the render in-app:

```swift
import SwiftUI

struct ContentView: View {
    @State private var ok = false
    @State private var image: UIImage?

    var body: some View {
        VStack(spacing: 16) {
            Text(ok ? "Rendered \(AppGroup.stateCount) states ✅" : "Rendering…")
            if let image { Image(uiImage: image).border(.gray) }
        }
        .onAppear {
            ok = CarapaceBridge.renderAllStates()
            image = UIImage(contentsOfFile: AppGroup.pngURL(state: AppGroup.stateCount - 1).path)
        }
    }
}
```

- [ ] **Step 7: Build & run in the simulator; verify**

Run the app target on an iOS Simulator (e.g. iPhone 15). 
Expected: the app shows "Rendered 4 states ✅" and a green bar on a dark background (the highest state ≈ full-width bar). If it shows "Rendering…" with no image, check the Xcode console — a `carapace_render_png` returning false means the skin folder reference or App Group container is wrong.

- [ ] **Step 8: Commit**

```bash
git add crates/embed-spike/widget-sample
git commit -m "spike(widget): SwiftUI app renders carapace states into the App Group"
```

---

## Task 4: WidgetKit extension that displays the rendered bitmap

A real home-screen widget loads the current-state PNG from the App Group.

**Files:**
- Create: `widget-sample/Widget/` (Widget Extension target `CarapaceWidgetExtension`).
- Create: `widget-sample/Widget/CarapaceWidget.swift`

**Interfaces:**
- Consumes: `AppGroup` (Task 3) — add the `Shared/AppGroup.swift` file to the widget target too.
- Produces: a static-state widget on the home screen; `CarapaceEntry`/`Provider` reused by Task 5.

- [ ] **Step 1: Add the Widget Extension target**

Xcode ▸ File ▸ New ▸ Target ▸ **Widget Extension**. Name `CarapaceWidgetExtension`. Uncheck "Include Live Activity" and "Include Configuration Intent" for now. Same personal team.

- [ ] **Step 2: Share the App Group with the widget target**

Widget target ▸ Signing & Capabilities ▸ + App Groups ▸ check `group.carapace.spike`. Add `Shared/AppGroup.swift` to the widget target's membership (File Inspector ▸ Target Membership).

- [ ] **Step 3: Write the widget loading the PNG**

Replace the generated widget file with `widget-sample/Widget/CarapaceWidget.swift`:

```swift
import WidgetKit
import SwiftUI

struct CarapaceEntry: TimelineEntry {
    let date: Date
    let state: Int
    let image: UIImage?
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> CarapaceEntry {
        CarapaceEntry(date: Date(), state: 0, image: nil)
    }
    func getSnapshot(in context: Context, completion: @escaping (CarapaceEntry) -> Void) {
        completion(entry())
    }
    func getTimeline(in context: Context, completion: @escaping (Timeline<CarapaceEntry>) -> Void) {
        completion(Timeline(entries: [entry()], policy: .never))
    }
    private func entry() -> CarapaceEntry {
        let s = AppGroup.currentState()
        let img = UIImage(contentsOfFile: AppGroup.pngURL(state: s).path)
        return CarapaceEntry(date: Date(), state: s, image: img)
    }
}

struct CarapaceWidgetView: View {
    var entry: CarapaceEntry
    var body: some View {
        ZStack {
            if let img = entry.image {
                Image(uiImage: img).resizable().scaledToFit()
            } else {
                Text("no render")
            }
        }
        .containerBackground(.black, for: .widget)
    }
}

@main
struct CarapaceWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "CarapaceWidget", provider: Provider()) { entry in
            CarapaceWidgetView(entry: entry)
        }
        .configurationDisplayName("Carapace")
        .description("A carapace skin rendered to a widget.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}
```

- [ ] **Step 4: Build & run; add the widget to the home screen**

Run the app target once (so the PNGs exist), then run the **widget** scheme (or long-press the simulator home screen ▸ + ▸ Carapace ▸ add).
Expected: the widget tile shows the carapace skin bitmap (green bar on black). Screenshot it — this is the core success-bar evidence (render in a real widget).

- [ ] **Step 5: Commit**

```bash
git add crates/embed-spike/widget-sample
git commit -m "spike(widget): WidgetKit extension displays the App Group bitmap"
```

---

## Task 5: AppIntent button — discrete bitmap swap

Prove WidgetKit's interactive-button → timeline-reload loop swaps the displayed bitmap.

**Files:**
- Create: `widget-sample/Widget/BumpIntent.swift`
- Modify: `widget-sample/Widget/CarapaceWidget.swift` (add the button)

**Interfaces:**
- Consumes: `AppGroup.currentState/setState`, `WidgetCenter`.
- Produces: a tappable widget that advances state and reloads.

- [ ] **Step 1: Write the AppIntent**

Create `widget-sample/Widget/BumpIntent.swift`:

```swift
import AppIntents
import WidgetKit

struct BumpIntent: AppIntent {
    static var title: LocalizedStringResource = "Bump"

    func perform() async throws -> some IntentResult {
        let next = (AppGroup.currentState() + 1) % AppGroup.stateCount
        AppGroup.setState(next)
        WidgetCenter.shared.reloadTimelines(ofKind: "CarapaceWidget")
        return .result()
    }
}
```

- [ ] **Step 2: Add the button to the widget view**

In `CarapaceWidget.swift`, change `CarapaceWidgetView`'s body to overlay a bump button:

```swift
struct CarapaceWidgetView: View {
    var entry: CarapaceEntry
    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            if let img = entry.image {
                Image(uiImage: img).resizable().scaledToFit()
            } else {
                Text("no render")
            }
            Button(intent: BumpIntent()) {
                Text("state \(entry.state) ▸")
                    .font(.caption2).padding(4)
            }
            .buttonStyle(.plain)
            .background(.white.opacity(0.15))
        }
        .containerBackground(.black, for: .widget)
    }
}
```

Add `BumpIntent.swift` to the widget target membership.

- [ ] **Step 3: Build & run; verify the swap**

Run the app once (PNGs exist), add the widget, then tap the "state N ▸" button repeatedly.
Expected: each tap advances the state label and the bar visibly changes width (cycling 0→1→2→3→0). Capture a before/after screenshot — this is the discrete-interactivity success-bar evidence.
If the bitmap does not change: confirm both targets share `group.carapace.spike`, the app wrote all four PNGs, and the widget kind string matches in both `reloadTimelines(ofKind:)` and `StaticConfiguration(kind:)`.

- [ ] **Step 4: Commit**

```bash
git add crates/embed-spike/widget-sample
git commit -m "spike(widget): AppIntent button swaps the widget bitmap (discrete interactivity)"
```

---

## Task 6 (stretch): Attempt rendering inside the widget extension

Probe the ambitious claim: can the extension itself run carapace within its memory budget?

**Files:**
- Modify: `widget-sample/Widget/CarapaceWidget.swift` (Provider) — temporary probe.

**Interfaces:**
- Consumes: `carapace_render_png` (link `libembed_spike.a` + bridging header into the **widget** target).

- [ ] **Step 1: Link the lib into the widget target**

Widget target ▸ Build Phases ▸ Link `libembed_spike.a`; set its Bridging Header / Header Search Path to reach `carapace.h` (mirror Task 3 Step 3 for this target).

- [ ] **Step 2: Render in the provider instead of loading a file**

In `Provider.entry()`, before loading the image, render directly to a temp file in the extension and load that:

```swift
private func entry() -> CarapaceEntry {
    let s = AppGroup.currentState()
    var img: UIImage?
    if let skin = Bundle.main.url(forResource: "skin", withExtension: nil)?.path {
        let tmp = NSTemporaryDirectory() + "ext-render.png"
        let level = Double(s) / Double(AppGroup.stateCount - 1)
        if carapace_render_png(skin, 240, 80, level, tmp) {
            img = UIImage(contentsOfFile: tmp)
        }
    }
    if img == nil { img = UIImage(contentsOfFile: AppGroup.pngURL(state: s).path) } // fallback
    return CarapaceEntry(date: Date(), state: s, image: img)
}
```

(Note: the widget's own bundle must also contain the `skin` folder reference, or point at the App Group copy.)

- [ ] **Step 3: Run and record the outcome (no commit of the probe expected)**

Run the widget. Observe whether it renders or the extension is jetsam-killed (Xcode console / Console.app "memory limit" / "jetsam").
Expected outcome is **unknown — that is the experiment.** Record exactly what happens (renders fine / killed at what point / wgpu init failure) for the findings doc. Then **revert** the probe (`git checkout -- crates/embed-spike/widget-sample/Widget/CarapaceWidget.swift`) so the shipped sample keeps the robust app-render path.

- [ ] **Step 4 (only if it worked): Commit behind a clear note**

If in-extension render worked, you may keep it under an `#if` or a comment documenting it as the stretch result. Otherwise leave the reverted app-render path as the deliverable.

---

## Task 7: Findings doc + engine-untouched check

Capture the verdict and prove the engine was never touched.

**Files:**
- Create: `docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-findings.md`

- [ ] **Step 1: Verify zero engine diff**

Run: `git diff --stat main...HEAD -- crates/carapace/src crates/hittest/src`
Expected: empty output. If not, move the offending change out of the engine crates.

- [ ] **Step 2: Write the findings doc**

Create `docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-findings.md` in the style of `2026-06-25-host-embedding-spike-findings.md`. Cover, with evidence (screenshots/log excerpts referenced):
- Headline go/no-go for carapace-as-home-screen-widget.
- What worked: one-shot `carapace_render_png` (host test result), app-render → App Group → widget display, AppIntent discrete swap.
- The four spec risks, each with the actual observed result: App Group in the simulator; one-shot wgpu/Metal render cost/correctness; **Task 6 stretch** (in-extension render — the key data point); staticlib + bridging-header wiring.
- Known limits left open (no device build, no Tier-2, PNG-file round-trip latency, state count fixed at 4).
- Recommendation for a real `carapace-widgets` crate (or against it).

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-findings.md
git commit -m "docs(spike): WidgetKit offscreen-bitmap findings — go/no-go + iOS gotchas"
```

---

## Self-Review (completed during planning)

- **Spec coverage:** success-bar #1 (app render) → Tasks 1,3; #2 (widget display from App Group) → Task 4; #3 (AppIntent discrete swap) → Task 5; stretch (in-extension render) → Task 6; findings doc → Task 7; zero-engine-diff constraint → Task 7 Step 1; staticlib/cfg-widening → Tasks 1,2. All spec sections map to a task.
- **Placeholder scan:** every code step shows complete code or exact Xcode settings; no TBD/TODO. Xcode GUI steps are inherently manual but give exact menu paths, identifiers, and acceptance checks.
- **Type consistency:** `carapace_render_png(skin_dir, w, h, state, out_path) -> bool` is identical across `oneshot.rs`, `carapace.h`, and every Swift call site; `AppGroup` API (`pngURL(state:)`, `currentState()`, `setState(_:)`, `stateCount`, `id`) is used consistently in Tasks 3/4/5; widget kind string `"CarapaceWidget"` matches between `StaticConfiguration` and `reloadTimelines(ofKind:)`.

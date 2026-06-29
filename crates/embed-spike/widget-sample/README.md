# CarapaceWidgetSpike — carapace skin in an iOS home-screen widget

Proves a carapace skin can render to an offscreen bitmap shown in a real WidgetKit
home-screen widget, with an `AppIntent` button discretely swapping the displayed
bitmap. Part of the WidgetKit offscreen-bitmap spike
(`docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-*`).

## Layout

| Path | Role |
|------|------|
| `project.rb` | Generates `CarapaceWidgetSpike.xcodeproj` (app + widget targets) via the `xcodeproj` gem. Single source of truth — the `.xcodeproj` is gitignored. |
| `App/` | SwiftUI container app. Renders the 4 state PNGs into the App Group, with a debug preview. Bridges to `carapace_render_png` via `Bridging-Header.h`. |
| `App/Seeded/` | Host-rendered fallback PNGs (`cargo run -p embed-spike --example seed_states`). Used in the Simulator, where the live render can't run (see below). |
| `Widget/` | WidgetKit extension: `TimelineProvider`, widget view, and `BumpIntent`. |
| `Shared/AppGroup.swift` | App Group id + PNG/state path helpers, compiled into both targets. |
| `Vendor/` | `carapace.h` + the `libembed_spike.dylib` you build (gitignored). |
| `evidence/` | Screenshots captured during the spike. |

## The Simulator caveat (important)

Carapace's renderer (Vello) requires GPU **`INDIRECT_EXECUTION`** (indirect compute
dispatch). The **iOS Simulator's Metal does not support it**, so the live in-app
render (`carapace_render_png`) **fails in the Simulator** and works only on a real
device (and on the macOS host — the Rust test passes there). To still demo the full
WidgetKit pipeline in the Simulator, the app bundles host-rendered PNGs under
`Seeded/` and `CarapaceBridge` falls back to seeding them into the App Group when the
live render fails. `ContentView` reports which path ran (`live GPU render` vs
`seeded (Simulator fallback)`).

The app also requested wgpu's **default device limits**, which exceed the Simulator
adapter (`max_inter_stage_shader_variables` 16 vs 15); the engine fix requests
`adapter.limits()` instead. The `INDIRECT_EXECUTION` gap is the deeper, unavoidable
one.

## Build & run

```sh
# 1. Build the iOS-simulator cdylib (from repo root) and set its install name.
cargo build -p embed-spike --target aarch64-apple-ios-sim --release
install_name_tool -id @rpath/libembed_spike.dylib \
  target/aarch64-apple-ios-sim/release/libembed_spike.dylib
cp target/aarch64-apple-ios-sim/release/libembed_spike.dylib \
  crates/embed-spike/widget-sample/Vendor/

# 2. (Re)generate the host-rendered fallback PNGs.
cargo run -p embed-spike --example seed_states

# 3. Generate the Xcode project.
cd crates/embed-spike/widget-sample && ruby project.rb

# 4. Build, install, run in the Simulator.
xcodebuild -project CarapaceWidgetSpike.xcodeproj -scheme CarapaceWidgetSpike \
  -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build
xcrun simctl install booted "$(find ~/Library/Developer/Xcode/DerivedData/CarapaceWidgetSpike-*/Build/Products/Debug-iphonesimulator -maxdepth 1 -name '*.app' | head -1)"
xcrun simctl launch booted com.carapace.spike
```

Then long-press the home screen ▸ `+` ▸ search **Carapace** ▸ add the Small widget.
Tap its **"state N ▸"** button to cycle the bitmap (0→1→2→3→0).

## Why a cdylib, not a staticlib

The plan called for a staticlib, but linking it leaks mlua's crate-private symbols as
cross-object local references the system linker can't resolve (`error_tostring`,
`do_call`, …). rustc links the cdylib fully and exports only the public C ABI, so the
sample embeds `libembed_spike.dylib` (`@rpath`, code-signed on copy) instead.

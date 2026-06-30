# CarapaceWidgetSpike — a carapace skin rendering live data in an iOS widget

Proves a carapace skin can render to an offscreen bitmap shown in a real WidgetKit home-screen
widget — and, crucially, that the skin can display **live information** (now-playing track, artist,
elapsed time, seek position) rendered natively by carapace from host data. Part of the WidgetKit
offscreen-bitmap spike (`docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-*`).

## Layout

| Path | Role |
|------|------|
| `project.rb` | Generates `CarapaceWidgetSpike.xcodeproj` (app + widget targets) via the `xcodeproj` gem. Single source of truth — the `.xcodeproj` is gitignored. |
| `App/` | SwiftUI container app. Renders the "Now Playing" skin (with live data) into the App Group, with a debug preview. Bridges to `carapace_render_info` via `Bridging-Header.h`. |
| `App/Seeded/` | Host-rendered fallback PNG (`cargo run -p embed-spike --example seed_states`). Used in the Simulator, where the live render can't run (see below). |
| `Widget/` | WidgetKit extension: `TimelineProvider` + the SwiftUI widget view. |
| `Shared/AppGroup.swift` | App Group id + path helper, compiled into both targets. |
| `Vendor/` | `carapace.h` + the `libembed_spike.dylib` you build (gitignored). |
| `evidence/` | Screenshots captured during the spike. |

The rendered skin is `crates/embed-spike/skin-nowplaying/` — a compact faceplate using carapace's
data binding: `text{ value = "track" }` pulls a host **string**, `value_fill{ value = "position" }`
pulls a host **number**. The host (the app, or on a real device the player) supplies the data via
`carapace_render_info(skin, w, h, n, keys, vals, out)`.

## The Simulator caveat (important)

Carapace's renderer (Vello) requires GPU **`INDIRECT_EXECUTION`** (indirect compute dispatch). The
**iOS Simulator's Metal does not support it**, so the live in-app render fails in the Simulator and
works only on a real device (and on the macOS host — the Rust test passes there). To still demo the
WidgetKit pipeline in the Simulator, the app bundles a host-rendered `Seeded/nowplaying.png` and
`CarapaceBridge` falls back to it when the live render fails. `ContentView` reports which path ran.

The render also requested wgpu's **default device limits**, which exceed the Simulator adapter
(`max_inter_stage_shader_variables` 16 vs 15); the fix requests `adapter.limits()` instead.

## Build & run

```sh
# 1. Build the iOS-simulator cdylib (from repo root) and set its install name.
cargo build -p embed-spike --target aarch64-apple-ios-sim --release
install_name_tool -id @rpath/libembed_spike.dylib \
  target/aarch64-apple-ios-sim/release/libembed_spike.dylib
cp target/aarch64-apple-ios-sim/release/libembed_spike.dylib \
  crates/embed-spike/widget-sample/Vendor/

# 2. (Re)generate the host-rendered fallback PNG.
cargo run -p embed-spike --example seed_states

# 3. Generate the Xcode project.
cd crates/embed-spike/widget-sample && ruby project.rb

# 4. Build, install, run in the Simulator.
xcodebuild -project CarapaceWidgetSpike.xcodeproj -scheme CarapaceWidgetSpike \
  -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build
xcrun simctl install booted "$(find ~/Library/Developer/Xcode/DerivedData/CarapaceWidgetSpike-*/Build/Products/Debug-iphonesimulator -maxdepth 1 -name '*.app' | head -1)"
xcrun simctl launch booted com.carapace.spike
```

Then long-press the home screen ▸ `+` ▸ search **Carapace** ▸ add the widget. The tile shows the
"Now Playing" card rendered by carapace. On a real device the Provider would call
`carapace_render_info` with the *current* track each timeline reload.

## Notes from the spike

- **Why a cdylib, not a staticlib** — linking the staticlib leaks mlua's crate-private symbols as
  cross-object local references the system linker can't resolve; rustc links the cdylib fully and
  exports only the public C ABI, so the sample embeds `libembed_spike.dylib` (`@rpath`, signed on
  copy).
- **Transparency / floating skins** — shaped skins render with a transparent background
  (`carapace_render_png` preserves alpha), so a skin floats with no opaque box. But iOS does **not**
  let third-party home-screen widgets show the wallpaper through them (`Color.clear` → a system dark
  material); the "faux transparency" trick (a crop of the wallpaper as the widget background) works
  mechanically but needs the user's own bare-wallpaper capture to align. The "Now Playing" skin
  sidesteps this by being a self-contained card.
- **Interactivity** — an earlier iteration proved an `AppIntent` button discretely swapping the
  rendered bitmap; see the git history (`BumpIntent`).

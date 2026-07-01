# Flutter iOS Host-Embedding Spike — Findings (2026-06-30 → 2026-07-01)

Throwaway feasibility spike, third in the host-integration line (after the native macOS embed spike
and the iOS WidgetKit spike). Crate: the existing `crates/embed-spike` (extended for iOS) + a new
Flutter app under `crates/embed-spike/flutter-sample/`. Platform tested: **iPhone 16 Pro Max, iOS
27, real device** (driven by `xcodebuild` + `xcrun devicectl` — XcodeBuildMCP here is Simulator-only).
Branch: `flutter-ios-embed-spike`.

## Headline: Flutter-as-carapace-host is FEASIBLE — proven on device

A Flutter iOS app embeds the carapace engine over the existing flat **C ABI**, is its **Host** (Swift
owns the state + actions via the vtable), and displays the engine's live render through a Flutter
**external `Texture`** backed by an IOSurface `CVPixelBuffer`. Beyond the base question, the final
demo realizes the founding **"skin wraps the host's content"** vision in a *foreign* host: a
carapace-rendered **medieval stone doorway** (vector masonry: voussoir arch + keystone, ashlar
jambs, a lit wall torch) whose arched opening is **transparent**, framing a **real Flutter music
player** (`audioplayers`, actual playback + seek + position) that shows through it. And the loop
closes both ways: Flutter pushes a **flame level** to the skin's host `lit` value ~12×/s — a
beat-pulse (synced to the real playback position + tempo) plus a candle flicker — so the **stone
torch visibly reacts to the music**. Content→skin AND skin-frames-content, live. Evidence:
`flutter-sample/evidence/0{1,2,3}-*.png`.

## What was built (per layer)

- **`crates/embed-spike` — iOS port of the live-engine C ABI.** The one-shot render path
  (`carapace_render_png/_info`) was already iOS-portable (from the widget spike), but the *live*
  engine (`carapace_create/tick/pointer/active_tier/destroy` in `mod ffi_impl`) plus its IOSurface
  helpers in `render.rs` were **macOS-gated**. Widened the `cfg(target_os = "macos")` gates to
  `any(macos, ios)` and fixed the deltas (below). **Still zero engine-crate diff** — only
  `crates/embed-spike` changed; `git diff main -- crates/carapace/src crates/hittest/src` is empty.
- **`flutter-sample/ios/Runner/CarapaceBridge.swift`** — a `FlutterTexture` that owns an
  IOSurface-backed BGRA8 `CVPixelBuffer`, hands its `IOSurfaceRef` to `carapace_create` as the render
  target, serves it back via `copyPixelBuffer`, and IS the Host (the `lit` value + `toggle` action).
- **`AppDelegate.swift`** — registers the texture + a `carapace` `FlutterMethodChannel`, forwards
  taps to `carapace_pointer`.
- **`lib/main.dart`** — Stacks the skin `Texture` under a Flutter UI `Positioned` in the arch
  opening; the animated music player.
- **`skin-frame/`** — the vector medieval-door skin. **Linkage: the cdylib, not the staticlib**
  (same mlua-symbol reason as the widget), vendored + `@rpath` + CodeSignOnCopy via `wire_carapace.rb`.

## The iOS deltas fixed the hard way (the real value)

1. **`io-surface 0.16` won't link for iOS — it drags in the macOS-only OpenGL framework.** Widening
   the cfg gates surfaced `ld: framework 'OpenGL' not found`: `io-surface` → `cgl` →
   `-framework OpenGL`, which does not exist on iOS. Fix: **drop the `io-surface` crate** and declare
   the four IOSurface functions we use (`IOSurfaceLock/Unlock/GetBaseAddress/GetBytesPerRow`, + width/
   height) directly from the system **`IOSurface.framework`** (present on both macOS and iOS) via a
   small `#[link(name="IOSurface", kind="framework")] extern "C"` block. `io-surface` stays only as a
   **macOS-only** dep for the one host-side `iosurface_png` example. This is the "move off the
   deprecated crate" the macOS findings recommended — now forced by iOS.
2. **VSyncClient startup segfault (`EXC_BAD_ACCESS` in `-[VSyncClient initWithTaskRunner:callback:]`).**
   Creating the wgpu/Metal device (`carapace_create`) **synchronously in
   `didInitializeImplicitFlutterEngine`** races Flutter's own engine/VSync init and intermittently
   segfaults it (white screen / crash-on-launch). Fix: **defer the entire carapace setup ~0.3 s past
   startup** (`DispatchQueue.main.asyncAfter`) and have **Dart retry `textureId`** until the channel +
   texture exist (a blanket defer alone caused `MissingPluginException`).
3. **Per-frame Tier-1 readback starves Flutter's main thread.** A `CADisplayLink` doing a synchronous
   GPU render + full readback **every frame** at the 720×1280 surface pegged the main thread and
   Flutter never drew its first frame (white screen). The macOS findings warned of exactly this. Fix:
   **on-demand rendering** — a `needsRender` dirty flag set at init + after each pointer event; the
   display link only does the expensive tick when set. Driving the music-reactive flame re-renders
   the whole door at **~12 fps** and stays smooth; **60 fps** is what starved Flutter. So the
   practical ceiling for the current main-thread Tier-1 path is a low-rate animated element;
   full-rate animated skins will need a dedicated render thread or a Tier-2 present.
4. **The skin Lua sandbox exposes no `math`.** The medieval arch geometry used `math.cos/sin/rad`,
   which are nil in the skin sandbox (only vocab globals + `host` are exposed; `io`/`os`/`math`/… are
   withheld). Worked around with a **precomputed cos/sin table** over fixed 20° steps (9 voussoirs).
   → motivated a follow-up: expose the (capability-free) `math` table to skins as its own engine PR.

## Tooling notes (device, headless)

- **`flutter run` is blocked over a wireless device by the macOS Local Network privacy permission**
  (`SocketException … port 5353`). Install/launch instead with `xcrun devicectl device
  install/process launch`; the app runs standalone (debug build).
- **Crash logs without root:** `xcrun devicectl device copy from --domain-type systemCrashLogs` pulls
  `Runner-*.ips` (JSON) to the Mac — the backtrace is how the VSyncClient race was diagnosed.
- Automatic signing under a paid team; `aarch64-apple-ios` (device) cdylib, `install_name_tool
  -id @rpath/…`.

## Verdict / recommendation

Pursue Flutter as a first-class carapace host — it is proven. For a real layer: a **Flutter plugin**
wrapping a stable `carapace-ffi`; a **dedicated render thread** owning the engine (the current
main-thread Tier-1 loop is the perf ceiling and the source of the starvation gotcha — fine for a
static frame skin, not for animation); a **Tier-2 zero-copy** present into the `CVPixelBuffer`'s
IOSurface (Metal, as on macOS) to drop the readback. The "skin frames arbitrary host content" model
maps cleanly onto the engine's `view{}` cutout / undrawn-transparent regions + Flutter Stack layering.

## Known limits left open (deliberately)

- Content is layered in Flutter (skin `Texture` under a `Positioned` UI in the opening); a strict
  "skin-on-top, content-behind-showing-through-a-transparent-cutout" compositing test (does the
  external texture alpha-blend over Flutter content *below* it) was not run — the layered result
  looks identical and met the goal.
- Tier-1 readback only (no Tier-2 on iOS yet); on-demand render only (no animated skins).
- One demo skin; signing team id is baked into the checked-in `Runner.xcodeproj` (spike convenience).

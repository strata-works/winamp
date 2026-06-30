# WidgetKit Offscreen-Bitmap Spike — Findings (2026-06-29)

Throwaway feasibility spike. Crate: `crates/embed-spike` (one-shot C ABI added to the existing
host-embedding cdylib) plus an iOS sample under `crates/embed-spike/widget-sample/` (SwiftUI app
+ WidgetKit extension + AppIntent). Platform tested: **iOS 26.5 Simulator, iPhone 17 Pro, Apple
silicon** + the **macOS host** for the render unit test. Branch: `widgetkit-bitmap-spike`.

## Headline: carapace-in-a-home-screen-widget WORKS — with one device-only caveat

A carapace skin renders to an offscreen PNG, is shared through an **App Group**, and is displayed
in a **real home-screen WidgetKit tile**, with an **AppIntent button** discretely swapping the
displayed bitmap. All three success-bars were met in the Simulator (screenshots in
`widget-sample/evidence/`).

## Follow-on (the part that makes a widget worth shipping): LIVE DATA, full-bleed

A static bitmap is a poster, not a widget. The decisive follow-on result: a carapace skin renders
**live information** natively. Carapace already supports data binding — `text{ value = "key" }`
reads a host **string** (`StateValue::Str`), `value_fill{ value = "key" }` reads a host **number**.
A new `carapace_render_info(skin, w, h, n, keys, vals, out)` feeds a key→value map to an `InfoHost`
(values that parse as numbers → `Scalar`, else `Str`). The sample's `skin-nowplaying` renders a
"Now Playing" card — track / artist / elapsed-time text + a seek bar — from host data
(`evidence/14-app-nowplaying.png`, `16-widget-fill.png`). On a real device the Provider would call
`carapace_render_info` with the *current* track each timeline reload.

It also **fills the entire widget**: render the skin edge-to-edge and use it as the widget's
`containerBackground` with `.scaledToFill()` — no black margins, the system applies the rounded
mask. Best fit when the skin canvas matches the family aspect (the wide card → `systemMedium`).

**Live data is dynamic, proven end-to-end.** `skin-clock` rendered the *actual system time* at six
successive moments (`16:56:02 → :05 → :07 → :09 → :11 → :12`, seconds-sweep bar advancing) — the
output changes every render (`evidence/17-live-clock.gif`, via `cargo run -p embed-spike --example
live_clock`). So the data → bind → render → bitmap loop is genuinely live, not a one-off.
**Caveat — where live rendering can run:** on a real **device** the widget's `TimelineProvider`
calls `carapace_render_info` with the current data per timeline entry, so the tile updates live
(subject to WidgetKit's refresh budget — per-minute is fine, the model Apple's own clock uses). In
the **Simulator** carapace cannot render at runtime (the INDIRECT_EXECUTION gap), so the tile shows
a pre-baked frame and looks static there; the live loop is demonstrated on the host instead. A
device build is the way to show the widget itself ticking.

Transparency note: shaped skins render with real alpha (`carapace_render_png` preserves it; clear
`base_color`), so a skin *floats* with no opaque box (`evidence/11-floating-proof.png`). But iOS
does **not** let third-party home-screen widgets show the wallpaper through them (`Color.clear` → a
system dark material); the "faux-transparency" wallpaper-crop trick works mechanically but needs the
user's own bare-wallpaper capture to align — you can't reconstruct it from iOS's layered wallpaper
assets. A self-contained card skin (like Now Playing) sidesteps this.

The one caveat is the most important finding: **carapace's renderer (Vello) cannot run in the iOS
Simulator** — it needs a GPU capability the Simulator's Metal does not expose (see Risk 2). So the
*live* render runs on the **macOS host** (unit test passes) and is expected to run on a **real
device**, but in the Simulator the app falls back to host-rendered PNGs to drive the rest of the
pipeline. Everything *above* the render — App Group sharing, widget display, AppIntent swap — is
proven end-to-end in the Simulator and is renderer-agnostic.

**Zero engine change.** `git diff main...HEAD -- crates/carapace/src crates/hittest/src` is
**empty**. All new code is in `crates/embed-spike/` (the one-shot `carapace_render_png`, a
platform re-gating of `render.rs`, and a one-line device-limits fix) and the new sample.

## What was built

- **`oneshot.rs` / `carapace_render_png(skin_dir, w, h, state, out_path) -> bool`** — a stateless
  headless render: load skin → `Engine` with a minimal `OneShotHost` serving `state` under key
  `"level"` → offscreen render → `readback_rgba` → `image::save_buffer` PNG. Never panics across
  the FFI boundary (`catch_unwind`). Host unit test (`tests/render_png.rs`) passes: low vs high
  state drives the bound bar's pixel count.
- **`render.rs` re-gating** — `init_gpu`/`new_offscreen`/`render_frame`/`readback_rgba` now build
  on iOS too; only the IOSurface helpers stay `#[cfg(target_os = "macos")]`.
- **SwiftUI app** — renders states `0..3` into the App Group (live on device, host-seeded PNG
  fallback in the Simulator), with a debug preview that reports which path ran.
- **WidgetKit extension** — `TimelineProvider` loads the current-state PNG into a SwiftUI `Image`;
  a `BumpIntent` advances the App Group state and calls `reloadTimelines`.

## The four spec risks — observed results

### Risk 1 — App Group in the Simulator: ✅ WORKS, no device provisioning
`group.carapace.spike` on both targets, ad-hoc signed (`CODE_SIGN_IDENTITY = -`,
`CODE_SIGNING_REQUIRED = YES`). `containerURL(forSecurityApplicationGroupIdentifier:)` resolves in
both the app and the extension to the *same* path; the app writes PNGs, the extension reads them.
os_log confirms cross-process sharing:
`Provider.entry: state=2 png=…/state-2.png loaded=true`.
Gotcha: simulator entitlements land in a separate `*-Simulated.xcent` (so `codesign -d
--entitlements` shows an empty dict — that is normal and not a failure). `CODE_SIGNING_REQUIRED =
NO` silently drops entitlement processing and the container never resolves — it must be `YES`.

### Risk 2 — one-shot wgpu/Metal render: ⚠️ CORRECT on host, BLOCKED in the Simulator
On the macOS host the one-shot render is correct (test passes; the seeded PNGs show the expected
0/⅓/⅔/full green bar). In the **iOS Simulator** it fails — two layers:
1. `request_device` rejected wgpu's **default limits** (`max_inter_stage_shader_variables` 16 vs
   the Simulator adapter's 15). Fixed by requesting `adapter.limits()` (always satisfiable, ≥
   defaults on real GPUs; host path unaffected).
2. After that, device creation succeeds but the **Vello** pipeline fails:
   `Device::create_buffer 'vello.reduced_buf': Downlevel flags INDIRECT_EXECUTION are required but
   not supported on the device.` The Simulator's Metal does not support GPU **indirect execution**
   (indirect compute dispatch), which Vello requires. This is a **Simulator-only** gap — real
   Apple-GPU devices support it. No engine-side workaround exists without changing the renderer
   (out of scope). **Cost** was not measurable in-Sim; on the host a cold one-shot (wgpu instance
   + device + Vello renderer + 240×80 render + readback) is sub-second per call (4 states seed
   well under a second total).

### Risk 3 — stretch, GPU-in-extension: ⚠️ UNANSWERABLE in the Simulator, but the extension survives
With `WIDGET_RENDER_PROBE=1` the cdylib links into the **widget extension** and `Provider.entry()`
attempts a live render. Result (os_log): `Provider PROBE: in-extension render ok=false`,
immediately followed by the graceful fallback `Provider.entry: state=3 … loaded=true`. The
extension **did not crash and was not jetsam-killed** — `carapace_render_png` caught the
`INDIRECT_EXECUTION` failure internally and returned `false`. So:
- Linking carapace into an app-extension target **works** (dylib embeds, bridging header resolves,
  symbol calls across the C ABI succeed).
- The **~30 MB memory-budget question is moot in the Simulator** — the render dies at GPU init
  (a feature gap), long before any heavy Vello allocation, so we never learn whether a *successful*
  render fits the budget. **That requires a real device.**

### Risk 4 — build wiring: ✅ WORKS, with one substitution
Two targets (app + app-extension) sharing the App Group, a bridging header, and the skin folder
reference all wired up — but via the **cdylib, not the staticlib** the plan assumed. Linking the
`staticlib` leaks mlua's crate-private symbols as cross-object **local** references the system
linker cannot resolve (`error_tostring`, `protect_lua_closure::do_call`, …); neither
`-force_load`, `codegen-units=1`, nor fat LTO fixed it (LTO made it worse — it stripped the
symbols entirely). rustc links the **cdylib** fully and exports only the public C ABI, so the
sample embeds `libembed_spike.dylib` (`@rpath`, code-signed on copy). The Xcode project is
generated reproducibly from `project.rb` (the `xcodeproj` Ruby gem) rather than hand-built in the
GUI. Other wiring gotcha: an app-extension needs `CFBundleVersion`/`CFBundleShortVersionString`
(set `MARKETING_VERSION` + `CURRENT_PROJECT_VERSION`) or install fails with "Failed to create app
extension placeholder", and the App Intents SSU step needs `GENERATE_INFOPLIST_FILE = YES` even
with an explicit `INFOPLIST_FILE`, or it errors with "Unable to parse Info.plist".

## Evidence

- `widget-sample/evidence/02-app-seeded.png` — app shows the carapace bitmap + "seeded (Simulator
  fallback)".
- `…/03-states-montage.png` — the 4 discrete state bitmaps the widget cycles through.
- `…/06-widget-state2.png` / `07-widget-state3.png` — the home-screen tile before/after a button
  tap (⅔ bar → full bar), the AppIntent swap.
- os_log (subsystem `com.carapace.spike`): `BumpIntent: 2 -> 3` → `Provider.entry: state=3 …
  loaded=true`; probe: `in-extension render ok=false`.

## Known limits left open

- **No device build.** The live render is proven on the macOS host only; the device path
  (INDIRECT_EXECUTION present, the in-extension memory budget) is untested — it needs real signing.
- **No Tier-2 / IOSurface.** This spike is CPU-readback PNG round-trip only; fine for a widget
  (static, infrequently reloaded) but it is the perf floor, not direct-to-surface.
- **PNG-file round-trip latency** between app render and widget display (acceptable for widgets;
  WidgetKit reloads are budgeted/coalesced anyway).
- **State count fixed at 4**, levels hard-mapped `i/3`.

## Recommendation: a `carapace-widgets` crate is viable — gate it on a device render check

The plumbing is sound and renderer-agnostic, and it reuses the existing host-embedding cdylib with
**zero engine change**. The whole product hinges on **one untested fact**: that Vello's
`INDIRECT_EXECUTION` path runs on real iOS/iPadOS devices (and, for the stretch, that a render fits
the extension's memory budget). **Before committing to `carapace-widgets`, run exactly this sample
on a physical device** (a paid team for App Group provisioning, or the personal-team path) and
confirm (a) the live `carapace_render_png` succeeds on-device and (b) whether in-extension render
survives the budget. If (a) holds — very likely — ship the **app-renders-into-App-Group** model
(robust, what this sample does) and treat in-extension render as an optimization probed per-OS.
Carry forward the cdylib-not-staticlib decision and the `project.rb` generator.

# WidgetKit Offscreen-Bitmap Spike — Design (2026-06-29)

Throwaway feasibility spike, sibling to the Flutter iOS spike
(`docs/superpowers/specs/2026-06-29-flutter-ios-embed-spike-design.md`). Both descend from the
native macOS host-embedding spike (`crates/embed-spike`, merged #21). This one targets the
`carapace-widgets` direction: a carapace skin rendered as an **offscreen bitmap** displayed in an
iOS **home-screen widget**.

Prior findings: `docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md`.

## Why this is a different beast (and not Flutter)

iOS home-screen widgets (WidgetKit) are **SwiftUI, out-of-process, snapshot-based**. The widget
runs in a system extension with a tight memory budget (~30 MB) that is killed quickly. There is
**no render loop, no `CADisplayLink`, no Metal animation, and no Flutter engine** inside it. The
render model is "produce a bitmap, hand it to SwiftUI `Image`." "Interactive" means discrete: since
iOS 17, `Button`/`Toggle` backed by an `AppIntent` whose `perform()` reloads the timeline — not a
continuous input stream. This spike therefore shares only the engine (the `embed-spike` iOS
staticlib) with the Flutter spike; everything above the C ABI is different.

## Goal & success bar

A minimal **SwiftUI container app + WidgetKit extension**, running in the **iOS Simulator**:

1. The **app** renders a carapace skin to a PNG via a one-shot headless FFI call.
2. That bitmap appears in a **real home-screen widget**, loaded from a shared **App Group**
   container.
3. An **AppIntent button** in the widget triggers `WidgetCenter.reloadTimelines` and visibly swaps
   to a *different* carapace-rendered bitmap (from a pre-rendered state set) — proving WidgetKit's
   discrete-interaction wiring end to end.

**Stretch probe:** attempt rendering the bitmap *inside* the extension's `TimelineProvider`
(wgpu/Metal headless); record whether init + the ~30 MB budget allow it. A hard block here is a
valid (negative) finding, not a failure of the spike.

## The interactivity constraint (load-bearing)

A WidgetKit `AppIntent` button's `perform()` runs **in the extension process**, which — in the
app-render model — cannot render. So "tap → freshly rendered bitmap" is impossible in the primary
path. The honest proof of discrete interactivity: the app **pre-renders a small set of state
bitmaps** (states `0..N`) into the App Group at launch; the AppIntent bump writes the new state to
the App Group and calls `reloadTimelines`, so the widget's next entry loads a *different*
pre-rendered carapace bitmap. Proving live render-on-tap is exactly what the in-extension stretch
would unlock.

## Non-goals (explicitly out of scope)

- Live/continuous rendering or animation in the widget (WidgetKit does not allow it).
- Continuous touch/gesture input (only discrete AppIntent reloads exist).
- Android `AppWidget` (iOS/WidgetKit only this pass).
- Physical-device build, full provisioning, App Store entitlement review (simulator only).
- Any change to the engine crates (`crates/carapace`, `crates/hittest`) — preserve the
  "zero engine diff" property; the new FFI rides the existing `Renderer` public API.

## Architecture & data flow

```
SwiftUI app ──C ABI──► embed-spike (iOS staticlib): carapace_render_png(skin, w, h, state, out_path)
   │  writes PNG(s)  ──►  App Group container (group.carapace.spike)
   ▼
WidgetKit extension: TimelineProvider loads PNG from App Group → SwiftUI Image
   AppIntent "bump": write new state to App Group → WidgetCenter.reloadTimelines
                     → next timeline entry loads the other pre-rendered PNG
   (stretch) TimelineProvider calls carapace_render_png itself instead of loading a file
```

- **New one-shot FFI: `carapace_render_png`.** Reuses the engine's existing offscreen render +
  `readback_rgba` + a PNG encode. Distinct from the live `carapace_create`/`carapace_tick` path; it
  is stateless (skin dir + size + a state scalar in, PNG bytes/file out). No engine-crate change.
- The app pre-renders states `0..N` at launch into the App Group container; the widget selects and
  loads by current state. The skin is the same minimal value-bound skin used by the Flutter spike,
  so successive states are visibly different.

## Components & layout

- **`crates/embed-spike/`** (shared with the Flutter spike):
  - Add `carapace_render_png(skin_dir, w, h, state, out_path)` to the Rust C ABI and to
    `carapace.h`.
  - Widen `cfg(target_os = "macos")` gates to `any(target_os = "macos", target_os = "ios")` where
    portable; add `staticlib` to `crate-type`. (The iOS staticlib build is shared by both spikes.)
  - No engine-crate change.
- **`crates/embed-spike/widget-sample/`** (new iOS project):
  - Minimal **SwiftUI app** target: calls `carapace_render_png` for states `0..N`, writes PNGs into
    the App Group container.
  - **WidgetKit extension** target: `TimelineProvider` loading the PNG into a SwiftUI `Image`; an
    `AppIntent` "bump" updating shared state + `reloadTimelines`.
  - App Group entitlement (`group.carapace.spike`) on both targets; a bridging header for
    `carapace.h`; both link `libembed_spike.a`.
- **Skin:** the same minimal `value_fill`-driven skin as the Flutter spike.

## Build & integration plan (in order)

1. Add `carapace_render_png` to `embed-spike`; verify it headlessly on the host (macOS) first,
   producing a correct PNG, before any iOS work.
2. `rustup target add aarch64-apple-ios-sim` (shared with the Flutter spike); build the staticlib.
3. Create the iOS project under `widget-sample/`: SwiftUI app + WidgetKit extension; confirm a
   stock widget (static text) appears on the simulator home screen first.
4. Add the App Group to both targets; confirm the app can write and the extension can read a file
   in the shared container in the simulator.
5. Wire `carapace.h` + `libembed_spike.a` into the app target; render states `0..N` to PNGs at
   launch; show one in the widget.
6. Add the AppIntent button; confirm the bump swaps the displayed bitmap.
7. **Stretch:** move a render into the extension's `TimelineProvider`; record memory/init behavior.

## Testing & verification

Manual/visual, mirroring the other spikes:

- The widget shows the carapace skin on the simulator home screen (screenshot evidence).
- Tapping the AppIntent button swaps to a different carapace-rendered bitmap (before/after
  screenshots).
- `carapace_render_png` verified on the host first (a saved PNG opened and inspected).

Engine-untouched invariant checked as before:
`git diff --stat <base>...HEAD -- crates/carapace/src crates/hittest/src` must be empty.

## Top risks (each becomes a findings-doc data point)

1. **App Group entitlement in the simulator** — whether the shared container works without full
   device provisioning.
2. **One-shot headless wgpu/Metal render** — cost and correctness when spun up per-render from a
   plain app process (no persistent engine).
3. **Stretch: GPU-in-extension** — whether wgpu/Metal init and a render fit the extension's ~30 MB
   budget at all.
4. **Build wiring** — staticlib + bridging header into both an app and a widget-extension target.

## Deliverables

- Working iOS simulator build under `crates/embed-spike/widget-sample/`.
- Findings doc `docs/superpowers/specs/2026-06-29-widgetkit-bitmap-spike-findings.md` with the
  go/no-go verdict for carapace-as-home-screen-widget and the iOS/WidgetKit gotchas.

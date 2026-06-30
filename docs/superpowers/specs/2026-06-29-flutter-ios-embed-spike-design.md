# Flutter iOS Host-Embedding Spike — Design (2026-06-29)

Throwaway feasibility spike, second in the host-embedding line after the native macOS spike
(`crates/embed-spike`, branch `host-embedding-spike`, merged #21). Goal: prove a **Flutter iOS**
app can embed the carapace engine and round-trip input, reusing the engine's existing flat C ABI.

See the prior findings: `docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md`
(finding #6 explicitly greenlights a Flutter `Texture`-widget embedding off the IOSurface result).

## Goal & success bar

A Flutter iOS app running in the **iOS Simulator** must:

1. **Render** a carapace skin into a Flutter **external texture** — skin pixels visible on screen.
2. **Round-trip a tap**: Dart → Swift → engine → a Swift-implemented host action that mutates a
   host-owned value, with the skin visibly reflecting the change on the next frame.

**Tier-1 (CPU readback into the surface) is acceptable.** Re-proving Tier-2 zero-copy on iOS Metal
is an explicit non-goal (documented as follow-up). Output is throwaway: a working simulator build
plus a findings doc that gives a go/no-go for a real Flutter host and records iOS-specific gotchas.

A hard block on a core step (e.g. wgpu-Metal won't init under the simulator, or a
`CVPixelBuffer` won't bind as a Flutter texture) is itself a **valid negative result** — the spike
exists to surface exactly these.

## Non-goals (explicitly out of scope)

- Tier-2 zero-copy IOSurface on iOS (deferred; Tier-1 readback is enough for the proof).
- Physical-device build, code-signing, provisioning (simulator only for this pass).
- `list{}` / `rows` collections over FFI (still unwired from the prior spike), audio (`rodio`),
  and window-replacement / borderless-shaped windows (all N/A or out of scope on iOS).
- The full Headspace faceplate demo. A minimal skin is used instead (see Skin).
- Any change to the engine crates (`crates/carapace`, `crates/hittest`) — preserve the prior
  spike's "zero engine diff" property. All work rides the existing public C ABI.

## Architecture & data flow

```
Dart UI  ──MethodChannel──►  Swift plugin  ──C ABI (carapace.h)──►  Rust staticlib (embed-spike, iOS)
 Texture(id)  ◄──tex id──        │  owns CVPixelBuffer (IOSurface-backed, BGRA8)      │
 GestureDetector ─tap(x,y)─►     │  implements FlutterTexture.copyPixelBuffer          Engine + Renderer
 "level: N" readout              │  CADisplayLink → carapace_tick(dt)                   + FfiHost (vtable)
                                 │  + registry.textureFrameAvailable(id)
                                 │  IS the Host: get_num / get_str / invoke (Swift)
```

- **Render loop.** A `CADisplayLink` computes wall-clock `dt` from frame timestamps, calls
  `carapace_tick(e, dt)`, then `registry.textureFrameAvailable(id)`. Flutter then calls back
  `copyPixelBuffer`, which returns the retained IOSurface-backed `CVPixelBuffer` the engine just
  wrote into.
- **Surface.** Swift creates a `CVPixelBuffer` with `kCVPixelBufferIOSurfacePropertiesKey`
  (`kCVPixelFormatType_32BGRA`), obtains its `IOSurfaceRef` via `CVPixelBufferGetIOSurface`, and
  passes that to `carapace_create`. The engine's Tier-1 path does one internal CPU readback+copy
  into the IOSurface; the surface→Flutter handoff is itself zero-copy.
- **Input.** `GestureDetector` reports tap coords in widget-local points. They cross the method
  channel to Swift, which scales widget-size → skin design-canvas and calls
  `carapace_pointer(e, x, y, 0)` (kind 0 = press).
- **Host (Swift).** Swift owns a single mutable `Double` (e.g. key `"level"`). `invoke("bump")`
  increments it (clamped); `get_num("level", out)` returns it. This makes Swift the Host with zero
  engine knowledge, exactly as `FfiHost` intends.

## Components & layout

- **`crates/embed-spike/`** (existing crate, extended):
  - Widen `cfg(target_os = "macos")` gates to `any(target_os = "macos", target_os = "ios")` on the
    portable Metal/IOSurface code; fix iOS deltas as discovered.
  - Add `staticlib` to `crate-type` (lean static linkage on iOS to avoid framework-embed/signing
    friction); keep `cdylib`/`rlib` for the macOS sample.
  - No change to engine crates.
- **`crates/embed-spike/flutter-sample/`** (new, from `flutter create`):
  - `ios/Runner/` — Swift `CarapaceTexture` type: `FlutterTexture` impl, host vtable, the
    `CADisplayLink` loop, and method-channel handlers. A bridging header pulls in `carapace.h`;
    the Runner links the iOS-built `libembed_spike.a`.
  - `lib/main.dart` — a `Texture` widget, a `GestureDetector`, and a "level: N" readout proving the
    round-trip end to end.
- **Skin.** The simplest skin binding a value-bound primitive (`value_fill`) to a host number plus a
  `region` hotspot whose action bumps it. Reuse the macOS Tier-1 test skin if one exists in
  `crates/embed-spike`; otherwise author a tiny purpose-built skin. No `list{}`, audio, or
  window-replacement.

## Build & integration plan (the genuinely uncertain parts, in order)

1. `rustup target add aarch64-apple-ios-sim`; build `embed-spike` for it as a staticlib.
2. `flutter create` the sample under `crates/embed-spike/flutter-sample/`; confirm a stock iOS
   simulator run first (toolchain sanity) before adding native code.
3. Wire `carapace.h` via a Swift bridging header; link `libembed_spike.a` into the Runner.
4. Confirm wgpu/Metal initializes under the iOS simulator (Apple-silicon sim supports Metal).
   `init_gpu` currently `.expect()`s adapter/device acquisition — if it bites, note it and convert
   to a clean failure (don't unwind across FFI).
5. Confirm `CVPixelBuffer` (IOSurface-backed) is accepted as the engine target surface AND as a
   Flutter external texture in the simulator.
6. Bring up incrementally: static frame on screen → CADisplayLink tick loop → tap round-trip.

## Testing & verification

Manual/visual, mirroring the macOS spike (no automated UI test for a throwaway spike):

- The skin renders in the simulator (screenshot evidence).
- Tapping the hotspot visibly moves the value-bound bar and updates the Dart "level: N" readout.
- A short run log + screenshot are the captured evidence, referenced from the findings doc.

The engine-untouched invariant is checked the same way as before:
`git diff --stat <base>...HEAD -- crates/carapace/src crates/hittest/src` must be empty.

## Top risks (each becomes a findings-doc data point)

1. **wgpu-Metal under the iOS simulator** — initialization or feature gaps.
2. **cfg-gated interop portability** — `target_os = "macos"` code that doesn't cleanly port to
   `ios` (IOSurface APIs, Metal interop specifics).
3. **CVPixelBuffer-as-Flutter-texture** — whether the IOSurface-backed buffer binds cleanly as an
   external texture in the simulator.
4. **Static-lib linkage + bridging header** — Xcode/Flutter build-graph friction getting
   `libembed_spike.a` + `carapace.h` into the Runner.

Any of 1 or 3 hard-blocking is a legitimate (negative) spike outcome.

## Deliverables

- Working Flutter iOS simulator build under `crates/embed-spike/flutter-sample/`.
- Findings doc `docs/superpowers/specs/2026-06-29-flutter-ios-embed-spike-findings.md` with the
  go/no-go verdict and the iOS-specific gotchas, in the style of the macOS findings.

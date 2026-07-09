# Feasibility spike: carapace ⇄ vercel-labs/native (Native SDK)

**Date:** 2026-07-09 · **Verdict:** feasible in both directions, zero carapace-engine diff.
**Status:** feasibility assessed and banked; full runnable "carapace wraps native" not built (deferred).

## What Native SDK is

`vercel-labs/native` (Native SDK, v0.4.0) — a Zig framework for native desktop apps with declarative
`.native` markup, Metal-backed GPU rendering (macOS), an Elm-style model/message/update loop. C ABI +
CLI (`native init/dev/build`, vendors Zig 0.16.0). macOS primary; Linux/Windows/iOS/Android too.

## Two integration directions

### B — Native SDK hosts a carapace skin  ✅ BUILT (milestone 1)
Native SDK is the outer app; a carapace skin renders inside its `gpu_surface` pane.
- Seam: the app pushes RGBA8 into a `gpu_surface` via
  `runtime.options.platform.services.presentGpuSurfacePixels(...)` (RGBA8, straight alpha, top-left,
  device-px, self-sustaining `.gpu_surface_frame` loop); input back via `.gpu_surface_input`.
- Built here: `src/main.zig` + `csrc/carapace_bridge.c` — a BGRA IOSurface pool + carapace engine;
  carapace free-runs, `cb_latest_rgba` reads the latest surface → RGBA8 on the main thread. Proof:
  the carapace `frame` demo skin rendered and was presented into the pane (dumped frame).
- **This is the WRONG way round** for carapace's north star (carapace should be the chrome), but it
  was the quickest end-to-end proof and de-risked the whole toolchain/build/pixel path.

### A — carapace wraps a Native SDK app  ✅ FEASIBLE (not built)
carapace draws the outer skin/window; the Native SDK app's UI is composited into a `view{}` cutout.
This is the on-brand direction and **is the live-host-view-region feature** with a whole Native SDK
app as the cutout's live content.
- Uses Native SDK's **embed path** (`native_sdk_app_*` C ABI over a pure-Zig `NullPlatform` — no
  AppKit/Metal). iOS/Android are just reference hosts; a macOS/Rust host is equally valid.
- Build the app as a macOS embed **static lib**: `native_sdk.addMobileLib(b, dep, .{ .scene = .canvas,
  .main = "src/main.zig" })` → `libcarapace_nativeapp.a` exporting `native_sdk_app_*`, the UiApp
  compiled in, zero AppKit/Metal deps. (`build/app.zig:123-164` — macOS is a first-class target; the
  iOS/Android gate is cosmetic.)
- Host loop: `native_sdk_app_create` → `set_text_measure`/`set_image_service` → `start` →
  `viewport(w,h,scale,NULL,…)`; per frame `native_sdk_app_frame()` then **pull**
  `native_sdk_app_render_pixels_damage(app, scale, buf, len, &out)` (RGBA8, straight alpha, top-left,
  damage-tracked; `damage_width==0` = unchanged). Input: `native_sdk_app_touch/scroll/key/text`.
- Write those pixels (RGBA8→BGRA) into a carapace `view{}` cutout's content IOSurface; carapace
  composites → skin frames the Native SDK app. The showcase's Studio cutout is the natural host.
- **Caveat (perf):** the embed path is a **CPU software render + damage-memcpy pull**, NOT a zero-copy
  GPU/IOSurface handoff. Fine for gadget-sized content; a concern at large surfaces / high refresh,
  which matters for carapace's perf-first mandate. Zero-copy GPU into carapace is the *only* thing
  that would need a change to Native SDK (expose the present-capture chain or an IOSurface-backed
  present to C).
- The app is **baked into the lib** (recompile per app; hard-coded `"mobile-surface"` label); use the
  `.scene = .canvas` variant — the default `native_sdk_app_create` lib is a WebView shell that renders
  no canvas pixels.

## Toolchain / build notes

- `npm i -g @native-sdk/cli` (blocked by the repo's `min-release-age=7` npmrc guard + Socket Firewall
  cooldown on the 1-day-old package; installed with `--min-release-age=0` after explicit approval).
- Out-of-tree app build.zig hand-wires the framework module graph (9 sub-modules) via
  `-Dnative-sdk-path=<sdk clone>`; build with the vendored `~/.native/toolchains/zig-0.16.0/zig build`
  (NOT `native build`, which owns the graph and can't inject external C). The macOS platform host
  (`appkit_host.m`) is compiled into the app, so AppKit/Metal/WebKit/etc. must be linked.
- Direction A needs only the `NullPlatform` embed lib — none of that AppKit weight.

## Conclusion

Both directions feasible with **no carapace-engine changes**. Carapace-wraps-native (A) is the
on-brand direction and folds directly into the live-host-view-region work. Not building the full
runnable version now — feasibility is the deliverable. If resumed: build the embed `.a`, de-risk by
pulling one frame to an image, then feed a carapace skin's `view{}` cutout from it.

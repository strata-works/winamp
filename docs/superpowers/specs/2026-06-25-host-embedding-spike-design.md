# Host-Embedding Spike — Design

**Date:** 2026-06-25
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Type:** Throwaway feasibility spike — same shape as the 2026-06-19 total-window-replacement
spike (`crates/window-spike`). Proves a capability, then a real phase rebuilds it properly.

## Purpose

Prove that a **native macOS Swift app** can embed the carapace engine as a sub-view, drive it
entirely as the **Host** across a C ABI, and display its live render via a **zero-copy
IOSurface** — with a CPU-readback fallback so the FFI/host loop is proven even if the
zero-copy path stalls.

This de-risks the **host-embedding** north-star item: the engine being driven by a foreign,
out-of-language host rather than the in-repo `winit`/`wgpu` demo. The single genuinely
unproven technical unknown is **cross-process/zero-copy shared-texture compositing** between
wgpu (Metal) and a host-owned `CALayer`; the FFI boundary is the second unknown. Both are
exercised here end to end.

The result informs whether a Flutter `Texture`-widget embedding (the other candidate host) is
viable, and whether the engine's existing public API + `Host` trait are already the right
embedding boundary.

### Why this is feasible to attempt

- The engine is already **host-agnostic**: the `Host` trait (`name`/`tick`/`get`/`actions`/
  `invoke`/`rows`) is the entire capability surface; the engine knows no concrete domain.
- The renderer already draws into an **arbitrary `wgpu::TextureView`**:
  `Renderer::draw(&mut scene, &RenderTarget { device, queue, view, width, height, base_color })`.
  The demo passes a winit surface view; the spike passes a view of an IOSurface-backed texture.
- wgpu `29.0.3` exposes `Device::create_texture_from_hal` and the Metal hal backend
  (`wgpu::hal::api::Metal`), so importing an IOSurface-backed `MTLTexture` into wgpu is
  reachable. The fiddliness of that import *is* the thing this spike de-risks.

## Scope

**In scope:**
- A throwaway `cdylib` crate `crates/embed-spike` exposing a flat C ABI over the existing
  `Engine` + `Renderer`, plus a hand-written `carapace.h`.
- An `FfiHost` — a Rust `Host` impl that bridges `get`/`actions`/`invoke` to C function
  pointers the Swift side registers, so **the Swift app is the host**.
- A minimal AppKit sample app (`crates/embed-spike/macos-sample/`) that owns one `NSView` +
  `CALayer`, runs a `CVDisplayLink` render tick, forwards mouse clicks, and serves **one live
  native value** + **one action**.
- The IOSurface present path with two tiers behind one seam (zero-copy primary, CPU-readback
  fallback).
- A tiny purpose-built spike skin (`skin.toml` + a few lines of Lua) binding one state key and
  one action.
- A findings doc with the per-tier verdict and a recommendation for the real `carapace-ffi`
  phase.

**Out of scope (YAGNI for a spike):**
- Flutter / the `Texture` widget. This spike's IOSurface result *informs* whether Flutter is
  viable next; it is not built here.
- **Approach C** (wgpu built on Swift's `MTLDevice`) — deferred to the real `carapace-ffi`
  phase. The findings will name it as the next ownership question.
- `list{}` / collections (`rows`) across FFI; multi-argument actions; string-heavy state; skin
  hot-swap across FFI; a stable/versioned ABI; memory-safety hardening; Windows/Linux.

## Architecture

Three layers; the engine crate is untouched.

```
┌─ Swift AppKit app (crates/embed-spike/macos-sample/) ──────────┐
│  NSView + CALayer  ◄── IOSurface ──┐                           │
│  CVDisplayLink → carapace_tick()    │   serves Host callbacks:  │
│  mouseDown    → carapace_pointer()  │   get_num / invoke        │
└──────────────────│──────────────────│───────────│──────────────┘
        C ABI (extern "C", cdylib)     │           │
┌──────────────────▼──────────────────▼───────────▼──────────────┐
│  crates/embed-spike  (throwaway cdylib)                        │
│   • carapace_* C functions                                      │
│   • FfiHost: Host impl over registered C function pointers      │
│   • owns wgpu device/queue + Renderer + IOSurface render target │
└──────────────────│──────────────────────────────────────────────┘
                   │ uses, unchanged
┌──────────────────▼──────────────────────────────────────────────┐
│  crates/carapace  (Engine, Renderer, Host trait — NO CHANGES)    │
└──────────────────────────────────────────────────────────────────┘
```

### C ABI surface

The whole header, roughly:

```c
typedef struct CarapaceEngine CarapaceEngine;

// Host callbacks the Swift app registers — Swift IS the Host.
typedef struct {
  void* ctx;
  bool   (*get_num)(void* ctx, const char* key, double* out);   // bound numeric state
  bool   (*get_str)(void* ctx, const char* key, char* buf, size_t cap);
  void   (*invoke)(void* ctx, const char* action);              // hotspot → host action
} CarapaceHostVTable;

CarapaceEngine* carapace_create(const char* skin_dir, CarapaceHostVTable host,
                                IOSurfaceRef surface, uint32_t w, uint32_t h);
void carapace_tick(CarapaceEngine*, double dt_seconds);         // tick + render into IOSurface
void carapace_pointer(CarapaceEngine*, double x, double y, int kind);  // kind: 0 down, 1 up, 2 move
void carapace_destroy(CarapaceEngine*);
```

- `get_str` is included for completeness but the default domain uses only `get_num`.
- `actions` are discovered by the engine from the skin script; `invoke` carries just an action
  name (no args) for the spike.
- `rows`/collections are **not** in the vtable — out of scope.

### FfiHost — the "Swift owns state" bridge

`FfiHost` stores the `CarapaceHostVTable` (an opaque `ctx` pointer plus C function pointers)
and implements `carapace::host::Host`:

- `get(key)` → calls `get_num`/`get_str`; maps the result to `StateValue`. Returns `None` when
  the callback reports absence.
- `invoke(action, _args)` → calls `invoke(ctx, action)`. (Args ignored in the spike.)
- `actions()` → a small fixed slice the spike advertises (the one action name).
- `rows()` → empty (default).
- `tick(dt)` → no-op; the Swift side mutates its own state on its own schedule.

The round-trip:

1. Swift reads a live native value (default **battery %** via `IOPSCopyPowerSourcesInfo`;
   **wall-clock seconds** as fallback) and answers `get_num("level", …)`.
2. The spike skin binds key `"level"` → a `text` / `value_fill` projects it. Each
   `carapace_tick` re-pulls through the vtable, so the **Swift-owned value drives the pixels**.
3. A click in the `NSView` → `carapace_pointer` → engine hit-test → the hotspot's
   `host.<action>()` → `FfiHost::invoke` → Swift's `invoke(ctx, "toggle")` runs native code
   (toggles a Swift-owned `paused` bool) → the next frame reflects it.

## Rendering: the IOSurface present seam (Approach A)

The render target lives behind a tiny internal enum so the hard path and the safety net share
one render call:

```rust
enum Present {
    Shared { texture: wgpu::Texture },   // Tier 2: wgpu texture imported from IOSurface MTLTexture
    Readback { staging: wgpu::Buffer },  // Tier 1: render offscreen, CPU-copy bytes into IOSurface
}
```

The tier is chosen once in `carapace_create`: attempt Tier 2, fall back to Tier 1 on any
failure, and record which was reached.

### Tier 1 — CPU readback (safety net; proves the loop)

wgpu renders into an offscreen `Rgba8Unorm` texture → copy to a staging buffer → `memcpy` into
the IOSurface's locked base address (`IOSurfaceLock`/`Unlock`, respecting `bytesPerRow`).
Swift's `CALayer.contents = ioSurface` displays it. Slow (the readback the perf-priority memory
warns against), but it proves **FFI + Swift-host + display** end to end. This is the floor the
spike must clear.

### Tier 2 — IOSurface zero-copy (the prize)

Swift creates the `IOSurfaceRef`. Rust obtains wgpu's `MTLDevice` via
`wgpu.as_hal::<wgpu::hal::api::Metal, _, _>()`, builds an `MTLTexture` from the same IOSurface
(`newTextureWithDescriptor:iosurface:plane:` through the `metal` crate), imports it with
`wgpu::Device::create_texture_from_hal`, and the existing `Renderer::draw` renders straight into
its view. No copy; Swift composites the same surface.

### Known sub-risks (documented honestly even if unresolved)

- **Alpha:** premultiplied vs straight alpha writing into the IOSurface (the window spike found
  macOS Metal advertises `PostMultiplied`, not `PreMultiplied`).
- **Color space:** sRGB tagging on the `CALayer` / texture format choice.
- **Device agreement:** IOSurface is the cross-device sharing primitive, so each side makes its
  own `MTLTexture` from the shared surface — verify wgpu's device and the displayed layer agree
  on format and that the surface is genuinely shared, not silently copied.

## Swift app & the driven domain

Smallest real thing: an AppKit window with one `NSView`; a `CVDisplayLink` (or 60 Hz timer)
calling `carapace_tick(dt)`; `mouseDown`/`mouseUp` → `carapace_pointer`. The host serves **one
live native value** — default **battery %** (visibly changes, unmistakably macOS-native, no
entitlements), with **wall-clock seconds** as the fallback if battery is awkward on the dev
machine — plus **one action** (a hotspot that toggles a Swift-owned `paused` bool, which
freezes/refreshes the value).

The skin is a **purpose-built spike skin** (a `skin.toml` plus a few lines of Lua: a `text` /
`value_fill` bound to `"level"` and one `region` calling the action) — *not* Headspace or
sysmon — so the host surface stays at one getter and one action.

Build wiring: the sample app links the `cdylib`. Preferred form is a Swift Package or a tiny
Xcode project under `crates/embed-spike/macos-sample/`; a `README` in that directory documents
the exact build/run commands so the result is reproducible.

## Success criteria

1. **Tier 1 reached:** the Swift window shows the live skin; the displayed value tracks the
   Swift-owned battery/clock value; clicking the hotspot runs Swift code that visibly changes
   the skin. ⇒ embedding + Swift-as-Host + FFI loop proven.
2. **Tier 2 verdict recorded:** zero-copy IOSurface either works (screenshot + the import recipe
   documented) or fails with the specific blocker named.
3. **Engine untouched:** `crates/carapace/src/` has zero diffs (the boundary was already right)
   — or, if one *domain-neutral* engine change proves unavoidable, it is named and justified
   (the same neutrality discipline Phase 6 held).
4. **Deliverables:** a runnable `crates/embed-spike` + `macos-sample/`, a `screenshot.png`, and
   `docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md` carrying the per-tier
   verdict and a recommendation for the real `carapace-ffi` phase (including the Approach-C
   ownership question and the Flutter go/no-go signal).

## Testing

This is a throwaway spike whose verdict comes from a **human run** (the window spike was
confirmed the same way): the headline results (does it composite? does the Swift value drive the
pixels? does a click invoke Swift code?) are visual/interactive and confirmed by running the
sample app, captured in `screenshot.png` and the findings doc. No automated test suite is added
for the throwaway crate. The existing engine test suite must remain green, which (given the
zero-engine-change goal) it will by construction; `cargo clippy -D warnings` must pass on the new
crate before any push (CI gates on it).

## Follow-ups (not this spike)

- Real `carapace-ffi` product crate with a stable, versioned ABI, collections/`rows`, multi-arg
  actions, hot-swap, and memory-safety hardening.
- **Approach C** (wgpu on the host's `MTLDevice`) as the realistic embedding ownership model.
- Flutter `Texture`-widget embedding, gated on this spike's zero-copy verdict.

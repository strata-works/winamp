# Host-Embedding Spike — Findings (2026-06-25 → 2026-06-28)

Throwaway feasibility spike. Crate: `crates/embed-spike` (Rust `cdylib` + C ABI) plus a native
Swift AppKit sample under `crates/embed-spike/macos-sample/`. Platform tested: **macOS / Metal,
Apple silicon, Retina**. Branch: `host-embedding-spike`.

## Headline: native-app embedding is FEASIBLE — and it reaches the founding north star

A native macOS Swift app embeds the carapace engine over a flat C ABI, acts as its **Host** (it
owns the state and actions; the engine has zero knowledge of Swift), and displays the engine's
GPU render through a **zero-copy IOSurface**. Beyond the original question, the same artifact
demonstrates both halves of the founding motivation in a *foreign* host:

- **Total window replacement** — the carapace skin (the real Headspace faceplate) *is* the
  window: borderless, transparent, shaped to the head silhouette, floating on the desktop with
  no OS chrome, draggable by its body, with the skin's drawn minimize/close glyphs acting as
  real window controls, and scroll/pinch/⌥-drag/+−-button zoom resize.
- **Live host view region** — the skin's CRT screen is a `view{}` cutout the engine fills with
  the **Swift app's own live content** (a ticking clock + label + animated bar), rendered by
  Swift into a second IOSurface and composited by the engine.

**The single most important result: zero engine change.** `git diff main...HEAD --
crates/carapace/src crates/hittest/src` is **empty**. Everything was built on the existing
public surface (`Engine`, `Renderer`, the `Host` trait, the `view{}` primitive + composite
pipeline). The embedding boundary the engine already had is sufficient.

## What was built (per layer)

- **`crates/embed-spike` (cdylib + rlib):** a flat C ABI (`carapace.h`) — `carapace_create`,
  `carapace_tick`, `carapace_pointer`, `carapace_active_tier`, `carapace_destroy` — wrapping a
  headless wgpu device + the `Renderer` + an `Engine` whose `Host` is `FfiHost`.
- **`FfiHost`:** a `carapace::host::Host` impl that bridges `get`/`actions`/`invoke` to a
  `#[repr(C)] CarapaceHostVTable` of function pointers the Swift app registers. This is what
  makes **Swift the host**.
- **macOS sample (AppKit):** owns the window/`NSView`/`CALayer`, creates the IOSurface(s),
  drives the tick via a main-runloop timer, forwards mouse/keyboard, and serves state/actions.

## Tiers (the render-to-IOSurface path)

- **Tier 1 — CPU readback: WORKED.** Engine renders into an offscreen texture → `readback_rgba`
  → swizzle+stride copy into the caller's BGRA IOSurface → `CALayer.contents`. Human-confirmed:
  the Swift-owned value drove the rendered bar; clicks round-tripped through the engine to Swift
  actions (`tier1-run.log`). **But it is laggy** — a synchronous GPU→CPU readback + map every
  frame *on the main thread* starves the AppKit event loop. This is the perf-priority
  anti-pattern; it is the floor, not the goal.
- **Tier 2 — zero-copy IOSurface: REACHED (blit variant).** No CPU readback. vello renders into
  an `Rgba8Unorm` storage offscreen, then `wgpu::util::TextureBlitter` GPU-blits (handling
  RGBA→BGRA) into a `Bgra8Unorm` wgpu texture that **aliases the caller's IOSurface**. The
  console reports `active tier: 2 (Shared/Metal)`; human-confirmed displaying correctly.

### The real Tier-2 recipe (this corrects the design doc's guesses)

- wgpu-hal 29 interop uses **`objc2-metal 0.3.2`** (with feature `objc2-io-surface`), **not** the
  old `metal` crate.
- `device.as_hal::<wgpu::hal::api::Metal, _, _>()` returns an `Option<impl Deref>` directly (no
  closure) → `.raw_device()` → `mtl_device.newTextureWithDescriptor_iosurface_plane(&desc, io, 0)`
  (the `io_surface::IOSurfaceRef` raw pointer reborrows as `&objc2_io_surface::IOSurfaceRef` —
  same opaque type) → `<Metal as wgpu::hal::Api>::Device::texture_from_raw(...)` →
  `device.create_texture_from_hal::<Metal>(...)`.
- `Features::BGRA8UNORM_STORAGE` **is** available on Apple silicon (requested in `init_gpu`, guarded
  by an `adapter.features()` check). It is the prerequisite for a *direct* Tier-2 variant (vello's
  compute shader writing straight into the BGRA IOSurface texture), which is one usage-flag change
  away; the **blit variant** was shipped and does **not** need it. Both are zero-CPU-copy.
- `io-surface 0.16` is deprecated (`#[allow(deprecated)]` throughout); a real crate should move to
  `objc2-io-surface`.

## Non-obvious gotchas found the hard way (the real value of the spike)

1. **CALayer caches an IOSurface by object identity.** Reassigning the *same* `IOSurface` to
   `layer.contents` every frame does **not** refresh the screen even though the surface's pixels
   changed — the picture freezes while the render loop spins at 60fps. Fix: explicitly call the
   (private but standard) **`setContentsChanged`** on the layer each frame. This was the "frozen
   clock" bug; it was isolated by proving the Rust pipeline and the Swift drawing each worked in
   isolation (PNG probes), leaving the display step as the only suspect.
2. **Render at the backing scale for Retina.** A point-sized IOSurface displayed in a point-sized
   window is upscaled 2× on Retina → blurry. Render into a `pointSize × backingScaleFactor`
   surface. Because gadget skins lay out at their design canvas and the renderer scales
   `canvas → target`, the fix is: **lay out and hit-test at the design canvas, render into the 2×
   surface** (mouse mapping stays in design coords). Verified headless: at 2× the skin fills the
   whole surface, not a corner.
3. **CPU→GPU coherency for a host-written surface.** Importing the content IOSurface as an
   *aliased* sampled texture let the GPU cache the first frame (host content froze). Robust fix:
   **`queue.write_texture` the content bytes every frame** (read the IOSurface, repack rows to the
   256-byte `bytes_per_row` alignment, upload). The aliased-output path (Tier 2 above) is fine
   because the engine *writes* it; only the host-*written*, GPU-*read* path needed this.
4. **Borderless windows + input.** A `.borderless` `NSWindow` returns `false` from
   `canBecomeKey`/`canBecomeMain` by default — override both. And `acceptsFirstMouse = true`
   delivers clicks *without* making the window key, so keyboard events never arrive; call
   `window.makeKey()` on click if you want key handling.
5. **Orientation when drawing into an IOSurface with Core Graphics.** A raw `CGBitmapContext`
   wrapped in `NSGraphicsContext(flipped: true)` with no CTM flip renders 180°-inverted. Correct:
   `translate(0,h); scale(1,-1)` *then* `flipped: true` → upright, un-mirrored (PNG-verified).
6. **Input devices may report `scrollingDeltaY == 0`** (trackpad phase events) — proportional
   scroll-zoom never moves. Use a fixed ±step past a deadzone, and provide an explicit affordance
   (on-screen +/− buttons) since a shaped window has no OS resize handles.

## Engine-untouched check

`git diff --stat main...HEAD -- crates/carapace/src/ crates/hittest/src/` → **empty**. The only
crates touched are the new `crates/embed-spike` and its Swift sample. Every capability
(embedding, host loop, IOSurface zero-copy, window replacement, live-host-view, Retina, resize)
rides the existing public API.

## Recommendation for the real `carapace-ffi` phase

1. **Pursue it — embedding is proven.** Promote `embed-spike` into a real `carapace-ffi` crate
   with a stable, versioned C ABI and a clean header (consider `cbindgen`), `objc2-io-surface`
   instead of the deprecated `io-surface`, and memory-safety hardening at the boundary.
2. **Threading / frame pacing.** The current loop renders + polls synchronously on the main
   thread (the source of the Tier-1 lag and a ceiling on Tier-2). The engine holds `Rc` (not
   `Send`), so the host must keep all engine calls on one thread — design the real layer around a
   **dedicated render thread** that owns the engine, presenting to the main thread only for
   `CALayer` updates. Or present via a `CAMetalLayer` drawable instead of `CALayer.contents`.
3. **Try the direct Tier-2 variant** (vello compute-writes straight into the BGRA IOSurface via
   `BGRA8UNORM_STORAGE`) and measure it against the blit variant.
4. **Window-control host actions** (`minimize`/`close`/`begin_drag`) worked through the generic
   action allowlist with no engine knowledge — keep them host-side; the engine stays neutral.
5. **Collections over FFI.** `rows()`/`list{}` were left out of the vtable (the playlist renders
   empty). A real layer needs a `rows` callback to drive `list{}` from the host.
6. **Flutter go/no-go:** the zero-copy IOSurface result is the green light for a Flutter
   `Texture`-widget embedding — Flutter external textures on macOS are IOSurface/Metal-backed, so
   the same import path applies. This is the recommended next host after `carapace-ffi`.

## Known limits left open (deliberately, for a spike)

- No stable/versioned ABI; `io-surface 0.16` deprecated; `#[allow(deprecated)]` throughout.
- Collections (`rows`/`list{}`), multi-argument actions, and skin hot-swap across FFI not wired.
- Single-thread render+poll on the main thread (frame-pacing ceiling).
- Premultiplied-alpha edge behavior on the shaped window looked clean here but was not formally
  characterized.
- macOS/Metal only; Windows/Linux and the Flutter host are future work.
- `init_gpu` uses `.expect()` for adapter/device acquisition, which runs inside `carapace_create`
  across the C ABI — a panic there would unwind across FFI (UB). Unreachable on the target (Metal
  is always present), but the real `carapace-ffi` layer must convert it to a null return.

# carapace-ffi v4 — seamless skin swap

**Status:** design
**Date:** 2026-07-08
**Predecessors:** v1 (#25, safe Apple C ABI), v2 (#27, render thread + command queue),
v3 (#35, `carapace_swap_skin` + collections + numeric action args).

## Summary

`carapace_swap_skin` (shipped in v3) currently **freezes the render loop** for the duration of the
new skin's load and then **pops** the new skin in with no transition. v4 makes the swap seamless:

1. **No stall** — the old skin keeps animating while the new skin is loaded and warmed; the render
   loop never blocks on disk/decode/upload.
2. **A dissolve, not a pop** — the old skin crossfades into the new one over a short, **skin-authored**
   duration.

The transition is a property the **incoming skin declares in its `skin.toml`**, not a host parameter.
`carapace_swap_skin(handle, dir)` keeps its exact signature (same-size swaps); host apps recompile
with zero code changes and inherit seamless same-size swaps. ABI **MINOR bumps 3.0 → 3.1** for the
manifest capability.

3. **Native-size swaps** (added after review feedback) — a companion export
   `carapace_swap_skin_resized` lets the host hand in a **new surface pool at the incoming skin's
   native size** so each skin renders at its own resolution instead of being scaled to the first
   skin's pool. The outgoing skin is briefly scaled as it dissolves out; you always *land* on a
   native-size skin. This reuses the same warm-then-crossfade machinery. Adding an export is
   **additive** — ABI **MINOR bumps 3.1 → 3.2** (existing symbols unchanged). See Component 5.

**Scope:** Apple-only, on the existing render-thread/`wgpu-hal` IOSurface path. No new threads
(inline warm). One transition kind: crossfade (plus `cut`).

## Motivation

Today, `Command::SwapSkin` calls `carapace::skin::load_dir(&dir)` **synchronously on the render
thread** inside the command handler (`render_thread.rs:268`), then enqueues the engine swap. The
running animation freezes for the whole load, and the new skin replaces the old instantly on the next
frame. `load_dir` itself is cheap (read `skin.toml`, read the Lua entry, scan the asset dir into a
name→path index), but the expensive work — PNG decode → RGBA and GPU texture upload — happens
**lazily on the new skin's first render**, also on the render thread, so it lands as a visible hitch.

The crossfade is not just eye candy: warming the new skin offscreen *while the old skin keeps
presenting* is the mechanism that hides the decode/upload cost, and the dissolve masks any residual
warm-up.

## Design decisions (resolved during brainstorming)

- **Transition is skin-authored**, declared in the incoming skin's `skin.toml`.
- **Default transition = crossfade, ~250 ms** when a skin declares no `[transition]` table. Every
  existing skin (including the showcase skins) gets the dissolve for free.
- **Inline warm** — `load_dir` + the warm render happen on the render thread; the old skin keeps
  presenting, costing at most ~1 dropped old-skin frame during warm. No worker thread. (A
  background-worker preload is a possible future enhancement, deferred — decode/upload must stay on
  the render thread regardless, so the marginal gain is small.)
- **C ABI: existing symbols unchanged**; MINOR 3.0 → 3.1 for the manifest capability, then 3.1 → 3.2
  for the one additive export `carapace_swap_skin_resized` (Component 5).

## Component 1 — Skin-authored transition (engine crate, additive)

New optional table in `skin.toml`, surfaced through the existing `Manifest` in
`crates/carapace/src/skin.rs`:

```toml
[transition]
kind = "crossfade"   # "cut" | "crossfade"   (default: "crossfade")
duration_ms = 250    # default: 250
```

### Types

```rust
/// How a skin dissolves in when swapped to. Declared by the *incoming* skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Instant replacement (still stall-free via warm).
    Cut,
    /// Alpha dissolve from the outgoing skin to this one.
    Crossfade,
}

/// The incoming skin's swap transition. Absent `[transition]` table → `Transition::default()`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Transition {
    #[serde(default = "default_transition_kind")]   // Crossfade
    pub kind: TransitionKind,
    #[serde(default = "default_transition_ms")]      // 250
    pub duration_ms: u32,
}

impl Default for Transition { /* Crossfade, 250 */ }
```

- `Manifest` gains `#[serde(default)] pub transition: Transition`.
- **Schema version stays `1`.** The table is fully defaulted, so every existing skin still loads and
  validates unchanged.
- `duration_ms` is clamped to a sane ceiling on read (e.g. `min(duration_ms, 5000)`) so a skin can't
  wedge the loop into a multi-second blend.
- `load_dir` already returns `(Manifest, SkinSource)`; today `Command::SwapSkin` throws the manifest
  away (`Ok((_m, source))`). v4 keeps it and reads `manifest.transition`.

### Tests (engine crate)

- `skin.rs`: a `skin.toml` with `[transition] kind="cut" duration_ms=100` parses to
  `Transition { Cut, 100 }`; a `skin.toml` with **no** `[transition]` table parses to
  `Transition { Crossfade, 250 }`; an out-of-range `duration_ms` clamps.

## Component 2 — Swap state machine (FFI render thread)

The render thread gains a swap state. The old skin's `Engine` keeps receiving frames until the
crossfade finishes.

```rust
enum SwapState {
    Idle,
    /// New engine built; render it offscreen once to force decode+upload before we start blending.
    Warming { incoming: Engine, transition: Transition, incoming_canvas: (u32, u32) },
    /// Both engines live; blend outgoing→incoming by eased t over `dur`.
    Crossfading { outgoing: Engine, t0: Instant, dur: Duration, incoming_canvas: (u32, u32) },
}
```

`RenderThread` gains `swap: SwapState` (default `Idle`) plus two scratch offscreen textures
(`tex_a`, `tex_b`, both `w×h`) and a blend pipeline (Component 3). The **primary** engine stays in
`self.engine` throughout; `Warming`/`Crossfading` hold the *other* engine.

### `Command::SwapSkin` handler (replaces the current synchronous body)

```
load_dir(dir)  ->  Err  => set_last_error + reply(ErrBadSkin)          // unchanged rejection path
               ->  Ok((manifest, source)) =>
     build incoming Engine::new(Box::new(FfiHost::new(self.vtable)), VocabRegistry::base(), source)
         -> Err => set_last_error + reply(ErrBadSkin)
         -> Ok(incoming) =>
             self.swap = SwapState::Warming { incoming, transition: manifest.transition, incoming_canvas }
             *invalidated = true
             reply(Ok)                                                  // synchronous success, as today
```

`carapace_swap_skin` still reports load/build failure **synchronously** via the one-shot `reply`
(same contract as v3). A second swap request while a swap is already in flight replaces the pending
`Warming`/`Crossfading` target with the newest (last-writer-wins), so rapid skin cycling can't stack.

### `render_one` integration

`render_one` branches on `self.swap`:

- **`Idle`** — exactly today's path: render `self.engine` into the present target, blit/readback,
  publish. Untouched.
- **`Warming`** — render `self.engine` (the *old* skin) into the present target and present it as
  normal (old skin stays smooth). Then, in the **same** loop iteration, render the `incoming` engine
  once into a throwaway offscreen (`tex_b`) to force its lazy asset decode + GPU upload. Transition:
  - `kind == Cut` → immediately promote: `self.engine = incoming`, refresh `cw/ch`, `swap = Idle`.
    (Still stall-free; no blend.)
  - `kind == Crossfade` → `swap = Crossfading { outgoing: mem::replace(&mut self.engine, incoming),
    t0: now, dur }`. From here on `self.engine` is the **new** skin.
- **`Crossfading`** — render `self.engine` (new) into `tex_a` and `outgoing` (old) into `tex_b`,
  compute `t = ease((now - t0) / dur)`, run the blend pass `mix(tex_b, tex_a, t)` into the present
  target, then the existing blit/readback path. When `t >= 1.0`: drop `outgoing`, `swap = Idle`.

The warm render costs the old skin at most one frame slot; the crossfade renders two engines + one
blend pass per frame for ≤ `duration_ms` (~15 frames at 60 fps) — cheap.

### Pointer / hit-test during a swap

- **`cw/ch`** (the hit-test design canvas) switches to `incoming_canvas` the moment we leave
  `Warming` (i.e. as soon as the new skin owns `self.engine`). During `Warming`, hit-testing still
  targets the old skin.
- Pointer commands during `Crossfading` route to `self.engine` (the incoming skin) only — the
  outgoing skin is on its way out and receives no input. This keeps input consistent with the canvas
  that hit-testing reports.
- The published snapshot (`SnapshotCell`) reflects `self.engine`'s scene, so from the host's side the
  interactive scene flips to the new skin at crossfade start (matching `cw/ch`).

### Free-run / paused interaction

A swap sets `*invalidated = true`, and `Crossfading` is inherently animating, so the loop must keep
ticking every frame until `t >= 1.0` even when `fps == 0` (paused): treat `!matches!(swap, Idle)` as
"must render next frame" alongside `invalidated`. When the crossfade completes it returns to whatever
the host's `fps`/paused state was.

## Component 3 — Crossfade blend pass (FFI crate, new GPU work; no engine diff)

The existing `blit` (`render.rs:427`) uses `wgpu::util::TextureBlitter` — a plain copy, no alpha. v4
adds a minimal fullscreen blend:

- **`CrossfadeBlender`** — a render pipeline with a fullscreen-triangle vertex shader and a fragment
  shader `out = mix(sample(old, uv), sample(new, uv), t)`, `t` supplied via a tiny uniform (or push
  constant). Two `TextureView` bind-group inputs + a sampler. Built once at render-thread
  construction (`build`), reused every crossfade frame.
- Writes into the same present offscreen the normal path uses (`Present::Shared.off.view`), so the
  downstream blit-to-IOSurface (Tier 2) / readback (Tier 1) paths are **unchanged**.
- Easing: a smoothstep on `t` (`t*t*(3-2t)`) for a natural dissolve. Clamped to `[0,1]`.

Two scratch offscreen textures (`tex_a`, `tex_b`, `Rgba8` storage, `w×h`) are allocated in `build`
and resized-with alongside the presents if a resize path exists (none today; note it). This is the
whole engine-diff-free footprint of the crossfade — the engine still renders one scene per call; the
FFI crate composites two of them.

## Data flow (crossfade frame)

```
loop tick (Crossfading)
  ├─ upload host content surface (unchanged)
  ├─ render_frame(self.engine  /*new*/, renderer, gpu, tex_a, …)   // new skin → tex_a
  ├─ render_frame(outgoing     /*old*/, renderer, gpu, tex_b, …)   // old skin → tex_b
  ├─ t = smoothstep(clamp((now - t0)/dur, 0, 1))
  ├─ CrossfadeBlender.draw(tex_b, tex_a, t) -> present.off.view    // mix
  ├─ present: blit off→IOSurface (Tier2) | readback→copy (Tier1)   // unchanged
  ├─ publish snapshot (self.engine's scene)                        // unchanged order
  └─ frame_ready(ctx, surface_index, frame_id)                     // unchanged
  if t >= 1.0 { drop outgoing; swap = Idle }
```

## Component 4 — Showcase integration (proving ground)

The native SwiftUI showcase is where "seamless" is actually judged. Today it **does not use the
live-swap API at all**: `applySkin`/`cycleSkin` (`App.swift:107–161`) fully **destroy + recreate** the
engine on every skin change — `carapace_destroy` (joins the render thread), resize the borderless
window to the new skin's canvas, then a fresh `CarapaceBridge`/`carapace_create` at the new size. The
`CarapaceBridge.swap()` wrapper around `carapace_swap_skin` exists but is unused.

v4 rewires the showcase to exercise (and demonstrate) the live swap:

- **`cycleSkin` calls `bridge.swap(nextDir)`** instead of tearing down and rebuilding. The crossfade
  runs inside the **current** surface pool/window size — the incoming skin scales into it — so the
  transition is genuinely seamless (no engine/thread teardown, no pool rebuild mid-blend).
- **Window resizes *after* the fade.** The three skins have different canvas sizes; the window
  animates to the incoming skin's canvas once the crossfade completes. The host schedules this by
  reading `transition.duration_ms` from the incoming `skin.toml` — `SkinManifest.swift` already
  scans the manifest host-side for canvas w/h; it gains a tiny `duration_ms` parse (regex, no TOML
  dep, no ABI change). During the brief window animation the fixed-size IOSurface simply scales in
  the view; the new skin is already fully shown, so this reads as a settle, not a hitch.
- **`MusicHost` and the dither content surface persist** across the swap (they already do; the swap
  keeps the same handle, so no host-state rebuild).
- **Pool re-fit at the exact new size is deferred.** If per-skin pixel-exact crispness after the
  resize proves necessary, a one-time pool rebuild *after* the transition (or a future
  `carapace_resize` export) can follow — out of scope for v4, and separate from the seamless moment.

This makes the flagship app the living verification of v4, and the manual check is: cycle skins and
confirm the old skin keeps animating, dissolves smoothly into the new one, and the window settles to
the new size afterward — no freeze, no pop.

## Component 5 — native-size swaps (`carapace_swap_skin_resized`)

`carapace_swap_skin` keeps the host's original surface pool, so the engine scales every incoming
skin's design canvas into that fixed size — swapping between differently-sized skins scales them all
to the first skin's resolution. To let each skin render at its **native** size, v4 adds a companion
export that swaps the skin *and* adopts a new host-provided pool.

### New export (additive, ABI 3.1 → 3.2)

```c
CarapaceStatus carapace_swap_skin_resized(
    CarapaceEngine* ptr,
    const char* skin_dir,
    const void* const* surfaces,   // new pool, host-allocated at the incoming skin's native size
    uint32_t surface_count,
    uint32_t width,                // new surface pixel size
    uint32_t height,
    const void* content_surface    // new content surface for a view{} cutout, or null
);
```

Same contract as `carapace_swap_skin`: **synchronous** (blocks until the render thread has built the
incoming engine, rebuilt its present pool at the new size, and entered the transition), reporting
`ErrBadSkin` on load/build failure with the current skin+pool left intact. The new surfaces must
outlive the engine until the next swap or destroy (same as `carapace_create`). `carapace_swap_skin`
is unchanged and remains the right call for same-size hosts.

### Render-thread mechanics

The `Command::SwapSkinResized { dir, pool, w, h, reply }` handler mirrors the `SwapSkin` handler,
plus a **pool rebuild** before entering `Warming`:

1. `load_dir` + `Engine::new` (as `SwapSkin`; failure → `ErrBadSkin`, pool untouched).
2. Rebuild the present pool at the new size: `build_present(gpu, surface, w, h)` per new surface
   (recomputing the weakest tier), `build_content(gpu, new_content)`, and fresh `tex_a`/`tex_b`
   offscreens at `w×h`.
3. **Atomic switch** on the render thread: replace `self.{surfaces, presents, held(all false),
   content, tier, w, h, tex_a, tex_b, next_surface=0}` with the rebuilt pool (the old `Present`s
   drop, freeing our wgpu wrappers; the host owns and frees the old IOSurfaces after the call
   returns). Then `self.swap = Warming { incoming, transition }`, `invalidated = true`, reply `Ok`.
4. The existing Warming → Crossfading path then runs **in the new pool**: the warm frame presents the
   outgoing skin scaled into the new size; the crossfade renders the incoming skin at native size
   into `tex_a` and the outgoing skin scaled into `tex_b`, blending into the new-size present. So the
   skin you land on is native; only the departing skin is scaled while it fades.

Because the export blocks until the switch is done (reply sent *after* the switch), no old-pool frame
is emitted after it returns — the host can swap its surface set and resize its window on `Ok`.

### Showcase adoption (revises Component 4)

The showcase's `swapSkin` becomes: compute the incoming skin's native `(w,h)` (canvas × backing
scale), allocate a new 3-surface pool (and, for Studio, a correctly-sized dither content surface),
call `carapace_swap_skin_resized`, then on `Ok` switch the frame-sink's surface set to the new pool
and resize the borderless window to the new native size (top-left anchored). The **window resize now
happens at swap start** (the new pool is already native size), not after the fade — so the departing
skin is what scales during the ~250 ms dissolve, and the incoming skin is pixel-native throughout.
This also **resolves Component 4's deferred content-surface gap**: the host now provisions a
correctly-sized content surface for the incoming skin, so live-swapping into Studio gets a proper
dither cutout.

The frame-sink surface handoff crosses the render thread; guard the shared surface array (a small
lock, or rely on the blocking swap making the window narrow) so a frame in flight during the switch
can't read a half-updated pool.

## What does NOT change

- **Existing C ABI symbols & signatures** — `carapace_swap_skin(handle, skin_dir)` and every other
  existing export are identical. The only ABI change is **one additive export**
  (`carapace_swap_skin_resized`, Component 5) → MINOR 3.1 → 3.2. Nothing is renamed or reordered.
- **Engine rendering** — `render_frame`, `Renderer`, vello path all untouched. v4 runs two engines
  and blends their outputs in the FFI crate.
- **Present tiers** — Tier-2 blit / Tier-1 readback paths unchanged; the blend writes into the same
  offscreen they already consume.
- **Host apps** — showcase (native SwiftUI), Flutter/WidgetKit spikes: recompile, zero code change,
  seamless swaps for free.

## Migration impact

- `Manifest` gains a defaulted `transition` field — no existing `skin.toml` changes required; all
  existing skins inherit `Crossfade{250}`.
- **Existing FFI gpu-tests that swap** (`swap_skin_applies_and_bad_dir_is_rejected`,
  `swap_skin_...canvas...` in `render_thread.rs`) currently render **one** frame after the swap and
  assert the new content is present. With the default crossfade they must now **drive frames until
  `t >= 1.0`** (advance an injectable clock or loop `duration_ms`) before asserting the new skin is
  fully shown — or set the fixture skins' `[transition] kind = "cut"` to assert the immediate-promote
  path. Both the crossfade-completes path and the `cut` path get explicit coverage.
- No change to `carapace-demo` host impls (transition is engine/FFI-internal; the vtable is untouched).

## ABI / headers

- `CARAPACE_ABI_MINOR` 0 → 1 (`carapace_abi_version()` returns `3 << 16 | 1`); update the
  `abi_version_is_v3` test to assert `3<<16 | 1` (rename to `abi_version_is_v3_1`).
- Regenerate `include/carapace.h` (cbindgen) — no symbol changes expected; the freshness test
  (`tests/header.rs`) should stay green apart from the version constant.

## Testing

**Engine crate:**
- Manifest transition parsing + defaults + clamp (Component 1 tests).

**FFI crate (host-portable, no GPU):**
- `queue.rs`: `SwapSkin` still ordered / not coalesced (existing test stays green).
- Swap state-machine transitions where testable without GPU (e.g. `Warming`→`Cut`→`Idle` promotion
  logic factored so it's unit-testable, or covered by the gpu-tests below).

**FFI crate (gpu-tests lane, `#[cfg(feature = "gpu-tests")]`):**
- **Crossfade completes:** swap to a different base-vocab skin; drive frames for ≥ `duration_ms`;
  after completion the swap state is `Idle`, `cw/ch` == new skin's canvas, and a rendered frame
  reflects the new skin.
- **Mid-crossfade blend:** at ~half duration, the present output is neither pure-old nor pure-new
  (a sampled pixel differs from both endpoints) — proves the blend actually runs.
- **`cut` transition:** a fixture skin with `kind="cut"` promotes in one frame (no lingering
  `Crossfading` state).
- **Bad dir rejected:** unchanged — `ErrBadSkin` returned synchronously, swap state stays `Idle`,
  old skin keeps rendering.
- **Old skin animates during warm:** frames still present between the swap command and crossfade
  start (no `None`/blocked frames).

**Determinism:** the loop's `dt`/`t` derive from `Instant::now()`. For deterministic crossfade tests,
inject the clock (a `Fn() -> Instant` or an accumulated-`dt` seam on the render thread) so a test can
step `t` from 0 → 1 without wall-clock sleeps.

**Showcase (manual, the real judgment):**
- Rewire `cycleSkin` to `bridge.swap()` and cycle through the three skins. Confirm: the old skin
  keeps animating during load, dissolves smoothly into the new skin over its declared duration, and
  the window settles to the new canvas size afterward — no freeze, no pop, `MusicHost`/dither persist.
- `SkinManifest.parseCanvas` gains a `duration_ms` companion (unit-testable in `ShowcaseTests`).

## Definition of done

- `carapace_abi_version()` returns `3<<16 | 1`; header regenerated; `tests/header.rs` green.
- `skin.toml` `[transition]` parses with the specified defaults/clamp; existing skins load unchanged.
- `carapace_swap_skin` never blocks the render loop: the old skin keeps presenting through load+warm;
  the new skin dissolves in over its declared duration; `cut` promotes instantly.
- Crossfade blend proven by the gpu-tests above (completes, mid-blend is a real mix, `cut` path).
- Pointer/hit-test canvas switches to the new skin at crossfade start; snapshot consistent.
- Showcase cycles skins via `bridge.swap()` (live-swap, no destroy+recreate); manual check confirms
  a seamless dissolve with the window settling to the new size after the fade.
- `cargo test --workspace` + `clippy -D warnings` + the `gpu-tests` lane all pass; `swift build` +
  `ShowcaseTests` green.
- README + `docs/api` updated for the `[transition]` manifest capability and the seamless-swap
  behavior (per the "keep README/docs current per phase" convention).

## Deferred (not in v4)

- Background-worker preload (fully removing disk I/O from the render thread).
- Transition kinds beyond crossfade (slide/wipe/etc.) and host-overridable transitions.
- A pure `carapace_resize` (resize the current skin's pool with no skin change) — Component 5 covers
  resize *coupled with* a swap, which is what the showcase needs; a standalone resize is deferred.
- Cross-platform (Windows/Linux/Android) — unchanged from prior versions.
- Engine self-scheduling animation clock (`next_wake()`/`is_animating()`).

# carapace-ffi v2 — render thread + command queue (design, 2026-07-02)

The second `carapace-ffi` increment, and the research's headline recommendation deferred from v1:
move the engine off the caller's thread onto a **dedicated render thread** that carapace owns, driven
by a **command queue**. This removes the synchronous GPU stall that janked both the macOS and Flutter
spikes (Tier-1 readback and even the Tier-2 blit poll block inside `carapace_tick` today), relaxes the
v1 single-thread `!Send` handle constraint, and delivers the first real engine→host push channel
(`frame_ready`).

Builds directly on the shipped v1 (`crates/carapace-ffi`, spec
`2026-07-01-carapace-ffi-design.md`, PR #25) and its research (`2026-07-01-carapace-ffi-research.md`,
§2 "Threading a single-threaded engine over FFI" + the addendum's render-thread items). Stays
**Apple-only** on the proven `wgpu-hal` IOSurface path; Windows/Linux/Android remain future work.

## Motivation

v1 `carapace_tick` runs the whole frame — drain host actions, lay out, render, present — **on the
caller's thread**, and blocks on a GPU poll (`render.rs`: Tier-1 `readback_rgba` waits indefinitely;
Tier-2 `blit` polls after submit). The spikes called this on their UI/main thread, so the stall
showed up as jank. v1 documented the handle as `!Send` (create/tick/destroy all one thread). This
increment fixes both: the engine lives on a thread carapace owns, and the host's public API becomes
thread-safe and non-blocking.

## Locked design decisions (from the brainstorm)

1. **carapace owns the render thread.** It spawns one `std::thread`; the `!Send` engine + GPU context
   are constructed on it and never leave it. Host API calls marshal onto the thread (input) or read
   published state (queries). Host may call from any thread.
2. **Present = host-provided surface pool + `frame_ready` callback.** The descriptor takes 2–3
   caller-owned IOSurfaces; carapace round-robins into a free one and signals which is ready. Host
   keeps allocation ownership (matches v1's contract + Flutter's texture-registration model);
   carapace owns scheduling. This is the triple-buffer scheme the research left open.
3. **Cadence = free-run at a target frame rate (default 60fps), host does nothing to animate.** The
   render thread computes wall-clock `dt` itself; the host just mutates its bound values and the loop
   samples them next frame. Optional `set_frame_rate` (0 = paused/on-demand) and `invalidate`
   (render-one-now) exist only for hosts that want to trade smoothness for battery — never on the
   happy path. Requiring per-frame `invalidate` was rejected as an adoption footgun.
4. **Synchronous queries read a published snapshot.** After each frame the render thread publishes the
   laid-out scene (+ tier) into an atomically-swapped cell; `hit_test`/`active_tier` read it on the
   caller's thread with no queue round-trip, so window-drag classification stays sub-millisecond
   (≤1 frame stale — fine for chrome).

## Architecture

Same crate (`crates/carapace-ffi`), same Apple `#[cfg]` gating and `crate-type`. The file layout gains
a render-loop/threading module and reshapes the handle:

| File | Change |
|---|---|
| `handle.rs` | `CarapaceEngine` (the C-ABI handle) becomes the **thread-safe front-end**: command sender, snapshot reader, callback table, atomic `poisoned`, `JoinHandle`. No engine/GPU fields — those move to the render thread. `create` spawns the thread + blocks for its init result; `destroy` signals shutdown + joins + frees. |
| `render_thread.rs` (new) | The render thread's owned state (`Engine`, `GpuCtx`, `Renderer`, `Present` per surface, tier, sizes) + the loop: park/wake → drain commands → tick → layout → render → present → publish snapshot → `frame_ready`. Owns the wall-clock `dt` timer and the target-fps pacing. |
| `queue.rs` (new) | `Command` enum + the channel type + move-coalescing drain helper. Host-portable, unit-tested without GPU. |
| `snapshot.rs` (new) | The published read-only scene/tier cell (atomic swap) + the `hit_test`/`active_tier` readers. Host-portable. |
| `render.rs` | Present internals unchanged (`try_shared`, `blit`, `readback_rgba`, IOSurface upload/copy), now called from the render thread; `Present` becomes **per-surface** (one per pooled IOSurface). |
| `host.rs` | `FfiHost` + `CarapaceHostVTable` unchanged in shape; the vtable's raw pointers now cross onto the render thread via the documented `Send` wrapper (§ The unsafe crux). Adds the `frame_ready`/`release` fn-ptrs to the callback set. |
| `guard.rs` | `ffi_guard!` still wraps the C entry points (arg validation, poison short-circuit). A parallel `catch_unwind` guards the **render-loop body** so a panic there poisons + exits the thread instead of aborting. |
| `include/carapace.h`, `cbindgen.toml` | Regenerated for the v2 ABI; freshness test unchanged. |

### Data flow

- **create:** validate desc → spawn render thread, handing it the skin dir, vtable (wrapped), surface
  pool, and sizes → thread builds `Engine` + `init_gpu()` + per-surface `Present` (Tier-2 with Tier-1
  fallback, per surface) → thread sends back `Ok`/`Err(status,msg)` over a one-shot → `create` blocks
  on that → on `Ok` box the front-end handle into `*out`; on `Err` set `last_error`, join the thread,
  leave `*out` null. Whole body guarded.
- **render loop (per frame):** park until (a) target-fps deadline elapses while running, (b) a command
  arrives, or (c) `invalidate`/pointer wakes it → drain+coalesce commands → `dt = now - last_render`
  → `engine.update(dt)` (drains host actions, `host.tick`) → `engine.layout(cw,ch)` → pick a
  host-free surface → `render_frame` into it → present (Tier-2 blit / Tier-1 readback) → publish
  scene+tier snapshot → `frame_ready(ctx, surface_index, frame_id)` → loop.
- **pointer:** thread-safe enqueue `Command::Pointer{x,y,kind}` + unpark. Returns immediately. The loop
  applies it via `handle_pointer_resolved` at design-canvas coords before the next render (press still
  enqueues host actions drained in `engine.update`).
- **hit_test / active_tier:** read the published snapshot on the caller's thread. No enqueue, no
  block. `hit_test` runs `Scene::hit_kind` against the snapshot (the additive engine helper shipped in
  v1). Returns stale-by-≤1-frame classification.
- **invalidate:** enqueue a one-shot render request + unpark (wakes a paused skin; no-op coalesced if
  already due).
- **set_frame_rate:** enqueue the new target fps (0 = paused). Applied by the loop.
- **release_surface:** mark surface index free for reuse; enqueue + unpark (may unblock a loop waiting
  for a free surface).
- **destroy:** set a shutdown flag + unpark → loop exits → `join` → free the handle. Idempotent on
  null; valid on a poisoned/exited thread.

## C ABI surface (v2)

`carapace_abi_version()` returns **2 << 16** (MAJOR bump). This is a **breaking** change — justified
because no external consumer links carapace-ffi's ABI yet (the Swift/Flutter samples still link the
frozen `embed-spike`; porting them is separate future work). Additive-only discipline resumes from
v2 onward.

```c
typedef struct CarapaceEngine CarapaceEngine;   // opaque, now thread-safe

// Statuses: v1 set, unchanged.
typedef enum { CARAPACE_OK=0, CARAPACE_ERR_NULL_ARG=1, CARAPACE_ERR_BAD_SKIN=2,
               CARAPACE_ERR_GPU_INIT=3, CARAPACE_ERR_POISONED=4, CARAPACE_ERR_PANIC=5 } CarapaceStatus;

typedef enum { CARAPACE_PRESS=0, CARAPACE_RELEASE=1, CARAPACE_MOVE=2,
               CARAPACE_ENTER=3, CARAPACE_LEAVE=4 } CarapacePointerKind;
typedef enum { CARAPACE_PASSTHROUGH=0, CARAPACE_CONTROL=1, CARAPACE_DRAG=2 } CarapaceHitKind;
typedef enum { CARAPACE_TIER_READBACK=1, CARAPACE_TIER_SHARED=2 } CarapaceTier;

typedef struct {
  void* ctx;
  bool  (*get_num)(void* ctx, const char* key, double* out);
  bool  (*get_str)(void* ctx, const char* key, char* buf, size_t cap);
  void  (*invoke)(void* ctx, const char* action);
  // NEW in v2 — fired on the render thread when a frame lands in surfaces[index].
  void  (*frame_ready)(void* ctx, uint32_t surface_index, uint64_t frame_id);
} CarapaceHostVTable;

typedef struct {
  const char*             skin_dir;         // caller-owned, borrowed for the call
  CarapaceHostVTable      vtable;           // fn-ptrs + ctx must outlive the engine; called on the render thread
  const void* const*      surfaces;         // array of `surface_count` caller-owned BGRA IOSurfaces (w x h)
  uint32_t                surface_count;    // 2 or 3 (2 = double buffer, 3 = triple)
  const void*             content_surface;  // optional live host content; null = none
  uint32_t                w, h;             // SURFACE pixel size (2x on Retina)
} CarapaceCreateDesc;

uint32_t       carapace_abi_version(void);                              // == 2<<16
CarapaceStatus carapace_create(const CarapaceCreateDesc* desc, CarapaceEngine** out);  // blocks for thread init
CarapaceStatus carapace_pointer(CarapaceEngine*, double x, double y, CarapacePointerKind kind);  // enqueue, returns now
CarapaceStatus carapace_hit_test(CarapaceEngine*, double x, double y, CarapaceHitKind* out);      // reads snapshot
CarapaceStatus carapace_active_tier(CarapaceEngine*, CarapaceTier* out);                          // reads snapshot
CarapaceStatus carapace_invalidate(CarapaceEngine*);                    // render one frame now (wake a paused skin)
CarapaceStatus carapace_set_frame_rate(CarapaceEngine*, uint32_t fps);  // 0 = paused/on-demand; default 60
CarapaceStatus carapace_release_surface(CarapaceEngine*, uint32_t index);  // host done displaying; reuse allowed
size_t         carapace_last_error(char* buf, size_t cap);              // copies; bytes needed
void           carapace_destroy(CarapaceEngine*);                       // signals + joins the thread; null-safe
```

**Removed from v1:** `carapace_tick(handle, dt)`. The loop ticks internally with wall-clock `dt`; the
host no longer supplies frames or `dt`. `invalidate` replaces the on-demand use of `tick`.

### Conventions (deltas from v1; v1's string/coordinate/versioning rules otherwise hold)

- **Handle threading.** The handle is now `Send + Sync` across the ABI: `pointer`/`hit_test`/
  `active_tier`/`invalidate`/`set_frame_rate`/`release_surface`/`destroy` may be called from any
  thread. Internally, input calls enqueue (channel is the sync boundary), query calls read the atomic
  snapshot. `create`/`destroy` are still expected once each, but from any thread.
- **Surface ownership & lifetime.** All `surface_count` IOSurfaces (and `content_surface`) are
  caller-owned and must outlive the engine (until `destroy` returns). carapace never frees them.
  `frame_ready` transfers *display* rights for `surfaces[index]` to the host; `release_surface(index)`
  transfers them back. A surface the host currently holds is never rendered into.
- **Frame ids.** `frame_id` is a monotonically increasing `u64` starting at 1, so the host can order
  `frame_ready` callbacks and detect drops.

## Cadence & pacing (§3 in detail)

- The loop keeps a target frame interval (`1/fps`, default 60). When **running** (`fps > 0`) it renders
  each interval using wall-clock `dt = now - last_render` (clamped ≥ 0, and capped to avoid a huge
  first/after-idle `dt` — e.g. `min(dt, 4 * interval)`). When **paused** (`fps == 0`) it renders only
  on `invalidate`/pointer, still using wall-clock `dt`.
- The host does **nothing** to animate: it mutates bound values (spectrum, level, track position)
  whenever, and the running loop samples them via the vtable on the next frame. This is the adoption
  guarantee — link it, hand it surfaces + value callbacks, it animates.
- **Backpressure:** if no surface is free (host holds them all), the loop skips presenting this
  interval rather than blocking or tearing; it retries next interval / on `release_surface`. With 3
  surfaces this is rare. A dropped interval is logged at debug, never an error.
- **Future additive hook:** an engine `next_wake()`/`is_animating()` query would let a running loop
  automatically coast to idle when nothing is animating (no host `set_frame_rate` needed). Out of
  scope here — the engine has no animation clock today (`os.time()` is blocked; no `anim{}`/tween
  primitives; animation is entirely host-value-driven).

## Sync queries via snapshot (§4 in detail)

After presenting, the render thread publishes `Arc<SceneSnapshot>` (the laid-out `Scene` + active
`Tier`) into an atomic cell (`arc_swap::ArcSwap`, or a `Mutex<Arc<..>>` if we avoid the dep — decided
at plan time; `arc-swap` fetched via `sfw` if chosen). `hit_test`/`active_tier` load the current
`Arc` and read it without touching the render thread:

- `active_tier` → the snapshot's tier.
- `hit_test` → `Scene::hit_kind(p)` on the snapshot scene (the v1 additive engine helper), mapped to
  `CarapaceHitKind`. No Lua fired, no layout — the layout is already baked into the snapshot.

Before the first frame is published, queries return a defined default (`active_tier` = the tier chosen
at create, available immediately; `hit_test` = `PASSTHROUGH`).

## Host callback contract (§5 in detail)

`get_num`/`get_str`/`invoke`/`frame_ready` all fire on **carapace's render thread**, not the host's UI
thread. Rules, documented in the header and enforced by convention:

- Callbacks **must be thread-safe** (the host reads/writes its own state from carapace's thread).
- Callbacks **must not block** — they run inline in the frame loop; a slow callback lowers frame rate.
- Callbacks **must not call any `carapace_*` function** — that reenters the queue/loop and can
  deadlock. A host that needs to react (e.g. hop to its UI thread) must post asynchronously.
- Input string args (`get_str` key, `invoke` action) are borrowed for the call only, as in v1.

## The unsafe crux (§7 in detail)

Moving the `!Send` `Engine` and the host's raw vtable (`ctx: *mut c_void` + fn-ptrs) onto a spawned
thread requires a **narrow, explicitly-documented `Send` wrapper** — precisely the construct v1's
cleanup removed when it was an *unsound, implicit* blanket impl (commit e4a56d7, "drop unsound Send
impl"). Here it is sound-by-contract and localized:

- The `Engine` is **built on the render thread** (skin source + registry sent over; skin source is
  data, `Send`). It never crosses back.
- Only the **vtable pointers** cross at spawn, inside a `struct SendVTable(CarapaceHostVTable)` with a
  hand-written `unsafe impl Send`. Its safety comment states the contract from §5: the host guarantees
  its callbacks are safe to invoke from carapace's render thread. This is the load-bearing assumption
  and the single most-scrutinized unsafe block — it gets a dedicated doc comment, review focus, and
  the reentrancy/thread-safety tests below.
- The published `Arc<SceneSnapshot>` must be `Send + Sync`: `Scene` is plain data (verified at plan
  time; if any `Rc` sneaks in, the snapshot converts to an owned/`Arc` form on publish).

## Panic / poison across the thread (§6 in detail)

- The render-loop body runs inside `catch_unwind`. A caught panic: records the message into
  `last_error` (the v1 panic hook still applies), sets the atomic `poisoned = true`, and the thread
  **exits cleanly**. Never `abort()` — carapace runs in the host's process.
- Every subsequent `carapace_*` call checks `poisoned` first and returns `ERR_POISONED` without
  enqueuing or reading a stale snapshot. `create` panicking on init returns `ERR_PANIC` with `*out`
  null (thread joined).
- `destroy` on a poisoned/exited thread still joins (the thread has already returned) and frees.
- Recovery is unchanged: `destroy` + fresh `create`.

## Testing

- **Host-portable unit tests (all CI targets, no GPU):**
  - `queue.rs`: command FIFO ordering; consecutive `Move` coalescing keeps the latest and preserves
    press/release ordering; drain empties.
  - `snapshot.rs`: publish/load returns the latest `Arc`; concurrent readers never see a torn value;
    pre-first-frame defaults (`PASSTHROUGH`, create-time tier).
  - Scheduling: with an injected fake clock + stub engine, running loop renders at ~target interval;
    paused loop renders only on invalidate; `dt` clamp caps a huge idle gap.
  - Poison: a forced-panic command poisons the thread; subsequent calls return `ERR_POISONED`;
    `destroy` still joins/frees; `last_error` populated.
  - Callback threading: `frame_ready`/`get_num` fire from a non-creator thread (assert thread id
    differs from the caller's).
  - `carapace_abi_version() == 2<<16` matches the header constant.
- **Apple-gated GPU tests** (`#[cfg(any(target_os="macos", target_os="ios"))]`):
  - End-to-end: create (2- and 3-surface pools) → running loop → `frame_ready` fires with increasing
    `frame_id` → the signalled surface has non-blank pixels.
  - Triple-buffer rotation: over several frames, `frame_ready` cycles through free surface indices and
    never a host-held one; `release_surface` returns a slot to rotation.
  - Backpressure: hold all surfaces → loop skips (no crash, no tear); release → rendering resumes.
  - `hit_test` over the published snapshot classifies control/drag/passthrough correctly after a frame
    lands; matches a synchronous layout of the same scene.
  - `invalidate` on a paused (`fps=0`) engine produces exactly one `frame_ready`.
- **Header freshness test:** regenerate `carapace.h` via cbindgen, assert byte-equality with the
  committed header.
- **Lint gate:** `clippy -D warnings` (+ the `gpu-tests` variant) — CI gates on it.
- **Concurrency hygiene:** the poison + callback-threading + snapshot tests run under a
  `cargo test`/CI pass; where practical, exercise them enough to catch obvious races (a `loom` model
  of the queue/snapshot is a noted possible follow-up, not required here).

## Non-goals (each additive; none break the v2 ABI)

- **Windows / Linux / Android backends.** Still deferred; v2 ships Apple only. The C ABI +
  descriptor-struct + additive-export design accepts them without an ABI break (paths recorded in the
  v1 research: D3D11 shared-handle→Vulkan, dmabuf, AHardwareBuffer, all via `wgpu-hal`).
- **Engine `next_wake()` / `is_animating()` self-scheduling** — the future hook that would let a
  running loop auto-idle; needs an engine animation model that doesn't exist yet.
- **Per-pixel GPU-alpha shaped mask / click-through** — still refines v1's geometry-based coverage.
- **Full push event channel** (region-change / drag-state callbacks beyond `frame_ready`) — pairs with
  future window-drag work; `frame_ready` is the first push primitive.
- **Porting the Swift / Flutter samples to carapace-ffi** — proven separately once v2 is stable; this
  is what will exercise the surface-pool + `frame_ready` contract against real host toolkits.
- **Resize / live surface-pool resize** — `Command::Resize` is designed for but not implemented here;
  v2 fixes surface size at create.

## Risks

- **The `Send` wrapper is the crux.** An unsound version was removed in v1 for good reason. Mitigation:
  the impl is one narrow wrapper with an explicit safety contract tied to the documented callback
  rules (§5), reviewed in isolation, and covered by the callback-threading test.
- **Callback reentrancy deadlock.** A host calling `carapace_*` from inside a callback deadlocks.
  Mitigation: documented prohibition + (optional) a debug-build reentrancy guard that returns
  `ERR_POISONED`/logs rather than hanging.
- **Present backpressure / dropped frames** if the host is slow to `release_surface`. Mitigation: 3
  surfaces by default; skip-not-block policy; debug logging of drops.
- **`wgpu-hal` instability** (unchanged from v1): zero-copy import stays pinned to the proven
  `=29.0.3`; any bump re-verifies `try_shared`.
- **Snapshot staleness for hit-testing** (≤1 frame). Accepted for chrome; documented.
- **Blocking `create`** waits on thread init — bounded by GPU device creation (already synchronous in
  v1). No new unbounded wait.

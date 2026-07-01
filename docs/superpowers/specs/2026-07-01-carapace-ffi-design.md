# carapace-ffi v1 — design (2026-07-01)

Productionize the throwaway `embed-spike` into a real, safe, versioned C ABI: `crates/carapace-ffi`.
This spec is scoped to the **first shippable increment** — Apple (macOS/iOS) only, single-threaded,
using the already-proven `wgpu-hal` IOSurface zero-copy path. It closes the spike's panic holes,
imposes ABI/error/string discipline, rounds out pointer input, and adds a first engine→host hit-test
channel (the north-star window-replacement enabler). It deliberately does **not** yet build the
render thread or the Windows/Linux/Android backends.

Grounded in `2026-07-01-carapace-ffi-research.md` (+ its review addendum), the proven
`embed-spike` ABI (`crates/embed-spike/src/{lib,host,render}.rs`), and the window-replacement spike
findings (`2026-06-19-window-replacement-spike-findings.md`).

## Locked scope

- **Platform:** Apple (macOS/iOS) only. Single-threaded — no render thread yet. Proven `wgpu-hal`
  IOSurface Tier-2 (zero-copy) path + Tier-1 readback fallback, ported from the spike.
- **Crate:** new `crates/carapace-ffi`. `embed-spike` stays **frozen** as reference; its samples
  keep linking it. No sample is ported in this increment.
- **Safety:** every export is panic-guarded — catch-unwind + poison + error code. **Never `abort()`**
  the host process.
- **ABI:** flat, versioned C ABI over opaque handles; cbindgen-generated committed header;
  `carapace_abi_version()`; additive-only exports; `carapace_last_error()`; documented
  string-ownership (zero `free()` contract).
- **Surface:** spike parity (2-tier present, host vtable, host-view cutout) + full pointer input
  (press/release/move/enter/leave) + engine→host channel (`hit_test` region kind + drag-region +
  geometry shape query).
- **Engine:** small **additive** changes to the `carapace` crate are permitted — a declarative
  region role and hit-classification/coverage helpers surfaced through hit-testing.

Chosen design axes (from the brainstorm): **A1** error model (return-code + thread-local
`last_error`), **B1** panic guard (one `ffi_guard!` macro), **C1** engine→host (pull hit-test +
geometry-based shape).

## Architecture & crate layout

`crates/carapace-ffi`, `crate-type = ["cdylib", "staticlib", "rlib"]` (mirrors the spike: iOS links a
static archive). The whole FFI module is `#[cfg(any(target_os = "macos", target_os = "ios"))]`-gated;
on other targets the crate builds as an empty shell (so `cargo build`/clippy on Linux CI stays green,
matching how the spike gates its Metal-only code).

Layered so each file has one job:

| File | Responsibility |
|---|---|
| `abi.rs` | The `#[no_mangle] extern "C"` exports. Thin: validate args → translate → delegate to `handle`. Every body wrapped in `ffi_guard!`. |
| `guard.rs` | `ffi_guard!` macro, `CarapaceStatus` enum, thread-local `last_error`, one-time panic hook install. |
| `handle.rs` | The opaque `CarapaceEngine` struct + its methods (owns `GpuCtx`, `Renderer`, `Engine`, `Present`, surfaces, `content`, `tier`, sizes, and `poisoned: bool`). No render loop — synchronous, driven by `carapace_tick`. |
| `host.rs` | `CarapaceHostVTable` + `FfiHost` — ported from the spike, string lifetimes hardened. |
| `render.rs` | GPU/present internals ported from the spike, with **`init_gpu() -> Result`** (no `.expect()`) and the Tier-2 import returning `Result`/`Option` cleanly. |
| `hit.rs` | Maps engine hit + role → `CarapaceHitKind`; drives the shape/coverage query. |
| `include/carapace.h` | cbindgen-generated, **committed to git**, guarded by a freshness test. |
| `cbindgen.toml` | cbindgen config (C output, enum/struct naming, include guard). |

`cbindgen` is a new build-time dependency; its first fetch runs through Socket Firewall
(`sfw cargo add --build cbindgen ...`) per repo policy.

## C ABI surface

Opaque handle + `repr(C)` types. `create` takes a **descriptor struct** so it can grow additively
without breaking its signature. Enums are `int32`-backed and additive (new variants appended).

```c
typedef struct CarapaceEngine CarapaceEngine;   // opaque

typedef enum {
  CARAPACE_OK             = 0,
  CARAPACE_ERR_NULL_ARG   = 1,   // a required pointer arg was null / non-UTF-8
  CARAPACE_ERR_BAD_SKIN   = 2,   // skin dir failed to load / parse
  CARAPACE_ERR_GPU_INIT   = 3,   // no Metal adapter / device request failed
  CARAPACE_ERR_POISONED   = 4,   // handle previously panicked; destroy + recreate
  CARAPACE_ERR_PANIC      = 5,   // a panic was caught in this call
} CarapaceStatus;                // int32

typedef enum { CARAPACE_PRESS=0, CARAPACE_RELEASE=1, CARAPACE_MOVE=2,
               CARAPACE_ENTER=3, CARAPACE_LEAVE=4 } CarapacePointerKind;

typedef enum { CARAPACE_PASSTHROUGH=0, CARAPACE_CONTROL=1,
               CARAPACE_DRAG=2 } CarapaceHitKind;

typedef enum { CARAPACE_TIER_READBACK=1, CARAPACE_TIER_SHARED=2 } CarapaceTier;

typedef struct {
  void* ctx;
  bool  (*get_num)(void* ctx, const char* key, double* out);
  bool  (*get_str)(void* ctx, const char* key, char* buf, size_t cap);
  void  (*invoke)(void* ctx, const char* action);
} CarapaceHostVTable;

typedef struct {
  const char*        skin_dir;         // caller-owned, borrowed for the call
  CarapaceHostVTable vtable;           // fn-ptrs must outlive the engine
  void*              surface;          // caller-owned IOSurface (BGRA, w x h), outlives engine
  void*              content_surface;  // optional live host content; null = none
  uint32_t           w, h;             // SURFACE pixel size (2x on Retina)
} CarapaceCreateDesc;

uint32_t       carapace_abi_version(void);   // MAJOR<<16 | MINOR, compile-time
CarapaceStatus carapace_create(const CarapaceCreateDesc* desc, CarapaceEngine** out);
CarapaceStatus carapace_tick(CarapaceEngine*, double dt_seconds);
CarapaceStatus carapace_pointer(CarapaceEngine*, double x, double y, CarapacePointerKind kind);
CarapaceStatus carapace_hit_test(CarapaceEngine*, double x, double y, CarapaceHitKind* out);
CarapaceStatus carapace_active_tier(CarapaceEngine*, CarapaceTier* out);
size_t         carapace_last_error(char* buf, size_t cap);  // copies; returns bytes needed
void           carapace_destroy(CarapaceEngine*);           // idempotent on null; frees poisoned
```

### Conventions (doc-wide, non-negotiable)

- **String ownership — zero `free()` contract.** Strings crossing *in* (`skin_dir`, the `get_str`
  key, action names) are caller-owned and borrowed only for the duration of the call. Strings
  crossing *out* (`last_error`, and the `get_str` result the host writes into our buffer) are
  **copied into a caller-provided buffer**, NUL-terminated and truncated to `cap`. carapace never
  returns a pointer the caller must free, and never frees a caller's pointer.
- **Coordinates.** `carapace_pointer` and `carapace_hit_test` take **design-canvas** coordinates
  (0..cw, 0..ch), matching the spike: the host maps its click into design space; layout + hit-test
  happen there. `w`/`h` in the descriptor are the **surface** pixel size (may be 2× the canvas on
  Retina).
- **Handle threading.** The handle is single-threaded and **not `Send`/`Sync` across the ABI** — the
  caller must create, tick, and destroy it from one thread. (The render-thread increment will relax
  this; v1 documents the constraint rather than enforcing concurrency.)
- **Versioning.** `carapace_abi_version()` returns `MAJOR<<16 | MINOR`. The header carries the same
  constants. Compatibility rule: exports and enum variants are **additive only**; struct fields are
  appended; behavior-changing edits bump `MAJOR`.

## Panic safety (B1)

A single audited `ffi_guard!` macro wraps each export body:

```rust
// Handle-bearing calls: poison the handle on panic.
macro_rules! ffi_guard {
    ($handle:expr, $body:expr) => {{
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(status) => status,
            Err(_) => {
                // last_error already populated by the panic hook (below).
                if let Some(h) = unsafe { $handle.as_mut() } { h.poisoned = true; }
                CarapaceStatus::ErrPanic as i32
            }
        }
    }};
}
```

- Declaration stays plain **`extern "C"`** (not `"C-unwind"`) — panics are caught *inside*, so
  nothing ever unwinds across the boundary. Under `panic=unwind` a panic escaping `extern "C"`
  aborts (guaranteed since Rust 1.81); catching it first makes that unreachable.
- `AssertUnwindSafe` is justified because on the panic path we **discard or poison** the touched
  state rather than continue using it.
- A one-time **panic hook** (installed via `std::sync::Once` on first `carapace_create`, chaining the
  previous hook) captures the panic message + location into the thread-local `last_error` *before*
  the unwind reaches the guard — because `catch_unwind`'s payload is opaque, the hook is how we get a
  human-readable message. The hook must not itself panic and touches only the thread-local.
- A handle-less variant of the guard (for `carapace_create`) sets `last_error`, returns `ErrPanic`,
  and leaves `*out` null.

### Poison semantics — when do we enter poison?

Poison is entered in exactly one situation: **a panic is caught by `ffi_guard!` inside a call that
holds a handle** (`carapace_tick` / `_pointer` / `_hit_test` / `_active_tier`). At that instant the
engine's interior may be half-mutated (partial layout, a `RefCell` left borrowed, a queue mid-drain),
so the instance is treated as radioactive: `poisoned = true`, message → `last_error`, return
`ERR_PANIC`. Every later guarded call on that handle **short-circuits with `ERR_POISONED` before
touching engine state**. Recovery is `carapace_destroy` (still valid on a poisoned handle) + a fresh
`carapace_create`.

Does **not** poison: expected `Err` returns (bad skin, GPU-init failure, null args — returned before
state is corrupted); Tier-2 import failure (falls back to Tier 1, still `CARAPACE_OK`);
`carapace_create` panicking (no handle exists yet). Poison is the last-resort net under the
`Result`-based taxonomy — in a correct build it never fires. We poison-and-continue instead of
`abort()` because carapace runs inside the **host's** process; one skin's bug must not kill the host
app.

## Engine→host hit-test (C1) — additive engine changes

Two small, non-breaking additions to the `carapace` crate (precedent: PR #23 added `math` to the
engine for FFI):

1. **Region role.** A `hotspot{}` may declare `role = "drag" | "passthrough" | "control"` (default
   `control`). Parsed additively via the vocab layer; existing skins are unaffected (absent = control).
2. **Non-firing classification + coverage.** Add to `Scene`:
   - `hit_kind(p) -> HitKind` where `HitKind ∈ {Control, Drag, Passthrough}` — classifies the point
     from the topmost interactive node's role **without firing Lua** (unlike `handle_pointer_resolved`,
     which executes handlers). Control = topmost interactive node (button/list/scrub, or a
     `role=control` hotspot); Drag = topmost node is a `role=drag` hotspot; Passthrough = the point is
     outside the skin's opaque coverage (see below) or over a `role=passthrough` region.
   - `covers(p) -> bool` — is the point inside the skin's opaque shape? Computed from **coverage
     geometry**: the union of image-dest rects, hotspot polygons, and fills. Geometry-based,
     allocation-free; per-pixel-enough for chrome. (True per-pixel GPU-alpha masking is a named
     later refinement.)

`carapace_hit_test` maps `Scene::hit_kind` (after laying out at the design canvas) to
`CarapaceHitKind` with no side effects. **The existing press→`begin_drag` action path is untouched**
— `hit_test` only adds *reporting*, so the host can pre-classify an OS event (move the window vs. let
the skin consume it vs. click-through) before dispatching it.

## Data flow

- **create:** `desc` → `skin::load_dir` (`Err` → `ERR_BAD_SKIN`) → `Engine::new(FfiHost::new(vtable))`
  → `init_gpu()?` (`Err` → `ERR_GPU_INIT`) → `try_shared` for Tier-2 IOSurface import, else Tier-1
  readback → optional `content_surface` → normal content texture → `Box::into_raw` → `*out`. Any
  `Err` sets `last_error`, returns the status, and leaves `*out` null. Whole body under the guard.
- **tick:** guard → if content present, upload this frame's content surface into the content texture
  (CPU→GPU coherency) → `render_frame` → present (Tier-2 GPU blit RGBA→BGRA into the IOSurface
  texture, or Tier-1 readback + swizzle into the IOSurface). `dt` is host wall-clock seconds
  (`max(0.0)`).
- **pointer:** guard → map `CarapacePointerKind` → engine `PointerEvent` → `handle_pointer_resolved`
  at design-canvas coords → enqueues host actions, drained on the next `tick`. (v1 forwards
  press/release/move/enter/leave; the engine currently acts on press — the others are plumbed for
  forward-compat and hover, wired to whatever the engine models now, additively.)
- **hit_test:** guard → layout at design canvas → `Scene::hit_kind(p)` → `*out`. No side effects, no
  Lua fired.
- **active_tier:** guard → `*out = tier`.
- **last_error:** copy the thread-local into `buf` (truncate + NUL-terminate), return bytes needed.
  Handle-less, infallible, not guarded for poison.
- **destroy:** guard → `Box::from_raw` drop; null-safe and valid on a poisoned handle.

## Error taxonomy

| Condition | Result |
|---|---|
| Success | `CARAPACE_OK` |
| Null / non-UTF-8 required arg | `ERR_NULL_ARG` |
| Skin load/parse failure | `ERR_BAD_SKIN` |
| No Metal adapter / device request failed (was `.expect()`) | `ERR_GPU_INIT` |
| Panic caught in this call | `ERR_PANIC` (+ poison if handle present) |
| Call on a poisoned handle | `ERR_POISONED` |
| **Tier-2 IOSurface import failed** | **not an error** — silent fallback to Tier 1, `CARAPACE_OK` |

`init_gpu` is rewritten to return `Result` (both `request_adapter` and `request_device` mapped to
`ERR_GPU_INIT` with a descriptive `last_error`), closing the live
`crates/embed-spike/src/render.rs:20,41` `.expect()` holes.

## Testing

- **Host-portable unit tests** (no GPU, run on all CI targets):
  - Host-vtable mapping (port the spike's `get`/`invoke` tests).
  - `ffi_guard!` poisons the handle and returns `ERR_PANIC` on a deliberately panicking body; a later
    call on that handle returns `ERR_POISONED`.
  - `last_error` round-trip + truncation at `cap` + NUL-termination + bytes-needed return.
  - `carapace_abi_version` equals the header constants.
  - Null-arg handling → `ERR_NULL_ARG` for each fallible export.
  - `Scene::hit_kind`/`covers` classification on a synthetic scene (control vs drag vs passthrough),
    plus `role` parsing in the engine crate.
- **Apple-gated GPU tests** (`#[cfg(any(target_os="macos", target_os="ios"))]`, mirroring the spike's
  Apple-only `render_png` gating): create→tick→readback a known skin and assert non-blank/expected
  pixels; Tier selection reported correctly; content-view upload composites; `hit_test` end-to-end
  over a laid-out skin.
- **Header freshness test:** regenerate `carapace.h` via cbindgen in-memory and assert byte-equality
  with the committed `include/carapace.h` (CI fails on drift).
- **Lint gate:** `clippy -D warnings` (and the `gpu-tests` variant) must pass — CI gates on it.

## Non-goals (named follow-ons; each additive, none break this ABI)

- **Render thread + command queue** — the research's central recommendation; the next increment.
  Relaxes the single-thread handle constraint and fixes synchronous readback latency.
- **Windows / Linux / Android backends** — the `wgpu-hal` D3D11→Vulkan, dmabuf, and AHardwareBuffer
  paths from the research.
- **Per-pixel GPU-alpha shaped mask / click-through** — refines C1's geometry coverage.
- **Push engine→host event channel** (callbacks on region change) — pairs with the render thread's
  threading contract.
- **Porting the Swift / Flutter samples** to carapace-ffi — proven separately once the ABI is stable.

## Risks

- **`wgpu-hal` instability.** Zero-copy import rides `wgpu-hal` internals (pinned `=29.0.3`). Any wgpu
  bump must re-verify `try_shared`. Mitigated in v1 by staying on the exact version the spike proved.
- **Panic-hook global state.** The one-time hook is process-global and chains the prior hook; it must
  be installed exactly once and never panic. Covered by a dedicated unit test.
- **`hit_kind` vs. layout cost.** `hit_test` lays out the scene per call (as the spike's pointer path
  already does). Acceptable for per-event use at v1 scale; the render-thread increment can cache the
  laid-out scene.

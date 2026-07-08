# Live host-view region — content registry (sub-project 1)

**Date:** 2026-07-08
**Status:** design approved, pre-implementation
**Scope:** engine + carapace-ffi + showcase wiring. First of two sub-projects.

## Goal

Generalize the "live host-view region" — a skin-declared `view{}` cutout the host fills with its own live content — from today's single, hardcoded, create-time-fixed surface into a **view-id-keyed content registry** the host can attach to, replace, and clear **at runtime**. This delivers two capabilities from one design:

1. **Multiple named regions** — a skin can declare several `view{}` cutouts, each filled independently.
2. **Live attach/replace/clear** — the host can change what's in a region after engine creation, without a full skin swap.

This sub-project builds the engine/FFI foundation and proves it with the existing synthetic dither. **Sub-project 2** (separate spec) puts *real* live content — actual video — into a cutout on top of this API; it is out of scope here.

## Current state (what exists, and the gap)

The renderer is already general; everything above it is hardwired to one region.

- **`view{}` vocab** (`crates/carapace/src/vocab.rs:365-381`): requires `id, x, y, w, h`. Becomes `Node::View { id, dest }` (`scene.rs:303-308`). `Scene::views()` (`scene.rs:519-528`) already enumerates all view nodes.
- **Renderer** (`crates/carapace/src/render.rs`): `Node::View` is skipped in the vello pass and composited in a dedicated loop (`render.rs:586-600`) that iterates **every** view node, resolves its id via a `view_tex: Fn(&str) -> Option<&TextureView>` closure, and blends each match. **The renderer already supports N cutouts per frame.**
- **The choke point** (`crates/carapace-ffi/src/render.rs:93-115`): `render_frame` takes `host_view: Option<(&str, &wgpu::TextureView)>` and builds a closure that resolves **at most one** id. So `draw()` can composite N, but only ever one pair is fed in.
- **Content plumbing is single-instance and create-fixed:**
  - `CarapaceCreateDesc.content_surface` — one `IOSurfaceRef` (`handle.rs:84-85`).
  - `ContentTex` — one struct; render thread holds `content: Option<ContentTex>` (`render_thread.rs:84`).
  - The view id is the hardcoded literal `"host"`, appearing 4× in `render_thread.rs` (`308, 348, 384, 408`): `content.as_ref().map(|c| ("host", &c.view))`.
  - Content is a BGRA (`0x42475241`) IOSurface, CPU-copied to a *normal* wgpu texture each frame via `upload_iosurface_to_texture` (`render.rs:363`) — deliberately not IOSurface-aliased, to avoid a frozen-first-frame bug (`handle.rs:23-27`).
  - Content is settable only at `carapace_create` or `carapace_swap_skin_resized` (which forces a full pool rebuild). No runtime attach/replace/clear export exists.
- **Showcase** (`showcase/Sources/Showcase/`): `ditherSurface(forDir:)` (`App.swift:139-153`) gates on the Studio skin, builds a BGRA IOSurface via `DitherRenderer` (474×214, `DitherRenderer.swift:14-16`), and passes it as `content_surface` at create (`CarapaceBridge.swift:33,64`) and swap (`CarapaceBridge.swift:100-122`). Studio's skin declares `view{ id="host", x=20, y=74, w=474, h=214 }` (`skin.lua:19`). The `"host"` binding lives entirely engine-side; Swift only supplies the surface.

**Gap to close:** one content surface → many, keyed by view id; and create-fixed → runtime-mutable. The renderer needs no change beyond feeding its existing N-capable loop from a map.

## Design

### Semantics (decided)

- **A1 — Registry persists across skin swaps.** Content is independent of skins: an attached surface stays until the host explicitly replaces or clears it. Each frame the renderer matches the *current* skin's `view{}` ids against the registry. A registry entry with no matching view is a harmless no-op; a `view{}` with no entry renders nothing (unchanged from today).
- **B1 — Create/swap seed params kept.** `CarapaceCreateDesc.content_surface` and `carapace_swap_skin_resized`'s content param are retained; each **seeds/replaces the `"host"` key** in the registry. Existing callers keep working unchanged; the new export is purely additive.

### Data model (render thread)

Replace `content: Option<ContentTex>` with:

```rust
content: HashMap<String, ContentTex>   // keyed by view_id
```

`ContentTex` is unchanged (`{ surface, tex, view, w, h }`). `render_one`'s per-frame upload loops over all entries (`upload_iosurface_to_texture` per entry) instead of the single `Option`. The `host_view` fed to `render_frame` changes from one pair to the whole map (see below).

### FFI export

New, additive export (ABI **3.2 → 3.3**, minor bump):

```c
CarapaceStatus carapace_set_content_surface(
    CarapaceEngine* engine,
    const char*     view_id,
    const void*     surface,   /* IOSurfaceRef; NULL = clear this view_id */
    uint32_t        w,
    uint32_t        h);
```

- Non-null `surface` → attach or replace the entry for `view_id` (build a fresh `ContentTex`).
- `NULL` surface → remove the entry for `view_id` (clear).
- Routed to the render thread as a new command `Command::SetContent { view_id: String, surface: *const c_void, w, h, reply }`.
- **Blocking contract:** the call returns only after the render thread has applied the change (attach/replace/clear) — mirroring `carapace_swap_skin_resized`. This guarantees the host may free a replaced-or-cleared surface the instant the call returns, with no use-after-free against the render thread's per-frame CPU copy. This is the same discipline used for the pinned-dither swap fix.
- Marshalling: the `view_id` C string is copied to an owned `String` on the caller side before the command crosses the thread boundary (the pointer is not held). The surface pointer is host-owned and non-retained, consistent with the existing `content_surface` contract in `carapace.h`.

### Renderer feed

`render_frame`'s `host_view: Option<(&str, &wgpu::TextureView)>` becomes a borrow of the content map (e.g. `content_views: &HashMap<String, ContentTex>`), and the `view_tex` closure resolves `id` by map lookup, feeding every matching `(id, tex)` to the already-N-capable composite loop in `Renderer::draw`. No change to `crates/carapace/src/render.rs`'s composite pass.

### Create/swap seeding

`carapace_create` builds a `ContentTex` from its `content_surface` (when non-null) and inserts it under `"host"` in the otherwise-empty registry.

`carapace_swap_skin_resized` **preserves the registry** (A1). A resized swap keeps the same GPU device — it rebuilds only the output present pool and the scratch offscreens — so the registry's `ContentTex` wgpu textures remain valid and are carried over untouched. The swap's `content_surface` param, when non-null, simply **re-seeds/replaces the `"host"` entry** (build a new `ContentTex`, insert under `"host"`); other entries set via `carapace_set_content_surface` survive the swap unchanged. Plain `carapace_swap_skin` does not touch the registry either. So A1 holds uniformly across every swap path — there is no reset exception.

### Lifetime & thread-safety

- The `SetContent` command copies the `view_id` into an owned `String`; the render thread owns the resulting `ContentTex`.
- Attach/replace builds the new `ContentTex` (imports the IOSurface into a wgpu texture via the existing `build_content` path, generalized to take a surface + dims) before dropping any prior entry for that id, then swaps it in.
- Clear removes the entry (drops its `ContentTex`, freeing the wgpu wrapper) before the blocking reply is sent — so the host's subsequent free of the IOSurface cannot race the render thread.

## Showcase changes

Rework the showcase to drive content through the new API (one code path), and add a second region to prove multi-region:

- On entering Studio: `bridge.setContentSurface("host", ditherSurface, w, h)`.
- On leaving Studio (before dropping `DitherRenderer`): `bridge.setContentSurface("host", nil, 0, 0)` — the blocking clear ensures the render thread released its `ContentTex` before the surface is unmapped.
- The create/swap seed params stay wired for backward-compat but the showcase no longer relies on them for the dither.
- **Multi-region proof:** add a second `view{ id="viz", … }` cutout to Studio's `skin.lua`, fed by a second cheap `DitherRenderer` (different Bayer phase/color) attached via `setContentSurface("viz", …)`. Confirms two named regions composite in one frame end-to-end.
- `CarapaceBridge` gains a `setContentSurface(_ viewId:, _ surface:, _ w:, _ h:)` wrapper over the new export.

## Testing

- **Engine/FFI GPU tests** (`--features gpu-tests`, macOS):
  1. Attach two surfaces on two view ids the skin declares → both regions non-blank.
  2. Clear one → it goes blank (view renders nothing), the other remains.
  3. Replace a surface → new pixels appear in that region.
  4. Attach for a `view_id` the skin does not declare → no crash, nothing rendered.
  5. Blocking-clear returns only after the render thread dropped the `ContentTex` (assert via the command reply — guards the UAF contract).
- **Header/ABI:** `carapace.h` mirrors the new export; `CARAPACE_ABI_MINOR` = 3; the ABI assertion test in `lib.rs` updated to 3.3; header test passes.
- **Showcase:** `swift build` + `swift test`; manual — cycle skins, both Studio cutouts animate, content clears cleanly on leaving Studio.
- **Local gate before push:** `cargo fmt --all`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo clippy --locked -p carapace-ffi --all-targets --features gpu-tests -- -D warnings`, `cargo test` — all green.

## Out of scope (sub-project 2 and beyond)

- Real live-content demo (actual video via AVPlayer / camera) — separate spec, built on this API.
- Host-supplied *GPU-resident* textures (vs. CPU-copied BGRA IOSurface) — the per-frame copy stays; any content the host repaints into a BGRA IOSurface works.
- New `view{}` attributes (z-order override, opacity, fit/scale mode) — not needed for this iteration.

## Files touched (anticipated)

- `crates/carapace-ffi/src/render_thread.rs` — `content: HashMap`, per-frame loop, `SetContent` command, remove `"host"` literals.
- `crates/carapace-ffi/src/handle.rs` — `carapace_set_content_surface` export + create/swap seeding into the map.
- `crates/carapace-ffi/src/queue.rs` — `Command::SetContent` variant.
- `crates/carapace-ffi/src/render.rs` — `render_frame` map feed; generalize `build_content` to `(surface, w, h)`.
- `crates/carapace-ffi/src/guard.rs` (or wherever ABI consts live) + `lib.rs` — ABI 3.3.
- `showcase/Sources/CCarapace/include/carapace.h` — mirror export + ABI.
- `showcase/Sources/Showcase/CarapaceBridge.swift` — `setContentSurface` wrapper.
- `showcase/Sources/Showcase/App.swift` — attach/clear on Studio enter/leave; second dither region.
- `showcase/skins/studio/skin.lua` — second `view{ id="viz" }`.
- `docs/api/*` — document `carapace_set_content_surface` + the content-registry model.

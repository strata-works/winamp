# Live Host-View Region — Content Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace carapace-ffi's single hardcoded `"host"` content surface with a view-id-keyed content registry and a runtime `carapace_set_content_surface` FFI, so a skin can host multiple named `view{}` cutouts whose live content the host can attach/replace/clear at any time.

**Architecture:** The render thread holds `content: HashMap<String, ContentTex>` instead of `Option<ContentTex>`. A new blocking FFI export routes a `SetContent` command to the render thread to insert/replace/remove an entry. `render_frame` feeds the whole map to the renderer's already-N-capable composite loop. Create/swap keep seeding the `"host"` key (backward-compatible); the registry persists across all swaps.

**Tech Stack:** Rust (carapace-ffi cdylib, wgpu), cbindgen (C header), Swift/AppKit (showcase host), Zig-free. macOS/iOS only (`#[cfg(any(target_os = "macos", target_os = "ios"))]`).

**Spec:** `docs/superpowers/specs/2026-07-08-host-view-content-registry-design.md`

## Global Constraints

- **Platform gate:** all new engine/FFI content code stays behind `#[cfg(any(target_os = "macos", target_os = "ios"))]` (the whole content/render path already is). No cross-platform impl in this plan.
- **ABI:** additive change only → bump `CARAPACE_ABI_MINOR` `2 → 3` (major stays `3`). No field reorder in `CarapaceCreateDesc`.
- **Content handle type:** the new export takes an **opaque `const void*` surface** (not `IOSurfaceRef`), matching `carapace_swap_skin_resized`'s style — cross-platform-ready.
- **Blocking contract:** `carapace_set_content_surface` returns only after the render thread has applied the change (attach/replace/clear), mirroring `carapace_swap_skin_resized`, so the host may free a replaced/cleared surface immediately.
- **frame_ready rule (unchanged):** callbacks fired on the render thread must not call any `carapace_*`.
- **Pixel format:** content surfaces are BGRA `IOSurface`, CPU-copied to a normal wgpu texture each frame (no IOSurface aliasing — the frozen-first-frame bug).
- **Local gate before every push:** `cargo fmt --all`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo clippy --locked -p carapace-ffi --all-targets --features gpu-tests -- -D warnings`, `cargo test` (+ `--features gpu-tests` on macOS). GPU tests need a real adapter (this Mac has one).
- **Git identity:** Daniel Agbemava <danagbemava@gmail.com>. Branch `host-view-content-registry`; never commit to `main`. No Claude attribution in commit/PR bodies.
- **cbindgen header:** `crates/carapace-ffi/include/carapace.h` is generated. Regenerate with `cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact`; the `header` test fails if the committed header is stale.

---

### Task 1: View-id-keyed content registry (data model + render feed)

Replace the single `Option<ContentTex>` with a `HashMap<String, ContentTex>`, feed the whole map to `render_frame`, and keep the `"host"` seed working. This is the load-bearing engine change; the existing `"host"` content path (Studio dither) must keep working unchanged.

**Files:**
- Modify: `crates/carapace-ffi/src/render.rs` (`render_frame` signature + `view_tex` closure, ~`93-114`)
- Modify: `crates/carapace-ffi/src/render_thread.rs` (`content` field `~138`; `build`'s seed `~209`; `render_one` per-frame upload `~265-275`; four `host_view` feed sites `381,421,461,485`)
- Test: `crates/carapace-ffi/src/render_thread.rs` (existing `#[cfg(all(test, target_os = "macos"))] mod render_tests`)

**Interfaces:**
- Produces: `render_frame(engine, renderer, gpu, view, w, h, dt, wait, content: &std::collections::HashMap<String, crate::handle::ContentTex>) -> Scene` — the `host_view` param becomes `content: &HashMap<String, ContentTex>`; the renderer's `view_tex` closure resolves an id via `content.get(id).map(|c| &c.view)`.
- Produces: `RenderThread.content: std::collections::HashMap<String, crate::handle::ContentTex>` (was `Option<ContentTex>`).

- [ ] **Step 1: Write the failing test** (existing `"host"` content still composites through the map)

Add to `mod render_tests` in `crates/carapace-ffi/src/render_thread.rs`:

```rust
#[test]
fn host_content_still_composites_through_registry() {
    // A skin that declares view{ id="host" } + a non-blank content surface must still
    // show that content after the registry refactor. Uses the studio-style host cutout skin.
    let (w, h) = (480u32, 320u32);
    let vt = crate::host::CarapaceHostVTable {
        ctx: std::ptr::null_mut(), get_num: None, get_str: None, invoke: None,
        frame_ready: None, row_count: None, get_row_str: None, get_row_num: None, invoke_arg: None,
    };
    // Fill a content surface with a solid non-zero color the skin's view{host} will sample.
    let content = crate::handle::test_support::make_bgra_iosurface(64, 64);
    unsafe { crate::handle::test_support::fill_iosurface(content, 64, 64, [10, 200, 30, 255]) };
    let dir = std::ffi::CString::new(concat!(
        env!("CARGO_MANIFEST_DIR"), "/tests/skins/hostview"
    )).unwrap();
    let (handle, surfaces) = crate::handle::test_support::create_test_handle_with_content(
        w, h, 2, vt, content, &dir,
    );
    assert_eq!(unsafe { crate::handle::carapace_set_frame_rate(handle, 0) }, crate::guard::CarapaceStatus::Ok);
    assert_eq!(unsafe { crate::handle::carapace_invalidate(handle) }, crate::guard::CarapaceStatus::Ok);
    crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || unsafe {
        crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h)
    });
    assert!(unsafe { crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h) },
        "host content must still composite via the registry");
    unsafe { crate::handle::carapace_destroy(handle) };
}
```

This test needs three test-support helpers and a fixture skin (Step 2). Note: the existing `render_tests` already use `create_test_handle_pool_vt`, `iosurface_has_nonzero_pixels`, `wait_for`, `make_bgra_iosurface` — reuse those patterns.

- [ ] **Step 2: Add the test fixtures**

Create fixture skin `crates/carapace-ffi/tests/skins/hostview/skin.toml`:

```toml
schema = 1
id = "hostview"
name = "Host View"
engine = "^0.1"
canvas = { width = 480, height = 320 }
entry = "skin.lua"
```

Create `crates/carapace-ffi/tests/skins/hostview/skin.lua` (fills the frame, then a host cutout):

```lua
return {
  fill{ path = {{x=0,y=0},{x=480,y=0},{x=480,y=320},{x=0,y=320}}, color = {r=20,g=20,b=20} },
  view{ id = "host", x = 40, y = 40, w = 400, h = 240 },
}
```

Add to `crates/carapace-ffi/src/handle.rs` `mod test_support` (find the existing block; it already has `make_bgra_iosurface`, `iosurface_has_nonzero_pixels`, `wait_for`, `create_test_handle_pool_vt`):

```rust
/// Fill a BGRA IOSurface with a solid color (bytes B,G,R,A per pixel).
pub unsafe fn fill_iosurface(s: IOSurfaceRef, w: u32, h: u32, rgba: [u8; 4]) {
    IOSurfaceLock(s, 0, std::ptr::null_mut());
    let base = IOSurfaceGetBaseAddress(s) as *mut u8;
    let stride = IOSurfaceGetBytesPerRow(s);
    let bgra = [rgba[2], rgba[1], rgba[0], rgba[3]];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let p = base.add(y * stride + x * 4);
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), p, 4);
        }
    }
    IOSurfaceUnlock(s, 0, std::ptr::null_mut());
}

/// Create a live handle whose pool is `count` `w`×`h` BGRA surfaces, seeded with a `content`
/// surface (the `"host"` cutout) and the given skin `dir`. Returns (handle, pool surfaces).
pub fn create_test_handle_with_content(
    w: u32, h: u32, count: usize, vt: crate::host::CarapaceHostVTable,
    content: IOSurfaceRef, dir: &std::ffi::CStr,
) -> (*mut crate::handle::CarapaceEngine, Vec<IOSurfaceRef>) {
    // Mirror create_test_handle_pool_vt but pass content_surface + a real skin dir.
    // (Copy that helper's body; set desc.content_surface = content and desc.skin_dir = dir.as_ptr().)
    unimplemented!("copy create_test_handle_pool_vt, set content_surface + skin_dir")
}
```

(Read the existing `create_test_handle_pool_vt` in `handle.rs` `mod test_support` and copy its body for `create_test_handle_with_content`, setting `desc.content_surface = content` and `desc.skin_dir = dir.as_ptr()`. Ensure `IOSurfaceLock/Unlock/GetBaseAddress/GetBytesPerRow` are imported in `test_support`.)

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p carapace-ffi --features gpu-tests host_content_still_composites_through_registry -- --exact`
Expected: FAIL to COMPILE (the registry types don't exist yet) — that is the red state for this refactor.

- [ ] **Step 4: Change `render_frame` to take the content map**

In `crates/carapace-ffi/src/render.rs`, change the signature and closure:

```rust
pub fn render_frame(
    engine: &mut Engine,
    renderer: &mut Renderer,
    gpu: &GpuCtx,
    view: &wgpu::TextureView,
    w: u32,
    h: u32,
    dt: Duration,
    wait: bool,
    content: &std::collections::HashMap<String, crate::handle::ContentTex>,
) -> Scene {
    engine.update(dt);
    let (cw, ch) = engine.scene().canvas;
    let scene = engine.layout(cw as f32, ch as f32);
    // Resolve each view{} id against the content registry; the renderer's draw() loop is already
    // N-capable, so every matching (id, texture) composites. Unmatched ids render nothing.
    let view_tex = |id: &str| content.get(id).map(|c| &c.view);
    renderer.draw(
        &scene,
        |k| engine.state(k),
        view_tex,
        // ... unchanged RenderTarget { ... } ...
    );
    if wait { let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely()); }
    scene
}
```

Update the `host_view` doc comment above `render_frame` to describe the map.

- [ ] **Step 5: Change `RenderThread.content` to a map + seed + upload loop**

In `crates/carapace-ffi/src/render_thread.rs`:

- Add `use std::collections::HashMap;` at the top.
- Field (`~138`): `content: HashMap<String, ContentTex>,`
- In `build` (`~209`), replace `let content = build_content(&gpu, content_surface);` with:

```rust
let mut content: HashMap<String, ContentTex> = HashMap::new();
if let Some(tex) = build_content(&gpu, content_surface) {
    content.insert("host".to_string(), tex);
}
```

- In `render_one`'s per-frame upload (`~265-275`), replace the single `if let Some(c) = self.content.as_ref()` block with a loop:

```rust
for c in self.content.values() {
    unsafe {
        crate::render::upload_iosurface_to_texture(&self.gpu.queue, c.surface, &c.tex, c.w, c.h)
    };
}
```

- At the four `host_view` feed sites (`381,421,461,485`), replace `let host_view = content.as_ref().map(|c| ("host", &c.view));` with a borrow of the map, and pass `&self.content` (or the destructured `content`) to `render_frame`. Because these sites destructure `self` (`let RenderThread { engine, renderer, gpu, content, .. } = self;`), `content` is now `&HashMap<..>` — pass it directly as the last `render_frame` arg. Remove the `host_view` locals.

- [ ] **Step 6: Preserve `content` in the resized-swap apply (temporary, finalized in Task 3)**

At `SwapSkinResized` apply (`~627-635`), the current code does `let new_content = build_content(...); self.content = new_content;`. Temporarily keep it compiling by seeding a map:

```rust
let mut new_content: HashMap<String, ContentTex> = HashMap::new();
if let Some(tex) = build_content(&self.gpu, pool.content as IOSurfaceRef) {
    new_content.insert("host".to_string(), tex);
}
self.content = new_content;
```

(Task 3 changes this to preserve the existing map instead of replacing it.)

- [ ] **Step 7: Run the test to verify it passes**

Run: `cargo test -p carapace-ffi --features gpu-tests host_content_still_composites_through_registry -- --exact`
Expected: PASS. Then run the whole suite: `cargo test -p carapace-ffi --features gpu-tests` — Expected: all pass (the existing `swap_resized_adopts_new_pool_and_renders` and pacing tests still green).

- [ ] **Step 8: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked -p carapace-ffi --all-targets --features gpu-tests -- -D warnings
git add crates/carapace-ffi/src/render.rs crates/carapace-ffi/src/render_thread.rs \
        crates/carapace-ffi/src/handle.rs crates/carapace-ffi/tests/skins/hostview
git commit -m "refactor(ffi): view-id-keyed content registry (HashMap<String, ContentTex>)"
```

---

### Task 2: `carapace_set_content_surface` — runtime attach/replace/clear

Add a blocking FFI export + `SetContent` command that inserts/replaces/removes a registry entry by `view_id`.

**Files:**
- Modify: `crates/carapace-ffi/src/queue.rs` (`Command` enum + a `SetContent` variant)
- Modify: `crates/carapace-ffi/src/render_thread.rs` (`apply` handles `Command::SetContent`)
- Modify: `crates/carapace-ffi/src/handle.rs` (new `carapace_set_content_surface` export)
- Test: `crates/carapace-ffi/src/render_thread.rs` `mod render_tests`

**Interfaces:**
- Consumes: `RenderThread.content` (Task 1).
- Produces: `Command::SetContent { view_id: String, surface: *const std::ffi::c_void, reply: std::sync::mpsc::Sender<CarapaceStatus> }` (raw pointer wrapped in a `Send` newtype like `SwapSkinResized`'s pool if needed).
- Produces: `carapace_set_content_surface(ptr: *mut CarapaceEngine, view_id: *const c_char, surface: *const c_void, w: u32, h: u32) -> CarapaceStatus`. `surface` null → clear; non-null → attach/replace. `w`/`h` are accepted for API symmetry with create/swap; dims are derived from the surface by `build_content`.

- [ ] **Step 1: Write the failing test** (attach two ids → both composite; clear one → gone)

```rust
#[test]
fn set_content_surface_attaches_replaces_and_clears() {
    let (w, h) = (480u32, 320u32);
    let vt = crate::host::CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None,
        get_str: None, invoke: None, frame_ready: None, row_count: None, get_row_str: None,
        get_row_num: None, invoke_arg: None };
    // Skin with TWO cutouts: id="a" (top) and id="b" (bottom).
    let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/skins/twocutout")).unwrap();
    let (handle, surfaces) = crate::handle::test_support::create_test_handle_with_content(
        w, h, 2, vt, std::ptr::null_mut(), &dir);
    unsafe { let _ = crate::handle::carapace_set_frame_rate(handle, 0); };

    let sa = crate::handle::test_support::make_bgra_iosurface(64, 64);
    unsafe { crate::handle::test_support::fill_iosurface(sa, 64, 64, [200, 0, 0, 255]) };
    let ida = std::ffi::CString::new("a").unwrap();
    assert_eq!(unsafe { crate::handle::carapace_set_content_surface(
        handle, ida.as_ptr(), sa as *const std::ffi::c_void, 64, 64) }, crate::guard::CarapaceStatus::Ok);

    unsafe { let _ = crate::handle::carapace_invalidate(handle); };
    crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || unsafe {
        crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h) });
    assert!(unsafe { crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h) });

    // Clear id="a": returns Ok even though the surface stays valid; blocking guarantees the
    // render thread dropped its ContentTex before we could free `sa`.
    assert_eq!(unsafe { crate::handle::carapace_set_content_surface(
        handle, ida.as_ptr(), std::ptr::null(), 0, 0) }, crate::guard::CarapaceStatus::Ok);

    // Attach for an id the skin does NOT declare → Ok, no crash, nothing rendered there.
    let idz = std::ffi::CString::new("nope").unwrap();
    assert_eq!(unsafe { crate::handle::carapace_set_content_surface(
        handle, idz.as_ptr(), sa as *const std::ffi::c_void, 64, 64) }, crate::guard::CarapaceStatus::Ok);

    unsafe { crate::handle::carapace_destroy(handle) };
}
```

Add fixture `crates/carapace-ffi/tests/skins/twocutout/skin.toml` (schema like `hostview`, id `twocutout`) and `skin.lua`:

```lua
return {
  fill{ path = {{x=0,y=0},{x=480,y=0},{x=480,y=320},{x=0,y=320}}, color = {r=20,g=20,b=20} },
  view{ id = "a", x = 20, y = 20,  w = 440, h = 130 },
  view{ id = "b", x = 20, y = 170, w = 440, h = 130 },
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace-ffi --features gpu-tests set_content_surface_attaches_replaces_and_clears -- --exact`
Expected: FAIL to compile (`carapace_set_content_surface` undefined).

- [ ] **Step 3: Add the `SetContent` command**

In `crates/carapace-ffi/src/queue.rs`, add to `enum Command`:

```rust
/// Attach/replace (`surface` non-null) or clear (`surface` null) the content for `view_id`.
/// Blocking via `reply` (mirrors SwapSkinResized) so the host can free a replaced/cleared surface.
SetContent {
    view_id: String,
    surface: *const c_void,
    reply: std::sync::mpsc::Sender<CarapaceStatus>,
},
```

(`Command` already holds a raw `*const c_void` in `SendPool`/other variants, so `Send` handling follows the existing pattern — if `Command` has an `unsafe impl Send`, this variant is covered; otherwise mirror `SwapSkinResized`.)

- [ ] **Step 4: Handle it in `apply`**

In `crates/carapace-ffi/src/render_thread.rs` `fn apply`, add an arm (model on the `SwapSkinResized` arm's reply pattern):

```rust
Command::SetContent { view_id, surface, reply } => {
    let status = if surface.is_null() {
        self.content.remove(&view_id);
        CarapaceStatus::Ok
    } else if let Some(tex) = build_content(&self.gpu, surface as IOSurfaceRef) {
        self.content.insert(view_id, tex); // replaces any prior entry (its ContentTex drops here)
        CarapaceStatus::Ok
    } else {
        set_last_error("set_content_surface: null/zero-dim/failed content surface");
        CarapaceStatus::ErrBadSkin
    };
    let _ = reply.send(status);
    *invalidated = true; // show the change on the next frame
}
```

- [ ] **Step 5: Add the export**

In `crates/carapace-ffi/src/handle.rs`, model on `carapace_swap_skin_resized`'s blocking send-then-recv pattern:

```rust
/// Attach/replace (`surface` non-null) or clear (`surface` null) the live content for the skin's
/// `view{ id = view_id }` cutout. Blocks until applied; then the caller may free a replaced/cleared
/// surface. `w`/`h` are accepted for symmetry with create/swap (dims are derived from the surface).
///
/// # Safety
/// `ptr` from `carapace_create`, not destroyed. `view_id` a valid NUL-terminated UTF-8 string.
/// `surface` null or a live BGRA IOSurface outliving this call.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn carapace_set_content_surface(
    ptr: *mut CarapaceEngine, view_id: *const c_char,
    surface: *const c_void, _w: u32, _h: u32,
) -> CarapaceStatus {
    // guard poison/null exactly like carapace_swap_skin_resized (copy that export's preamble).
    let engine = /* deref+guard as in swap_skin_resized */;
    let view_id = match unsafe { std::ffi::CStr::from_ptr(view_id) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return CarapaceStatus::ErrBadSkin,
    };
    let (reply, rx) = std::sync::mpsc::channel();
    if engine.tx.send(Command::SetContent { view_id, surface, reply }).is_err() {
        return CarapaceStatus::ErrPoisoned;
    }
    rx.recv().unwrap_or(CarapaceStatus::ErrPoisoned)
}
```

(Read `carapace_swap_skin_resized` in `handle.rs` and copy its exact null/poison guard preamble and `engine.tx` access — names must match.)

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p carapace-ffi --features gpu-tests set_content_surface_attaches_replaces_and_clears -- --exact`
Expected: PASS.

- [ ] **Step 7: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked -p carapace-ffi --all-targets --features gpu-tests -- -D warnings
git add crates/carapace-ffi/src/queue.rs crates/carapace-ffi/src/render_thread.rs \
        crates/carapace-ffi/src/handle.rs crates/carapace-ffi/tests/skins/twocutout
git commit -m "feat(ffi): carapace_set_content_surface — runtime attach/replace/clear content"
```

---

### Task 3: Persist the registry across a resized swap (A1)

A resized swap keeps the same GPU device — the registry's textures stay valid — so preserve the map and only re-seed `"host"` from the passed surface, rather than rebuilding.

**Files:**
- Modify: `crates/carapace-ffi/src/render_thread.rs` (`SwapSkinResized` apply, the Task-1 Step-6 block)
- Test: `crates/carapace-ffi/src/render_thread.rs` `mod render_tests`

**Interfaces:** Consumes `RenderThread.content` (Task 1), `carapace_set_content_surface` (Task 2), `carapace_swap_skin_resized` (existing).

- [ ] **Step 1: Write the failing test** (a non-host entry survives a resized swap)

```rust
#[test]
fn content_registry_survives_resized_swap() {
    let (w, h) = (480u32, 320u32);
    let vt = crate::host::CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: None,
        get_str: None, invoke: None, frame_ready: None, row_count: None, get_row_str: None,
        get_row_num: None, invoke_arg: None };
    let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/skins/twocutout")).unwrap();
    let (handle, _surfaces) = crate::handle::test_support::create_test_handle_with_content(
        w, h, 2, vt, std::ptr::null_mut(), &dir);
    unsafe { let _ = crate::handle::carapace_set_frame_rate(handle, 0); };

    let sb = crate::handle::test_support::make_bgra_iosurface(64, 64);
    unsafe { crate::handle::test_support::fill_iosurface(sb, 64, 64, [0, 0, 200, 255]) };
    let idb = std::ffi::CString::new("b").unwrap();
    unsafe { let _ = crate::handle::carapace_set_content_surface(handle, idb.as_ptr(),
        sb as *const std::ffi::c_void, 64, 64); };

    // Resized swap to the SAME two-cutout skin at a new pixel size; a new pool.
    let (w2, h2) = (600u32, 400u32);
    let new_surfaces: Vec<crate::render::IOSurfaceRef> = (0..2)
        .map(|_| crate::handle::test_support::make_bgra_iosurface(w2 as usize, h2 as usize)).collect();
    let refs: Vec<*const std::ffi::c_void> = new_surfaces.iter().map(|&s| s as *const std::ffi::c_void).collect();
    assert_eq!(unsafe { crate::handle::carapace_swap_skin_resized(handle, dir.as_ptr(),
        refs.as_ptr(), refs.len() as u32, w2, h2, std::ptr::null()) }, crate::guard::CarapaceStatus::Ok);

    // The "b" content persisted: cutout b must still be non-blank after the swap.
    unsafe { let _ = crate::handle::carapace_invalidate(handle); };
    crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || {
        for i in 0..2 { unsafe { let _ = crate::handle::carapace_release_surface(handle, i); } }
        unsafe { crate::handle::test_support::iosurface_has_nonzero_pixels(new_surfaces[0], w2, h2)
            || crate::handle::test_support::iosurface_has_nonzero_pixels(new_surfaces[1], w2, h2) }
    });
    assert!(unsafe { crate::handle::test_support::iosurface_has_nonzero_pixels(new_surfaces[0], w2, h2)
        || crate::handle::test_support::iosurface_has_nonzero_pixels(new_surfaces[1], w2, h2) },
        "content set before a resized swap must survive it (A1)");
    unsafe { crate::handle::carapace_destroy(handle) };
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p carapace-ffi --features gpu-tests content_registry_survives_resized_swap -- --exact`
Expected: FAIL — the current apply rebuilds `self.content` from scratch (only `"host"`), dropping `"b"`.

- [ ] **Step 3: Preserve the map in the resized-swap apply**

Replace the Task-1 Step-6 block in the `SwapSkinResized` apply with:

```rust
// A1: preserve the content registry across a resized swap. The GPU device is unchanged, so the
// existing ContentTex textures stay valid. Only re-seed the "host" key from the passed surface.
if let Some(tex) = build_content(&self.gpu, pool.content as IOSurfaceRef) {
    self.content.insert("host".to_string(), tex);
} // null pool.content → keep whatever "host" entry already exists
```

(Do NOT reassign `self.content` — leave the map intact.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p carapace-ffi --features gpu-tests content_registry_survives_resized_swap -- --exact`
Expected: PASS. Then `cargo test -p carapace-ffi --features gpu-tests` — all green.

- [ ] **Step 5: fmt + clippy + commit**

```bash
cargo fmt --all
cargo clippy --locked -p carapace-ffi --all-targets --features gpu-tests -- -D warnings
git add crates/carapace-ffi/src/render_thread.rs
git commit -m "feat(ffi): content registry persists across resized swaps (A1)"
```

---

### Task 4: ABI bump 3.2 → 3.3 + regenerate header

**Files:**
- Modify: `crates/carapace-ffi/src/guard.rs:42` (`CARAPACE_ABI_MINOR`)
- Modify: `crates/carapace-ffi/src/lib.rs:46` (ABI assertion test)
- Modify (generated): `crates/carapace-ffi/include/carapace.h`
- Test: `crates/carapace-ffi/tests/header.rs` (existing `header` test)

**Interfaces:** none new.

- [ ] **Step 1: Bump the constant**

`crates/carapace-ffi/src/guard.rs:42`: `pub const CARAPACE_ABI_MINOR: u32 = 3;`

- [ ] **Step 2: Update the assertion test**

`crates/carapace-ffi/src/lib.rs:46`: `assert_eq!(CARAPACE_ABI_MINOR, 3);`

- [ ] **Step 3: Run the header test to verify it fails (stale header)**

Run: `cargo test -p carapace-ffi --test header`
Expected: FAIL — committed `carapace.h` lacks `carapace_set_content_surface` and shows MINOR 2.

- [ ] **Step 4: Regenerate the header**

Run: `cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact`
Then inspect: `git diff crates/carapace-ffi/include/carapace.h` — Expected: `#define CARAPACE_ABI_MINOR 3` and a new `carapace_set_content_surface(...)` declaration with `const void *surface`.

- [ ] **Step 5: Run the header test to verify it passes**

Run: `cargo test -p carapace-ffi --test header`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/carapace-ffi/src/guard.rs crates/carapace-ffi/src/lib.rs crates/carapace-ffi/include/carapace.h
git commit -m "chore(ffi): bump ABI to 3.3 (carapace_set_content_surface)"
```

---

### Task 5: Showcase — set/clear API + second Studio cutout

Rework the Swift showcase to drive content through `carapace_set_content_surface` (set on entering Studio, blocking-clear on leaving) and add a second `view{}` cutout to prove multi-region.

**Files:**
- Modify: `showcase/Sources/CCarapace/include/carapace.h` (mirror the new export + ABI — copy the regenerated Rust header's additions)
- Modify: `showcase/Sources/Showcase/CarapaceBridge.swift` (add `setContentSurface`)
- Modify: `showcase/Sources/Showcase/App.swift` (attach on enter-Studio, clear on leave; second dither)
- Modify: `showcase/skins/studio/skin.lua` (add `view{ id="viz" }`)
- Test: `showcase` builds + `swift test`; manual run

**Interfaces:** Consumes `carapace_set_content_surface` (Task 2). The showcase already builds against `libcarapace_ffi.dylib` and has `DitherRenderer` producing an `IOSurface`.

- [ ] **Step 1: Mirror the header**

Copy the two additions from the regenerated `crates/carapace-ffi/include/carapace.h` into `showcase/Sources/CCarapace/include/carapace.h`: `#define CARAPACE_ABI_MINOR 3` and the `carapace_set_content_surface(...)` declaration. (This header is the hand-mirrored copy the Swift package imports.)

- [ ] **Step 2: Add `setContentSurface` to the bridge**

In `showcase/Sources/Showcase/CarapaceBridge.swift`, add (model on the existing `swapResized` wrapper's `withCString`/`Unmanaged` handling):

```swift
/// Attach/replace (surface != nil) or clear (surface == nil) the content for `viewId`.
/// Blocking; on return a replaced/cleared surface is safe to free.
@discardableResult
func setContentSurface(_ viewId: String, _ surface: IOSurface?, _ w: Int, _ h: Int) -> Bool {
    guard let e = engine else { return false }
    let ptr = surface.map { UnsafeRawPointer(Unmanaged.passUnretained($0 as IOSurfaceRef).toOpaque()) }
    return viewId.withCString { vid in
        carapace_set_content_surface(e, vid, ptr, UInt32(w), UInt32(h)) == Ok
    }
}
```

- [ ] **Step 3: Drive content via the API in App.swift**

In `showcase/Sources/Showcase/App.swift`, in `ditherSurface(forDir:)`/`stopDither()` (the Studio-only dither path), replace the "pass dither as create/swap content" reliance:
- On entering Studio: after the bridge exists, `bridge.setContentSurface("host", ditherSurface, w, h)`.
- On leaving Studio (before `stopDither()` tears down the `DitherRenderer`): `_ = bridge.setContentSurface("host", nil, 0, 0)` — the blocking clear guarantees the render thread dropped its `ContentTex` before the surface unmaps (the UAF discipline).

(Read the current `ditherSurface`/`applySkin`/`swapSkin` to place these calls; keep passing the create/swap seed for backward-compat but no longer depend on it for the dither lifecycle.)

- [ ] **Step 4: Add the second Studio cutout + a second dither**

- `showcase/skins/studio/skin.lua`: add a second cutout, e.g. `view{ id = "viz", x = ..., y = ..., w = ..., h = ... }` in a free area of the Studio chrome (pick coords that don't overlap `id="host"`; read the existing skin.lua for layout).
- In `App.swift`, create a second `DitherRenderer` (different Bayer phase/color) and `bridge.setContentSurface("viz", vizSurface, vw, vh)` on entering Studio; clear it on leave.

- [ ] **Step 5: Build + test**

Run:
```bash
cargo build -p carapace-ffi
cd showcase && swift build && swift test
```
Expected: build clean, tests pass. Manual: `swift run Showcase`, cycle to Studio — both cutouts animate; cycling away clears cleanly (no crash, no frozen frame).

- [ ] **Step 6: Commit**

```bash
git add showcase/Sources/CCarapace/include/carapace.h showcase/Sources/Showcase/CarapaceBridge.swift \
        showcase/Sources/Showcase/App.swift showcase/skins/studio/skin.lua
git commit -m "feat(showcase): drive host-view content via set/clear API + second Studio cutout"
```

---

### Task 6: Docs — content registry + `carapace_set_content_surface`

**Files:**
- Modify: `docs/api/skin-authoring.md` (or the `view{}` reference) — document that a skin may declare multiple `view{}` cutouts, each filled by a named content surface.
- Modify: the carapace-ffi API reference (find where `carapace_swap_skin_resized` is documented in `docs/api/`) — add `carapace_set_content_surface`, the registry model, A1 persistence, and the blocking/clear contract.

**Interfaces:** none.

- [ ] **Step 1: Document the registry + export**

Add a section describing: the view-id-keyed registry; `carapace_set_content_surface(engine, view_id, surface, w, h)` (null = clear; blocking; opaque `const void*`); create/swap seed `"host"`; content persists across swaps (A1); multiple named cutouts.

- [ ] **Step 2: Commit**

```bash
git add docs/api
git commit -m "docs(api): document the host-view content registry + carapace_set_content_surface"
```

---

### Task 7: Full gate + push + PR

- [ ] **Step 1: Full local gate**

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p carapace-ffi --all-targets --features gpu-tests -- -D warnings
cargo test --workspace
cargo test -p carapace-ffi --features gpu-tests
(cd showcase && swift build && swift test)
```
Expected: all green.

- [ ] **Step 2: Push + draft PR**

```bash
git push -u origin host-view-content-registry
gh pr create --draft --base main --head host-view-content-registry \
  --title "feat: live host-view region — content registry (carapace_set_content_surface)" \
  --body "Implements docs/superpowers/specs/2026-07-08-host-view-content-registry-design.md. ABI 3.2->3.3."
```
(If `gh pr edit`/`create` hits the Projects-classic GraphQL bug, use `gh api -X POST repos/<owner>/<repo>/pulls ...`.)

---

## Self-Review

**Spec coverage:**
- Registry data model (`HashMap<String, ContentTex>`) → Task 1. ✓
- `render_frame` map feed → Task 1. ✓
- `carapace_set_content_surface` blocking attach/replace/clear + `SetContent` command → Task 2. ✓
- Opaque `const void*` handle → Task 2 (signature) + Task 4 (header). ✓
- A1 persist across all swaps (incl. resized) → Task 3. ✓
- B1 keep create/swap seed of `"host"` → Task 1 (build seed) + Task 3 (resized reseed). ✓
- ABI 3.2→3.3 + header regen → Task 4. ✓
- Showcase set/clear rework + second cutout → Task 5. ✓
- GPU test matrix (attach two, clear one, unknown id, resized-swap survival, host still works) → Tasks 1–3. ✓
- Docs → Task 6. ✓
- Cross-platform seam (opaque handle) → Task 2/4; impl out of scope per Global Constraints. ✓

**Placeholder scan:** `create_test_handle_with_content` (Task 1 Step 2) and the export preamble (Task 2 Step 5) instruct the implementer to copy an existing helper/export body verbatim — because those bodies are long and must match current names exactly; this is a "copy the reference" directive, not a vague placeholder. All behavioral code is shown.

**Type consistency:** `content: HashMap<String, ContentTex>` used identically in Tasks 1–3; `carapace_set_content_surface(ptr, view_id, surface, w, h)` signature identical in Tasks 2, 4, 5; `Command::SetContent { view_id, surface, reply }` identical in Tasks 2 (queue) and its `apply` arm.

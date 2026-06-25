# Host-Embedding Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove a native macOS Swift app can embed the carapace engine across a C ABI, act as its full Host (Swift owns the state + action), and display its live render via a zero-copy IOSurface — with a CPU-readback fallback so the FFI/host loop is proven either way.

**Architecture:** A throwaway `cdylib` crate `crates/embed-spike` wraps the *unchanged* `carapace` engine: it owns a headless wgpu device + the `Renderer`, builds an `Engine` whose `Host` is an `FfiHost` that forwards `get`/`actions`/`invoke` to C function pointers the Swift app registers, and renders each tick into a caller-supplied `IOSurface`. A minimal AppKit sample app owns the window/`NSView`/`CALayer`, creates the IOSurface, drives the tick via `CVDisplayLink`, forwards mouse clicks, and serves one live native value (battery %) plus one toggle action.

**Tech Stack:** Rust, wgpu 29.0.3 (Metal), vello 0.9.0 (via carapace), the engine's existing public API (`Engine`, `Renderer`, `Host`), `pollster` (block on async wgpu init), `core-foundation` + `io-surface` (IOSurface from Rust), `metal` + `wgpu-hal` (Tier 2 texture import). Swift / AppKit / CoreVideo / Metal for the sample app.

## Global Constraints

- **Platform:** macOS / Metal only. No Windows/Linux path.
- **Engine untouched:** `crates/carapace/src/` must have ZERO diffs. If one *domain-neutral* engine change proves unavoidable, stop and surface it for approval (do not slip it in). Verified at the end with `git diff --stat crates/carapace/src/`.
- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>` (use `git -c user.name=… -c user.email=…` if the repo identity differs).
- **No Claude attribution** in any commit message or PR body.
- **New third-party deps:** the FIRST fetch of any new crate must run through Socket Firewall — `sfw cargo add <crate>` (not plain `cargo add`). Applies to `pollster`, `core-foundation`, `io-surface`, `metal`.
- **Clippy gate:** `cargo clippy -p embed-spike --all-targets -- -D warnings` must pass before any push (CI gates on `-D warnings`).
- **Throwaway:** this is a spike. Optimize for a fast, honest verdict; do not build a stable/versioned ABI or harden memory safety beyond what the demo needs to run.
- **Branch:** all work lands on `host-embedding-spike` (already created and checked out; the design doc is committed there).

---

## File Structure

- `Cargo.toml` (workspace root) — **Modify:** add `crates/embed-spike` to `members`.
- `crates/embed-spike/Cargo.toml` — **Create:** `cdylib` + the deps above.
- `crates/embed-spike/carapace.h` — **Create:** hand-written C header (the ABI Swift links against).
- `crates/embed-spike/src/lib.rs` — **Create:** the C ABI functions + the `CarapaceEngine` struct that owns wgpu/Renderer/Engine/target.
- `crates/embed-spike/src/host.rs` — **Create:** `CarapaceHostVTable` (repr C) + `FfiHost` (`Host` impl).
- `crates/embed-spike/src/render.rs` — **Create:** the wgpu device init + the `Present` seam (Tier 1 readback / Tier 2 shared) + per-frame draw.
- `crates/embed-spike/skin/` — **Create:** `skin.toml` + `main.lua` (the purpose-built spike skin binding `level` + action `toggle`).
- `crates/embed-spike/examples/render_png.rs` — **Create:** in-process proof (Task 3): fake vtable → tick → PNG.
- `crates/embed-spike/examples/iosurface_png.rs` — **Create:** Tier-1 proof (Task 4): render into an IOSurface from Rust → PNG.
- `crates/embed-spike/macos-sample/` — **Create:** Swift Package (AppKit app) + `README.md` (build/run).
- `crates/embed-spike/screenshot.png` — **Create:** headline result (Task 5/6).
- `docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md` — **Create:** the verdict (Task 7).

---

### Task 1: Crate scaffold + the spike skin

**Files:**
- Modify: `Cargo.toml` (workspace `members`)
- Create: `crates/embed-spike/Cargo.toml`
- Create: `crates/embed-spike/src/lib.rs` (temporary `lib.rs` with just the skin-load test path)
- Create: `crates/embed-spike/skin/skin.toml`
- Create: `crates/embed-spike/skin/main.lua`
- Test: `crates/embed-spike/tests/skin_loads.rs`

**Interfaces:**
- Consumes: `carapace::skin::load_dir`, `carapace::engine::Engine`, `carapace::vocab::VocabRegistry`, `carapace::fixture::FixtureHost`.
- Produces: a buildable `embed-spike` crate and a skin directory whose path later tasks pass to `carapace_create`. The skin binds state key `"level"` and declares a hotspot firing host action `"toggle"`.

- [ ] **Step 1: Add the crate to the workspace**

In `Cargo.toml` (root), add to `members`:

```toml
members = ["crates/hittest", "crates/carapace", "crates/carapace-demo", "crates/window-spike", "crates/embed-spike"]
```

- [ ] **Step 2: Create `crates/embed-spike/Cargo.toml`**

```toml
[package]
name = "embed-spike"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib", "rlib"]   # cdylib for Swift to link; rlib so tests/examples can use it

[dependencies]
carapace = { path = "../carapace" }

[dev-dependencies]
carapace = { path = "../carapace" }
```

(`rlib` is included so the integration tests and examples in this crate can call the same code Swift will. The other deps are added in later tasks, right before they are first used.)

- [ ] **Step 3: Write the spike skin manifest**

`crates/embed-spike/skin/skin.toml` — mirror an existing skin's manifest shape. Inspect `crates/carapace-demo/skins/minimal/skin.toml` first and copy its keys exactly (canvas size, entry script field name, etc.). Then:

```toml
# Purpose-built spike skin: one bound value + one action. NOT a real product skin.
name = "embed-spike"
canvas = [240, 80]          # match the key name/shape used by minimal/skin.toml
script = "main.lua"         # match minimal/skin.toml's entry-script key name
```

> If `minimal/skin.toml` uses different key names (e.g. `[canvas] width=…`), use ITS names — this block is illustrative.

- [ ] **Step 4: Write the spike skin script**

`crates/embed-spike/skin/main.lua` — inspect `crates/carapace-demo/skins/minimal/` and a skin that uses `text`/`value_fill` (grep the demo skins for `value_fill` and `region`) to copy exact constructor argument names. Target behavior: a dark fill, a value-bound bar/text reading `level`, and a region that fires `toggle`.

```lua
-- Background.
fill{ path = { {0,0}, {240,0}, {240,80}, {0,80} }, color = { r=18, g=20, b=26, a=255 } }

-- A horizontal bar whose fill fraction tracks host state key "level" (0.0..1.0).
value_fill{ path = { {16,16}, {224,16}, {224,40}, {16,40} },
            value = "level", color = { r=120, g=230, b=80 }, direction = "right" }

-- The whole lower strip is a hotspot that invokes the host action "toggle".
region{ path = { {0,48}, {240,48}, {240,80}, {0,80} },
        on_press = function() host.toggle() end }
```

> Use the EXACT constructor + argument names the demo skins use. If `value` is spelled `value_key`, or `color` takes `{r,g,b}` without `a`, match the demo. The shapes above are the intent, not a guarantee of spelling.

- [ ] **Step 5: Write the failing test**

`crates/embed-spike/tests/skin_loads.rs`:

```rust
//! The spike skin builds on the real engine and exposes the binding + action the FFI host needs.
use std::path::Path;

#[test]
fn spike_skin_builds_and_binds_level_and_toggle() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skin");
    let (_manifest, source) = carapace::skin::load_dir(&dir).expect("load spike skin");

    // A host that allows the "toggle" action and returns a value for "level",
    // so the skin builds and the binding resolves.
    let host = carapace::fixture::FixtureHost::new();
    let engine = carapace::engine::Engine::new(
        Box::new(host),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .expect("engine builds the spike skin");

    // The scene has nodes (fill + value_fill + hotspot) — the script ran.
    assert!(!engine.scene().nodes.is_empty(), "skin produced a scene");
}
```

> Check `carapace::fixture::FixtureHost`'s real constructor/API first (`grep -n "impl FixtureHost" crates/carapace/src/fixture.rs`). If it needs registered actions/state, configure it so `toggle`/`level` exist, or use a tiny local stub host implementing `carapace::host::Host`. The assertion that must pass: `Engine::new` returns `Ok` for this skin.

- [ ] **Step 6: Run the test to verify it fails (then passes once the skin is right)**

Run: `cargo test -p embed-spike --test skin_loads`
Expected first run: FAIL (skin spelling mismatch or missing host action). Fix the `.lua`/`.toml`/host until it PASSES. The failure-then-fix here is how you confirm the constructor names are correct against the real engine.

- [ ] **Step 7: Build the cdylib**

Run: `cargo build -p embed-spike`
Expected: builds a `libembed_spike.dylib` under `target/debug/`.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/embed-spike
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(embed): scaffold crate + purpose-built spike skin"
```

---

### Task 2: FfiHost — the Swift-owned Host over a C vtable

**Files:**
- Create: `crates/embed-spike/src/host.rs`
- Modify: `crates/embed-spike/src/lib.rs` (add `mod host;`)
- Test: `crates/embed-spike/src/host.rs` (a `#[cfg(test)] mod tests` with a fake vtable)

**Interfaces:**
- Consumes: `carapace::host::{Host, ActionSpec, Value, Row}`, `carapace::state::StateValue`.
- Produces:
  - `#[repr(C)] pub struct CarapaceHostVTable { ctx: *mut c_void, get_num: Option<extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool>, get_str: Option<extern "C" fn(*mut c_void, *const c_char, *mut c_char, usize) -> bool>, invoke: Option<extern "C" fn(*mut c_void, *const c_char)> }`
  - `pub struct FfiHost { vtable: CarapaceHostVTable }` implementing `carapace::host::Host`, advertising exactly one action: `"toggle"`.
  - `impl FfiHost { pub fn new(vtable: CarapaceHostVTable) -> Self }`

- [ ] **Step 1: Write the vtable + FfiHost skeleton**

`crates/embed-spike/src/host.rs`:

```rust
use std::ffi::{c_char, c_void, CStr, CString};
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::state::StateValue;

/// C function table the Swift app registers. Swift IS the host.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CarapaceHostVTable {
    pub ctx: *mut c_void,
    pub get_num: Option<extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool>,
    pub get_str: Option<extern "C" fn(*mut c_void, *const c_char, *mut c_char, usize) -> bool>,
    pub invoke: Option<extern "C" fn(*mut c_void, *const c_char)>,
}

// The spike runs the engine and the host calls on one thread (the render tick); the raw ctx
// pointer is only ever touched there. Send/Sync are asserted to satisfy the engine's `Box<dyn Host>`.
unsafe impl Send for CarapaceHostVTable {}
unsafe impl Sync for CarapaceHostVTable {}

const ACTIONS: &[ActionSpec] = &[ActionSpec { name: "toggle" }];

pub struct FfiHost {
    vtable: CarapaceHostVTable,
}

impl FfiHost {
    pub fn new(vtable: CarapaceHostVTable) -> Self {
        Self { vtable }
    }
}

impl Host for FfiHost {
    fn name(&self) -> &str {
        "ffi"
    }

    fn tick(&mut self, _dt: Duration) {
        // Swift owns its own clock/state; nothing to advance Rust-side.
    }

    fn get(&self, key: &str) -> Option<StateValue> {
        let ckey = CString::new(key).ok()?;
        // Try numeric first.
        if let Some(get_num) = self.vtable.get_num {
            let mut out = 0.0_f64;
            if get_num(self.vtable.ctx, ckey.as_ptr(), &mut out as *mut f64) {
                return Some(StateValue::Num(out));
            }
        }
        // Then string.
        if let Some(get_str) = self.vtable.get_str {
            let mut buf = vec![0_u8; 256];
            if get_str(
                self.vtable.ctx,
                ckey.as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
            ) {
                let s = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
                    .to_string_lossy()
                    .into_owned();
                return Some(StateValue::Str(s));
            }
        }
        None
    }

    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }

    fn invoke(&mut self, action: &str, _args: &[Value]) {
        if let (Some(invoke), Ok(caction)) = (self.vtable.invoke, CString::new(action)) {
            invoke(self.vtable.ctx, caction.as_ptr());
        }
    }

    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new() // collections out of scope for the spike
    }
}
```

> Verify `StateValue`'s real variant names (`grep -n "enum StateValue" crates/carapace/src/state.rs`) — if it's `StateValue::Number`/`StateValue::Text`, adjust. Verify `ActionSpec` is constructible as shown (it is `pub name: &'static str` per `host.rs`).

- [ ] **Step 2: Register the module**

In `crates/embed-spike/src/lib.rs` add at top: `pub mod host;`

- [ ] **Step 3: Write the failing unit test (fake vtable)**

Append to `crates/embed-spike/src/host.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static INVOKED: AtomicU32 = AtomicU32::new(0);

    extern "C" fn fake_get_num(_ctx: *mut c_void, key: *const c_char, out: *mut f64) -> bool {
        let k = unsafe { CStr::from_ptr(key) }.to_str().unwrap();
        if k == "level" {
            unsafe { *out = 0.42 };
            true
        } else {
            false
        }
    }

    extern "C" fn fake_invoke(_ctx: *mut c_void, action: *const c_char) {
        let a = unsafe { CStr::from_ptr(action) }.to_str().unwrap();
        if a == "toggle" {
            INVOKED.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn vtable() -> CarapaceHostVTable {
        CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: Some(fake_get_num),
            get_str: None,
            invoke: Some(fake_invoke),
        }
    }

    #[test]
    fn get_maps_numeric_state_through_the_vtable() {
        let host = FfiHost::new(vtable());
        assert_eq!(host.get("level"), Some(StateValue::Num(0.42)));
        assert_eq!(host.get("missing"), None);
    }

    #[test]
    fn invoke_routes_to_the_callback_and_action_is_advertised() {
        let mut host = FfiHost::new(vtable());
        assert!(host.actions().iter().any(|a| a.name == "toggle"));
        host.invoke("toggle", &[]);
        assert_eq!(INVOKED.load(Ordering::SeqCst), 1);
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail, then pass**

Run: `cargo test -p embed-spike --lib host`
Expected: compile, then PASS. (If `StateValue::Num` was wrong, the first run fails to compile — fix the variant and re-run to green.)

- [ ] **Step 5: Commit**

```bash
git add crates/embed-spike/src/host.rs crates/embed-spike/src/lib.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(embed): FfiHost — Swift-owned Host over a C vtable"
```

---

### Task 3: Engine assembly + in-process render to PNG

**Files:**
- Create: `crates/embed-spike/src/render.rs`
- Modify: `crates/embed-spike/src/lib.rs` (the C ABI functions + `CarapaceEngine` struct)
- Create: `crates/embed-spike/examples/render_png.rs`
- Add dep: `pollster`

**Interfaces:**
- Consumes: `FfiHost`, `carapace::engine::Engine`, `carapace::render::{Renderer, RenderTarget}`, `carapace::scene::{Color, Pt}`, `carapace::engine::PointerEvent`.
- Produces:
  - `pub struct GpuCtx { device: wgpu::Device, queue: wgpu::Queue }` and `pub fn init_gpu() -> GpuCtx` (headless Metal device).
  - `pub struct OffscreenTarget { tex: wgpu::Texture, view: wgpu::TextureView, w: u32, h: u32 }` + `pub fn new_offscreen(device, w, h)`.
  - `pub fn render_frame(engine: &mut Engine, renderer: &mut Renderer, gpu: &GpuCtx, view: &wgpu::TextureView, w: u32, h: u32, dt: Duration)` — the shared per-frame draw used by every tier.
  - `pub fn readback_rgba(gpu: &GpuCtx, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8>` — RGBA8 rows, tightly packed (bytesPerRow stripped).
  - The opaque `CarapaceEngine` handle + `carapace_create`/`carapace_tick`/`carapace_pointer`/`carapace_destroy` (offscreen-only for now; IOSurface added in Task 4).

- [ ] **Step 1: Add `pollster`**

Run: `sfw cargo add -p embed-spike pollster`
Expected: `pollster` added to `[dependencies]`.

- [ ] **Step 2: Write the GPU + draw helpers**

`crates/embed-spike/src/render.rs`:

```rust
use std::time::Duration;

use carapace::engine::Engine;
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Color;

pub struct GpuCtx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Headless Metal device — no surface, we render into our own textures.
pub fn init_gpu() -> GpuCtx {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("Metal adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("device");
    GpuCtx { device, queue }
}

pub struct OffscreenTarget {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub fn new_offscreen(device: &wgpu::Device, w: u32, h: u32) -> OffscreenTarget {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("embed-spike-offscreen"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    OffscreenTarget { tex, view, w, h }
}

/// The one draw path every tier shares: drain+tick, reflow, draw into `view`.
pub fn render_frame(
    engine: &mut Engine,
    renderer: &mut Renderer,
    gpu: &GpuCtx,
    view: &wgpu::TextureView,
    w: u32,
    h: u32,
    dt: Duration,
) {
    engine.update(dt); // drains queued host actions, ticks host
    let scene = engine.layout(w as f32, h as f32);
    renderer.draw(
        &scene,
        |k| engine.state(k),
        |_| None, // no view{} regions in the spike
        &RenderTarget {
            device: &gpu.device,
            queue: &gpu.queue,
            view,
            width: w,
            height: h,
            // Transparent base so the IOSurface carries the skin's own alpha later.
            base_color: Color { r: 0, g: 0, b: 0, a: 0 },
        },
    );
    // Ensure GPU work is complete before the caller reads back / composites.
    let _ = gpu.device.poll(wgpu::PollType::Wait);
}

/// Copy an RGBA8 texture back to CPU, returning tightly-packed rows (no padding).
pub fn readback_rgba(gpu: &GpuCtx, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let bpp = 4u32;
    let unpadded = w * bpp;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = ((unpadded + align - 1) / align) * align;

    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([enc.finish()]);

    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::Wait);
    let data = slice.get_mapped_range();

    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    buf.unmap();
    out
}
```

> wgpu 29 specifics to confirm against the local version while compiling: `Instance::new` taking `&InstanceDescriptor`; `request_adapter`/`request_device` return types (the demo at `crates/carapace-demo/src/main.rs:399-411` is the source of truth — mirror its exact calls); `device.poll(PollType::Wait)`; `TexelCopyTextureInfo`/`TexelCopyBufferInfo` names. If a name differs, the demo's working calls win — copy them.

- [ ] **Step 3: Write the C ABI + handle in `lib.rs`**

`crates/embed-spike/src/lib.rs` (replace the temporary contents):

```rust
pub mod host;
pub mod render;

use std::ffi::{c_char, CStr};
use std::time::Duration;

use carapace::engine::{Engine, PointerEvent};
use carapace::render::Renderer;
use carapace::scene::Pt;

use crate::host::{CarapaceHostVTable, FfiHost};
use crate::render::{init_gpu, new_offscreen, render_frame, GpuCtx, OffscreenTarget};

/// Opaque handle handed across the C ABI.
pub struct CarapaceEngine {
    gpu: GpuCtx,
    renderer: Renderer,
    engine: Engine,
    target: OffscreenTarget, // Task 4 swaps this for a Present enum (offscreen | iosurface)
    w: u32,
    h: u32,
}

/// # Safety
/// `skin_dir` must be a valid NUL-terminated UTF-8 path. `vtable` function pointers must
/// outlive the returned engine. Returns null on failure.
#[no_mangle]
pub unsafe extern "C" fn carapace_create(
    skin_dir: *const c_char,
    vtable: CarapaceHostVTable,
    w: u32,
    h: u32,
) -> *mut CarapaceEngine {
    let dir = match CStr::from_ptr(skin_dir).to_str() {
        Ok(s) => std::path::PathBuf::from(s),
        Err(_) => return std::ptr::null_mut(),
    };
    let (_m, source) = match carapace::skin::load_dir(&dir) {
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };
    let engine = match Engine::new(
        Box::new(FfiHost::new(vtable)),
        carapace::vocab::VocabRegistry::base(),
        source,
    ) {
        Ok(e) => e,
        Err(_) => return std::ptr::null_mut(),
    };
    let gpu = init_gpu();
    let renderer = Renderer::new(&gpu.device);
    let target = new_offscreen(&gpu.device, w, h);
    Box::into_raw(Box::new(CarapaceEngine { gpu, renderer, engine, target, w, h }))
}

/// Tick + render one frame into the engine's target.
/// # Safety: `ptr` must come from `carapace_create` and not be destroyed.
#[no_mangle]
pub unsafe extern "C" fn carapace_tick(ptr: *mut CarapaceEngine, dt_seconds: f64) {
    let Some(e) = ptr.as_mut() else { return };
    let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
    render_frame(&mut e.engine, &mut e.renderer, &e.gpu, &e.target.view, e.w, e.h, dt);
}

/// Forward a pointer event in canvas coordinates. kind: 0 = press (others ignored in spike).
/// # Safety: `ptr` must come from `carapace_create`.
#[no_mangle]
pub unsafe extern "C" fn carapace_pointer(ptr: *mut CarapaceEngine, x: f64, y: f64, kind: i32) {
    let Some(e) = ptr.as_mut() else { return };
    if kind == 0 {
        e.engine
            .handle_pointer_resolved(e.w as f32, e.h as f32, Pt { x, y }, PointerEvent::Press);
    }
}

/// # Safety: `ptr` must come from `carapace_create`; do not use it afterward.
#[no_mangle]
pub unsafe extern "C" fn carapace_destroy(ptr: *mut CarapaceEngine) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}
```

> Confirm `Pt`'s fields (`grep -n "pub struct Pt" -A4 crates/carapace/src/scene.rs`) — it may be `Pt { x: f64, y: f64 }` or use `f32`; match it. Confirm `PointerEvent::Press` is the variant name (seen used in the demo). Confirm `Engine::new` and `Renderer::new` signatures against `crates/carapace-demo/src/main.rs`.

- [ ] **Step 4: Write the in-process render-to-PNG example (the failing proof)**

Add dev-dep for PNG writing: `sfw cargo add -p embed-spike --dev image`

`crates/embed-spike/examples/render_png.rs`:

```rust
//! In-process proof: a fake host serves level=0.6; the engine ticks once and we dump the frame.
//! Confirms engine + FfiHost + renderer compose without any IOSurface/Swift involved.
use std::ffi::{c_char, c_void, CStr};
use std::time::Duration;

use embed_spike::host::CarapaceHostVTable;
use embed_spike::render::{init_gpu, new_offscreen, readback_rgba, render_frame};

extern "C" fn get_num(_ctx: *mut c_void, key: *const c_char, out: *mut f64) -> bool {
    let k = unsafe { CStr::from_ptr(key) }.to_str().unwrap_or("");
    if k == "level" {
        unsafe { *out = 0.6 };
        true
    } else {
        false
    }
}

fn main() {
    let (w, h) = (240u32, 80u32);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skin");
    let (_m, source) = carapace::skin::load_dir(&dir).unwrap();
    let vtable = CarapaceHostVTable {
        ctx: std::ptr::null_mut(),
        get_num: Some(get_num),
        get_str: None,
        invoke: None,
    };
    let mut engine = carapace::engine::Engine::new(
        Box::new(embed_spike::host::FfiHost::new(vtable)),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .unwrap();

    let gpu = init_gpu();
    let mut renderer = carapace::render::Renderer::new(&gpu.device);
    let target = new_offscreen(&gpu.device, w, h);

    render_frame(&mut engine, &mut renderer, &gpu, &target.view, w, h, Duration::from_millis(16));
    let rgba = readback_rgba(&gpu, &target.tex, w, h);

    // The value bar (green ~120,230,80) must appear somewhere — assert non-empty + has a green-ish pixel.
    let has_green = rgba.chunks_exact(4).any(|p| p[1] > 180 && p[0] < 180 && p[2] < 160 && p[3] > 0);
    assert!(has_green, "expected the value bar to render");

    image::save_buffer("target/render_png.png", &rgba, w, h, image::ColorType::Rgba8).unwrap();
    println!("wrote target/render_png.png");
}
```

- [ ] **Step 5: Run the example — verify it fails then passes**

Run: `cargo run -p embed-spike --example render_png`
Expected: compiles, prints `wrote target/render_png.png`, and the `has_green` assert passes. Open `target/render_png.png` and confirm a dark canvas with a green bar ~60% wide. If it panics on a wgpu API name, fix against the demo's calls and re-run to green.

- [ ] **Step 6: Commit**

```bash
git add crates/embed-spike/Cargo.toml crates/embed-spike/src crates/embed-spike/examples
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(embed): C ABI + headless render; in-process render_png proof"
```

---

### Task 4: Tier 1 — render into a caller-supplied IOSurface (readback)

**Files:**
- Modify: `crates/embed-spike/src/render.rs` (add the `Present` seam + IOSurface readback)
- Modify: `crates/embed-spike/src/lib.rs` (`carapace_create` accepts an `IOSurfaceRef`; add `carapace_active_tier`)
- Create: `crates/embed-spike/carapace.h`
- Create: `crates/embed-spike/examples/iosurface_png.rs`
- Add deps: `core-foundation`, `io-surface`, `libc`

**Interfaces:**
- Consumes: `io_surface::IOSurfaceRef` (raw `*mut __IOSurface` via the `io-surface` crate), the Task-3 helpers.
- Produces:
  - `pub enum Tier { Readback = 1, Shared = 2 }` and `pub enum Present { Readback { off: OffscreenTarget }, Shared { /* Task 5/6 */ } }`.
  - `pub fn copy_into_iosurface(surface: IOSurfaceRef, rgba: &[u8], w: u32, h: u32)` — locks the surface, copies rows honoring its `bytesPerRow`, unlocks.
  - `carapace_create(skin_dir, vtable, surface: IOSurfaceRef, w, h)` (signature gains `surface`).
  - `carapace_active_tier(ptr) -> i32`.

- [ ] **Step 1: Add the IOSurface deps**

Run: `sfw cargo add -p embed-spike core-foundation io-surface libc`
Expected: three crates added. (`io-surface` exposes `IOSurfaceRef` + `IOSurfaceGetBaseAddress`/`Lock`/`Unlock`/`GetBytesPerRow`; `core-foundation` for the types it needs; `libc` for `memcpy`/`c_void`.)

- [ ] **Step 2: Add the IOSurface copy helper to `render.rs`**

Append to `crates/embed-spike/src/render.rs`:

```rust
use io_surface::IOSurfaceRef;

#[repr(i32)]
#[derive(Clone, Copy, PartialEq)]
pub enum Tier {
    Readback = 1,
    Shared = 2,
}

/// Lock a caller-owned IOSurface and copy tightly-packed RGBA rows into it, honoring its stride.
pub fn copy_into_iosurface(surface: IOSurfaceRef, rgba: &[u8], w: u32, h: u32) {
    use io_surface::{
        IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceUnlock,
    };
    unsafe {
        IOSurfaceLock(surface, 0, std::ptr::null_mut());
        let base = IOSurfaceGetBaseAddress(surface) as *mut u8;
        let stride = IOSurfaceGetBytesPerRow(surface) as usize;
        let row_bytes = (w * 4) as usize;
        for y in 0..h as usize {
            let src = &rgba[y * row_bytes..(y + 1) * row_bytes];
            let dst = base.add(y * stride);
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst, row_bytes);
        }
        IOSurfaceUnlock(surface, 0, std::ptr::null_mut());
    }
}
```

> The exact symbol names/signatures depend on the `io-surface` crate version. After `cargo add`, inspect it: `grep -rn "pub fn IOSurface" ~/.cargo/registry/src/*/io-surface-*/src/`. If lock/unlock or base-address accessors differ, match them. The lock options arg is a `u32` bitfield (0 = read/write) and the seed pointer can be null.

- [ ] **Step 3: Switch the handle to a `Present` seam**

In `crates/embed-spike/src/lib.rs`, replace the `target: OffscreenTarget` field and `carapace_create`/`carapace_tick` with the IOSurface-aware version. For Tier 1, we still render into the offscreen texture, then copy into the surface:

```rust
use io_surface::IOSurfaceRef;
use crate::render::{copy_into_iosurface, readback_rgba, Tier};

pub struct CarapaceEngine {
    gpu: GpuCtx,
    renderer: Renderer,
    engine: Engine,
    off: OffscreenTarget,
    surface: IOSurfaceRef,
    tier: Tier,
    w: u32,
    h: u32,
}

/// # Safety: see Task 3. `surface` must be a valid IOSurface of size w×h, BGRA/RGBA, that
/// outlives the engine.
#[no_mangle]
pub unsafe extern "C" fn carapace_create(
    skin_dir: *const c_char,
    vtable: CarapaceHostVTable,
    surface: IOSurfaceRef,
    w: u32,
    h: u32,
) -> *mut CarapaceEngine {
    // ... identical skin/engine/gpu setup as Task 3 ...
    let off = new_offscreen(&gpu.device, w, h);
    let tier = Tier::Readback; // Task 6 will try Tier::Shared first and fall back here
    Box::into_raw(Box::new(CarapaceEngine { gpu, renderer, engine, off, surface, tier, w, h }))
}

#[no_mangle]
pub unsafe extern "C" fn carapace_tick(ptr: *mut CarapaceEngine, dt_seconds: f64) {
    let Some(e) = ptr.as_mut() else { return };
    let dt = Duration::from_secs_f64(dt_seconds.max(0.0));
    render_frame(&mut e.engine, &mut e.renderer, &e.gpu, &e.off.view, e.w, e.h, dt);
    if e.tier == Tier::Readback {
        let rgba = readback_rgba(&e.gpu, &e.off.tex, e.w, e.h);
        copy_into_iosurface(e.surface, &rgba, e.w, e.h);
    }
}

#[no_mangle]
pub unsafe extern "C" fn carapace_active_tier(ptr: *mut CarapaceEngine) -> i32 {
    match ptr.as_ref() {
        Some(e) => e.tier as i32,
        None => 0,
    }
}
```

> Pixel format: IOSurface for `CALayer.contents` display is conventionally BGRA8 (`'BGRA'`). The engine renders RGBA8. For Tier 1 readback either (a) create the surface as RGBA and rely on the display path, or (b) swizzle R↔B in `copy_into_iosurface`. Start with BGRA surface + a swizzle in the copy loop (swap bytes 0 and 2 per pixel); the example in Step 5 makes the right choice visible.

- [ ] **Step 4: Write the C header**

`crates/embed-spike/carapace.h`:

```c
#ifndef CARAPACE_H
#define CARAPACE_H
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <IOSurface/IOSurfaceRef.h>

typedef struct CarapaceEngine CarapaceEngine;

typedef struct {
  void* ctx;
  bool (*get_num)(void* ctx, const char* key, double* out);
  bool (*get_str)(void* ctx, const char* key, char* buf, size_t cap);
  void (*invoke)(void* ctx, const char* action);
} CarapaceHostVTable;

CarapaceEngine* carapace_create(const char* skin_dir, CarapaceHostVTable host,
                                IOSurfaceRef surface, uint32_t w, uint32_t h);
void carapace_tick(CarapaceEngine* e, double dt_seconds);
void carapace_pointer(CarapaceEngine* e, double x, double y, int32_t kind);
int32_t carapace_active_tier(CarapaceEngine* e);  // 1 = readback, 2 = shared
void carapace_destroy(CarapaceEngine* e);
#endif
```

> Keep struct field order/types byte-identical between this header and the Rust `#[repr(C)]` vtable — a mismatch is a silent crash. `IOSurfaceRef` here is Apple's real type; the Rust side's `io_surface::IOSurfaceRef` must be the same pointer width (it is — both are `*mut __IOSurface`).

- [ ] **Step 5: Write the IOSurface render proof**

`crates/embed-spike/examples/iosurface_png.rs` — create an IOSurface from Rust, run a tick through the real `carapace_*` ABI, then read the surface memory back to a PNG:

```rust
//! Tier-1 proof without Swift: build an IOSurface in Rust, drive the C ABI, dump the surface.
use std::ffi::{c_char, c_void, CStr, CString};

use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use io_surface::{
    kIOSurfaceBytesPerElement, kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth,
    IOSurface, IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceUnlock,
};

use embed_spike::host::CarapaceHostVTable;

extern "C" fn get_num(_c: *mut c_void, key: *const c_char, out: *mut f64) -> bool {
    if unsafe { CStr::from_ptr(key) }.to_str() == Ok("level") {
        unsafe { *out = 0.6 };
        true
    } else {
        false
    }
}

fn main() {
    let (w, h) = (240u32, 80u32);
    // 'BGRA' = 0x42475241.
    let props = CFDictionary::from_CFType_pairs(&[
        (unsafe { CFString::wrap_under_get_rule(kIOSurfaceWidth) }, CFNumber::from(w as i64).as_CFType()),
        (unsafe { CFString::wrap_under_get_rule(kIOSurfaceHeight) }, CFNumber::from(h as i64).as_CFType()),
        (unsafe { CFString::wrap_under_get_rule(kIOSurfaceBytesPerElement) }, CFNumber::from(4i64).as_CFType()),
        (unsafe { CFString::wrap_under_get_rule(kIOSurfacePixelFormat) }, CFNumber::from(0x42475241i64).as_CFType()),
    ]);
    let surface = IOSurface::new(&props);
    let surface_ref = surface.as_concrete_TypeRef();

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skin");
    let cdir = CString::new(dir.to_str().unwrap()).unwrap();
    let vtable = CarapaceHostVTable { ctx: std::ptr::null_mut(), get_num: Some(get_num), get_str: None, invoke: None };

    unsafe {
        let e = embed_spike::carapace_create(cdir.as_ptr(), vtable, surface_ref, w, h);
        assert!(!e.is_null());
        embed_spike::carapace_tick(e, 0.016);
        assert_eq!(embed_spike::carapace_active_tier(e), 1, "expected Tier 1 readback");

        // Read the surface back to a PNG.
        IOSurfaceLock(surface_ref, 0x1 /* read-only */, std::ptr::null_mut());
        let base = IOSurfaceGetBaseAddress(surface_ref) as *const u8;
        let stride = IOSurfaceGetBytesPerRow(surface_ref) as usize;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let p = base.add(y * stride + x * 4);
                // surface is BGRA; write RGBA for the PNG.
                rgba[(y * w as usize + x) * 4] = *p.add(2);
                rgba[(y * w as usize + x) * 4 + 1] = *p.add(1);
                rgba[(y * w as usize + x) * 4 + 2] = *p;
                rgba[(y * w as usize + x) * 4 + 3] = *p.add(3);
            }
        }
        IOSurfaceUnlock(surface_ref, 0x1, std::ptr::null_mut());
        embed_spike::carapace_destroy(e);

        let has_green = rgba.chunks_exact(4).any(|p| p[1] > 180 && p[0] < 180 && p[2] < 160 && p[3] > 0);
        assert!(has_green, "value bar visible in the IOSurface");
        image::save_buffer("target/iosurface_png.png", &rgba, w, h, image::ColorType::Rgba8).unwrap();
        println!("wrote target/iosurface_png.png");
    }
}
```

> The `io-surface` crate's constructor/const names vary by version — after `cargo add`, check its docs/source and match the real `IOSurface::new` + key constants. The goal of this example: prove `carapace_tick` lands the rendered skin in the IOSurface's memory (green bar present, Tier == 1).

- [ ] **Step 6: Run the proof**

Run: `cargo run -p embed-spike --example iosurface_png`
Expected: prints `wrote target/iosurface_png.png`, asserts pass. Open the PNG — dark canvas, green bar, correct colors (if red/blue look swapped, fix the BGRA swizzle in `copy_into_iosurface` and re-run).

- [ ] **Step 7: Commit**

```bash
git add crates/embed-spike
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(embed): Tier 1 — render into a caller-supplied IOSurface (readback)"
```

---

### Task 5: Swift sample app — Tier 1 end-to-end (the headline run)

**Files:**
- Create: `crates/embed-spike/macos-sample/Package.swift`
- Create: `crates/embed-spike/macos-sample/Sources/EmbedSpike/main.swift`
- Create: `crates/embed-spike/macos-sample/Sources/CCarapace/` (module map exposing `carapace.h`)
- Create: `crates/embed-spike/macos-sample/README.md`
- Create: `crates/embed-spike/screenshot.png`

**Interfaces:**
- Consumes: the C ABI from `carapace.h` + `libembed_spike.dylib`.
- Produces: a runnable AppKit app proving display + Swift-owned state + click→action. No Rust changes.

- [ ] **Step 1: Build the dylib for linking**

Run: `cargo build -p embed-spike`
Confirm: `ls target/debug/libembed_spike.dylib` exists. Note its absolute path for the linker flags below.

- [ ] **Step 2: Create the C module wrapper**

`crates/embed-spike/macos-sample/Sources/CCarapace/include/carapace.h` — copy of `crates/embed-spike/carapace.h`.
`crates/embed-spike/macos-sample/Sources/CCarapace/include/module.modulemap`:

```
module CCarapace {
    header "carapace.h"
    link "embed_spike"
    export *
}
```

`crates/embed-spike/macos-sample/Sources/CCarapace/shim.c`: `// empty — header-only module`

- [ ] **Step 3: Create the Swift package**

`crates/embed-spike/macos-sample/Package.swift`:

```swift
// swift-tools-version:5.9
import PackageDescription

let repoTarget = "../../../target/debug"  // adjust if building --release

let package = Package(
    name: "EmbedSpike",
    targets: [
        .systemLibrary(name: "CCarapace", path: "Sources/CCarapace"),
        .executableTarget(
            name: "EmbedSpike",
            dependencies: ["CCarapace"],
            linkerSettings: [
                .unsafeFlags(["-L", repoTarget, "-lembed_spike",
                              "-Xlinker", "-rpath", "-Xlinker", repoTarget])
            ]
        ),
    ]
)
```

- [ ] **Step 4: Write the AppKit app**

`crates/embed-spike/macos-sample/Sources/EmbedSpike/main.swift`:

```swift
import AppKit
import CoreVideo
import IOSurface
import CCarapace

let W: UInt32 = 240, H: UInt32 = 80

// Swift owns the state: a battery level and a paused flag the toggle action flips.
final class HostState {
    var paused = false
    func level() -> Double {
        // Battery fraction 0..1; falls back to a slow wall-clock sweep if unavailable.
        if let frac = batteryFraction(), !paused { return frac }
        if paused { return lastLevel }
        lastLevel = (Date().timeIntervalSince1970.truncatingRemainder(dividingBy: 10)) / 10
        return lastLevel
    }
    var lastLevel: Double = 0.5
}
let state = HostState()

// --- Host vtable callbacks (Swift IS the host) ---
func getNum(_ ctx: UnsafeMutableRawPointer?, _ key: UnsafePointer<CChar>?, _ out: UnsafeMutablePointer<Double>?) -> Bool {
    guard let key = key, let out = out else { return false }
    if String(cString: key) == "level" { out.pointee = state.level(); return true }
    return false
}
func invoke(_ ctx: UnsafeMutableRawPointer?, _ action: UnsafePointer<CChar>?) {
    guard let action = action else { return }
    if String(cString: action) == "toggle" { state.paused.toggle() }
}

// --- The carapace-backed view ---
final class SkinView: NSView {
    var engine: OpaquePointer?
    var surface: IOSurface!
    var last = CACurrentMediaTime()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        surface = IOSurface(properties: [
            .width: Int(W), .height: Int(H),
            .bytesPerElement: 4, .pixelFormat: 0x42475241 /* 'BGRA' */
        ])!
        var vt = CarapaceHostVTable(ctx: nil, get_num: getNum, get_str: nil, invoke: invoke)
        let skinDir = (#filePath as NSString)
            .deletingLastPathComponent + "/../../../skin"   // crates/embed-spike/skin
        engine = carapace_create(skinDir, vt, surface, W, H)
        print("active tier:", carapace_active_tier(engine))
    }
    required init?(coder: NSCoder) { fatalError() }

    func tick() {
        let now = CACurrentMediaTime()
        carapace_tick(engine, now - last)
        last = now
        layer?.contents = surface          // zero-copy display of the surface
        layer?.contentsGravity = .resize
    }

    override func mouseDown(with e: NSEvent) {
        let p = convert(e.locationInWindow, from: nil)
        // AppKit y is bottom-up; canvas y is top-down.
        let cx = Double(p.x) * Double(W) / Double(bounds.width)
        let cy = (Double(bounds.height) - Double(p.y)) * Double(H) / Double(bounds.height)
        carapace_pointer(engine, cx, cy, 0)
    }
}

// --- App bootstrap + display link ---
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let win = NSWindow(contentRect: NSRect(x: 200, y: 200, width: 480, height: 160),
                   styleMask: [.titled, .closable], backing: .buffered, defer: false)
let view = SkinView(frame: win.contentLayoutRect)
view.autoresizingMask = [.width, .height]
win.contentView = view
win.makeKeyAndOrderFront(nil)

var link: CVDisplayLink?
CVDisplayLinkCreateWithActiveCGDisplays(&link)
CVDisplayLinkSetOutputHandler(link!) { _, _, _, _, _ in
    DispatchQueue.main.async { view.tick() }
    return kCVReturnSuccess
}
CVDisplayLinkStart(link!)
app.activate(ignoringOtherApps: true)
app.run()
```

> `IOSurface(properties:)` keys are `IOSurfacePropertyKey` cases (`.width`, `.height`, `.bytesPerElement`, `.pixelFormat`). The `#filePath`-relative skin path must resolve to `crates/embed-spike/skin` at runtime — print it and adjust the `..` count if `swift run`'s working dir differs. `batteryFraction()` is a small helper using `IOPSCopyPowerSourcesInfo`; if it's fiddly on the dev machine, return `nil` and the wall-clock sweep drives the bar (still Swift-owned state — acceptable for the spike).

- [ ] **Step 5: Write the build/run README**

`crates/embed-spike/macos-sample/README.md` — exact commands:

```md
# embed-spike macOS sample

1. Build the Rust dylib:  `cargo build -p embed-spike`
2. Run the app:           `cd crates/embed-spike/macos-sample && swift run`

A window opens showing the spike skin. The green bar tracks the Mac's battery level
(Swift-owned state served across the C ABI). Click the lower strip to toggle "paused" —
the bar freezes/refreshes. The console prints `active tier: 1` (readback) or `2` (shared).
```

- [ ] **Step 6: Human run — confirm the Tier 1 success criterion**

Run the two commands. Confirm, by eye:
1. The window shows the live skin (dark canvas + green bar).
2. The bar's fill tracks the battery level (or the wall-clock sweep) — i.e. Swift-owned state drives Rust-rendered pixels.
3. Clicking the lower strip toggles paused (bar freezes/resumes) — i.e. a click invokes Swift code through the engine.

Capture a screenshot to `crates/embed-spike/screenshot.png`.

> This is a human-confirmed step (like the window spike's Tier 2/3). If any of the three fail, debug before proceeding — Tier 1 working is the floor the whole spike must clear.

- [ ] **Step 7: Commit**

```bash
git add crates/embed-spike/macos-sample crates/embed-spike/screenshot.png
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(embed): Swift AppKit sample — Tier 1 embedding end-to-end"
```

---

### Task 6: Tier 2 — zero-copy IOSurface import into wgpu

**Files:**
- Modify: `crates/embed-spike/src/render.rs` (the Tier 2 import + `Present::Shared`)
- Modify: `crates/embed-spike/src/lib.rs` (`carapace_create` tries Tier 2, falls back to Tier 1; `carapace_tick` skips readback when Shared)
- Add deps: `metal`, `wgpu-hal`, `objc2` (only if needed for the texture descriptor)

**Interfaces:**
- Consumes: `wgpu`'s `Device::as_hal` / `Device::create_texture_from_hal`, `wgpu::hal::api::Metal`, the `metal` crate's `Device`/`Texture`/`TextureDescriptor` + `new_texture_with_iosurface` (or the manual `newTextureWithDescriptor:iosurface:plane:` selector).
- Produces: `pub fn try_shared(device: &wgpu::Device, surface: IOSurfaceRef, w, h) -> Option<wgpu::Texture>` — `Some` when the import works, `None` on any failure (caller then uses Tier 1).

- [ ] **Step 1: Add the Metal interop deps**

Run: `sfw cargo add -p embed-spike metal wgpu-hal`
Expected: both added. Pin `wgpu-hal` to `=29.0.3` to match `wgpu` exactly (ABI of the hal texture import must match the linked wgpu).

- [ ] **Step 2: Write the Tier 2 import behind a clean `Option` boundary**

Append to `crates/embed-spike/src/render.rs`:

```rust
/// Try to build a wgpu texture that aliases the caller's IOSurface (zero-copy).
/// Returns None on any failure so the caller falls back to Tier 1.
pub fn try_shared(device: &wgpu::Device, surface: IOSurfaceRef, w: u32, h: u32) -> Option<wgpu::Texture> {
    use wgpu::hal::api::Metal;

    // 1. Get wgpu's underlying MTLDevice.
    let raw_mtl_device: metal::Device = unsafe {
        device.as_hal::<Metal, _, _>(|hal_device| {
            hal_device.map(|d| d.raw_device().lock().clone())
        })?
    };

    // 2. Build an MTLTexture backed by the IOSurface.
    let desc = metal::TextureDescriptor::new();
    desc.set_texture_type(metal::MTLTextureType::D2);
    desc.set_pixel_format(metal::MTLPixelFormat::BGRA8Unorm);
    desc.set_width(w as u64);
    desc.set_height(h as u64);
    desc.set_usage(metal::MTLTextureUsage::RenderTarget);
    desc.set_storage_mode(metal::MTLStorageMode::Shared);

    // `surface` is io_surface::IOSurfaceRef (== *mut __IOSurface). The metal crate wants an
    // IOSurfaceRef of its own type — both are the same opaque pointer; transmute the pointer.
    let io: metal::foreign_types::ForeignType = unsafe { std::mem::transmute(surface) };
    let mtl_tex: metal::Texture =
        raw_mtl_device.new_texture_with_iosurface(/* see note */ unsafe { std::mem::transmute(surface) }, &desc, 0)?;

    // 3. Import the MTLTexture into wgpu as a texture.
    let hal_tex = unsafe {
        <Metal as wgpu::hal::Api>::Device::texture_from_raw(
            mtl_tex,
            wgpu::TextureFormat::Bgra8Unorm,
            metal::MTLTextureType::D2,
            1, // array layers
            1, // mip levels
            wgpu::hal::CopyExtent { width: w, height: h, depth: 1 },
        )
    };
    let tex = unsafe {
        device.create_texture_from_hal::<Metal>(
            hal_tex,
            &wgpu::TextureDescriptor {
                label: Some("iosurface-shared"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
        )
    };
    Some(tex)
}
```

> **This is the de-risk point — the exact `metal`/`wgpu-hal` calls WILL need adjustment against the linked versions.** Confirm: (a) `Device::as_hal` closure signature + how to reach the raw `metal::Device` (it may be `raw_device()` returning a `Mutex<metal::Device>`); (b) the metal crate's IOSurface texture constructor — it may be `new_texture_with_iosurface`, or you may need to send the `newTextureWithDescriptor:iosurface:plane:` selector via `objc2`/`msg_send!`; (c) `Device::texture_from_raw` / `create_texture_from_hal` signatures in wgpu-hal 29. If any of these don't exist as written, that is itself a finding — record the specific blocker (Task 7) and keep Tier 1 as the shipped result. Do NOT spend unbounded time; timebox Tier 2 and report honestly.

- [ ] **Step 3: Wire Tier 2 selection into `carapace_create` with fallback**

In `lib.rs`, change the `Present`/tier setup so it stores `Present`:

```rust
enum Present {
    Shared { tex: wgpu::Texture },
    Readback { off: OffscreenTarget },
}
```

In `carapace_create`, after building `gpu`:

```rust
let (present, tier) = match crate::render::try_shared(&gpu.device, surface, w, h) {
    Some(tex) => (Present::Shared { tex }, Tier::Shared),
    None => (Present::Readback { off: new_offscreen(&gpu.device, w, h) }, Tier::Readback),
};
```

In `carapace_tick`, draw into the right view and only read back for Tier 1:

```rust
match &e.present {
    Present::Shared { tex } => {
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        render_frame(&mut e.engine, &mut e.renderer, &e.gpu, &view, e.w, e.h, dt);
        // No copy — Swift composites the same surface. poll(Wait) in render_frame ensures completion.
    }
    Present::Readback { off } => {
        render_frame(&mut e.engine, &mut e.renderer, &e.gpu, &off.view, e.w, e.h, dt);
        let rgba = readback_rgba(&e.gpu, &off.tex, e.w, e.h);
        copy_into_iosurface(e.surface, &rgba, e.w, e.h);
    }
}
```

> When rendering into a BGRA shared texture, the engine's RGBA color values may land swapped vs the Tier-1 readback path (which swizzles in the copy). Check the on-screen colors; if swapped, the fix belongs in how the shared texture's format is interpreted, not in the engine. Document whichever is needed.

- [ ] **Step 4: Rebuild + run both proofs**

Run: `cargo build -p embed-spike` then `cargo run -p embed-spike --example iosurface_png`
Expected: still green (the example asserts Tier == 1 — update that assert to accept the tier actually reached, or add a parallel assertion). If Tier 2 compiled, `carapace_active_tier` from the Swift app prints `2`.

- [ ] **Step 5: Human run — Tier 2 verdict**

Run the Swift app (`swift run` in `macos-sample/`). Read the console tier and confirm by eye the skin still displays correctly with `active tier: 2`. Capture the result.
- If Tier 2 works: update `crates/embed-spike/screenshot.png` to the zero-copy run and note "Tier 2 reached" for the findings.
- If Tier 2 does not work after the timebox: leave Tier 1 as shipped, capture the exact compile/runtime blocker for the findings, and move on. **A negative Tier-2 finding is a valid spike result.**

- [ ] **Step 6: Commit**

```bash
git add crates/embed-spike
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "spike(embed): Tier 2 — zero-copy IOSurface import (verdict: <reached|blocked>)"
```

---

### Task 7: Findings doc + wrap-up

**Files:**
- Create: `docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md`
- Verify: `crates/carapace/src/` unchanged

**Interfaces:** none (documentation).

- [ ] **Step 1: Prove the engine stayed untouched**

Run: `git diff --stat main...HEAD -- crates/carapace/src/`
Expected: **no output** (zero lines changed). If anything shows, justify it explicitly in the findings or revert it. Record the result verbatim in the doc.

- [ ] **Step 2: Run the full gate**

Run:
```bash
cargo test -p embed-spike
cargo clippy -p embed-spike --all-targets -- -D warnings
cargo test --workspace   # engine suite still green (it must be, given zero engine changes)
```
Expected: all pass. Fix anything red before writing the verdict.

- [ ] **Step 3: Write the findings doc**

`docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md` — mirror the structure of `2026-06-19-window-replacement-spike-findings.md`. Include, with concrete observed detail (not adjectives):
- **Headline:** is native macOS host embedding feasible? (Tier reached.)
- **Tier 1 (readback):** worked? screenshot ref; the FFI + Swift-host loop confirmed (battery value drove the bar; click toggled). Pixel format/swizzle notes.
- **Tier 2 (zero-copy IOSurface):** the exact `metal`/`wgpu-hal` recipe that worked, OR the specific blocker (missing API, signature mismatch, runtime validation error) — quote it. Alpha/color-space/device-agreement observations.
- **Engine-untouched check:** the `git diff --stat` result.
- **Recommendation for the real `carapace-ffi` phase:** vtable shape that held up; whether to pursue **Approach C** (wgpu on the host's `MTLDevice`) next; and the **Flutter go/no-go signal** (does zero-copy IOSurface → external-texture look viable?).
- **Known limits left open:** collections/`rows`, multi-arg actions, hot-swap, threading, stable ABI.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-06-25-host-embedding-spike-findings.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" \
  commit -m "docs(spike): host-embedding feasibility findings + recommendation"
```

- [ ] **Step 5: Decide integration**

The spike branch is `host-embedding-spike`. Use the `superpowers:finishing-a-development-branch` skill to choose: open a PR (findings + throwaway crate as a reference artifact, like `window-spike`), or keep the branch as a record. Do not merge to `main` without asking.

---

## Self-Review Notes (for the executor)

- **Every wgpu/metal/IOSurface/Swift API name in this plan is provisional** against the locally-linked versions. The authoritative references in-repo: `crates/carapace-demo/src/main.rs` (wgpu device/surface/draw), `crates/window-spike/src/main.rs` (transparent surface + alpha modes on this Mac), `crates/carapace/src/render.rs` (`Renderer::draw` signature), `crates/carapace/src/host.rs` (`Host`/`ActionSpec`), `crates/carapace/src/scene.rs` (`Pt`), `crates/carapace/src/state.rs` (`StateValue` variants). When a name in this plan disagrees with those, **those win** — adjust and keep the test/proof green.
- **Tier 2 is the single real unknown.** Timebox it. Tier 1 green + an honest Tier 2 verdict satisfies the spike's success criteria even if zero-copy doesn't land.
- **The engine must not change.** If you feel the urge to edit `crates/carapace/`, stop and surface it — the whole point is testing whether the existing boundary suffices.

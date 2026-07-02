//! The dedicated render thread: owns the `!Send` Engine + GPU, runs the pacing loop. Apple-only.
#![cfg(any(target_os = "macos", target_os = "ios"))]
// Much of `RenderThread`'s state (surface pool bookkeeping, pacing fields) isn't read by the
// skeleton loop yet — pacing/present land in Tasks 5/6. Allow it to sit unused in the meantime,
// matching `queue.rs`/`snapshot.rs`'s precedent for staged-ahead modules.
#![allow(dead_code)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use carapace::engine::Engine;
use carapace::render::Renderer;

use crate::guard::{CarapaceStatus, set_last_error};
use crate::handle::ContentTex;
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::queue::{Command, CommandRx};
use crate::render::{GpuCtx, IOSurfaceRef, Present, Tier, build_content, build_present, init_gpu};
use crate::snapshot::SnapshotCell;

/// The raw host-owned state that must cross onto the spawned render thread at construction: the
/// IOSurface pool, the optional content surface, and the host vtable (fn pointers + its opaque
/// `ctx`). Bundling them here — rather than passing the vtable as a separate `spawn` param — means
/// every piece of host-owned raw state crossing the thread boundary is covered by ONE audited
/// `unsafe impl Send`, instead of scattering the justification across multiple impls.
///
/// # Safety contract
/// These pointers are caller-owned and guaranteed (by the C ABI contract, see `carapace.h`) to
/// (1) be valid for their declared kind/size and (2) outlive the engine. They are only ever
/// touched by the render thread after this move. The engine itself is built on the render thread
/// and never crosses, so the ONLY thing this wrapper makes `Send` is opaque host memory (surfaces)
/// and host callbacks (the vtable's fn pointers + `ctx`) the host promised are thread-safe to use
/// from our render thread. This is the single load-bearing `Send` assertion in the crate.
pub(crate) struct SendSurfaces {
    pub surfaces: Vec<*const c_void>,
    pub content: *const c_void,
    pub vtable: CarapaceHostVTable,
}

// SAFETY: see the struct's safety contract above.
unsafe impl Send for SendSurfaces {}

/// Reported back from the render thread to `carapace_create` so it can return the create-time
/// status synchronously (the engine + GPU are built ON the render thread).
pub(crate) enum InitResult {
    Ok { cw: u32, ch: u32, tier: Tier },
    Err(CarapaceStatus, String),
}

/// State owned exclusively by the render thread. The `!Send` `Engine`/`Renderer`/`GpuCtx` live here
/// and never cross a thread boundary — they are constructed on the render thread by `build`.
struct RenderThread {
    engine: Engine,
    renderer: Renderer,
    gpu: GpuCtx,
    presents: Vec<Present>,
    surfaces: Vec<IOSurfaceRef>,
    held: Vec<bool>,
    content: Option<ContentTex>,
    tier: Tier,
    w: u32,
    h: u32,
    cw: u32,
    ch: u32,
    fps: u32,
    next_surface: usize,
    frame_id: u64,
    last_render: Instant,
    /// Host callbacks, copied in at `build` time so `render_one` can fire `frame_ready` from THIS
    /// thread without touching the front-end handle.
    vtable: CarapaceHostVTable,
}

const DEFAULT_FPS: u32 = 60;

/// Build the engine + GPU + per-surface present pool ON the render thread. Returns the state, or a
/// status + message on failure (reported back so `carapace_create` can return synchronously).
fn build(
    dir: &Path,
    vtable: CarapaceHostVTable,
    surfaces: Vec<IOSurfaceRef>,
    content_surface: IOSurfaceRef,
    w: u32,
    h: u32,
) -> Result<RenderThread, (CarapaceStatus, String)> {
    let (_m, source) = carapace::skin::load_dir(dir).map_err(|e| {
        (
            CarapaceStatus::ErrBadSkin,
            format!("skin load failed: {e:?}"),
        )
    })?;
    let engine = Engine::new(
        Box::new(FfiHost::new(vtable)),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .map_err(|e| {
        (
            CarapaceStatus::ErrBadSkin,
            format!("engine init failed: {e:?}"),
        )
    })?;
    let (cw, ch) = engine.scene().canvas;
    let gpu = init_gpu().map_err(|m| (CarapaceStatus::ErrGpuInit, m))?;
    let renderer = Renderer::new(&gpu.device);

    // One Present per pooled surface. Tier is the WEAKEST any surface resolved to (if any fell back
    // to Readback, report Readback) so `active_tier` never over-promises.
    let mut presents = Vec::with_capacity(surfaces.len());
    let mut tier = Tier::Shared;
    for &s in &surfaces {
        let (p, t) = build_present(&gpu, s, w, h);
        if t == Tier::Readback {
            tier = Tier::Readback;
        }
        presents.push(p);
    }
    let content = build_content(&gpu, content_surface);
    let held = vec![false; surfaces.len()];
    Ok(RenderThread {
        engine,
        renderer,
        gpu,
        presents,
        surfaces,
        held,
        content,
        tier,
        w,
        h,
        cw,
        ch,
        fps: DEFAULT_FPS,
        next_surface: 0,
        frame_id: 0,
        last_render: Instant::now(),
        vtable,
    })
}

impl RenderThread {
    /// Round-robin from `next_surface`, skipping surfaces the host still holds. `None` means the
    /// host holds every pooled surface — the caller must skip the frame (never block, never tear).
    fn pick_free_surface(&self) -> Option<usize> {
        let n = self.surfaces.len();
        (0..n)
            .map(|i| (self.next_surface + i) % n)
            .find(|&i| !self.held[i])
    }

    /// Render exactly one frame into the next free pooled surface and announce it via
    /// `frame_ready`. Backpressure (every surface held by the host) silently skips the frame.
    fn render_one(&mut self, dt: Duration) {
        let Some(idx) = self.pick_free_surface() else {
            // Backpressure: host holds every surface. Skip this frame (never block, never tear).
            return;
        };
        // Upload this frame's host content (CPU->GPU coherency) before the destructure below,
        // which needs `self.gpu`/`self.content` split from `self.engine`/`self.renderer`.
        if let Some(c) = self.content.as_ref() {
            unsafe {
                crate::render::upload_iosurface_to_texture(
                    &self.gpu.queue,
                    c.surface,
                    &c.tex,
                    c.w,
                    c.h,
                )
            };
        }
        // Destructure into locals (mirrors the old v1 `tick_inner` pattern) so `render_frame` can
        // borrow `engine`/`renderer` mutably while `gpu`/`presents`/`surfaces`/`content` are read
        // immutably in the same expression — the borrow checker can't see that split through
        // `self.field` access alone.
        let RenderThread {
            engine,
            renderer,
            gpu,
            presents,
            surfaces,
            content,
            w,
            h,
            ..
        } = self;
        let (w, h) = (*w, *h);
        let host_view = content.as_ref().map(|c| ("host", &c.view));
        match &presents[idx] {
            Present::Shared {
                off,
                iosurface_view,
                blitter,
                ..
            } => {
                crate::render::render_frame(
                    engine, renderer, gpu, &off.view, w, h, dt, false, host_view,
                );
                crate::render::blit(gpu, blitter, &off.view, iosurface_view);
            }
            Present::Readback { off } => {
                crate::render::render_frame(
                    engine, renderer, gpu, &off.view, w, h, dt, true, host_view,
                );
                let rgba = crate::render::readback_rgba(gpu, &off.tex, w, h);
                unsafe { crate::render::copy_into_iosurface(surfaces[idx], &rgba, w, h) };
            }
        }
        self.held[idx] = true;
        self.next_surface = (idx + 1) % self.surfaces.len();
        self.frame_id += 1;
        // Announce readiness to the host (on THIS render thread).
        if let Some(cb) = self.vtable.frame_ready {
            cb(self.vtable.ctx, idx as u32, self.frame_id);
        }
    }
}

#[allow(clippy::too_many_arguments)] // the render thread needs all of these at construction; a
// param struct would just move the noise. Documented deviation from the 7-arg lint.
pub(crate) fn spawn(
    dir: PathBuf,
    send_surfaces: SendSurfaces,
    w: u32,
    h: u32,
    rx: CommandRx,
    cell: SnapshotCell,
    poisoned: Arc<AtomicBool>,
    init_tx: mpsc::Sender<InitResult>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("carapace-render".into())
        .spawn(move || {
            // Bind the whole value first so the closure captures `send_surfaces` as ONE unit (which
            // is `Send`). Destructuring it directly in the capture list would, under Rust 2021's
            // disjoint closure captures, capture each field separately and bypass the `Send` impl.
            let send_surfaces = send_surfaces;
            let SendSurfaces {
                surfaces,
                content,
                vtable,
            } = send_surfaces;
            let surfaces: Vec<IOSurfaceRef> =
                surfaces.into_iter().map(|p| p as IOSurfaceRef).collect();
            let mut rt = match build(&dir, vtable, surfaces, content as IOSurfaceRef, w, h) {
                Ok(rt) => {
                    let _ = init_tx.send(InitResult::Ok {
                        cw: rt.cw,
                        ch: rt.ch,
                        tier: rt.tier,
                    });
                    rt
                }
                Err((status, msg)) => {
                    set_last_error(&msg);
                    let _ = init_tx.send(InitResult::Err(status, msg));
                    return;
                }
            };
            run_loop(&mut rt, &rx, &cell, &poisoned);
        })
        .expect("spawn carapace render thread")
}

/// Skeleton loop: block for a command and handle each one directly (no pacing yet — Task 6 replaces
/// the `rx.recv()` blocking wait with `recv_timeout`-based free-run pacing and snapshot publish).
fn run_loop(
    rt: &mut RenderThread,
    rx: &CommandRx,
    _cell: &SnapshotCell,
    _poisoned: &Arc<AtomicBool>,
) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::Invalidate => {
                let now = Instant::now();
                let dt = now.duration_since(rt.last_render);
                rt.last_render = now;
                rt.render_one(dt);
            }
            Command::SetFrameRate(fps) => {
                rt.fps = fps;
            }
            Command::ReleaseSurface(i) => {
                if let Some(slot) = rt.held.get_mut(i as usize) {
                    *slot = false;
                }
            }
            Command::Shutdown => break,
            // Task 7 wires pointer routing into the snapshot/hit-test path; ignored here.
            Command::Pointer { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_surfaces_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SendSurfaces>();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod render_tests {
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    static FRAME_READY_COUNT: AtomicU32 = AtomicU32::new(0);
    static LAST_FRAME_ID: AtomicU64 = AtomicU64::new(0);

    extern "C" fn on_frame_ready(_ctx: *mut std::ffi::c_void, _idx: u32, frame_id: u64) {
        FRAME_READY_COUNT.fetch_add(1, Ordering::SeqCst);
        LAST_FRAME_ID.store(frame_id, Ordering::SeqCst);
    }

    #[test]
    fn one_invalidate_renders_nonblank_and_fires_frame_ready_once() {
        FRAME_READY_COUNT.store(0, Ordering::SeqCst);
        // Build via carapace_create with fps set to 0 (paused) so only our invalidate renders.
        let (w, h) = (300u32, 140u32);
        let vt = crate::host::CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: Some(on_frame_ready),
        };
        let (handle, surfaces) =
            crate::handle::test_support::create_test_handle_pool_vt(w, h, 2, vt);
        assert_eq!(
            unsafe { crate::handle::carapace_set_frame_rate(handle, 0) },
            crate::guard::CarapaceStatus::Ok
        );
        assert_eq!(
            unsafe { crate::handle::carapace_invalidate(handle) },
            crate::guard::CarapaceStatus::Ok
        );
        // Give the render thread a moment to process the single frame.
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            FRAME_READY_COUNT.load(Ordering::SeqCst),
            1,
            "exactly one frame"
        );
        assert_eq!(
            LAST_FRAME_ID.load(Ordering::SeqCst),
            1,
            "frame_id starts at 1"
        );
        // The surface handed to frame_ready must be non-blank. `pick_free_surface` round-robins
        // from index 0 on a freshly-built thread, so the first invalidate lands in surfaces[0].
        assert!(
            unsafe { crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h) },
            "rendered surface must be non-blank"
        );
        unsafe { crate::handle::carapace_destroy(handle) };
    }
}

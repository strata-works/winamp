//! The dedicated render thread: owns the `!Send` Engine + GPU, runs the pacing loop. Apple-only.
#![cfg(any(target_os = "macos", target_os = "ios"))]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use carapace::engine::{Engine, PointerEvent};
use carapace::render::Renderer;
use carapace::scene::{Pt, Scene};

use crate::crossfade::CrossfadeBlender;
use crate::guard::{CarapaceStatus, last_error_string, set_last_error};
use crate::handle::ContentTex;
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::queue::{Command, CommandRx, PointerKind, drain_coalescing};
use crate::render::{
    GpuCtx, IOSurfaceRef, OffscreenTarget, Present, Tier, build_content, build_present, init_gpu,
    new_offscreen,
};
use crate::snapshot::{SnapshotCell, SnapshotTier};
use carapace::skin::Transition;

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
    Ok { tier: Tier },
    Err(CarapaceStatus, String),
}

/// The render thread's live skin-swap phase. `Idle` is the normal single-skin path. `Warming` holds
/// a freshly built incoming engine that has not yet been warmed (asset decode/upload happens on its
/// first offscreen render). `Crossfading` holds the *outgoing* engine while `self.engine` is already
/// the incoming skin; the two are blended by `elapsed/dur` progress.
enum SwapState {
    Idle,
    Warming {
        incoming: Engine,
        transition: Transition,
    },
    Crossfading {
        outgoing: Engine,
        elapsed: Duration,
        dur: Duration,
    },
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
    /// Live skin-swap phase. `Idle` when no swap is in flight.
    swap: SwapState,
    /// Scratch offscreen the *incoming* skin renders into during warm/crossfade.
    tex_a: OffscreenTarget,
    /// Scratch offscreen the *outgoing* skin renders into during crossfade.
    tex_b: OffscreenTarget,
    /// The GPU pass that blends `tex_b` (old) and `tex_a` (new) into the present offscreen.
    blender: CrossfadeBlender,
    /// Test-only: set by `Command::ForcePanic`; checked inside `render_guarded`'s `catch_unwind` so
    /// a forced panic sets `poisoned` via the exact same path a genuine render panic would.
    #[cfg(test)]
    force_panic: bool,
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
    // `tex_a`/`tex_b`/`blender` borrow `gpu.device`, and `gpu` is moved into the struct literal
    // below — build these into `let` bindings first to avoid a move-before-borrow error.
    let tex_a = new_offscreen(&gpu.device, w, h);
    let tex_b = new_offscreen(&gpu.device, w, h);
    let blender = CrossfadeBlender::new(&gpu.device);
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
        swap: SwapState::Idle,
        tex_a,
        tex_b,
        blender,
        #[cfg(test)]
        force_panic: false,
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

    /// Render exactly one frame into the next free pooled surface. Lays out ONCE (inside
    /// `render_frame`) and returns that laid-out `Scene` plus the surface index and frame id, so the
    /// caller can publish the snapshot and only THEN fire `frame_ready` — the spec's data-flow order
    /// is present -> publish snapshot -> frame_ready, so a host reacting to the callback never reads
    /// a stale (previous-frame) snapshot. This method itself never calls `frame_ready`. Backpressure
    /// (every surface held by the host) silently skips the frame and returns `None` — nothing is
    /// published and no callback fires.
    fn render_one(&mut self, dt: Duration) -> Option<(Scene, u32, u64)> {
        let Some(idx) = self.pick_free_surface() else {
            // Backpressure: host holds every surface. Skip this frame (never block, never tear).
            return None;
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
        let scene = match std::mem::replace(&mut self.swap, SwapState::Idle) {
            SwapState::Idle => self.render_single_into_present(idx, dt),

            SwapState::Warming {
                mut incoming,
                transition,
            } => {
                // 1. Old skin keeps presenting this frame.
                let scene = self.render_single_into_present(idx, dt);
                // 2. Warm the incoming engine: one offscreen render forces asset decode + upload.
                self.warm_incoming(&mut incoming, dt);
                // 3. Transition. On entering Crossfading, `self.engine` becomes the incoming skin,
                //    so the render_one tail's `cw/ch = self.engine.scene().canvas` flips hit-testing
                //    to the new skin from the first crossfade frame.
                match transition.kind {
                    carapace::skin::TransitionKind::Cut => {
                        self.engine = incoming; // promote; swap stays Idle (already replaced)
                    }
                    carapace::skin::TransitionKind::Crossfade => {
                        let outgoing = std::mem::replace(&mut self.engine, incoming);
                        self.swap = SwapState::Crossfading {
                            outgoing,
                            elapsed: Duration::ZERO,
                            dur: Duration::from_millis(transition.duration_ms as u64),
                        };
                    }
                }
                scene
            }

            SwapState::Crossfading {
                mut outgoing,
                elapsed,
                dur,
            } => {
                let elapsed = elapsed + dt;
                let t = crossfade_t(elapsed, dur);
                let scene = self.render_crossfade(idx, &mut outgoing, dt, t);
                // t < 1 → stay crossfading (carry the advanced elapsed); t >= 1 → drop `outgoing`,
                // swap is already `Idle` from the mem::replace above.
                if t < 1.0 {
                    self.swap = SwapState::Crossfading {
                        outgoing,
                        elapsed,
                        dur,
                    };
                }
                scene
            }
        };

        self.held[idx] = true;
        self.next_surface = (idx + 1) % self.surfaces.len();
        self.frame_id += 1;
        // Refresh the design canvas from the scene we just laid out, so pointer hit-testing uses
        // the current skin's canvas even across a skin swap. The swap applies during render_frame's
        // engine.update, so the new canvas is only observable after the frame is produced.
        let (cw, ch) = self.engine.scene().canvas;
        self.cw = cw;
        self.ch = ch;
        // Do NOT fire `frame_ready` here — the caller (`render_guarded`) must publish the snapshot
        // first, then fire the callback, so a host reacting to it always sees this frame's snapshot.
        Some((scene, idx as u32, self.frame_id))
    }

    /// Render the current `self.engine` into `presents[idx].off` and present it. This is the
    /// unchanged single-skin path (former inline body of `render_one`).
    fn render_single_into_present(&mut self, idx: usize, dt: Duration) -> Scene {
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
        // `render_frame` lays out once and returns that scene; capture it so we publish the exact
        // scene we drew (no second layout pass).
        match &presents[idx] {
            Present::Shared {
                off,
                iosurface_view,
                blitter,
                ..
            } => {
                let scene = crate::render::render_frame(
                    engine, renderer, gpu, &off.view, w, h, dt, false, host_view,
                );
                crate::render::blit(gpu, blitter, &off.view, iosurface_view);
                scene
            }
            Present::Readback { off } => {
                let scene = crate::render::render_frame(
                    engine, renderer, gpu, &off.view, w, h, dt, true, host_view,
                );
                let rgba = crate::render::readback_rgba(gpu, &off.tex, w, h);
                unsafe { crate::render::copy_into_iosurface(surfaces[idx], &rgba, w, h) };
                scene
            }
        }
    }

    /// Render the incoming engine once into scratch `tex_a` purely to force its lazy asset decode
    /// and GPU texture upload (the cost we hide behind the still-animating old skin). The result is
    /// discarded — the old skin's frame is the one presented this iteration.
    fn warm_incoming(&mut self, incoming: &mut Engine, dt: Duration) {
        let RenderThread {
            renderer,
            gpu,
            tex_a,
            content,
            w,
            h,
            ..
        } = self;
        let host_view = content.as_ref().map(|c| ("host", &c.view));
        let _ = crate::render::render_frame(
            incoming,
            renderer,
            gpu,
            &tex_a.view,
            *w,
            *h,
            dt,
            false,
            host_view,
        );
    }

    /// Render `self.engine` (incoming) into `tex_a` and `outgoing` into `tex_b`, blend by `t` into
    /// `presents[idx].off`, then present. Returns the incoming engine's laid-out scene (what the
    /// snapshot publishes — hit-testing already targets the incoming skin).
    fn render_crossfade(
        &mut self,
        idx: usize,
        outgoing: &mut Engine,
        dt: Duration,
        t: f32,
    ) -> Scene {
        // Render incoming (self.engine) -> tex_a; capture its scene for the snapshot.
        let scene = {
            let RenderThread {
                engine,
                renderer,
                gpu,
                tex_a,
                content,
                w,
                h,
                ..
            } = self;
            let host_view = content.as_ref().map(|c| ("host", &c.view));
            crate::render::render_frame(
                engine,
                renderer,
                gpu,
                &tex_a.view,
                *w,
                *h,
                dt,
                false,
                host_view,
            )
        };
        // Render outgoing -> tex_b.
        {
            let RenderThread {
                renderer,
                gpu,
                tex_b,
                content,
                w,
                h,
                ..
            } = self;
            let host_view = content.as_ref().map(|c| ("host", &c.view));
            let _ = crate::render::render_frame(
                outgoing,
                renderer,
                gpu,
                &tex_b.view,
                *w,
                *h,
                dt,
                false,
                host_view,
            );
        }
        // Blend tex_b (old) over/into tex_a (new) into the present offscreen (`off.view` is the same
        // for both tiers), then present it via the shared blit/readback path.
        let off_view = match &self.presents[idx] {
            Present::Shared { off, .. } => &off.view,
            Present::Readback { off } => &off.view,
        };
        self.blender
            .draw(&self.gpu, &self.tex_b.view, &self.tex_a.view, off_view, t);
        self.present_offscreen(idx);
        scene
    }

    /// Present offscreen `presents[idx].off` into pooled `surfaces[idx]` (Tier-2 blit / Tier-1
    /// readback). Assumes the offscreen already holds this frame's pixels.
    fn present_offscreen(&self, idx: usize) {
        match &self.presents[idx] {
            Present::Shared {
                off,
                iosurface_view,
                blitter,
                ..
            } => {
                crate::render::blit(&self.gpu, blitter, &off.view, iosurface_view);
            }
            Present::Readback { off } => {
                let rgba = crate::render::readback_rgba(&self.gpu, &off.tex, self.w, self.h);
                unsafe {
                    crate::render::copy_into_iosurface(self.surfaces[idx], &rgba, self.w, self.h)
                };
            }
        }
    }

    /// Apply one drained command to render-thread state. Returns `false` on `Shutdown` (the loop
    /// then exits); otherwise `true`. Sets `*invalidated` when the command should trigger a frame
    /// this iteration — either an explicit `Invalidate` or an input event (so input shows a frame
    /// even while paused).
    fn apply(&mut self, cmd: Command, invalidated: &mut bool) -> bool {
        match cmd {
            Command::Shutdown => return false,
            Command::SetFrameRate(f) => self.fps = f,
            Command::ReleaseSurface(i) => {
                if let Some(slot) = self.held.get_mut(i as usize) {
                    *slot = false;
                }
            }
            Command::Invalidate => *invalidated = true,
            Command::Pointer { x, y, kind } => {
                if let Some(ev) = map_pointer(kind) {
                    self.engine.handle_pointer_resolved(
                        self.cw as f32,
                        self.ch as f32,
                        Pt {
                            x: x as f32,
                            y: y as f32,
                        },
                        ev,
                    );
                }
                *invalidated = true; // input should show a frame even when paused
            }
            Command::SwapSkin { dir, reply } => {
                let status = match carapace::skin::load_dir(&dir) {
                    Ok((manifest, source)) => {
                        match Engine::new(
                            Box::new(FfiHost::new(self.vtable)),
                            carapace::vocab::VocabRegistry::base(),
                            source,
                        ) {
                            Ok(incoming) => {
                                // Last-writer-wins: a swap already in flight is replaced.
                                self.swap = SwapState::Warming {
                                    incoming,
                                    transition: manifest.transition,
                                };
                                *invalidated = true; // drive the warm/blend immediately
                                CarapaceStatus::Ok
                            }
                            Err(e) => {
                                set_last_error(&format!("swap_skin: engine init failed: {e:?}"));
                                CarapaceStatus::ErrBadSkin
                            }
                        }
                    }
                    Err(e) => {
                        set_last_error(&format!("swap_skin: load failed: {e:?}"));
                        CarapaceStatus::ErrBadSkin
                    }
                };
                let _ = reply.send(status);
            }
            Command::SwapSkinResized {
                dir,
                pool,
                w,
                h,
                reply,
            } => {
                let status = match carapace::skin::load_dir(&dir) {
                    Ok((manifest, source)) => {
                        match Engine::new(
                            Box::new(FfiHost::new(self.vtable)),
                            carapace::vocab::VocabRegistry::base(),
                            source,
                        ) {
                            Ok(incoming) => {
                                // Rebuild the present pool at the new size from the host's surfaces.
                                let new_surfaces: Vec<IOSurfaceRef> = pool
                                    .surfaces
                                    .into_iter()
                                    .map(|p| p as IOSurfaceRef)
                                    .collect();
                                let mut new_presents = Vec::with_capacity(new_surfaces.len());
                                let mut tier = Tier::Shared;
                                for &s in &new_surfaces {
                                    let (p, t) = build_present(&self.gpu, s, w, h);
                                    if t == Tier::Readback {
                                        tier = Tier::Readback;
                                    }
                                    new_presents.push(p);
                                }
                                let new_content =
                                    build_content(&self.gpu, pool.content as IOSurfaceRef);
                                let n = new_surfaces.len();
                                // Atomic switch: old Presents drop (our wgpu wrappers freed); the host
                                // owns + frees the old IOSurfaces after this call returns.
                                self.surfaces = new_surfaces;
                                self.presents = new_presents;
                                self.held = vec![false; n];
                                self.content = new_content;
                                self.tier = tier;
                                self.w = w;
                                self.h = h;
                                self.tex_a = new_offscreen(&self.gpu.device, w, h);
                                self.tex_b = new_offscreen(&self.gpu.device, w, h);
                                self.next_surface = 0;
                                // Warm the incoming skin, then crossfade — now in the new pool.
                                self.swap = SwapState::Warming {
                                    incoming,
                                    transition: manifest.transition,
                                };
                                *invalidated = true;
                                CarapaceStatus::Ok
                            }
                            Err(e) => {
                                set_last_error(&format!(
                                    "swap_skin_resized: engine init failed: {e:?}"
                                ));
                                CarapaceStatus::ErrBadSkin
                            }
                        }
                    }
                    Err(e) => {
                        set_last_error(&format!("swap_skin_resized: load failed: {e:?}"));
                        CarapaceStatus::ErrBadSkin
                    }
                };
                let _ = reply.send(status);
            }
            #[cfg(test)]
            Command::ForcePanic => {
                self.force_panic = true;
                *invalidated = true; // drive the next render_guarded call, where it actually panics
            }
        }
        true
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
    poison_msg: Arc<Mutex<String>>,
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
                    let _ = init_tx.send(InitResult::Ok { tier: rt.tier });
                    rt
                }
                Err((status, msg)) => {
                    set_last_error(&msg);
                    let _ = init_tx.send(InitResult::Err(status, msg));
                    return;
                }
            };
            run_loop(&mut rt, &rx, &cell, &poisoned, &poison_msg);
        })
        .expect("spawn carapace render thread")
}

/// The interval between paced frames at the current fps. Used both to size the wait before the next
/// deadline and to clamp `dt`.
fn frame_interval(fps: u32) -> Duration {
    // Paused (fps == 0) has no paced deadline; use a nominal 60fps interval only for the dt clamp.
    let effective = if fps > 0 { fps } else { 60 };
    Duration::from_secs_f64(1.0 / effective as f64)
}

/// Eased crossfade progress in `[0, 1]`: linear ratio `elapsed/dur`, clamped, then smoothstep for a
/// natural dissolve. A zero duration completes instantly (`1.0`), so a `duration_ms = 0` skin never
/// wedges the loop in a blend.
fn crossfade_t(elapsed: Duration, dur: Duration) -> f32 {
    if dur.is_zero() {
        return 1.0;
    }
    let x = (elapsed.as_secs_f32() / dur.as_secs_f32()).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Free-run pacing loop. Running (`fps > 0`): wake at the next frame deadline OR on a command;
/// paused (`fps == 0`): effectively block on commands and render only on Invalidate/Pointer.
/// A panic in the render body poisons the handle and exits the thread (never abort).
fn run_loop(
    rt: &mut RenderThread,
    rx: &CommandRx,
    cell: &SnapshotCell,
    poisoned: &Arc<AtomicBool>,
    poison_msg: &Arc<Mutex<String>>,
) {
    // Start the pacing clock when the loop actually begins (GPU build already elapsed). This also
    // gives the host's initial configuration (e.g. `set_frame_rate(0)` right after create) a full
    // frame interval to arrive before the first paced timeout could fire.
    rt.last_render = Instant::now();
    let mut pending: Vec<Command> = Vec::new();
    loop {
        // Decide how long to wait: running (or a swap in flight) → until the frame deadline OR a
        // command; paused with no swap → block.
        let animating = rt.fps > 0 || !matches!(rt.swap, SwapState::Idle);
        let wait = if animating {
            frame_interval(rt.fps).saturating_sub(rt.last_render.elapsed())
        } else {
            Duration::from_secs(3600) // effectively "block until a command"
        };

        match rx.recv_timeout(wait) {
            Ok(first) => {
                // Drain + coalesce every command queued right now, then act on the batch.
                drain_coalescing(rx, first, &mut pending);
                let mut invalidated = false;
                for cmd in pending.drain(..) {
                    if !rt.apply(cmd, &mut invalidated) {
                        return; // Shutdown
                    }
                }
                if invalidated {
                    render_guarded(rt, cell, poisoned, poison_msg);
                }
            }
            // Frame deadline reached while running: render one paced frame.
            Err(RecvTimeoutError::Timeout) => {
                if rt.fps > 0 || !matches!(rt.swap, SwapState::Idle) {
                    render_guarded(rt, cell, poisoned, poison_msg);
                }
            }
            // The command sender (handle front-end) was dropped: nothing more can arrive.
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if poisoned.load(Ordering::Acquire) {
            return;
        }
    }
}

/// Render one frame with a panic guard, then publish the laid-out scene for hit-testing. On a panic
/// in the render body the handle is poisoned and the thread exits (via `run_loop`'s poison check);
/// we NEVER abort. The process-wide panic hook (installed by `carapace_create`) runs first and
/// writes the message into THIS render thread's `last_error` TLS; we then lift that message into the
/// shared `poison_msg` slot so the poison path in each front-end export can surface it to the host
/// on the caller's own thread (the render thread's TLS is otherwise unreachable from there).
fn render_guarded(
    rt: &mut RenderThread,
    cell: &SnapshotCell,
    poisoned: &Arc<AtomicBool>,
    poison_msg: &Arc<Mutex<String>>,
) {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let now = Instant::now();
    // Clamp a huge idle/after-park gap so a paused-then-resumed engine doesn't see a giant dt.
    let dt = now
        .saturating_duration_since(rt.last_render)
        .min(frame_interval(rt.fps) * 4);
    let tier = match rt.tier {
        Tier::Readback => SnapshotTier::Readback,
        Tier::Shared => SnapshotTier::Shared,
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // Test-only forced-panic path (see `Command::ForcePanic`): panics HERE, inside this same
        // `catch_unwind`, so it poisons + exits via the exact contract a real render panic would.
        #[cfg(test)]
        if rt.force_panic {
            panic!("forced render-thread panic");
        }
        // Publish the exact laid-out scene `render_one` drew (single layout) BEFORE announcing
        // `frame_ready`, matching the spec's data-flow order (present -> publish snapshot ->
        // frame_ready) so a host reacting to the callback never observes a stale (previous-frame)
        // snapshot. `None` = backpressure skipped this frame, so nothing is published and the
        // callback does not fire.
        if let Some((scene, idx, frame_id)) = rt.render_one(dt) {
            crate::snapshot::publish(cell, scene, tier);
            if let Some(cb) = rt.vtable.frame_ready {
                cb(rt.vtable.ctx, idx, frame_id);
            }
        }
    }));
    // Reset the pacing clock even on a backpressure skip (no free surface) — intentional: the next
    // iteration's dt is still bounded by the `frame_interval(rt.fps) * 4` clamp above, so a run of
    // skipped frames can't accumulate an unbounded dt once a surface frees up.
    rt.last_render = now;
    if result.is_err() {
        // The panic hook already captured the message into this render thread's `last_error` TLS;
        // copy it into the shared slot BEFORE flipping `poisoned` so any export that observes the
        // poison flag can already read the message. Poison-recovering lock so a poisoned `Mutex`
        // can't wedge the exit path.
        *poison_msg.lock().unwrap_or_else(|e| e.into_inner()) = last_error_string();
        poisoned.store(true, Ordering::Release);
    }
}

/// Map a queued pointer kind to an engine pointer event. The engine models `Press` today; the other
/// kinds are plumbed through the ABI but are no-ops for now (additive — future engine work fills in).
fn map_pointer(kind: PointerKind) -> Option<PointerEvent> {
    match kind {
        PointerKind::Press => Some(PointerEvent::Press),
        _ => None,
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

    #[test]
    fn crossfade_t_endpoints_and_midpoint() {
        use std::time::Duration;
        let dur = Duration::from_millis(200);
        assert_eq!(super::crossfade_t(Duration::ZERO, dur), 0.0);
        assert_eq!(super::crossfade_t(Duration::from_millis(200), dur), 1.0);
        assert_eq!(super::crossfade_t(Duration::from_millis(400), dur), 1.0); // clamped past end
        // smoothstep(0.5) == 0.5
        let mid = super::crossfade_t(Duration::from_millis(100), dur);
        assert!((mid - 0.5).abs() < 1e-6, "mid was {mid}");
        // zero duration completes instantly
        assert_eq!(super::crossfade_t(Duration::ZERO, Duration::ZERO), 1.0);
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
            row_count: None,
            get_row_str: None,
            get_row_num: None,
            invoke_arg: None,
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
        // Wait for the single paused-render to land. The FIRST GPU frame pays a one-time vello
        // pipeline/shader-compile cost that, under parallel GPU-test contention, can exceed several
        // hundred ms — a fixed short sleep is flaky (the frame is produced, just late). Poll up to a
        // generous ceiling for the frame to arrive; the paused engine cannot produce a second frame,
        // so the exact-count assertions below still hold.
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || {
            FRAME_READY_COUNT.load(Ordering::SeqCst) >= 1
        });
        // A short settle so a stray extra frame (there should be none while paused) would show up.
        std::thread::sleep(std::time::Duration::from_millis(50));
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

    #[test]
    fn swap_skin_applies_and_bad_dir_is_rejected() {
        let (w, h) = (300u32, 140u32);
        let vt = crate::host::CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: None,
            row_count: None,
            get_row_str: None,
            get_row_num: None,
            invoke_arg: None,
        };
        let (handle, surfaces) =
            crate::handle::test_support::create_test_handle_pool_vt(w, h, 2, vt);
        assert_eq!(
            unsafe { crate::handle::carapace_set_frame_rate(handle, 0) },
            crate::guard::CarapaceStatus::Ok
        );
        // A valid skin dir → Ok, and a following invalidate renders a non-blank frame. The test
        // fixture loads `skins/classic` by default, so swap to a DIFFERENT base-vocab skin
        // (`minimal`) to prove a real content swap. Both load under `VocabRegistry::base()`.
        let good = std::ffi::CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../carapace-demo/skins/minimal"
        ))
        .unwrap();
        assert_eq!(
            unsafe { crate::handle::carapace_swap_skin(handle, good.as_ptr()) },
            crate::guard::CarapaceStatus::Ok
        );
        assert_eq!(
            unsafe { crate::handle::carapace_invalidate(handle) },
            crate::guard::CarapaceStatus::Ok
        );
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || unsafe {
            crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h)
        });
        assert!(unsafe {
            crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[0], w, h)
        });
        // Swap to a skin with a DIFFERENT canvas size (`frame` is 480x320 vs. `minimal`'s
        // 300x140) to exercise the canvas-changing path the cw/ch refresh fix targets. This
        // doesn't reach into the private cw/ch fields; it just confirms the swap still renders
        // a non-blank frame when the canvas dimensions change underneath it.
        let bigger = std::ffi::CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../carapace-demo/skins/frame"
        ))
        .unwrap();
        assert_eq!(
            unsafe { crate::handle::carapace_swap_skin(handle, bigger.as_ptr()) },
            crate::guard::CarapaceStatus::Ok
        );
        assert_eq!(
            unsafe { crate::handle::carapace_invalidate(handle) },
            crate::guard::CarapaceStatus::Ok
        );
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || unsafe {
            crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[1], w, h)
        });
        assert!(unsafe {
            crate::handle::test_support::iosurface_has_nonzero_pixels(surfaces[1], w, h)
        });
        // A bad dir → ErrBadSkin, engine intact.
        let bad = std::ffi::CString::new("/no/such/skin/dir").unwrap();
        assert_eq!(
            unsafe { crate::handle::carapace_swap_skin(handle, bad.as_ptr()) },
            crate::guard::CarapaceStatus::ErrBadSkin
        );
        unsafe { crate::handle::carapace_destroy(handle) };
    }

    #[test]
    fn swap_resized_adopts_new_pool_and_renders() {
        use std::ffi::c_void;
        let (w1, h1) = (300u32, 140u32);
        let vt = crate::host::CarapaceHostVTable {
            ctx: std::ptr::null_mut(),
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: None,
            row_count: None,
            get_row_str: None,
            get_row_num: None,
            invoke_arg: None,
        };
        let (handle, _old) = crate::handle::test_support::create_test_handle_pool_vt(w1, h1, 2, vt);
        assert_eq!(
            unsafe { crate::handle::carapace_set_frame_rate(handle, 0) },
            crate::guard::CarapaceStatus::Ok
        );

        // A NEW pool at the `frame` skin's native size (480x320).
        let (w2, h2) = (480u32, 320u32);
        let new_surfaces: Vec<crate::render::IOSurfaceRef> = (0..2)
            .map(|_| crate::handle::test_support::make_bgra_iosurface(w2 as usize, h2 as usize))
            .collect();
        let refs: Vec<*const c_void> = new_surfaces.iter().map(|&s| s as *const c_void).collect();
        let dir = std::ffi::CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../carapace-demo/skins/frame"
        ))
        .unwrap();

        assert_eq!(
            unsafe {
                crate::handle::carapace_swap_skin_resized(
                    handle,
                    dir.as_ptr(),
                    refs.as_ptr(),
                    refs.len() as u32,
                    w2,
                    h2,
                    std::ptr::null(),
                )
            },
            crate::guard::CarapaceStatus::Ok
        );
        // Drive frames so the warm/crossfade advances into the new pool.
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || {
            for i in 0..2 {
                unsafe {
                    let _ = crate::handle::carapace_release_surface(handle, i);
                }
            }
            unsafe {
                crate::handle::test_support::iosurface_has_nonzero_pixels(new_surfaces[0], w2, h2)
                    || crate::handle::test_support::iosurface_has_nonzero_pixels(
                        new_surfaces[1],
                        w2,
                        h2,
                    )
            }
        });
        assert!(
            unsafe {
                crate::handle::test_support::iosurface_has_nonzero_pixels(new_surfaces[0], w2, h2)
                    || crate::handle::test_support::iosurface_has_nonzero_pixels(
                        new_surfaces[1],
                        w2,
                        h2,
                    )
            },
            "the new-size pool must receive a rendered frame"
        );

        // A bad dir → ErrBadSkin, existing pool/skin intact (still renders on the OLD pool is not
        // re-checked here; we only assert the error is synchronous and the handle survives).
        let bad = std::ffi::CString::new("/no/such/skin").unwrap();
        assert_eq!(
            unsafe {
                crate::handle::carapace_swap_skin_resized(
                    handle,
                    bad.as_ptr(),
                    refs.as_ptr(),
                    refs.len() as u32,
                    w2,
                    h2,
                    std::ptr::null(),
                )
            },
            crate::guard::CarapaceStatus::ErrBadSkin
        );
        unsafe { crate::handle::carapace_destroy(handle) };
    }
}

#[cfg(all(test, target_os = "macos"))]
mod pacing_tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    // Each test owns its OWN frame counter, handed to the render thread through the vtable `ctx`
    // pointer. A single shared `static` would be corrupted by cross-test interference: cargo runs
    // these tests in parallel, so `free_run`'s stream of frames would otherwise pollute `paused`'s
    // count. The counter is leaked to `'static` because the render thread holds `ctx` for the whole
    // handle lifetime.
    extern "C" fn count_ready(ctx: *mut std::ffi::c_void, _i: u32, _f: u64) {
        // SAFETY: `ctx` is the `&'static AtomicU32` each test passes in `make`.
        unsafe { (*(ctx as *const AtomicU32)).fetch_add(1, Ordering::SeqCst) };
    }

    fn make(counter: &'static AtomicU32) -> *mut crate::handle::CarapaceEngine {
        let vt = crate::host::CarapaceHostVTable {
            ctx: counter as *const AtomicU32 as *mut std::ffi::c_void,
            get_num: None,
            get_str: None,
            invoke: None,
            frame_ready: Some(count_ready),
            row_count: None,
            get_row_str: None,
            get_row_num: None,
            invoke_arg: None,
        };
        let (h, _s) = crate::handle::test_support::create_test_handle_pool_vt(300, 140, 3, vt);
        h
    }

    #[test]
    fn free_run_at_60_produces_many_frames_in_300ms() {
        let count: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        let h = make(count); // default fps = 60, running immediately
        // Poll-release all indices periodically so the loop never backpressures for long, and keep
        // going until we've observed enough paced frames OR a generous ceiling elapses. The first
        // frame pays a one-time vello pipeline-compile cost (can be hundreds of ms under parallel
        // GPU-test load), so a fixed 300ms window is flaky; a deadline that stops early once the
        // target is met keeps the fast path fast while tolerating a slow cold start.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while count.load(Ordering::SeqCst) < 5 && std::time::Instant::now() < deadline {
            for i in 0..3 {
                unsafe {
                    let _ = crate::handle::carapace_release_surface(h, i);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let n = count.load(Ordering::SeqCst);
        assert!(
            n >= 5,
            "expected several frames while free-running at 60fps, got {n}"
        );
        unsafe { crate::handle::carapace_destroy(h) };
    }

    #[test]
    fn paused_engine_renders_only_on_invalidate() {
        let count: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        let h = make(count);
        // Pause immediately. This is sent within microseconds of `create` returning, well inside
        // the loop's first ~16ms frame interval, so it takes effect before any paced frame renders
        // (the pool's surfaces stay free for the single invalidate below).
        unsafe {
            let _ = crate::handle::carapace_set_frame_rate(h, 0);
        }
        // Paused: no frames should appear on their own.
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "paused: no frames without invalidate"
        );
        unsafe {
            let _ = crate::handle::carapace_invalidate(h);
        }
        // Wait for the single invalidate-driven frame. Like the first frame anywhere, it can pay a
        // one-time GPU pipeline-compile cost, so poll up to a generous ceiling rather than a fixed
        // short sleep. A paused engine cannot emit a second frame, so the exact-count check holds.
        crate::handle::test_support::wait_for(std::time::Duration::from_secs(10), || {
            count.load(Ordering::SeqCst) >= 1
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        unsafe { crate::handle::carapace_destroy(h) };
    }

    #[test]
    fn crossfade_auto_advances_while_paused() {
        let count: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        let h = make(count); // default fps
        unsafe {
            assert_eq!(
                crate::handle::carapace_set_frame_rate(h, 0),
                crate::guard::CarapaceStatus::Ok
            )
        };
        // Let any startup frames settle, then zero the baseline so we count only swap-driven frames.
        std::thread::sleep(std::time::Duration::from_millis(80));
        for i in 0..3 {
            unsafe {
                let _ = crate::handle::carapace_release_surface(h, i);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
        count.store(0, Ordering::SeqCst);

        // Swap classic -> minimal: absent [transition] → default crossfade (250 ms).
        let dir = std::ffi::CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../carapace-demo/skins/minimal"
        ))
        .unwrap();
        unsafe {
            assert_eq!(
                crate::handle::carapace_swap_skin(h, dir.as_ptr()),
                crate::guard::CarapaceStatus::Ok
            )
        };

        // Release surfaces continuously so the auto-advancing crossfade never backpressures.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline && count.load(Ordering::SeqCst) < 3 {
            for i in 0..3 {
                unsafe {
                    let _ = crate::handle::carapace_release_surface(h, i);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        let n = count.load(Ordering::SeqCst);
        assert!(
            n >= 3,
            "crossfade should auto-produce frames while paused (old skin keeps animating), got {n}"
        );
        unsafe { crate::handle::carapace_destroy(h) };
    }

    #[test]
    fn cut_swap_promotes_without_crossfade_burst() {
        let count: &'static AtomicU32 = Box::leak(Box::new(AtomicU32::new(0)));
        let h = make(count);
        unsafe {
            assert_eq!(
                crate::handle::carapace_set_frame_rate(h, 0),
                crate::guard::CarapaceStatus::Ok
            )
        };
        std::thread::sleep(std::time::Duration::from_millis(80));
        for i in 0..3 {
            unsafe {
                let _ = crate::handle::carapace_release_surface(h, i);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
        count.store(0, Ordering::SeqCst);

        // Swap to the `cut` fixture: promotes in one warming frame, no crossfade.
        let dir = std::ffi::CString::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/skins/cut"))
            .unwrap();
        unsafe {
            assert_eq!(
                crate::handle::carapace_swap_skin(h, dir.as_ptr()),
                crate::guard::CarapaceStatus::Ok
            )
        };

        // Keep releasing for well past a crossfade's worth of time; a cut must NOT spawn a burst.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < deadline {
            for i in 0..3 {
                unsafe {
                    let _ = crate::handle::carapace_release_surface(h, i);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        let n = count.load(Ordering::SeqCst);
        assert!(
            n <= 2,
            "cut swap promotes in one frame — no crossfade burst; got {n}"
        );
        unsafe { crate::handle::carapace_destroy(h) };
    }
}

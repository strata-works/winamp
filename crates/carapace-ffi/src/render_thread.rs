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
use std::time::Instant;

use carapace::engine::Engine;
use carapace::render::Renderer;

use crate::guard::{CarapaceStatus, set_last_error};
use crate::handle::ContentTex;
use crate::host::{CarapaceHostVTable, FfiHost};
use crate::queue::{Command, CommandRx};
use crate::render::{GpuCtx, IOSurfaceRef, Present, Tier, build_content, build_present, init_gpu};
use crate::snapshot::SnapshotCell;

/// The raw host-owned pointers that must cross onto the spawned render thread at construction:
/// the IOSurface pool, the optional content surface, and the host vtable (fn pointers + its opaque
/// `ctx`). Bundling them keeps a SINGLE `Send` assertion in the crate.
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
    })
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

/// Skeleton loop: block for a command, handle Shutdown, ignore the rest for now (pacing/present land
/// in Task 6/5). Task 6 replaces this with `recv_timeout`-based free-run pacing.
fn run_loop(
    _rt: &mut RenderThread,
    rx: &CommandRx,
    _cell: &SnapshotCell,
    _poisoned: &Arc<AtomicBool>,
) {
    while let Ok(cmd) = rx.recv() {
        if matches!(cmd, Command::Shutdown) {
            break;
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

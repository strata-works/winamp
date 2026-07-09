//! The render thread's command channel. Host API calls enqueue `Command`s; the render loop drains
//! them each frame. Host-portable (no GPU): kept ungated so its logic is unit-tested on all CI.

#![allow(dead_code)]

use std::ffi::c_void;
use std::path::PathBuf;

use crate::guard::CarapaceStatus;

/// A host-owned surface pool crossing to the render thread for a resized swap. Same Send contract
/// as `render_thread::SendSurfaces`: the pointers are caller-owned, valid for their `w`×`h` size,
/// and outlive the engine until the next swap/destroy. Only the render thread touches them.
pub struct SendPool {
    /// The new pooled BGRA IOSurfaces (raw, caller-owned).
    pub surfaces: Vec<*const c_void>,
    /// The new content IOSurface for a `view{}` cutout, or null.
    pub content: *const c_void,
}
// SAFETY: opaque host memory only touched by the render thread after the move; see contract above.
unsafe impl Send for SendPool {}

/// A single host-owned content surface crossing to the render thread. Same Send contract as
/// `SendPool`: opaque host memory, only the render thread touches it after the move.
pub struct SendSurface(pub *const c_void);
// SAFETY: opaque host memory only touched by the render thread after the move.
unsafe impl Send for SendSurface {}

/// Pointer event kind, mirrored 1:1 by the C `CarapacePointerKind` (see `handle.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Press,
    Release,
    Move,
    Enter,
    Leave,
}

/// A message from a host API call to the render thread. Additive: append new variants.
pub enum Command {
    Pointer {
        x: f64,
        y: f64,
        kind: PointerKind,
    },
    /// Render exactly one frame now (wakes a paused engine).
    Invalidate,
    /// Set the free-run target frame rate; 0 = paused (render only on Invalidate/Pointer).
    SetFrameRate(u32),
    /// Host is done displaying `surfaces[index]`; it may be rendered into again.
    ReleaseSurface(u32),
    /// Stop the loop and let the thread exit (sent by `carapace_destroy`).
    Shutdown,
    /// Load the skin at `dir` and swap it in on the render thread, keeping the host. `reply`
    /// receives the outcome so `carapace_swap_skin` can report `ErrBadSkin` synchronously.
    SwapSkin {
        dir: std::path::PathBuf,
        reply: std::sync::mpsc::Sender<CarapaceStatus>,
    },
    /// Load the skin at `dir` and swap it in on the render thread, replacing the surface pool with
    /// `pool` at the new `w`×`h` size (native-size swap). `reply` reports `ErrBadSkin` synchronously.
    SwapSkinResized {
        /// The incoming skin's directory.
        dir: PathBuf,
        /// The new host-owned surface pool, at `w`×`h` size.
        pool: SendPool,
        /// The new pool's width.
        w: u32,
        /// The new pool's height.
        h: u32,
        /// Reports the outcome synchronously to the calling `carapace_swap_skin_resized`.
        reply: std::sync::mpsc::Sender<CarapaceStatus>,
    },
    /// Attach/replace (`surface` non-null) or clear (`surface` null) the content for `view_id`.
    /// Blocking via `reply` (mirrors SwapSkinResized) so the host can free a replaced/cleared surface.
    SetContent {
        view_id: String,
        surface: SendSurface,
        reply: std::sync::mpsc::Sender<CarapaceStatus>,
    },
    /// Test-only: forces a panic inside `render_guarded`'s `catch_unwind` on the render thread, to
    /// prove the panic→poison→`ErrPoisoned` contract end-to-end. Never compiled into a shipping
    /// build (and excluded defensively in `cbindgen.toml`, which can't see `#[cfg(test)]`).
    #[cfg(test)]
    ForcePanic,
}

pub type CommandTx = std::sync::mpsc::Sender<Command>;
pub type CommandRx = std::sync::mpsc::Receiver<Command>;

/// Push `first`, then drain everything currently queued into `out`, collapsing a run of consecutive
/// `Pointer{Move}` into only the most recent one (stale positions are worthless; the latest wins).
/// All other commands — and Moves separated by a non-Move — keep their order.
pub fn drain_coalescing(rx: &CommandRx, first: Command, out: &mut Vec<Command>) {
    push_coalesced(out, first);
    while let Ok(cmd) = rx.try_recv() {
        push_coalesced(out, cmd);
    }
}

fn is_move(c: &Command) -> bool {
    matches!(
        c,
        Command::Pointer {
            kind: PointerKind::Move,
            ..
        }
    )
}

fn push_coalesced(out: &mut Vec<Command>, cmd: Command) {
    if is_move(&cmd) && out.last().is_some_and(is_move) {
        *out.last_mut().unwrap() = cmd; // replace the previous trailing Move
    } else {
        out.push(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn mv(x: f64) -> Command {
        Command::Pointer {
            x,
            y: 0.0,
            kind: PointerKind::Move,
        }
    }

    #[test]
    fn coalesces_consecutive_moves_keeping_the_latest() {
        let (tx, rx) = channel::<Command>();
        // queue: Move(1), Move(2), Press, Move(3) — drain starting from an initial Move(0)
        tx.send(mv(1.0)).unwrap();
        tx.send(mv(2.0)).unwrap();
        tx.send(Command::Pointer {
            x: 9.0,
            y: 9.0,
            kind: PointerKind::Press,
        })
        .unwrap();
        tx.send(mv(3.0)).unwrap();
        let mut out = Vec::new();
        drain_coalescing(&rx, mv(0.0), &mut out);
        // Expect: Move(2) [latest of the leading run 0,1,2], Press, Move(3)
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], Command::Pointer { x, kind: PointerKind::Move, .. } if x == 2.0));
        assert!(matches!(
            out[1],
            Command::Pointer {
                kind: PointerKind::Press,
                ..
            }
        ));
        assert!(matches!(out[2], Command::Pointer { x, kind: PointerKind::Move, .. } if x == 3.0));
    }

    #[test]
    fn preserves_non_move_order_and_shutdown() {
        let (tx, rx) = channel::<Command>();
        tx.send(Command::SetFrameRate(30)).unwrap();
        tx.send(Command::ReleaseSurface(1)).unwrap();
        tx.send(Command::Shutdown).unwrap();
        let mut out = Vec::new();
        drain_coalescing(&rx, Command::Invalidate, &mut out);
        assert!(matches!(out[0], Command::Invalidate));
        assert!(matches!(out[1], Command::SetFrameRate(30)));
        assert!(matches!(out[2], Command::ReleaseSurface(1)));
        assert!(matches!(out[3], Command::Shutdown));
    }

    #[test]
    fn swap_skin_is_preserved_in_order_and_not_coalesced() {
        let (tx, rx) = channel::<Command>();
        let (rtx, _rrx) = channel::<crate::guard::CarapaceStatus>();
        tx.send(Command::SwapSkin {
            dir: "/tmp/a".into(),
            reply: rtx.clone(),
        })
        .unwrap();
        tx.send(Command::Invalidate).unwrap();
        let mut out = Vec::new();
        drain_coalescing(&rx, Command::SetFrameRate(30), &mut out);
        assert!(matches!(out[0], Command::SetFrameRate(30)));
        assert!(matches!(out[1], Command::SwapSkin { .. }));
        assert!(matches!(out[2], Command::Invalidate));
    }
}

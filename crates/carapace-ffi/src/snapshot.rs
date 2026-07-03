//! The render thread's published, read-only view of the world. After each frame the loop calls
//! `publish`; the C query exports (`hit_test`, `active_tier`) read it on the CALLER's thread with a
//! short read-lock and no engine access — so classification is sub-millisecond and never blocks the
//! render thread. The snapshot is ≤1 frame stale, which is fine for chrome hit-testing.
//!
//! Host-portable (no GPU): `SnapshotTier` mirrors `render::Tier` so this module needs no Apple gate.

#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use carapace::scene::{HitKind, Pt, Scene};

/// Present tier, mirrored so `snapshot.rs` stays GPU-free. Maps to/from `render::Tier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotTier {
    Readback,
    Shared,
}

/// The latest laid-out scene + tier. `scene` is `None` until the first frame is published.
pub struct SceneSnapshot {
    pub scene: Option<Scene>,
    pub tier: SnapshotTier,
}

/// Shared, atomically-swappable snapshot. Readers take a read-lock and clone the inner `Arc` out
/// (cheap); the writer takes a write-lock only to swap the `Arc` (never holds it across a render).
pub type SnapshotCell = Arc<RwLock<Arc<SceneSnapshot>>>;

pub fn new_cell(initial_tier: SnapshotTier) -> SnapshotCell {
    Arc::new(RwLock::new(Arc::new(SceneSnapshot {
        scene: None,
        tier: initial_tier,
    })))
}

pub fn publish(cell: &SnapshotCell, scene: Scene, tier: SnapshotTier) {
    let next = Arc::new(SceneSnapshot {
        scene: Some(scene),
        tier,
    });
    // A poisoned lock (a reader panicked mid-read) must not wedge the render thread: recover it.
    let mut guard = cell.write().unwrap_or_else(|e| e.into_inner());
    *guard = next;
}

/// Swap only the tier, leaving `scene` as `None`. Used by `carapace_create` right after init to
/// seed the tier the render thread resolved to, before frame 1 has been published.
pub fn publish_tier_only(cell: &SnapshotCell, tier: SnapshotTier) {
    let next = Arc::new(SceneSnapshot { scene: None, tier });
    let mut guard = cell.write().unwrap_or_else(|e| e.into_inner());
    *guard = next;
}

fn load(cell: &SnapshotCell) -> Arc<SceneSnapshot> {
    cell.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn hit_kind_of(cell: &SnapshotCell, p: Pt) -> HitKind {
    match &load(cell).scene {
        Some(scene) => scene.hit_kind(p),
        None => HitKind::Passthrough,
    }
}

pub fn tier_of(cell: &SnapshotCell) -> SnapshotTier {
    load(cell).tier
}

#[cfg(test)]
mod tests {
    use super::*;
    use carapace::scene::{HitKind, Pt};

    #[test]
    fn before_first_publish_hit_is_passthrough_and_tier_is_initial() {
        let cell = new_cell(SnapshotTier::Shared);
        assert!(matches!(tier_of(&cell), SnapshotTier::Shared));
        let k = hit_kind_of(&cell, Pt { x: 5.0, y: 5.0 });
        assert!(matches!(k, HitKind::Passthrough));
    }

    #[test]
    fn publish_then_query_reads_the_published_scene() {
        let cell = new_cell(SnapshotTier::Readback);
        // An empty scene covers nothing → hit_kind is Passthrough everywhere, but tier updates.
        let scene = carapace::scene::Scene {
            nodes: Vec::new(),
            canvas: (100, 50),
        };
        publish(&cell, scene, SnapshotTier::Shared);
        assert!(matches!(tier_of(&cell), SnapshotTier::Shared));
        // reading does not panic and returns a defined classification
        let _ = hit_kind_of(&cell, Pt { x: 5.0, y: 5.0 });
    }

    #[test]
    fn publish_tier_only_swaps_tier_and_keeps_scene_none() {
        let cell = new_cell(SnapshotTier::Shared);
        publish_tier_only(&cell, SnapshotTier::Readback);
        assert!(matches!(tier_of(&cell), SnapshotTier::Readback));
        assert!(matches!(
            hit_kind_of(&cell, Pt { x: 0.0, y: 0.0 }),
            HitKind::Passthrough
        ));
    }

    #[test]
    fn snapshot_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SceneSnapshot>();
        assert_send_sync::<SnapshotCell>();
    }
}

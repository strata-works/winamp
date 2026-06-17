# Phase 1 — Lessons Learned

**Date:** 2026-06-17
**Status:** Output of the throwaway prototype (decision 7); input to the Phase 2 formal spec.
**Branch:** `phase1-prototype`. Prototype crate: `crates/proto` (disposable).

The prototype's job was to surface real problems in three risk areas before
formalizing the engine. These are the problems it surfaced. They are written to be
acted on in the Phase 2 spec and the Phase 3 core engine — **not** to defend the
prototype code, which is throwaway.

## Confirmed (the architecture held)

- **Capability sandbox is cheap in Lua (decision 8).** `mlua`'s `Chunk::set_environment`
  gives the skin a custom `_ENV` containing only `fill`/`region`/`value_fill`/`host`.
  Base globals (`io`/`os`/`require`) are simply absent — no allowlist enforcement code,
  no metatable tricks. The negative tests pass trivially. Decision 8 is low-cost; keep it.
- **State-survives-swap falls out of the architecture (decision 3).** Because the scene
  is rebuilt from the host on swap and nodes hold only binding *keys*, "state survives"
  needed no special preservation logic — the headless test passes by construction. The
  disposable-scene / state-outside-graph split is the right call; carry it to Phase 3.
- **Host-agnosticism works.** The same generic `Host` trait + `value_of` + action
  allowlist serve both the media and system-monitor hosts unmodified. No domain name
  leaked into the generic modules (verified in the final review). The engine genuinely
  carries zero domain knowledge.
- **`hittest` (Phase 0) drops in cleanly.** Skin-defined concave geometry (the
  `sysmon-dial` L-shaped hotspot) resolves correctly; the notch misses.

## Problems to fix in the real engine

1. **The Phase 0 single-region `Renderer` does not carry forward.** A real UI is a
   *multi-node* scene; the prototype had to build a `vello::Scene` directly and ignore
   `spike-render`'s single-region trait. Phase 3's `render` module must take a scene of
   many shapes from the start. (Already anticipated, now confirmed.)

2. **`value_fill` needs a real fill model, not a bbox bar.** The prototype fills the
   axis-aligned bounding box left→right by `value`, regardless of the path's shape or
   intended orientation (the "dial" skin is really a horizontal bar). The base
   vocabulary (Phase 5) needs value-driven fills with an explicit direction and the
   ability to fill the *actual* free-form region (clip to the path), not just its bbox.

3. **Geometry is declared twice.** A clickable, visible control currently needs both a
   `region{...}` (hotspot) and a separate `fill{...}` with the *same* path — the skin
   author repeats the geometry and they can silently drift. The real vocabulary should
   let one node be both drawn and hit-testable (shared geometry), or derive the hotspot
   from the drawn shape.

4. **Define the host-boundary re-entrancy/borrow discipline.** The prototype shares the
   host as `Rc<RefCell<Box<dyn Host>>>`; a skin action fires from inside Lua and calls
   `borrow_mut()`. It works only because no Rust borrow is held across the Lua call. A
   skin that triggers a render/state-read *during* an action, or an action that swaps the
   skin, would risk a borrow panic. Phase 3 needs an explicit, documented model for
   re-entrant host calls (e.g. command queue, or `&mut` threaded through the call), not
   ad-hoc `RefCell`.

5. **Hit-testing rebuilds geometry per click.** `Scene::hit` constructs a
   `hittest::Region` from raw points on every call. The real scene should cache the
   region (or an acceleration structure) alongside the node.

6. **No headless-GPU rendering path.** `Renderer::new()` hard-requires a wgpu adapter and
   panics without one. Phase 3 needs a CI story (software adapter / `lavapipe`, or a
   render-free test mode) so rendering logic can be tested without a GPU.

7. **External API churn is a real maintenance cost.** `wgpu`/`vello`/`winit`/`mlua` are
   all fast-moving; Phase 0 already hit 5+ `wgpu` breaks. Phase 2 should decide a version
   pinning + upgrade-cadence policy rather than tracking latest.

## Open — pending the live interactive run

The headless tests prove correctness; they do not prove *feel*. Confirm on a real run
(`cargo run -p proto`) and fold any findings back here before Phase 2:

- Does the skin swap (`Tab`) actually *look* seamless mid-playback, or is there a visible
  flash/reset when the scene is torn down and rebuilt?
- Does free-form hit-testing feel right at the edges of the concave hotspot
  (`sysmon-dial`), including near the notch?
- Is the per-frame full re-render (and per-frame `surface.resize`) smooth, or does it
  reveal a need for dirty-tracking / partial redraw earlier than Phase 3?
- Does clicking map accurately to the intended hotspot on a HiDPI display?

## Recommendation for Phase 2

Carry decisions 3 and 8 forward unchanged (validated). Spend the Phase 2 spec on: the
multi-node scene + render contract (problem 1), the richer value-fill / shared-geometry
vocabulary (problems 2–3), and — most importantly — the host-boundary re-entrancy model
(problem 4), which is the one genuinely unresolved architectural question the prototype
exposed.

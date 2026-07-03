# Scene node provenance + picking (engine change, 2026-07-03)

A small, focused addition to the `carapace` engine that lets an **authoring tool** map a rendered
scene node back to the **source line** of the `fill{}`/`text{}`/… call that produced it, and pick the
**topmost node under a point**. This is the foundation the `carapace-preview` property inspector +
write-back ("Plan B") needs and could not build from outside the crate.

## Why this must live in the engine

`carapace-preview` (Plan A, merged in #30) renders skins live and edits host data. Plan B adds a
**property inspector** (click a node → edit its literal props) and a **parameters panel**, both of
which rewrite `skin.lua` on disk. To rewrite the *right* literal, the tool must correlate a picked
scene node with the exact source call that emitted it.

The original `carapace-preview` design (`2026-07-01-carapace-preview-design.md`) assumed the tool
could capture this correlation **at runtime from outside the engine** via an mlua debug hook. A code
survey (2026-07-03) proved that is **impossible under the public API**:

- The `mlua::Lua` instance is created and held **privately** inside `script::load` /
  `LoadedSkin` (`script.rs:44`); it is never returned or exposed, so `lua.set_hook` is unreachable
  from a downstream crate.
- The Lua `debug` library is deliberately **absent from the sandbox** (`script.rs:210-223`), so even
  skin-side introspection is blocked.
- `Node` (`scene.rs:176-237`) carries **no source span, id, or provenance** on any variant, and
  vocab extension can't recover one — a `Primitive::build` never sees caller line info.
- The `Scene` has **no public point→node method**; `hit_any` only considers *interactive* kinds
  (Hotspot/List/Scrub) and returns an action, not a node.

The correlation data (which call site emitted which node) exists **only inside the engine at load
time**. So the engine must capture it and expose it. This change does exactly that, minimally.

## What we add (public API)

1. **`carapace::scene::Origin`** — per-node provenance:
   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
   pub struct Origin {
       /// 1-based line of the primitive call in the skin's entry Lua, if known.
       pub line: Option<u32>,
       /// Monotonic index of the primitive call that emitted this node. Nodes from the
       /// same call (e.g. a `fill{}` with `on_press` emits Fill + Hotspot) share it.
       /// `None` for engine-generated nodes (list rows, selection highlight).
       pub call: Option<u32>,
   }
   ```

2. **Load-time capture.** In `script::load`, each primitive constructor closure reads the Lua
   caller's current line via `lua.inspect_stack(1, |d| d.current_line())` and stamps one `Origin`
   per emitted node — parallel to the existing per-node `anchors` push. Collected into
   `LoadedSkin.origins: Vec<Origin>` (mirrors `LoadedSkin.anchors`).

3. **`Engine::scene_origins(&self) -> &[Origin]`** — the design-scene origins, parallel to
   `scene().nodes` (mirrors the existing `scene_anchors()`).

4. **`Engine::layout_with_origins(&self, w, h) -> (Scene, Vec<Origin>)`** — the resolved,
   list-expanded scene **plus** origins aligned 1:1 with the returned `scene.nodes`. `layout()` keeps
   its exact current signature/behavior (delegates to a shared internal helper). This is the method
   the inspector calls: pick an index into `scene.nodes`, look up `origins[index]`.

5. **`Scene::pick(&self, p: Pt) -> Option<usize>`** — the index of the **topmost** (last in
   z-order) node whose resolved bounding box contains `p`, skipping zero-area nodes (Text has no
   measured geometry, so text nodes are not pickable — a documented limitation). Reuses the engine's
   own bbox logic.

6. **`carapace::layout::node_bbox` made `pub`** — already computes a node's bbox for every kind
   (rect / path / hotspot-contour / text-point); exporting it lets the tool render selection
   outlines without re-deriving per-kind geometry.

## Design decision: parallel `Vec<Origin>`, not a `Node` field

Origins ride **alongside** nodes (like `anchors` already does), not inside each `Node` variant.

- `Node` literals are constructed all over `vocab.rs` (every primitive's `build()`), plus `scene.rs`,
  `layout.rs`, and tests. Adding a field to the 9-variant enum would touch dozens of sites.
- Only **three** functions construct or re-shape a whole node list: `script::load`,
  `layout::resolve_scene` (rebuilds 1:1, `layout.rs:201-225`), and `engine::expand_lists` (the only
  step that changes node count, `engine.rs:156-243`). A parallel vec is threaded through just these.
- This mirrors the **existing** `anchors` precedent exactly (a parallel `Vec` surfaced via
  `scene_anchors()`, not a `Scene` field), so `Scene`'s public shape is unchanged and the ~25
  `Scene { nodes, canvas }` literal sites across the workspace (incl. `carapace-ffi`, `embed-spike`)
  **do not churn**.

`anchors` is consumed *inside* `resolve_scene` and discarded; `origins` differ in that they must
**survive** `expand_lists` and be returned to the caller — hence the new `layout_with_origins`
method and an origin-aware `expand_lists`.

## How the consumer uses it (Plan B, informative)

- **Loop detection falls out for free.** A `fill{}` inside a `for` loop runs once per iteration:
  every emitted node gets the **same `line`** but a **distinct `call`** ordinal. The tool groups
  final nodes by `call`; if more than one `call` maps to the same source line, that line ran in a
  loop → its props are **read-only** ("from a loop"). A `fill{}` with `on_press` emits Fill+Hotspot
  sharing **one** `call` → correctly treated as a single editable primitive.
- **Editable iff** `origins[i].call` is unique for its source `line` **and** the static `full_moon`
  parse shows that call's field is a literal. Generated nodes (`call: None`) are read-only.
- Text nodes can't be picked (no bounds); the inspector reaches text props via the parameters panel
  or a future node list, not the click path. Documented.

## Verified facts this design is built on

- **mlua 0.11.6** (`Cargo.toml:11`, `lua54` + `vendored`): `Lua::inspect_stack(level: usize, f:
  impl FnOnce(&Debug) -> R) -> Option<R>` (`state.rs:917`); level 0 = current C function, level 1 =
  the Lua caller (the skin chunk). `Debug::current_line(&self) -> Option<usize>` (`debug.rs:143`)
  lazily calls `lua_getinfo(state, "l")`, so `inspect_stack(1, |d| d.current_line()).flatten()`
  yields the executing skin line. Deprecated alias: `curr_line()`.
- The constructor closure (`script.rs:133`) is `move |_, args: Table| { … }` — it currently ignores
  the Lua context arg; changing `_` → `lua` gives access to `inspect_stack`. Nodes are pushed in
  skin-execution order (`b.nodes.extend(nodes)`, `script.rs:146`), parallel to the anchors push
  (`script.rs:143-145`).
- `Scene { pub nodes: Vec<Node>, pub canvas: (u32,u32) }` (`scene.rs:239-243`). `Node` derives
  `Clone, Debug` only — no `Eq`/`Hash`/id (`scene.rs:175`).
- `resolve_scene(design: &Scene, anchors: &[Anchors], logical: (f32,f32)) -> Scene` (`layout.rs:201`)
  maps nodes 1:1 (enumerate/map/collect) — origin order is preserved by cloning the input origins.
- `expand_lists(scene: &mut Scene, host: &dyn Host)` (`engine.rs:156`) replaces each `List` with
  `List` + optional highlight `Fill` + N row `Text`s — the only count change. Made origin-aware:
  passthrough nodes keep their origin; generated nodes inherit the `List`'s `line` with `call: None`.
- `node_bbox(&Node) -> Option<Rect>` (`layout.rs:79`, currently private) already handles all kinds;
  `Rect`/`resolve_bbox`/`Anchors` are already `pub` (`layout.rs:6,70,16`). `Pt`/`ImageDest` are
  `pub` (`scene.rs:3,100`). `hittest::Region::contains` (`crates/hittest/src/lib.rs:22`) is `pub`.

## Non-goals

- No change to how skins are authored or rendered; no new Lua surface; `debug` stays out of the
  sandbox. Origins are metadata only — the renderer and hit-test ignore them.
- No stable cross-*edit* node identity (indices/ordinals are per-load); the tool re-picks after each
  reload. Fine for an interactive inspector.
- No column-level provenance from the runtime (Lua exposes only line). Exact literal **spans** for
  write-back come from the tool's static `full_moon` parse, correlated to the runtime line. Two
  primitive calls on the *same physical line* are indistinguishable at runtime — documented; skins
  place one primitive per line.

## Testing

- **Capture:** a fixture skin with `fill{}` on a known line → after load, `scene_origins()[0].line`
  equals that line; two fills on different lines get those lines; a `fill{}` with `on_press` yields
  two nodes sharing one `call` ordinal.
- **Loop:** a `for` loop emitting N fills → N nodes, same `line`, N distinct `call` ordinals.
- **Survives layout:** `layout_with_origins(w,h)` returns `origins.len() == scene.nodes.len()`; a
  `list{}` that expands to rows → the retained List keeps its origin, generated rows have
  `call: None` and the List's line.
- **Pick:** a two-node scene (small fill over a large fill) → `pick(p)` inside the small one returns
  the topmost index; a point in empty space returns `None`; a point over only a `Text` node returns
  `None` (unbounded).
- **Regression:** `layout()` output is byte-identical to before (delegation preserves behavior); all
  existing `carapace` tests pass unchanged; `carapace-ffi` snapshot tests unaffected.

## Deliverables

- The API above in `crates/carapace` (`scene.rs`, `script.rs`, `engine.rs`, `layout.rs`), zero
  behavior change to existing consumers.
- New tests as above. `cargo clippy -p carapace --all-targets -- -D warnings` + `cargo fmt` clean.
- Plan: `docs/superpowers/plans/2026-07-03-scene-node-provenance.md`.
- Unblocks: `docs/superpowers/plans/2026-07-03-carapace-preview-inspector.md` (Plan B).
</content>
</invoke>

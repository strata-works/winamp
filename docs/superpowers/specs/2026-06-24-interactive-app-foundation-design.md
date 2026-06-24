# Spec 2 — Interactive-App Foundation (design)

**Date:** 2026-06-24
**Status:** Approved, ready for implementation planning
**Builds on:** Spec 1 — Responsive frame skins (PR #18, `main` at 9928ea2); the live-host-view-region seam (PR #17).
**Roadmap:** Second of three sequenced demo specs (see `demo-apps-roadmap` memory). Spec 2's `list{}` is the shared dependency Spec 3's playlist will reuse.

## Goal

Turn the Spec-1 frame-skin app-shell — today a **mock** of hand-authored static text rows hosted in a `view{ id="app" }` cutout — into a **live, clickable, navigable, two-pane read-only file browser**.

Three pieces:

1. A dynamic **`list{}` vocab primitive** — the engine binds dynamic *values* today but the row *count* is frozen at parse time (the scene is a flat `Vec<Node>` built once). A file browser needs the count to change as you navigate. This is the deep engine change.
2. **Input routing into the `view{}` region** — the demo/embedder translates an outer click into the nested app engine's coordinate space and forwards it. The engine stays neutral: no new input primitive; it reuses the existing pointer entrypoint + host-action machinery.
3. A **`FileBrowserHost`** — a real read-only filesystem host, abstracted behind a tiny `FileSystem` trait so tests never touch disk.

### Out of scope (YAGNI guardrails)

- No scrolling / mouse-wheel input (rows clamp to the visible region).
- No persistent selection highlight (navigation only).
- No second `view{}` region — both panes are two `list{}` primitives inside **one** nested app engine hosted by the existing single `app` view.
- No file opening in external apps; no filesystem writes (read-only throughout).
- No audio — that is Spec 3.

## Architecture decisions

| Decision | Choice | Why |
|---|---|---|
| List data model | **Typed `rows()` host method** | Collections become a first-class host concept via a dedicated `Host::rows(collection) -> Vec<Row>`, leaving scalar `get()` untouched. Defaulted method → existing hosts compile unchanged. |
| Expansion mechanism | **Deferred `Node::List`, expanded at layout (Approach A)** | `Primitive::build()` is parse-time with no host access; `Engine::layout(w,h)` already runs every frame and resolves anchors. Layout is the natural seam. Render/compositing/golden snapshots stay unchanged. |
| Browser scope | **Two-pane (keep Spec-1 look)** | A live left shortcuts column + a live right directory listing, matching the existing mock visually. |
| Input routing | **Engine-internal hit + demo-side forwarding** | List row-hit arithmetic lives in the engine (generic, reuses `host.invoke`); the coordinate transform into the view region lives in the demo (the actual "routing"). Engine exposes no new input primitive. |

Rejected alternatives: indexed flat keys / collection `StateValue` variant (data model); eager expansion in `build()` / demo-patches-scene (expansion); single-pane / navigate+selection (scope). See the roadmap memory for fuller rationale.

## Component design

### 1. Host trait: collections become first-class

`crates/carapace/src/host.rs` gains a `Row` type and one **defaulted** method:

```rust
/// One row of a host collection: cells addressed by key.
/// BTreeMap keeps cell order deterministic for snapshot tests.
#[derive(Clone, Debug, Default)]
pub struct Row { pub cells: BTreeMap<String, StateValue> }

pub trait Host {
    // … existing methods unchanged …
    /// Host-provided collections for list{}. Default: no collections.
    fn rows(&self, _collection: &str) -> Vec<Row> { Vec::new() }
}
```

The default empty impl means `FixtureHost` and `DemoHost` need no changes. Per-row clicks reuse the **existing** `host.invoke(action, args)` path — the row index rides in as `Value::Num(i)`. No new action machinery.

### 2. `list{}` primitive + `Node::List`

A new `ListPrim` is registered in `VocabRegistry::base()`. It parses into a new `Node` variant:

```rust
Node::List {
    collection: String,        // which host collection to iterate
    region: ImageDest,         // origin + size; anchored like any element
    row_height: f32,
    on_select: Option<String>, // host action fired with the clicked row index
    template: RowTemplate,      // row-relative cell nodes, parsed ONCE
}
```

`RowTemplate` is a small set of row-relative visual nodes (initially `Text`, extensible to `ValueFill`) built once at parse time from the `template = { … }` Lua table. A `Text` template node uses `bind = "<cellkey>"`, which resolves against **the current row's cells** at expansion — distinct from `TextContent::Bound`, which reads global `get()`.

Lua authoring (declarative, no per-frame Lua execution, per the performance-priority memory):

```lua
list{
  collection = "entries", x = 132, y = 28, w = 320, h = 270, row_height = 20,
  anchor = { "left", "right", "top", "bottom" }, on_select = "open_entry",
  template = {
    text{ bind = "name", size = 13, x = 4,   y = 3, color = {r=200,g=210,b=225} },
    text{ bind = "size", size = 13, x = 316, y = 3, halign = "right", color = {r=150,g=160,b=175} },
  },
}
```

### 3. Expansion at layout (Approach A — the heart of the spec)

`Engine::layout(w,h)` already runs per frame and resolves anchors into a concrete `Scene`. For each `Node::List` it now also:

1. calls `host.rows(collection)`;
2. **clamps to visible rows**: `n = min(rows.len(), floor(region.h / row_height))` — bounds node count regardless of directory size, and makes the "no scrolling" decision concrete;
3. emits, per visible row `i in 0..n`, the template's visual nodes offset by `i × row_height` (within the anchored `region`), with each `bind` resolved to that row's cell value (a missing cell renders empty, not an error);
4. **retains a lightweight `Node::List`** (carrying the resolved `region` + current count `n`) in the output scene, for hit-testing.

The renderer treats `Node::List` as a no-op container (draws nothing itself). The emitted per-row nodes are ordinary `Node::Text`/`Node::ValueFill`, so **render, compositing, and the gadget-skin golden snapshots need no changes**.

### 4. Input routing

Two cleanly separated pieces.

**Engine-internal (generic, not file-browser-specific):**

```rust
impl Scene {
    /// Topmost list row under p: (on_select action, row index). None if no list/row hit.
    pub fn hit_row(&self, p: Pt) -> Option<(String, usize)> { /* scan Node::List, divide by row_height, clamp to count */ }
}
```

The engine's pointer handler tries polygon `hit()` first, then `hit_row()` → `host.invoke(action, [Value::Num(index as f64)])`. No per-frame handler registration, no handler-table growth, no Lua at fire time.

**Demo-side (the actual "routing into `view{}`"):** on an outer click, the demo:

1. resolves the outer scene at logical size and finds the `app` view rect via `scene.views()`;
2. tests containment;
3. translates `inner = outer_logical − view_rect.origin` (the shell reflows to the view's logical size, so the mapping is 1:1 — no scale factor);
4. calls the nested shell engine's existing `handle_pointer_resolved(view_w, view_h, inner_p, Press)`.

The engine exposes **no new input primitive** — exactly the neutrality the roadmap called for. The coordinate transform is a pure function (unit-testable).

> Note: the anchored-hotspot hit-test gap flagged as a "Spec-2 deferral" in the branch ledger was actually fixed in Spec 1 (`Engine::handle_pointer_resolved`, commit 08c90a0). Spec 2's routing therefore only forwards clicks **into** the nested view; the nested engine's own `handle_pointer_resolved` already hit-tests its resolved scene correctly.

### 5. `FileBrowserHost`

Filesystem access goes through a tiny trait so tests never hit disk:

```rust
pub trait FileSystem {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>>;
}
pub struct DirEntryInfo { pub name: String, pub is_dir: bool, pub size: u64 }

pub struct StdFs;        // real, read-only std::fs
pub struct MockFs { /* in-memory tree */ }  // tests
```

`FileBrowserHost`:

```rust
pub struct FileBrowserHost<F: FileSystem> {
    fs: F,
    root: PathBuf,          // sandbox boundary — never traverse above this
    current_dir: PathBuf,
    shortcuts: Vec<(String, PathBuf)>,
}
```

- `rows("shortcuts")` → configured bookmarks (e.g. Home, ~/Music, ~/Documents), each a row with a `label` cell.
- `rows("entries")` → `".."` (omitted when `current_dir == root`) followed by directories then files, sorted; each row has `name` + human-readable `size` cells (directories show no size or a `<dir>` marker).
- `invoke`:
  - `open_shortcut(i)` → set `current_dir` to bookmark `i`'s path (clamped within `root`).
  - `open_entry(i)` → if the row is a directory, `cd` into it; if `".."`, go up (never above `root`); files are no-ops (read-only).
- `get("current_path")` → the path string shown in the chrome's status/title line.

Read failures (permission denied, vanished dir) degrade to an empty listing rather than panicking.

### 6. Demo wiring

The inline `APP_SHELL` static-text skin in `crates/carapace-demo/src/main.rs` is replaced by a two-`list{}` file-browser skin (left = `shortcuts`, right = `entries`, plus a chrome path line bound to `current_path`) in **one** nested app engine hosted by the existing `view{ id="app" }`. The shell engine swaps `DemoHost` → `FileBrowserHost<StdFs>` rooted at a sensible demo directory (e.g. the user's home or the repo root).

## Testing strategy

- **Unit — `FileBrowserHost` (MockFs):** a fixed in-memory tree; assert `rows("entries")`/`rows("shortcuts")` content and ordering; assert `invoke(open_entry/open_shortcut)` navigation changes subsequent `rows()`; assert `..` never escapes `root`.
- **Unit — coordinate transform:** pure-function test mapping outer-logical points to inner view coords given a view rect (inside, on-edge, outside).
- **Integration — layout expansion:** build a skin with a `list{}`, an `Engine` with a mock host returning fixed rows; assert `layout()` expands to the expected count of visual nodes (and clamps to the visible region); assert `Scene::hit_row(p)` at chosen `y` values returns the correct `(action, index)`.
- **Snapshot — `scene.summary()`:** deterministic textual summary for `Node::List` and its expanded rows (BTreeMap cells keep ordering stable).
- **Regression — golden snapshots:** existing gadget-skin pixel goldens stay **byte-identical** (no render-path change); run the `gpu-tests` variant.
- **CI:** `cargo test` + `cargo fmt --check` + `cargo clippy -D warnings` (and the gpu-tests clippy variant) per the run-clippy-before-push memory.

## Files touched (anticipated)

- `crates/carapace/src/host.rs` — `Row`, `Host::rows()` default method.
- `crates/carapace/src/scene.rs` — `Node::List`, `RowTemplate`, `Scene::hit_row()`, `summary()` arm.
- `crates/carapace/src/vocab.rs` — `ListPrim` parse + registration; row-template parsing.
- `crates/carapace/src/layout.rs` — `Node::List` expansion in the resolve pass.
- `crates/carapace/src/render.rs` — `Node::List` treated as no-op container.
- `crates/carapace/src/engine.rs` — pointer handler tries `hit_row()` → `host.invoke`.
- `crates/carapace-demo/src/file_browser_host.rs` (new) — `FileSystem`, `StdFs`, `MockFs`, `FileBrowserHost`.
- `crates/carapace-demo/src/main.rs` — replace `APP_SHELL`; wire `FileBrowserHost`; outer→inner click forwarding.

## Open follow-ups (deferred, non-blocking)

Inherited from the branch ledger, not addressed here: cache per-frame decoded-image Blob (perf); double `engine.layout/frame` on the resizable path. Scrolling and selection highlight are explicit non-goals for this spec but natural extensions afterward.

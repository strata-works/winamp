# Headspace Music Player (Spec 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the Headspace gadget skin from a mock faceplate into a functioning music player — real audio playback (rodio), transport controls, click-to-seek, auto-advance, and a clickable playlist reusing Spec 2's `list{}`.

**Architecture:** A new `MusicPlayerHost` (replacing the mock `DemoHost`) drives audio through an `AudioBackend` trait — real `RodioBackend` (rodio owns the audio thread), mock `MockAudio` for tests. A new generic `scrub{}` engine primitive + `Scene::hit_scrub` gives click-to-seek the same way Spec 2's `list{}` + `hit_row` gave clickable lists. The demo's gadget render+pointer path is routed through `Engine::layout()` so list expansion and scrub hit-testing work in gadget skins (byte-identical for existing list/scrub-free skins).

**Tech Stack:** Rust workspace (`carapace` engine + `carapace-demo` binary), `mlua` (Lua skin DSL), `wgpu`/`vello` (render), **`rodio`** (audio — added in `carapace-demo` only). Tests use `cargo test`; audio logic is tested over an in-memory `MockAudio` (no hardware, no files).

## Global Constraints

- **Git identity:** commit as `Daniel Agbemava <danagbemava@gmail.com>` (`git -c user.name=... -c user.email=...`).
- **The `rodio` dependency MUST be added with `sfw`:** the first fetch runs as `sfw cargo add rodio …` (Socket Firewall), per project policy. `rodio` goes in `crates/carapace-demo/Cargo.toml` ONLY — never the `carapace` engine crate.
- **No new dependency in the `carapace` engine crate.** The `scrub{}` primitive is pure engine code.
- **CI gates (run before every commit that touches engine/demo code):**
  - `cargo fmt --all --check`
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`
  - `cargo test --locked --workspace`
  - GPU regression (golden snapshots): `cargo test --locked -p carapace --features gpu-tests --test render_offscreen`
- **Gadget-skin golden snapshots must stay byte-identical** — the gadget-path generalization is behavior-preserving for list/scrub-free skins; `gadget_path_still_uniform_scales` must keep passing.
- **Performance is first-class:** position comes from polling the backend in `tick()` (never faked); no per-frame Lua; rodio owns the audio thread (no hand-rolled playback thread).
- **Graceful, read-only:** audio errors log and stop; they never panic. No filesystem writes.

---

## File Structure

**Engine crate (`crates/carapace/`):**
- `src/scene.rs` — `Node::Scrub` variant; `Scene::hit_scrub()`; `summary()` arm (Tasks 1, 2).
- `src/vocab.rs` — `ScrubPrim` parse + registration (Task 1).
- `src/layout.rs` — `Node::Scrub` `node_bbox`/`transform_node` arms (Task 1).
- `src/render.rs` — `Node::Scrub` draw arm, proportional fill (Task 1).
- `src/engine.rs` — `handle_pointer_resolved` tries `hit_scrub` (Task 2).

**Demo crate (`crates/carapace-demo/`):**
- `src/audio.rs` (new) — `AudioBackend`, `AudioError`, `MockAudio` (Task 3); `RodioBackend` (Task 5).
- `src/music_player_host.rs` (new) — `Track`, `MusicPlayerHost` (Task 4).
- `src/main.rs` — gadget render/pointer through `layout()` (Task 6); media host = `MusicPlayerHost` (Task 7).
- `skins/reference/skin.lua` — `scrub{}`, `list{}` playlist, next/prev, `time` text (Task 8).
- `skins/reference/assets/audio/` (new) — bundled clips + `LICENSE` (Task 7).
- `Cargo.toml` — `rodio` (Task 5).
- `src/demo_host.rs` — retired once unused (Task 7).

**Docs:** `README.md` — `scrub{}` + music player + gadget-path list support (Task 9).

---

### Task 1: `scrub{}` primitive — `Node::Scrub`, parse, layout, render

Adds the new scrub node, its match arms (summary, layout bbox/transform, render proportional fill), the `ScrubPrim` parser, and base-vocab registration. No hit-testing yet (Task 2).

**Files:**
- Modify: `crates/carapace/src/scene.rs` (variant + `summary()` arm)
- Modify: `crates/carapace/src/layout.rs` (`node_bbox`, `transform_node` arms)
- Modify: `crates/carapace/src/render.rs` (`draw()` arm)
- Modify: `crates/carapace/src/vocab.rs` (`ScrubPrim` + register)
- Test: `crates/carapace/src/scene.rs` (summary), `crates/carapace/src/layout.rs` (anchor resolve), `crates/carapace/src/script.rs` (parse)

**Interfaces:**
- Produces: `Node::Scrub { region: ImageDest, value_key: String, direction: FillDir, color: Color, on_seek: String }`; a `scrub` Lua constructor building exactly one such node.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `crates/carapace/src/scene.rs`:

```rust
    #[test]
    fn summary_describes_scrub_nodes() {
        let scene = Scene {
            canvas: (300, 100),
            nodes: vec![Node::Scrub {
                region: ImageDest { x: 10.0, y: 20.0, w: 200.0, h: 12.0 },
                value_key: "position".to_string(),
                direction: FillDir::Right,
                color: Color { r: 1, g: 2, b: 3, a: 255 },
                on_seek: "seek".to_string(),
            }],
        };
        assert_eq!(scene.summary(), "canvas 300x100\nscrub value=position on_seek=seek");
    }
```

Add to the `#[cfg(test)] mod tests` in `crates/carapace/src/layout.rs`:

```rust
    #[test]
    fn scrub_region_stretches_under_full_anchors() {
        let design = Scene {
            canvas: (200, 100),
            nodes: vec![Node::Scrub {
                region: ImageDest { x: 10.0, y: 10.0, w: 180.0, h: 12.0 },
                value_key: "position".to_string(),
                direction: crate::scene::FillDir::Right,
                color: Color { r: 0, g: 0, b: 0, a: 255 },
                on_seek: "seek".to_string(),
            }],
        };
        let anchors = vec![Anchors { left: true, right: true, top: true, bottom: false, min: None }];
        let resolved = resolve_scene(&design, &anchors, (300.0, 100.0));
        match &resolved.nodes[0] {
            Node::Scrub { region, .. } => assert_eq!(region.w, 280.0, "w stretched by +100"),
            other => panic!("expected Scrub, got {other:?}"),
        }
    }
```

> Note: the layout test imports may need `use crate::scene::Color;` — add it to the `mod tests` `use` block if not already present (mirror the existing list anchor test).

Add to the `#[cfg(test)] mod tests` in `crates/carapace/src/script.rs`:

```rust
    #[test]
    fn scrub_prim_parses_region_value_and_on_seek() {
        use crate::scene::{FillDir, Node};
        let q = new_queue();
        let skin = load(
            &src("scrub{ x=10, y=20, w=200, h=12, value='position', on_seek='seek', color={r=1,g=2,b=3} }"),
            &FixtureHost::new(),
            Rc::new(VocabRegistry::base()),
            q,
        )
        .unwrap();
        assert_eq!(skin.scene.nodes.len(), 1);
        match &skin.scene.nodes[0] {
            Node::Scrub { region, value_key, direction, on_seek, .. } => {
                assert_eq!((region.x, region.y, region.w, region.h), (10.0, 20.0, 200.0, 12.0));
                assert_eq!(value_key, "position");
                assert_eq!(on_seek, "seek");
                assert!(matches!(direction, FillDir::Right));
            }
            other => panic!("expected Scrub, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p carapace --lib`
Expected: FAIL — `Node::Scrub` does not exist (compile error).

- [ ] **Step 3: Add the `Node::Scrub` variant**

In `crates/carapace/src/scene.rs`, add to `enum Node` (after the `List` variant):

```rust
    Scrub {
        region: ImageDest,
        value_key: String,
        direction: FillDir,
        color: Color,
        on_seek: String,
    },
```

Add the `Scene::summary()` arm (after the `Node::List` arm):

```rust
                Node::Scrub {
                    value_key, on_seek, ..
                } => format!("scrub value={value_key} on_seek={on_seek}"),
```

- [ ] **Step 4: Add the layout arms**

In `crates/carapace/src/layout.rs`, add a `node_bbox` arm (next to the `Node::List` arm):

```rust
        Node::Scrub { region, .. } => Some(Rect {
            x: region.x,
            y: region.y,
            w: region.w,
            h: region.h,
        }),
```

And a `transform_node` arm (inside `match &mut n`, next to the `Node::List` arm):

```rust
        Node::Scrub { region, .. } => {
            *region = ImageDest {
                x: to.x,
                y: to.y,
                w: to.w,
                h: to.h,
            };
        }
```

- [ ] **Step 5: Add the render arm**

In `crates/carapace/src/render.rs`, add a `draw()` arm (next to the `Node::ValueFill` arm). This mirrors `ValueFill` but fills a proportion of the `region` RECT (no path clip needed — a rect needs no path-intersection). Match the coordinate types used by the existing `ValueFill` arm (its `Rect::new` takes `f64`):

```rust
                Node::Scrub {
                    region,
                    value_key,
                    color,
                    direction,
                    ..
                } => {
                    use crate::scene::FillDir;
                    let v = value_of(&read_value, value_key);
                    let x0 = region.x as f64;
                    let y0 = region.y as f64;
                    let x1 = (region.x + region.w) as f64;
                    let y1 = (region.y + region.h) as f64;
                    let w = region.w as f64;
                    let h = region.h as f64;
                    let vf = v as f64;
                    let extent = match direction {
                        FillDir::Right => Rect::new(x0, y0, x0 + w * vf, y1),
                        FillDir::Left => Rect::new(x1 - w * vf, y0, x1, y1),
                        FillDir::Up => Rect::new(x0, y1 - h * vf, x1, y1),
                        FillDir::Down => Rect::new(x0, y0, x1, y0 + h * vf),
                    };
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &extent);
                }
```

> The helpers `value_of`, `vcolor`, `Rect`, `Fill`, `xform`, `vs` are all already in scope in this `match` (the `ValueFill` arm uses them). If `value_of` returns `f32`, the `v as f64` cast above is correct; if it returns `f64`, drop the `vf` cast and use `v` directly — match what the `ValueFill` arm does.

- [ ] **Step 6: Add `ScrubPrim` and register it**

In `crates/carapace/src/vocab.rs`, add (after `ValueFillPrim`, or near `ListPrim`):

```rust
struct ScrubPrim;
impl Primitive for ScrubPrim {
    fn id(&self) -> &str {
        "scrub"
    }
    fn build(&self, args: &Table, _ctx: &mut dyn BuildContext) -> Result<Vec<Node>, BuildError> {
        let x: f32 = args.get("x").map_err(|_| BuildError::MissingField("x"))?;
        let y: f32 = args.get("y").map_err(|_| BuildError::MissingField("y"))?;
        let w: f32 = args.get("w").map_err(|_| BuildError::MissingField("w"))?;
        let h: f32 = args.get("h").map_err(|_| BuildError::MissingField("h"))?;
        let value_key: String = args
            .get("value")
            .map_err(|_| BuildError::MissingField("value"))?;
        let on_seek: String = args
            .get("on_seek")
            .map_err(|_| BuildError::MissingField("on_seek"))?;
        Ok(vec![Node::Scrub {
            region: crate::scene::ImageDest { x, y, w, h },
            value_key,
            direction: parse_direction(args)?,
            color: parse_color(args)?,
            on_seek,
        }])
    }
}
```

Register in `VocabRegistry::base()` (after `ListPrim`):

```rust
        r.register(Box::new(ScrubPrim));
```

> `parse_direction` defaults to `FillDir::Right` when no `direction` is given (same as `ValueFillPrim`); `parse_color` reads the `color` table.

- [ ] **Step 7: Run tests; fix base-vocab count assertions**

Run: `cargo test -p carapace`
Expected: the three new tests PASS. A base-vocab COUNT assertion will now FAIL (base went 8 → 9). Update each failing count assertion from its current number to +1:
- `crates/carapace/src/vocab.rs` — the `base_registry_now_has_eight`-style test (rename to nine, value `9`).
- `crates/carapace/tests/frame_prim.rs` — same base-count assertion (→ `9`).
- `crates/carapace-demo/src/gauge.rs` and `crates/carapace-demo/src/transport.rs` — base+extension count assertions (`9` → `10`).

Re-run `cargo test --locked --workspace` until green.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src crates/carapace/tests crates/carapace-demo/src/gauge.rs crates/carapace-demo/src/transport.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): scrub{} primitive — Node::Scrub, parse, layout, proportional render"
```

---

### Task 2: `Scene::hit_scrub` + click-to-seek dispatch

**Files:**
- Modify: `crates/carapace/src/scene.rs` (`hit_scrub`)
- Modify: `crates/carapace/src/engine.rs` (`handle_pointer_resolved`)
- Test: `crates/carapace/src/scene.rs` (unit); `crates/carapace/tests/scrub_seek.rs` (new integration)

**Interfaces:**
- Consumes: `Node::Scrub` (Task 1).
- Produces: `Scene::hit_scrub(&self, p: Pt) -> Option<(String, f32)>` — topmost `Node::Scrub` under `p` as `(on_seek action, fraction 0..1)`; `handle_pointer_resolved` enqueues `Command::HostAction { action, args: [Value::Num(fraction)] }` when no hotspot and no list row is hit.

- [ ] **Step 1: Write the failing unit tests**

Add to `crates/carapace/src/scene.rs` tests:

```rust
    fn scrub_scene() -> Scene {
        Scene {
            canvas: (200, 50),
            nodes: vec![Node::Scrub {
                region: ImageDest { x: 0.0, y: 0.0, w: 100.0, h: 20.0 },
                value_key: "position".to_string(),
                direction: FillDir::Right,
                color: Color { r: 0, g: 0, b: 0, a: 255 },
                on_seek: "seek".to_string(),
            }],
        }
    }

    #[test]
    fn hit_scrub_maps_x_to_fraction() {
        let s = scrub_scene();
        assert_eq!(s.hit_scrub(Pt { x: 0.0, y: 10.0 }), Some(("seek".to_string(), 0.0)));
        assert_eq!(s.hit_scrub(Pt { x: 50.0, y: 10.0 }), Some(("seek".to_string(), 0.5)));
        assert_eq!(s.hit_scrub(Pt { x: 100.0, y: 10.0 }), Some(("seek".to_string(), 1.0)));
    }

    #[test]
    fn hit_scrub_misses_outside_region() {
        let s = scrub_scene();
        assert_eq!(s.hit_scrub(Pt { x: 50.0, y: 30.0 }), None, "below region");
        assert_eq!(s.hit_scrub(Pt { x: -1.0, y: 10.0 }), None, "left of region");
    }
```

- [ ] **Step 2: Run unit tests to verify they fail**

Run: `cargo test -p carapace --lib scene::tests::hit_scrub`
Expected: FAIL — `hit_scrub` not found (compile error).

- [ ] **Step 3: Implement `hit_scrub`**

In `crates/carapace/src/scene.rs`, add to `impl Scene` (next to `hit_row`):

```rust
    /// Topmost scrub bar under `p`: `(on_seek action, click fraction 0..1)`. Reverse order.
    pub fn hit_scrub(&self, p: Pt) -> Option<(String, f32)> {
        for node in self.nodes.iter().rev() {
            let Node::Scrub { region, on_seek, .. } = node else {
                continue;
            };
            if p.x < region.x
                || p.x > region.x + region.w
                || p.y < region.y
                || p.y > region.y + region.h
            {
                continue;
            }
            let frac = if region.w > 0.0 {
                ((p.x - region.x) / region.w).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Some((on_seek.clone(), frac));
        }
        None
    }
```

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cargo test -p carapace --lib scene::tests::hit_scrub`
Expected: PASS (2 tests).

- [ ] **Step 5: Write the failing integration test**

Create `crates/carapace/tests/scrub_seek.rs`:

```rust
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::{Engine, PointerEvent};
use carapace::host::{ActionSpec, Host, Value};
use carapace::scene::Pt;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;

struct SeekHost {
    last: Rc<RefCell<Option<(String, f64)>>>,
}
impl Host for SeekHost {
    fn name(&self) -> &str {
        "seek-test"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, _key: &str) -> Option<StateValue> {
        Some(StateValue::Scalar(0.0))
    }
    fn actions(&self) -> &[ActionSpec] {
        &[ActionSpec { name: "seek" }]
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        let n = match args.first() {
            Some(Value::Num(n)) => *n,
            _ => -1.0,
        };
        *self.last.borrow_mut() = Some((action.to_string(), n));
    }
}

const SKIN: &str =
    "scrub{ x=0, y=0, w=100, h=20, value='position', on_seek='seek', color={r=1,g=2,b=3} }";

#[test]
fn clicking_a_scrub_invokes_on_seek_with_fraction() {
    let last = Rc::new(RefCell::new(None));
    let mut engine = Engine::new(
        Box::new(SeekHost { last: last.clone() }),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 20)),
    )
    .unwrap();

    // Click at x=75 of a 100-wide bar → fraction 0.75.
    engine.handle_pointer_resolved(100.0, 20.0, Pt { x: 75.0, y: 10.0 }, PointerEvent::Press);
    engine.update(Duration::from_millis(0));

    assert_eq!(*last.borrow(), Some(("seek".to_string(), 0.75)));
}
```

- [ ] **Step 6: Run integration test to verify it fails**

Run: `cargo test -p carapace --test scrub_seek`
Expected: FAIL — `last` stays `None` (no scrub dispatch yet).

- [ ] **Step 7: Add scrub dispatch to `handle_pointer_resolved`**

In `crates/carapace/src/engine.rs`, replace the `hit_row` tail of `handle_pointer_resolved` so it early-returns after a row hit and then tries `hit_scrub`:

```rust
        if let Some((action, index)) = scene.hit_row(p) {
            self.queue.borrow_mut().push(Command::HostAction {
                action,
                args: vec![crate::host::Value::Num(index as f64)],
            });
            return;
        }
        if let Some((action, fraction)) = scene.hit_scrub(p) {
            self.queue.borrow_mut().push(Command::HostAction {
                action,
                args: vec![crate::host::Value::Num(fraction as f64)],
            });
        }
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p carapace --test scrub_seek`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace/src/scene.rs crates/carapace/src/engine.rs crates/carapace/tests/scrub_seek.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(engine): Scene::hit_scrub + click-to-seek dispatch with fraction"
```

---

### Task 3: `AudioBackend` trait + `AudioError` + `MockAudio`

**Files:**
- Create: `crates/carapace-demo/src/audio.rs`
- Modify: `crates/carapace-demo/src/main.rs` (register `mod audio;`)
- Test: `crates/carapace-demo/src/audio.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub enum AudioError { Open(String), Decode(String), Unsupported }`
  - `pub trait AudioBackend { fn play(&mut self, &Path) -> Result<(), AudioError>; fn set_paused(&mut self, bool); fn stop(&mut self); fn seek(&mut self, f32); fn position(&self) -> Duration; fn duration(&self) -> Option<Duration>; fn is_finished(&self) -> bool; }`
  - `pub struct NullAudio` — a no-op backend (graceful fallback when no audio device is available).
  - test-only `MockAudio` (shares an `Rc<RefCell<MockAudioState>>` so a test can drive position/finished) with `MockAudio::new() -> (Self, Rc<RefCell<MockAudioState>>)`.

- [ ] **Step 1: Write the failing test**

Create `crates/carapace-demo/src/audio.rs` with the test first:

```rust
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn mock_records_play_pause_seek_and_reports_state() {
        let (mut audio, state) = MockAudio::new();
        audio.play(&PathBuf::from("/tmp/a.wav")).unwrap();
        assert_eq!(state.borrow().last_played, Some(PathBuf::from("/tmp/a.wav")));

        audio.set_paused(true);
        assert!(state.borrow().paused);

        audio.seek(0.5);
        assert_eq!(state.borrow().last_seek, Some(0.5));

        // Test drives position + finished through the shared state.
        state.borrow_mut().position = Duration::from_secs(3);
        state.borrow_mut().duration = Some(Duration::from_secs(10));
        state.borrow_mut().finished = true;
        assert_eq!(audio.position(), Duration::from_secs(3));
        assert_eq!(audio.duration(), Some(Duration::from_secs(10)));
        assert!(audio.is_finished());

        audio.stop();
        assert!(state.borrow().stopped);
    }
}
```

- [ ] **Step 2: Register the module and run the test (fails)**

In `crates/carapace-demo/src/main.rs`, add near the other `mod` declarations:

```rust
#[allow(dead_code)] // AudioBackend/AudioError consumed by music_player_host (Task 4) + main wiring (Task 7).
mod audio;
```

Run: `cargo test -p carapace-demo --bin carapace-demo audio`
Expected: FAIL — `MockAudio`/`AudioBackend` undefined (compile error).

- [ ] **Step 3: Implement the trait + MockAudio**

Add to the top of `crates/carapace-demo/src/audio.rs` (above `mod tests`):

```rust
/// What can go wrong loading/decoding a track. Logged, never panics.
#[derive(Debug)]
pub enum AudioError {
    Open(String),
    Decode(String),
    Unsupported,
}

/// One audio output sink. Real impl wraps rodio; tests use MockAudio.
pub trait AudioBackend {
    /// Load `path` and begin playing it, replacing any current track.
    fn play(&mut self, path: &Path) -> Result<(), AudioError>;
    fn set_paused(&mut self, paused: bool);
    fn stop(&mut self);
    /// Seek to `fraction` (0..1) of the current track.
    fn seek(&mut self, fraction: f32);
    fn position(&self) -> Duration;
    fn duration(&self) -> Option<Duration>;
    /// The current source has played to its end.
    fn is_finished(&self) -> bool;
}

/// A no-op backend used when no audio device is available, so the demo never panics.
pub struct NullAudio;
impl AudioBackend for NullAudio {
    fn play(&mut self, _path: &Path) -> Result<(), AudioError> {
        Ok(())
    }
    fn set_paused(&mut self, _paused: bool) {}
    fn stop(&mut self) {}
    fn seek(&mut self, _fraction: f32) {}
    fn position(&self) -> Duration {
        Duration::ZERO
    }
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn is_finished(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MockAudioState {
    pub last_played: Option<std::path::PathBuf>,
    pub paused: bool,
    pub stopped: bool,
    pub last_seek: Option<f32>,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub finished: bool,
}

#[cfg(test)]
pub struct MockAudio {
    state: std::rc::Rc<std::cell::RefCell<MockAudioState>>,
}

#[cfg(test)]
impl MockAudio {
    /// Returns the backend and a handle to its shared state for the test to drive.
    pub fn new() -> (Self, std::rc::Rc<std::cell::RefCell<MockAudioState>>) {
        let state = std::rc::Rc::new(std::cell::RefCell::new(MockAudioState::default()));
        (Self { state: state.clone() }, state)
    }
}

#[cfg(test)]
impl AudioBackend for MockAudio {
    fn play(&mut self, path: &Path) -> Result<(), AudioError> {
        let mut s = self.state.borrow_mut();
        s.last_played = Some(path.to_path_buf());
        s.paused = false;
        s.stopped = false;
        s.finished = false;
        s.position = Duration::ZERO;
        Ok(())
    }
    fn set_paused(&mut self, paused: bool) {
        self.state.borrow_mut().paused = paused;
    }
    fn stop(&mut self) {
        self.state.borrow_mut().stopped = true;
    }
    fn seek(&mut self, fraction: f32) {
        self.state.borrow_mut().last_seek = Some(fraction);
    }
    fn position(&self) -> Duration {
        self.state.borrow().position
    }
    fn duration(&self) -> Option<Duration> {
        self.state.borrow().duration
    }
    fn is_finished(&self) -> bool {
        self.state.borrow().finished
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p carapace-demo --bin carapace-demo audio`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/audio.rs crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): AudioBackend trait + AudioError + in-memory MockAudio"
```

---

### Task 4: `MusicPlayerHost`

**Files:**
- Create: `crates/carapace-demo/src/music_player_host.rs`
- Modify: `crates/carapace-demo/src/main.rs` (register `mod music_player_host;`)
- Test: `crates/carapace-demo/src/music_player_host.rs` (`#[cfg(test)] mod tests`, using `MockAudio`)

**Interfaces:**
- Consumes: `AudioBackend`, `AudioError`, `MockAudio` (Task 3); `Host`, `Row`, `ActionSpec`, `Value`, `StateValue`; `WindowOutbox`, `WINDOW_ACTIONS`, `handle_window_action` (from `crate::window`).
- Produces: `pub struct Track { pub title: String, pub path: PathBuf, pub duration: Option<Duration> }`; `pub struct MusicPlayerHost` implementing `Host`. Actions: `toggle_play`, `stop`, `next`, `prev`, `seek`, `play_index` + window actions. State keys: `playing`, `position`, `track_title`, `time`. Collection: `rows("playlist")` (cells `now`, `title`, `duration`). `MusicPlayerHost::new(backend: Box<dyn AudioBackend>, playlist: Vec<Track>, window: WindowOutbox) -> Self`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/carapace-demo/src/music_player_host.rs` (the test module first; it uses `MockAudio` from `crate::audio`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::MockAudio;
    use carapace::host::Value;
    use carapace::state::StateValue;
    use std::path::PathBuf;
    use std::time::Duration;

    fn track(title: &str, secs: u64) -> Track {
        Track {
            title: title.to_string(),
            path: PathBuf::from(format!("/tmp/{title}.wav")),
            duration: Some(Duration::from_secs(secs)),
        }
    }

    fn host() -> (MusicPlayerHost, std::rc::Rc<std::cell::RefCell<crate::audio::MockAudioState>>) {
        let (mock, state) = MockAudio::new();
        let playlist = vec![track("one", 10), track("two", 20), track("three", 30)];
        (
            MusicPlayerHost::new(Box::new(mock), playlist, Default::default()),
            state,
        )
    }

    #[test]
    fn cold_toggle_play_loads_current_then_pauses_resumes() {
        let (mut h, state) = host();
        h.invoke("toggle_play", &[]);
        assert_eq!(h.get("playing"), Some(StateValue::Bool(true)));
        assert_eq!(state.borrow().last_played, Some(PathBuf::from("/tmp/one.wav")));
        h.invoke("toggle_play", &[]); // pause
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
        assert!(state.borrow().paused);
    }

    #[test]
    fn play_index_and_next_prev_navigate() {
        let (mut h, state) = host();
        h.invoke("play_index", &[Value::Num(1.0)]);
        assert_eq!(state.borrow().last_played, Some(PathBuf::from("/tmp/two.wav")));
        h.invoke("next", &[]);
        assert_eq!(state.borrow().last_played, Some(PathBuf::from("/tmp/three.wav")));
        h.invoke("next", &[]); // past the end → stop
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
        assert!(state.borrow().stopped);
        h.invoke("play_index", &[Value::Num(2.0)]);
        h.invoke("prev", &[]);
        assert_eq!(state.borrow().last_played, Some(PathBuf::from("/tmp/two.wav")));
    }

    #[test]
    fn seek_forwards_fraction_to_backend() {
        let (mut h, state) = host();
        h.invoke("seek", &[Value::Num(0.25)]);
        assert_eq!(state.borrow().last_seek, Some(0.25));
    }

    #[test]
    fn tick_auto_advances_when_finished() {
        let (mut h, state) = host();
        h.invoke("toggle_play", &[]); // play track 0
        state.borrow_mut().finished = true;
        h.tick(Duration::from_millis(16));
        assert_eq!(state.borrow().last_played, Some(PathBuf::from("/tmp/two.wav")), "advanced to track 1");
    }

    #[test]
    fn position_is_backend_fraction_and_rows_mark_current() {
        let (mut h, state) = host();
        h.invoke("play_index", &[Value::Num(1.0)]);
        state.borrow_mut().position = Duration::from_secs(5);
        state.borrow_mut().duration = Some(Duration::from_secs(20));
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.25)));

        let rows = h.rows("playlist");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].get("now"), Some(&StateValue::Str("▶".into())));
        assert_eq!(rows[0].get("now"), Some(&StateValue::Str("".into())));
        assert_eq!(rows[1].get("title"), Some(&StateValue::Str("two".into())));
        assert_eq!(rows[1].get("duration"), Some(&StateValue::Str("0:20".into())));
    }
}
```

- [ ] **Step 2: Register the module + run the tests (fail)**

In `crates/carapace-demo/src/main.rs`, add near the other `mod` declarations:

```rust
#[allow(dead_code)] // MusicPlayerHost wired as the media host in Task 7.
mod music_player_host;
```

Run: `cargo test -p carapace-demo --bin carapace-demo music_player_host`
Expected: FAIL — `MusicPlayerHost`/`Track` undefined (compile error).

- [ ] **Step 3: Implement `Track` + `MusicPlayerHost`**

Add to the top of `crates/carapace-demo/src/music_player_host.rs` (above `mod tests`):

```rust
use std::path::PathBuf;
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::state::StateValue;

use crate::audio::AudioBackend;
use crate::window::{WINDOW_ACTIONS, WindowOutbox, handle_window_action};

const DOMAIN_ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "toggle_play" },
    ActionSpec { name: "stop" },
    ActionSpec { name: "next" },
    ActionSpec { name: "prev" },
    ActionSpec { name: "seek" },
    ActionSpec { name: "play_index" },
];

pub struct Track {
    pub title: String,
    pub path: PathBuf,
    pub duration: Option<Duration>,
}

pub struct MusicPlayerHost {
    backend: Box<dyn AudioBackend>,
    playlist: Vec<Track>,
    current: usize,
    playing: bool,
    started: bool,
    window: WindowOutbox,
    actions: Vec<ActionSpec>,
}

fn fmt_mmss(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

impl MusicPlayerHost {
    pub fn new(backend: Box<dyn AudioBackend>, playlist: Vec<Track>, window: WindowOutbox) -> Self {
        let mut actions = DOMAIN_ACTIONS.to_vec();
        actions.extend_from_slice(WINDOW_ACTIONS);
        Self {
            backend,
            playlist,
            current: 0,
            playing: false,
            started: false,
            window,
            actions,
        }
    }

    fn load_current(&mut self) {
        let Some(track) = self.playlist.get(self.current) else {
            return;
        };
        match self.backend.play(&track.path) {
            Ok(()) => {
                self.started = true;
                self.playing = true;
            }
            Err(e) => {
                eprintln!("carapace-demo: audio error: {e:?}");
                self.playing = false;
            }
        }
    }
}

impl Host for MusicPlayerHost {
    fn name(&self) -> &str {
        "music-player"
    }

    fn tick(&mut self, _dt: Duration) {
        // Auto-advance when the current track finishes (rodio owns the audio thread; we poll).
        if self.playing && self.backend.is_finished() {
            self.invoke("next", &[]);
        }
    }

    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => {
                let pos = self.backend.position().as_secs_f32();
                let dur = self.backend.duration().map(|d| d.as_secs_f32()).unwrap_or(0.0);
                let frac = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) } else { 0.0 };
                Some(StateValue::Scalar(frac))
            }
            "track_title" => {
                let title = self.playlist.get(self.current).map(|t| t.title.as_str()).unwrap_or("");
                Some(StateValue::Str(title.into()))
            }
            "time" => {
                let pos = self.backend.position();
                let dur = self.backend.duration().unwrap_or(Duration::ZERO);
                Some(StateValue::Str(format!("{} / {}", fmt_mmss(pos), fmt_mmss(dur)).into()))
            }
            _ => None,
        }
    }

    fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }

    fn invoke(&mut self, action: &str, args: &[Value]) {
        if handle_window_action(action, &self.window) {
            return;
        }
        match action {
            "toggle_play" => {
                if !self.started {
                    self.load_current();
                } else {
                    self.playing = !self.playing;
                    self.backend.set_paused(!self.playing);
                }
            }
            "stop" => {
                self.backend.stop();
                self.playing = false;
                self.started = false;
            }
            "next" => {
                if self.current + 1 < self.playlist.len() {
                    self.current += 1;
                    self.load_current();
                } else {
                    self.backend.stop();
                    self.playing = false;
                    self.started = false;
                }
            }
            "prev" => {
                self.current = self.current.saturating_sub(1);
                self.load_current();
            }
            "play_index" => {
                if let Some(Value::Num(n)) = args.first() {
                    let i = *n as usize;
                    if i < self.playlist.len() {
                        self.current = i;
                        self.load_current();
                    }
                }
            }
            "seek" => {
                if let Some(Value::Num(f)) = args.first() {
                    self.backend.seek(*f as f32);
                }
            }
            _ => {}
        }
    }

    fn rows(&self, collection: &str) -> Vec<Row> {
        if collection != "playlist" {
            return Vec::new();
        }
        self.playlist
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let now = if i == self.current { "▶" } else { "" };
                let dur = t.duration.map(fmt_mmss).unwrap_or_else(|| "--:--".to_string());
                Row::new()
                    .set("now", StateValue::Str(now.into()))
                    .set("title", StateValue::Str(t.title.as_str().into()))
                    .set("duration", StateValue::Str(dur.as_str().into()))
            })
            .collect()
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p carapace-demo --bin carapace-demo music_player_host`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/music_player_host.rs crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): MusicPlayerHost — transport, playlist rows, auto-advance over AudioBackend"
```

---

### Task 5: `RodioBackend` (real audio) + `rodio` dependency

This binds the `AudioBackend` trait to rodio. It is **not** unit-tested (CI has no audio device); correctness is validated by the live demo (Task 9 smoke). Keep it thin.

**Files:**
- Modify: `crates/carapace-demo/Cargo.toml` (add `rodio`)
- Modify: `crates/carapace-demo/src/audio.rs` (add `RodioBackend`)

**Interfaces:**
- Consumes: `AudioBackend`, `AudioError` (Task 3).
- Produces: `pub struct RodioBackend` implementing `AudioBackend`, with `RodioBackend::new() -> Result<Self, AudioError>`.

- [ ] **Step 1: Add the dependency with `sfw`**

Run (the first fetch MUST go through Socket Firewall):

```bash
cd crates/carapace-demo && sfw cargo add rodio
```

Then check the resolved version and which Cargo features it exposes for decoders:

```bash
cargo tree -p rodio | head -3
# Inspect features for the bundled clip formats (wav is usually default; mp3/flac/vorbis may be feature-gated):
cargo info rodio 2>/dev/null | sed -n '/features/,/^$/p' || true
```

Enable the format features matching the bundled clips (Task 7). The bundled clips default to **WAV** (decoded by rodio with no extra feature). If you commit mp3/flac/vorbis clips instead, enable the matching features, e.g. `sfw cargo add rodio --features symphonia-mp3` (exact feature names per the resolved version's docs). Record the final `rodio` line in your report.

- [ ] **Step 2: Implement `RodioBackend`**

Add to `crates/carapace-demo/src/audio.rs` (NOT under `#[cfg(test)]` — this is the real impl). This reference targets the stable rodio 0.20.x API. **If `cargo add` resolved a version whose API differs (e.g. a newer `Player`/`DeviceSinkBuilder` API), adapt the method calls to that version's docs — the `AudioBackend` trait boundary and all the Task-4 logic are unaffected.** Build errors here are expected to be resolved by matching the resolved rodio version's API:

```rust
use std::fs::File;
use std::io::BufReader;

pub struct RodioBackend {
    // The OutputStream must be kept alive for audio to play.
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
    sink: rodio::Sink,
    duration: Option<Duration>,
}

impl RodioBackend {
    pub fn new() -> Result<Self, AudioError> {
        let (stream, handle) =
            rodio::OutputStream::try_default().map_err(|e| AudioError::Open(e.to_string()))?;
        let sink = rodio::Sink::try_new(&handle).map_err(|e| AudioError::Open(e.to_string()))?;
        Ok(Self { _stream: stream, handle, sink, duration: None })
    }
}

impl AudioBackend for RodioBackend {
    fn play(&mut self, path: &Path) -> Result<(), AudioError> {
        use rodio::Source;
        let file = File::open(path).map_err(|e| AudioError::Open(e.to_string()))?;
        let decoder =
            rodio::Decoder::new(BufReader::new(file)).map_err(|e| AudioError::Decode(e.to_string()))?;
        self.duration = decoder.total_duration();
        // Replace any current track: a fresh sink drops the old one's queue.
        self.sink = rodio::Sink::try_new(&self.handle).map_err(|e| AudioError::Open(e.to_string()))?;
        self.sink.append(decoder);
        self.sink.play();
        Ok(())
    }
    fn set_paused(&mut self, paused: bool) {
        if paused {
            self.sink.pause();
        } else {
            self.sink.play();
        }
    }
    fn stop(&mut self) {
        self.sink.stop();
        self.duration = None;
    }
    fn seek(&mut self, fraction: f32) {
        if let Some(dur) = self.duration {
            let target = dur.mul_f32(fraction.clamp(0.0, 1.0));
            let _ = self.sink.try_seek(target); // best-effort; some formats can't seek
        }
    }
    fn position(&self) -> Duration {
        self.sink.get_pos()
    }
    fn duration(&self) -> Option<Duration> {
        self.duration
    }
    fn is_finished(&self) -> bool {
        self.sink.empty()
    }
}
```

- [ ] **Step 3: Build + lint (no unit test for real audio)**

Run: `cargo build -p carapace-demo`
Expected: builds clean. If the rodio API differs from the reference above, adapt per Step 2's note until it builds.

Run: `cargo clippy --locked --workspace --all-targets -- -D warnings`
Expected: clean.

> `RodioBackend` is still unconstructed until Task 7, so it may be flagged dead. Keep it under the module's existing `#[allow(dead_code)]` (added in Task 3); do NOT remove that allow yet.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add crates/carapace-demo/Cargo.toml crates/carapace-demo/src/audio.rs Cargo.lock
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): RodioBackend — bind AudioBackend to rodio (real playback)"
```

---

### Task 6: Gadget-path generalization (render + pointer through `layout()`)

Routes the demo's gadget (non-resizable) render and pointer through `Engine::layout(canvas, canvas)` so `list{}` expansion and `scrub`/`row` hit-testing work in gadget skins. Behavior-preserving for existing list/scrub-free skins.

**Files:**
- Modify: `crates/carapace-demo/src/main.rs` (render scene selection ~line 639-645; gadget pointer branch ~line 742-748)

**Interfaces:** none new — internal wiring. Verified by the golden regression + existing tests.

- [ ] **Step 1: Route gadget RENDER through `layout()`**

In `crates/carapace-demo/src/main.rs`, replace the render scene-selection block (currently `let resolved; let scene = if self.meta.resizable { resolved = self.engine.layout(...); &resolved } else { self.engine.scene() };`) with:

```rust
                // Both archetypes resolve through layout(): frame skins to the logical window size,
                // gadget skins to their own canvas (identity for list/scrub-free skins, but enabling
                // list expansion + scrub/row hit geometry). Renderer still scales physical/canvas.
                let resolved = if self.meta.resizable {
                    self.engine.layout(logical.0, logical.1)
                } else {
                    let (cw, ch) = self.engine.scene().canvas;
                    self.engine.layout(cw as f32, ch as f32)
                };
                let scene = &resolved;
```

- [ ] **Step 2: Route gadget POINTER through `handle_pointer_resolved()`**

Replace the gadget `else` branch of the `MouseInput` handler (currently maps physical→canvas and calls `self.engine.handle_pointer(...)`) with:

```rust
                    } else {
                        // Gadget skin: map physical cursor to canvas coords, then resolve-hit so
                        // list rows + scrub bars are reachable (identity layout for plain skins).
                        let (cw, ch) = self.engine.scene().canvas;
                        let cx = (self.cursor.0 * cw as f64 / pw) as f32;
                        let cy = (self.cursor.1 * ch as f64 / ph) as f32;
                        self.engine.handle_pointer_resolved(
                            cw as f32,
                            ch as f32,
                            Pt { x: cx, y: cy },
                            PointerEvent::Press,
                        );
                    }
```

- [ ] **Step 3: Build + verify behavior-preserving (golden regression)**

Run: `cargo build -p carapace-demo` → clean.
Run: `cargo test --locked --workspace` → all pass.
Run: `cargo test --locked -p carapace --features gpu-tests --test render_offscreen`
Expected: PASS — **`gadget_path_still_uniform_scales` must stay green** (gadget skins render byte-identically; `layout(canvas,canvas)` with default anchors is an identity for non-list/non-scrub nodes, and `expand_lists` is a no-op without lists).

- [ ] **Step 4: Commit**

```bash
cargo fmt --all && cargo clippy --locked --workspace --all-targets -- -D warnings
git add crates/carapace-demo/src/main.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): route gadget render+pointer through layout() for list/scrub support"
```

---

### Task 7: Bundled clips + wire `MusicPlayerHost` as the media host (retire `DemoHost`)

**Files:**
- Create: `crates/carapace-demo/skins/reference/assets/audio/` (clips + `LICENSE`)
- Modify: `crates/carapace-demo/src/main.rs` (media host = `MusicPlayerHost`; drop the two `#[allow(dead_code)]`)
- Delete: `crates/carapace-demo/src/demo_host.rs` (and its `mod demo_host;` + imports) once unused

**Interfaces:**
- Consumes: `MusicPlayerHost::new`, `Track` (Task 4); `RodioBackend::new` (Task 5).

- [ ] **Step 1: Provide bundled audio clips**

Create `crates/carapace-demo/skins/reference/assets/audio/` with 2-3 short audio files and a `LICENSE` file naming each clip's source/license.

Preferred: genuinely CC0 / public-domain clips. If the implementation environment cannot fetch external audio, generate license-clean placeholder **WAV** files (self-generated tones are public domain; rodio decodes WAV with no extra feature) so the build and demo are functional, and report this as a concern so the controller can swap in real clips. A reliable generator (run once, then commit the resulting `.wav` files — do NOT commit the generator):

```bash
python3 - <<'PY'
import math, struct, wave, os
os.makedirs("crates/carapace-demo/skins/reference/assets/audio", exist_ok=True)
def tone(name, freq, secs=4):
    path=f"crates/carapace-demo/skins/reference/assets/audio/{name}"
    with wave.open(path,"w") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(22050)
        for i in range(int(22050*secs)):
            s=int(0.3*32767*math.sin(2*math.pi*freq*i/22050))
            w.writeframesraw(struct.pack("<h", s))
for n,f in [("track-01.wav",262),("track-02.wav",330),("track-03.wav",392)]:
    tone(n,f)
print("generated 3 wav clips")
PY
cat > crates/carapace-demo/skins/reference/assets/audio/LICENSE <<'EOF'
Bundled demo audio. The committed clips are self-generated sine tones,
which carry no copyright (public domain), standing in for CC0 music clips.
Replace with genuinely CC0 / public-domain music as desired.
EOF
```

Record in your report exactly which clips you committed (real CC0 vs generated placeholders).

- [ ] **Step 2: Wire `MusicPlayerHost` as the media host**

In `crates/carapace-demo/src/main.rs`, find where the media `Engine` is constructed with `DemoHost` (`Box::new(DemoHost::with_outbox(window_outbox.clone()))`). Replace the host with a `MusicPlayerHost` whose playlist points at the bundled clips. Build the playlist paths relative to the skin asset dir (use the existing `skin_root()`/asset-path helper the demo already uses to locate `skins/reference/assets`):

```rust
        let audio_dir = skin_root().join("skins/reference/assets/audio");
        let playlist = vec![
            music_player_host::Track {
                title: "Headspace — Track 01".to_string(),
                path: audio_dir.join("track-01.wav"),
                duration: None,
            },
            music_player_host::Track {
                title: "Headspace — Track 02".to_string(),
                path: audio_dir.join("track-02.wav"),
                duration: None,
            },
            music_player_host::Track {
                title: "Headspace — Track 03".to_string(),
                path: audio_dir.join("track-03.wav"),
                duration: None,
            },
        ];
        let backend: Box<dyn audio::AudioBackend> = match audio::RodioBackend::new() {
            Ok(b) => Box::new(b),
            Err(e) => {
                eprintln!("carapace-demo: no audio device ({e:?}); player is silent");
                Box::new(audio::NullAudio) // graceful: the demo renders, playback is a no-op
            }
        };
        let engine = Engine::new(
            Box::new(music_player_host::MusicPlayerHost::new(
                backend,
                playlist,
                window_outbox.clone(),
            )),
            demo_registry(),
            src,
        )
        .unwrap();
```

> Simplify the backend construction to match the demo's error-handling style; the key point is: construct a `RodioBackend`, build it into a `MusicPlayerHost` with the bundled playlist, and pass it to `Engine::new` in place of `DemoHost`. If `skin_root()` is named differently, use whatever helper resolves `CARGO_MANIFEST_DIR`/asset paths today.

Now remove the two `#[allow(dead_code)]` attributes from `mod audio;` and `mod music_player_host;` (their items are consumed now). Confirm clippy stays green.

- [ ] **Step 3: Retire `DemoHost`**

Remove `mod demo_host;`, its `use` imports, and delete `crates/carapace-demo/src/demo_host.rs`. If anything else still references `DemoHost` (e.g. the sysmon path uses a different host — verify), leave those untouched; only remove the now-dead media host. `cargo clippy -D warnings` will flag any straggler import.

- [ ] **Step 4: Build, test, runtime smoke**

Run: `cargo test --locked --workspace` → pass.
Run: `cargo clippy --locked --workspace --all-targets -- -D warnings` → clean.
Runtime smoke: `cargo run -p carapace-demo` → launches without panic; Tab to the Headspace skin; the play/pause button starts audio; the progress bar advances. (Audio output depends on the host machine; at minimum it must not panic.)

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/carapace-demo
git rm crates/carapace-demo/src/demo_host.rs
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): MusicPlayerHost is the media host with bundled clips; retire DemoHost"
```

---

### Task 8: Headspace skin — scrub bar, playlist, next/prev, time readout

**Files:**
- Modify: `crates/carapace-demo/skins/reference/skin.lua`

**Interfaces:**
- Consumes: `scrub{}` (Task 1), `list{}` (Spec 2), the `MusicPlayerHost` state keys (`position`, `time`, `track_title`) and actions (`seek`, `next`, `prev`, `play_index`), and `rows("playlist")` cells (`now`, `title`, `duration`) (Task 4).

- [ ] **Step 1: Replace the value_fill progress bar with a `scrub{}`**

In `crates/carapace-demo/skins/reference/skin.lua`, replace the existing `value_fill{ … value = "position" … }` seek bar with an interactive scrub at the same rect:

```lua
-- live, click-to-seek bar bound to position, over the bitmap's seek groove
scrub{ x = 78, y = 216, w = 186, h = 14, value = "position", on_seek = "seek",
       color = {r=120,g=230,b=80} }
```

- [ ] **Step 2: Add next/prev hotspots + a time readout + the playlist**

Add to `skin.lua` (place the playlist in the lower area of the faceplate; adjust the rect to fit the artwork — the canvas is 342×394). Use `region{}` hotspots for next/prev, a bound `text{}` for the time, and a `list{}` for the playlist:

```lua
-- previous / next track hotspots (positioned over the artwork's transport area; tune to taste)
region{ path = rect{x=218, y=24, w=24, h=24}, on_press = function() host.prev() end }
region{ path = rect{x=246, y=24, w=24, h=24}, on_press = function() host.next() end }

-- elapsed / total time readout
text{ value = "time", font = "vt323.ttf", size = 13, x = 78, y = 232,
      color = {r = 120, g = 230, b = 80} }

-- clickable playlist (host-driven rows); clicking a row plays that track
list{ collection = "playlist", x = 78, y = 250, w = 186, h = 120, row_height = 18,
      on_select = "play_index",
      template = {
        { bind = "now",      x = 0,  y = 1, size = 13, color = {r=120,g=230,b=80} },
        { bind = "title",    x = 14, y = 1, size = 13, color = {r=200,g=235,b=200} },
        { bind = "duration", right = 2, y = 1, size = 13, halign = "right", color = {r=120,g=180,b=120} },
      } }
```

> The exact rects (`x/y/w/h`) are art-dependent; choose values that sit on a readable part of the faceplate and don't collide with the existing controls/`view{ id="display" }`. If the faceplate has no clear room, it is acceptable to nudge positions — keep the canvas at 342×394 unless genuinely impossible, in which case note the change.

- [ ] **Step 3: Runtime smoke**

Run: `cargo run -p carapace-demo`
Tab to the Headspace skin and verify (visually): the playlist lists 3 tracks with the `▶` marker on the current one; clicking a row switches tracks; clicking the scrub bar seeks; next/prev change tracks; the time readout updates while playing. (No automated test — `list{}`/`scrub{}`/`MusicPlayerHost` are each unit/integration-tested; this is the visual integration.)

Run: `cargo test --locked --workspace` and `cargo clippy --locked --workspace --all-targets -- -D warnings` → still green (skin is data, but confirm nothing regressed).

- [ ] **Step 4: Commit**

```bash
git add crates/carapace-demo/skins/reference/skin.lua
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "feat(demo): Headspace skin — click-to-seek scrub, live playlist, next/prev, time"
```

---

### Task 9: README + final golden verification

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document `scrub{}` and the music player**

Add `scrub{}` to the base-primitives enumeration (base vocab is now **nine**: `fill`, `region`, `value_fill`, `image`, `frame`, `text`, `view`, `list`, `scrub`). Add a short `scrub{}` paragraph near the `list{}` docs:

```markdown
- `scrub{ x, y, w, h, value, on_seek, color, direction? }` — a click-to-seek progress bar. It
  renders a proportional fill from host state `value` (like `value_fill`) but is hittable:
  clicking it invokes the `on_seek` host action with the click's 0..1 fraction (via
  `Scene::hit_scrub`, the seek-bar analogue of `list{}`'s `hit_row`).
```

Update the Headspace/demo description to say it is now a **functioning music player**: real audio via rodio behind a mockable `AudioBackend`, a `list{}` playlist, click-to-seek, next/prev, and auto-advance — driven by `MusicPlayerHost`. Note that gadget skins now route through `layout()` so `list{}`/`scrub{}` work in them (gadget rendering stays pixel-identical). Keep limitations honest (no volume/shuffle/repeat, no drag-scrub, no playlist scrolling, bundled clips not a library scan).

- [ ] **Step 2: Run the full CI gate + GPU goldens**

```bash
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p carapace --features gpu-tests --test render_offscreen
```

Expected: all PASS — gadget goldens **byte-identical** (`gadget_path_still_uniform_scales`).

- [ ] **Step 3: Commit**

```bash
git add README.md
git -c user.name="Daniel Agbemava" -c user.email="danagbemava@gmail.com" commit -m "docs(readme): document scrub{} + Headspace music player + gadget list support"
```

---

## Self-Review

**Spec coverage:**
- rodio behind `AudioBackend` trait (mockable) → Tasks 3, 5. ✓
- `MusicPlayerHost` (play/pause/stop/next/prev/seek/play_index, auto-advance, playlist rows, position/time/title) → Task 4. ✓
- Bundled CC0 clips (with WAV fallback + LICENSE) → Task 7. ✓
- `scrub{}` primitive + `Node::Scrub` + render + parse + layout → Task 1. ✓
- `Scene::hit_scrub` + click-to-seek dispatch (engine neutral, reuses host-action queue) → Task 2. ✓
- Gadget-path generalization (list/scrub work in gadget skins; byte-identical goldens) → Task 6. ✓
- One engine, one host; `MusicPlayerHost` replaces `DemoHost` for media skins → Task 7. ✓
- Headspace skin: scrub, playlist, next/prev, time → Task 8. ✓
- README current in same PR → Task 9. ✓
- Out-of-scope (no volume/shuffle/repeat/drag-scrub/scroll/library-scan) → respected throughout.

**Placeholder scan:** No `TBD`/`TODO`/"add error handling"/"similar to Task N". The two legitimate external-dependency adaptations (rodio API version; clip sourcing) are explicitly bounded with reference code + a fallback, not open-ended placeholders. Every code step shows complete code.

**Type consistency:** `AudioBackend` method set (Task 3) consumed verbatim by `MusicPlayerHost` (Task 4) and `RodioBackend` (Task 5). `Track { title, path, duration }` consistent across Tasks 4, 7. `Node::Scrub { region, value_key, direction, color, on_seek }` consistent across Tasks 1, 2 (summary/layout/render/parse/hit). `Scene::hit_scrub -> Option<(String, f32)>` (Task 2) matches the dispatch. Host action names (`seek`, `next`, `prev`, `play_index`, `toggle_play`, `stop`) match the skin's `on_seek`/`on_select`/handlers (Task 8) and the `DOMAIN_ACTIONS` allowlist (Task 4). State keys (`position`, `time`, `track_title`, `playing`) match the skin bindings and the existing faceplate.

---

## Execution Handoff

Recommended: subagent-driven development (fresh subagent per task + two-stage review). Order: engine `scrub{}` (1–2) lands and is green before the demo audio stack (3–5), then the gadget-path generalization (6), host wiring + clips (7), the skin (8), and docs/verification (9). The rodio binding (5) and bundled clips (7) are the two tasks with bounded external-dependency adaptation — review their reports for which concrete choices were made.

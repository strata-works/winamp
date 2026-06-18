# CI + Behavioral Regression Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up GitHub Actions CI and a behavioral regression harness (golden snapshots + invariant/property tests) for the `carapace` engine, with performance benchmarks captured as a ready-to-run Phase 3b task.

**Architecture:** A single ubuntu CI job runs fmt + clippy + the full headless test suite. A domain-neutral `Scene::summary()` feeds an `insta` snapshot harness that pins the engine's behavior (scene + state trajectory) for curated scenarios; `proptest` invariant tests assert the Phase 2 contracts hold under randomized input. Perf benches (Criterion) are documented for 3b where render makes them meaningful.

**Tech Stack:** GitHub Actions; Rust stable; `insta` (snapshots), `proptest` (properties) as `carapace` dev-dependencies. (Criterion deferred to 3b.)

## Global Constraints

- Rust, **edition 2024**, stable. Work on branch `ci-and-regression-harness` (stacked on `phase3a-headless-core`; carapace must be present — it is, on this branch).
- Do **not** modify Phase 0/1 crates' logic (`hittest`, `spike-render`, `proto`). `cargo fmt --all` reformatting them is fine; logic changes are not.
- Engine carries zero domain knowledge: the harness uses the existing test-only `FixtureHost`/`OtherFixtureHost`; no media/sysmon names enter `carapace/src/` (a `Scene::summary` added to `scene.rs` must stay domain-neutral — it prints node kinds/colors/keys, never app concepts).
- CI must be **green on arrival:** establish a `cargo fmt --all` clean baseline before enabling the `--check` gate; scope the `-D warnings` clippy gate to the **keeper** crates (`carapace`, `hittest`) since the throwaway crates may carry warnings and are removed at end of Phase 3.
- Snapshots are committed; intentional behavior changes are blessed with `INSTA_UPDATE=always`; CI runs with no update env so drift fails.

---

### Task 1: CI workflow + fmt/clippy baseline

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `rustfmt.toml`
- Modify: (reformatting only) any files `cargo fmt --all` touches.

**Interfaces:** none (infra).

- [ ] **Step 1: Establish a clean fmt baseline**

Run: `cargo fmt --all`
Then check what changed: `git status --porcelain`
This reformats the workspace to rustfmt defaults so the CI `--check` gate passes. (Logic is unchanged; only formatting.)

- [ ] **Step 2: Add a pinned rustfmt config**

Create `rustfmt.toml` (defaults, pinned so local + CI agree):

```toml
edition = "2024"
```

- [ ] **Step 3: Verify the gates pass locally**

Run each and confirm success (these are exactly what CI will run):

```bash
cargo fmt --all --check
cargo clippy -p hittest -p carapace --all-targets -- -D warnings
cargo test --workspace
```

Expected: `fmt --check` clean (after Step 1); clippy 0 warnings on hittest+carapace; all workspace tests pass. If `fmt --check` reports diffs, re-run `cargo fmt --all` and re-stage.

- [ ] **Step 4: Add the CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        run: rustup toolchain install stable --profile minimal --component rustfmt,clippy
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-
      - name: Format
        run: cargo fmt --all --check
      - name: Clippy (keeper crates)
        run: cargo clippy -p hittest -p carapace --all-targets -- -D warnings
      - name: Test
        run: cargo test --workspace
```

> Note: the clippy gate is scoped to `hittest` + `carapace` (the keeper crates). When the throwaway `proto`/`spike-render` crates are removed at the end of Phase 3, widen this to `--workspace`. The 3b plan adds the GPU/render leg (software adapter or macOS/Metal matrix) for render tests.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "ci: add GitHub Actions workflow (fmt + clippy + test) and fmt baseline"
```

> CI itself can only be confirmed green once pushed (GitHub runs it on the PR). Step 3 having passed locally means the same commands will pass on the runner.

---

### Task 2: `Scene::summary()` — stable domain-neutral scene serialization

**Files:**
- Modify: `crates/carapace/src/scene.rs`

**Interfaces:**
- Produces: `impl Scene { pub fn summary(&self) -> String }` — one line per node, deterministic order, no `hittest::Region` internals.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/carapace/src/scene.rs`:

```rust
    #[test]
    fn summary_is_stable_and_domain_neutral() {
        let scene = Scene {
            canvas: (300, 120),
            nodes: vec![
                Node::Fill { path: vec![Pt { x: 0.0, y: 0.0 }], color: Color { r: 10, g: 20, b: 30 } },
                Node::Hotspot { region: region_of(&l_path()), on_press: 2 },
                Node::ValueFill {
                    path: vec![Pt { x: 0.0, y: 0.0 }],
                    value_key: "level".to_string(),
                    color: Color { r: 1, g: 2, b: 3 },
                },
            ],
        };
        let expected = "canvas 300x120\n\
                        fill rgb=10,20,30\n\
                        hotspot handler=2\n\
                        value_fill key=level rgb=1,2,3";
        assert_eq!(scene.summary(), expected);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p carapace --lib scene::tests::summary_is_stable_and_domain_neutral`
Expected: FAIL — `summary` not found.

- [ ] **Step 3: Implement `summary`**

Add to the `impl Scene` block in `crates/carapace/src/scene.rs` (alongside `hit`):

```rust
    /// A stable, domain-neutral textual summary of the scene, for snapshot tests.
    /// Prints node kinds + style + binding keys; never the raw hit-test geometry.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("canvas {}x{}", self.canvas.0, self.canvas.1)];
        for node in &self.nodes {
            lines.push(match node {
                Node::Fill { color, .. } => {
                    format!("fill rgb={},{},{}", color.r, color.g, color.b)
                }
                Node::Hotspot { on_press, .. } => format!("hotspot handler={}", on_press),
                Node::ValueFill { value_key, color, .. } => format!(
                    "value_fill key={} rgb={},{},{}",
                    value_key, color.r, color.g, color.b
                ),
            });
        }
        lines.join("\n")
    }
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p carapace --lib scene`
Expected: PASS (all scene tests, including the new one).

- [ ] **Step 5: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/src/scene.rs
git commit -m "feat(carapace): Scene::summary() for snapshot regression tests"
```

---

### Task 3: Golden-snapshot behavioral harness (`insta`)

**Files:**
- Modify: `crates/carapace/Cargo.toml` (add `insta` dev-dep)
- Create: `crates/carapace/tests/behavior_snapshots.rs`
- Create (generated, committed): `crates/carapace/tests/snapshots/*.snap`

**Interfaces:**
- Consumes: `carapace::engine::{Engine, PointerEvent}`, `carapace::command::{Command, SkinSource}`, `carapace::scene::Pt`, `carapace::vocab::VocabRegistry`, `carapace::fixture::{FixtureHost, OtherFixtureHost}`, `Scene::summary` (Task 2).

- [ ] **Step 1: Add insta**

Run: `cargo add insta --dev -p carapace`

- [ ] **Step 2: Write the harness + scenarios**

> Lifetime note: `Step::Cmd` holds an owned `Command` (which contains `Box<dyn Host>` for `SwitchHost`), so `trajectory` consumes `steps: Vec<Step>` by value (not `&[Step]`) — that's how commands get enqueued.

Create `crates/carapace/tests/behavior_snapshots.rs`:

```rust
use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::{FixtureHost, OtherFixtureHost};
use carapace::host::Host;
use carapace::scene::Pt;
use carapace::vocab::VocabRegistry;

enum Step {
    Click(f32, f32),
    Cmd(Command),
    Tick(u64), // milliseconds
}

fn src(s: &str) -> SkinSource {
    SkinSource { lua_src: s.to_string(), canvas: (200, 200) }
}

/// Run a scenario and return a full trajectory string: the scene summary + the
/// declared state keys, captured after each step.
fn trajectory(host: Box<dyn Host>, skin: &str, state_keys: &[&str], steps: Vec<Step>) -> String {
    let mut e = Engine::new(host, VocabRegistry::base(), src(skin)).unwrap();
    let snap = |e: &Engine| -> String {
        let states: Vec<String> =
            state_keys.iter().map(|k| format!("{}={:?}", k, e.state(k))).collect();
        format!("scene:\n{}\nstate: {}", e.scene().summary(), states.join(" "))
    };
    let mut out = format!("=== step 0 (initial) ===\n{}\n", snap(&e));
    for (i, step) in steps.into_iter().enumerate() {
        match step {
            Step::Click(x, y) => {
                e.handle_pointer(Pt { x, y }, PointerEvent::Press);
                e.update(Duration::ZERO);
            }
            Step::Cmd(cmd) => {
                e.handle_command(cmd);
                e.update(Duration::ZERO);
            }
            Step::Tick(ms) => e.update(Duration::from_millis(ms)),
        }
        out.push_str(&format!("=== step {} ===\n{}\n", i + 1, snap(&e)));
    }
    out
}

const TOGGLE_SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
    value_fill{ path={{x=0,y=120},{x=200,y=120},{x=200,y=140},{x=0,y=140}},
                value='level', color={r=1,g=2,b=3} }
"#;

#[test]
fn snapshot_click_then_tick() {
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "level"],
        vec![Step::Click(50.0, 50.0), Step::Tick(250), Step::Click(50.0, 50.0)],
    );
    insta::assert_snapshot!("click_then_tick", t);
}

#[test]
fn snapshot_swap_preserves_state() {
    let other = "value_fill{ path={{x=0,y=0},{x=200,y=0},{x=200,y=10}}, value='level', color={r=9,g=9,b=9} }";
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "level"],
        vec![
            Step::Click(50.0, 50.0),
            Step::Tick(300),
            Step::Cmd(Command::Swap(src(other))),
        ],
    );
    insta::assert_snapshot!("swap_preserves_state", t);
}

#[test]
fn snapshot_failed_swap_keeps_scene() {
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "level"],
        vec![Step::Cmd(Command::Swap(src("not lua {{{")))],
    );
    insta::assert_snapshot!("failed_swap_keeps_scene", t);
}

#[test]
fn snapshot_switch_host_resets() {
    let noop_skin = "region{ path={{x=0,y=0},{x=50,y=0},{x=50,y=50}}, on_press=function() host.noop() end }";
    let t = trajectory(
        Box::new(FixtureHost::new()),
        TOGGLE_SKIN,
        &["on", "flag"],
        vec![
            Step::Click(50.0, 50.0),
            Step::Cmd(Command::SwitchHost {
                host: Box::new(OtherFixtureHost::new()),
                skin: src(noop_skin),
            }),
            Step::Click(10.0, 10.0),
        ],
    );
    insta::assert_snapshot!("switch_host_resets", t);
}
```

- [ ] **Step 3: Generate the snapshots**

Run: `INSTA_UPDATE=always cargo test -p carapace --test behavior_snapshots`
Expected: PASS; creates `crates/carapace/tests/snapshots/behavior_snapshots__*.snap` (4 files).
Inspect them: `git status --porcelain crates/carapace/tests/snapshots` should list 4 new `.snap` files. Open one and sanity-check the trajectory reads sensibly (e.g. `on=Some(Bool(true))` after the first click drains).

- [ ] **Step 4: Confirm the snapshots are enforced (no update env)**

Run: `cargo test -p carapace --test behavior_snapshots`
Expected: PASS (compares against the committed snapshots, no env var).

- [ ] **Step 5: Confirm the harness actually catches drift**

Temporarily change `TOGGLE_SKIN`'s `value_fill` color from `{r=1,g=2,b=3}` to `{r=9,g=2,b=3}`. Run: `cargo test -p carapace --test behavior_snapshots`
Expected: FAIL — insta reports a snapshot mismatch (proving drift is caught). Then **revert** the color change and do NOT accept the `.snap.new` files (`rm -f crates/carapace/tests/snapshots/*.snap.new`). Re-run to confirm green.

- [ ] **Step 6: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/Cargo.toml crates/carapace/tests/behavior_snapshots.rs crates/carapace/tests/snapshots
git commit -m "test(carapace): insta behavioral snapshot regression harness"
```

---

### Task 4: Invariant / property tests (`proptest`)

**Files:**
- Modify: `crates/carapace/Cargo.toml` (add `proptest` dev-dep)
- Create: `crates/carapace/tests/invariants.rs`

**Interfaces:**
- Consumes: same engine/command/fixture surface as Task 3.

- [ ] **Step 1: Add proptest**

Run: `cargo add proptest --dev -p carapace`

- [ ] **Step 2: Write the invariant tests**

Create `crates/carapace/tests/invariants.rs`:

```rust
use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::scene::Pt;
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;
use proptest::prelude::*;

const SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
"#;

fn src(s: &str) -> SkinSource {
    SkinSource { lua_src: s.to_string(), canvas: (200, 200) }
}

fn engine() -> Engine {
    Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), src(SKIN)).unwrap()
}

#[derive(Clone, Debug)]
enum Op {
    Click(f32, f32),
    Tick(u64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0.0f32..200.0, 0.0f32..200.0).prop_map(|(x, y)| Op::Click(x, y)),
        (0u64..1000).prop_map(Op::Tick),
    ]
}

proptest! {
    // Invariant: no sequence of clicks/ticks ever panics.
    #[test]
    fn never_panics(ops in proptest::collection::vec(op_strategy(), 0..40)) {
        let mut e = engine();
        for op in ops {
            match op {
                Op::Click(x, y) => e.handle_pointer(Pt { x, y }, PointerEvent::Press),
                Op::Tick(ms) => e.update(Duration::from_millis(ms)),
            }
        }
        // also drain anything queued
        e.update(Duration::ZERO);
    }

    // Invariant: a click never mutates host state before the drain.
    #[test]
    fn no_mutation_before_drain(x in 0.0f32..200.0, y in 0.0f32..200.0) {
        let mut e = engine();
        let before = e.state("on");
        e.handle_pointer(Pt { x, y }, PointerEvent::Press); // NO update
        prop_assert_eq!(e.state("on"), before);
    }
}

// Invariant: a swap to a broken skin always leaves the prior scene intact (transactional).
#[test]
fn transactional_swap_invariant() {
    let mut e = engine();
    let before = e.scene().summary();
    for bad in ["not lua {{{", "frobnicate{}", "host.does_not_exist()", "io.read()"] {
        e.handle_command(Command::Swap(src(bad)));
        e.update(Duration::ZERO);
        assert_eq!(e.scene().summary(), before, "broken swap `{bad}` changed the scene");
    }
}

// Invariant: the sandbox blocks capability globals for any skin built through the engine.
#[test]
fn sandbox_blocks_capabilities_invariant() {
    for bad in ["io.write('x')", "os.time()", "require('os')", "load('return 1')"] {
        let r = Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), src(bad));
        assert!(r.is_err(), "sandbox failed to block `{bad}`");
    }
}

// Sanity: a real click DOES toggle after a drain (so the no-mutation test isn't vacuous).
#[test]
fn click_then_drain_toggles() {
    let mut e = engine();
    e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
    e.update(Duration::ZERO);
    assert_eq!(e.state("on"), Some(StateValue::Bool(true)));
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p carapace --test invariants`
Expected: PASS (proptest runs its default case count per property; the 3 plain invariant tests pass). If proptest finds a failing case it prints a minimized counterexample — that would be a real engine bug to investigate, not a test to weaken.

- [ ] **Step 4: Commit**

```bash
cargo fmt -p carapace
git add crates/carapace/Cargo.toml crates/carapace/tests/invariants.rs
git commit -m "test(carapace): proptest engine invariants (no-panic, drain timing, transactional swap, sandbox)"
```

---

### Task 5: Wire the harness into CI verification + final green check

**Files:** none new — a verification + doc-pointer task.

- [ ] **Step 1: Full local run exactly as CI will**

```bash
cargo fmt --all --check
cargo clippy -p hittest -p carapace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all green. The workspace test run now includes `behavior_snapshots` (4) + `invariants` (5) plus the 29 prior carapace tests and Phase 0/1 tests.

- [ ] **Step 2: Note the deferred 3b perf-bench task in the design doc**

Confirm `docs/superpowers/specs/2026-06-18-ci-and-regression-harness-design.md` §4 already records the Criterion bench task for 3b (it does). No code change; this step is a checkpoint that the deferral is documented, not dropped.

- [ ] **Step 3: Commit (if fmt produced any drift)**

```bash
cargo fmt --all
git add -A
git commit -m "chore: workspace fmt after harness" || echo "nothing to commit"
```

---

## Self-Review

**Spec coverage (against the CI+harness design):**
- §1 CI (ubuntu, fmt/clippy/test, push+PR) → Task 1. Clippy scoped to keepers per Global Constraints. ✓
- §2 golden snapshots (insta, scene+state trajectory, blessing, drift-catches) → Tasks 2 (summary) + 3 (harness, with the explicit drift-catch step). ✓
- §3 invariant/property tests (no-panic, mutation-only-at-drain, transactional swap, sandbox) → Task 4. ✓
- §4 perf benches deferred to 3b, structured → documented in the design doc; Task 5 Step 2 checkpoints it. ✓
- Domain-neutral summary; harness uses existing fixtures → Task 2 constraint + Task 3 imports. ✓
- fmt-clean baseline + green-on-arrival → Task 1 Steps 1/3, Task 5. ✓

**Placeholder scan:** none. `trajectory` is written once in its final by-value form (the lifetime note explains why it takes `Vec<Step>`, not a draft). Every code step is complete; no TBD/"add error handling".

**Type consistency:** `Step`/`Op`, `trajectory`, `src`, `engine` helpers are defined in their test files. `Engine::{new,handle_pointer,handle_command,update,scene,state}`, `Command::{Swap,SwitchHost}`, `SkinSource`, `Pt`, `VocabRegistry::base`, `FixtureHost`/`OtherFixtureHost`, `Scene::summary` all match the Phase 3a surfaces. `PointerEvent::Press` used consistently.

## Deferred to Phase 3b (do NOT execute now)

**Perf-regression benchmarks (Criterion).** When 3b lands `render`:
- Add `criterion` dev-dep + `crates/carapace/benches/engine.rs` with `[[bench]]` harness=false.
- Benchmark hot paths: `Scene::hit` over a many-node scene; the command **drain** (`update` with a full queue); **scene rebuild** (`Engine::swap` / skin load); and **render frame-time** (direct-to-surface) — the path behind the Phase 1 ~31fps finding.
- Add a CI bench job alongside the 3b GPU/render leg; use Criterion baselines to flag regressions.
- Widen the CI clippy gate to `--workspace` once `proto`/`spike-render` are removed at end of Phase 3.

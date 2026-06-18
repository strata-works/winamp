# CI + Behavioral Regression Harness — Design

**Date:** 2026-06-18
**Status:** Approved design, pre-implementation.
**Project:** carapace (repo codename `winamp`)
**Depends on:** Phase 3a (`crates/carapace`, currently in PR #4 — this work stacks on the
`phase3a-headless-core` branch).

## Purpose

Stand up continuous integration and a behavioral regression harness now that the engine
is real (not throwaway), so refactors and future phases can't silently change behavior.
Performance is a tracked first-class concern (see the performance memory / Phase 1
lessons), so a perf-regression harness is planned too — but **phased** to where each kind
of coverage earns its place.

Three kinds of regression coverage, added at different points:

| Coverage | When | Why then |
|----------|------|----------|
| Golden / snapshot behavioral tests | **now** | engine is deterministic + headless; ready |
| Invariant / property tests | **now** | headless; locks the Phase 2 contracts under fuzzing |
| Performance benchmarks | **Phase 3b** | render makes frame-time meaningful; the ~31fps finding becomes addressable |

## Branching

This work depends on `carapace`, which is in PR #4 (not yet on `main`). It is built on a
branch stacked on `phase3a-headless-core`; its PR is based on `phase3a-headless-core` (so
the diff is only the CI + harness files) and retargeted to `main` once PR #4 merges.
Adding CI here also means CI runs on PR #4 itself.

## 1. CI (now)

`.github/workflows/ci.yml`, triggered on push to `main` and on pull requests. A single
`ubuntu-latest` job, Rust stable, with cargo caching:

- **format:** `cargo fmt --all --check`
- **lint:** `cargo clippy --all-targets --workspace -- -D warnings`
- **test:** `cargo test --workspace` (the full headless suite, including the harness below)

Notes:
- Vendored Lua (`mlua` `vendored`) compiles via the runner's default C toolchain — no extra
  system packages needed.
- First run builds vendored Lua + the dep tree (slow); cargo caching makes subsequent runs
  fast.
- **GPU/render leg is deferred to Phase 3b.** When `render` tests land they need a GPU; CI
  runners have none by default. The 3b plan adds either a software-adapter job
  (`lavapipe`/`llvmpipe` via Mesa on the ubuntu runner) or a macOS/Metal matrix leg. This
  is recorded as a 3b follow-up so it is not forgotten.
- A repo `rustfmt.toml` (defaults) and the existing clippy-clean state make the format/lint
  gates pass from day one.

## 2. Golden / snapshot behavioral harness (now)

Uses **`insta`** (standard Rust snapshot crate; dev-dependency of `carapace`).

A `scenarios` set, each entry `(name, skin_lua, host, &[Step])` where:

```
enum Step { Click(Pt), Command(Command), Tick(Duration) }
```

The harness runs each scenario on a real `Engine` headlessly and, **after each step**,
captures a stable, domain-neutral textual summary into a committed `.snap` file:

- **scene summary:** `canvas` + per-node `{ kind, color, value_key, is_hotspot }`. It does
  **not** serialize the raw `hittest::Region` (not cleanly serializable and noisy); a
  summary keeps snapshots readable and stable. A small `Scene::summary() -> String` (or a
  free function in the harness) produces this.
- **state:** the scenario's declared keys (e.g. `on`, `level`, `flag`) and their values.

Behavior: CI re-runs the scenarios; any drift fails the build. Intentional changes are
blessed with `cargo insta review` / `INSTA_UPDATE=always`. Committed snapshots double as
living behavior documentation.

Initial scenarios cover the spine: a click toggling state at the drain; a chained-actions
handler; a skin swap preserving state; a failed swap keeping the prior scene; a host
switch resetting state.

Hosts used: the existing test-only `FixtureHost` / `OtherFixtureHost` (domain-neutral). If
the harness needs `Scene` summarization or fixture access from an integration test, the
relevant items are exposed as `#[doc(hidden)] pub` test-support (as `fixture` already is) —
no domain knowledge enters the engine.

## 3. Invariant / property tests (now)

Uses **`proptest`** (dev-dependency). A focused set generating random `Step` sequences and
asserting the engine's **invariants** (not exact output):

- **never panics** on any generated sequence of clicks/commands/ticks;
- **mutation only at drain:** host state does not change between a `handle_pointer` and the
  next `update`;
- **transactional swap:** a swap to a deliberately-broken skin always leaves the prior
  scene intact;
- **sandbox holds:** a generated skin cannot reach `io`/`os`/`require` (load returns `Err`).

Kept small and fast (bounded case counts) so CI stays quick.

## 4. Performance benchmarks (deferred to Phase 3b — structured now)

**Not built now.** The plan records a ready-to-execute task for 3b:

- A `crates/carapace/benches/` directory using **Criterion** (dev-dependency, added in 3b).
- Benchmarks for the hot paths: hit-testing (`Scene::hit` over a realistic scene), the
  command **drain**, and **scene rebuild** (skin load). In 3b, add **render frame-time**
  once direct-to-surface rendering exists — this is where the ~31fps finding is measured
  and fixed.
- A CI bench job (or a local `cargo bench` target) slots in alongside the GPU/render leg in
  3b. Criterion's baseline comparison is used to flag regressions.

Recording it now (with the directory layout and target list) means it drops into 3b without
re-deciding the shape.

## Testing the harness itself

The snapshot + invariant tests *are* tests; their "test" is that they run green in CI and
that an intentional behavior change requires a deliberate snapshot bless (proving they'd
catch an unintended one). The implementation plan includes a step that perturbs a scenario
to confirm the snapshot harness fails as expected, then reverts.

## Out of scope

- The GPU/render CI leg and perf benches → Phase 3b (structured above).
- Release/publish automation, coverage reporting, multi-Rust-version matrix (YAGNI for a
  pre-1.0 experiment).
- Removing the throwaway `proto`/`spike-render` crates → end of Phase 3 (they still build
  and are exercised by `cargo test --workspace`, so CI stays green meanwhile).

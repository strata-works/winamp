# Contributing to Carapace

Thanks for your interest in Carapace — a skin engine that renders declarative Lua skins on wgpu/vello and embeds into host apps via a safe C ABI. This guide gets you from a fresh clone to a passing pull request.

- **Using** Carapace (writing skins, embedding the engine)? See the [API documentation](./docs/api/README.md).
- **Understanding** the design? The [root README](./README.md) has the architecture overview.
- **Contributing** to Carapace? Read on.

## Prerequisites

- **Rust** — a recent stable toolchain via [rustup](https://rustup.rs). The workspace is edition 2024 and is built against Rust 1.96. `rustfmt` and `clippy` components are required for the checks below (`rustup component add rustfmt clippy`).
- **A GPU** — rendering uses wgpu/vello: macOS via Metal, Linux via Vulkan. Most of the test suite is headless and GPU-free; only the render-correctness tests need a real adapter (see [Testing](#testing)).
- **System libraries** (Linux only):
  ```sh
  sudo apt install libfontconfig1-dev libasound2-dev pkg-config
  ```
  `fontconfig` is needed by the text layer (parley/fontique) for system-font fallback; `libasound2-dev` (ALSA) is needed by the demo's audio (rodio). macOS uses Core Text / CoreAudio and needs nothing extra.

## Set up

```sh
git clone https://github.com/strata-works/winamp.git
cd winamp

# Dependency versions are pinned via a committed Cargo.lock; CI builds --locked.
cargo build --locked
cargo test --workspace
```

Then run the live demo to confirm your GPU path works — a borderless, draggable window where the skin *is* the window (`Tab` cycles skins, `H` swaps between the media-player and system-monitor hosts):

```sh
cargo run -p carapace-demo
```

## Repository layout

It's a Cargo workspace:

| Crate | What it is |
|-------|------------|
| [`crates/carapace`](crates/carapace) | **The engine.** Scene graph, hit-testing, host command queue, state + value bindings, transactional skin swap, the Lua vocabulary, asset loading, and the wgpu/vello renderer. |
| [`crates/hittest`](crates/hittest) | A dependency-free point-in-region kernel (concave + holed shapes). Decoupled from rendering. |
| [`crates/carapace-demo`](crates/carapace-demo) | A live `winit`/`wgpu` host app + bundled demo skins (a media player and a `sysinfo` system monitor). The best place to see the engine driven end-to-end. |
| [`crates/carapace-ffi`](crates/carapace-ffi) | The production C ABI (Apple macOS/iOS) for embedding the engine as native UI. Header at `crates/carapace-ffi/include/carapace.h`. |
| [`crates/carapace-preview`](crates/carapace-preview) | The live skin previewer / dev tool (offscreen render → browser, with an inspector + parameters panel that write back to `skin.lua`). |
| `crates/window-spike`, `crates/embed-spike` | Throwaway feasibility spikes; not part of the shipped engine. |

New to the codebase? The [Engine API reference](./docs/api/engine-api.md) maps the engine's public surface, and the [Skin Authoring Reference](./docs/api/skin-authoring.md) documents the Lua vocabulary — useful context whichever layer you touch.

## Development workflow

Run these locally before pushing — they are exactly what CI runs, so a clean run here means a green PR:

```sh
# 1. Formatting (CI: `cargo fmt --all --check`)
cargo fmt --all

# 2. Lints — CI treats warnings as errors
cargo clippy --locked --workspace --all-targets -- -D warnings

# 3. Headless test suite (engine, hit-test kernel, skin/scene tests)
cargo test --locked --workspace
```

Handy while iterating:

```sh
cargo run -p carapace-demo                         # the live demo host
cargo run -p carapace-preview -- <skin-dir>        # preview a skin with hot-reload
```

## Testing

- **Headless suite** — the bulk of the tests are GPU-free and run in `cargo test --workspace`. Many use scene snapshots (`Scene::summary()`) for stable, domain-neutral assertions.
- **GPU render tests** — the actual pixel-render tests need a real adapter and are gated behind the `gpu-tests` feature so the headless lane stays adapter-free. Run them locally (they use your system GPU) with:
  ```sh
  cargo test -p carapace --features gpu-tests --test render_offscreen
  cargo test -p carapace-preview --features gpu-tests renders_a_nonempty_frame
  ```
  > If you add a test that needs a wgpu adapter, gate it behind `#[cfg(feature = "gpu-tests")]` — otherwise the headless CI lane (and `cargo test --workspace`) will fail with "no wgpu adapter".
- **New behavior gets a test.** Test-driven development is encouraged: write the failing test first, then make it pass.

## What CI checks

Two workflows run on every pull request (`.github/workflows/`):

- **`ci.yml`**
  - `check` — `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`.
  - `render` — the `gpu-tests` render lane under a software Vulkan adapter (Mesa lavapipe).
- **`docs.yml`** — builds the mdBook guide + the `carapace` rustdoc reference (and deploys them to GitHub Pages on `main`). It runs when `docs/**` or `crates/**` change.

All must be green to merge.

## Coding conventions

- **Edition 2024**, and `clippy -D warnings` + `rustfmt` clean (CI gates on both).
- **Match the surrounding code** — its naming, structure, comment density, and idioms. Improve code you touch, but don't reformat or restructure unrelated code in the same change.
- **Commit messages** use [Conventional Commits](https://www.conventionalcommits.org): `type(scope): summary`, e.g. `feat(engine): …`, `fix(preview): …`, `docs(api): …`, `ci(docs): …`. Common types here: `feat`, `fix`, `docs`, `chore`, `ci`, `test`, `refactor`.
- **`Cargo.lock` is committed** and CI builds `--locked`; if a change updates dependencies, commit the lockfile change with it.
- **Keep per-crate READMEs concise** (tagline, brief usage, feature bullets). Full guides and how-tos belong in the centralized [`docs/api/`](./docs/api/README.md), not in crate READMEs.
- **Document public API.** New public items in the `carapace` engine crate should carry `///` doc comments (they feed the generated [API reference](./docs/api/engine-api.md)).

## Adding a skin primitive

A common contribution is a new Lua primitive (e.g. a widget). Implement `carapace::vocab::Primitive` and register it on a `VocabRegistry` — see [Engine API → Vocab](./docs/api/engine-api.md#vocab) for the trait and `BuildContext`, and `crates/carapace-demo/src/transport.rs` / `gauge.rs` for worked examples. Skin-facing behavior should be covered by a headless scene-snapshot test.

## Submitting a change

1. Branch off `main` (e.g. `git checkout -b fix/thing`).
2. Make your change with tests; run the three local checks above until they're clean.
3. Open a pull request against `main` with a clear description of what and why. Keep PRs focused — one logical change per PR.
4. CI must pass and the change should be reviewed before merge.

Questions or a change you're unsure about? Open an issue to discuss it first — especially for larger or cross-cutting work.

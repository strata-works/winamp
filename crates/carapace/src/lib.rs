//! `carapace` is a skin engine: it loads a `skin.toml` manifest plus a Lua entry script, runs the
//! script against a vocabulary of primitives to build a `Scene`, then lays that scene out and
//! renders it on wgpu/vello. The engine is single-threaded — `Engine` is `!Send`/`!Sync` and must
//! be constructed, driven, and dropped on one thread.
//!
//! See the guide under `docs/api/` in the repo for the full API reference and lifecycle walkthrough.
#![deny(missing_docs)]

/// Resolves and indexes a skin's on-disk assets (images, fonts).
pub mod asset;
/// The engine's command queue: `SkinSource`, `Command`, and the host-action/`Swap`/`SwitchHost` types it carries.
pub mod command;
/// The `Engine`: owns the Lua VM, drives the skin lifecycle, and exposes input/update/layout entry points.
pub mod engine;
/// The `Host` trait: the capability surface a host app implements to expose data and actions to a skin.
pub mod host;
/// GPU-free layout resolution: resolves per-element anchors against the current window size.
pub mod layout;
/// The wgpu/vello renderer that draws a resolved `Scene`.
pub mod render;
/// The `Scene` graph, its `Node` variants, and hit-testing/picking queries.
pub mod scene;
/// Lua script loading and execution errors for the skin entry point.
pub mod script;
/// Geometry types (paths, contours) shared by hotspots and fills.
pub mod shape;
/// Skin manifest (`skin.toml`) loading and validation.
pub mod skin;
/// `StateValue`: the data type that crosses the engine/skin boundary via `Host::get`.
pub mod state;
/// Skin/host hot-swap support.
pub mod swap;
/// The primitive vocabulary (`fill`, `image`, `text`, ...) and `VocabRegistry` used to build a `Scene` from Lua.
pub mod vocab;

/// Re-exported so host extensions can implement `vocab::Primitive` (whose `build` takes an
/// `mlua::Table`) without depending on `mlua` directly and version-matching the engine.
pub use mlua;

#[doc(hidden)]
pub mod fixture;

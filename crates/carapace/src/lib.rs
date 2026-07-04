//! `carapace` is a skin engine: it loads a `skin.toml` manifest plus a Lua entry script, runs the
//! script against a vocabulary of primitives to build a `Scene`, then lays that scene out and
//! renders it on wgpu/vello. The engine is single-threaded — `Engine` is `!Send`/`!Sync` and must
//! be constructed, driven, and dropped on one thread.
//!
//! See the guide under `docs/api/` in the repo for the full API reference and lifecycle walkthrough.

pub mod asset;
pub mod command;
pub mod engine;
pub mod host;
pub mod layout;
pub mod render;
pub mod scene;
pub mod script;
pub mod shape;
pub mod skin;
pub mod state;
pub mod swap;
pub mod vocab;

/// Re-exported so host extensions can implement `vocab::Primitive` (whose `build` takes an
/// `mlua::Table`) without depending on `mlua` directly and version-matching the engine.
pub use mlua;

#[doc(hidden)]
pub mod fixture;

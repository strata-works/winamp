pub mod asset;
pub mod command;
pub mod engine;
pub mod host;
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

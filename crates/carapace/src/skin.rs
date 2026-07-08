use std::path::Path;

use serde::Deserialize;

use crate::command::SkinSource;

const SUPPORTED_SCHEMA: u32 = 1;
const SUPPORTED_ENGINE: &str = "^0.1";
const MAX_TRANSITION_MS: u32 = 5000;

fn default_asset_dir() -> String {
    "assets".to_string()
}

fn default_transition_kind() -> TransitionKind {
    TransitionKind::Crossfade
}
fn default_transition_ms() -> u32 {
    250
}

/// How a skin dissolves in when another skin is swapped to it. Declared by the *incoming* skin's
/// `skin.toml` `[transition]` table. Absent table → [`Transition::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Instant replacement (still stall-free — the load is warmed off the presented frame).
    Cut,
    /// Alpha dissolve from the outgoing skin to this one over `duration_ms`.
    Crossfade,
}

/// The incoming skin's swap transition. An absent `[transition]` table yields the default
/// (`Crossfade`, 250 ms). `duration_ms` is clamped to a sane ceiling by [`load_dir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Transition {
    /// The dissolve style.
    #[serde(default = "default_transition_kind")]
    pub kind: TransitionKind,
    /// Dissolve duration in milliseconds (clamped to 5000 on load).
    #[serde(default = "default_transition_ms")]
    pub duration_ms: u32,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            kind: default_transition_kind(),
            duration_ms: default_transition_ms(),
        }
    }
}

/// The skin's design-resolution size, in logical pixels.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Canvas {
    /// Design-resolution width.
    pub width: u32,
    /// Design-resolution height.
    pub height: u32,
}

/// The parsed, validated contents of a skin's `skin.toml`.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Manifest {
    /// Manifest schema version; must equal `1`.
    pub schema: u32,
    /// Skin identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Required engine version; must equal `"^0.1"` (exact string match).
    pub engine: String,
    /// Design-resolution size.
    pub canvas: Canvas,
    /// Lua entry filename, relative to the skin directory.
    pub entry: String,
    /// Assets subdirectory, relative to the skin directory; default `"assets"`.
    #[serde(default = "default_asset_dir")]
    pub asset_dir: String,
    /// Whether the skin window should be resizable (frame-skin archetype).
    #[serde(default)]
    pub resizable: bool,
    /// Minimum window size in logical pixels; `[width, height]` in the TOML.
    #[serde(default)]
    pub min_size: Option<(u32, u32)>,
    /// Maximum window size in logical pixels; `[width, height]` in the TOML.
    #[serde(default)]
    pub max_size: Option<(u32, u32)>,
    /// How this skin dissolves in when swapped to. Defaulted; see [`Transition`].
    #[serde(default)]
    pub transition: Transition,
}

/// Errors from loading or validating a skin directory via [`load_dir`].
#[derive(Debug)]
pub enum SkinError {
    /// Failed to read a file in the skin directory (`skin.toml`, the Lua entry, etc).
    Io(std::io::Error),
    /// `skin.toml` failed to parse.
    Toml(toml::de::Error),
    /// `schema` in `skin.toml` is not a version this engine supports.
    UnsupportedSchema(u32),
    /// `engine` in `skin.toml` doesn't match the required version string.
    EngineIncompat(String),
    /// Asset directory resolution failed.
    Asset(crate::asset::AssetError),
}
impl From<std::io::Error> for SkinError {
    fn from(e: std::io::Error) -> Self {
        SkinError::Io(e)
    }
}
impl From<toml::de::Error> for SkinError {
    fn from(e: toml::de::Error) -> Self {
        SkinError::Toml(e)
    }
}
impl From<crate::asset::AssetError> for SkinError {
    fn from(e: crate::asset::AssetError) -> Self {
        SkinError::Asset(e)
    }
}

/// Loads and validates `skin.toml` from a skin directory, reads the Lua entry
/// file, resolves the asset directory, and returns `(Manifest, SkinSource)`.
///
/// A skin directory holds `skin.toml`, the `entry` Lua file, and an
/// `asset_dir` (default `assets/`, recursively indexed by relative path;
/// symlinks are skipped to prevent sandbox escape).
pub fn load_dir(dir: &Path) -> Result<(Manifest, SkinSource), SkinError> {
    let mut manifest: Manifest = toml::from_str(&std::fs::read_to_string(dir.join("skin.toml"))?)?;
    if manifest.schema != SUPPORTED_SCHEMA {
        return Err(SkinError::UnsupportedSchema(manifest.schema));
    }
    if manifest.engine != SUPPORTED_ENGINE {
        return Err(SkinError::EngineIncompat(manifest.engine.clone()));
    }
    manifest.transition.duration_ms = manifest.transition.duration_ms.min(MAX_TRANSITION_MS);
    let lua_src = std::fs::read_to_string(dir.join(&manifest.entry))?;
    let canvas = (manifest.canvas.width, manifest.canvas.height);
    let assets = std::rc::Rc::new(crate::asset::AssetResolver::resolve(
        dir,
        &manifest.asset_dir,
    )?);
    let source = SkinSource {
        lua_src,
        canvas,
        assets,
    };
    Ok((manifest, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skins_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/skins")
    }

    #[test]
    fn loads_ok_skin() {
        let (m, src) = load_dir(&skins_dir().join("ok")).unwrap();
        assert_eq!(m.id, "ok");
        assert_eq!(src.canvas, (300, 120));
        assert!(src.lua_src.contains("fill"));
    }

    #[test]
    fn rejects_unknown_schema() {
        let dir = tempdir_with(
            "schema = 2\nid='x'\nname='x'\nengine='^0.1'\ncanvas={width=1,height=1}\nentry='s.lua'",
            "",
        );
        assert!(matches!(
            load_dir(dir.path()),
            Err(SkinError::UnsupportedSchema(2))
        ));
    }

    #[test]
    fn rejects_incompatible_engine() {
        let dir = tempdir_with(
            "schema = 1\nid='x'\nname='x'\nengine='^9.9'\ncanvas={width=1,height=1}\nentry='s.lua'",
            "",
        );
        assert!(matches!(
            load_dir(dir.path()),
            Err(SkinError::EngineIncompat(_))
        ));
    }

    #[test]
    fn transition_defaults_to_crossfade_250_when_absent() {
        let (m, _) = load_dir(&skins_dir().join("ok")).unwrap();
        assert_eq!(m.transition.kind, TransitionKind::Crossfade);
        assert_eq!(m.transition.duration_ms, 250);
    }

    #[test]
    fn transition_parses_explicit_cut() {
        let dir = tempdir_with(
            "schema=1\nid='x'\nname='x'\nengine='^0.1'\ncanvas={width=1,height=1}\nentry='s.lua'\n\
             [transition]\nkind='cut'\nduration_ms=100",
            "return {}",
        );
        let (m, _) = load_dir(dir.path()).unwrap();
        assert_eq!(m.transition.kind, TransitionKind::Cut);
        assert_eq!(m.transition.duration_ms, 100);
    }

    #[test]
    fn transition_duration_is_clamped() {
        let dir = tempdir_with(
            "schema=1\nid='x'\nname='x'\nengine='^0.1'\ncanvas={width=1,height=1}\nentry='s.lua'\n\
             [transition]\nkind='crossfade'\nduration_ms=999999",
            "return {}",
        );
        let (m, _) = load_dir(dir.path()).unwrap();
        assert_eq!(m.transition.duration_ms, 5000);
    }

    // Minimal temp-dir helper (no external crate).
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir_with(toml: &str, lua: &str) -> TempDir {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        toml.hash(&mut h);
        let base = std::env::temp_dir().join(format!("carapace-skintest-{}", h.finish()));
        let _ = std::fs::create_dir_all(&base);
        std::fs::write(base.join("skin.toml"), toml).unwrap();
        std::fs::write(base.join("s.lua"), lua).unwrap();
        TempDir(base)
    }
}

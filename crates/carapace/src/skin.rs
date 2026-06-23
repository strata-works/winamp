use std::path::Path;

use serde::Deserialize;

use crate::command::SkinSource;

const SUPPORTED_SCHEMA: u32 = 1;
const SUPPORTED_ENGINE: &str = "^0.1";

fn default_asset_dir() -> String {
    "assets".to_string()
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub engine: String,
    pub canvas: Canvas,
    pub entry: String,
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
}

#[derive(Debug)]
pub enum SkinError {
    Io(std::io::Error),
    Toml(toml::de::Error),
    UnsupportedSchema(u32),
    EngineIncompat(String),
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

pub fn load_dir(dir: &Path) -> Result<(Manifest, SkinSource), SkinError> {
    let manifest: Manifest = toml::from_str(&std::fs::read_to_string(dir.join("skin.toml"))?)?;
    if manifest.schema != SUPPORTED_SCHEMA {
        return Err(SkinError::UnsupportedSchema(manifest.schema));
    }
    if manifest.engine != SUPPORTED_ENGINE {
        return Err(SkinError::EngineIncompat(manifest.engine.clone()));
    }
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

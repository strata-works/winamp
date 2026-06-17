use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub entry: String,
}

#[derive(Debug)]
pub struct SkinFiles {
    pub manifest: Manifest,
    pub lua_src: String,
}

#[derive(Debug)]
pub enum SkinError {
    Io(std::io::Error),
    Toml(toml::de::Error),
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

pub fn load_dir(dir: &Path) -> Result<SkinFiles, SkinError> {
    let manifest_src = std::fs::read_to_string(dir.join("skin.toml"))?;
    let manifest: Manifest = toml::from_str(&manifest_src)?;
    let lua_src = std::fs::read_to_string(dir.join(&manifest.entry))?;
    Ok(SkinFiles { manifest, lua_src })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_media_classic_skin_dir() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skins/media-classic");
        let skin = load_dir(&dir).unwrap();
        assert_eq!(skin.manifest.id, "media-classic");
        assert_eq!(skin.manifest.width, 300);
        assert!(skin.lua_src.contains("value_fill"));
    }
}

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub enum AssetError {
    Unresolved(String),
    Io(String),
    Decode(String),
}

/// Type-agnostic, sandboxed asset resolver: scans a directory into a name->path index,
/// then serves raw bytes (any type) or decoded images, caching both.
pub struct AssetResolver {
    index: HashMap<String, PathBuf>,
    bytes_cache: RefCell<HashMap<String, Arc<[u8]>>>,
    image_cache: RefCell<HashMap<String, Arc<DecodedImage>>>,
}

fn walk(root: &Path, dir: &Path, index: &mut HashMap<String, PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, index)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                index.insert(rel.to_string_lossy().replace('\\', "/"), path.clone());
            }
        }
    }
    Ok(())
}

impl AssetResolver {
    /// An empty resolver — for inline skins with no assets. Every lookup is `Unresolved`.
    pub fn empty() -> Self {
        Self {
            index: HashMap::new(),
            bytes_cache: RefCell::new(HashMap::new()),
            image_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Resolve (scan) the asset directory under `skin_dir`, recursively, sandboxed.
    /// A missing asset dir yields an empty resolver (a skin may legitimately have no assets).
    pub fn resolve(skin_dir: &Path, asset_dir: &str) -> Result<Self, AssetError> {
        let root = skin_dir.join(asset_dir);
        let mut index = HashMap::new();
        if root.is_dir() {
            walk(&root, &root, &mut index).map_err(|e| AssetError::Io(e.to_string()))?;
        }
        Ok(Self {
            index,
            bytes_cache: RefCell::new(HashMap::new()),
            image_cache: RefCell::new(HashMap::new()),
        })
    }

    pub fn bytes(&self, name: &str) -> Result<Arc<[u8]>, AssetError> {
        if name.contains("..") {
            return Err(AssetError::Unresolved(name.to_string()));
        }
        if let Some(b) = self.bytes_cache.borrow().get(name) {
            return Ok(b.clone());
        }
        let path = self
            .index
            .get(name)
            .ok_or_else(|| AssetError::Unresolved(name.to_string()))?;
        let raw = std::fs::read(path).map_err(|e| AssetError::Io(e.to_string()))?;
        let arc: Arc<[u8]> = Arc::from(raw.into_boxed_slice());
        self.bytes_cache
            .borrow_mut()
            .insert(name.to_string(), arc.clone());
        Ok(arc)
    }

    pub fn image(&self, name: &str) -> Result<Arc<DecodedImage>, AssetError> {
        if let Some(img) = self.image_cache.borrow().get(name) {
            return Ok(img.clone());
        }
        let bytes = self.bytes(name)?;
        let dynimg =
            image::load_from_memory(&bytes).map_err(|e| AssetError::Decode(e.to_string()))?;
        let rgba = dynimg.to_rgba8();
        let decoded = Arc::new(DecodedImage {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        });
        self.image_cache
            .borrow_mut()
            .insert(name.to_string(), decoded.clone());
        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a temp skin dir with an assets/ subdir holding a tiny PNG (encoded via the image crate).
    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn temp_skin(seed: &str) -> Tmp {
        let base = std::env::temp_dir().join(format!("carapace-asset-{seed}"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("assets/sub")).unwrap();
        // a 2x2 RGBA PNG: top-left red, rest transparent
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.save(base.join("assets/red.png")).unwrap();
        img.save(base.join("assets/sub/red.png")).unwrap();
        std::fs::write(base.join("assets/not_an_image.txt"), b"hello").unwrap();
        Tmp(base)
    }

    #[test]
    fn resolves_and_decodes_png() {
        let t = temp_skin("decode");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        let img = r.image("red.png").unwrap();
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]); // top-left red, opaque
    }

    #[test]
    fn recursive_keying_works() {
        let t = temp_skin("recursive");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        assert!(
            r.image("sub/red.png").is_ok(),
            "nested asset resolvable by relative path"
        );
    }

    #[test]
    fn unresolved_and_traversal_error() {
        let t = temp_skin("sandbox");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        assert!(matches!(
            r.image("nope.png"),
            Err(AssetError::Unresolved(_))
        ));
        assert!(matches!(
            r.bytes("../secret"),
            Err(AssetError::Unresolved(_))
        ));
    }

    #[test]
    fn corrupt_image_is_decode_error() {
        let t = temp_skin("corrupt");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        assert!(matches!(
            r.image("not_an_image.txt"),
            Err(AssetError::Decode(_))
        ));
    }

    #[test]
    fn bytes_are_cached_and_empty_resolver_is_inert() {
        let t = temp_skin("cache");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        let a = r.bytes("red.png").unwrap();
        let b = r.bytes("red.png").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "second read hits the cache");
        assert!(matches!(
            AssetResolver::empty().image("red.png"),
            Err(AssetError::Unresolved(_))
        ));
    }
}

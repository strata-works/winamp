use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A decoded, ready-to-upload raster image: tightly packed 8-bit RGBA rows.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedImage {
    /// Pixel data, `width * height * 4` bytes, row-major, RGBA8 (non-premultiplied).
    pub rgba: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Failure modes for [`AssetResolver`] lookups and decoding.
#[derive(Debug)]
pub enum AssetError {
    /// The requested name isn't in the resolver's index (missing, symlinked-away, or a `..`
    /// traversal attempt was rejected outright).
    Unresolved(String),
    /// The underlying file could not be read from disk.
    Io(String),
    /// The bytes were read but failed to decode as the requested asset type (e.g. not a
    /// valid image).
    Decode(String),
}

/// Type-agnostic, sandboxed asset resolver: scans a directory into a name->path index,
/// then serves raw bytes (any type) or decoded images, caching both.
pub struct AssetResolver {
    index: HashMap<String, PathBuf>,
    bytes_cache: RefCell<HashMap<String, Arc<[u8]>>>,
    image_cache: RefCell<HashMap<String, Arc<DecodedImage>>>,
    font_cache: RefCell<HashMap<String, Arc<crate::scene::FontData>>>,
}

fn walk(root: &Path, dir: &Path, index: &mut HashMap<String, PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        // `DirEntry::file_type` does NOT follow symlinks (lstat). Skip symlinks so an asset
        // link can't escape the skin dir to read an arbitrary file (sandbox integrity).
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        if ft.is_dir() {
            walk(root, &path, index)?;
        } else if ft.is_file()
            && let Ok(rel) = path.strip_prefix(root)
        {
            index.insert(rel.to_string_lossy().replace('\\', "/"), path.clone());
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
            font_cache: RefCell::new(HashMap::new()),
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
            font_cache: RefCell::new(HashMap::new()),
        })
    }

    /// Read the raw bytes of `name` (a relative path as indexed by [`resolve`](Self::resolve)),
    /// caching the result. Rejects any name containing `..` outright (sandbox safety) before
    /// even consulting the index.
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

    /// Load and decode `name` as an image (PNG/JPEG/etc., via the `image` crate), caching the
    /// decoded RGBA result. Reuses [`bytes`](Self::bytes) internally, so both the raw bytes and
    /// the decoded image are cached independently.
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

    /// Load `name` as font data (raw font-file bytes wrapped for the text-layout engine),
    /// caching the result. Reuses [`bytes`](Self::bytes) internally.
    pub fn font(&self, name: &str) -> Result<Arc<crate::scene::FontData>, AssetError> {
        if let Some(f) = self.font_cache.borrow().get(name) {
            return Ok(f.clone());
        }
        let bytes = self.bytes(name)?;
        let font = Arc::new(crate::scene::FontData::new(bytes));
        self.font_cache
            .borrow_mut()
            .insert(name.to_string(), font.clone());
        Ok(font)
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
        std::fs::write(
            base.join("assets/face.ttf"),
            b"\x00\x01\x00\x00FAKEFONTBYTES",
        )
        .unwrap();
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

    #[test]
    fn font_returns_raw_bytes_and_caches() {
        let t = temp_skin("font");
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        let a = r.font("face.ttf").unwrap();
        let b = r.font("face.ttf").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "second font read hits the cache");
        assert_eq!(&a.bytes[0..4], &[0x00, 0x01, 0x00, 0x00]);
        assert!(matches!(r.font("nope.ttf"), Err(AssetError::Unresolved(_))));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escaping_the_skin_dir_is_not_resolved() {
        let t = temp_skin("symlink");
        // Write a secret OUTSIDE the skin dir and a symlink to it INSIDE assets/.
        let secret = std::env::temp_dir().join("carapace-asset-symlink-secret.txt");
        std::fs::write(&secret, b"top secret").unwrap();
        std::os::unix::fs::symlink(&secret, t.0.join("assets/leak.txt")).unwrap();
        let r = AssetResolver::resolve(&t.0, "assets").unwrap();
        // The symlink must NOT have been indexed -> unresolvable.
        assert!(matches!(
            r.bytes("leak.txt"),
            Err(AssetError::Unresolved(_))
        ));
        let _ = std::fs::remove_file(&secret);
    }
}

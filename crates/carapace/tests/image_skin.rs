use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::scene::Node;
use carapace::vocab::VocabRegistry;

// Builds a temp skin with an assets/ PNG and an `image` node; verifies it builds headlessly.
struct Tmp(std::path::PathBuf);
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_image_skin() -> Tmp {
    let base = std::env::temp_dir().join("carapace-image-skin");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("assets")).unwrap();
    let mut img = image::RgbaImage::new(8, 6);
    img.put_pixel(0, 0, image::Rgba([0, 255, 0, 255]));
    img.save(base.join("assets/face.png")).unwrap();
    std::fs::write(
        base.join("skin.toml"),
        "schema=1\nid='img'\nname='img'\nengine='^0.1'\ncanvas={width=100,height=80}\nentry='skin.lua'\n",
    )
    .unwrap();
    std::fs::write(
        base.join("skin.lua"),
        "image{ asset='face.png', x=0, y=0 }\n",
    )
    .unwrap();
    Tmp(base)
}

#[test]
fn image_skin_builds_headlessly() {
    let t = temp_image_skin();
    let (_m, source) = carapace::skin::load_dir(&t.0).unwrap();
    let e = Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), source).unwrap();
    let has_image = e
        .scene()
        .nodes
        .iter()
        .any(|n| matches!(n, Node::Image { .. }));
    assert!(has_image, "the skin built an Image node from its asset");
}

#[test]
fn missing_asset_fails_to_build() {
    let base = std::env::temp_dir().join("carapace-image-skin-missing");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("assets")).unwrap();
    std::fs::write(
        base.join("skin.toml"),
        "schema=1\nid='m'\nname='m'\nengine='^0.1'\ncanvas={width=10,height=10}\nentry='skin.lua'\n",
    )
    .unwrap();
    std::fs::write(
        base.join("skin.lua"),
        "image{ asset='nope.png', x=0, y=0 }\n",
    )
    .unwrap();
    let (_m, source) = carapace::skin::load_dir(&base).unwrap();
    let r = Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), source);
    assert!(r.is_err(), "missing asset makes the skin fail to build");
    let _ = std::fs::remove_dir_all(&base);
}

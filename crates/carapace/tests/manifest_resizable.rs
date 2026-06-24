#[test]
fn manifest_defaults_to_non_resizable() {
    let toml = r#"
schema = 1
id = "x"
name = "X"
engine = "carapace"
entry = "skin.lua"
canvas = { width = 100, height = 80 }
"#;
    let m: carapace::skin::Manifest = toml::from_str(toml).unwrap();
    assert!(!m.resizable);
    assert_eq!(m.min_size, None);
}

#[test]
fn manifest_parses_resizable_and_min_size() {
    let toml = r#"
schema = 1
id = "x"
name = "X"
engine = "carapace"
entry = "skin.lua"
canvas = { width = 480, height = 320 }
resizable = true
min_size = [320, 220]
"#;
    let m: carapace::skin::Manifest = toml::from_str(toml).unwrap();
    assert!(m.resizable);
    assert_eq!(m.min_size, Some((320, 220)));
}

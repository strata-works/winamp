//! Guards that the committed C header matches the current ABI. On macOS (where the ABI symbols are
//! active) it regenerates in memory and diffs. Run the ignored `regenerate_header` to update.

#[cfg(target_os = "macos")]
fn generate() -> String {
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let config = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml")).unwrap();
    let mut out = Vec::new();
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(config)
        .generate()
        .expect("cbindgen generate")
        .write(&mut out);
    String::from_utf8(out).unwrap()
}

#[cfg(target_os = "macos")]
#[test]
fn header_is_fresh() {
    let generated = generate();
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/include/carapace.h"))
            .expect("committed include/carapace.h must exist");
    assert_eq!(
        generated, committed,
        "carapace.h is stale — regenerate with: cargo test -p carapace-ffi --test header regenerate_header -- --ignored --exact"
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn regenerate_header() {
    let generated = generate();
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/include/carapace.h"),
        generated,
    )
    .unwrap();
}

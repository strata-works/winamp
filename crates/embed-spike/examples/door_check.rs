//! Host-side check: does the medieval door skin load + build (run its Lua) without error?
//! `Engine::new` runs the skin's Lua and returns the real error; the FFI path only sees NULL.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macOS only");
}

#[cfg(target_os = "macos")]
fn main() {
    use std::collections::HashMap;
    let dir = std::path::Path::new("crates/embed-spike/skin-frame");

    let (_m, source) = match carapace::skin::load_dir(dir) {
        Ok(v) => {
            println!("load_dir: OK");
            v
        }
        Err(e) => {
            println!("load_dir: ERROR: {e:?}");
            return;
        }
    };

    let mut values = HashMap::new();
    values.insert("lit".to_string(), "1".to_string());
    let host = Box::new(embed_spike::oneshot::InfoHost { values });
    match carapace::engine::Engine::new(host, carapace::vocab::VocabRegistry::base(), source) {
        Ok(_) => println!("Engine::new: OK — the skin builds fine"),
        Err(e) => println!("Engine::new: ERROR: {e:?}"),
    }

    // Also render it so we can eyeball the frame on the host.
    let mut v2 = HashMap::new();
    v2.insert("lit".to_string(), "1".to_string());
    let ok = embed_spike::oneshot::render_skin_with_host(
        dir,
        Box::new(embed_spike::oneshot::InfoHost { values: v2 }),
        360,
        640,
        "/tmp/door.png",
    );
    println!("render_skin_with_host -> /tmp/door.png : {}", ok.is_some());
}

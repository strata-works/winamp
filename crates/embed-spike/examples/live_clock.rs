#[cfg(not(target_os = "macos"))]
fn main() {}
#[cfg(target_os = "macos")]
fn main() {
    use embed_spike::oneshot::{render_skin_with_host, InfoHost};
    use std::collections::HashMap;
    use std::process::Command;
    let root = env!("CARGO_MANIFEST_DIR");
    let skin = std::path::PathBuf::from(format!("{root}/skin-clock"));
    let sh = |fmt: &str| {
        String::from_utf8(Command::new("date").arg(fmt).output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_string()
    };
    for i in 0..6 {
        let time = sh("+%H:%M:%S");
        let date = sh("+%a %b %d");
        let secs: f32 = sh("+%S").parse().unwrap_or(0.0);
        let mut values = HashMap::new();
        values.insert("time".into(), time.clone());
        values.insert("date".into(), date);
        values.insert("seconds".into(), format!("{}", secs / 60.0));
        let out = format!("/tmp/clock/frame-{i}.png");
        let ok =
            render_skin_with_host(&skin, Box::new(InfoHost { values }), 640, 280, &out).is_some();
        println!("frame {i}: time={time} ok={ok}");
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
}

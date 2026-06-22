// Proves one Engine runs two domains: media -> SwitchHost(sysmon) on the same instance.
use std::path::Path;
use std::time::Duration;

use carapace::command::{Command, SkinSource};
use carapace::engine::Engine;
use carapace::scene::{FillDir, Node};
use carapace::state::StateValue;
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;
use carapace_demo::gauge::GaugePrim;
use carapace_demo::sysmon_host::SysmonHost;
use carapace_demo::transport::TransportPrim;

fn src(dir: &str) -> SkinSource {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skins")
        .join(dir);
    carapace::skin::load_dir(&p).unwrap().1
}

#[test]
fn one_engine_switches_media_to_system_monitor() {
    let mut reg = VocabRegistry::base();
    reg.register(Box::new(TransportPrim));
    reg.register(Box::new(GaugePrim));
    let mut e = Engine::new(Box::new(DemoHost::new()), reg, src("classic")).unwrap();

    // Live-switch the whole domain on the same engine instance.
    e.handle_command(Command::SwitchHost {
        host: Box::new(SysmonHost::new()),
        skin: src("sysmon"),
    });
    e.update(Duration::from_millis(200)); // applies the switch + ticks the sysmon host

    assert!(
        e.scene().nodes.iter().any(|n| matches!(
            n,
            Node::ValueFill {
                direction: FillDir::Up,
                ..
            }
        )),
        "sysmon scene has vertical gauges"
    );
    match e.state("cpu") {
        Some(StateValue::Scalar(v)) => assert!((0.0..=1.0).contains(&v)),
        other => panic!("cpu should be a unit Scalar on the sysmon host, got {other:?}"),
    }
}

use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::scene::Node;
use carapace::vocab::VocabRegistry;

#[test]
fn base_registry_now_has_nine() {
    assert_eq!(VocabRegistry::base().iter().count(), 9);
}

#[test]
fn frame_builds_a_frame_node_with_slice_and_center() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../carapace-demo/skins/reference");
    let (_m, mut src) = carapace::skin::load_dir(&dir).unwrap();
    src.lua_src = "frame{ asset='headspace.png', x=0, y=0, w=100, h=80, \
        slice={left=10,right=10,top=10,bottom=10}, center='hollow' }\n"
        .to_string();
    let e = Engine::new(Box::new(FixtureHost::new()), VocabRegistry::base(), src).unwrap();
    match &e.scene().nodes[0] {
        Node::Frame { dest, slice, .. } => {
            assert_eq!((dest.w, dest.h), (100.0, 80.0));
            assert_eq!(
                (slice.left, slice.right, slice.top, slice.bottom),
                (10.0, 10.0, 10.0, 10.0)
            );
        }
        _ => panic!("expected Frame node"),
    }
    assert!(e.scene().summary().contains("frame"));
}

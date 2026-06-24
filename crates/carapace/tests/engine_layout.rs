use carapace::command::SkinSource;
use carapace::engine::Engine;
use carapace::fixture::FixtureHost;
use carapace::scene::{Node, Pt};
use carapace::vocab::VocabRegistry;

// A right/top-anchored hotspot must be hittable where it is DRAWN after resize — the bug behind
// "frame-skin buttons stop working once you resize the window". Design-space hit-testing misses it;
// hit-testing the layout-resolved scene finds it.
#[test]
fn resolved_scene_hits_anchored_hotspot_at_its_drawn_position() {
    const SKIN: &str = "region{ path = rect{x=90,y=4,w=8,h=8}, anchor={'right','top'}, on_press=function() end }\n";
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    // At design size the hotspot sits at x 90..98.
    assert!(e.scene().hit(Pt { x: 94.0, y: 8.0 }).is_some());
    // Resized to 200 wide it rides the right edge -> x 190..198. The old design position now misses,
    // but the resolved scene hits where the control is actually drawn.
    let resolved = e.layout(200.0, 100.0);
    assert!(resolved.hit(Pt { x: 194.0, y: 8.0 }).is_some());
    assert!(resolved.hit(Pt { x: 94.0, y: 8.0 }).is_none());
}

// Multi-node frame skin for the spec §8 multi-anchor integration test.
// Design canvas: 200×120.
//
// Node 0: fill title bar — rect(0,0,200,20), anchor {top,left,right}
// Node 1: view "c"       — (x=10,y=24,w=180,h=88), anchor {left,right,top,bottom}
// Node 2: view "btn"     — (x=180,y=4,w=12,h=12),  anchor {right,top}
const FRAME_SKIN: &str = "\
    fill{ path = rect{x=0,y=0,w=200,h=20}, color={r=255,g=255,b=255}, anchor={'top','left','right'} }\n\
    view{ id='c',   x=10,  y=24, w=180, h=88, anchor={'left','right','top','bottom'} }\n\
    view{ id='btn', x=180, y=4,  w=12,  h=12, anchor={'right','top'} }\n";
const FRAME_CANVAS: (u32, u32) = (200, 120);

// A full-bleed content view anchored to all four edges, in a 100x100 design.
const SKIN: &str = "view{ id='app', x=10, y=10, w=80, h=80, \
    anchor = { 'left','right','top','bottom' } }\n";

#[test]
fn layout_stretches_view_and_sets_canvas_to_logical() {
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    let resolved = e.layout(200.0, 150.0);
    assert_eq!(resolved.canvas, (200, 150)); // canvas = logical size -> render scales by DPI only
    match &resolved.nodes[0] {
        Node::View { dest, .. } => {
            // gaps left/top=10, right=100-90=10, bottom=10. -> x=10,y=10,w=180,h=130.
            assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 180.0, 130.0));
        }
        _ => panic!("expected a View node"),
    }
}

#[test]
fn layout_at_design_size_is_identity() {
    let e = Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(SKIN, (100, 100)),
    )
    .unwrap();
    let resolved = e.layout(100.0, 100.0);
    match &resolved.nodes[0] {
        Node::View { dest, .. } => {
            assert_eq!((dest.x, dest.y, dest.w, dest.h), (10.0, 10.0, 80.0, 80.0))
        }
        _ => panic!(),
    }
}

// ---------------------------------------------------------------------------
// Spec §8: multi-anchor resolve_scene integration test
// ---------------------------------------------------------------------------
//
// Design canvas: 200×120
//
// Node 0 — fill title bar, rect(0,0,200,20), anchor {top,left,right}
//   Design:  x=0  y=0  w=200 h=20
//   At 200×120 (identity): x=0, y=0, w=200, h=20  — path x∈[0,200], y∈[0,20]
//   At 320×200 (enlarged): delta_x=+120, delta_y=+80
//     left+right → stretch x: x'=0, w'=200+120=320
//     top only  → fixed    y: y'=0, h'=20
//     Path x-extent: 0..320, y-extent: 0..20
//
// Node 1 — view "c", x=10,y=24,w=180,h=88, anchor {left,right,top,bottom}
//   left+right → stretch x:  x'=10, w'=180+120=300
//     (right gap = 200-(10+180)=10, so width extends by full delta_x=120 → 180+120=300)
//   top+bottom → stretch y:  y'=24, h'=88+80=168
//     (bottom gap = 120-(24+88)=8, so height extends by full delta_y=80 → 88+80=168)
//   At identity: x=10, y=24, w=180, h=88
//   At 320×200:  x=10, y=24, w=300, h=168
//
// Node 2 — view "btn", x=180,y=4,w=12,h=12, anchor {right,top}
//   right only → rides far edge: x'=180+delta_x=180+120=300, w'=12
//   top only   → fixed:          y'=4, h'=12
//   At identity: x=180, y=4, w=12, h=12
//   At 320×200:  x=300, y=4, w=12, h=12

fn make_frame_engine() -> Engine {
    Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        SkinSource::inline(FRAME_SKIN, FRAME_CANVAS),
    )
    .unwrap()
}

#[test]
fn multi_anchor_layout_at_design_size_is_identity() {
    let e = make_frame_engine();
    let resolved = e.layout(200.0, 120.0);

    // Node 0: fill title bar — path must span x∈[0,200], y∈[0,20]
    match &resolved.nodes[0] {
        Node::Fill { path, .. } => {
            let xs: Vec<f32> = path.iter().map(|p| p.x).collect();
            let ys: Vec<f32> = path.iter().map(|p| p.y).collect();
            let x_min = xs.iter().cloned().fold(f32::INFINITY, f32::min);
            let x_max = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let y_min = ys.iter().cloned().fold(f32::INFINITY, f32::min);
            let y_max = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            // identity: unchanged rect(0,0,200,20)
            assert_eq!((x_min, x_max), (0.0, 200.0), "fill x-extent identity");
            assert_eq!((y_min, y_max), (0.0, 20.0), "fill y-extent identity");
        }
        other => panic!("node[0] expected Fill, got {other:?}"),
    }

    // Node 1: view "c" — identity rect
    match &resolved.nodes[1] {
        Node::View { id, dest } => {
            assert_eq!(id, "c");
            assert_eq!(
                (dest.x, dest.y, dest.w, dest.h),
                (10.0, 24.0, 180.0, 88.0),
                "view 'c' identity"
            );
        }
        other => panic!("node[1] expected View, got {other:?}"),
    }

    // Node 2: view "btn" — identity rect
    match &resolved.nodes[2] {
        Node::View { id, dest } => {
            assert_eq!(id, "btn");
            assert_eq!(
                (dest.x, dest.y, dest.w, dest.h),
                (180.0, 4.0, 12.0, 12.0),
                "view 'btn' identity"
            );
        }
        other => panic!("node[2] expected View, got {other:?}"),
    }
}

#[test]
fn multi_anchor_layout_enlarged_stretches_and_rides_edges() {
    // Enlarge from design 200×120 to 320×200. delta_x=+120, delta_y=+80.
    let e = make_frame_engine();
    let resolved = e.layout(320.0, 200.0);

    // Node 0: fill title bar — left+right anchor → x stretches to 0..320; y unchanged 0..20
    match &resolved.nodes[0] {
        Node::Fill { path, .. } => {
            let xs: Vec<f32> = path.iter().map(|p| p.x).collect();
            let ys: Vec<f32> = path.iter().map(|p| p.y).collect();
            let x_min = xs.iter().cloned().fold(f32::INFINITY, f32::min);
            let x_max = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let y_min = ys.iter().cloned().fold(f32::INFINITY, f32::min);
            let y_max = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            // left+right both → w'=200+120=320; x stays 0
            assert_eq!((x_min, x_max), (0.0, 320.0), "fill x-extent enlarged");
            // top only → h'=20 unchanged; y stays 0
            assert_eq!((y_min, y_max), (0.0, 20.0), "fill y-extent unchanged");
        }
        other => panic!("node[0] expected Fill, got {other:?}"),
    }

    // Node 1: view "c" — left+right+top+bottom → stretches both axes
    // right gap=200-(10+180)=10; bottom gap=120-(24+88)=8
    // w'=180+120=300 (delta_x extends width); h'=88+80=168 (delta_y extends height)
    match &resolved.nodes[1] {
        Node::View { id, dest } => {
            assert_eq!(id, "c");
            assert_eq!(dest.x, 10.0, "view 'c' x unchanged");
            assert_eq!(dest.y, 24.0, "view 'c' y unchanged");
            assert_eq!(dest.w, 300.0, "view 'c' w stretched: 180+120=300");
            assert_eq!(dest.h, 168.0, "view 'c' h stretched: 88+80=168");
        }
        other => panic!("node[1] expected View, got {other:?}"),
    }

    // Node 2: view "btn" — right+top → rides right edge, y fixed
    // x'=180+120=300 (delta_x=120); w'=12 (unchanged); y'=4, h'=12
    match &resolved.nodes[2] {
        Node::View { id, dest } => {
            assert_eq!(id, "btn");
            assert_eq!(dest.x, 300.0, "view 'btn' x rides right: 180+120=300");
            assert_eq!(dest.y, 4.0, "view 'btn' y unchanged");
            assert_eq!(dest.w, 12.0, "view 'btn' w unchanged");
            assert_eq!(dest.h, 12.0, "view 'btn' h unchanged");
        }
        other => panic!("node[2] expected View, got {other:?}"),
    }
}

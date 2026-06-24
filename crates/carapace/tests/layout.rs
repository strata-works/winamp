use carapace::layout::{Anchors, Rect, resolve_bbox};

const DESIGN: (f32, f32) = (100.0, 100.0);
const BIG: (f32, f32) = (200.0, 140.0);

fn a(left: bool, right: bool, top: bool, bottom: bool) -> Anchors {
    Anchors {
        left,
        right,
        top,
        bottom,
        min: None,
    }
}

#[test]
fn left_only_is_fixed_position_and_size() {
    let r = resolve_bbox(
        DESIGN,
        BIG,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 30.0,
            h: 20.0,
        },
        a(true, false, true, false),
    );
    assert_eq!(
        r,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 30.0,
            h: 20.0
        }
    );
}

#[test]
fn right_only_rides_the_right_edge() {
    // width grows 100->200 (+100); a right-anchored element keeps width, x shifts by +100.
    let r = resolve_bbox(
        DESIGN,
        BIG,
        Rect {
            x: 60.0,
            y: 10.0,
            w: 30.0,
            h: 20.0,
        },
        a(false, true, true, false),
    );
    assert_eq!(r.x, 160.0);
    assert_eq!(r.w, 30.0);
}

#[test]
fn left_and_right_stretches_width() {
    // gaps: left=10, right=100-(10+80)=10. At width 200: w = 200-10-10 = 180.
    let r = resolve_bbox(
        DESIGN,
        BIG,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 80.0,
            h: 20.0,
        },
        a(true, true, true, false),
    );
    assert_eq!(r.x, 10.0);
    assert_eq!(r.w, 180.0);
}

#[test]
fn top_and_bottom_stretches_height() {
    // height grows 100->140 (+40); top=10,bottom=10 gaps -> h = 140-20 = 120.
    let r = resolve_bbox(
        DESIGN,
        BIG,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 30.0,
            h: 80.0,
        },
        a(true, false, true, true),
    );
    assert_eq!(r.y, 10.0);
    assert_eq!(r.h, 120.0);
}

#[test]
fn stretch_clamps_to_min() {
    let mut an = a(true, true, true, false);
    an.min = Some((40.0, 0.0)); // never narrower than 40 even when window shrinks
    let small = (50.0, 100.0);
    // design width 100, shrink to 50: unclamped w = 80 + (50-100) = 30 -> clamp to 40.
    let r = resolve_bbox(
        DESIGN,
        small,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 80.0,
            h: 20.0,
        },
        an,
    );
    assert_eq!(r.w, 40.0);
}

#[test]
fn from_edges_parses_named_anchors() {
    assert_eq!(
        Anchors::from_edges(&["left", "right", "top"]),
        a(true, true, true, false)
    );
    assert_eq!(Anchors::from_edges(&[]), a(false, false, false, false));
}

#[test]
fn top_left_default_is_fixed() {
    assert_eq!(Anchors::TOP_LEFT, a(true, false, true, false));
}

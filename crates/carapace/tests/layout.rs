use carapace::layout::{Anchors, Frac, Rect, resolve_bbox};

const DESIGN: (f32, f32) = (100.0, 100.0);
const BIG: (f32, f32) = (200.0, 140.0);

fn a(left: bool, right: bool, top: bool, bottom: bool) -> Anchors {
    Anchors {
        left,
        right,
        top,
        bottom,
        min: None,
        max: None,
        frac: Frac::EMPTY,
    }
}

// Anchors with explicit min/max/frac (helper `a(...)` covers the edges-only case).
fn anc(edges: Anchors, min: Option<(f32, f32)>, max: Option<(f32, f32)>, frac: Frac) -> Anchors {
    Anchors {
        min,
        max,
        frac,
        ..edges
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

#[test]
fn frac_extent_is_container_fraction() {
    // 30% of logical width, regardless of design width.
    let a = anc(
        Anchors::from_edges(&["left", "top"]),
        None,
        None,
        Frac {
            w: Some(0.30),
            ..Frac::EMPTY
        },
    );
    let r = resolve_bbox(
        (400.0, 300.0),
        (1000.0, 300.0),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 144.0,
            h: 100.0,
        },
        a,
    );
    assert_eq!(r.w, 300.0); // 0.30 * 1000
    assert_eq!(r.h, 100.0); // untouched (no frac.h, top-only anchor)
}

#[test]
fn frac_position_only_leaves_extent_to_anchors() {
    // frac.x overrides ONLY position; extent still comes from the anchor rule (independent).
    // No x pins (top,bottom only) -> extent stays design width (neither-pinned keeps e).
    let a = anc(
        Anchors::from_edges(&["top", "bottom"]),
        None,
        None,
        Frac {
            x: Some(0.30),
            ..Frac::EMPTY
        },
    );
    let r = resolve_bbox(
        (400.0, 300.0),
        (1000.0, 300.0),
        Rect {
            x: 144.0,
            y: 0.0,
            w: 256.0,
            h: 300.0,
        },
        a,
    );
    assert_eq!(r.x, 300.0); // 0.30 * 1000
    assert_eq!(r.w, 256.0); // NOT stretched -- extent is design width, position frac is independent
}

#[test]
fn frac_fill_remaining_uses_both_x_and_w() {
    // "Fill the rest" = both position AND extent fractional (x=30%, w=70%) -> right edge at 100%.
    let a = anc(
        Anchors::from_edges(&["top", "bottom"]),
        None,
        None,
        Frac {
            x: Some(0.30),
            w: Some(0.70),
            ..Frac::EMPTY
        },
    );
    let r = resolve_bbox(
        (400.0, 300.0),
        (1000.0, 300.0),
        Rect {
            x: 144.0,
            y: 0.0,
            w: 256.0,
            h: 300.0,
        },
        a,
    );
    assert_eq!(r.x, 300.0);
    assert_eq!(r.w, 700.0); // right edge = 300 + 700 = 1000
}

#[test]
fn max_caps_a_stretch() {
    // both-edges-pinned stretch of a 100-wide element (design canvas 400), capped at 320.
    let a = anc(
        Anchors::from_edges(&["left", "right", "top"]),
        None,
        Some((320.0, f32::INFINITY)),
        Frac::EMPTY,
    );
    let capped = resolve_bbox(
        (400.0, 300.0),
        (700.0, 300.0),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        },
        a,
    );
    assert_eq!(capped.w, 320.0, "100 + delta 300 = 400, capped to 320");
    let below = resolve_bbox(
        (400.0, 300.0),
        (480.0, 300.0),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 20.0,
        },
        a,
    );
    assert_eq!(below.w, 180.0, "100 + delta 80 = 180, below the 320 cap");
}

#[test]
fn min_then_max_max_wins_when_contradictory() {
    let a = anc(
        Anchors::from_edges(&["left", "top"]),
        Some((200.0, 0.0)),
        Some((100.0, f32::INFINITY)),
        Frac {
            w: Some(0.10),
            ..Frac::EMPTY
        },
    );
    // frac 0.10*1000=100, min raises to 200, max lowers to 100 -> max wins.
    let r = resolve_bbox(
        (400.0, 300.0),
        (1000.0, 300.0),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 40.0,
            h: 20.0,
        },
        a,
    );
    assert_eq!(r.w, 100.0);
}

#[test]
fn sidebar_content_split_composes() {
    // 30% sidebar (max 320) + content occupying the remaining 70% (both x and w fractional).
    let side = anc(
        Anchors::from_edges(&["left", "top", "bottom"]),
        None,
        Some((320.0, f32::INFINITY)),
        Frac {
            w: Some(0.30),
            ..Frac::EMPTY
        },
    );
    let content = anc(
        Anchors::from_edges(&["top", "bottom"]),
        None,
        None,
        Frac {
            x: Some(0.30),
            w: Some(0.70),
            ..Frac::EMPTY
        },
    );
    let d = (480.0, 320.0);
    // At 800 wide: sidebar 240 (<320 cap) at x0; content x=240 w=560 -> they tile, right edge 800.
    let s = resolve_bbox(
        d,
        (800.0, 320.0),
        Rect {
            x: 0.0,
            y: 34.0,
            w: 144.0,
            h: 270.0,
        },
        side,
    );
    let c = resolve_bbox(
        d,
        (800.0, 320.0),
        Rect {
            x: 144.0,
            y: 34.0,
            w: 336.0,
            h: 270.0,
        },
        content,
    );
    assert_eq!(s.x, 0.0);
    assert_eq!(s.w, 240.0);
    assert_eq!(c.x, 240.0);
    assert_eq!(c.w, 560.0);
    // At 2000 wide: sidebar caps at 320; content still starts at 30% (600) -> a gap 320..600.
    let s2 = resolve_bbox(
        d,
        (2000.0, 320.0),
        Rect {
            x: 0.0,
            y: 34.0,
            w: 144.0,
            h: 270.0,
        },
        side,
    );
    assert_eq!(s2.w, 320.0, "capped");
}

#[test]
fn no_frac_no_max_is_identity_with_today() {
    // A both-pinned stretch with neither frac nor max must match the pre-change behavior.
    let a = Anchors::from_edges(&["left", "right", "top", "bottom"]);
    let r = resolve_bbox(
        (200.0, 100.0),
        (300.0, 140.0),
        Rect {
            x: 10.0,
            y: 10.0,
            w: 180.0,
            h: 80.0,
        },
        a,
    );
    assert_eq!(
        r,
        Rect {
            x: 10.0,
            y: 10.0,
            w: 280.0,
            h: 120.0
        }
    );
}

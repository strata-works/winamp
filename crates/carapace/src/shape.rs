use crate::scene::Pt;

/// Axis-aligned rectangle as 4 corners, clockwise from the top-left.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Pt> {
    vec![
        Pt { x, y },
        Pt { x: x + w, y },
        Pt { x: x + w, y: y + h },
        Pt { x, y: y + h },
    ]
}

/// Circle approximated by `segments` points evenly spaced on the radius.
pub fn circle(cx: f32, cy: f32, r: f32, segments: u32) -> Vec<Pt> {
    let n = segments.max(3);
    (0..n)
        .map(|i| {
            let a = (i as f32) / (n as f32) * std::f32::consts::TAU;
            Pt {
                x: cx + r * a.cos(),
                y: cy + r * a.sin(),
            }
        })
        .collect()
}

/// Rounded rectangle: 4 corner arcs of `segments` points each (4*segments total); the straight
/// sides are the polygon edges between adjacent arc endpoints. `radius` is clamped to min(w,h)/2.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, segments: u32) -> Vec<Pt> {
    let seg = segments.max(1);
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    // Corner centers and the start angle of each 90° arc, ordered CW so the polygon is continuous:
    // top-right, bottom-right, bottom-left, top-left.
    let corners = [
        (x + w - r, y + r, -std::f32::consts::FRAC_PI_2), // TR: from top, sweeping to right
        (x + w - r, y + h - r, 0.0),                      // BR
        (x + r, y + h - r, std::f32::consts::FRAC_PI_2),  // BL
        (x + r, y + r, std::f32::consts::PI),             // TL
    ];
    let mut pts = Vec::with_capacity((4 * seg) as usize);
    for (ccx, ccy, start) in corners {
        for i in 0..seg {
            let a = start + (i as f32) / ((seg - 1).max(1) as f32) * std::f32::consts::FRAC_PI_2;
            pts.push(Pt {
                x: ccx + r * a.cos(),
                y: ccy + r * a.sin(),
            });
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: Pt, bx: f32, by: f32) -> f32 {
        ((a.x - bx).powi(2) + (a.y - by).powi(2)).sqrt()
    }

    #[test]
    fn rect_is_four_corners_cw() {
        assert_eq!(
            rect(10.0, 20.0, 30.0, 40.0),
            vec![
                Pt { x: 10.0, y: 20.0 },
                Pt { x: 40.0, y: 20.0 },
                Pt { x: 40.0, y: 60.0 },
                Pt { x: 10.0, y: 60.0 },
            ]
        );
    }

    #[test]
    fn circle_has_n_points_on_the_radius() {
        let pts = circle(5.0, 6.0, 3.0, 16);
        assert_eq!(pts.len(), 16);
        for p in &pts {
            assert!((dist(*p, 5.0, 6.0) - 3.0).abs() < 1e-3, "point off the radius: {p:?}");
        }
    }

    #[test]
    fn rounded_rect_point_count_and_bounds() {
        let segs = 6;
        let pts = rounded_rect(0.0, 0.0, 100.0, 50.0, 8.0, segs);
        assert_eq!(pts.len() as u32, 4 * segs);
        for p in &pts {
            assert!(p.x >= -1e-3 && p.x <= 100.0 + 1e-3, "x out of box: {p:?}");
            assert!(p.y >= -1e-3 && p.y <= 50.0 + 1e-3, "y out of box: {p:?}");
        }
    }

    #[test]
    fn rounded_rect_radius_is_clamped() {
        // radius 999 on a 40x20 box clamps to min(w,h)/2 = 10; corner points stay within the box.
        let pts = rounded_rect(0.0, 0.0, 40.0, 20.0, 999.0, 4);
        for p in &pts {
            assert!(p.x >= -1e-3 && p.x <= 40.0 + 1e-3 && p.y >= -1e-3 && p.y <= 20.0 + 1e-3);
        }
    }
}

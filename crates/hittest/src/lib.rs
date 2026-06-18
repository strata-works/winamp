#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct Contour {
    pub points: Vec<Point>,
}

#[derive(Clone, Debug)]
pub struct Region {
    pub contours: Vec<Contour>,
}

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

impl Region {
    pub fn contains(&self, p: Point) -> bool {
        // Crossing-number (PNPOLY) ray cast to the right, accumulated across
        // every contour into a single parity. Two nested contours => the
        // overlap toggles twice => hole. This is even-odd fill.
        let mut inside = false;
        for contour in &self.contours {
            let pts = &contour.points;
            let n = pts.len();
            if n < 3 {
                continue;
            }
            let mut j = n - 1;
            for i in 0..n {
                let pi = pts[i];
                let pj = pts[j];
                if (pi.y > p.y) != (pj.y > p.y) {
                    let x_cross = pi.x + (p.y - pi.y) / (pj.y - pi.y) * (pj.x - pi.x);
                    if p.x < x_cross {
                        inside = !inside;
                    }
                }
                j = i;
            }
        }
        inside
    }
}

/// Concave L-shape on a 200x200 canvas. Concave vertex at (90, 90).
/// Inside examples: (60, 60), (130, 60). Outside (the notch): (130, 130).
pub fn l_shape() -> Region {
    Region {
        contours: vec![Contour {
            points: vec![
                pt(40.0, 40.0),
                pt(160.0, 40.0),
                pt(160.0, 90.0),
                pt(90.0, 90.0),
                pt(90.0, 160.0),
                pt(40.0, 160.0),
            ],
        }],
    }
}

/// Square ring (square with a square hole) on a 200x200 canvas.
/// Inside the ring material: (50, 100). Inside the hole (=outside region): (100, 100).
pub fn ring() -> Region {
    Region {
        contours: vec![
            Contour {
                points: vec![
                    pt(40.0, 40.0),
                    pt(160.0, 40.0),
                    pt(160.0, 160.0),
                    pt(40.0, 160.0),
                ],
            },
            Contour {
                points: vec![
                    pt(80.0, 80.0),
                    pt(120.0, 80.0),
                    pt(120.0, 120.0),
                    pt(80.0, 120.0),
                ],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Region {
        Region {
            contours: vec![Contour {
                points: vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)],
            }],
        }
    }

    #[test]
    fn point_inside_convex_square() {
        assert!(square().contains(pt(5.0, 5.0)));
    }

    #[test]
    fn point_outside_convex_square() {
        assert!(!square().contains(pt(15.0, 5.0)));
    }

    #[test]
    fn l_shape_interior_points_are_inside() {
        let l = l_shape();
        assert!(l.contains(pt(60.0, 60.0)), "lower-left arm");
        assert!(l.contains(pt(130.0, 60.0)), "top arm");
    }

    #[test]
    fn l_shape_notch_is_outside() {
        // The concave notch — the whole point of the spike.
        assert!(!l_shape().contains(pt(130.0, 130.0)));
    }

    #[test]
    fn ring_material_is_inside_but_hole_is_outside() {
        let r = ring();
        assert!(r.contains(pt(50.0, 100.0)), "ring material");
        assert!(!r.contains(pt(100.0, 100.0)), "the hole");
    }
}

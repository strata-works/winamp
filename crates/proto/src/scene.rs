use hittest::{Contour, Point, Region};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub type HandlerId = usize;

#[derive(Clone, Debug)]
pub enum Node {
    Fill { path: Vec<Pt>, color: Color },
    Hotspot { path: Vec<Pt>, on_press: HandlerId },
    ValueFill { path: Vec<Pt>, value_key: String, color: Color },
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub canvas: (u32, u32),
}

fn region_of(path: &[Pt]) -> Region {
    Region {
        contours: vec![Contour {
            points: path.iter().map(|p| Point { x: p.x, y: p.y }).collect(),
        }],
    }
}

impl Scene {
    /// Topmost hotspot containing `p` (later nodes draw on top, so iterate in reverse).
    pub fn hit(&self, p: Pt) -> Option<HandlerId> {
        for node in self.nodes.iter().rev() {
            if let Node::Hotspot { path, on_press } = node {
                if region_of(path).contains(Point { x: p.x, y: p.y }) {
                    return Some(*on_press);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Concave L-shape (same shape family as the Phase 0 kernel test).
    fn l_path() -> Vec<Pt> {
        vec![
            Pt { x: 40.0, y: 40.0 },
            Pt { x: 160.0, y: 40.0 },
            Pt { x: 160.0, y: 90.0 },
            Pt { x: 90.0, y: 90.0 },
            Pt { x: 90.0, y: 160.0 },
            Pt { x: 40.0, y: 160.0 },
        ]
    }

    #[test]
    fn click_inside_hotspot_returns_its_handler() {
        let scene = Scene {
            nodes: vec![Node::Hotspot { path: l_path(), on_press: 7 }],
            canvas: (200, 200),
        };
        assert_eq!(scene.hit(Pt { x: 60.0, y: 60.0 }), Some(7));
    }

    #[test]
    fn click_in_concave_notch_misses() {
        let scene = Scene {
            nodes: vec![Node::Hotspot { path: l_path(), on_press: 7 }],
            canvas: (200, 200),
        };
        assert_eq!(scene.hit(Pt { x: 130.0, y: 130.0 }), None);
    }

    #[test]
    fn topmost_overlapping_hotspot_wins() {
        let square = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 100.0, y: 0.0 },
            Pt { x: 100.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let scene = Scene {
            nodes: vec![
                Node::Hotspot { path: square.clone(), on_press: 1 },
                Node::Hotspot { path: square, on_press: 2 },
            ],
            canvas: (200, 200),
        };
        assert_eq!(scene.hit(Pt { x: 50.0, y: 50.0 }), Some(2));
    }
}

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
    Fill {
        path: Vec<Pt>,
        color: Color,
    },
    Hotspot {
        region: Region,
        on_press: HandlerId,
    },
    ValueFill {
        path: Vec<Pt>,
        value_key: String,
        color: Color,
    },
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub canvas: (u32, u32),
}

/// Build a single-contour Region from a polygon path (cached into Hotspot nodes).
pub fn region_of(path: &[Pt]) -> Region {
    Region {
        contours: vec![Contour {
            points: path.iter().map(|p| Point { x: p.x, y: p.y }).collect(),
        }],
    }
}

impl Scene {
    /// A stable, domain-neutral textual summary of the scene, for snapshot tests.
    /// Prints node kinds + style + binding keys; never the raw hit-test geometry.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("canvas {}x{}", self.canvas.0, self.canvas.1)];
        for node in &self.nodes {
            lines.push(match node {
                Node::Fill { color, .. } => {
                    format!("fill rgb={},{},{}", color.r, color.g, color.b)
                }
                Node::Hotspot { on_press, .. } => format!("hotspot handler={}", on_press),
                Node::ValueFill {
                    value_key, color, ..
                } => format!(
                    "value_fill key={} rgb={},{},{}",
                    value_key, color.r, color.g, color.b
                ),
            });
        }
        lines.join("\n")
    }

    /// Topmost hotspot containing `p` (later nodes draw on top → iterate in reverse).
    pub fn hit(&self, p: Pt) -> Option<HandlerId> {
        for node in self.nodes.iter().rev() {
            if let Node::Hotspot { region, on_press } = node
                && region.contains(Point { x: p.x, y: p.y })
            {
                return Some(*on_press);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn hotspot(path: &[Pt], id: HandlerId) -> Node {
        Node::Hotspot {
            region: region_of(path),
            on_press: id,
        }
    }

    #[test]
    fn click_inside_hotspot_returns_handler() {
        let s = Scene {
            nodes: vec![hotspot(&l_path(), 7)],
            canvas: (200, 200),
        };
        assert_eq!(s.hit(Pt { x: 60.0, y: 60.0 }), Some(7));
    }

    #[test]
    fn click_in_concave_notch_misses() {
        let s = Scene {
            nodes: vec![hotspot(&l_path(), 7)],
            canvas: (200, 200),
        };
        assert_eq!(s.hit(Pt { x: 130.0, y: 130.0 }), None);
    }

    #[test]
    fn summary_is_stable_and_domain_neutral() {
        let scene = Scene {
            canvas: (300, 120),
            nodes: vec![
                Node::Fill {
                    path: vec![Pt { x: 0.0, y: 0.0 }],
                    color: Color {
                        r: 10,
                        g: 20,
                        b: 30,
                    },
                },
                Node::Hotspot {
                    region: region_of(&l_path()),
                    on_press: 2,
                },
                Node::ValueFill {
                    path: vec![Pt { x: 0.0, y: 0.0 }],
                    value_key: "level".to_string(),
                    color: Color { r: 1, g: 2, b: 3 },
                },
            ],
        };
        let expected = "canvas 300x120\n\
                        fill rgb=10,20,30\n\
                        hotspot handler=2\n\
                        value_fill key=level rgb=1,2,3";
        assert_eq!(scene.summary(), expected);
    }

    #[test]
    fn topmost_overlapping_hotspot_wins() {
        let sq = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 100.0, y: 0.0 },
            Pt { x: 100.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let s = Scene {
            nodes: vec![hotspot(&sq, 1), hotspot(&sq, 2)],
            canvas: (200, 200),
        };
        assert_eq!(s.hit(Pt { x: 50.0, y: 50.0 }), Some(2));
    }
}

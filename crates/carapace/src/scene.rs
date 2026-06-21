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
    pub a: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub at: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    Linear {
        from: Pt,
        to: Pt,
        stops: Vec<ColorStop>,
    },
    Radial {
        center: Pt,
        radius: f32,
        stops: Vec<ColorStop>,
    },
    Sweep {
        center: Pt,
        start_deg: f32,
        end_deg: f32,
        stops: Vec<ColorStop>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    Gradient(Gradient),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextContent {
    Static(String),
    Bound(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillDir {
    Right,
    Left,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug)]
pub struct FontData {
    pub bytes: std::sync::Arc<[u8]>,
    pub id: u64,
}

impl FontData {
    /// Content-addressed id (hash of the bytes), stable across allocations so the renderer's
    /// font/layout caches never confuse two different fonts that happen to reuse an address.
    pub fn new(bytes: std::sync::Arc<[u8]>) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        Self {
            id: h.finish(),
            bytes,
        }
    }
}

pub type HandlerId = usize;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageDest {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub enum Node {
    Fill {
        path: Vec<Pt>,
        paint: Paint,
    },
    Hotspot {
        region: Region,
        on_press: HandlerId,
    },
    ValueFill {
        path: Vec<Pt>,
        value_key: String,
        color: Color,
        direction: FillDir,
    },
    Image {
        image: std::sync::Arc<crate::asset::DecodedImage>,
        dest: ImageDest,
    },
    Text {
        content: TextContent,
        font: Option<std::sync::Arc<FontData>>,
        font_name: Option<String>,
        size: f32,
        paint: Paint,
        halign: HAlign,
        valign: VAlign,
        max_width: Option<f32>,
        pos: Pt,
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
                Node::Fill { paint, .. } => match paint {
                    Paint::Solid(c) => format!("fill rgba={},{},{},{}", c.r, c.g, c.b, c.a),
                    Paint::Gradient(g) => {
                        let (kind, n) = match g {
                            Gradient::Linear { stops, .. } => ("linear", stops.len()),
                            Gradient::Radial { stops, .. } => ("radial", stops.len()),
                            Gradient::Sweep { stops, .. } => ("sweep", stops.len()),
                        };
                        format!("fill gradient={} stops={}", kind, n)
                    }
                },
                Node::Hotspot { on_press, .. } => format!("hotspot handler={}", on_press),
                Node::ValueFill {
                    value_key,
                    color,
                    direction,
                    ..
                } => {
                    let dir = match direction {
                        FillDir::Right => "right",
                        FillDir::Left => "left",
                        FillDir::Up => "up",
                        FillDir::Down => "down",
                    };
                    format!(
                        "value_fill key={} dir={} rgba={},{},{},{}",
                        value_key, dir, color.r, color.g, color.b, color.a
                    )
                }
                Node::Image { image, dest } => format!(
                    "image {}x{} at {},{} dest {}x{}",
                    image.width,
                    image.height,
                    dest.x as i64,
                    dest.y as i64,
                    dest.w as i64,
                    dest.h as i64
                ),
                Node::Text {
                    content,
                    font_name,
                    size,
                    paint,
                    halign,
                    valign,
                    ..
                } => {
                    let head = match content {
                        TextContent::Static(s) => format!("text \"{s}\""),
                        TextContent::Bound(k) => format!("text value={k}"),
                    };
                    let font = font_name.as_deref().unwrap_or("system");
                    let h = match halign {
                        HAlign::Left => "left",
                        HAlign::Center => "center",
                        HAlign::Right => "right",
                    };
                    let v = match valign {
                        VAlign::Top => "top",
                        VAlign::Middle => "middle",
                        VAlign::Bottom => "bottom",
                    };
                    let paint_s = match paint {
                        Paint::Solid(c) => format!("rgba={},{},{},{}", c.r, c.g, c.b, c.a),
                        Paint::Gradient(g) => {
                            let (kind, n) = match g {
                                Gradient::Linear { stops, .. } => ("linear", stops.len()),
                                Gradient::Radial { stops, .. } => ("radial", stops.len()),
                                Gradient::Sweep { stops, .. } => ("sweep", stops.len()),
                            };
                            format!("gradient={kind} stops={n}")
                        }
                    };
                    format!(
                        "{head} font={font} size={} halign={h} valign={v} {paint_s}",
                        *size as i64
                    )
                }
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
                    paint: Paint::Solid(Color {
                        r: 10,
                        g: 20,
                        b: 30,
                        a: 255,
                    }),
                },
                Node::Hotspot {
                    region: region_of(&l_path()),
                    on_press: 2,
                },
                Node::ValueFill {
                    path: vec![Pt { x: 0.0, y: 0.0 }],
                    value_key: "level".to_string(),
                    color: Color {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255,
                    },
                    direction: FillDir::Right,
                },
            ],
        };
        let expected = "canvas 300x120\n\
                        fill rgba=10,20,30,255\n\
                        hotspot handler=2\n\
                        value_fill key=level dir=right rgba=1,2,3,255";
        assert_eq!(scene.summary(), expected);
    }

    #[test]
    fn summary_includes_image_nodes() {
        use crate::asset::DecodedImage;
        use std::sync::Arc;
        let scene = Scene {
            canvas: (342, 394),
            nodes: vec![Node::Image {
                image: Arc::new(DecodedImage {
                    rgba: vec![0; 4],
                    width: 342,
                    height: 394,
                }),
                dest: ImageDest {
                    x: 0.0,
                    y: 0.0,
                    w: 342.0,
                    h: 394.0,
                },
            }],
        };
        assert_eq!(
            scene.summary(),
            "canvas 342x394\nimage 342x394 at 0,0 dest 342x394"
        );
    }

    #[test]
    fn summary_describes_gradient_fills() {
        let scene = Scene {
            canvas: (10, 10),
            nodes: vec![Node::Fill {
                path: vec![Pt { x: 0.0, y: 0.0 }],
                paint: Paint::Gradient(Gradient::Linear {
                    from: Pt { x: 0.0, y: 0.0 },
                    to: Pt { x: 0.0, y: 10.0 },
                    stops: vec![
                        ColorStop {
                            at: 0.0,
                            color: Color {
                                r: 0,
                                g: 0,
                                b: 0,
                                a: 255,
                            },
                        },
                        ColorStop {
                            at: 1.0,
                            color: Color {
                                r: 255,
                                g: 255,
                                b: 255,
                                a: 0,
                            },
                        },
                    ],
                }),
            }],
        };
        assert_eq!(
            scene.summary(),
            "canvas 10x10\nfill gradient=linear stops=2"
        );
    }

    #[test]
    fn summary_describes_text_nodes() {
        let scene = Scene {
            canvas: (200, 50),
            nodes: vec![
                Node::Text {
                    content: TextContent::Static("HI".to_string()),
                    font: None,
                    font_name: Some("vt323.ttf".to_string()),
                    size: 18.0,
                    paint: Paint::Solid(Color {
                        r: 1,
                        g: 2,
                        b: 3,
                        a: 255,
                    }),
                    halign: HAlign::Center,
                    valign: VAlign::Top,
                    max_width: None,
                    pos: Pt { x: 40.0, y: 8.0 },
                },
                Node::Text {
                    content: TextContent::Bound("track_title".to_string()),
                    font: None,
                    font_name: None,
                    size: 12.0,
                    paint: Paint::Gradient(Gradient::Linear {
                        from: Pt { x: 0.0, y: 0.0 },
                        to: Pt { x: 0.0, y: 12.0 },
                        stops: vec![
                            ColorStop {
                                at: 0.0,
                                color: Color {
                                    r: 0,
                                    g: 0,
                                    b: 0,
                                    a: 255,
                                },
                            },
                            ColorStop {
                                at: 1.0,
                                color: Color {
                                    r: 9,
                                    g: 9,
                                    b: 9,
                                    a: 255,
                                },
                            },
                        ],
                    }),
                    halign: HAlign::Right,
                    valign: VAlign::Middle,
                    max_width: Some(120.0),
                    pos: Pt { x: 200.0, y: 30.0 },
                },
            ],
        };
        assert_eq!(
            scene.summary(),
            "canvas 200x50\n\
             text \"HI\" font=vt323.ttf size=18 halign=center valign=top rgba=1,2,3,255\n\
             text value=track_title font=system size=12 halign=right valign=middle gradient=linear stops=2"
        );
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

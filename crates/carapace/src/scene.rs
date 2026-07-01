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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameCenter {
    Stretch,
    Hollow,
}

pub type RowTemplate = Vec<RowCell>;

/// One text cell of a list row template, in row-relative coords. Built once at parse time.
#[derive(Clone, Debug)]
pub struct RowCell {
    pub bind: String,
    /// Horizontal placement: from the region's left edge, or from its right edge. Exactly one.
    pub x_from_left: Option<f32>,
    pub x_from_right: Option<f32>,
    pub y: f32,
    pub size: f32,
    pub color: Color,
    pub halign: HAlign,
    pub font: Option<std::sync::Arc<FontData>>,
    pub font_name: Option<String>,
}

impl RowCell {
    /// The concrete Text node for this cell in a row, positioned within `region`.
    pub fn to_node(&self, region: &ImageDest, row_top: f32, value: &str) -> Node {
        let x = match (self.x_from_left, self.x_from_right) {
            (Some(l), _) => region.x + l,
            (None, Some(r)) => region.x + region.w - r,
            (None, None) => region.x,
        };
        Node::Text {
            content: TextContent::Static(value.to_string()),
            font: self.font.clone(),
            font_name: self.font_name.clone(),
            size: self.size,
            paint: Paint::Solid(self.color),
            halign: self.halign,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt {
                x,
                y: row_top + self.y,
            },
        }
    }
}

/// Author-declared interaction role for a `hotspot{}`, reported by [`Scene::hit_kind`] so a host
/// can classify an OS event without firing the hotspot's Lua handler. Default is `Control`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotspotRole {
    /// Skin consumes the event (a button/control). Default.
    Control,
    /// Host should treat the region as window chrome (move the window).
    Drag,
    /// Event falls through to whatever is behind the skin (a deliberate hole).
    Passthrough,
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
        role: HotspotRole,
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
    Frame {
        image: std::sync::Arc<crate::asset::DecodedImage>,
        dest: ImageDest,
        slice: Slice,
        center: FrameCenter,
    },
    View {
        id: String,
        dest: ImageDest,
    },
    List {
        collection: String,
        region: ImageDest,
        row_height: f32,
        on_select: Option<String>,
        /// Visible row count, set during layout expansion; 0 in the design scene.
        count: usize,
        template: RowTemplate,
        /// Optional selection highlight: a bar of `highlight` color drawn behind the row whose
        /// index equals the host scalar at `selected`. Both must be set for a highlight to appear.
        highlight: Option<Color>,
        selected: Option<String>,
    },
    Scrub {
        region: ImageDest,
        value_key: String,
        direction: FillDir,
        color: Color,
        on_seek: String,
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
                Node::Frame {
                    image,
                    slice,
                    center,
                    ..
                } => format!(
                    "frame {}x{} slice {},{},{},{} center={}",
                    image.width,
                    image.height,
                    slice.left as i64,
                    slice.right as i64,
                    slice.top as i64,
                    slice.bottom as i64,
                    match center {
                        FrameCenter::Stretch => "stretch",
                        FrameCenter::Hollow => "hollow",
                    }
                ),
                Node::View { id, .. } => format!("view id={id}"),
                Node::List {
                    collection, count, ..
                } => format!("list collection={collection} rows={count}"),
                Node::Scrub {
                    value_key, on_seek, ..
                } => format!("scrub value={value_key} on_seek={on_seek}"),
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

    /// The host-content regions a skin declares, in canvas coords — the embedder fills these.
    pub fn views(&self) -> Vec<(String, ImageDest)> {
        self.nodes
            .iter()
            .filter_map(|n| match n {
                Node::View { id, dest } => Some((id.clone(), *dest)),
                _ => None,
            })
            .collect()
    }

    /// Topmost list row under `p`: `(on_select action, row index)`. Lists draw later → reverse.
    pub fn hit_row(&self, p: Pt) -> Option<(String, usize)> {
        for node in self.nodes.iter().rev() {
            let Node::List {
                region,
                row_height,
                on_select,
                count,
                ..
            } = node
            else {
                continue;
            };
            let Some(action) = on_select else { continue };
            if *row_height <= 0.0 || *count == 0 {
                continue;
            }
            if p.x < region.x || p.x > region.x + region.w || p.y < region.y {
                continue;
            }
            let idx = ((p.y - region.y) / row_height).floor() as usize;
            if idx < *count {
                return Some((action.clone(), idx));
            }
        }
        None
    }

    /// Topmost scrub bar under `p`: `(on_seek action, click fraction 0..1)`. Reverse order.
    pub fn hit_scrub(&self, p: Pt) -> Option<(String, f32)> {
        for node in self.nodes.iter().rev() {
            let Node::Scrub {
                region, on_seek, ..
            } = node
            else {
                continue;
            };
            if p.x < region.x
                || p.x > region.x + region.w
                || p.y < region.y
                || p.y > region.y + region.h
            {
                continue;
            }
            let frac = if region.w > 0.0 {
                ((p.x - region.x) / region.w).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Some((on_seek.clone(), frac));
        }
        None
    }

    /// Topmost hotspot containing `p` (later nodes draw on top → iterate in reverse).
    pub fn hit(&self, p: Pt) -> Option<HandlerId> {
        for node in self.nodes.iter().rev() {
            if let Node::Hotspot {
                region, on_press, ..
            } = node
                && region.contains(Point { x: p.x, y: p.y })
            {
                return Some(*on_press);
            }
        }
        None
    }

    /// Topmost interactive node under `p`, by z-order (later nodes draw on top → reverse scan),
    /// regardless of kind. This is what input dispatch should use: a `list{}` or `scrub{}` drawn
    /// on top of a background hotspot (e.g. a full-window drag region) correctly wins the click.
    pub fn hit_any(&self, p: Pt) -> Option<Hit> {
        for node in self.nodes.iter().rev() {
            match node {
                Node::Hotspot {
                    region, on_press, ..
                } if region.contains(Point { x: p.x, y: p.y }) => {
                    return Some(Hit::Handler(*on_press));
                }
                Node::List {
                    region,
                    row_height,
                    on_select: Some(action),
                    count,
                    ..
                } if *row_height > 0.0
                    && *count > 0
                    && p.x >= region.x
                    && p.x <= region.x + region.w
                    && p.y >= region.y =>
                {
                    let idx = ((p.y - region.y) / row_height).floor() as usize;
                    if idx < *count {
                        return Some(Hit::Row {
                            action: action.clone(),
                            index: idx,
                        });
                    }
                }
                Node::Scrub {
                    region, on_seek, ..
                } if p.x >= region.x
                    && p.x <= region.x + region.w
                    && p.y >= region.y
                    && p.y <= region.y + region.h =>
                {
                    let fraction = if region.w > 0.0 {
                        ((p.x - region.x) / region.w).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    return Some(Hit::Scrub {
                        action: on_seek.clone(),
                        fraction,
                    });
                }
                _ => {}
            }
        }
        None
    }
}

/// The topmost interactive node under a point — see [`Scene::hit_any`].
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    /// A polygon hotspot's registered handler.
    Handler(HandlerId),
    /// A `list{}` row: the `on_select` host action + the row index.
    Row { action: String, index: usize },
    /// A `scrub{}` bar: the `on_seek` host action + the 0..1 click fraction.
    Scrub { action: String, fraction: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_any_prefers_topmost_list_over_background_hotspot() {
        let full = vec![
            Pt { x: 0.0, y: 0.0 },
            Pt { x: 200.0, y: 0.0 },
            Pt { x: 200.0, y: 100.0 },
            Pt { x: 0.0, y: 100.0 },
        ];
        let scene = Scene {
            canvas: (200, 100),
            nodes: vec![
                // background full-canvas drag hotspot (drawn first / lowest z)
                Node::Hotspot {
                    region: region_of(&full),
                    on_press: 7,
                    role: HotspotRole::Control,
                },
                // a list drawn on top of it
                Node::List {
                    collection: "c".to_string(),
                    region: ImageDest {
                        x: 0.0,
                        y: 0.0,
                        w: 100.0,
                        h: 60.0,
                    },
                    row_height: 20.0,
                    on_select: Some("open".to_string()),
                    count: 3,
                    template: vec![],
                    highlight: None,
                    selected: None,
                },
            ],
        };
        // A click inside the list region resolves to the row, not the background drag.
        assert_eq!(
            scene.hit_any(Pt { x: 50.0, y: 30.0 }),
            Some(Hit::Row {
                action: "open".to_string(),
                index: 1
            })
        );
        // A click below the list (but inside the canvas) falls through to the drag hotspot.
        assert_eq!(
            scene.hit_any(Pt { x: 50.0, y: 80.0 }),
            Some(Hit::Handler(7))
        );
    }

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
            role: HotspotRole::Control,
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
                    role: HotspotRole::Control,
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
    fn views_accessor_and_summary() {
        let scene = Scene {
            canvas: (300, 200),
            nodes: vec![Node::View {
                id: "display".to_string(),
                dest: ImageDest {
                    x: 10.0,
                    y: 20.0,
                    w: 100.0,
                    h: 80.0,
                },
            }],
        };
        assert_eq!(
            scene.views(),
            vec![(
                "display".to_string(),
                ImageDest {
                    x: 10.0,
                    y: 20.0,
                    w: 100.0,
                    h: 80.0
                }
            )]
        );
        assert_eq!(scene.summary(), "canvas 300x200\nview id=display");
    }

    #[test]
    fn summary_describes_list_nodes() {
        let scene = Scene {
            canvas: (200, 100),
            nodes: vec![Node::List {
                collection: "entries".to_string(),
                region: ImageDest {
                    x: 10.0,
                    y: 20.0,
                    w: 100.0,
                    h: 60.0,
                },
                row_height: 20.0,
                on_select: Some("open_entry".to_string()),
                count: 3,
                template: vec![],
                highlight: None,
                selected: None,
            }],
        };
        assert_eq!(
            scene.summary(),
            "canvas 200x100\nlist collection=entries rows=3"
        );
    }

    fn list_scene(count: usize, on_select: Option<&str>) -> Scene {
        Scene {
            canvas: (200, 100),
            nodes: vec![Node::List {
                collection: "c".to_string(),
                region: ImageDest {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 80.0,
                },
                row_height: 20.0,
                on_select: on_select.map(|s| s.to_string()),
                count,
                template: vec![],
                highlight: None,
                selected: None,
            }],
        }
    }

    #[test]
    fn hit_row_maps_y_to_index() {
        let s = list_scene(3, Some("open"));
        assert_eq!(
            s.hit_row(Pt { x: 50.0, y: 10.0 }),
            Some(("open".to_string(), 0))
        );
        assert_eq!(
            s.hit_row(Pt { x: 50.0, y: 30.0 }),
            Some(("open".to_string(), 1))
        );
        assert_eq!(
            s.hit_row(Pt { x: 50.0, y: 50.0 }),
            Some(("open".to_string(), 2))
        );
    }

    #[test]
    fn hit_row_misses_beyond_count_and_outside_region() {
        let s = list_scene(3, Some("open"));
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 70.0 }), None, "row 3 >= count");
        assert_eq!(s.hit_row(Pt { x: 50.0, y: -5.0 }), None, "above region");
        assert_eq!(s.hit_row(Pt { x: 150.0, y: 10.0 }), None, "right of region");
    }

    #[test]
    fn hit_row_none_without_on_select() {
        let s = list_scene(3, None);
        assert_eq!(s.hit_row(Pt { x: 50.0, y: 10.0 }), None);
    }

    #[test]
    fn summary_describes_scrub_nodes() {
        let scene = Scene {
            canvas: (300, 100),
            nodes: vec![Node::Scrub {
                region: ImageDest {
                    x: 10.0,
                    y: 20.0,
                    w: 200.0,
                    h: 12.0,
                },
                value_key: "position".to_string(),
                direction: FillDir::Right,
                color: Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                },
                on_seek: "seek".to_string(),
            }],
        };
        assert_eq!(
            scene.summary(),
            "canvas 300x100\nscrub value=position on_seek=seek"
        );
    }

    fn scrub_scene() -> Scene {
        Scene {
            canvas: (200, 50),
            nodes: vec![Node::Scrub {
                region: ImageDest {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 20.0,
                },
                value_key: "position".to_string(),
                direction: FillDir::Right,
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                on_seek: "seek".to_string(),
            }],
        }
    }

    #[test]
    fn hit_scrub_maps_x_to_fraction() {
        let s = scrub_scene();
        assert_eq!(
            s.hit_scrub(Pt { x: 0.0, y: 10.0 }),
            Some(("seek".to_string(), 0.0))
        );
        assert_eq!(
            s.hit_scrub(Pt { x: 50.0, y: 10.0 }),
            Some(("seek".to_string(), 0.5))
        );
        assert_eq!(
            s.hit_scrub(Pt { x: 100.0, y: 10.0 }),
            Some(("seek".to_string(), 1.0))
        );
    }

    #[test]
    fn hit_scrub_misses_outside_region() {
        let s = scrub_scene();
        assert_eq!(s.hit_scrub(Pt { x: 50.0, y: 30.0 }), None, "below region");
        assert_eq!(s.hit_scrub(Pt { x: -1.0, y: 10.0 }), None, "left of region");
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

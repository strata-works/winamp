use hittest::{Contour, Point, Region};

/// A 2D point in canvas coordinates (design space before layout, logical space after).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

/// An 8-bit-per-channel RGBA color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Red channel, 0..255.
    pub r: u8,
    /// Green channel, 0..255.
    pub g: u8,
    /// Blue channel, 0..255.
    pub b: u8,
    /// Alpha channel, 0..255 (0 = transparent, 255 = opaque).
    pub a: u8,
}

/// One color stop in a [`Gradient`]: a position along the gradient axis and the color there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    /// Position along the gradient, 0.0..1.0.
    pub at: f32,
    /// The color at this stop.
    pub color: Color,
}

/// A multi-stop gradient paint, in one of three shapes.
#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    /// Stops interpolated along the line from `from` to `to`.
    Linear {
        from: Pt,
        to: Pt,
        stops: Vec<ColorStop>,
    },
    /// Stops interpolated radially outward from `center` to `radius`.
    Radial {
        center: Pt,
        radius: f32,
        stops: Vec<ColorStop>,
    },
    /// Stops interpolated angularly around `center`, from `start_deg` to `end_deg`.
    Sweep {
        center: Pt,
        start_deg: f32,
        end_deg: f32,
        stops: Vec<ColorStop>,
    },
}

/// A fill style: either a flat color or a gradient.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// A single flat color.
    Solid(Color),
    /// A multi-stop gradient.
    Gradient(Gradient),
}

/// The text a [`Node::Text`] draws: either a literal string or a value bound to host state.
#[derive(Clone, Debug, PartialEq)]
pub enum TextContent {
    /// A fixed, author-supplied string.
    Static(String),
    /// A key resolved against host/engine state at render time.
    Bound(String),
}

/// Direction a [`Node::ValueFill`] or [`Node::Scrub`] fill grows/reads from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillDir {
    /// Fills/reads left-to-right.
    Right,
    /// Fills/reads right-to-left.
    Left,
    /// Fills/reads bottom-to-top.
    Up,
    /// Fills/reads top-to-bottom.
    Down,
}

/// Horizontal text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Vertical text alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

/// A decoded font's bytes plus a content-addressed id for cache keying.
#[derive(Debug)]
pub struct FontData {
    /// The raw font file bytes (e.g. TTF/OTF).
    pub bytes: std::sync::Arc<[u8]>,
    /// Content-addressed id (hash of `bytes`), stable across allocations.
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

/// Opaque id of a registered hotspot press handler (index into the engine's handler table).
pub type HandlerId = usize;

/// A rectangular destination in canvas coordinates: where an image/frame/view/list/scrub is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageDest {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// 9-slice insets for a [`Node::Frame`]: the border widths kept unscaled on each edge while the
/// center and edge segments stretch to fill `dest`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    /// Left border width, unscaled.
    pub left: f32,
    /// Right border width, unscaled.
    pub right: f32,
    /// Top border width, unscaled.
    pub top: f32,
    /// Bottom border width, unscaled.
    pub bottom: f32,
}

/// How a [`Node::Frame`]'s center region (inside the 9-slice insets) is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameCenter {
    /// The center image segment is stretched to fill the remaining space.
    Stretch,
    /// The center is left empty (a hole showing whatever is behind the frame).
    Hollow,
}

/// A parsed `list{}` row template: the per-row text cells to instantiate for each visible row.
pub type RowTemplate = Vec<RowCell>;

/// One text cell of a list row template, in row-relative coords. Built once at parse time.
#[derive(Clone, Debug)]
pub struct RowCell {
    /// The row-data key this cell's text is bound to (looked up per row at render time).
    pub bind: String,
    /// Horizontal placement: from the region's left edge, or from its right edge. Exactly one.
    pub x_from_left: Option<f32>,
    pub x_from_right: Option<f32>,
    /// Vertical offset from the row's top edge.
    pub y: f32,
    /// Font size.
    pub size: f32,
    /// Text color.
    pub color: Color,
    /// Horizontal text alignment.
    pub halign: HAlign,
    /// Resolved font data, if a custom font was loaded.
    pub font: Option<std::sync::Arc<FontData>>,
    /// The custom font's asset name, if any (`None` = system default).
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

/// A single drawable/interactive element of a [`Scene`]. Later nodes in `Scene::nodes` draw on
/// top and win hit-tests. Produced by vocab primitives (`fill`, `hotspot`, `value_fill`, `image`,
/// `frame`, `view`, `list`, `scrub`, `text`) at skin build time, then possibly reflowed by
/// `layout::resolve_scene`.
#[derive(Clone, Debug)]
pub enum Node {
    /// A flat-shaded or gradient-shaded polygon.
    Fill { path: Vec<Pt>, paint: Paint },
    /// An invisible polygon that dispatches `on_press`'s handler when clicked; `role` tells the
    /// host how to classify the region (control / drag / passthrough) via `hit_kind`.
    Hotspot {
        region: Region,
        on_press: HandlerId,
        role: HotspotRole,
    },
    /// A polygon filled proportionally to a host scalar at `value_key` (0..1), growing/revealing
    /// along `direction` (e.g. a VU meter or level bar).
    ValueFill {
        path: Vec<Pt>,
        value_key: String,
        color: Color,
        direction: FillDir,
    },
    /// A decoded bitmap image drawn into `dest`.
    Image {
        image: std::sync::Arc<crate::asset::DecodedImage>,
        dest: ImageDest,
    },
    /// A 9-slice-scaled bitmap: `image` sliced by `slice` and stretched to fill `dest`, with
    /// `center` controlling whether the middle segment is drawn or left hollow.
    Frame {
        image: std::sync::Arc<crate::asset::DecodedImage>,
        dest: ImageDest,
        slice: Slice,
        center: FrameCenter,
    },
    /// A host-content region: a named rectangle (`id`) the embedder fills with its own texture
    /// (e.g. cover art or a live view), composited over the scene at `dest`.
    View { id: String, dest: ImageDest },
    /// A scrollable/selectable row list bound to a host `collection`. In the design scene
    /// `count` is 0; layout expansion fills it in and appends generated row `Text`/highlight
    /// nodes.
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
    /// A draggable/clickable seek bar: reads a host scalar at `value_key` to draw its fill
    /// (growing along `direction`), and dispatches `on_seek` with a 0..1 click fraction.
    Scrub {
        region: ImageDest,
        value_key: String,
        direction: FillDir,
        color: Color,
        on_seek: String,
    },
    /// A run of shaped text: either `content`'s static string or a bound host value, drawn with
    /// the given font/size/paint/alignment at `pos`.
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

/// Where a scene node came from in the skin source. Metadata only — the renderer and
/// hit-test ignore it. Populated at load; carried through `layout`. See
/// `docs/superpowers/specs/2026-07-03-scene-node-provenance-design.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Origin {
    /// 1-based line of the primitive call in the skin's entry Lua, if known.
    pub line: Option<u32>,
    /// Monotonic index of the primitive call that emitted this node. Nodes from the
    /// same call (e.g. a `fill{}` with `on_press` emits Fill + Hotspot) share it.
    /// `None` for engine-generated nodes (list rows, selection highlight).
    pub call: Option<u32>,
}

/// An ordered list of drawable/interactive [`Node`]s plus the canvas they're authored (design
/// scene) or resolved (logical scene, via `layout`/`layout_with_origins`) against. Later nodes
/// draw on top and win hit-tests.
#[derive(Clone, Debug)]
pub struct Scene {
    /// The scene's nodes, in draw order (first = bottom, last = top).
    pub nodes: Vec<Node>,
    /// The canvas size this scene's coordinates are authored/resolved against: the skin's
    /// declared size for a design scene, or the requested logical size for a resolved one.
    pub canvas: (u32, u32),
}

/// Build a single-contour [`Region`] from a polygon path (cached into `Hotspot` nodes) for
/// point-in-polygon hit-testing.
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
                } => {
                    if let Some(idx) = list_row_index_at(region, *row_height, *count, p) {
                        return Some(Hit::Row {
                            action: action.clone(),
                            index: idx,
                        });
                    }
                }
                Node::Scrub {
                    region, on_seek, ..
                } if scrub_contains(region, p) => {
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

    /// Index of the topmost (last in z-order) node whose bounding box contains `p`. Zero-area
    /// nodes (Text has no measured size) are skipped. This is a scene-level pick for authoring
    /// tools — broader than `hit_any`, which dispatches only interactive kinds. Call on a
    /// layout-resolved scene so bounds are in logical coordinates.
    pub fn pick(&self, p: Pt) -> Option<usize> {
        self.nodes.iter().enumerate().rev().find_map(|(i, node)| {
            let b = crate::layout::node_bbox(node)?;
            let inside = b.w > 0.0
                && b.h > 0.0
                && p.x >= b.x
                && p.x <= b.x + b.w
                && p.y >= b.y
                && p.y <= b.y + b.h;
            inside.then_some(i)
        })
    }

    /// True if `p` falls inside any drawn node's bounds — the skin's opaque coverage geometry.
    /// Rect-bounded nodes use their dest/region; polygon nodes use `region_of(path)`. `Text` is
    /// ignored (no reliable glyph bounds). This is the geometry a host uses for a shaped-window /
    /// click-through mask.
    pub fn covers(&self, p: Pt) -> bool {
        let inside_rect =
            |x: f32, y: f32, w: f32, h: f32| p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h;
        self.nodes.iter().any(|node| match node {
            Node::Hotspot { region, .. } => region.contains(Point { x: p.x, y: p.y }),
            Node::Fill { path, .. } | Node::ValueFill { path, .. } => {
                region_of(path).contains(Point { x: p.x, y: p.y })
            }
            Node::Image { dest, .. } | Node::Frame { dest, .. } | Node::View { dest, .. } => {
                inside_rect(dest.x, dest.y, dest.w, dest.h)
            }
            Node::List { region, .. } | Node::Scrub { region, .. } => {
                inside_rect(region.x, region.y, region.w, region.h)
            }
            Node::Text { .. } => false,
        })
    }

    /// Classify `p` for a host embedder WITHOUT firing any Lua handler (unlike
    /// `handle_pointer_resolved`). Topmost interactive node decides; otherwise opaque coverage vs.
    /// transparent. See [`HitKind`].
    pub fn hit_kind(&self, p: Pt) -> HitKind {
        let pt = Point { x: p.x, y: p.y };
        for node in self.nodes.iter().rev() {
            match node {
                Node::Hotspot { region, role, .. } if region.contains(pt) => {
                    return match role {
                        HotspotRole::Drag => HitKind::Drag,
                        HotspotRole::Passthrough => HitKind::Passthrough,
                        HotspotRole::Control => HitKind::Control,
                    };
                }
                Node::List {
                    region,
                    row_height,
                    on_select: Some(_),
                    count,
                    ..
                } if list_row_index_at(region, *row_height, *count, p).is_some() => {
                    return HitKind::Control;
                }
                Node::Scrub { region, .. } if scrub_contains(region, p) => {
                    return HitKind::Control;
                }
                _ => {}
            }
        }
        if self.covers(p) {
            HitKind::Control
        } else {
            HitKind::Passthrough
        }
    }
}

/// Row index of a `list{}` hit at `p`, or `None` if `p` misses the list's bounds or lands past
/// `count` rows. Shared by `hit_any` (which needs the index) and `hit_kind` (which only needs to
/// know whether a row was hit) — see both call sites for the exact bounds this must preserve.
fn list_row_index_at(region: &ImageDest, row_height: f32, count: usize, p: Pt) -> Option<usize> {
    if row_height > 0.0
        && count > 0
        && p.x >= region.x
        && p.x <= region.x + region.w
        && p.y >= region.y
    {
        let idx = ((p.y - region.y) / row_height).floor() as usize;
        if idx < count {
            return Some(idx);
        }
    }
    None
}

/// True if `p` falls within a `scrub{}` node's rectangular bounds. Shared by `hit_any` and
/// `hit_kind`.
fn scrub_contains(region: &ImageDest, p: Pt) -> bool {
    p.x >= region.x && p.x <= region.x + region.w && p.y >= region.y && p.y <= region.y + region.h
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

/// Coarse interaction classification of a point for a host embedder — see [`Scene::hit_kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitKind {
    /// Event should fall through the skin (transparent, or a `role=passthrough` region).
    Passthrough,
    /// The skin consumes the event (a control, or opaque non-interactive skin).
    Control,
    /// Host should move the window (a `role=drag` region).
    Drag,
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

    #[test]
    fn pick_returns_topmost_node_by_bbox() {
        fn fill(pts: &[(f32, f32)]) -> Node {
            Node::Fill {
                path: pts.iter().map(|&(x, y)| Pt { x, y }).collect(),
                paint: Paint::Solid(Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
            }
        }
        let scene = Scene {
            nodes: vec![
                fill(&[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]), // big, drawn first
                fill(&[(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)]), // small, drawn on top
            ],
            canvas: (100, 100),
        };
        assert_eq!(scene.pick(Pt { x: 20.0, y: 20.0 }), Some(1), "topmost wins");
        assert_eq!(
            scene.pick(Pt { x: 80.0, y: 80.0 }),
            Some(0),
            "only the big fill"
        );
        assert_eq!(scene.pick(Pt { x: 200.0, y: 200.0 }), None, "empty space");
        // pick uses inclusive <=/>= bounds, so a point exactly on the big fill's bbox edge
        // (100,100) is still contained.
        assert_eq!(
            scene.pick(Pt { x: 100.0, y: 100.0 }),
            Some(0),
            "point on bbox edge is inclusive"
        );
    }

    #[test]
    fn pick_skips_zero_area_nodes() {
        // A degenerate single-point path has a zero-area bbox — same as a Text node (which
        // node_bbox reports as a zero-size point). Neither is pickable.
        let scene = Scene {
            nodes: vec![Node::Fill {
                path: vec![Pt { x: 5.0, y: 5.0 }],
                paint: Paint::Solid(Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
            }],
            canvas: (50, 50),
        };
        assert_eq!(scene.pick(Pt { x: 5.0, y: 5.0 }), None);
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

    #[test]
    fn hit_kind_classifies_drag_control_and_passthrough() {
        // A 100x100 canvas: a drag hotspot over the left half, an opaque fill over the right
        // half. No test `DecodedImage` constructor exists, so a `Node::Fill` stands in for the
        // opaque image — `covers`/`hit_kind` treat any `Fill`'s `region_of(path)` as opaque.
        let drag = Node::Hotspot {
            region: region_of(&[
                Pt { x: 0.0, y: 0.0 },
                Pt { x: 50.0, y: 0.0 },
                Pt { x: 50.0, y: 100.0 },
                Pt { x: 0.0, y: 100.0 },
            ]),
            on_press: 0,
            role: HotspotRole::Drag,
        };
        let fill = Node::Fill {
            path: vec![
                Pt { x: 50.0, y: 0.0 },
                Pt { x: 100.0, y: 0.0 },
                Pt { x: 100.0, y: 100.0 },
                Pt { x: 50.0, y: 100.0 },
            ],
            paint: Paint::Solid(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        };
        let scene = Scene {
            nodes: vec![drag, fill],
            canvas: (100, 100),
        };

        assert_eq!(scene.hit_kind(Pt { x: 10.0, y: 50.0 }), HitKind::Drag); // over drag hotspot
        assert_eq!(scene.hit_kind(Pt { x: 75.0, y: 50.0 }), HitKind::Control); // over opaque fill
        assert_eq!(
            scene.hit_kind(Pt { x: 200.0, y: 200.0 }),
            HitKind::Passthrough
        ); // outside all nodes
        assert!(scene.covers(Pt { x: 75.0, y: 50.0 }));
        assert!(!scene.covers(Pt { x: 200.0, y: 200.0 }));
    }

    #[test]
    fn hit_kind_classifies_passthrough_role_hotspot() {
        // A `role="passthrough"` hotspot must classify as `Passthrough` even though the point is
        // inside its region — the whole point of the role is to let the event fall through.
        let scene = Scene {
            canvas: (100, 100),
            nodes: vec![Node::Hotspot {
                region: region_of(&[
                    Pt { x: 0.0, y: 0.0 },
                    Pt { x: 100.0, y: 0.0 },
                    Pt { x: 100.0, y: 100.0 },
                    Pt { x: 0.0, y: 100.0 },
                ]),
                on_press: 0,
                role: HotspotRole::Passthrough,
            }],
        };
        assert_eq!(
            scene.hit_kind(Pt { x: 50.0, y: 50.0 }),
            HitKind::Passthrough
        );
    }

    #[test]
    fn hit_kind_classifies_list_row_as_control() {
        let s = list_scene(3, Some("open"));
        assert_eq!(s.hit_kind(Pt { x: 50.0, y: 10.0 }), HitKind::Control);
    }

    #[test]
    fn hit_kind_classifies_scrub_as_control() {
        let s = scrub_scene();
        assert_eq!(s.hit_kind(Pt { x: 50.0, y: 10.0 }), HitKind::Control);
    }
}

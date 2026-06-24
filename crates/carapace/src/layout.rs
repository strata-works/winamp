//! GPU-free layout resolution for frame skins. Resolves per-element anchors against the current
//! window size, producing concrete logical rects. Pure geometry — no GPU, no engine state.

use crate::scene::{ImageDest, Node, Pt, Scene};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Which window edges an element is pinned to (gap held constant), plus an optional stretch floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchors {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    /// Minimum (w, h) a stretched element collapses to. 0 on an axis = no floor.
    pub min: Option<(f32, f32)>,
}

impl Anchors {
    /// The default: fixed size, pinned to the top-left — identical to pre-anchor behavior.
    pub const TOP_LEFT: Anchors = Anchors {
        left: true,
        right: false,
        top: true,
        bottom: false,
        min: None,
    };

    /// Build from a list of edge names (`"left"`, `"right"`, `"top"`, `"bottom"`); unknown ignored.
    pub fn from_edges(edges: &[&str]) -> Anchors {
        Anchors {
            left: edges.contains(&"left"),
            right: edges.contains(&"right"),
            top: edges.contains(&"top"),
            bottom: edges.contains(&"bottom"),
            min: None,
        }
    }
}

/// Resolve one axis: origin `p`, extent `e`, design length `d`, logical length `l`, pins
/// `(near, far)`, floor `min_e`. Returns `(p', e')`.
fn resolve_axis(p: f32, e: f32, d: f32, l: f32, near: bool, far: bool, min_e: f32) -> (f32, f32) {
    let delta = l - d;
    let (mut np, mut ne) = match (near, far) {
        (true, true) => (p, e + delta),  // both gaps fixed -> stretch
        (true, false) => (p, e),         // near gap fixed
        (false, true) => (p + delta, e), // far gap fixed -> rides far edge
        (false, false) => (p * (l / d.max(1.0)), e), // proportional re-center
    };
    if ne < min_e {
        ne = min_e;
    }
    if ne < 0.0 {
        ne = 0.0;
    }
    if !np.is_finite() {
        np = p;
    }
    (np, ne)
}

/// Resolve a design-space bounding box to a logical bounding box under its anchors.
pub fn resolve_bbox(design: (f32, f32), logical: (f32, f32), bbox: Rect, a: Anchors) -> Rect {
    let (min_w, min_h) = a.min.unwrap_or((0.0, 0.0));
    let (x, w) = resolve_axis(bbox.x, bbox.w, design.0, logical.0, a.left, a.right, min_w);
    let (y, h) = resolve_axis(bbox.y, bbox.h, design.1, logical.1, a.top, a.bottom, min_h);
    Rect { x, y, w, h }
}

/// The design-space bounding box of a node (rect for rect-nodes; point-bbox for text; path bbox
/// otherwise). Returns `None` for nodes without geometry to resolve.
fn node_bbox(node: &Node) -> Option<Rect> {
    fn path_bbox(path: &[Pt]) -> Option<Rect> {
        let xs = path.iter().map(|p| p.x);
        let ys = path.iter().map(|p| p.y);
        let x0 = xs.clone().fold(f32::INFINITY, f32::min);
        let x1 = xs.fold(f32::NEG_INFINITY, f32::max);
        let y0 = ys.clone().fold(f32::INFINITY, f32::min);
        let y1 = ys.fold(f32::NEG_INFINITY, f32::max);
        if x0.is_finite() && x1.is_finite() {
            Some(Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            })
        } else {
            None
        }
    }
    match node {
        Node::Image { dest, .. } | Node::View { dest, .. } | Node::Frame { dest, .. } => {
            Some(Rect {
                x: dest.x,
                y: dest.y,
                w: dest.w,
                h: dest.h,
            })
        }
        Node::List { region, .. } => Some(Rect {
            x: region.x,
            y: region.y,
            w: region.w,
            h: region.h,
        }),
        Node::Fill { path, .. } | Node::ValueFill { path, .. } => path_bbox(path),
        Node::Hotspot { region, .. } => {
            let pts: Vec<Pt> = region
                .contours
                .iter()
                .flat_map(|c| c.points.iter().map(|p| Pt { x: p.x, y: p.y }))
                .collect();
            path_bbox(&pts)
        }
        Node::Text { pos, .. } => Some(Rect {
            x: pos.x,
            y: pos.y,
            w: 0.0,
            h: 0.0,
        }),
    }
}

/// Apply a design->logical (translate + per-axis scale) transform to a node's geometry.
fn transform_node(node: &Node, from: Rect, to: Rect) -> Node {
    let sx = if from.w.abs() > f32::EPSILON {
        to.w / from.w
    } else {
        1.0
    };
    let sy = if from.h.abs() > f32::EPSILON {
        to.h / from.h
    } else {
        1.0
    };
    let map = |p: Pt| Pt {
        x: to.x + (p.x - from.x) * sx,
        y: to.y + (p.y - from.y) * sy,
    };
    let map_path = |path: &[Pt]| path.iter().map(|p| map(*p)).collect::<Vec<_>>();
    let mut n = node.clone();
    match &mut n {
        Node::Image { dest, .. } | Node::View { dest, .. } | Node::Frame { dest, .. } => {
            *dest = ImageDest {
                x: to.x,
                y: to.y,
                w: to.w,
                h: to.h,
            };
        }
        Node::List { region, .. } => {
            *region = ImageDest {
                x: to.x,
                y: to.y,
                w: to.w,
                h: to.h,
            };
        }
        Node::Fill { path, .. } | Node::ValueFill { path, .. } => {
            *path = map_path(path);
        }
        Node::Hotspot { region, .. } => {
            for c in &mut region.contours {
                for p in &mut c.points {
                    let m = map(Pt { x: p.x, y: p.y });
                    p.x = m.x;
                    p.y = m.y;
                }
            }
        }
        Node::Text { pos, .. } => {
            *pos = map(*pos);
        }
    }
    n
}

/// Resolve a design scene to a logical scene: each node's geometry is reflowed by its anchors,
/// and the result's `canvas` is set to the logical size (so the renderer scales it by DPI only).
pub fn resolve_scene(design: &Scene, anchors: &[Anchors], logical: (f32, f32)) -> Scene {
    let d = (design.canvas.0 as f32, design.canvas.1 as f32);
    let nodes = design
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let a = anchors.get(i).copied().unwrap_or(Anchors::TOP_LEFT);
            match node_bbox(node) {
                Some(bbox) => {
                    let to = resolve_bbox(d, logical, bbox, a);
                    transform_node(node, bbox, to)
                }
                None => node.clone(),
            }
        })
        .collect();
    Scene {
        nodes,
        canvas: (
            logical.0.round().max(1.0) as u32,
            logical.1.round().max(1.0) as u32,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImageDest, Node, Scene};

    #[test]
    fn list_region_stretches_under_full_anchors() {
        let design = Scene {
            canvas: (200, 100),
            nodes: vec![Node::List {
                collection: "c".to_string(),
                region: ImageDest {
                    x: 10.0,
                    y: 10.0,
                    w: 180.0,
                    h: 80.0,
                },
                row_height: 20.0,
                on_select: None,
                count: 0,
                template: vec![],
            }],
        };
        let anchors = vec![Anchors {
            left: true,
            right: true,
            top: true,
            bottom: true,
            min: None,
        }];
        let resolved = resolve_scene(&design, &anchors, (300.0, 140.0));
        match &resolved.nodes[0] {
            Node::List { region, .. } => {
                assert_eq!(region.w, 280.0, "w stretched by +100");
                assert_eq!(region.h, 120.0, "h stretched by +40");
            }
            other => panic!("expected List, got {other:?}"),
        }
    }
}

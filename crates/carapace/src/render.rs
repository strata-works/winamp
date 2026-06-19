use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{Color as VColor, Fill};
use vello::{AaConfig, RenderParams, Scene as VScene};

use crate::scene::{Color, Node, Pt, Scene};
use crate::state::StateValue;

pub struct RenderTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

pub struct Renderer {
    inner: vello::Renderer,
}

fn vcolor(c: Color) -> VColor {
    VColor::from_rgba8(c.r, c.g, c.b, 255)
}

fn bez(path: &[Pt]) -> BezPath {
    let mut bp = BezPath::new();
    if let Some((first, rest)) = path.split_first() {
        bp.move_to(KPoint::new(first.x as f64, first.y as f64));
        for p in rest {
            bp.line_to(KPoint::new(p.x as f64, p.y as f64));
        }
        bp.close_path();
    }
    bp
}

fn bbox(path: &[Pt]) -> (f64, f64, f64, f64) {
    let xs = path.iter().map(|p| p.x as f64);
    let ys = path.iter().map(|p| p.y as f64);
    (
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    )
}

fn value_of(read: &impl Fn(&str) -> Option<StateValue>, key: &str) -> f64 {
    match read(key) {
        Some(StateValue::Scalar(v)) => v.clamp(0.0, 1.0) as f64,
        Some(StateValue::Bool(true)) => 1.0,
        _ => 0.0,
    }
}

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let inner = vello::Renderer::new(
            device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .expect("create vello renderer");
        Self { inner }
    }

    pub fn draw(
        &mut self,
        scene: &Scene,
        read_value: impl Fn(&str) -> Option<StateValue>,
        target: &RenderTarget,
    ) {
        let (cw, ch) = scene.canvas;
        // Uniform canvas -> surface scale so the skin fills the window.
        let sx = target.width as f64 / cw.max(1) as f64;
        let sy = target.height as f64 / ch.max(1) as f64;
        let xform = Affine::scale_non_uniform(sx, sy);

        let mut vs = VScene::new();
        for node in &scene.nodes {
            match node {
                Node::Fill { path, color } => {
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &bez(path));
                }
                Node::Hotspot { .. } => {} // invisible
                Node::ValueFill {
                    path,
                    value_key,
                    color,
                } => {
                    let v = value_of(&read_value, value_key);
                    let (x0, y0, x1, y1) = bbox(path);
                    let filled = Rect::new(x0, y0, x0 + (x1 - x0) * v, y1);
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &filled);
                }
                Node::Image { .. } => {} // rendering not yet implemented
            }
        }

        self.inner
            .render_to_texture(
                target.device,
                target.queue,
                &vs,
                target.view,
                &RenderParams {
                    base_color: VColor::from_rgba8(0, 0, 0, 255),
                    width: target.width,
                    height: target.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("vello render_to_texture");
    }
}

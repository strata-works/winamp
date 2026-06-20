use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{
    Blob, Brush, Color as VColor, Fill, Gradient as PGradient, ImageAlphaType, ImageBrush,
    ImageData, ImageFormat, ImageQuality,
};
use vello::{AaConfig, RenderParams, Scene as VScene};

use crate::scene::{Color, ColorStop, Gradient, Node, Paint, Pt, Scene};
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
    VColor::from_rgba8(c.r, c.g, c.b, c.a)
}

fn pstops(stops: &[ColorStop]) -> Vec<(f32, VColor)> {
    stops
        .iter()
        .map(|s| {
            (
                s.at,
                VColor::from_rgba8(s.color.r, s.color.g, s.color.b, s.color.a),
            )
        })
        .collect()
}

/// A peniko brush for a Paint.
fn paint_brush(paint: &Paint) -> Brush {
    match paint {
        Paint::Solid(c) => Brush::Solid(vcolor(*c)),
        Paint::Gradient(g) => Brush::Gradient(match g {
            Gradient::Linear { from, to, stops } => PGradient::new_linear(
                KPoint::new(from.x as f64, from.y as f64),
                KPoint::new(to.x as f64, to.y as f64),
            )
            .with_stops(&pstops(stops)[..]),
            Gradient::Radial {
                center,
                radius,
                stops,
            } => PGradient::new_radial(KPoint::new(center.x as f64, center.y as f64), *radius)
                .with_stops(&pstops(stops)[..]),
            Gradient::Sweep {
                center,
                start_deg,
                end_deg,
                stops,
            } => PGradient::new_sweep(
                KPoint::new(center.x as f64, center.y as f64),
                start_deg.to_radians(),
                end_deg.to_radians(),
            )
            .with_stops(&pstops(stops)[..]),
        }),
    }
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
                Node::Fill { path, paint } => {
                    vs.fill(Fill::NonZero, xform, &paint_brush(paint), None, &bez(path));
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
                Node::Image { image, dest } => {
                    // sRGB RGBA8 blob -> vello ImageData, placed at dest, under canvas->surface scale.
                    let blob = Blob::new(std::sync::Arc::new(image.rgba.clone()));
                    let vimg = ImageData {
                        data: blob,
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::Alpha,
                        width: image.width,
                        height: image.height,
                    };
                    // Scale native image size to dest.w x dest.h, then translate to dest.x, dest.y,
                    // all under the canvas->surface transform.
                    let place = Affine::translate((dest.x as f64, dest.y as f64))
                        * Affine::scale_non_uniform(
                            dest.w as f64 / image.width.max(1) as f64,
                            dest.h as f64 / image.height.max(1) as f64,
                        );
                    vs.draw_image(
                        ImageBrush::new(vimg)
                            .with_quality(ImageQuality::Medium)
                            .as_ref(),
                        xform * place,
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::value_of;
    use crate::state::StateValue;
    use std::sync::Arc;

    #[test]
    fn value_of_ignores_string_state() {
        let read = |_: &str| Some(StateValue::Str(Arc::from("not a number")));
        assert_eq!(value_of(&read, "k"), 0.0);
    }
}

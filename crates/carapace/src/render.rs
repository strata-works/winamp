use std::collections::HashMap;

use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{
    Blob, Brush, Color as VColor, Fill, Gradient as PGradient, ImageAlphaType, ImageBrush,
    ImageData, ImageFormat, ImageQuality,
};
use vello::{AaConfig, Glyph, RenderParams, Scene as VScene};

use crate::scene::{Color, ColorStop, Gradient, Node, Paint, Pt, Scene};
use crate::state::StateValue;

// Everything that affects shaping. `valign` is excluded — it's a draw-time offset, not a
// layout input — so vertically-different placements of identical text share one cached layout.
type LayoutKey = (u64, u32, crate::scene::HAlign, Option<u32>, String);

pub struct RenderTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub base_color: crate::scene::Color,
}

pub struct Renderer {
    inner: vello::Renderer,
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    // Content-addressed font id -> registered parley family name. Keeps system fallback for None
    // fonts (id 0). Keying on content (not the Arc address) means a freed font's address being
    // recycled by a different font across skin swaps can never alias the wrong family.
    families: HashMap<u64, String>,
    // Cache of shaped layouts so unchanged text is never re-shaped per frame (perf-priority).
    layouts: HashMap<LayoutKey, parley::Layout<Brush>>,
    // Composite pipeline: blits an embedder-supplied wgpu texture into a view{}'s rect.
    composite_pipeline: wgpu::RenderPipeline,
    composite_sampler: wgpu::Sampler,
    composite_bgl: wgpu::BindGroupLayout,
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

fn text_of(read: &impl Fn(&str) -> Option<StateValue>, key: &str) -> String {
    match read(key) {
        Some(StateValue::Str(s)) => s.to_string(),
        _ => String::new(),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("view-composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("view-composite-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("view-composite-pl"),
            bind_group_layouts: &[Some(&composite_bgl)],
            immediate_size: 0,
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("view-composite-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let composite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("view-composite-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            inner,
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            families: HashMap::new(),
            layouts: HashMap::new(),
            composite_pipeline,
            composite_sampler,
            composite_bgl,
        }
    }

    #[doc(hidden)]
    pub fn layout_cache_len(&self) -> usize {
        self.layouts.len()
    }

    pub fn draw<'v>(
        &mut self,
        scene: &Scene,
        read_value: impl Fn(&str) -> Option<StateValue>,
        view_tex: impl Fn(&str) -> Option<&'v wgpu::TextureView>,
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
                    direction,
                } => {
                    use crate::scene::FillDir;
                    let v = value_of(&read_value, value_key);
                    let (x0, y0, x1, y1) = bbox(path);
                    let (w, h) = (x1 - x0, y1 - y0);
                    let extent = match direction {
                        FillDir::Right => Rect::new(x0, y0, x0 + w * v, y1),
                        FillDir::Left => Rect::new(x1 - w * v, y0, x1, y1),
                        FillDir::Up => Rect::new(x0, y1 - h * v, x1, y1),
                        FillDir::Down => Rect::new(x0, y0, x1, y0 + h * v),
                    };
                    // Clip to the actual path, then fill the value-extent rect: result = path ∩ extent.
                    vs.push_clip_layer(Fill::NonZero, xform, &bez(path));
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &extent);
                    vs.pop_layer();
                }
                Node::View { .. } => {} // composited in the live-host-view-region render task
                Node::List { .. } => {} // expands to Text rows during layout; nothing to draw here
                Node::Scrub {
                    region,
                    value_key,
                    color,
                    direction,
                    ..
                } => {
                    use crate::scene::FillDir;
                    let v = value_of(&read_value, value_key);
                    let x0 = region.x as f64;
                    let y0 = region.y as f64;
                    let x1 = (region.x + region.w) as f64;
                    let y1 = (region.y + region.h) as f64;
                    let w = region.w as f64;
                    let h = region.h as f64;
                    let extent = match direction {
                        FillDir::Right => Rect::new(x0, y0, x0 + w * v, y1),
                        FillDir::Left => Rect::new(x1 - w * v, y0, x1, y1),
                        FillDir::Up => Rect::new(x0, y1 - h * v, x1, y1),
                        FillDir::Down => Rect::new(x0, y0, x1, y0 + h * v),
                    };
                    vs.fill(Fill::NonZero, xform, vcolor(*color), None, &extent);
                }
                Node::Frame {
                    image,
                    dest,
                    slice,
                    center,
                } => {
                    use crate::scene::FrameCenter;
                    let blob = Blob::new(std::sync::Arc::new(image.rgba.clone()));
                    let vimg = ImageData {
                        data: blob,
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::Alpha,
                        width: image.width,
                        height: image.height,
                    };
                    let iw = image.width as f64;
                    let ih = image.height as f64;
                    let dw = dest.w as f64;
                    let dh = dest.h as f64;

                    // Clamp insets so opposing corners never overlap in source or dest.
                    let mut sl = slice.left as f64;
                    let mut sr = slice.right as f64;
                    let mut st = slice.top as f64;
                    let mut sb = slice.bottom as f64;

                    let clamp_pair = |a: &mut f64, b: &mut f64, limit: f64| {
                        if *a + *b > limit && *a + *b > 0.0 {
                            let k = limit / (*a + *b);
                            *a *= k;
                            *b *= k;
                        }
                    };
                    {
                        let mut sl2 = sl;
                        let mut sr2 = sr;
                        clamp_pair(&mut sl2, &mut sr2, iw.min(dw));
                        sl = sl2;
                        sr = sr2;
                    }
                    {
                        let mut st2 = st;
                        let mut sb2 = sb;
                        clamp_pair(&mut st2, &mut sb2, ih.min(dh));
                        st = st2;
                        sb = sb2;
                    }

                    // Columns: (src_x, src_w, dst_x, dst_w) for left | center | right
                    let dx = dest.x as f64;
                    let dy = dest.y as f64;
                    let cols = [
                        (0.0, sl, dx, sl),
                        (sl, iw - sl - sr, dx + sl, dw - sl - sr),
                        (iw - sr, sr, dx + dw - sr, sr),
                    ];
                    let rows = [
                        (0.0, st, dy, st),
                        (st, ih - st - sb, dy + st, dh - st - sb),
                        (ih - sb, sb, dy + dh - sb, sb),
                    ];

                    for (ri, &(srcy, srch, dsty, dsth)) in rows.iter().enumerate() {
                        for (ci, &(srcx, srcw, dstx, dstw)) in cols.iter().enumerate() {
                            let is_center = ri == 1 && ci == 1;
                            if is_center && matches!(center, FrameCenter::Hollow) {
                                continue;
                            }
                            if srcw <= 0.0 || srch <= 0.0 || dstw <= 0.0 || dsth <= 0.0 {
                                continue;
                            }
                            vs.push_clip_layer(
                                Fill::NonZero,
                                xform,
                                &bez(&crate::shape::rect(
                                    dstx as f32,
                                    dsty as f32,
                                    dstw as f32,
                                    dsth as f32,
                                )),
                            );
                            let place = Affine::translate((dstx, dsty))
                                * Affine::scale_non_uniform(dstw / srcw, dsth / srch)
                                * Affine::translate((-srcx, -srcy));
                            vs.draw_image(
                                ImageBrush::new(vimg.clone())
                                    .with_quality(ImageQuality::Medium)
                                    .as_ref(),
                                xform * place,
                            );
                            vs.pop_layer();
                        }
                    }
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
                Node::Text {
                    content,
                    font,
                    size,
                    paint,
                    halign,
                    valign,
                    max_width,
                    pos,
                    ..
                } => {
                    use crate::scene::{HAlign, TextContent, VAlign};
                    let s = match content {
                        TextContent::Static(s) => s.clone(),
                        TextContent::Bound(k) => text_of(&read_value, k),
                    };
                    if s.is_empty() {
                        continue;
                    }

                    // Register the skin font once (keyed by content-addressed id); None => system
                    // default family (id 0, reserved only for "no font" so it can't alias a real one).
                    let font_id = font.as_ref().map(|f| f.id).unwrap_or(0);
                    let family = font.as_ref().map(|f| {
                        // Disjoint borrows of two distinct self fields — allowed since both are
                        // direct field accesses (not through a method).
                        let font_cx = &mut self.font_cx;
                        self.families
                            .entry(font_id)
                            .or_insert_with(|| {
                                let blob =
                                    vello::peniko::Blob::new(std::sync::Arc::new(f.bytes.to_vec()));
                                let registered = font_cx.collection.register_fonts(blob, None);
                                let id = registered[0].0;
                                font_cx
                                    .collection
                                    .family_name(id)
                                    .unwrap_or("system-ui")
                                    .to_string()
                            })
                            .clone()
                    });

                    // Build + cache the parley layout. Key covers everything that affects shaping
                    // (valign excluded — it's a draw offset). On a hit, no re-shaping happens.
                    let key: LayoutKey = (
                        font_id,
                        size.to_bits(),
                        *halign,
                        max_width.map(|w| w.to_bits()),
                        s.clone(),
                    );
                    if !self.layouts.contains_key(&key) {
                        let mut builder =
                            self.layout_cx
                                .ranged_builder(&mut self.font_cx, &s, 1.0, true);
                        builder.push_default(StyleProperty::FontSize(*size));
                        if let Some(fam) = &family {
                            // Named family picks the registered skin font; absent => default
                            // collection (system fonts) provides glyphs/fallback.
                            builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
                                FontFamilyName::Named(std::borrow::Cow::Owned(fam.clone())),
                            )));
                        }
                        let mut layout = builder.build(&s);
                        layout.break_all_lines(*max_width);
                        let align = match halign {
                            HAlign::Left => Alignment::Start,
                            HAlign::Center => Alignment::Center,
                            HAlign::Right => Alignment::End,
                        };
                        layout.align(align, AlignmentOptions::default());
                        self.layouts.insert(key.clone(), layout);
                    }
                    let layout = &self.layouts[&key];

                    // 2-D anchor offset from the block's measured size.
                    let block_w = max_width.unwrap_or(layout.width());
                    let off_x = match halign {
                        HAlign::Left => 0.0,
                        HAlign::Center => -block_w / 2.0,
                        HAlign::Right => -block_w,
                    };
                    let block_h = layout.height();
                    let off_y = match valign {
                        VAlign::Top => 0.0,
                        VAlign::Middle => -block_h / 2.0,
                        VAlign::Bottom => -block_h,
                    };
                    let origin =
                        Affine::translate(((pos.x + off_x) as f64, (pos.y + off_y) as f64));
                    let brush = paint_brush(paint);

                    for line in layout.lines() {
                        for item in line.items() {
                            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                                continue;
                            };
                            let run = glyph_run.run();
                            let mut gx = glyph_run.offset();
                            let gy = glyph_run.baseline();
                            let glyphs = glyph_run.glyphs().map(move |g| {
                                let gl = Glyph {
                                    id: g.id,
                                    x: gx + g.x,
                                    y: gy - g.y,
                                };
                                gx += g.advance;
                                gl
                            });
                            vs.draw_glyphs(run.font())
                                .font_size(run.font_size())
                                .brush(&brush)
                                // Text gradient brush coords are glyph-block-local by design: the
                                // `origin` translate applies to both glyphs and brush, so a `0..size`
                                // gradient spans the text height regardless of `pos`. Intentional for
                                // chrome numerals and what the reference skin assumes (not canvas-space).
                                .transform(xform * origin)
                                .draw(Fill::NonZero, glyphs);
                        }
                    }
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
                    base_color: VColor::from_rgba8(
                        target.base_color.r,
                        target.base_color.g,
                        target.base_color.b,
                        target.base_color.a,
                    ),
                    width: target.width,
                    height: target.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("vello render_to_texture");

        // Composite embedder-supplied content into each view's surface-space rect.
        let mut srcs: Vec<(Rect, &'v wgpu::TextureView)> = Vec::new();
        for node in &scene.nodes {
            if let Node::View { id, dest } = node
                && let Some(tex) = view_tex(id)
            {
                let r = Rect::new(
                    dest.x as f64 * sx,
                    dest.y as f64 * sy,
                    (dest.x + dest.w) as f64 * sx,
                    (dest.y + dest.h) as f64 * sy,
                );
                srcs.push((r, tex));
            }
        }
        if !srcs.is_empty() {
            let bgs: Vec<wgpu::BindGroup> = srcs
                .iter()
                .map(|(_, tex)| {
                    target.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("view-composite-bg"),
                        layout: &self.composite_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(tex),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.composite_sampler),
                            },
                        ],
                    })
                })
                .collect();
            let mut enc = target
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("view-composite-enc"),
                });
            {
                let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("view-composite-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                rp.set_pipeline(&self.composite_pipeline);
                for ((r, _), bg) in srcs.iter().zip(bgs.iter()) {
                    rp.set_viewport(
                        r.x0 as f32,
                        r.y0 as f32,
                        (r.x1 - r.x0) as f32,
                        (r.y1 - r.y0) as f32,
                        0.0,
                        1.0,
                    );
                    rp.set_bind_group(0, bg, &[]);
                    rp.draw(0..4, 0..1);
                }
            }
            target.queue.submit(Some(enc.finish()));
        }
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

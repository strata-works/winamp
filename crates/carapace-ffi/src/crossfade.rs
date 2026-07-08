//! `CrossfadeBlender` — a self-contained GPU pass that blends two `Rgba8Unorm` textures by an
//! alpha `t` into a target view (`out = mix(old, new, t)`). Used by the render thread's crossfade
//! swap; contains no engine or IOSurface knowledge, so it is unit-testable in isolation.
//!
//! The `t_new` texture is assumed to already match the destination's aspect (it is the incoming
//! skin rendered at the destination's native size). The `t_old` texture is the OUTGOING skin
//! rendered at ITS OWN aspect, so it is sampled through an `old_uv` transform that letterbox-fits
//! it into the destination WITHOUT stretching — the fix for the swap "squish" where a landscape
//! skin was distorted into a portrait target (and vice versa). In the letterbox bars (where the
//! outgoing skin does not reach) the incoming skin shows through, so a bar is never transparent.
//!
//! The render thread wires it into the live crossfade swap (see `render_thread::render_crossfade`).
#![cfg(any(target_os = "macos", target_os = "ios"))]

use crate::render::GpuCtx;

/// The WGSL for the blend: a fullscreen triangle whose fragment shader outputs `mix(old, new, t)`
/// inside the outgoing skin's letterbox rect, and the incoming skin alone in the bars. The uniform
/// packs `params.x = t` and `old_uv = (scale_u, scale_v, offset_u, offset_v)` — the aspect-fit
/// transform applied to the outgoing texture's UVs. Identity `old_uv = (1, 1, 0, 0)` reproduces the
/// original full-frame `mix(old, new, t)` everywhere (same-size swaps take that path).
const SHADER: &str = r#"
@group(0) @binding(0) var t_old: texture_2d<f32>;
@group(0) @binding(1) var t_new: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
struct U { params: vec4<f32>, old_uv: vec4<f32> };
@group(0) @binding(3) var<uniform> u: U;

struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var xy = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var out: VsOut;
    let p = xy[i];
    out.pos = vec4(p, 0.0, 1.0);
    out.uv = vec2((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let ouv = in.uv * u.old_uv.xy + u.old_uv.zw;
    let b = textureSample(t_new, samp, in.uv);
    // Outside the outgoing skin's aspect-fit rect → show the incoming skin alone (letterbox bar).
    let in_rect = all(ouv >= vec2(0.0)) && all(ouv <= vec2(1.0));
    let a = textureSample(t_old, samp, clamp(ouv, vec2(0.0), vec2(1.0)));
    return select(b, mix(a, b, u.params.x), in_rect);
}
"#;

/// Blends two `Rgba8Unorm` textures by an alpha into a target `Rgba8Unorm` view. Built once and
/// reused for every crossfade frame; the per-frame `draw` writes `t` into a uniform and re-binds
/// the (stable) source views.
pub struct CrossfadeBlender {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
}

impl CrossfadeBlender {
    /// Build the blend pipeline against the offscreen format (`Rgba8Unorm`).
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crossfade"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crossfade-bgl"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crossfade-pl"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crossfade-pipeline"),
            layout: Some(&pl),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("crossfade-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crossfade-u"),
            size: 32, // two vec4<f32>: params (t) + old_uv aspect-fit transform
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            sampler,
            uniform,
        }
    }

    /// Blend the outgoing (`old_view`) and incoming (`new_view`) textures into `dst_view` by alpha
    /// `t`, sampling the outgoing texture through `old_uv = (scale_u, scale_v, offset_u, offset_v)`
    /// so it aspect-fits without stretching. Pass identity `(1.0, 1.0, 0.0, 0.0)` for a same-aspect
    /// blend. Submits its own encoder; ordering with the downstream present (blit/readback of `dst`)
    /// is guaranteed by same-queue submission order.
    pub fn draw(
        &self,
        gpu: &GpuCtx,
        old_view: &wgpu::TextureView,
        new_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        t: f32,
        old_uv: [f32; 4],
    ) {
        // Uniform: params (t in .x, rest padding) then old_uv (aspect-fit transform).
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&t.to_le_bytes());
        for (i, v) in old_uv.iter().enumerate() {
            let off = 16 + i * 4;
            bytes[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        gpu.queue.write_buffer(&self.uniform, 0, &bytes);

        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crossfade-bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(old_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(new_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });

        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crossfade-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([enc.finish()]);
    }
}

#[cfg(all(test, target_os = "macos", feature = "gpu-tests"))]
mod tests {
    use super::*;
    use crate::render::init_gpu;

    fn solid(gpu: &GpuCtx, w: u32, h: u32, rgba: [u8; 4]) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("solid"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: Vec<u8> = std::iter::repeat_n(rgba, (w * h) as usize)
            .flatten()
            .collect();
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    #[test]
    fn blends_two_solids_at_half() {
        let gpu = init_gpu().expect("gpu");
        let (w, h) = (4u32, 4u32);
        let (_r, red) = solid(&gpu, w, h, [255, 0, 0, 255]);
        let (_b, blue) = solid(&gpu, w, h, [0, 0, 255, 255]);
        let dst = crate::render::new_offscreen(&gpu.device, w, h);

        let blender = CrossfadeBlender::new(&gpu.device);
        blender.draw(&gpu, &red, &blue, &dst.view, 0.5, [1.0, 1.0, 0.0, 0.0]);

        let px = crate::render::readback_rgba(&gpu, &dst.tex, w, h);
        // mix(red, blue, 0.5) ≈ (128, 0, 128). Allow rounding slack.
        assert!((px[0] as i32 - 128).abs() <= 4, "R was {}", px[0]);
        assert_eq!(px[1], 0, "G");
        assert!((px[2] as i32 - 128).abs() <= 4, "B was {}", px[2]);
    }

    #[test]
    fn letterbox_bar_shows_incoming_only() {
        // A `old_uv` that fits the old texture into the central 50% of height (bars top & bottom):
        // scale_v = 2, offset_v = -0.5 → rows near y=0 and y=h map outside [0,1] (bars). Those rows
        // must show the INCOMING (blue) alone regardless of `t`; the central band blends.
        let gpu = init_gpu().expect("gpu");
        let (w, h) = (4u32, 8u32);
        let (_r, red) = solid(&gpu, w, h, [255, 0, 0, 255]); // outgoing
        let (_b, blue) = solid(&gpu, w, h, [0, 0, 255, 255]); // incoming
        let dst = crate::render::new_offscreen(&gpu.device, w, h);

        let blender = CrossfadeBlender::new(&gpu.device);
        // t = 0 → central band = old (red); bars = incoming (blue) even though t=0.
        blender.draw(&gpu, &red, &blue, &dst.view, 0.0, [1.0, 2.0, 0.0, -0.5]);
        let px = crate::render::readback_rgba(&gpu, &dst.tex, w, h);
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2]]
        };
        // Top bar (y=0): incoming blue, not the outgoing red.
        assert_eq!(at(0, 0), [0, 0, 255], "top bar shows incoming");
        // Bottom bar (y=h-1): incoming blue.
        assert_eq!(at(0, h - 1), [0, 0, 255], "bottom bar shows incoming");
        // Central band (y=h/2): t=0 → outgoing red.
        assert_eq!(at(0, h / 2), [255, 0, 0], "central band shows outgoing at t=0");
    }
}

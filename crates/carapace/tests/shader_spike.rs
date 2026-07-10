#![cfg(feature = "gpu-tests")]

// THROWAWAY SPIKE (sub-project 1, Task 1): de-risks the crux of the engine `shader{}` primitive
// BEFORE any production code exists. Proves two things with real GPU pixels:
//   1. vello can render 2D content into a TRANSPARENT offscreen (not just an opaque one).
//   2. that transparent offscreen composites CORRECTLY OVER a pre-filled "shader background"
//      target using premultiplied-alpha blending, i.e. the 4-stage order
//      (shader bg -> target) -> (vello 2D -> transparent offscreen) -> (composite offscreen
//      OVER target) yields "2D drawn over an animated shader background".
// Nothing later depends on this file's code; only its findings (see the sibling findings doc)
// gate the design of the real `shader{}` primitive.

use std::time::Instant;

// Build a device + an offscreen Rgba8Unorm storage texture, render, read back.
// (Copied verbatim from render_offscreen.rs:8-105 — the standard device+readback rig.)
struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

fn offscreen(w: u32, h: u32) -> Offscreen {
    offscreen_with_usage(
        w,
        h,
        wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
    )
}

fn offscreen_with_usage(w: u32, h: u32, usage: wgpu::TextureUsages) -> Offscreen {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("no wgpu adapter (need a GPU or lavapipe)");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("device");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Offscreen {
            device,
            queue,
            texture,
            view,
            w,
            h,
        }
    })
}

// Create a second render target texture ON THE SAME DEVICE as `o` (textures/views can't cross
// devices — a fresh `offscreen()` call would spin up its own adapter+device). Adds
// TEXTURE_BINDING so it can also be sampled as the Stage-3 composite source.
fn same_device_target(o: &Offscreen, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = o.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("off2"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

// Read the texture back into tight RGBA8 (256-byte-aligned readback — see vello_backend.rs).
fn readback(o: &Offscreen) -> Vec<u8> {
    pollster::block_on(async {
        let unpadded = o.w * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buf = o.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rb"),
            size: (padded * o.h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = o
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &o.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(o.h),
                },
            },
            wgpu::Extent3d {
                width: o.w,
                height: o.h,
                depth_or_array_layers: 1,
            },
        );
        o.queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        o.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let mapped = slice.get_mapped_range();
        let mut data = Vec::with_capacity((unpadded * o.h) as usize);
        for row in 0..o.h {
            let start = (row * padded) as usize;
            data.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buf.unmap();
        data
    })
}

fn px(data: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2]]
}

// Build the composite pipeline: a fullscreen-quad blit of `off2` sampled with
// premultiplied-alpha blending over whatever is already in the target (`LoadOp::Load`).
// This is the same shader source as `crates/carapace/src/composite.wgsl` (copied inline per the
// brief — the spike is throwaway and self-contained).
const COMPOSITE_WGSL: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
    var uv = array<vec2<f32>, 4>(vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(0.0, 0.0), vec2(1.0, 0.0));
    var o: VsOut;
    o.pos = vec4(p[vi], 0.0, 1.0);
    o.uv = uv[vi];
    return o;
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src, samp, in.uv);
}
"#;

struct CompositePipeline {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bgl: wgpu::BindGroupLayout,
}

fn build_composite_pipeline(device: &wgpu::Device) -> CompositePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("spike-composite"),
        source: wgpu::ShaderSource::Wgsl(COMPOSITE_WGSL.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("spike-composite-bgl"),
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
        label: Some("spike-composite-pl"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("spike-composite-pipeline"),
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
                // Vello outputs premultiplied RGBA into the transparent offscreen, so composite
                // it OVER the pre-filled background with premultiplied-alpha blending.
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("spike-composite-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    CompositePipeline {
        pipeline,
        sampler,
        bgl,
    }
}

// Composite `src_view` OVER `target_view` in-place, using `LoadOp::Load` (keep what's already
// there) + premultiplied-alpha blending.
fn composite_over(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cp: &CompositePipeline,
    src_view: &wgpu::TextureView,
    target_view: &wgpu::TextureView,
) {
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("spike-composite-bg"),
        layout: &cp.bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&cp.sampler),
            },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("spike-composite-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // keep the pre-filled "shader background"
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&cp.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
    queue.submit(Some(enc.finish()));
}

// Proves: (bg shader) -> target, (vello 2D) -> transparent offscreen, composite offscreen OVER
// target, yields 2D-over-shader-background. Uses a hardcoded solid-color "shader" (a clear) as
// the stand-in background so the test isolates the COMPOSITING ORDER, not shader authoring.
#[test]
fn four_stage_composites_2d_over_shader_background() {
    let (w, h) = (64u32, 64u32);
    let o = offscreen(w, h); // RENDER_ATTACHMENT target, Rgba8Unorm
    let cp = build_composite_pipeline(&o.device);

    // Stage 1: "shader" background — clear the target to solid blue via a render pass.
    let clear_bg = || {
        let mut enc = o.device.create_command_encoder(&Default::default());
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bg"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &o.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        o.queue.submit(Some(enc.finish()));
    };
    clear_bg();

    // Stage 2: vello 2D into a TRANSPARENT offscreen (a red fill covering the left half).
    let (_off2_texture, off2_view) = same_device_target(&o, w, h);
    let mut r = carapace::render::Renderer::new(&o.device);
    let scene = carapace::scene::Scene {
        canvas: (w, h),
        nodes: vec![carapace::scene::Node::Fill {
            path: vec![
                carapace::scene::Pt { x: 0.0, y: 0.0 },
                carapace::scene::Pt {
                    x: (w as f32) / 2.0,
                    y: 0.0,
                },
                carapace::scene::Pt {
                    x: (w as f32) / 2.0,
                    y: h as f32,
                },
                carapace::scene::Pt {
                    x: 0.0,
                    y: h as f32,
                },
            ],
            paint: carapace::scene::Paint::Solid(carapace::scene::Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
        }],
    };
    let draw_2d = |r: &mut carapace::render::Renderer| {
        r.draw(
            &scene,
            |_| None,
            |_| None,
            &carapace::render::RenderTarget {
                device: &o.device,
                queue: &o.queue,
                view: &off2_view,
                width: w,
                height: h,
                base_color: carapace::scene::Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                }, // TRANSPARENT — key to the spike
            },
        );
    };
    draw_2d(&mut r);

    // Stage 3: composite off2 OVER the target with premultiplied-alpha blending.
    composite_over(&o.device, &o.queue, &cp, &off2_view, &o.view);

    // Assert against the ACTUAL composited target (o.view), not a placeholder.
    let composited = readback(&o);
    // Right half (2D transparent there) shows the blue background:
    assert_eq!(
        px(&composited, w, 3 * w / 4, h / 2),
        [0, 0, 255],
        "right half must be untouched blue background"
    );
    // Left half (red 2D) shows red OVER the blue background:
    assert_eq!(
        px(&composited, w, w / 4, h / 2),
        [255, 0, 0],
        "left half must be opaque red over the blue background"
    );

    // Perf: time 100 iterations of (vello->offscreen + composite) and print the mean ms.
    // Each iteration redoes stages 1-3 (bg clear, vello draw into transparent offscreen,
    // composite-over). Two variants, both worth recording:
    //   (a) blocking: poll-to-completion after every iteration — measures true GPU execution
    //       time per frame, but forces a CPU/GPU sync bubble every iteration (no pipelining
    //       across frames), so it's a pessimistic upper bound vs. a real swapchain loop.
    //   (b) pipelined: submit all iterations back-to-back, poll once at the end — lets the
    //       driver overlap CPU submission of frame N+1 with GPU execution of frame N, which is
    //       what a real 60fps render loop does. This is the more realistic per-frame estimate.
    let iters = 100;

    let start = Instant::now();
    for _ in 0..iters {
        clear_bg();
        draw_2d(&mut r);
        composite_over(&o.device, &o.queue, &cp, &off2_view, &o.view);
        o.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
    let blocking_mean_ms = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;
    eprintln!("4-stage add'l per-frame (blocking, no pipelining): {blocking_mean_ms:.3} ms");

    let start = Instant::now();
    for _ in 0..iters {
        clear_bg();
        draw_2d(&mut r);
        composite_over(&o.device, &o.queue, &cp, &off2_view, &o.view);
    }
    o.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let pipelined_mean_ms = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;
    eprintln!("4-stage add'l per-frame (pipelined): {pipelined_mean_ms:.3} ms");
}

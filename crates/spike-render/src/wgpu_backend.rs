use crate::{Pixmap, Renderer};
use bytemuck::{Pod, Zeroable};
use hittest::Region;
use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    // Clip-space position, precomputed from canvas coords.
    pos: [f32; 2],
}

pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl WgpuRenderer {
    pub fn new() -> Self {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("no wgpu adapter available");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("failed to create wgpu device");
            Self { device, queue }
        })
    }

    fn tessellate(region: &Region, size: (u32, u32)) -> (Vec<Vertex>, Vec<u32>) {
        // Build a lyon path from the region's contours.
        let mut builder = Path::builder();
        for contour in &region.contours {
            if let Some((first, rest)) = contour.points.split_first() {
                builder.begin(point(first.x, first.y));
                for p in rest {
                    builder.line_to(point(p.x, p.y));
                }
                builder.end(true);
            }
        }
        let path = builder.build();

        let (w, h) = (size.0 as f32, size.1 as f32);
        let mut geometry: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let mut tess = FillTessellator::new();
        tess.tessellate_path(
            &path,
            &FillOptions::default().with_fill_rule(FillRule::EvenOdd),
            &mut BuffersBuilder::new(&mut geometry, |v: FillVertex| {
                let p = v.position();
                // Canvas (0..w, 0..h, y-down) -> clip space (-1..1, y-up).
                Vertex {
                    pos: [p.x / w * 2.0 - 1.0, 1.0 - p.y / h * 2.0],
                }
            }),
        )
        .expect("tessellation failed");

        (geometry.vertices, geometry.indices)
    }
}

impl Renderer for WgpuRenderer {
    fn name(&self) -> &'static str {
        "wgpu"
    }

    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap {
        pollster::block_on(async {
            let (w, h) = size;
            let (vertices, indices) = Self::tessellate(region, size);

            let format = wgpu::TextureFormat::Rgba8Unorm;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("target"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("solid"),
                source: wgpu::ShaderSource::Wgsl(
                    r#"
@vertex fn vs(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(pos, 0.0, 1.0);
}
@group(0) @binding(0) var<uniform> color: vec4<f32>;
@fragment fn fs() -> @location(0) vec4<f32> {
    return color;
}
"#
                    .into(),
                ),
            });

            let color = [
                fill[0] as f32 / 255.0,
                fill[1] as f32 / 255.0,
                fill[2] as f32 / 255.0,
                fill[3] as f32 / 255.0,
            ];
            let color_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("color"),
                contents: bytemuck::cast_slice(&color),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: color_buf.as_entire_binding(),
                }],
            });

            let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&bind_layout)],
                immediate_size: 0,
            });
            let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });

            let vbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ibuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

            // Readback buffer must have 256-byte-aligned row stride.
            let unpadded = w * 4;
            let align = 256;
            let padded = ((unpadded + align - 1) / align) * align;
            let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: bg[0] as f64 / 255.0,
                                g: bg[1] as f64 / 255.0,
                                b: bg[2] as f64 / 255.0,
                                a: bg[3] as f64 / 255.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &out_buf,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(h),
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            self.queue.submit(Some(encoder.finish()));

            let slice = out_buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            // wgpu 29: poll takes PollType, returns Result
            self.device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll failed");
            let mapped = slice.get_mapped_range();

            // Strip row padding into a tight RGBA8 buffer.
            let mut data = Vec::with_capacity((unpadded * h) as usize);
            for row in 0..h {
                let start = (row * padded) as usize;
                data.extend_from_slice(&mapped[start..start + unpadded as usize]);
            }
            drop(mapped);
            out_buf.unmap();

            Pixmap { width: w, height: h, data }
        })
    }
}

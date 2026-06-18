use crate::{Pixmap, Renderer};
use hittest::Region;
use vello::kurbo::{Affine, BezPath, Point as KPoint};
use vello::peniko::{Color, Fill};
use vello::{AaConfig, RenderParams, Scene};

pub struct VelloRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
}

impl VelloRenderer {
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
            let renderer = vello::Renderer::new(
                &device,
                vello::RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .expect("failed to create vello renderer");
            Self {
                device,
                queue,
                renderer,
            }
        })
    }

    fn build_path(region: &Region) -> BezPath {
        let mut path = BezPath::new();
        for contour in &region.contours {
            if let Some((first, rest)) = contour.points.split_first() {
                path.move_to(KPoint::new(first.x as f64, first.y as f64));
                for p in rest {
                    path.line_to(KPoint::new(p.x as f64, p.y as f64));
                }
                path.close_path();
            }
        }
        path
    }
}

impl Renderer for VelloRenderer {
    fn name(&self) -> &'static str {
        "vello"
    }

    fn render(&mut self, region: &Region, size: (u32, u32), fill: [u8; 4], bg: [u8; 4]) -> Pixmap {
        pollster::block_on(async {
            let (w, h) = size;

            let mut scene = Scene::new();
            scene.fill(
                Fill::EvenOdd,
                Affine::IDENTITY,
                Color::from_rgba8(fill[0], fill[1], fill[2], fill[3]),
                None,
                &Self::build_path(region),
            );

            let format = wgpu::TextureFormat::Rgba8Unorm;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("vello-target"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    &scene,
                    &view,
                    &RenderParams {
                        base_color: Color::from_rgba8(bg[0], bg[1], bg[2], bg[3]),
                        width: w,
                        height: h,
                        antialiasing_method: AaConfig::Area,
                    },
                )
                .expect("vello render failed");

            // Texture -> padded buffer -> tight RGBA8 (same readback as Task 4).
            let unpadded = w * 4;
            let padded = ((unpadded + 255) / 256) * 256;
            let out_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(Some(encoder.finish()));

            let slice = out_buf.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("device poll failed");
            let mapped = slice.get_mapped_range();
            let mut data = Vec::with_capacity((unpadded * h) as usize);
            for row in 0..h {
                let start = (row * padded) as usize;
                data.extend_from_slice(&mapped[start..start + unpadded as usize]);
            }
            drop(mapped);
            out_buf.unmap();

            Pixmap {
                width: w,
                height: h,
                data,
            }
        })
    }
}

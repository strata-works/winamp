use crate::host::{Host, StateValue};
use crate::scene::{Color, Node, Pt, Scene};
use vello::kurbo::{Affine, BezPath, Point as KPoint, Rect};
use vello::peniko::{Color as VColor, Fill};
use vello::{AaConfig, RenderParams, Scene as VScene};

pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
}

fn value_of(host: &dyn Host, key: &str) -> f32 {
    match host.get(key) {
        Some(StateValue::Scalar(v)) => v.clamp(0.0, 1.0),
        Some(StateValue::Bool(b)) => {
            if b {
                1.0
            } else {
                0.0
            }
        }
        None => 0.0,
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

fn vcolor(c: Color) -> VColor {
    VColor::from_rgba8(c.r, c.g, c.b, 255)
}

fn bbox(path: &[Pt]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for p in path {
        let x = p.x as f64;
        let y = p.y as f64;
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x > max_x {
            max_x = x;
        }
        if y > max_y {
            max_y = y;
        }
    }
    (min_x, min_y, max_x, max_y)
}

impl Renderer {
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

    pub fn render(&mut self, scene: &Scene, host: &dyn Host) -> Pixmap {
        pollster::block_on(async {
            let (w, h) = scene.canvas;

            let mut vs = VScene::new();
            for node in &scene.nodes {
                match node {
                    Node::Fill { path, color } => {
                        vs.fill(
                            Fill::NonZero,
                            Affine::IDENTITY,
                            vcolor(*color),
                            None,
                            &bez(path),
                        );
                    }
                    Node::Hotspot { .. } => {
                        // invisible — skip
                    }
                    Node::ValueFill {
                        path,
                        value_key,
                        color,
                    } => {
                        let val = value_of(host, value_key);
                        let (min_x, min_y, max_x, max_y) = bbox(path);
                        let width = max_x - min_x;
                        let filled_x = min_x + width * val as f64;
                        let rect = Rect::new(min_x, min_y, filled_x, max_y);
                        vs.fill(Fill::NonZero, Affine::IDENTITY, vcolor(*color), None, &rect);
                    }
                }
            }

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
                    &vs,
                    &view,
                    &RenderParams {
                        base_color: VColor::from_rgba8(0, 0, 0, 255),
                        width: w,
                        height: h,
                        antialiasing_method: AaConfig::Area,
                    },
                )
                .expect("vello render failed");

            // Texture -> padded buffer -> tight RGBA8 (same readback as vello_backend.rs).
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

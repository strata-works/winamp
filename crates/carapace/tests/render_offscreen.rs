#![cfg(feature = "gpu-tests")]

use carapace::render::{RenderTarget, Renderer};
use carapace::scene::{Color, Node, Pt, Scene};
use carapace::state::StateValue;

// Build a device + an offscreen Rgba8Unorm storage texture, render, read back.
struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

fn offscreen(w: u32, h: u32) -> Offscreen {
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
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
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

// Read the texture back into tight RGBA8 (256-byte-aligned readback — see vello_backend.rs).
fn readback(o: &Offscreen) -> Vec<u8> {
    pollster::block_on(async {
        let unpadded = o.w * 4;
        let padded = ((unpadded + 255) / 256) * 256;
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

#[test]
fn renders_fill_and_value_fill_at_sentinel_pixels() {
    let o = offscreen(200, 200);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![
            // red square covering the top-left quadrant
            Node::Fill {
                path: vec![
                    Pt { x: 20.0, y: 20.0 },
                    Pt { x: 100.0, y: 20.0 },
                    Pt { x: 100.0, y: 100.0 },
                    Pt { x: 20.0, y: 100.0 },
                ],
                color: Color { r: 255, g: 0, b: 0 },
            },
            // a value_fill bar across the bottom, value=0.5 -> fills left half of its bbox
            Node::ValueFill {
                path: vec![
                    Pt { x: 0.0, y: 150.0 },
                    Pt { x: 200.0, y: 150.0 },
                    Pt { x: 200.0, y: 170.0 },
                    Pt { x: 0.0, y: 170.0 },
                ],
                value_key: "v".to_string(),
                color: Color { r: 0, g: 255, b: 0 },
            },
        ],
    };
    let read = |k: &str| {
        if k == "v" {
            Some(StateValue::Scalar(0.5))
        } else {
            None
        }
    };
    r.draw(
        &scene,
        read,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
        },
    );
    let data = readback(&o);
    // sentinels (canvas==surface here, so coords map 1:1):
    assert_eq!(px(&data, 200, 60, 60), [255, 0, 0], "inside the red fill");
    assert_eq!(
        px(&data, 200, 150, 60),
        [0, 0, 0],
        "outside any fill = base black"
    );
    assert_eq!(
        px(&data, 200, 50, 160),
        [0, 255, 0],
        "value_fill filled half (x=50 < 100)"
    );
    assert_eq!(
        px(&data, 200, 150, 160),
        [0, 0, 0],
        "value_fill empty half (x=150 > 100)"
    );
}

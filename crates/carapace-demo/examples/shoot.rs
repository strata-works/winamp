//! Live render check: load each shipping demo skin through the real engine + GPU renderer
//! and write a PNG, so the Phase 5c text path (parley layout, fonts from the skin's assets,
//! value-bound `track_title`, gradient-chrome fill) can be eyeballed end to end.
//!
//! Run: `cargo run -p carapace-demo --example shoot`
//! Output: /tmp/carapace-5c/<skin>.png

use std::path::Path;
use std::time::Duration;

use carapace::engine::Engine;
use carapace::render::{RenderTarget, Renderer};
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;

/// Mirror the demo's registry so the shooter can render the host extension (`transport{}`).
fn demo_registry() -> VocabRegistry {
    let mut r = VocabRegistry::base();
    r.register(Box::new(carapace_demo::transport::TransportPrim));
    r.register(Box::new(carapace_demo::gauge::GaugePrim));
    r
}

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
            .expect("no wgpu adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("device");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shoot"),
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

fn shoot(skin: &str, out_dir: &Path) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("skins")
        .join(skin);
    let (manifest, source) = carapace::skin::load_dir(&dir).expect("load skin dir");
    let (w, h) = (manifest.canvas.width, manifest.canvas.height);

    // Pick the domain's host so bound metrics resolve (sysmon -> SysmonHost, else DemoHost).
    let host: Box<dyn carapace::host::Host> = if skin == "sysmon" {
        Box::new(carapace_demo::sysmon_host::SysmonHost::new())
    } else {
        Box::new(DemoHost::new())
    };
    let mut engine = Engine::new(host, demo_registry(), source).expect("engine");
    // Advance a little so position/metric-bound fills show progress.
    if skin != "sysmon" {
        engine.handle_command(carapace::command::Command::HostAction {
            action: "toggle_play".to_string(),
            args: vec![],
        });
    }
    engine.update(Duration::from_secs(3));

    let o = offscreen(w, h);
    let mut r = Renderer::new(&o.device);
    r.draw(
        engine.scene(),
        |k| engine.state(k),
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
            base_color: carapace::scene::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        },
    );
    let data = readback(&o);
    let img = image::RgbaImage::from_raw(w, h, data).expect("rgba");
    let path = out_dir.join(format!("{skin}.png"));
    img.save(&path).expect("save png");
    println!("wrote {} ({w}x{h})", path.display());
}

fn main() {
    let out_dir = Path::new("/tmp/carapace-5c");
    std::fs::create_dir_all(out_dir).unwrap();
    for skin in ["reference", "minimal", "classic", "transport", "sysmon"] {
        shoot(skin, out_dir);
    }
}

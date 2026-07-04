//! Headless offscreen GPU render context: renders a skin into an
//! `Rgba8Unorm` texture, reads it back to tightly-packed RGBA, PNG-encodes it,
//! and hashes frames for change-detection. Mirrors the proven `embed-spike`
//! offscreen path against the public `carapace` API.

use carapace::engine::Engine;
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Color;
use std::hash::{Hash, Hasher};
use std::time::Duration;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct GpuCtx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Headless GPU — no surface; we render into our own textures.
pub fn init_gpu() -> GpuCtx {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no wgpu adapter — carapace-preview needs a GPU");
    let mut required_features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::BGRA8UNORM_STORAGE)
    {
        required_features |= wgpu::Features::BGRA8UNORM_STORAGE;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("wgpu device");
    GpuCtx { device, queue }
}

pub struct Offscreen {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

/// Clamp requested offscreen dimensions to `[1, max]` on each axis. A canvas request larger than the
/// GPU's `max_texture_dimension_2d` would otherwise panic wgpu in `create_texture`; clamping degrades
/// an oversized request to a large-but-valid texture instead of crashing the previewer.
pub(crate) fn clamp_offscreen_dims(w: u32, h: u32, max: u32) -> (u32, u32) {
    let max = max.max(1);
    (w.clamp(1, max), h.clamp(1, max))
}

pub fn new_offscreen(device: &wgpu::Device, w: u32, h: u32) -> Offscreen {
    let (w, h) = clamp_offscreen_dims(w, h, device.limits().max_texture_dimension_2d);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("carapace-preview-offscreen"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Offscreen { tex, view, w, h }
}

/// update → layout(at target size) → draw → readback. Lays out at the offscreen
/// size so resizable skins reflow; transparent base preserves skin alpha.
pub fn render_rgba(
    engine: &mut Engine,
    renderer: &mut Renderer,
    gpu: &GpuCtx,
    off: &Offscreen,
    dt: Duration,
) -> Vec<u8> {
    engine.update(dt);
    let scene = engine.layout(off.w as f32, off.h as f32);
    let no_views = |_id: &str| None;
    renderer.draw(
        &scene,
        |k| engine.state(k),
        no_views,
        &RenderTarget {
            device: &gpu.device,
            queue: &gpu.queue,
            view: &off.view,
            width: off.w,
            height: off.h,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    );
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    readback_rgba(gpu, &off.tex, off.w, off.h)
}

/// Copy an RGBA8 texture back to CPU, returning tightly-packed rows (no padding).
fn readback_rgba(gpu: &GpuCtx, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let bpp = 4u32;
    let unpadded = w * bpp;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;

    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
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
    gpu.queue.submit([enc.finish()]);

    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range();

    let mut out = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    buf.unmap();
    out
}

pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec()).expect("rgba buffer matches w*h*4");
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("png encode");
    buf.into_inner()
}

pub fn frame_hash(rgba: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rgba.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "gpu-tests")]
    use std::path::Path;
    #[cfg(feature = "gpu-tests")]
    use std::time::Duration;

    // The canonical minimal render fixture that ships with the engine crate.
    #[cfg(feature = "gpu-tests")]
    const OK_SKIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../carapace/tests/skins/ok");

    #[cfg(feature = "gpu-tests")]
    fn load_ok_engine() -> (carapace::engine::Engine, u32, u32) {
        let (manifest, source) =
            carapace::skin::load_dir(Path::new(OK_SKIN)).expect("load ok skin");
        let host: Box<dyn carapace::host::Host> = Box::new(crate::preview_host::PreviewHost::new(
            Default::default(),
            Default::default(),
            Vec::new(),
        ));
        let engine =
            carapace::engine::Engine::new(host, carapace::vocab::VocabRegistry::base(), source)
                .expect("engine");
        (engine, manifest.canvas.width, manifest.canvas.height)
    }

    #[cfg(feature = "gpu-tests")]
    #[test]
    fn renders_a_nonempty_frame_of_expected_dims() {
        let (mut engine, w, h) = load_ok_engine();
        let gpu = init_gpu();
        let mut renderer = carapace::render::Renderer::new(&gpu.device);
        let off = new_offscreen(&gpu.device, w, h);
        let rgba = render_rgba(&mut engine, &mut renderer, &gpu, &off, Duration::ZERO);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // The "ok" skin fills a dark triangle over transparent — at least one pixel is opaque.
        assert!(
            rgba.chunks_exact(4).any(|p| p[3] > 0),
            "expected some opaque pixels"
        );
    }

    #[test]
    fn clamp_offscreen_dims_bounds_each_axis() {
        // In range: unchanged.
        assert_eq!(clamp_offscreen_dims(342, 394, 16384), (342, 394));
        // Oversized height (the reported crash: setCanvas h=394400) clamps to max, width kept.
        assert_eq!(clamp_offscreen_dims(342, 394400, 16384), (342, 16384));
        // Both axes oversized clamp to max.
        assert_eq!(clamp_offscreen_dims(99999, 99999, 16384), (16384, 16384));
        // Zero floors to 1 so wgpu never sees a zero-area texture.
        assert_eq!(clamp_offscreen_dims(0, 0, 16384), (1, 1));
    }

    #[test]
    fn png_round_trips() {
        let w = 2;
        let h = 2;
        let rgba = vec![255u8; (w * h * 4) as usize];
        let png = encode_png(&rgba, w, h);
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (w, h));
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        let a = vec![0u8, 1, 2, 3];
        let b = vec![0u8, 1, 2, 3];
        let c = vec![9u8, 1, 2, 3];
        assert_eq!(frame_hash(&a), frame_hash(&b));
        assert_ne!(frame_hash(&a), frame_hash(&c));
    }
}

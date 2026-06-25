// io-surface 0.16 is deprecated in favour of objc2-io-surface; we knowingly use it here.
#[allow(deprecated)]
use io_surface::{
    IOSurfaceGetBaseAddress, IOSurfaceGetBytesPerRow, IOSurfaceLock, IOSurfaceRef, IOSurfaceUnlock,
};
use std::time::Duration;

use carapace::engine::Engine;
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Color;

pub struct GpuCtx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Headless Metal device — no surface, we render into our own textures.
pub fn init_gpu() -> GpuCtx {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("Metal adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("wgpu device");
    GpuCtx { device, queue }
}

pub struct OffscreenTarget {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub w: u32,
    pub h: u32,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub fn new_offscreen(device: &wgpu::Device, w: u32, h: u32) -> OffscreenTarget {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("embed-spike-offscreen"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        // STORAGE_BINDING + TEXTURE_BINDING for Vello/carapace renderer;
        // RENDER_ATTACHMENT so the clear pass works; COPY_SRC for readback.
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    OffscreenTarget { tex, view, w, h }
}

/// The one draw path every tier shares: drain+tick, reflow, draw into `view`.
pub fn render_frame(
    engine: &mut Engine,
    renderer: &mut Renderer,
    gpu: &GpuCtx,
    view: &wgpu::TextureView,
    w: u32,
    h: u32,
    dt: Duration,
) {
    engine.update(dt); // drains queued host actions, ticks host
    let scene = engine.layout(w as f32, h as f32);
    renderer.draw(
        &scene,
        |k| engine.state(k),
        |_| None, // no view{} regions in the spike
        &RenderTarget {
            device: &gpu.device,
            queue: &gpu.queue,
            view,
            width: w,
            height: h,
            // Transparent base so the IOSurface carries the skin's own alpha later.
            base_color: Color { r: 0, g: 0, b: 0, a: 0 },
        },
    );
    // Ensure GPU work is complete before the caller reads back / composites.
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
}

/// Copy an RGBA8 texture back to CPU, returning tightly-packed rows (no padding).
pub fn readback_rgba(gpu: &GpuCtx, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
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
    let mut enc =
        gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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

/// Tier identifies which present path the engine is using.
#[repr(i32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Readback = 1,
    Shared = 2,
}

/// Lock a caller-owned IOSurface (BGRA8 format) and copy tightly-packed RGBA8 rows into it,
/// swizzling R↔B per pixel and honoring the surface's bytesPerRow stride.
///
/// # Safety
/// `surface` must be a valid, live IOSurface of at least w×h pixels.
/// `rgba` must contain exactly `w * h * 4` bytes of packed RGBA8 data.
#[allow(deprecated)]
pub unsafe fn copy_into_iosurface(surface: IOSurfaceRef, rgba: &[u8], w: u32, h: u32) {
    let mut seed: u32 = 0;
    // Lock for read+write (options = 0).
    IOSurfaceLock(surface, 0, &mut seed);
    let base = IOSurfaceGetBaseAddress(surface) as *mut u8;
    let stride = IOSurfaceGetBytesPerRow(surface);
    let row_bytes = (w * 4) as usize;
    for y in 0..h as usize {
        let src = rgba[y * row_bytes..(y + 1) * row_bytes].as_ptr();
        let dst = base.add(y * stride);
        // Swizzle RGBA → BGRA per pixel.
        for x in 0..w as usize {
            let s = src.add(x * 4);
            let d = dst.add(x * 4);
            // dst[0]=B=src[2], dst[1]=G=src[1], dst[2]=R=src[0], dst[3]=A=src[3]
            d.write(*s.add(2)); // B
            d.add(1).write(*s.add(1)); // G
            d.add(2).write(*s); // R
            d.add(3).write(*s.add(3)); // A
        }
    }
    IOSurfaceUnlock(surface, 0, &mut seed);
}

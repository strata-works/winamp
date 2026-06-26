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
    // Request BGRA8UNORM_STORAGE if the adapter exposes it: it's the prerequisite for the
    // Tier-2 "direct" variant (vello's compute shader writing straight into a BGRA IOSurface
    // texture). On Apple silicon it is typically available; if absent we degrade to the blit
    // variant of Tier 2, which doesn't need it.
    let mut required_features = wgpu::Features::empty();
    if adapter.features().contains(wgpu::Features::BGRA8UNORM_STORAGE) {
        required_features |= wgpu::Features::BGRA8UNORM_STORAGE;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features,
        ..Default::default()
    }))
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

/// Import a caller-owned IOSurface as a wgpu `Bgra8Unorm` texture that aliases the surface's
/// memory (zero CPU copy). Returns `None` on any failure so the caller falls back to Tier 1.
///
/// `usage` controls which wgpu usages the imported texture is created with; the matching
/// `MTLTextureUsage` is derived from it. For the blit variant of Tier 2 pass
/// `RENDER_ATTACHMENT` (the blitter renders into it); for the direct variant additionally pass
/// `STORAGE_BINDING | TEXTURE_BINDING` (requires `Features::BGRA8UNORM_STORAGE`).
///
/// # Safety
/// `surface` must be a live IOSurface of at least `w`×`h` BGRA8 pixels that outlives the texture.
#[allow(deprecated)]
pub unsafe fn try_shared(
    device: &wgpu::Device,
    surface: IOSurfaceRef,
    w: u32,
    h: u32,
    usage: wgpu::TextureUsages,
) -> Option<wgpu::Texture> {
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{
        MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType,
        MTLTextureUsage,
    };

    // Derive the MTLTextureUsage from the requested wgpu usage.
    let mut mtl_usage = MTLTextureUsage::empty();
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        mtl_usage |= MTLTextureUsage::RenderTarget;
    }
    if usage.contains(wgpu::TextureUsages::STORAGE_BINDING) {
        mtl_usage |= MTLTextureUsage::ShaderWrite;
    }
    if usage.contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        mtl_usage |= MTLTextureUsage::ShaderRead;
    }

    // 1. Reach wgpu's underlying MTLDevice through the Metal HAL.
    let hal_device = device.as_hal::<wgpu::hal::api::Metal>()?;
    let mtl_device: &ProtocolObject<dyn MTLDevice> = hal_device.raw_device();

    // 2. Build an MTLTextureDescriptor matching the BGRA IOSurface.
    let desc = MTLTextureDescriptor::new();
    desc.setTextureType(MTLTextureType::Type2D);
    desc.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    // setWidth/setHeight are `unsafe` (they can over-allocate); our values come straight from
    // the surface dimensions so they're sound.
    unsafe {
        desc.setWidth(w as usize);
        desc.setHeight(h as usize);
    }
    desc.setUsage(mtl_usage);
    desc.setStorageMode(MTLStorageMode::Shared);

    // 3. Create an MTLTexture backed by the IOSurface (plane 0).
    //    `io_surface::IOSurfaceRef` is `*const __IOSurface`; objc2's `IOSurfaceRef` is the same
    //    opaque ObjC type. Reborrow the raw pointer as objc2's `&IOSurfaceRef`.
    let io: &objc2_io_surface::IOSurfaceRef =
        unsafe { (surface as *const objc2_io_surface::IOSurfaceRef).as_ref()? };
    let mtl_tex = mtl_device.newTextureWithDescriptor_iosurface_plane(&desc, io, 0)?;

    // 4. Wrap the MTLTexture as a wgpu-hal texture, then import it into wgpu.
    let hal_tex = unsafe {
        <wgpu::hal::api::Metal as wgpu::hal::Api>::Device::texture_from_raw(
            mtl_tex,
            wgpu::TextureFormat::Bgra8Unorm,
            MTLTextureType::Type2D,
            1, // array layers
            1, // mip levels
            wgpu::hal::CopyExtent { width: w, height: h, depth: 1 },
        )
    };
    let tex = unsafe {
        device.create_texture_from_hal::<wgpu::hal::api::Metal>(
            hal_tex,
            &wgpu::TextureDescriptor {
                label: Some("iosurface-shared"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage,
                view_formats: &[],
            },
        )
    };
    Some(tex)
}

/// GPU-blit an `Rgba8Unorm` source view into a `Bgra8Unorm` destination view. The blitter's
/// shader handles the RGBA→BGRA channel reorder, so colours stay correct. Pure GPU work — no
/// CPU readback. Caller must `poll(Wait)` afterwards before CoreAnimation composites.
pub fn blit(
    gpu: &GpuCtx,
    blitter: &wgpu::util::TextureBlitter,
    src: &wgpu::TextureView,
    dst: &wgpu::TextureView,
) {
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    blitter.copy(&gpu.device, &mut enc, src, dst);
    gpu.queue.submit([enc.finish()]);
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
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

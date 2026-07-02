#![allow(unsafe_op_in_unsafe_fn)]
// SDD v2: the render primitives (`render_frame`, `blit`, `readback_rgba`,
// `upload_iosurface_to_texture`, `copy_into_iosurface`, the IOSurface lock accessors, plus
// `GpuCtx`/`OffscreenTarget`/`Present` fields) lost their sole consumer when `carapace_tick` was
// removed in Task 4. The render thread's present path (Tasks 5/6) re-consumes them; allow the
// interim dead code, matching `queue.rs`/`snapshot.rs`/`render_thread.rs`'s staged-ahead precedent.
#![allow(dead_code)]
use std::time::Duration;

use carapace::engine::Engine;
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Color;

use crate::handle::ContentTex;

pub struct GpuCtx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Headless Metal device — no surface, we render into our own textures. Returns `Err(msg)` instead
/// of panicking so `carapace_create` can surface `ErrGpuInit` (the spike's `.expect()` holes).
pub fn init_gpu() -> Result<GpuCtx, String> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .map_err(|e| format!("no Metal adapter available: {e}"))?;
    // Request BGRA8UNORM_STORAGE if the adapter exposes it: it's the prerequisite for the
    // Tier-2 "direct" variant (vello's compute shader writing straight into a BGRA IOSurface
    // texture). On Apple silicon it is typically available; if absent we degrade to the blit
    // variant of Tier 2, which doesn't need it.
    let mut required_features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::BGRA8UNORM_STORAGE)
    {
        required_features |= wgpu::Features::BGRA8UNORM_STORAGE;
    }
    // Request exactly what the adapter supports rather than wgpu's defaults. The iOS Simulator's
    // Metal adapter caps max_inter_stage_shader_variables at 15 (default wants 16), so the default
    // limits fail request_device there; adapter.limits() is always satisfiable and ≥ defaults on
    // a real GPU, so the macOS path is unaffected.
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .map_err(|e| format!("wgpu device request failed: {e}"))?;
    Ok(GpuCtx { device, queue })
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
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
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
///
/// `host_view`: optional `(view_id, texture_view)` for a skin `view{}` cutout. When present, a
/// skin node `view{ id = view_id, ... }` is composited with the supplied texture (the host's own
/// live content). `None` means no host content is supplied for any view id.
///
/// `wait`: set `true` when the caller needs all prior GPU work complete before returning
/// (e.g. Readback path — `readback_rgba` runs immediately after). Set `false` when the
/// caller does its own poll afterwards, e.g. the Shared blit path — `blit()` already calls
/// `poll(Wait)`, so a second stall here would be redundant.
#[allow(clippy::too_many_arguments)]
pub fn render_frame(
    engine: &mut Engine,
    renderer: &mut Renderer,
    gpu: &GpuCtx,
    view: &wgpu::TextureView,
    w: u32,
    h: u32,
    dt: Duration,
    wait: bool,
    host_view: Option<(&str, &wgpu::TextureView)>,
) {
    engine.update(dt); // drains queued host actions, ticks host
    // Lay out at the DESIGN CANVAS, not the surface (`w,h`) size. The renderer computes
    // sx = target.width / scene.canvas.0, so laying out at the canvas and rendering into a 2×
    // surface scales the skin up to fill the surface SHARPLY. When surface == canvas (the 1:1
    // callers) sx = 1 and behavior is identical.
    let (cw, ch) = engine.scene().canvas;
    let scene = engine.layout(cw as f32, ch as f32);
    let view_tex = |id: &str| host_view.and_then(|(vid, v)| if vid == id { Some(v) } else { None });
    renderer.draw(
        &scene,
        |k| engine.state(k),
        view_tex, // composite host content into the matching view{} cutout (if any)
        &RenderTarget {
            device: &gpu.device,
            queue: &gpu.queue,
            view,
            width: w,
            height: h,
            // Transparent base so the IOSurface carries the skin's own alpha later.
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    );
    // Poll only when the caller requests it. The Shared path skips this stall because
    // blit() is the single poll for that path; the Readback path passes wait=true so
    // readback_rgba can safely map the texture.
    if wait {
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }
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

// IOSurface accessors from the system IOSurface.framework (present on BOTH macOS and iOS). We
// declare them directly rather than via the `io-surface` crate, which transitively links the
// macOS-only OpenGL framework (through `cgl`) and therefore fails to link for iOS. The framework
// is linked explicitly so the symbols resolve regardless of which other crate requests it.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub type IOSurfaceRef = *mut core::ffi::c_void;

// `pub(crate)` (rather than private) on the Lock/Unlock/GetBaseAddress/GetBytesPerRow accessors
// so the Task 7 pixel test in `handle.rs` can lock the surface and inspect its bytes directly,
// the same way `upload_iosurface_to_texture`/`copy_into_iosurface` do below.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn IOSurfaceLock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    pub(crate) fn IOSurfaceUnlock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    pub(crate) fn IOSurfaceGetBaseAddress(buffer: IOSurfaceRef) -> *mut core::ffi::c_void;
    pub(crate) fn IOSurfaceGetBytesPerRow(buffer: IOSurfaceRef) -> usize;
    pub fn IOSurfaceGetWidth(buffer: IOSurfaceRef) -> usize;
    pub fn IOSurfaceGetHeight(buffer: IOSurfaceRef) -> usize;
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
#[cfg(any(target_os = "macos", target_os = "ios"))]
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
            wgpu::hal::CopyExtent {
                width: w,
                height: h,
                depth: 1,
            },
        )
    };
    let tex = unsafe {
        device.create_texture_from_hal::<wgpu::hal::api::Metal>(
            hal_tex,
            &wgpu::TextureDescriptor {
                label: Some("iosurface-shared"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
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

/// Create a NORMAL (non-aliased) wgpu `Bgra8Unorm` texture for the engine's `view{}` composite
/// path. Unlike an IOSurface-aliased import, this texture is wgpu-owned memory that we re-upload
/// the host's content into every frame via `upload_iosurface_to_texture`. That per-frame copy is
/// what guarantees CPU→GPU coherency: the GPU always samples this frame's content, never a stale
/// first-frame cache (the bug an aliased import exhibits because the GPU caches the import).
///
/// `COPY_DST` so `queue.write_texture` can upload into it; `TEXTURE_BINDING` so the engine can
/// sample it into the cutout.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn make_content_texture(
    device: &wgpu::Device,
    w: u32,
    h: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("embed-spike-content"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (tex, view)
}

/// Read the current bytes of a caller-owned BGRA8 IOSurface and upload them into `tex` via
/// `queue.write_texture`. Called every frame BEFORE rendering so the engine samples this frame's
/// host content (the fix for the frozen-`view{}` coherency bug — see `make_content_texture`).
///
/// ## 256-byte `bytes_per_row` alignment
/// `queue.write_texture` requires `bytes_per_row` to be a multiple of
/// `COPY_BYTES_PER_ROW_ALIGNMENT` (256). An IOSurface's stride is whatever CoreGraphics chose and
/// is frequently NOT a multiple of 256 (e.g. a 404-wide BGRA surface has stride 1616, which is
/// `404*4` and `1616 % 256 == 80 != 0`). When the surface stride is already 256-aligned we hand
/// it straight to wgpu; otherwise we repack each row into a 256-aligned staging buffer first
/// (mirroring the padded-stride pattern in `readback_rgba`), then upload that with the padded
/// stride.
///
/// # Safety
/// `surface` must be a live IOSurface of at least `w`×`h` BGRA8 pixels. `tex` must be a
/// `Bgra8Unorm` texture of at least `w`×`h` with `COPY_DST` usage.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[allow(deprecated)]
pub unsafe fn upload_iosurface_to_texture(
    queue: &wgpu::Queue,
    surface: IOSurfaceRef,
    tex: &wgpu::Texture,
    w: u32,
    h: u32,
) {
    let mut seed: u32 = 0;
    // Read-only lock: we only read the surface's bytes here.
    IOSurfaceLock(surface, 0x1 /* kIOSurfaceLockReadOnly */, &mut seed);
    let base = IOSurfaceGetBaseAddress(surface) as *const u8;
    let stride = IOSurfaceGetBytesPerRow(surface) as u32;

    let extent = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let copy = |bytes: &[u8], bytes_per_row: u32| {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(h),
            },
            extent,
        );
    };

    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    if stride.is_multiple_of(align) {
        // Stride is already 256-aligned: upload the surface bytes directly, no repack.
        let bytes = unsafe { std::slice::from_raw_parts(base, (stride * h) as usize) };
        copy(bytes, stride);
    } else {
        // Stride is not 256-aligned: repack each row into a padded staging buffer whose row
        // stride IS a multiple of 256, then upload that.
        let row_bytes = (w * 4) as usize;
        let padded = stride.div_ceil(align) * align;
        let mut staging = vec![0u8; (padded * h) as usize];
        for y in 0..h as usize {
            let src =
                unsafe { std::slice::from_raw_parts(base.add(y * stride as usize), row_bytes) };
            let dst_start = y * padded as usize;
            staging[dst_start..dst_start + row_bytes].copy_from_slice(src);
        }
        copy(&staging, padded);
    }

    IOSurfaceUnlock(surface, 0x1, &mut seed);
}

/// GPU-blit an `Rgba8Unorm` source view into a `Bgra8Unorm` destination view. The blitter's
/// shader handles the RGBA→BGRA channel reorder, so colours stay correct. Pure GPU work — no
/// CPU readback. This function calls `poll(Wait)` itself, making it the single GPU stall on
/// the Shared path; callers should pass `wait = false` to `render_frame` to avoid a redundant
/// stall before this blit.
pub fn blit(
    gpu: &GpuCtx,
    blitter: &wgpu::util::TextureBlitter,
    src: &wgpu::TextureView,
    dst: &wgpu::TextureView,
) {
    let mut enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
#[cfg(any(target_os = "macos", target_os = "ios"))]
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

/// How a rendered frame reaches the caller's IOSurface.
// SDD v2: the fields are consumed by the render thread's present path (Tasks 5/6); until then only
// `build_present` constructs them, so allow the interim "constructed but not read".
#[allow(dead_code)]
pub enum Present {
    /// Tier 2 (zero CPU copy): vello renders into an `Rgba8` storage offscreen, then a GPU
    /// blit copies+reorders it into the `Bgra8` texture that aliases the IOSurface. Nothing
    /// touches the CPU. (Blit variant — chosen for robustness; see task-6-report.md.)
    Shared {
        off: OffscreenTarget,
        // Held only to keep the imported wgpu texture (and the MTLTexture aliasing the
        // IOSurface) alive for the engine's lifetime; we render through `iosurface_view`.
        #[allow(dead_code)]
        iosurface_tex: wgpu::Texture,
        iosurface_view: wgpu::TextureView,
        blitter: wgpu::util::TextureBlitter,
    },
    /// Tier 1 (fallback): render into the offscreen, read it back to CPU, swizzle-copy into
    /// the IOSurface.
    Readback { off: OffscreenTarget },
}

/// Try Tier 2 (zero-copy IOSurface import) first; fall back to Tier 1 readback on any failure.
/// The IOSurface texture only needs RENDER_ATTACHMENT — the blitter renders into it, so no BGRA
/// storage feature is required. Lifted verbatim from the spike's `carapace_create`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn build_present(
    gpu: &GpuCtx,
    surface: IOSurfaceRef,
    w: u32,
    h: u32,
) -> (Present, Tier) {
    match unsafe {
        try_shared(
            &gpu.device,
            surface,
            w,
            h,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
    } {
        Some(iosurface_tex) => {
            let iosurface_view = iosurface_tex.create_view(&wgpu::TextureViewDescriptor::default());
            let blitter =
                wgpu::util::TextureBlitter::new(&gpu.device, wgpu::TextureFormat::Bgra8Unorm);
            let off = new_offscreen(&gpu.device, w, h);
            (
                Present::Shared {
                    off,
                    iosurface_tex,
                    iosurface_view,
                    blitter,
                },
                Tier::Shared,
            )
        }
        None => (
            Present::Readback {
                off: new_offscreen(&gpu.device, w, h),
            },
            Tier::Readback,
        ),
    }
}

/// Optionally import the host's content IOSurface as a sampled texture for the skin's
/// `view{ id = "host" }` cutout. Null surface, a failed import, or zero dimensions all yield
/// None (the cutout simply shows nothing). NEVER panic. Lifted verbatim from the spike's
/// `carapace_create`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn build_content(gpu: &GpuCtx, content_surface: IOSurfaceRef) -> Option<ContentTex> {
    if content_surface.is_null() {
        None
    } else {
        let (cw, ch) = unsafe {
            (
                IOSurfaceGetWidth(content_surface) as u32,
                IOSurfaceGetHeight(content_surface) as u32,
            )
        };
        if cw == 0 || ch == 0 {
            None
        } else {
            // A NORMAL wgpu texture we re-upload the content surface into every tick
            // (CPU→GPU coherency). No IOSurface aliasing here — that's what froze the
            // content before.
            let (tex, view) = make_content_texture(&gpu.device, cw, ch);
            Some(ContentTex {
                surface: content_surface,
                tex,
                view,
                w: cw,
                h: ch,
            })
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn init_gpu_succeeds_and_offscreen_allocates() {
        let gpu = init_gpu().expect("Metal device on a macOS test host");
        let off = new_offscreen(&gpu.device, 8, 8);
        assert_eq!((off.w, off.h), (8, 8));
    }
}

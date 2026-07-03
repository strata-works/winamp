//! Offscreen wgpu harness for the paper-mesh transpile spike.
//!
//! Links the transpiled paper.design vertex + fragment WGSL into a real
//! `RenderPipeline`, derives the uniform bind-group layout from naga
//! introspection of the emitted module (glslang auto-assigns `@group/@binding`
//! via `--amb/--aml`, so binding numbers are discovered, never assumed), fills
//! the uniforms by NAME, renders one frame of the mesh gradient offscreen, and
//! writes a PNG.
//!
//! Zero engine-crate diff: this lives entirely in the demo example.

#![allow(dead_code)]

use std::path::Path;

/// A ready-to-use offscreen GPU (no surface / window).
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Acquire an offscreen adapter + device (ported from `examples/shoot.rs`).
pub fn new_gpu() -> Gpu {
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
        Gpu { device, queue }
    })
}

/// G2 module check: does wgpu's own naga validator accept this WGSL?
pub fn wgpu_accepts(gpu: &Gpu, wgsl: &str) -> Result<(), String> {
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _m = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("paper"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
    match pollster::block_on(scope.pop()) {
        Some(e) => Err(format!("{e}")),
        None => Ok(()),
    }
}

/// One uniform global variable as emitted into the WGSL module.
pub struct UniformField {
    pub name: String,
    pub group: u32,
    pub binding: u32,
    pub size: u64,
}

/// Introspect the naga module for its uniform globals + their assigned bindings.
pub fn uniform_fields(module: &naga::Module) -> Vec<UniformField> {
    let mut layouter = naga::proc::Layouter::default();
    let _ = layouter.update(module.to_ctx());
    module
        .global_variables
        .iter()
        .filter_map(|(_, gv)| {
            if gv.space != naga::AddressSpace::Uniform {
                return None;
            }
            let rb = gv.binding.as_ref()?;
            Some(UniformField {
                name: gv.name.clone().unwrap_or_default(),
                group: rb.group,
                binding: rb.binding,
                size: layouter[gv.ty].size as u64,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Uniform value filling (by NAME, per the brief's default table).
//
// KEY FINDING (Task 3/Step 6): glslang merges all freestanding ES uniforms into
// a single `gl_DefaultUniformBlock` struct, emitted as ONE `@group/@binding`
// uniform global per stage — NOT individual globals. So we fill the whole block
// by iterating its struct MEMBERS at their naga-computed byte offsets, keyed by
// member name. (A non-struct scalar uniform is filled by the global's own name.)
// ---------------------------------------------------------------------------

/// The f32 lane(s) for a named uniform member. `time` animates `u_time`.
/// Unlisted names -> empty (zero-filled).
fn member_values(name: &str, w: u32, h: u32, time: f32) -> Vec<f32> {
    match name {
        "u_time" => vec![time],
        "u_resolution" => vec![w as f32, h as f32],
        "u_pixelRatio" => vec![1.0],
        "u_scale" => vec![1.0],
        "u_fit" => vec![0.0],
        "u_worldWidth" | "u_worldHeight" => vec![0.0],
        "u_originX" | "u_originY" => vec![0.5],
        "u_rotation" | "u_offsetX" | "u_offsetY" => vec![0.0],
        "u_imageAspectRatio" => vec![1.0],
        "u_colorsCount" => vec![4.0],
        // vec4[10]: four visible colors, then zeros (tight, matches array stride 16).
        "u_colors" => {
            let mut v = vec![
                0.94, 0.28, 0.44, 1.0, //
                0.15, 0.39, 0.92, 1.0, //
                0.99, 0.76, 0.18, 1.0, //
                0.11, 0.78, 0.55, 1.0, //
            ];
            v.resize(40, 0.0);
            v
        }
        "u_distortion" => vec![0.6],
        "u_swirl" => vec![0.5],
        "u_grainMixer" => vec![0.0],
        "u_grainOverlay" => vec![0.0],
        _ => vec![],
    }
}

/// A fully-filled uniform block ready to bind at `(group, binding)`.
struct FilledBlock {
    group: u32,
    binding: u32,
    size: u64,
    data: Vec<u8>,
}

/// Re-parse `wgsl`, locate every uniform-space global, and fill each into a
/// byte buffer by member name at naga's computed offsets. `group_offset` shifts
/// the emitted `@group` numbers so a second stage's blocks don't collide with
/// the first (both stages emit `@group(0)` independently).
fn fill_blocks(wgsl: &str, w: u32, h: u32, time: f32) -> Result<Vec<FilledBlock>, String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| format!("wgsl re-parse: {e:?}"))?;
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .map_err(|e| format!("layout: {e:?}"))?;
    let mut blocks = Vec::new();
    for (_, gv) in module.global_variables.iter() {
        if gv.space != naga::AddressSpace::Uniform {
            continue;
        }
        let Some(rb) = gv.binding.as_ref() else {
            continue;
        };
        let size = layouter[gv.ty].size as u64;
        let mut data = vec![0u8; size as usize];
        let write = |data: &mut [u8], offset: usize, vals: &[f32]| {
            for (i, f) in vals.iter().enumerate() {
                let start = offset + i * 4;
                if start + 4 <= data.len() {
                    data[start..start + 4].copy_from_slice(&f.to_ne_bytes());
                }
            }
        };
        match &module.types[gv.ty].inner {
            naga::TypeInner::Struct { members, .. } => {
                for m in members {
                    let name = m.name.clone().unwrap_or_default();
                    write(
                        &mut data,
                        m.offset as usize,
                        &member_values(&name, w, h, time),
                    );
                }
            }
            _ => {
                let name = gv.name.clone().unwrap_or_default();
                write(&mut data, 0, &member_values(&name, w, h, time));
            }
        }
        blocks.push(FilledBlock {
            group: rb.group,
            binding: rb.binding,
            size,
            data,
        });
    }
    Ok(blocks)
}

/// Bump every `@group(N)` in a WGSL string by `offset` (leaves `@binding`
/// alone). Used to relocate the fragment's uniform block off the vertex's.
fn shift_groups(wgsl: &str, offset: u32) -> String {
    if offset == 0 {
        return wgsl.to_string();
    }
    let mut out = String::with_capacity(wgsl.len());
    let mut rest = wgsl;
    while let Some(pos) = rest.find("@group(") {
        out.push_str(&rest[..pos + 7]);
        let after = &rest[pos + 7..];
        let end = after.find(')').unwrap_or(0);
        let num: u32 = after[..end].trim().parse().unwrap_or(0);
        out.push_str(&(num + offset).to_string());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// G3: link both stages, bind uniforms, render one frame, write PNG.
#[allow(clippy::too_many_arguments)]
pub fn render_mesh(
    gpu: &Gpu,
    vert_wgsl: &str,
    frag_wgsl: &str,
    frag_fields: &[UniformField],
    vert_fields: &[UniformField],
    w: u32,
    h: u32,
    time: f32,
    out: &Path,
) -> Result<(), String> {
    let rgba = render_mesh_rgba(
        gpu,
        vert_wgsl,
        frag_wgsl,
        frag_fields,
        vert_fields,
        w,
        h,
        time,
    )?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("rgba buffer wrong size")?;
    img.save(out).map_err(|e| e.to_string())?;
    Ok(())
}

/// The core render: returns the tightly-packed RGBA8 buffer (w*h*4 bytes).
#[allow(clippy::too_many_arguments)]
pub fn render_mesh_rgba(
    gpu: &Gpu,
    vert_wgsl: &str,
    frag_wgsl: &str,
    frag_fields: &[UniformField],
    vert_fields: &[UniformField],
    w: u32,
    h: u32,
    time: f32,
) -> Result<Vec<u8>, String> {
    let device = &gpu.device;
    let queue = &gpu.queue;

    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    // Both stages independently emit their merged uniform block at `@group(0)`.
    // Relocate the fragment's groups above the vertex's so they don't collide
    // in the linked pipeline. `frag_fields`/`vert_fields` (block descriptors
    // from `uniform_fields`) tell us where each stage's groups live.
    let vert_max_group = vert_fields.iter().map(|f| f.group).max();
    let group_offset = vert_max_group.map(|g| g + 1).unwrap_or(0);
    let _ = frag_fields; // contract param; block layout is re-derived from WGSL below
    let frag_wgsl_shifted = shift_groups(frag_wgsl, group_offset);

    let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("paper-vs"),
        source: wgpu::ShaderSource::Wgsl(vert_wgsl.into()),
    });
    let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("paper-fs"),
        source: wgpu::ShaderSource::Wgsl(frag_wgsl_shifted.as_str().into()),
    });

    // --- Bind group layout + buffers, derived from WGSL introspection. ---
    struct Bound {
        binding: u32,
        buffer: wgpu::Buffer,
        visibility: wgpu::ShaderStages,
    }
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<u32, Vec<Bound>> = BTreeMap::new();

    let mut add = |wgsl: &str, vis: wgpu::ShaderStages| -> Result<(), String> {
        for blk in fill_blocks(wgsl, w, h, time)? {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("paper-uniform"),
                size: blk.size.max(16),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, &blk.data);
            groups.entry(blk.group).or_default().push(Bound {
                binding: blk.binding,
                buffer,
                visibility: vis,
            });
        }
        Ok(())
    };
    add(vert_wgsl, wgpu::ShaderStages::VERTEX)?;
    add(&frag_wgsl_shifted, wgpu::ShaderStages::FRAGMENT)?;

    // Build a bind group layout + bind group per group index, in order.
    let mut bgls: Vec<wgpu::BindGroupLayout> = Vec::new();
    let mut bind_groups: Vec<wgpu::BindGroup> = Vec::new();
    let mut group_indices: Vec<u32> = groups.keys().copied().collect();
    group_indices.sort_unstable();
    for gi in &group_indices {
        let bounds = &groups[gi];
        let entries: Vec<wgpu::BindGroupLayoutEntry> = bounds
            .iter()
            .map(|b| wgpu::BindGroupLayoutEntry {
                binding: b.binding,
                visibility: b.visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("paper-bgl"),
            entries: &entries,
        });
        let bg_entries: Vec<wgpu::BindGroupEntry> = bounds
            .iter()
            .map(|b| wgpu::BindGroupEntry {
                binding: b.binding,
                resource: b.buffer.as_entire_binding(),
            })
            .collect();
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("paper-bg"),
            layout: &bgl,
            entries: &bg_entries,
        });
        bgls.push(bgl);
        bind_groups.push(bg);
    }
    let bgl_refs: Vec<Option<&wgpu::BindGroupLayout>> = bgls.iter().map(Some).collect();

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("paper-pl"),
        bind_group_layouts: &bgl_refs,
        immediate_size: 0,
    });

    // The vertex shader consumes `a_position` at location 0 (vec4). Supply a
    // clip-space quad as a vertex buffer.
    #[rustfmt::skip]
    let quad: [f32; 16] = [
        -1.0, -1.0, 0.0, 1.0,
         1.0, -1.0, 0.0, 1.0,
        -1.0,  1.0, 0.0, 1.0,
         1.0,  1.0, 0.0, 1.0,
    ];
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("paper-quad"),
        size: std::mem::size_of_val(&quad) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vbuf, 0, bytemuck_cast(&quad));

    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 0,
        }],
    };

    let vs_entry = entry_point_name(vert_wgsl, "@vertex");
    let fs_entry = entry_point_name(frag_wgsl, "@fragment");

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("paper-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs,
            entry_point: vs_entry.as_deref(),
            compilation_options: Default::default(),
            buffers: &[vertex_layout],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs,
            entry_point: fs_entry.as_deref(),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    // Target texture.
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("paper-target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("paper-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        for (i, bg) in bind_groups.iter().enumerate() {
            pass.set_bind_group(group_indices[i], bg, &[]);
        }
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..4, 0..1);
    }
    queue.submit(Some(enc.finish()));

    if let Some(e) = pollster::block_on(scope.pop()) {
        return Err(format!("pipeline/render validation: {e}"));
    }

    Ok(readback(device, queue, &texture, w, h))
}

/// Copy a texture back to CPU as tightly-packed RGBA8 (256-byte row padding).
fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Vec<u8> {
    pollster::block_on(async {
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rb"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
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
        queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let mapped = slice.get_mapped_range();
        let mut data = Vec::with_capacity((unpadded * h) as usize);
        for row in 0..h {
            let start = (row * padded) as usize;
            data.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buf.unmap();
        data
    })
}

/// Reinterpret a `&[f32]` slice as bytes without pulling in a dep.
fn bytemuck_cast(floats: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(floats.as_ptr() as *const u8, std::mem::size_of_val(floats))
    }
}

/// Extract the entry-point function name that follows a stage attribute in WGSL,
/// e.g. `@vertex fn main_1(...)` -> `main_1`. Returns None (let wgpu default)
/// if not found.
fn entry_point_name(wgsl: &str, stage_attr: &str) -> Option<String> {
    let idx = wgsl.find(stage_attr)?;
    let after = &wgsl[idx + stage_attr.len()..];
    let fn_idx = after.find("fn ")?;
    let rest = &after[fn_idx + 3..];
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transpile::transpile;

    fn shaders_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/paper_mesh_spike/shaders")
    }

    #[test]
    fn accepts_valid_wgsl() {
        let gpu = new_gpu();
        let wgsl = "@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }";
        assert!(wgpu_accepts(&gpu, wgsl).is_ok());
    }

    /// Observation dump: transpile the REAL shaders, print bindings + WGSL heads.
    #[test]
    #[ignore]
    fn observe_real_shaders() {
        if !crate::transpile::glslang_available() {
            eprintln!("SKIP observe: glslang not installed");
            return;
        }
        let dir = shaders_dir();
        let frag = std::fs::read_to_string(dir.join("mesh_gradient.frag")).unwrap();
        let vert = std::fs::read_to_string(dir.join("vertex.vert")).unwrap();
        let f = transpile(&frag, naga::ShaderStage::Fragment).unwrap();
        let v = transpile(&vert, naga::ShaderStage::Vertex).unwrap();
        for (label, t) in [("VERTEX", &v), ("FRAGMENT", &f)] {
            eprintln!("===== {label} uniform_fields =====");
            for uf in uniform_fields(&t.module) {
                eprintln!(
                    "  name={:<20} group={} binding={} size={}",
                    uf.name, uf.group, uf.binding, uf.size
                );
            }
            eprintln!("----- {label} WGSL head -----");
            for line in t.wgsl.lines().take(48) {
                eprintln!("  {line}");
            }
        }
    }

    /// The real render must be GENUINE: non-uniform pixels, and animated
    /// (two different `time` values -> different buffers).
    #[test]
    fn real_mesh_renders_and_animates() {
        if !crate::transpile::glslang_available() {
            eprintln!("SKIP real_mesh_renders_and_animates: glslang not installed");
            return;
        }
        let dir = shaders_dir();
        let frag = std::fs::read_to_string(dir.join("mesh_gradient.frag")).unwrap();
        let vert = std::fs::read_to_string(dir.join("vertex.vert")).unwrap();
        let f = transpile(&frag, naga::ShaderStage::Fragment).unwrap();
        let v = transpile(&vert, naga::ShaderStage::Vertex).unwrap();
        let ff = uniform_fields(&f.module);
        let vf = uniform_fields(&v.module);

        let gpu = new_gpu();
        let (w, h) = (256u32, 256u32);
        let a = render_mesh_rgba(&gpu, &v.wgsl, &f.wgsl, &ff, &vf, w, h, 0.0)
            .expect("render t=0 must succeed");
        let b = render_mesh_rgba(&gpu, &v.wgsl, &f.wgsl, &ff, &vf, w, h, 3.0)
            .expect("render t=3 must succeed");

        // Also write PNGs at three times for manual eyeballing against
        // https://shaders.paper.design/ (each should look visibly different).
        let out_dir = std::path::Path::new("/tmp/paper-mesh-spike");
        for (i, t) in [0.0f32, 1.0, 2.0].into_iter().enumerate() {
            render_mesh(
                &gpu,
                &v.wgsl,
                &f.wgsl,
                &ff,
                &vf,
                512,
                512,
                t,
                &out_dir.join(format!("mesh_t{i}.png")),
            )
            .expect("render_mesh PNG must succeed");
        }

        // Non-uniform: not all pixels identical.
        let first = &a[0..4];
        let uniform = a.chunks_exact(4).all(|px| px == first);
        assert!(
            !uniform,
            "render is a flat/constant image (pipeline no-op?)"
        );

        // Meaningful variance across the luminance channel.
        let mean: f64 = a.iter().map(|&b| b as f64).sum::<f64>() / a.len() as f64;
        let var: f64 = a.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / a.len() as f64;
        assert!(
            var > 25.0,
            "pixel variance too low ({var:.2}); image ~blank"
        );

        // Animated: differ across time.
        let diff = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
        let frac = diff as f64 / a.len() as f64;
        assert!(
            frac > 0.01,
            "frames at t=0 and t=3 differ in only {:.3}% of bytes; u_time not animating",
            frac * 100.0
        );
        eprintln!("variance={var:.1} animated_diff_frac={:.3}", frac);
    }
}

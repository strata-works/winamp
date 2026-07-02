//! Live paper mesh-gradient renderer for the macOS host (Phase 2).
//!
//! Builds a wgpu pipeline from the PRE-BAKED Phase-1 WGSL against the engine's
//! device, renders one animated frame per tick into an owned texture, and hands
//! that texture's view to the engine's `view{ id = "paper" }` compositor.
//! naga `wgsl-in` is used ONCE (in `new`) to compute uniform byte offsets from
//! the baked WGSL — no glslang, no per-frame transpile.

use std::collections::BTreeMap;

const VERT_WGSL: &str = include_str!("../shaders/paper/vertex.wgsl");
const FRAG_WGSL: &str = include_str!("../shaders/paper/mesh_gradient.wgsl");

pub struct PaperView {
    pipeline: wgpu::RenderPipeline,
    bind_groups: Vec<(u32, wgpu::BindGroup)>,
    buffers: Vec<wgpu::Buffer>,
    /// (index into `buffers`, byte offset) of every `u_time` lane to update per frame.
    time_targets: Vec<(usize, u64)>,
    vbuf: wgpu::Buffer,
    // Read via `texture()`, which is currently only called by the test's readback helper;
    // Task 4 wires the FFI copy path through it too.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// f32 lanes for a named uniform member. `time` animates `u_time`; unlisted -> empty.
fn member_values(name: &str, w: u32, h: u32, time: f32) -> Vec<f32> {
    match name {
        "u_time" => vec![time],
        "u_resolution" => vec![w as f32, h as f32],
        "u_pixelRatio" | "u_scale" | "u_imageAspectRatio" => vec![1.0],
        "u_fit" | "u_worldWidth" | "u_worldHeight" | "u_rotation" | "u_offsetX" | "u_offsetY" => {
            vec![0.0]
        }
        "u_originX" | "u_originY" => vec![0.5],
        "u_colorsCount" => vec![4.0],
        "u_colors" => {
            let mut v = vec![
                0.94, 0.28, 0.44, 1.0, 0.15, 0.39, 0.92, 1.0, 0.99, 0.76, 0.18, 1.0, 0.11, 0.78,
                0.55, 1.0,
            ];
            v.resize(40, 0.0);
            v
        }
        "u_distortion" => vec![0.6],
        "u_swirl" => vec![0.5],
        "u_grainMixer" | "u_grainOverlay" => vec![0.0],
        _ => vec![],
    }
}

/// Bump every `@group(N)` in `wgsl` by `offset` (leaves `@binding` alone).
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

/// Entry-point fn name after a stage attribute, e.g. `@vertex fn main_1(` -> `main_1`.
fn entry_point_name(wgsl: &str, stage_attr: &str) -> Option<String> {
    let idx = wgsl.find(stage_attr)?;
    let after = &wgsl[idx + stage_attr.len()..];
    let fn_idx = after.find("fn ")?;
    let name: String = after[fn_idx + 3..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// One uniform block filled by member name at naga offsets, plus the byte offset
/// of its `u_time` member if present.
struct Block {
    group: u32,
    binding: u32,
    data: Vec<u8>,
    time_offset: Option<u64>,
}

fn fill_blocks(wgsl: &str, w: u32, h: u32) -> Result<Vec<Block>, String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| format!("wgsl parse: {e:?}"))?;
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .map_err(|e| format!("layout: {e:?}"))?;
    let mut blocks = Vec::new();
    let write = |data: &mut [u8], off: usize, vals: &[f32]| {
        for (i, f) in vals.iter().enumerate() {
            let s = off + i * 4;
            if s + 4 <= data.len() {
                data[s..s + 4].copy_from_slice(&f.to_ne_bytes());
            }
        }
    };
    for (_, gv) in module.global_variables.iter() {
        if gv.space != naga::AddressSpace::Uniform {
            continue;
        }
        let Some(rb) = gv.binding.as_ref() else {
            continue;
        };
        let size = layouter[gv.ty].size as usize;
        let mut data = vec![0u8; size];
        let mut time_offset = None;
        match &module.types[gv.ty].inner {
            naga::TypeInner::Struct { members, .. } => {
                for m in members {
                    let name = m.name.clone().unwrap_or_default();
                    if name == "u_time" {
                        time_offset = Some(m.offset as u64);
                    }
                    write(
                        &mut data,
                        m.offset as usize,
                        &member_values(&name, w, h, 0.0),
                    );
                }
            }
            _ => {
                let name = gv.name.clone().unwrap_or_default();
                if name == "u_time" {
                    time_offset = Some(0);
                }
                write(&mut data, 0, &member_values(&name, w, h, 0.0));
            }
        }
        blocks.push(Block {
            group: rb.group,
            binding: rb.binding,
            data,
            time_offset,
        });
    }
    Ok(blocks)
}

impl PaperView {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32) -> Result<Self, String> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

        // Both stages emit their uniform block at @group(0); relocate the fragment's
        // above the vertex's. Vertex uses group 0 only -> offset 1.
        let vert_blocks = fill_blocks(VERT_WGSL, w, h)?;
        let vert_max_group = vert_blocks.iter().map(|b| b.group).max();
        let group_offset = vert_max_group.map(|g| g + 1).unwrap_or(0);
        let frag_wgsl = shift_groups(FRAG_WGSL, group_offset);
        let frag_blocks = fill_blocks(&frag_wgsl, w, h)?;

        let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("paper-vs"),
            source: wgpu::ShaderSource::Wgsl(VERT_WGSL.into()),
        });
        let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("paper-fs"),
            source: wgpu::ShaderSource::Wgsl(frag_wgsl.as_str().into()),
        });

        // Buffers per (group,binding), grouped by group index.
        struct Bound {
            binding: u32,
            buffer_idx: usize,
            vis: wgpu::ShaderStages,
        }
        let mut buffers: Vec<wgpu::Buffer> = Vec::new();
        let mut time_targets: Vec<(usize, u64)> = Vec::new();
        let mut groups: BTreeMap<u32, Vec<Bound>> = BTreeMap::new();
        let add = |buffers: &mut Vec<wgpu::Buffer>,
                   groups: &mut BTreeMap<u32, Vec<Bound>>,
                   time_targets: &mut Vec<(usize, u64)>,
                   blk: &Block,
                   vis: wgpu::ShaderStages| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("paper-uniform"),
                size: (blk.data.len() as u64).max(16),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, &blk.data);
            let idx = buffers.len();
            buffers.push(buffer);
            if let Some(off) = blk.time_offset {
                time_targets.push((idx, off));
            }
            groups.entry(blk.group).or_default().push(Bound {
                binding: blk.binding,
                buffer_idx: idx,
                vis,
            });
        };
        for b in &vert_blocks {
            add(
                &mut buffers,
                &mut groups,
                &mut time_targets,
                b,
                wgpu::ShaderStages::VERTEX,
            );
        }
        for b in &frag_blocks {
            add(
                &mut buffers,
                &mut groups,
                &mut time_targets,
                b,
                wgpu::ShaderStages::FRAGMENT,
            );
        }

        let mut bgls: Vec<wgpu::BindGroupLayout> = Vec::new();
        let mut bind_groups: Vec<(u32, wgpu::BindGroup)> = Vec::new();
        for (gi, bounds) in groups.iter() {
            let entries: Vec<wgpu::BindGroupLayoutEntry> = bounds
                .iter()
                .map(|b| wgpu::BindGroupLayoutEntry {
                    binding: b.binding,
                    visibility: b.vis,
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
                    resource: buffers[b.buffer_idx].as_entire_binding(),
                })
                .collect();
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("paper-bg"),
                layout: &bgl,
                entries: &bg_entries,
            });
            bgls.push(bgl);
            bind_groups.push((*gi, bg));
        }
        let bgl_refs: Vec<Option<&wgpu::BindGroupLayout>> = bgls.iter().map(Some).collect();
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("paper-pl"),
            bind_group_layouts: &bgl_refs,
            immediate_size: 0,
        });

        #[rustfmt::skip]
        let quad: [f32; 16] = [
            -1.0,-1.0,0.0,1.0,  1.0,-1.0,0.0,1.0,
            -1.0, 1.0,0.0,1.0,  1.0, 1.0,0.0,1.0,
        ];
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("paper-quad"),
            size: std::mem::size_of_val(&quad) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vbuf, 0, unsafe {
            std::slice::from_raw_parts(quad.as_ptr() as *const u8, std::mem::size_of_val(&quad))
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("paper-pipeline"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: entry_point_name(VERT_WGSL, "@vertex").as_deref(),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: entry_point_name(FRAG_WGSL, "@fragment").as_deref(),
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
            // TEXTURE_BINDING so the engine composite can sample it; COPY_SRC for the test readback.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("paper pipeline validation: {e}"));
        }
        Ok(PaperView {
            pipeline,
            bind_groups,
            buffers,
            time_targets,
            vbuf,
            texture,
            view,
        })
    }

    pub fn render(&self, device: &wgpu::Device, queue: &wgpu::Queue, time: f32) {
        for (idx, off) in &self.time_targets {
            queue.write_buffer(&self.buffers[*idx], *off, &time.to_ne_bytes());
        }
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("paper-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
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
            pass.set_pipeline(&self.pipeline);
            for (gi, bg) in &self.bind_groups {
                pass.set_bind_group(*gi, bg, &[]);
            }
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.draw(0..4, 0..1);
        }
        queue.submit(Some(enc.finish()));
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.view
    }
    #[allow(dead_code)]
    pub(crate) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless() -> (wgpu::Device, wgpu::Queue) {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("adapter");
            adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("device")
        })
    }

    /// Read back the paper texture as tightly-packed RGBA8 (test-only helper).
    fn readback(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pv: &PaperView,
        w: u32,
        h: u32,
    ) -> Vec<u8> {
        let padded = (w * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pv-rb"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: pv.texture(),
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
        let mut out = Vec::with_capacity((w * 4 * h) as usize);
        for row in 0..h {
            let s = (row * padded) as usize;
            out.extend_from_slice(&mapped[s..s + (w * 4) as usize]);
        }
        out
    }

    #[test]
    fn paper_view_renders_and_animates() {
        let (device, queue) = headless();
        let (w, h) = (256u32, 256u32);
        let pv = PaperView::new(&device, &queue, w, h).expect("PaperView::new");

        pv.render(&device, &queue, 0.0);
        let a = readback(&device, &queue, &pv, w, h);
        pv.render(&device, &queue, 3.0);
        let b = readback(&device, &queue, &pv, w, h);

        // Non-uniform: not a flat image.
        let first = &a[0..4];
        assert!(
            !a.chunks_exact(4).all(|px| px == first),
            "flat image (pipeline no-op?)"
        );
        // Animated: t=0 vs t=3 differ.
        let diff = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
        assert!(diff as f64 / a.len() as f64 > 0.01, "u_time not animating");
    }
}

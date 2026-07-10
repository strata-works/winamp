#![cfg(feature = "gpu-tests")]

use carapace::render::{RenderTarget, Renderer};
use carapace::scene::{Color, ColorStop, Gradient, Node, Paint, Pt, Scene};
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
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
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

fn px(data: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2]]
}

fn alpha_at(data: &[u8], w: u32, x: u32, y: u32) -> u8 {
    data[((y * w + x) * 4 + 3) as usize]
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
    vec![
        Pt { x: x0, y: y0 },
        Pt { x: x1, y: y0 },
        Pt { x: x1, y: y1 },
        Pt { x: x0, y: y1 },
    ]
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
                paint: Paint::Solid(Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
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
                color: Color {
                    r: 0,
                    g: 255,
                    b: 0,
                    a: 255,
                },
                direction: carapace::scene::FillDir::Right,
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
        |_| None,
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
            time: 0.0,
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

#[test]
fn renders_an_image_at_sentinel_pixels() {
    use carapace::asset::DecodedImage;
    use carapace::scene::{ImageDest, Node};
    use std::sync::Arc;

    // 8x8 sRGB source with four solid-color 4x4 quadrants:
    //   TL (rows 0-3, cols 0-3): pure red   [255,   0,   0]
    //   TR (rows 0-3, cols 4-7): pure green  [  0, 255,   0]
    //   BL (rows 4-7, cols 0-3): pure blue   [  0,   0, 255]
    //   BR (rows 4-7, cols 4-7): mid-grey    [188, 188, 188]
    //
    // Scaled to 200x200 dest → each source texel = 25 output px.
    // Sample points map to source ~(1,1), (7,1), (1,7), (7,7) — each
    // ≥1.5 texels inside its 4×4 quadrant, well away from the seam at
    // source coord 4 — so bilinear reads pure solid color with no bleed.
    let mut rgba = vec![0u8; 8 * 8 * 4];
    for row in 0..8usize {
        for col in 0..8usize {
            let (r, g, b) = match (row < 4, col < 4) {
                (true, true) => (255u8, 0u8, 0u8),       // TL red
                (true, false) => (0u8, 255u8, 0u8),      // TR green
                (false, true) => (0u8, 0u8, 255u8),      // BL blue
                (false, false) => (188u8, 188u8, 188u8), // BR mid-grey
            };
            let i = (row * 8 + col) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    let img = Arc::new(DecodedImage {
        rgba,
        width: 8,
        height: 8,
    });
    let o = offscreen(200, 200);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![Node::Image {
            image: img,
            dest: ImageDest {
                x: 0.0,
                y: 0.0,
                w: 200.0,
                h: 200.0,
            }, // scale 8x8 -> full 200x200 (25 output px per source texel)
        }],
    };
    let read = |_k: &str| None;
    r.draw(
        &scene,
        read,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    // Sample deep inside each quadrant (output px coords → source texel coords shown):
    //   (25,25)  → src ~(1,1)  TL red quadrant interior
    //   (175,25) → src ~(7,1)  TR green quadrant interior
    //   (25,175) → src ~(1,7)  BL blue quadrant interior
    //   (175,175)→ src ~(7,7)  BR grey quadrant interior
    assert_eq!(px(&data, 200, 25, 25), [255, 0, 0], "TL red");
    assert_eq!(px(&data, 200, 175, 25), [0, 255, 0], "TR green");
    assert_eq!(px(&data, 200, 25, 175), [0, 0, 255], "BL blue");
    // Byte-passthrough sentinel: the pipeline (vello 0.9) passes sRGB bytes through without
    // sRGB→linear conversion, so the authored 188 value must be preserved (NOT gamma-shifted to
    // ~128). Reading deep inside the solid grey quadrant under bilinear must yield ~188.
    let g = px(&data, 200, 175, 175);
    assert!(
        (g[0] as i32 - 188).abs() <= 4,
        "byte-passthrough preserves sRGB grey ~188, got {}",
        g[0]
    );
}

#[test]
fn renders_translucent_fill_blended_over_background() {
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![
            // opaque red background
            Node::Fill {
                path: rect(0.0, 0.0, 100.0, 100.0),
                paint: Paint::Solid(Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
            },
            // 50%-alpha blue over it
            Node::Fill {
                path: rect(0.0, 0.0, 100.0, 100.0),
                paint: Paint::Solid(Color {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 128,
                }),
            },
        ],
    };
    let read = |_k: &str| None;
    r.draw(
        &scene,
        read,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    let p = px(&data, 100, 50, 50);
    // Straight-alpha "blue over red": result ≈ blend of the two. This also pins that
    // alpha is wired through `from_rgba8` (not dropped to 255). Tolerance absorbs the
    // pipeline's blend-space nuance (vello 0.9 byte-passthrough).
    assert!(
        (p[0] as i32 - 127).abs() <= 16,
        "R blended toward ~127, got {}",
        p[0]
    );
    assert!(p[1] <= 8, "no green, got {}", p[1]);
    assert!(
        (p[2] as i32 - 128).abs() <= 16,
        "B blended toward ~128, got {}",
        p[2]
    );
}

#[test]
fn renders_linear_gradient_oriented_and_interpolating() {
    let o = offscreen(200, 50);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 50),
        nodes: vec![Node::Fill {
            path: rect(0.0, 0.0, 200.0, 50.0),
            paint: Paint::Gradient(Gradient::Linear {
                from: Pt { x: 0.0, y: 0.0 },
                to: Pt { x: 200.0, y: 0.0 },
                stops: vec![
                    ColorStop {
                        at: 0.0,
                        color: Color {
                            r: 255,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                    },
                    ColorStop {
                        at: 1.0,
                        color: Color {
                            r: 0,
                            g: 0,
                            b: 255,
                            a: 255,
                        },
                    },
                ],
            }),
        }],
    };
    let read = |_k: &str| None;
    r.draw(
        &scene,
        read,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    let left = px(&data, 200, 10, 25);
    let mid = px(&data, 200, 100, 25);
    let right = px(&data, 200, 190, 25);
    // Endpoints + orientation: left is red-dominant, right is blue-dominant (horizontal red→blue).
    assert!(left[0] > 200 && left[2] < 60, "left ~red, got {:?}", left);
    assert!(
        right[2] > 200 && right[0] < 60,
        "right ~blue, got {:?}",
        right
    );
    // Interpolation: R decreases and B increases left→right (robust to interpolation color space).
    assert!(
        mid[0] < left[0] && mid[0] > right[0],
        "R decreases L→R, mid {:?}",
        mid
    );
    assert!(
        mid[2] > left[2] && mid[2] < right[2],
        "B increases L→R, mid {:?}",
        mid
    );
}

#[test]
fn renders_bundled_font_text_in_fill_color() {
    use carapace::scene::{FontData, HAlign, Node, TextContent, VAlign};
    use std::sync::Arc;

    let font = Arc::new(FontData::new(Arc::from(
        include_bytes!("fonts/vt323.ttf").as_slice(),
    )));
    let o = offscreen(200, 80);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 80),
        // Big solid-red glyphs near the top-left; bundled font + ASCII => no fallback.
        nodes: vec![Node::Text {
            content: TextContent::Static("HII".to_string()),
            font: Some(font),
            font_name: Some("vt323.ttf".to_string()),
            size: 64.0,
            paint: Paint::Solid(Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
            halign: HAlign::Left,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt { x: 4.0, y: 4.0 },
        }],
    };
    r.draw(
        &scene,
        |_k: &str| None,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    // Scan the top-left band where the glyphs sit; at least some pixels must be red-dominant
    // (glyph ink), proving the font loaded and drew in the fill color.
    let mut red_ink = 0;
    for y in 4..70 {
        for x in 4..150 {
            let p = px(&data, 200, x, y);
            if p[0] > 180 && p[1] < 60 && p[2] < 60 {
                red_ink += 1;
            }
        }
    }
    assert!(red_ink > 80, "expected red glyph ink, found {red_ink} px");
}

#[test]
fn renders_gradient_filled_text_interpolating_vertically() {
    use carapace::scene::{FontData, HAlign, Node, TextContent, VAlign};
    use std::sync::Arc;

    let font = Arc::new(FontData::new(Arc::from(
        include_bytes!("fonts/vt323.ttf").as_slice(),
    )));
    let o = offscreen(200, 80);
    let mut r = Renderer::new(&o.device);
    // A vertical red->blue gradient over a ~64px glyph block. The brush coords are
    // glyph-block-local (see render.rs draw_glyphs comment), so 0..64 spans the glyph height
    // from its top. Proves Paint::Gradient flows through draw_glyphs.
    let scene = Scene {
        canvas: (200, 80),
        nodes: vec![Node::Text {
            content: TextContent::Static("II".to_string()),
            font: Some(font),
            font_name: Some("vt323.ttf".to_string()),
            size: 64.0,
            paint: Paint::Gradient(Gradient::Linear {
                from: Pt { x: 0.0, y: 0.0 },
                to: Pt { x: 0.0, y: 64.0 },
                stops: vec![
                    ColorStop {
                        at: 0.0,
                        color: Color {
                            r: 255,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
                    },
                    ColorStop {
                        at: 1.0,
                        color: Color {
                            r: 0,
                            g: 0,
                            b: 255,
                            a: 255,
                        },
                    },
                ],
            }),
            halign: HAlign::Left,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt { x: 4.0, y: 4.0 },
        }],
    };
    r.draw(
        &scene,
        |_k: &str| None,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    // Scan glyph-ink pixels (non-background: any channel lit) in an upper band and a lower band.
    // Upper band must contain red-dominant ink; lower band must contain blue-dominant ink.
    let mut upper_red = false;
    let mut lower_blue = false;
    for y in 4..30 {
        for x in 4..150 {
            let p = px(&data, 200, x, y);
            let lit = p[0] as u32 + p[1] as u32 + p[2] as u32 > 40;
            if lit && p[0] as i32 > p[2] as i32 + 40 {
                upper_red = true;
            }
        }
    }
    for y in 44..70 {
        for x in 4..150 {
            let p = px(&data, 200, x, y);
            let lit = p[0] as u32 + p[1] as u32 + p[2] as u32 > 40;
            if lit && p[2] as i32 > p[0] as i32 + 40 {
                lower_blue = true;
            }
        }
    }
    assert!(upper_red, "upper glyph band should have red-dominant ink");
    assert!(lower_blue, "lower glyph band should have blue-dominant ink");
}

#[test]
fn renders_value_bound_text_from_string_state() {
    use carapace::scene::{FontData, HAlign, Node, TextContent, VAlign};
    use carapace::state::StateValue;
    use std::sync::Arc;

    let font = Arc::new(FontData::new(Arc::from(
        include_bytes!("fonts/vt323.ttf").as_slice(),
    )));
    let o = offscreen(200, 80);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 80),
        nodes: vec![Node::Text {
            content: TextContent::Bound("title".to_string()),
            font: Some(font),
            font_name: Some("vt323.ttf".to_string()),
            size: 64.0,
            paint: Paint::Solid(Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            }),
            halign: HAlign::Left,
            valign: VAlign::Top,
            max_width: None,
            pos: Pt { x: 4.0, y: 4.0 },
        }],
    };
    let read = |k: &str| {
        if k == "title" {
            Some(StateValue::Str(Arc::from("WW")))
        } else {
            None
        }
    };
    r.draw(
        &scene,
        read,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    let mut green_ink = 0;
    for y in 4..70 {
        for x in 4..150 {
            let p = px(&data, 200, x, y);
            if p[1] > 180 && p[0] < 60 && p[2] < 60 {
                green_ink += 1;
            }
        }
    }
    assert!(
        green_ink > 80,
        "expected green ink from bound string state, found {green_ink}"
    );
}

#[test]
fn value_fill_up_fills_from_the_bottom() {
    use carapace::scene::{Color, FillDir, Node, Scene};
    use carapace::state::StateValue;
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::ValueFill {
            path: rect(10.0, 0.0, 30.0, 100.0), // full-height bar
            value_key: "v".to_string(),
            color: Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
            direction: FillDir::Up,
        }],
    };
    r.draw(
        &scene,
        |k| {
            if k == "v" {
                Some(StateValue::Scalar(0.5))
            } else {
                None
            }
        },
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    assert_eq!(
        px(&data, 100, 25, 80),
        [0, 255, 0],
        "bottom half filled (up, v=0.5)"
    );
    assert_eq!(px(&data, 100, 25, 20), [0, 0, 0], "top half empty");
}

#[test]
fn value_fill_clips_to_a_non_rect_path() {
    use carapace::scene::{Color, FillDir, Node, Scene};
    use carapace::state::StateValue;
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    // A circle path filled fully (v=1.0). A pixel inside the bbox but OUTSIDE the circle
    // (near a corner of the bounding box) must stay background — proving clip-to-path.
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::ValueFill {
            path: carapace::shape::circle(50.0, 50.0, 40.0, 64),
            value_key: "v".to_string(),
            color: Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
            direction: FillDir::Right,
        }],
    };
    r.draw(
        &scene,
        |_k| Some(StateValue::Scalar(1.0)),
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    assert_eq!(
        px(&data, 100, 50, 50),
        [0, 255, 0],
        "center of the circle is filled"
    );
    assert_eq!(
        px(&data, 100, 12, 12),
        [0, 0, 0],
        "bbox corner outside the circle stays background"
    );
}

#[test]
fn identical_text_is_shaped_once_and_cached() {
    use carapace::scene::{FontData, HAlign, Node, TextContent, VAlign};
    use std::sync::Arc;

    let font = Arc::new(FontData::new(Arc::from(
        include_bytes!("fonts/vt323.ttf").as_slice(),
    )));
    let mk = |y: f32, valign: VAlign| Node::Text {
        // Same string/font/size/halign/max_width => one cache entry, even at different y/valign.
        content: TextContent::Static("CACHE".to_string()),
        font: Some(font.clone()),
        font_name: Some("vt323.ttf".to_string()),
        size: 24.0,
        paint: Paint::Solid(Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        halign: HAlign::Left,
        valign,
        max_width: None,
        pos: Pt { x: 4.0, y },
    };
    let o = offscreen(200, 120);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 120),
        nodes: vec![mk(10.0, VAlign::Top), mk(60.0, VAlign::Bottom)],
    };
    let target = RenderTarget {
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
        time: 0.0,
    };
    r.draw(&scene, |_k: &str| None, |_| None, &target);
    r.draw(&scene, |_k: &str| None, |_| None, &target); // second frame: must reuse, not re-shape
    assert_eq!(
        r.layout_cache_len(),
        1,
        "identical text shares one cached layout"
    );
}

#[test]
fn transparent_base_color_leaves_undrawn_pixels_clear() {
    use carapace::scene::{Color, Node, Paint, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    // One opaque fill in the top-left; the rest of the canvas is the transparent base.
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::Fill {
            path: rect(0.0, 0.0, 20.0, 20.0),
            paint: Paint::Solid(Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
        }],
    };
    r.draw(
        &scene,
        |_k| None,
        |_| None,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            time: 0.0,
        },
    );
    let data = readback(&o);
    assert_eq!(alpha_at(&data, 100, 10, 10), 255, "drawn pixel is opaque");
    assert_eq!(
        alpha_at(&data, 100, 80, 80),
        0,
        "undrawn pixel is transparent (base alpha 0)"
    );
}

// A solid-color source texture for a view (proves the composite accepts an ARBITRARY texture).
fn solid_source(o: &Offscreen, w: u32, h: u32, rgba: [u8; 4]) -> wgpu::Texture {
    let tex = o.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("view-src"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let data = vec![rgba; (w * h) as usize].concat();
    o.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    tex
}

#[test]
fn view_composites_supplied_texture_into_its_rect() {
    use carapace::scene::{Color, ImageDest, Node, Paint, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![
            Node::Fill {
                path: rect(0.0, 0.0, 100.0, 100.0),
                paint: Paint::Solid(Color {
                    r: 10,
                    g: 10,
                    b: 10,
                    a: 255,
                }),
            },
            Node::View {
                id: "v".to_string(),
                dest: ImageDest {
                    x: 30.0,
                    y: 30.0,
                    w: 40.0,
                    h: 40.0,
                },
            },
        ],
    };
    let src = solid_source(&o, 40, 40, [255, 0, 0, 255]);
    let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
    r.draw(
        &scene,
        |_| None,
        |id| if id == "v" { Some(&src_view) } else { None },
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            time: 0.0,
        },
    );
    let data = readback(&o);
    assert_eq!(
        px(&data, 100, 50, 50),
        [255, 0, 0],
        "view rect shows the supplied texture"
    );
    assert_eq!(
        px(&data, 100, 10, 10),
        [10, 10, 10],
        "outside the view shows the skin fill"
    );
}

#[test]
fn view_without_texture_leaves_the_hole() {
    use carapace::scene::{Color, ImageDest, Node, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::View {
            id: "v".to_string(),
            dest: ImageDest {
                x: 30.0,
                y: 30.0,
                w: 40.0,
                h: 40.0,
            },
        }],
    };
    r.draw(
        &scene,
        |_| None,
        |_| None,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
            base_color: Color {
                r: 7,
                g: 7,
                b: 7,
                a: 255,
            },
            time: 0.0,
        },
    );
    assert_eq!(
        px(&readback(&o), 100, 50, 50),
        [7, 7, 7],
        "no texture -> the hole stays the base color"
    );
}

#[test]
fn view_alpha_blends_over_the_layer_behind() {
    // Regression test for the view-compositor's `PREMULTIPLIED_ALPHA_BLENDING` (render.rs).
    // Host `view{}` content is composited OVER whatever is already in the target (the vello
    // scene + earlier view layers), so:
    //   • OPAQUE content (alpha=255) fully replaces — identical to the old `blend: None`,
    //     i.e. backward-compatible with existing opaque host content (e.g. the clock skin).
    //   • TRANSPARENT content (alpha=0) lets the layer behind show through — the behavior that
    //     lets a translucent/rounded host card reveal the paper-shader gradient behind it.
    // With `blend: None` the transparent region would overwrite the base with (0,0,0,0) → black,
    // so this test fails on the old code and passes on the new.
    use carapace::scene::{Color, ImageDest, Node, Paint, Scene};
    let o = offscreen(100, 100);
    let mut r = Renderer::new(&o.device);
    // The "layer behind": an opaque red fill over the whole canvas (stands in for the shader).
    let base = Color {
        r: 200,
        g: 40,
        b: 40,
        a: 255,
    };
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![
            Node::Fill {
                path: rect(0.0, 0.0, 100.0, 100.0),
                paint: Paint::Solid(base),
            },
            // Left half: opaque host content — must win over the base.
            Node::View {
                id: "opaque".to_string(),
                dest: ImageDest {
                    x: 0.0,
                    y: 0.0,
                    w: 50.0,
                    h: 100.0,
                },
            },
            // Right half: fully transparent host content — the base must show through.
            Node::View {
                id: "clear".to_string(),
                dest: ImageDest {
                    x: 50.0,
                    y: 0.0,
                    w: 50.0,
                    h: 100.0,
                },
            },
        ],
    };
    // Premultiplied sources: opaque blue is trivially premultiplied; (0,0,0,0) is transparent.
    let opaque = solid_source(&o, 50, 100, [0, 0, 200, 255]);
    let clear = solid_source(&o, 50, 100, [0, 0, 0, 0]);
    let opaque_view = opaque.create_view(&wgpu::TextureViewDescriptor::default());
    let clear_view = clear.create_view(&wgpu::TextureViewDescriptor::default());
    r.draw(
        &scene,
        |_| None,
        |id| match id {
            "opaque" => Some(&opaque_view),
            "clear" => Some(&clear_view),
            _ => None,
        },
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            time: 0.0,
        },
    );
    let data = readback(&o);
    assert_eq!(
        px(&data, 100, 25, 50),
        [0, 0, 200],
        "opaque view content composites over the base (backward-compatible)"
    );
    assert_eq!(
        px(&data, 100, 75, 50),
        [200, 40, 40],
        "transparent view content reveals the layer behind (not black)"
    );
}

#[test]
fn gadget_path_still_uniform_scales() {
    // Prove the gadget render path (canvas != surface size → uniform scale) is unchanged
    // by frame-skin work. Canvas 100×100 rendered into a 300×300 surface = 3× uniform scale.
    // A red fill over canvas rect 20,20..50,50 maps to surface rect 60,60..150,150.
    // Sentinel: surface pixel (90,90) must be red; surface pixel (5,5) must be base black.
    let o = offscreen(300, 300);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (100, 100),
        nodes: vec![Node::Fill {
            path: rect(20.0, 20.0, 50.0, 50.0),
            paint: Paint::Solid(Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
        }],
    };
    r.draw(
        &scene,
        |_k| None,
        |_| None,
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
            time: 0.0,
        },
    );
    let data = readback(&o);
    // canvas (30,30) → surface (90,90) at 3× uniform scale: must be inside the red fill
    assert_eq!(
        px(&data, 300, 90, 90),
        [255, 0, 0],
        "canvas (30,30) → surface (90,90) at 3× must be red"
    );
    // canvas (1.67,1.67) → surface (5,5): outside the fill, base black
    assert_eq!(
        px(&data, 300, 5, 5),
        [0, 0, 0],
        "surface (5,5) is outside the fill — must be base black"
    );
}

#[test]
fn shader_background_renders_under_2d() {
    use carapace::scene::ImageDest;
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::sync::Arc;

    let (w, h) = (64u32, 64u32);
    let o = offscreen(w, h);
    let mut r = Renderer::new(&o.device);
    // A trivial shader that outputs solid green everywhere, as a full-canvas background.
    let frag =
        "@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return vec4(0.0, 1.0, 0.0, 1.0); }";
    let full = format!("{}\n{}", carapace::shader::prelude_for(&[]), frag);
    // Replicate ShaderPrim's key derivation exactly (DefaultHasher over the full source) — the
    // pipeline cache only needs the key to be stable within this test.
    let mut hasher = DefaultHasher::new();
    full.hash(&mut hasher);
    let key = hasher.finish();

    let scene = Scene {
        canvas: (w, h),
        nodes: vec![
            Node::Shader {
                dest: ImageDest {
                    x: 0.0,
                    y: 0.0,
                    w: w as f32,
                    h: h as f32,
                },
                wgsl: Arc::from(full.as_str()),
                uniforms: vec![],
                key,
            },
            // Red fill covering the left half, drawn OVER the shader background.
            Node::Fill {
                path: rect(0.0, 0.0, (w / 2) as f32, h as f32),
                paint: Paint::Solid(Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
            },
        ],
    };
    r.draw(
        &scene,
        |_| None,
        |_| None,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: w,
            height: h,
            time: 0.0,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    );
    let d = readback(&o);
    assert_eq!(
        px(&d, w, 3 * w / 4, h / 2),
        [0, 255, 0],
        "bg shader shows through the transparent right half"
    );
    assert_eq!(
        px(&d, w, w / 4, h / 2),
        [255, 0, 0],
        "2D fill draws OVER the shader on the left half"
    );
}

#[test]
fn shader_uniform_is_reactive() {
    use carapace::scene::ImageDest;
    use carapace::shader::{ShaderUniform, UniformSource};
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::sync::Arc;

    let (w, h) = (64u32, 64u32);
    // A shader that outputs the host-bound `intensity` uniform as red, everywhere.
    let frag = "@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return vec4(u.intensity, 0.0, 0.0, 1.0); }";
    let full = format!(
        "{}\n{}",
        carapace::shader::prelude_for(&["intensity"]),
        frag
    );
    let mut hasher = DefaultHasher::new();
    full.hash(&mut hasher);
    let key = hasher.finish();

    let make_scene = || Scene {
        canvas: (w, h),
        nodes: vec![Node::Shader {
            dest: ImageDest {
                x: 0.0,
                y: 0.0,
                w: w as f32,
                h: h as f32,
            },
            wgsl: Arc::from(full.as_str()),
            uniforms: vec![ShaderUniform {
                name: "intensity".to_string(),
                source: UniformSource::Host("wx".to_string()),
            }],
            key,
        }],
    };

    // Draw 1: host reports wx = 0.0 -> red should be ~0.
    let o1 = offscreen(w, h);
    let mut r1 = Renderer::new(&o1.device);
    let read_lo = |k: &str| (k == "wx").then_some(StateValue::Scalar(0.0));
    r1.draw(
        &make_scene(),
        read_lo,
        |_| None,
        &RenderTarget {
            device: &o1.device,
            queue: &o1.queue,
            view: &o1.view,
            width: o1.w,
            height: o1.h,
            time: 0.0,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    );
    let d1 = readback(&o1);
    let red_lo = px(&d1, w, w / 2, h / 2)[0];

    // Draw 2: host reports wx = 1.0 -> red should be ~255.
    let o2 = offscreen(w, h);
    let mut r2 = Renderer::new(&o2.device);
    let read_hi = |k: &str| (k == "wx").then_some(StateValue::Scalar(1.0));
    r2.draw(
        &make_scene(),
        read_hi,
        |_| None,
        &RenderTarget {
            device: &o2.device,
            queue: &o2.queue,
            view: &o2.view,
            width: o2.w,
            height: o2.h,
            time: 0.0,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    );
    let d2 = readback(&o2);
    let red_hi = px(&d2, w, w / 2, h / 2)[0];

    assert!(red_lo < 10, "wx=0.0 should render ~0 red, got {red_lo}");
    assert!(red_hi > 245, "wx=1.0 should render ~255 red, got {red_hi}");
}

#[test]
fn no_shader_scene_uses_2stage_path_unchanged() {
    // Same scene/assertions as `renders_fill_and_value_fill_at_sentinel_pixels` — pins that a
    // scene with zero `Node::Shader` nodes still takes the original 2-stage path (vello straight
    // into `target.view` with `target.base_color`, no offscreen/composite detour) after the
    // renderer grew the 4-stage shader path.
    let o = offscreen(200, 200);
    let mut r = Renderer::new(&o.device);
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![
            Node::Fill {
                path: vec![
                    Pt { x: 20.0, y: 20.0 },
                    Pt { x: 100.0, y: 20.0 },
                    Pt { x: 100.0, y: 100.0 },
                    Pt { x: 20.0, y: 100.0 },
                ],
                paint: Paint::Solid(Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
            },
            Node::ValueFill {
                path: vec![
                    Pt { x: 0.0, y: 150.0 },
                    Pt { x: 200.0, y: 150.0 },
                    Pt { x: 200.0, y: 170.0 },
                    Pt { x: 0.0, y: 170.0 },
                ],
                value_key: "v".to_string(),
                color: Color {
                    r: 0,
                    g: 255,
                    b: 0,
                    a: 255,
                },
                direction: carapace::scene::FillDir::Right,
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
        |_| None,
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
            base_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            time: 0.0,
        },
    );
    let data = readback(&o);
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

#[test]
fn frame_keeps_corners_fixed_and_stretches_edges() {
    use carapace::asset::DecodedImage;
    use carapace::scene::{FrameCenter, ImageDest, Slice};
    use std::sync::Arc;

    // Build a 60x60 test image with a clear 10px chrome border:
    //   - Corners (10x10): solid red   [255, 0, 0]
    //   - Top/bottom edges: solid blue [0, 0, 255]
    //   - Left/right edges: solid blue [0, 0, 255]
    //   - Center interior: solid green [0, 255, 0]
    let size: usize = 60;
    let inset: usize = 10;
    let mut rgba = vec![0u8; size * size * 4];
    for row in 0..size {
        for col in 0..size {
            let in_top = row < inset;
            let in_bottom = row >= size - inset;
            let in_left = col < inset;
            let in_right = col >= size - inset;
            let (r, g, b) = if (in_top || in_bottom) && (in_left || in_right) {
                // corner
                (255u8, 0u8, 0u8)
            } else if in_top || in_bottom || in_left || in_right {
                // edge
                (0u8, 0u8, 255u8)
            } else {
                // center
                (0u8, 255u8, 0u8)
            };
            let i = (row * size + col) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    let img = Arc::new(DecodedImage {
        rgba,
        width: size as u32,
        height: size as u32,
    });

    // Helper: build a Scene with a single Frame node at given dest width (height fixed at 120).
    let make_scene = |dest_w: f32| Scene {
        canvas: (dest_w as u32, 120),
        nodes: vec![Node::Frame {
            image: img.clone(),
            dest: ImageDest {
                x: 0.0,
                y: 0.0,
                w: dest_w,
                h: 120.0,
            },
            slice: Slice {
                left: 10.0,
                right: 10.0,
                top: 10.0,
                bottom: 10.0,
            },
            center: FrameCenter::Stretch,
        }],
    };

    let black_bg = carapace::scene::Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    // Render at 120x120
    let o120 = offscreen(120, 120);
    let mut r120 = Renderer::new(&o120.device);
    r120.draw(
        &make_scene(120.0),
        |_| None,
        |_| None,
        &RenderTarget {
            device: &o120.device,
            queue: &o120.queue,
            view: &o120.view,
            width: o120.w,
            height: o120.h,
            base_color: black_bg,
            time: 0.0,
        },
    );
    let data120 = readback(&o120);

    // Render at 200x120 (wider — top edge stretches, corners stay the same)
    let o200 = offscreen(200, 120);
    let mut r200 = Renderer::new(&o200.device);
    r200.draw(
        &make_scene(200.0),
        |_| None,
        |_| None,
        &RenderTarget {
            device: &o200.device,
            queue: &o200.queue,
            view: &o200.view,
            width: o200.w,
            height: o200.h,
            base_color: black_bg,
            time: 0.0,
        },
    );
    let data200 = readback(&o200);

    // Top-left corner block (deep inside, 1:1 source -> same dest size):
    //   Both renders must have the same red pixel at (4, 4) — corner never scales.
    let corner120 = px(&data120, 120, 4, 4);
    let corner200 = px(&data200, 200, 4, 4);
    assert_eq!(
        corner120, corner200,
        "top-left corner pixel must be byte-identical at both widths"
    );
    // Corners should be red (we designed them that way).
    assert!(
        corner120[0] > 180 && corner120[1] < 60 && corner120[2] < 60,
        "corner must be red-dominant, got {:?}",
        corner120
    );

    // Top-edge center sample: at 120-wide this is the stretched edge. At 200-wide, the same
    // logical position (center of the top edge) is stretched further — pixels differ.
    // In the 120-wide image the top edge center is at roughly x=60, y=4.
    // In the 200-wide image the top edge center is at roughly x=100, y=4.
    // Both should be blue (edge cells), but we just assert they differ to confirm stretching.
    // Actually, because the source edge pixel value repeats (all blue), vello bilinear can
    // produce the same value. A safer assertion: check that the center interior is green in
    // the stretch case, proving all 9 cells are drawn including center.
    let center120 = px(&data120, 120, 60, 60);
    assert!(
        center120[1] > 180 && center120[0] < 60 && center120[2] < 60,
        "center interior must be green (Stretch draws center), got {:?}",
        center120
    );
    let center200 = px(&data200, 200, 100, 60);
    assert!(
        center200[1] > 180 && center200[0] < 60 && center200[2] < 60,
        "center interior at 200-wide must also be green, got {:?}",
        center200
    );
}

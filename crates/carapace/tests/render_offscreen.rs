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
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
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
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
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
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
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
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
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
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
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
        &RenderTarget {
            device: &o.device,
            queue: &o.queue,
            view: &o.view,
            width: o.w,
            height: o.h,
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
    };
    r.draw(&scene, |_k: &str| None, &target);
    r.draw(&scene, |_k: &str| None, &target); // second frame: must reuse, not re-shape
    assert_eq!(
        r.layout_cache_len(),
        1,
        "identical text shares one cached layout"
    );
}

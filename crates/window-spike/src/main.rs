//! THROWAWAY feasibility spike — total window replacement. Not production code.
//! Proves a borderless/transparent/draggable window rendering the real Headspace skin.

use std::sync::Arc;

use carapace::scene::Node;
use carapace_demo::demo_host::DemoHost;
use vello::kurbo::Affine;
use vello::peniko::{Blob, Color as VColor, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use vello::{AaConfig, RenderParams, Scene as VScene};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const SCALE: u32 = 2;

/// The decoded skin bitmap + its native size, extracted from the real reference skin.
struct Bitmap {
    rgba: Arc<Vec<u8>>,
    w: u32,
    h: u32,
}

fn load_headspace_bitmap() -> Bitmap {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../carapace-demo/skins/reference");
    let (_m, source) = carapace::skin::load_dir(&dir).expect("load reference skin");
    let engine = carapace::engine::Engine::new(
        Box::new(DemoHost::new()),
        carapace::vocab::VocabRegistry::base(),
        source,
    )
    .expect("build engine");
    // The reference skin's first Image node is the Headspace faceplate.
    for node in &engine.scene().nodes {
        if let Node::Image { image, .. } = node {
            return Bitmap {
                rgba: Arc::new(image.rgba.clone()),
                w: image.width,
                h: image.height,
            };
        }
    }
    panic!("reference skin has no Image node");
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
    intermediate: wgpu::TextureView,
}

struct App {
    bitmap: Bitmap,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    renderer: Option<vello::Renderer>,
    cursor: (f64, f64),
    clickthrough: bool,
}

impl App {
    fn new() -> Self {
        Self {
            bitmap: load_headspace_bitmap(),
            window: None,
            gpu: None,
            renderer: None,
            cursor: (0.0, 0.0),
            clickthrough: false,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let (cw, ch) = (self.bitmap.w, self.bitmap.h);
        let attrs = Window::default_attributes()
            .with_title("window-spike")
            .with_decorations(false) // no OS chrome
            .with_transparent(true) // transparent window
            .with_inner_size(winit::dpi::LogicalSize::new(cw * SCALE, ch * SCALE));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let phys = window.inner_size();
        let (pw, ph) = (phys.width.max(1), phys.height.max(1));

        let gpu = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let surface = instance.create_surface(window.clone()).expect("surface");
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .expect("adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("device");
            let caps = surface.get_capabilities(&adapter);
            let surface_format = *caps.formats.first().expect("format");
            // FINDING: record which alpha modes the macOS adapter offers. Prefer PreMultiplied;
            // fall back to PostMultiplied; Opaque would defeat transparency.
            eprintln!("surface alpha_modes offered: {:?}", caps.alpha_modes);
            let alpha_mode = if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                caps.alpha_modes[0]
            };
            eprintln!("using alpha_mode: {alpha_mode:?}");
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: pw,
                height: ph,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            let blitter = wgpu::util::TextureBlitter::new(&device, surface_format);
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("intermediate"),
                size: wgpu::Extent3d {
                    width: pw,
                    height: ph,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let intermediate = tex.create_view(&wgpu::TextureViewDescriptor::default());
            Gpu {
                surface,
                device,
                queue,
                config,
                blitter,
                intermediate,
            }
        });
        self.renderer = Some(
            vello::Renderer::new(
                &gpu.device,
                vello::RendererOptions {
                    use_cpu: false,
                    antialiasing_support: vello::AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .expect("vello renderer"),
        );
        self.window = Some(window.clone());
        self.gpu = Some(gpu);
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let (Some(gpu), Some(renderer), Some(_win)) =
                    (self.gpu.as_mut(), self.renderer.as_mut(), self.window.as_ref())
                else {
                    return;
                };
                let frame = match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => return,
                };
                let surface_view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Build a vello scene: draw the bitmap scaled to fill, on a TRANSPARENT base.
                let mut vs = VScene::new();
                let img = ImageData {
                    data: Blob::new(self.bitmap.rgba.clone()),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: self.bitmap.w,
                    height: self.bitmap.h,
                };
                let sx = gpu.config.width as f64 / self.bitmap.w as f64;
                let sy = gpu.config.height as f64 / self.bitmap.h as f64;
                vs.draw_image(
                    ImageBrush::new(img).as_ref(),
                    Affine::scale_non_uniform(sx, sy),
                );
                renderer
                    .render_to_texture(
                        &gpu.device,
                        &gpu.queue,
                        &vs,
                        &gpu.intermediate,
                        &RenderParams {
                            // TRANSPARENT base — the whole point of the spike.
                            base_color: VColor::from_rgba8(0, 0, 0, 0),
                            width: gpu.config.width,
                            height: gpu.config.height,
                            antialiasing_method: AaConfig::Area,
                        },
                    )
                    .expect("render");
                let mut enc = gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                gpu.blitter
                    .copy(&gpu.device, &mut enc, &gpu.intermediate, &surface_view);
                gpu.queue.submit(Some(enc.finish()));
                frame.present();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                let Some(win) = self.window.as_ref() else { return };
                let size = win.inner_size();
                // cursor (physical) -> canvas coords
                let cx = self.cursor.0 * self.bitmap.w as f64 / size.width.max(1) as f64;
                let cy = self.cursor.1 * self.bitmap.h as f64 / size.height.max(1) as f64;
                // Eyeballed Headspace min/close glyph rects (canvas space) — tune by observation.
                let in_rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
                    cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1
                };
                if in_rect(150.0, 4.0, 170.0, 24.0) {
                    win.set_minimized(true); // minimize glyph
                } else if in_rect(172.0, 4.0, 194.0, 24.0) {
                    event_loop.exit(); // close glyph
                } else {
                    let _ = win.drag_window(); // press on body -> move window
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        logical_key: winit::keyboard::Key::Character(ref c),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } if c == "t" => {
                // Tier 3 probe: toggle whole-window click-through.
                self.clickthrough = !self.clickthrough;
                if let Some(win) = self.window.as_ref() {
                    let _ = win.set_cursor_hittest(!self.clickthrough);
                    eprintln!("cursor_hittest set to {}", !self.clickthrough);
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut App::new()).unwrap();
}

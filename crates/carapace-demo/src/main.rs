use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use carapace::command::{Command, SkinSource};
use carapace::engine::{Engine, PointerEvent};
use carapace::render::{RenderTarget, Renderer};
use carapace::scene::Pt;
use carapace::vocab::VocabRegistry;
use carapace_demo::demo_host::DemoHost;
use carapace_demo::sysmon_host::SysmonHost;
use carapace_demo::window::{WindowOp, WindowOutbox};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const MONITOR_SKIN: &str = "\
    fill{ path = rect{x=0,y=0,w=186,h=150}, color = {r=12,g=16,b=22} }\n\
    gauge{ x = 16,  y = 20, value = 'cpu',  label = 'CPU' }\n\
    gauge{ x = 76,  y = 20, value = 'mem',  label = 'MEM' }\n\
    gauge{ x = 136, y = 20, value = 'swap', label = 'SWP' }\n";

/// A self-contained sub-renderer that paints a live CPU/MEM/SWP gauge into an off-screen
/// texture. The texture view is supplied to the main `Renderer::draw` as the `"display"` view.
struct Monitor {
    engine: carapace::engine::Engine,
    renderer: Renderer,
    /// Held for ownership only — the TextureView references this texture's memory.
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}

impl Monitor {
    fn new(device: &wgpu::Device, outbox: WindowOutbox) -> Self {
        let mut reg = VocabRegistry::base();
        reg.register(Box::new(carapace_demo::gauge::GaugePrim));
        let engine = Engine::new(
            Box::new(carapace_demo::sysmon_host::SysmonHost::with_outbox(outbox)),
            reg,
            carapace::command::SkinSource::inline(MONITOR_SKIN, (186, 150)),
        )
        .unwrap();
        let (_tex, view) = Self::make_tex(device, 186, 150);
        Self {
            engine,
            renderer: Renderer::new(device),
            _tex,
            view,
            size: (186, 150),
        }
    }

    fn make_tex(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("monitor"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    fn paint(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, dt: std::time::Duration) {
        self.engine.update(dt);
        self.renderer.draw(
            self.engine.scene(),
            |k| self.engine.state(k),
            |_| None,
            &RenderTarget {
                device,
                queue,
                view: &self.view,
                width: self.size.0,
                height: self.size.1,
                base_color: carapace::scene::Color {
                    r: 12,
                    g: 16,
                    b: 22,
                    a: 255,
                },
            },
        );
    }
}

const MEDIA_SKINS: &[&str] = &[
    "skins/classic",
    "skins/minimal",
    "skins/reference",
    "skins/transport",
];
const SYSMON_SKINS: &[&str] = &["skins/sysmon"];
const INIT_SCALE: u32 = 3;

fn skin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn demo_registry() -> VocabRegistry {
    let mut r = VocabRegistry::base();
    r.register(Box::new(carapace_demo::transport::TransportPrim));
    r.register(Box::new(carapace_demo::gauge::GaugePrim));
    r
}

fn load_source_from(list: &[&str], i: usize) -> (SkinSource, (u32, u32)) {
    let (_m, src) = carapace::skin::load_dir(&skin_root().join(list[i])).expect("load skin");
    let canvas = src.canvas;
    (src, canvas)
}

/// GPU state: wgpu surface + device + queue + surface config + blitter + intermediate texture.
struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// Shader-based blitter from the intermediate Rgba8Unorm storage texture to the surface.
    blitter: wgpu::util::TextureBlitter,
    /// Intermediate Rgba8Unorm texture sized to the surface; recreated on resize.
    intermediate: (wgpu::Texture, wgpu::TextureView),
}

impl Gpu {
    fn make_intermediate(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vello-intermediate"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // STORAGE_BINDING for vello render_to_texture; TEXTURE_BINDING so the blitter
            // can sample it; RENDER_ATTACHMENT so the view-composite render pass can write to it.
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    fn reconfigure(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.intermediate =
            Self::make_intermediate(&self.device, self.config.width, self.config.height);
    }
}

struct App {
    skin_index: usize,
    sysmon: bool,
    engine: Engine,
    cursor: (f64, f64),
    last: Instant,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    renderer: Option<Renderer>,
    monitor: Option<Monitor>,
    window_outbox: WindowOutbox,
}

impl App {
    fn new() -> Self {
        let window_outbox: WindowOutbox = Default::default();
        let (src, _canvas) = load_source_from(MEDIA_SKINS, 0);
        let engine = Engine::new(
            Box::new(DemoHost::with_outbox(window_outbox.clone())),
            demo_registry(),
            src,
        )
        .unwrap();
        Self {
            skin_index: 0,
            sysmon: false,
            engine,
            cursor: (0.0, 0.0),
            last: Instant::now(),
            window: None,
            gpu: None,
            renderer: None,
            monitor: None,
            window_outbox,
        }
    }

    fn apply_window_ops(&self, event_loop: &ActiveEventLoop) {
        for op in self.window_outbox.borrow_mut().drain(..) {
            match op {
                WindowOp::BeginDrag => {
                    if let Some(w) = &self.window {
                        let _ = w.drag_window();
                    }
                }
                WindowOp::Minimize => {
                    if let Some(w) = &self.window {
                        w.set_minimized(true);
                    }
                }
                WindowOp::Close => event_loop.exit(),
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Guard: winit may call resumed again on foreground transitions; create once.
        if self.window.is_some() {
            return;
        }

        // Size window from the initial skin canvas * scale.
        let (cw, ch) = self.engine.scene().canvas;
        let attrs = Window::default_attributes()
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(winit::dpi::LogicalSize::new(
                cw * INIT_SCALE,
                ch * INIT_SCALE,
            ));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let phys = window.inner_size();
        let (pw, ph) = (phys.width.max(1), phys.height.max(1));

        let gpu = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            // Surface must be created from Arc<Window> for 'static lifetime.
            let surface = instance
                .create_surface(window.clone())
                .expect("create wgpu surface");

            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
                .expect("no compatible wgpu adapter");

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("create wgpu device");

            let caps = surface.get_capabilities(&adapter);
            // Use the first supported format (typically Bgra8Unorm on macOS Metal).
            let surface_format = *caps
                .formats
                .first()
                .expect("surface has no supported formats");

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

            // Blitter: converts the Rgba8Unorm intermediate texture to the surface format.
            let blitter = wgpu::util::TextureBlitter::new(&device, surface_format);

            let intermediate = Gpu::make_intermediate(&device, pw, ph);

            Gpu {
                surface,
                device,
                queue,
                config,
                blitter,
                intermediate,
            }
        });

        let renderer = Renderer::new(&gpu.device);
        let monitor = Monitor::new(&gpu.device, self.window_outbox.clone());

        self.window = Some(window.clone());
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.monitor = Some(monitor);

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.reconfigure(size.width, size.height);
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now - self.last;
                self.last = now;

                self.engine.update(dt);
                self.apply_window_ops(event_loop);

                // Paint the monitor sub-render before borrowing self.engine immutably.
                if let (Some(mon), Some(gpu)) = (self.monitor.as_mut(), self.gpu.as_ref()) {
                    mon.paint(&gpu.device, &gpu.queue, dt);
                }

                let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.renderer.as_mut())
                else {
                    return;
                };

                // Acquire the surface frame.  wgpu 29 returns CurrentSurfaceTexture (not Result).
                let frame = match gpu.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f) => f,
                    wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                        // Still renderable; reconfigure next frame for best quality.
                        f
                    }
                    wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                        // Reconfigure and skip this frame.
                        let (w, h) = (gpu.config.width, gpu.config.height);
                        gpu.reconfigure(w, h);
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                        return;
                    }
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded
                    | wgpu::CurrentSurfaceTexture::Validation => {
                        // Skip the frame; try again next vsync.
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                        return;
                    }
                };

                let surface_view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Capture the monitor texture view before the immutable engine borrows.
                let mon_view = self.monitor.as_ref().map(|m| &m.view);

                // Render the scene into the intermediate Rgba8Unorm storage texture.
                let scene = self.engine.scene();
                let read_value = |k: &str| self.engine.state(k);
                renderer.draw(
                    scene,
                    read_value,
                    |id| if id == "display" { mon_view } else { None },
                    &RenderTarget {
                        device: &gpu.device,
                        queue: &gpu.queue,
                        view: &gpu.intermediate.1,
                        width: gpu.config.width,
                        height: gpu.config.height,
                        base_color: carapace::scene::Color {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 0,
                        },
                    },
                );

                // Blit the intermediate texture onto the surface frame using a render pass
                // (handles format mismatch between Rgba8Unorm and the surface format).
                let mut encoder =
                    gpu.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("blit-encoder"),
                        });
                gpu.blitter.copy(
                    &gpu.device,
                    &mut encoder,
                    &gpu.intermediate.1,
                    &surface_view,
                );
                gpu.queue.submit(Some(encoder.finish()));

                frame.present();

                // Drive continuous animation at vsync.
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
            }

            WindowEvent::MouseInput { state, button, .. }
                if state == ElementState::Pressed && button == MouseButton::Left =>
            {
                if let Some(window) = self.window.as_ref() {
                    let phys = window.inner_size();
                    let (pw, ph) = (phys.width.max(1) as f64, phys.height.max(1) as f64);
                    let (cw, ch) = self.engine.scene().canvas;
                    // Map physical cursor -> canvas coords via the same ratio used for the blit.
                    let cx = (self.cursor.0 * cw as f64 / pw) as f32;
                    let cy = (self.cursor.1 * ch as f64 / ph) as f32;
                    self.engine
                        .handle_pointer(Pt { x: cx, y: cy }, PointerEvent::Press);
                }
            }

            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::Tab) => {
                        let list = if self.sysmon {
                            SYSMON_SKINS
                        } else {
                            MEDIA_SKINS
                        };
                        self.skin_index = (self.skin_index + 1) % list.len();
                        let (src, _) = load_source_from(list, self.skin_index);
                        self.engine.handle_command(Command::Swap(src));
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    Key::Character(c) if c == "h" || c == "H" => {
                        self.sysmon = !self.sysmon;
                        self.skin_index = 0;
                        let list = if self.sysmon {
                            SYSMON_SKINS
                        } else {
                            MEDIA_SKINS
                        };
                        let (src, _) = load_source_from(list, 0);
                        let host: Box<dyn carapace::host::Host> = if self.sysmon {
                            Box::new(SysmonHost::with_outbox(self.window_outbox.clone()))
                        } else {
                            Box::new(DemoHost::with_outbox(self.window_outbox.clone()))
                        };
                        self.engine
                            .handle_command(Command::SwitchHost { host, skin: src });
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}

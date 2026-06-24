use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[allow(dead_code)]
// AudioBackend/AudioError consumed by music_player_host (Task 4) + main wiring (Task 7).
mod audio;
mod file_browser_host;
#[allow(dead_code)] // MusicPlayerHost wired as the media host in Task 7.
mod music_player_host;

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
    fill{ path = rect{x=0,y=0,w=210,h=150}, color = {r=12,g=16,b=22} }\n\
    gauge{ x = 18,  y = 20, value = 'cpu',  label = 'CPU' }\n\
    gauge{ x = 85,  y = 20, value = 'mem',  label = 'MEM' }\n\
    gauge{ x = 152, y = 20, value = 'swap', label = 'SWP' }\n";

/// The monitor texture matches the reference skin's `view{ id="display" }` rect so the gauges
/// composite 1:1 with no stretch.
const MONITOR_SIZE: (u32, u32) = (210, 150);

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
            carapace::command::SkinSource::inline(MONITOR_SKIN, MONITOR_SIZE),
        )
        .unwrap();
        let (_tex, view) = Self::make_tex(device, MONITOR_SIZE.0, MONITOR_SIZE.1);
        Self {
            engine,
            renderer: Renderer::new(device),
            _tex,
            view,
            size: MONITOR_SIZE,
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

/// Inline skin for the nested file-browser shown inside the frame skin's `view{ id="app" }`.
/// Design size matches the view's design rect (456×272). Two live lists + a path line.
const APP_SHELL: &str = "\
    fill{ path = rect{x=0,y=0,w=456,h=272}, color = {r=18,g=20,b=26} }\n\
    fill{ path = rect{x=0,y=0,w=456,h=24}, color = {r=40,g=46,b=60}, anchor={'left','right','top'} }\n\
    text{ value='current_path', size=13, x=8, y=4, color={r=170,g=185,b=210}, anchor={'left','right','top'} }\n\
    fill{ path = rect{x=0,y=24,w=120,h=248}, color = {r=24,g=28,b=38}, anchor={'left','top','bottom'} }\n\
    list{ collection='shortcuts', x=8, y=32, w=104, h=232, row_height=20, on_select='open_shortcut',\n\
          anchor={'left','top','bottom'},\n\
          template={ { bind='label', x=4, y=2, size=13, color={r=150,g=200,b=170} } } }\n\
    list{ collection='entries', x=128, y=32, w=320, h=232, row_height=20, on_select='open_entry',\n\
          anchor={'left','right','top','bottom'},\n\
          template={ { bind='name', x=4, y=2, size=13, color={r=200,g=210,b=225} },\n\
                     { bind='size', right=8, y=2, size=13, halign='right', color={r=150,g=160,b=175} } } }\n";

/// Design size of the app-shell skin (matches the `view{ id="app" }` design rect).
const APP_SHELL_SIZE: (u32, u32) = (456, 272);

/// A self-contained sub-renderer that paints the file-browser app-shell into an off-screen
/// texture. The texture is resized each frame to the resolved physical size of the `"app"` view,
/// and the shell engine is reflowed to the logical size so DPI scaling is correct.
struct AppShell {
    engine: Engine,
    renderer: Renderer,
    /// Held for ownership — the TextureView borrows this texture's memory.
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
    /// Current physical size of the texture.
    phys_size: (u32, u32),
}

impl AppShell {
    fn new(device: &wgpu::Device, _outbox: WindowOutbox) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let shortcuts = vec![
            ("Repo".to_string(), root.clone()),
            ("Crates".to_string(), root.join("crates")),
            ("Docs".to_string(), root.join("docs")),
        ];
        let engine = Engine::new(
            Box::new(file_browser_host::FileBrowserHost::new(
                file_browser_host::StdFs,
                root,
                shortcuts,
            )),
            VocabRegistry::base(),
            SkinSource::inline(APP_SHELL, APP_SHELL_SIZE),
        )
        .unwrap();
        let phys = APP_SHELL_SIZE;
        let (_tex, view) = Self::make_tex(device, phys.0, phys.1);
        Self {
            engine,
            renderer: Renderer::new(device),
            _tex,
            view,
            phys_size: phys,
        }
    }

    /// Drain queued navigation host-actions (the shell engine is not ticked elsewhere).
    fn tick(&mut self, dt: std::time::Duration) {
        self.engine.update(dt);
    }

    /// Forward a click already translated into the shell's local coords.
    fn handle_click(&mut self, inner: Pt, view_w: f32, view_h: f32) {
        self.engine
            .handle_pointer_resolved(view_w, view_h, inner, PointerEvent::Press);
    }

    fn make_tex(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("app-shell"),
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

    /// Reflow the shell engine to `(logical_w, logical_h)`, resize the texture to
    /// `(phys_w, phys_h)` if needed, then render into the texture.
    fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        logical_w: f32,
        logical_h: f32,
        phys_w: u32,
        phys_h: u32,
    ) {
        // Recreate texture if physical size changed.
        if self.phys_size != (phys_w, phys_h) {
            let (tex, view) = Self::make_tex(device, phys_w, phys_h);
            self._tex = tex;
            self.view = view;
            self.phys_size = (phys_w, phys_h);
        }

        // Reflow the shell to the logical view size.
        let shell_scene = self.engine.layout(logical_w, logical_h);
        self.renderer.draw(
            &shell_scene,
            |k| self.engine.state(k),
            |_| None,
            &RenderTarget {
                device,
                queue,
                view: &self.view,
                width: phys_w,
                height: phys_h,
                base_color: carapace::scene::Color {
                    r: 18,
                    g: 20,
                    b: 26,
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
    "skins/frame",
];
const SYSMON_SKINS: &[&str] = &["skins/sysmon"];
const INIT_SCALE: u32 = 3;

fn skin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Map an outer-logical point into a view region's local coords, or None if outside.
/// The nested shell reflows to the view's logical size, so the mapping is a pure translate.
fn view_local(p: Pt, dest: &carapace::scene::ImageDest) -> Option<Pt> {
    if p.x < dest.x || p.x > dest.x + dest.w || p.y < dest.y || p.y > dest.y + dest.h {
        return None;
    }
    Some(Pt {
        x: p.x - dest.x,
        y: p.y - dest.y,
    })
}

fn demo_registry() -> VocabRegistry {
    let mut r = VocabRegistry::base();
    r.register(Box::new(carapace_demo::transport::TransportPrim));
    r.register(Box::new(carapace_demo::gauge::GaugePrim));
    r
}

/// Archetype metadata extracted from the skin manifest for the demo window.
struct SkinMeta {
    resizable: bool,
    min_size: Option<(u32, u32)>,
    max_size: Option<(u32, u32)>,
}

fn load_source_from(list: &[&str], i: usize) -> (SkinSource, (u32, u32), SkinMeta) {
    let (m, src) = carapace::skin::load_dir(&skin_root().join(list[i])).expect("load skin");
    let canvas = src.canvas;
    (
        src,
        canvas,
        SkinMeta {
            resizable: m.resizable,
            min_size: m.min_size,
            max_size: m.max_size,
        },
    )
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
    meta: SkinMeta,
    cursor: (f64, f64),
    last: Instant,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    renderer: Option<Renderer>,
    monitor: Option<Monitor>,
    app_shell: Option<AppShell>,
    window_outbox: WindowOutbox,
}

impl App {
    fn new() -> Self {
        let window_outbox: WindowOutbox = Default::default();
        let (src, _canvas, meta) = load_source_from(MEDIA_SKINS, 0);
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
            meta,
            cursor: (0.0, 0.0),
            last: Instant::now(),
            window: None,
            gpu: None,
            renderer: None,
            monitor: None,
            app_shell: None,
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

        // Size window from the skin manifest archetype.
        // Frame skin: open at design (canvas) logical size and allow resize.
        // Gadget skin: open at canvas * INIT_SCALE, non-resizable (unchanged behaviour).
        let (cw, ch) = self.engine.scene().canvas;
        let mut attrs = Window::default_attributes()
            .with_decorations(false)
            .with_transparent(true);
        if self.meta.resizable {
            attrs = attrs
                .with_resizable(true)
                .with_inner_size(winit::dpi::LogicalSize::new(cw, ch));
            if let Some((mw, mh)) = self.meta.min_size {
                attrs = attrs.with_min_inner_size(winit::dpi::LogicalSize::new(mw, mh));
            }
            if let Some((mw, mh)) = self.meta.max_size {
                attrs = attrs.with_max_inner_size(winit::dpi::LogicalSize::new(mw, mh));
            }
        } else {
            attrs = attrs
                .with_resizable(false)
                .with_inner_size(winit::dpi::LogicalSize::new(
                    cw * INIT_SCALE,
                    ch * INIT_SCALE,
                ));
        }
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
        let app_shell = AppShell::new(&gpu.device, self.window_outbox.clone());

        self.window = Some(window.clone());
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.monitor = Some(monitor);
        self.app_shell = Some(app_shell);

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.reconfigure(size.width, size.height);
                }
                // Request a redraw so the next frame re-evaluates engine.layout at the new size.
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now - self.last;
                self.last = now;

                self.engine.update(dt);
                if let Some(shell) = self.app_shell.as_mut() {
                    shell.tick(dt);
                }
                self.apply_window_ops(event_loop);

                // Paint the monitor sub-render before borrowing self.engine immutably.
                if let (Some(mon), Some(gpu)) = (self.monitor.as_mut(), self.gpu.as_ref()) {
                    mon.paint(&gpu.device, &gpu.queue, dt);
                }

                // For a frame skin, resolve the outer scene and paint the app-shell sub-renderer
                // into its texture BEFORE the outer mutable borrows of gpu/renderer.
                let scale_factor = self
                    .window
                    .as_ref()
                    .map(|w| w.scale_factor() as f32)
                    .unwrap_or(1.0);

                if self.meta.resizable
                    && let (Some(shell), Some(gpu)) = (self.app_shell.as_mut(), self.gpu.as_ref())
                {
                    let phys_w = gpu.config.width;
                    let phys_h = gpu.config.height;
                    let logical_w = phys_w as f32 / scale_factor;
                    let logical_h = phys_h as f32 / scale_factor;

                    // Find the resolved "app" view rect in logical coords.
                    let outer_scene = self.engine.layout(logical_w, logical_h);
                    if let Some((_id, dest)) =
                        outer_scene.views().into_iter().find(|(id, _)| id == "app")
                    {
                        // Physical size of the view slot = logical dest * scale_factor.
                        let view_phys_w = ((dest.w * scale_factor).round() as u32).max(1);
                        let view_phys_h = ((dest.h * scale_factor).round() as u32).max(1);
                        shell.paint(
                            &gpu.device,
                            &gpu.queue,
                            dest.w,
                            dest.h,
                            view_phys_w,
                            view_phys_h,
                        );
                    }
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

                // Capture sub-renderer texture views before the immutable engine borrows.
                let mon_view = self.monitor.as_ref().map(|m| &m.view);
                let shell_view = self.app_shell.as_ref().map(|s| &s.view);

                // For a frame skin, resolve the scene at the window's LOGICAL size so that
                // Renderer::draw scales by physical/logical = DPI (retina-correct).
                // For a gadget skin, pass the design scene unchanged; render scales by
                // physical/canvas (pixel-identical to before).
                //
                // Pointer mapping intentionally keeps the existing physical→design-canvas
                // mapping for BOTH archetypes: engine.handle_pointer hit-tests engine.scene()
                // (the design scene), so cursor coords must be in design space. Frame-skin
                // hotspots (e.g. top-anchored title-bar buttons) coincide at the top edge;
                // full anchored-hotspot hit-testing is deferred to a later spec.
                let logical = (
                    gpu.config.width as f32 / scale_factor,
                    gpu.config.height as f32 / scale_factor,
                );
                // Both archetypes resolve through layout(): frame skins to the logical window size,
                // gadget skins to their own canvas (identity for list/scrub-free skins, but enabling
                // list expansion + scrub/row hit geometry). Renderer still scales physical/canvas.
                let resolved = if self.meta.resizable {
                    self.engine.layout(logical.0, logical.1)
                } else {
                    let (cw, ch) = self.engine.scene().canvas;
                    self.engine.layout(cw as f32, ch as f32)
                };
                let scene = &resolved;
                let read_value = |k: &str| self.engine.state(k);
                renderer.draw(
                    scene,
                    read_value,
                    |id| {
                        if id == "app" {
                            shell_view
                        } else if id == "display" {
                            mon_view
                        } else {
                            None
                        }
                    },
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
                    if self.meta.resizable {
                        let sf = window.scale_factor() as f32;
                        let logical_w = pw as f32 / sf;
                        let logical_h = ph as f32 / sf;
                        let p = Pt {
                            x: self.cursor.0 as f32 / sf,
                            y: self.cursor.1 as f32 / sf,
                        };
                        // If the click is inside the "app" view, route it into the nested shell
                        // engine; otherwise hit-test the outer (chrome) scene.
                        let app_dest = self
                            .engine
                            .layout(logical_w, logical_h)
                            .views()
                            .into_iter()
                            .find(|(id, _)| id == "app")
                            .map(|(_, d)| d);
                        let routed = match (app_dest, self.app_shell.as_mut()) {
                            (Some(dest), Some(shell)) => match view_local(p, &dest) {
                                Some(inner) => {
                                    shell.handle_click(inner, dest.w, dest.h);
                                    true
                                }
                                None => false,
                            },
                            _ => false,
                        };
                        if !routed {
                            self.engine.handle_pointer_resolved(
                                logical_w,
                                logical_h,
                                p,
                                PointerEvent::Press,
                            );
                        }
                    } else {
                        // Gadget skin: map physical cursor to canvas coords, then resolve-hit so
                        // list rows + scrub bars are reachable (identity layout for plain skins).
                        let (cw, ch) = self.engine.scene().canvas;
                        let cx = (self.cursor.0 * cw as f64 / pw) as f32;
                        let cy = (self.cursor.1 * ch as f64 / ph) as f32;
                        self.engine.handle_pointer_resolved(
                            cw as f32,
                            ch as f32,
                            Pt { x: cx, y: cy },
                            PointerEvent::Press,
                        );
                    }
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
                        let (src, canvas, meta) = load_source_from(list, self.skin_index);
                        self.meta = meta;
                        self.engine.handle_command(Command::Swap(src));
                        apply_window_archetype(&self.window, &self.meta, canvas);
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
                        let (src, canvas, meta) = load_source_from(list, 0);
                        self.meta = meta;
                        let host: Box<dyn carapace::host::Host> = if self.sysmon {
                            Box::new(SysmonHost::with_outbox(self.window_outbox.clone()))
                        } else {
                            Box::new(DemoHost::with_outbox(self.window_outbox.clone()))
                        };
                        self.engine
                            .handle_command(Command::SwitchHost { host, skin: src });
                        apply_window_archetype(&self.window, &self.meta, canvas);
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

/// Update the live window's resizability and size to match a newly-loaded skin's archetype.
///
/// winit 0.30 supports live property changes on an existing window, so no surface/device
/// recreation is needed. The resulting `Resized` event drives `gpu.reconfigure`.
fn apply_window_archetype(window: &Option<Arc<Window>>, meta: &SkinMeta, canvas: (u32, u32)) {
    if let Some(w) = window {
        w.set_resizable(meta.resizable);
        if meta.resizable {
            let (cw, ch) = canvas;
            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(cw, ch));
            if let Some((mw, mh)) = meta.min_size {
                w.set_min_inner_size(Some(winit::dpi::LogicalSize::new(mw, mh)));
            }
            if let Some((mw, mh)) = meta.max_size {
                w.set_max_inner_size(Some(winit::dpi::LogicalSize::new(mw, mh)));
            } else {
                w.set_max_inner_size::<winit::dpi::LogicalSize<u32>>(None);
            }
        } else {
            w.set_min_inner_size::<winit::dpi::LogicalSize<u32>>(None);
            w.set_max_inner_size::<winit::dpi::LogicalSize<u32>>(None);
            let (cw, ch) = canvas;
            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(
                cw * INIT_SCALE,
                ch * INIT_SCALE,
            ));
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}

#[cfg(test)]
mod tests {
    use super::view_local;
    use carapace::scene::{ImageDest, Pt};

    #[test]
    fn view_local_translates_inside_and_rejects_outside() {
        let d = ImageDest {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 50.0,
        };
        assert_eq!(
            view_local(Pt { x: 10.0, y: 20.0 }, &d),
            Some(Pt { x: 0.0, y: 0.0 })
        );
        assert_eq!(
            view_local(Pt { x: 60.0, y: 45.0 }, &d),
            Some(Pt { x: 50.0, y: 25.0 })
        );
        assert_eq!(
            view_local(Pt { x: 5.0, y: 45.0 }, &d),
            None,
            "left of region"
        );
        assert_eq!(
            view_local(Pt { x: 60.0, y: 80.0 }, &d),
            None,
            "below region"
        );
    }
}

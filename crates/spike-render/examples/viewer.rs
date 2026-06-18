//! Live viewer to visually compare rendering backends and feel the hit-testing.
//! Usage: cargo run -p spike-render --example viewer  (vello is the chosen backend)
//! Left-click: report INSIDE/OUTSIDE + recolor (green=inside, red=outside).
//! Space: toggle L-shape / ring. Esc or close: quit.
//!
//! The window is a presenter only: the backend renders its real output into a
//! CANVAS x CANVAS RGBA8 Pixmap, which is blitted (DPI-robust nearest-neighbor
//! scale to the window's physical size) via softbuffer, so what you see is
//! faithful to the backend's own rendering.

use std::num::NonZeroU32;
use std::rc::Rc;

use hittest::{Point, Region, l_shape, ring};
use spike_render::Renderer;
use spike_render::vello_backend::VelloRenderer;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const CANVAS: u32 = 200; // region coordinate space
const INIT_SCALE: u32 = 3; // initial logical window size = CANVAS * INIT_SCALE
const BG: [u8; 4] = [0, 0, 0, 255];
const FILL_DEFAULT: [u8; 4] = [150, 150, 150, 255]; // neutral gray until first click
const FILL_INSIDE: [u8; 4] = [0, 200, 0, 255]; // green: last click landed inside
const FILL_OUTSIDE: [u8; 4] = [220, 30, 30, 255]; // red: last click missed

fn make_renderer(name: &str) -> Box<dyn Renderer> {
    match name {
        // Phase 0 chose vello; the other candidates were pruned after the decision.
        "vello" => Box::new(VelloRenderer::new()),
        other => {
            eprintln!("unknown backend '{other}'; only 'vello' is available");
            std::process::exit(2);
        }
    }
}

struct App {
    backend_name: String,
    renderer: Box<dyn Renderer>,
    region: Region,
    use_ring: bool,
    fill: [u8; 4],
    cursor: (f64, f64), // physical pixels, as delivered by winit
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
}

impl App {
    fn new(backend_name: String) -> Self {
        Self {
            renderer: make_renderer(&backend_name),
            backend_name,
            region: l_shape(),
            use_ring: false,
            fill: FILL_DEFAULT,
            cursor: (0.0, 0.0),
            window: None,
            surface: None,
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (self.window.as_ref(), self.surface.as_mut()) else {
            return;
        };
        // Physical size handles HiDPI: on a 2x display this is 2x the logical size.
        let size = window.inner_size();
        let (pw, ph) = (size.width.max(1), size.height.max(1));
        surface
            .resize(NonZeroU32::new(pw).unwrap(), NonZeroU32::new(ph).unwrap())
            .unwrap();

        // Each backend renders its real output into a CANVASxCANVAS RGBA8 Pixmap.
        let pm = self
            .renderer
            .render(&self.region, (CANVAS, CANVAS), self.fill, BG);

        let mut buf = surface.buffer_mut().unwrap();
        // Nearest-neighbor scale CANVAS -> physical window, RGBA8 -> softbuffer 0RGB u32.
        for py in 0..ph {
            let sy = py * CANVAS / ph;
            for px in 0..pw {
                let sx = px * CANVAS / pw;
                let i = ((sy * CANVAS + sx) * 4) as usize;
                let (r, g, b) = (
                    pm.data[i] as u32,
                    pm.data[i + 1] as u32,
                    pm.data[i + 2] as u32,
                );
                buf[(py * pw + px) as usize] = (r << 16) | (g << 8) | b;
            }
        }
        buf.present().unwrap();
    }

    fn on_click(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        let (pw, ph) = (size.width.max(1) as f64, size.height.max(1) as f64);
        // Cursor is physical pixels; map back to region space via the physical extent.
        let rx = (self.cursor.0 * CANVAS as f64 / pw) as f32;
        let ry = (self.cursor.1 * CANVAS as f64 / ph) as f32;
        let inside = self.region.contains(Point { x: rx, y: ry });
        self.fill = if inside { FILL_INSIDE } else { FILL_OUTSIDE };
        println!(
            "[{}] click ({rx:.1}, {ry:.1}) -> {}",
            self.backend_name,
            if inside { "INSIDE" } else { "OUTSIDE" }
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Guard: winit can call resumed again on foreground transitions. Build once.
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("skin-engine backend viewer")
            .with_inner_size(winit::dpi::LogicalSize::new(
                CANVAS * INIT_SCALE,
                CANVAS * INIT_SCALE,
            ));
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. }
                if state == ElementState::Pressed && button == MouseButton::Left =>
            {
                self.on_click();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::Space) => {
                        self.use_ring = !self.use_ring;
                        self.region = if self.use_ring { ring() } else { l_shape() };
                        self.fill = FILL_DEFAULT;
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
    // vello is the only remaining backend; default to it when no arg is given.
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "vello".to_string());
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(name);
    event_loop.run_app(&mut app).unwrap();
}

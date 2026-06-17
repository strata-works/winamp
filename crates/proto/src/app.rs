use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::host::{Host, MediaHost, SysmonHost};
use crate::render::Renderer;
use crate::scene::Pt;
use crate::skin::load_dir;
use crate::swap::Engine;

// (host_index, skin_index) -> the two hosts, two skins each.
const SKIN_DIRS: [[&str; 2]; 2] = [
    ["skins/media-classic", "skins/media-minimal"],
    ["skins/sysmon-bars", "skins/sysmon-dial"],
];

const INIT_SCALE: u32 = 3;

fn skin_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn make_host(host_index: usize) -> Box<dyn Host> {
    match host_index {
        0 => Box::new(MediaHost::new()),
        _ => Box::new(SysmonHost::new()),
    }
}

fn load_engine(host_index: usize, skin_index: usize) -> Engine {
    let files = load_dir(&skin_root().join(SKIN_DIRS[host_index][skin_index]))
        .expect("load skin dir");
    Engine::new(
        make_host(host_index),
        &files.lua_src,
        (files.manifest.width, files.manifest.height),
    )
    .expect("build engine")
}

struct App {
    host_index: usize,
    skin_index: usize,
    engine: Engine,
    renderer: Renderer,
    cursor: (f64, f64),
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    // Throwaway instrumentation: count redraws and report fps once per second.
    frames: u32,
    last_report: Instant,
}

impl App {
    fn new() -> Self {
        let host_index = 0;
        let skin_index = 0;
        let engine = load_engine(host_index, skin_index);
        let renderer = Renderer::new();
        Self {
            host_index,
            skin_index,
            engine,
            renderer,
            cursor: (0.0, 0.0),
            window: None,
            surface: None,
            frames: 0,
            last_report: Instant::now(),
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (self.window.as_ref(), self.surface.as_mut()) else {
            return;
        };

        self.engine.tick(1.0 / 60.0);

        let pm = self.engine.render_with(&mut self.renderer);

        let size = window.inner_size();
        let (pw, ph) = (size.width.max(1), size.height.max(1));
        surface
            .resize(NonZeroU32::new(pw).unwrap(), NonZeroU32::new(ph).unwrap())
            .unwrap();

        let (cw, ch) = (pm.width, pm.height);
        let mut buf = surface.buffer_mut().unwrap();
        // Nearest-neighbor scale canvas -> physical window, RGBA8 -> softbuffer 0RGB u32.
        for py in 0..ph {
            let sy = py * ch / ph;
            for px in 0..pw {
                let sx = px * cw / pw;
                let i = ((sy * cw + sx) * 4) as usize;
                let (r, g, b) = (
                    pm.data[i] as u32,
                    pm.data[i + 1] as u32,
                    pm.data[i + 2] as u32,
                );
                buf[(py * pw + px) as usize] = (r << 16) | (g << 8) | b;
            }
        }
        buf.present().unwrap();

        // fps report once per second.
        self.frames += 1;
        let elapsed = self.last_report.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            println!("fps: {:.0}", self.frames as f32 / elapsed.as_secs_f32());
            self.frames = 0;
            self.last_report = Instant::now();
        }

        // Keep animating.
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn on_click(&mut self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        let (pw, ph) = (size.width.max(1) as f64, size.height.max(1) as f64);
        let (cw, ch) = self.engine.scene().canvas;
        // Map physical cursor -> canvas coords via the same ratio used for the blit.
        let cx = (self.cursor.0 * cw as f64 / pw) as f32;
        let cy = (self.cursor.1 * ch as f64 / ph) as f32;
        let _ = self.engine.click(Pt { x: cx, y: cy });
    }

    fn swap_skin(&mut self) {
        self.skin_index ^= 1;
        let files =
            load_dir(&skin_root().join(SKIN_DIRS[self.host_index][self.skin_index]))
                .expect("load skin dir");
        self.engine
            .swap(&files.lua_src, (files.manifest.width, files.manifest.height))
            .expect("swap engine");
        println!(
            "[{}] swapped to skin {}, state preserved",
            files.manifest.id, self.skin_index
        );
    }

    fn switch_host(&mut self) {
        self.host_index ^= 1;
        self.skin_index = 0;
        self.engine = load_engine(self.host_index, 0);
        println!(
            "switched to host_index={}, skin_index=0",
            self.host_index
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Guard: winit can call resumed again on foreground transitions. Build once.
        if self.window.is_some() {
            return;
        }
        let (cw, ch) = self.engine.scene().canvas;
        let attrs = Window::default_attributes()
            .with_title("carapace skin engine")
            .with_inner_size(winit::dpi::LogicalSize::new(
                cw * INIT_SCALE,
                ch * INIT_SCALE,
            ));
        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();
        self.window = Some(window.clone());
        self.surface = Some(surface);
        window.request_redraw();
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
                    Key::Named(NamedKey::Tab) => {
                        self.swap_skin();
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                    Key::Character(ref c) if c.as_str().eq_ignore_ascii_case("h") => {
                        self.switch_host();
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

pub fn run() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}

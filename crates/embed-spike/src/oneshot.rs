use std::ffi::{c_char, CStr};
use std::path::PathBuf;
use std::time::Duration;

use carapace::engine::Engine;
use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::render::Renderer;
use carapace::state::StateValue;

use crate::render::{init_gpu, new_offscreen, readback_rgba, render_frame};

/// Minimal stateless host for one-shot renders: reports a single scalar under key "level".
/// Advertises `toggle` only so the spike skin's `region{ on_press = host.toggle }` resolves at
/// load; it is never invoked (one-shot render forwards no input).
pub struct OneShotHost {
    pub level: f32,
}

const ACTIONS: &[ActionSpec] = &[ActionSpec { name: "toggle" }];

impl Host for OneShotHost {
    fn name(&self) -> &str {
        "oneshot"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        if key == "level" {
            Some(StateValue::Scalar(self.level))
        } else {
            None
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, _action: &str, _args: &[Value]) {}
    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new()
    }
}

/// One-shot headless render of `skin_dir` at the given `state` (drives host key "level") into a
/// `w`×`h` PNG written to `out_path`. Stateless; no IOSurface; CPU-readback path. Returns true on
/// success, false on any failure (never panics across the FFI boundary).
///
/// # Safety
/// `skin_dir` and `out_path` must be valid NUL-terminated UTF-8 paths.
#[no_mangle]
pub unsafe extern "C" fn carapace_render_png(
    skin_dir: *const c_char,
    w: u32,
    h: u32,
    state: f64,
    out_path: *const c_char,
) -> bool {
    if skin_dir.is_null() || out_path.is_null() || w == 0 || h == 0 {
        return false;
    }
    let render = || -> Option<()> {
        let dir = PathBuf::from(unsafe { CStr::from_ptr(skin_dir) }.to_str().ok()?);
        let out = unsafe { CStr::from_ptr(out_path) }.to_str().ok()?.to_string();

        let (_manifest, source) = carapace::skin::load_dir(&dir).ok()?;
        let mut engine = Engine::new(
            Box::new(OneShotHost {
                level: state as f32,
            }),
            carapace::vocab::VocabRegistry::base(),
            source,
        )
        .ok()?;

        let gpu = init_gpu();
        let mut renderer = Renderer::new(&gpu.device);
        let off = new_offscreen(&gpu.device, w, h);
        // wait=true so readback sees completed GPU work; no host view{} content for this skin.
        render_frame(
            &mut engine,
            &mut renderer,
            &gpu,
            &off.view,
            w,
            h,
            Duration::ZERO,
            true,
            None,
        );
        let rgba = readback_rgba(&gpu, &off.tex, w, h);
        image::save_buffer(&out, &rgba, w, h, image::ColorType::Rgba8).ok()?;
        Some(())
    };
    // Convert any None (and isolate against unwind) into a clean `false`.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(render))
        .ok()
        .flatten()
        .is_some()
}

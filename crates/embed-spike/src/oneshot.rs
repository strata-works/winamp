use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use carapace::engine::Engine;
use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::render::Renderer;
use carapace::state::StateValue;

use crate::render::{init_gpu, new_offscreen, readback_rgba, render_frame};

const ACTIONS: &[ActionSpec] = &[ActionSpec { name: "toggle" }];

/// Minimal stateless host for one-shot renders: reports a single scalar for any key.
pub struct OneShotHost {
    pub level: f32,
}

impl Host for OneShotHost {
    fn name(&self) -> &str {
        "oneshot"
    }
    fn tick(&mut self, _dt: Duration) {}
    // Serve the state scalar for ANY numeric key a skin binds (level, position, cpu, …) so a
    // single one-shot host can render arbitrary showcase skins, not just ones bound to "level".
    fn get(&self, _key: &str) -> Option<StateValue> {
        Some(StateValue::Scalar(self.level))
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, _action: &str, _args: &[Value]) {}
    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new()
    }
}

/// Stateless host that serves a key→value map for one-shot LIVE-INFO renders. A value that parses
/// as a number is served as `Scalar` (drives `value_fill`/gauges); otherwise it is served as `Str`
/// (drives `text{ value = "key" }`). This is what lets a widget render real data through a skin.
pub struct InfoHost {
    pub values: HashMap<String, String>,
}

impl Host for InfoHost {
    fn name(&self) -> &str {
        "info"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        let v = self.values.get(key)?;
        Some(match v.parse::<f32>() {
            Ok(n) => StateValue::Scalar(n),
            Err(_) => StateValue::Str(v.as_str().into()),
        })
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, _action: &str, _args: &[Value]) {}
    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new()
    }
}

/// Shared one-shot render: load `dir`, build an engine over `host`, render `w`×`h`, write a PNG to
/// `out`. Stateless; no IOSurface; CPU-readback path. Returns `None` on any failure.
pub fn render_skin_with_host(
    dir: &Path,
    host: Box<dyn Host>,
    w: u32,
    h: u32,
    out: &str,
) -> Option<()> {
    let (_manifest, source) = carapace::skin::load_dir(dir).ok()?;
    let mut engine = Engine::new(host, carapace::vocab::VocabRegistry::base(), source).ok()?;

    let gpu = init_gpu();
    let mut renderer = Renderer::new(&gpu.device);
    let off = new_offscreen(&gpu.device, w, h);
    // wait=true so readback sees completed GPU work; no host view{} content for these skins.
    render_frame(
        &mut engine,
        &mut renderer,
        &gpu,
        &off.view,
        w,
        h,
        Duration::ZERO,
        true,
        &[],
    );
    let rgba = readback_rgba(&gpu, &off.tex, w, h);
    image::save_buffer(out, &rgba, w, h, image::ColorType::Rgba8).ok()?;
    Some(())
}

/// One-shot headless render of `skin_dir` at the given `state` (drives any numeric host key) into a
/// `w`×`h` PNG written to `out_path`. Never panics across the FFI boundary.
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
        let out = unsafe { CStr::from_ptr(out_path) }
            .to_str()
            .ok()?
            .to_string();
        render_skin_with_host(
            &dir,
            Box::new(OneShotHost {
                level: state as f32,
            }),
            w,
            h,
            &out,
        )
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(render))
        .ok()
        .flatten()
        .is_some()
}

/// One-shot LIVE-INFO render: `n` parallel `keys`/`vals` C-string pairs become the host data a skin
/// binds via `text{ value = "key" }` (strings) and `value_fill{ value = "key" }` (numbers). This is
/// how a widget renders real information (track, artist, time, position, …) through a carapace skin.
/// Never panics across the FFI boundary.
///
/// # Safety
/// `skin_dir`/`out_path` and each of the `n` entries in `keys`/`vals` must be valid NUL-terminated
/// UTF-8 strings; `keys` and `vals` must each point to `n` valid pointers.
#[no_mangle]
pub unsafe extern "C" fn carapace_render_info(
    skin_dir: *const c_char,
    w: u32,
    h: u32,
    n: u32,
    keys: *const *const c_char,
    vals: *const *const c_char,
    out_path: *const c_char,
) -> bool {
    if skin_dir.is_null() || out_path.is_null() || w == 0 || h == 0 {
        return false;
    }
    if n > 0 && (keys.is_null() || vals.is_null()) {
        return false;
    }
    let render = || -> Option<()> {
        let dir = PathBuf::from(unsafe { CStr::from_ptr(skin_dir) }.to_str().ok()?);
        let out = unsafe { CStr::from_ptr(out_path) }
            .to_str()
            .ok()?
            .to_string();
        let mut values = HashMap::new();
        for i in 0..n as isize {
            let k = unsafe { CStr::from_ptr(*keys.offset(i)) }.to_str().ok()?;
            let v = unsafe { CStr::from_ptr(*vals.offset(i)) }.to_str().ok()?;
            values.insert(k.to_string(), v.to_string());
        }
        render_skin_with_host(&dir, Box::new(InfoHost { values }), w, h, &out)
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(render))
        .ok()
        .flatten()
        .is_some()
}

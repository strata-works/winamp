//! `SkinSession`: skin load + reload with captured (non-fatal) Lua errors.
//! Consumed by the engine-thread task added later — kept ungated so it's
//! unit-tested now (same precedent as `preview_host.rs`).

use crate::preview_host::{ActionLog, PreviewHost, Values, scan_actions};
use carapace::engine::Engine;
use carapace::host::{ActionSpec, Host};
use carapace::vocab::VocabRegistry;
use std::path::{Path, PathBuf};

pub struct SkinSession {
    pub dir: PathBuf,
    pub engine: Option<Engine>,
    pub name: String,
    pub canvas: (u32, u32),
    pub last_error: Option<String>,
    values: Values,
    log: ActionLog,
}

/// Build an `Engine` for `dir`, scanning the source for the host-action allowlist first.
fn build_engine(
    dir: &Path,
    values: &Values,
    log: &ActionLog,
) -> Result<(Engine, String, (u32, u32)), String> {
    let (manifest, source) = carapace::skin::load_dir(dir).map_err(|e| format!("{e:?}"))?;
    let actions: Vec<ActionSpec> = scan_actions(&source.lua_src)
        .into_iter()
        .map(|name| ActionSpec { name })
        .collect();
    let host: Box<dyn Host> = Box::new(PreviewHost::new(values.clone(), log.clone(), actions));
    let engine = Engine::new(host, VocabRegistry::base(), source).map_err(|e| format!("{e:?}"))?;
    Ok((
        engine,
        manifest.name,
        (manifest.canvas.width, manifest.canvas.height),
    ))
}

impl SkinSession {
    pub fn new(dir: PathBuf, values: Values, log: ActionLog) -> SkinSession {
        let mut s = SkinSession {
            dir,
            engine: None,
            name: String::new(),
            canvas: (0, 0),
            last_error: None,
            values,
            log,
        };
        s.reload();
        s
    }

    pub fn reload(&mut self) {
        match build_engine(&self.dir, &self.values, &self.log) {
            Ok((engine, name, canvas)) => {
                self.engine = Some(engine);
                self.name = name;
                self.canvas = canvas;
                self.last_error = None;
            }
            Err(e) => {
                // Keep the last-good engine up; surface the error.
                self.last_error = Some(e);
            }
        }
    }

    /// Not called by `main.rs` today (it reads `last_error` directly) — a convenience
    /// accessor kept for callers (tests, future tooling) that want a `Result`.
    #[allow(dead_code)]
    pub fn load_result(&self) -> Result<(), String> {
        match &self.last_error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp_skin(lua: &str) -> PathBuf {
        // Unique dir under the crate target dir; no external tempfile dep.
        // `env!("CARGO_TARGET_TMPDIR")` is compile-time-only and only defined for
        // integration-test binaries (files under `tests/`), not for unit tests
        // compiled into `src/main.rs` — so read it at runtime instead, with the
        // same fallback Cargo itself suggests.
        let tmpdir = std::env::var("CARGO_TARGET_TMPDIR")
            .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned());
        let base = std::path::Path::new(&tmpdir).join(format!(
            "skin_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("skin.toml"),
            "schema = 1\nid = \"t\"\nname = \"Temp\"\nengine = \"^0.1\"\ncanvas = { width = 100, height = 60 }\nentry = \"skin.lua\"\n",
        )
        .unwrap();
        fs::write(base.join("skin.lua"), lua).unwrap();
        base
    }

    #[test]
    fn loads_a_valid_skin() {
        let dir =
            tmp_skin("fill{ path = {{x=0,y=0},{x=100,y=0},{x=100,y=60}}, color = {r=1,g=2,b=3} }");
        let s = SkinSession::new(dir, Default::default(), Default::default());
        assert!(s.engine.is_some());
        assert!(s.last_error.is_none());
        assert_eq!(s.canvas, (100, 60));
        assert_eq!(s.name, "Temp");
    }

    #[test]
    fn broken_lua_is_captured_not_fatal_and_keeps_last_good() {
        let dir =
            tmp_skin("fill{ path = {{x=0,y=0},{x=100,y=0},{x=100,y=60}}, color = {r=1,g=2,b=3} }");
        let mut s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        assert!(s.engine.is_some());
        // Overwrite with a syntax error and reload.
        std::fs::write(dir.join("skin.lua"), "fill{ this is not lua ").unwrap();
        s.reload();
        assert!(s.last_error.is_some(), "error should be captured");
        assert!(s.engine.is_some(), "last-good engine should survive");
    }

    #[test]
    fn reload_picks_up_a_valid_change() {
        let dir =
            tmp_skin("fill{ path = {{x=0,y=0},{x=100,y=0},{x=100,y=60}}, color = {r=1,g=2,b=3} }");
        let mut s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        std::fs::write(
            dir.join("skin.lua"),
            "fill{ path = {{x=0,y=0},{x=50,y=0},{x=50,y=30}}, color = {r=9,g=9,b=9} }",
        )
        .unwrap();
        s.reload();
        assert!(s.last_error.is_none());
        assert!(s.engine.is_some());
    }
}

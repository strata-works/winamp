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
    entry: String,
    pub last_error: Option<String>,
    values: Values,
    log: ActionLog,
}

/// Build an `Engine` for `dir`, scanning the source for the host-action allowlist first.
fn build_engine(
    dir: &Path,
    values: &Values,
    log: &ActionLog,
) -> Result<(Engine, String, (u32, u32), String), String> {
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
        manifest.entry,
    ))
}

impl SkinSession {
    pub fn new(dir: PathBuf, values: Values, log: ActionLog) -> SkinSession {
        let mut s = SkinSession {
            dir,
            engine: None,
            name: String::new(),
            canvas: (0, 0),
            entry: "skin.lua".to_string(),
            last_error: None,
            values,
            log,
        };
        s.reload();
        s
    }

    pub fn reload(&mut self) {
        match build_engine(&self.dir, &self.values, &self.log) {
            Ok((engine, name, canvas, entry)) => {
                self.engine = Some(engine);
                self.name = name;
                self.canvas = canvas;
                self.entry = entry;
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

    pub fn entry_path(&self) -> PathBuf {
        self.dir.join(&self.entry)
    }

    fn read_source(&self) -> String {
        std::fs::read_to_string(self.entry_path()).unwrap_or_default()
    }

    pub fn source_model(&self) -> crate::provenance::SourceModel {
        crate::provenance::parse_source(&self.read_source())
    }

    pub fn params_json(&self) -> serde_json::Value {
        use crate::provenance::LiteralValue;
        let m = self.source_model();
        let items: Vec<serde_json::Value> = m
            .params
            .iter()
            .map(|p| match &p.value {
                LiteralValue::Scalar { text, .. } => {
                    serde_json::json!({"name": p.name, "kind": "scalar", "value": text})
                }
                LiteralValue::Table { subfields } => {
                    let subs: Vec<_> = subfields
                        .iter()
                        .map(|(n, s)| serde_json::json!({"name": n, "value": s.text}))
                        .collect();
                    serde_json::json!({"name": p.name, "kind": "table", "subfields": subs})
                }
            })
            .collect();
        serde_json::Value::Array(items)
    }

    pub fn pick(
        &self,
        w: f32,
        h: f32,
        p: carapace::scene::Pt,
    ) -> Option<crate::inspector::NodeInfo> {
        let engine = self.engine.as_ref()?;
        let (scene, origins) = engine.layout_with_origins(w, h);
        let idx = scene.pick(p)?;
        crate::inspector::node_info(&origins, idx, &self.source_model())
    }

    pub fn apply_prop(&self, line: u32, field: &str, value: &str) -> Result<(), String> {
        use crate::provenance::{FieldState, LiteralValue};
        let src = self.read_source();
        let model = crate::provenance::parse_source(&src);
        let call = model
            .calls
            .iter()
            .find(|c| c.line == line)
            .ok_or_else(|| format!("no call at line {line}"))?;
        let f = call
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| format!("no field {field}"))?;
        let FieldState::Literal {
            value: LiteralValue::Scalar { span, .. },
        } = &f.state
        else {
            return Err(format!("{field} is not an editable scalar"));
        };
        let out = crate::provenance::splice(&src, *span, value);
        std::fs::write(self.entry_path(), out).map_err(|e| e.to_string())
    }

    pub fn apply_param(&self, name: &str, sub: Option<&str>, value: &str) -> Result<(), String> {
        use crate::provenance::LiteralValue;
        let src = self.read_source();
        let model = crate::provenance::parse_source(&src);
        let param = model
            .params
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| format!("no param {name}"))?;
        let span = match (&param.value, sub) {
            (LiteralValue::Scalar { span, .. }, None) => *span,
            (LiteralValue::Table { subfields }, Some(sub)) => subfields
                .iter()
                .find(|(n, _)| n == sub)
                .map(|(_, s)| s.span)
                .ok_or_else(|| format!("no subfield {sub}"))?,
            _ => return Err("param/subfield mismatch".to_string()),
        };
        let out = crate::provenance::splice(&src, span, value);
        std::fs::write(self.entry_path(), out).map_err(|e| e.to_string())
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

    #[test]
    fn source_model_reads_params_from_disk() {
        let dir = tmp_skin(
            "local RI = 90\nfill{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = {r=1,g=2,b=3} }",
        );
        let s = SkinSession::new(dir, Default::default(), Default::default());
        let m = s.source_model();
        assert!(m.params.iter().any(|p| p.name == "RI"));
    }

    #[test]
    fn apply_param_rewrites_the_file() {
        let dir = tmp_skin(
            "local RI = 90\nfill{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = {r=1,g=2,b=3} }",
        );
        let s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        s.apply_param("RI", None, "120").unwrap();
        let on_disk = std::fs::read_to_string(s.entry_path()).unwrap();
        assert!(on_disk.starts_with("local RI = 120"), "got: {on_disk}");
    }

    #[test]
    fn apply_prop_rewrites_a_call_field() {
        let dir = tmp_skin(
            "fill{ x = 10, path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = {r=1,g=2,b=3} }",
        );
        let s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        s.apply_prop(1, "x", "42").unwrap();
        let on_disk = std::fs::read_to_string(s.entry_path()).unwrap();
        assert!(on_disk.contains("x = 42"), "got: {on_disk}");
    }

    #[test]
    fn apply_param_color_subfield() {
        let dir = tmp_skin(
            "local STONE = {r=10, g=20, b=30}\nfill{ path = {{x=0,y=0},{x=1,y=0},{x=1,y=1}}, color = STONE }",
        );
        let s = SkinSession::new(dir.clone(), Default::default(), Default::default());
        s.apply_param("STONE", Some("g"), "99").unwrap();
        let on_disk = std::fs::read_to_string(s.entry_path()).unwrap();
        assert!(on_disk.contains("g=99"), "got: {on_disk}");
    }
}

//! `PreviewHost`: the `carapace::host::Host` impl driving skins in the previewer.
//! Consumed by the server/engine-thread task added later — kept ungated so it's
//! unit-tested now (same precedent as `protocol.rs`).

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

/// Host-value map the browser data panel drives; shared with the engine thread.
pub type Values = Rc<RefCell<HashMap<String, StateValue>>>;
/// Action-invocation log; drained + broadcast by the engine loop each tick.
pub type ActionLog = Rc<RefCell<Vec<String>>>;

/// Scan skin source for every `host.<ident>` call, dedupe, and leak each name to
/// `&'static str` (required because `ActionSpec.name` is `&'static str`). A handful
/// of leaked strings per reload is acceptable for a dev tool.
pub fn scan_actions(lua_src: &str) -> Vec<&'static str> {
    let mut seen: HashSet<String> = HashSet::new();
    let bytes = lua_src.as_bytes();
    let needle = b"host.";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let mut j = i + needle.len();
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + needle.len() {
                seen.insert(lua_src[i + needle.len()..j].to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    seen.into_iter()
        .map(|s| &*Box::leak(s.into_boxed_str()))
        .collect()
}

/// A `carapace::host::Host` for the previewer: values come from the browser panel,
/// invoked actions are logged (never mutate values — the panel owns those).
pub struct PreviewHost {
    values: Values,
    log: ActionLog,
    actions: Vec<ActionSpec>,
}

impl PreviewHost {
    pub fn new(values: Values, log: ActionLog, actions: Vec<ActionSpec>) -> Self {
        Self {
            values,
            log,
            actions,
        }
    }
}

impl Host for PreviewHost {
    fn name(&self) -> &str {
        "preview"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        self.values.borrow().get(key).cloned()
    }
    fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        self.log.borrow_mut().push(action.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carapace::host::{Host, Value};
    use carapace::state::StateValue;

    const MINIMAL_SRC: &str = r#"
        region{ on_press = function() host.begin_drag() end }
        region{ on_press = function() host.minimize() end }
        region{ on_press = function() host.close() end }
        region{ on_press = function() host.toggle_play() end }
        region{ on_press = function() host.toggle_play() end }  -- duplicate
    "#;

    #[test]
    fn scan_finds_deduped_action_names_including_in_closures() {
        let mut got = scan_actions(MINIMAL_SRC);
        got.sort_unstable();
        assert_eq!(got, vec!["begin_drag", "close", "minimize", "toggle_play"]);
    }

    #[test]
    fn get_reads_the_shared_value_map() {
        let values: Values = Default::default();
        values
            .borrow_mut()
            .insert("level".into(), StateValue::Scalar(0.5));
        let host = PreviewHost::new(values, Default::default(), Vec::new());
        assert_eq!(host.get("level"), Some(StateValue::Scalar(0.5)));
        assert_eq!(host.get("missing"), None);
    }

    #[test]
    fn invoke_appends_to_the_action_log() {
        let log: ActionLog = Default::default();
        let mut host = PreviewHost::new(Default::default(), log.clone(), Vec::new());
        host.invoke("toggle_play", &[Value::Num(1.0)]);
        host.invoke("close", &[]);
        assert_eq!(
            *log.borrow(),
            vec!["toggle_play".to_string(), "close".to_string()]
        );
    }

    #[test]
    fn actions_reports_scanned_allowlist() {
        let specs: Vec<carapace::host::ActionSpec> = scan_actions("host.play() host.stop()")
            .into_iter()
            .map(|name| carapace::host::ActionSpec { name })
            .collect();
        let host = PreviewHost::new(Default::default(), Default::default(), specs);
        let names: Vec<&str> = host.actions().iter().map(|a| a.name).collect();
        assert!(names.contains(&"play") && names.contains(&"stop"));
    }
}

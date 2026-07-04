use std::collections::BTreeMap;
use std::time::Duration;

use crate::state::StateValue;

/// An argument passed to [`Host::invoke`].
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A numeric argument.
    Num(f64),
    /// A boolean argument.
    Bool(bool),
    /// A string argument.
    Str(String),
}

/// One row of a host collection, as iterated by a skin's `list{}`.
/// Cells are addressed by key; `BTreeMap` keeps cell order deterministic for
/// snapshot tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The row's cells, keyed by column/field name.
    pub cells: BTreeMap<String, StateValue>,
}

impl Row {
    /// An empty row with no cells set.
    pub fn new() -> Self {
        Self::default()
    }
    /// Builder-style cell insert.
    pub fn set(mut self, key: &str, value: StateValue) -> Self {
        self.cells.insert(key.to_string(), value);
        self
    }
    /// Reads a cell by key, if present.
    pub fn get(&self, key: &str) -> Option<&StateValue> {
        self.cells.get(key)
    }
}

/// One entry in a host's action allowlist, checked before [`Host::invoke`] runs.
#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    /// The action name, as referenced by skins (e.g. via `on_press`).
    pub name: &'static str,
}

/// The capability surface an integrator implements to expose their app's data
/// and actions to the skin. Boxed as `Box<dyn Host>` and passed to `Engine::new`
/// (and `Command::SwitchHost`). The engine knows none of the concrete names —
/// everything flows through this trait.
pub trait Host {
    /// Identifies the host.
    fn name(&self) -> &str;
    /// Called once per `Engine::update`, after queued commands have been drained.
    fn tick(&mut self, dt: Duration);
    /// Reads a data value by key, backing value/text bindings and `Engine::state`.
    fn get(&self, key: &str) -> Option<StateValue>;
    /// The allowlist of actions this host accepts, checked before `invoke` is called.
    fn actions(&self) -> &[ActionSpec];
    /// Performs an allowlisted action with the given arguments.
    fn invoke(&mut self, action: &str, args: &[Value]);
    /// Host-provided collections that `list{}` iterates. Default: no collections.
    fn rows(&self, _collection: &str) -> Vec<Row> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::FixtureHost;

    #[test]
    fn rows_defaults_to_empty() {
        let h = FixtureHost::new();
        assert!(h.rows("anything").is_empty());
    }

    #[test]
    fn row_builder_sets_and_gets_cells() {
        let r = Row::new().set("name", StateValue::Str("a.txt".into()));
        assert_eq!(r.get("name"), Some(&StateValue::Str("a.txt".into())));
        assert_eq!(r.get("missing"), None);
    }
}

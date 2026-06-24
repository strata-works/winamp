use std::collections::BTreeMap;
use std::time::Duration;

use crate::state::StateValue;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
}

/// One row of a host-provided collection: cells addressed by key.
/// BTreeMap keeps cell order deterministic for snapshot tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub cells: BTreeMap<String, StateValue>,
}

impl Row {
    pub fn new() -> Self {
        Self::default()
    }
    /// Builder-style cell insert.
    pub fn set(mut self, key: &str, value: StateValue) -> Self {
        self.cells.insert(key.to_string(), value);
        self
    }
    pub fn get(&self, key: &str) -> Option<&StateValue> {
        self.cells.get(key)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    pub name: &'static str,
}

/// The host capability surface. The engine knows none of the concrete names.
pub trait Host {
    fn name(&self) -> &str;
    fn tick(&mut self, dt: Duration);
    fn get(&self, key: &str) -> Option<StateValue>;
    fn actions(&self) -> &[ActionSpec];
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

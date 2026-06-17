use std::time::Duration;

use crate::host::{ActionSpec, Host, Value};
use crate::state::StateValue;

/// Test-only, domain-neutral host: a `toggle` action flips `on`; `bump(n)` adds to
/// `level`; `level` also advances on tick. Never shipped.
pub struct FixtureHost {
    on: bool,
    level: f32,
}

impl FixtureHost {
    pub fn new() -> Self {
        Self { on: false, level: 0.0 }
    }
}

impl Default for FixtureHost {
    fn default() -> Self {
        Self::new()
    }
}

const ACTIONS: &[ActionSpec] = &[ActionSpec { name: "toggle" }, ActionSpec { name: "bump" }];

/// Test-only, domain-neutral host with a single `noop` action that flips `flag`.
/// Distinct from `FixtureHost` — has no `toggle`/`bump`/`on`/`level`. Never shipped.
pub struct OtherFixtureHost {
    flag: bool,
}

impl OtherFixtureHost {
    pub fn new() -> Self {
        Self { flag: false }
    }
}

impl Default for OtherFixtureHost {
    fn default() -> Self {
        Self::new()
    }
}

const OTHER_ACTIONS: &[ActionSpec] = &[ActionSpec { name: "noop" }];

impl Host for OtherFixtureHost {
    fn name(&self) -> &str {
        "other_fixture"
    }
    fn tick(&mut self, _dt: Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "flag" => Some(StateValue::Bool(self.flag)),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        OTHER_ACTIONS
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        if action == "noop" {
            self.flag = !self.flag;
        }
    }
}

impl Host for FixtureHost {
    fn name(&self) -> &str {
        "fixture"
    }
    fn tick(&mut self, dt: Duration) {
        self.level = (self.level + dt.as_secs_f32()).min(1.0);
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "on" => Some(StateValue::Bool(self.on)),
            "level" => Some(StateValue::Scalar(self.level)),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        match action {
            "toggle" => self.on = !self.on,
            "bump" => {
                if let Some(Value::Num(n)) = args.first() {
                    self.level = (self.level + *n as f32).min(1.0);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_flips_on() {
        let mut h = FixtureHost::new();
        assert_eq!(h.get("on"), Some(StateValue::Bool(false)));
        h.invoke("toggle", &[]);
        assert_eq!(h.get("on"), Some(StateValue::Bool(true)));
    }

    #[test]
    fn bump_uses_its_argument() {
        let mut h = FixtureHost::new();
        h.invoke("bump", &[Value::Num(0.25)]);
        assert_eq!(h.get("level"), Some(StateValue::Scalar(0.25)));
    }

    #[test]
    fn tick_advances_level_unknown_inert() {
        let mut h = FixtureHost::new();
        h.tick(Duration::from_secs_f32(0.5));
        assert_eq!(h.get("level"), Some(StateValue::Scalar(0.5)));
        assert_eq!(h.get("nope"), None);
        h.invoke("nope", &[]); // must not panic
    }
}

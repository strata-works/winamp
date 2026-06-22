use std::time::Duration;

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;
use sysinfo::System;

use crate::window::{WINDOW_ACTIONS, WindowOutbox, handle_window_action};

pub struct SysmonHost {
    sys: System,
    cpu: f32,
    mem: f32,
    swap: f32,
    window: WindowOutbox,
}

fn frac(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) as f32
    }
}

impl SysmonHost {
    pub fn with_outbox(window: WindowOutbox) -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            cpu: 0.0,
            mem: 0.0,
            swap: 0.0,
            window,
        }
    }
    pub fn new() -> Self {
        Self::with_outbox(WindowOutbox::default())
    }
}

impl Default for SysmonHost {
    fn default() -> Self {
        Self::new()
    }
}

impl Host for SysmonHost {
    fn name(&self) -> &str {
        "demo-sysmon"
    }
    fn tick(&mut self, _dt: Duration) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.cpu = (self.sys.global_cpu_usage() / 100.0).clamp(0.0, 1.0);
        self.mem = frac(self.sys.used_memory(), self.sys.total_memory());
        self.swap = frac(self.sys.used_swap(), self.sys.total_swap());
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "cpu" => Some(StateValue::Scalar(self.cpu)),
            "mem" => Some(StateValue::Scalar(self.mem)),
            "swap" => Some(StateValue::Scalar(self.swap)),
            "cpu_pct" => Some(StateValue::Str(std::sync::Arc::from(
                format!("{}%", (self.cpu * 100.0) as u32).as_str(),
            ))),
            "mem_used" => Some(StateValue::Str(std::sync::Arc::from(
                format!("{} MiB", self.sys.used_memory() / 1024 / 1024).as_str(),
            ))),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        WINDOW_ACTIONS
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        handle_window_action(action, &self.window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carapace::host::Host;
    use carapace::state::StateValue;
    use std::time::Duration;

    #[test]
    fn cpu_mem_swap_are_scalars_in_unit_range() {
        let mut h = SysmonHost::new();
        h.tick(Duration::from_millis(200)); // second sample populates cpu delta
        for key in ["cpu", "mem", "swap"] {
            match h.get(key) {
                Some(StateValue::Scalar(v)) => assert!((0.0..=1.0).contains(&v), "{key}={v}"),
                other => panic!("{key} should be a unit Scalar, got {other:?}"),
            }
        }
        assert!(matches!(h.get("cpu_pct"), Some(StateValue::Str(_))));
        assert!(h.get("nope").is_none());
    }
}

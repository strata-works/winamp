#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StateValue {
    Bool(bool),
    Scalar(f32),
}

/// A host exposes a generic capability surface. The engine knows none of the
/// concrete names — only this trait.
pub trait Host {
    fn name(&self) -> &'static str;
    fn tick(&mut self, dt: f32);
    fn get(&self, key: &str) -> Option<StateValue>;
    fn actions(&self) -> &'static [&'static str];
    fn invoke(&mut self, action: &str);
}

pub struct MediaHost {
    playing: bool,
    position: f32,
}

impl MediaHost {
    pub fn new() -> Self {
        Self { playing: false, position: 0.0 }
    }
}

impl Host for MediaHost {
    fn name(&self) -> &'static str {
        "media"
    }
    fn tick(&mut self, dt: f32) {
        if self.playing {
            self.position = (self.position + dt * 0.1).min(1.0);
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => Some(StateValue::Scalar(self.position)),
            _ => None,
        }
    }
    fn actions(&self) -> &'static [&'static str] {
        &["toggle_play", "stop"]
    }
    fn invoke(&mut self, action: &str) {
        match action {
            "toggle_play" => self.playing = !self.playing,
            "stop" => {
                self.playing = false;
                self.position = 0.0;
            }
            _ => {}
        }
    }
}

pub struct SysmonHost {
    cpu: f32,
    sampling: bool,
    phase: f32,
}

impl SysmonHost {
    pub fn new() -> Self {
        Self { cpu: 0.3, sampling: true, phase: 0.0 }
    }
}

impl Host for SysmonHost {
    fn name(&self) -> &'static str {
        "sysmon"
    }
    fn tick(&mut self, dt: f32) {
        if self.sampling {
            self.phase += dt;
            self.cpu = 0.5 + 0.5 * self.phase.sin();
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "cpu" => Some(StateValue::Scalar(self.cpu)),
            "sampling" => Some(StateValue::Bool(self.sampling)),
            _ => None,
        }
    }
    fn actions(&self) -> &'static [&'static str] {
        &["toggle_sampling"]
    }
    fn invoke(&mut self, action: &str) {
        if action == "toggle_sampling" {
            self.sampling = !self.sampling;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_position_advances_only_while_playing() {
        let mut h = MediaHost::new();
        h.tick(1.0);
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.0)));
        h.invoke("toggle_play");
        h.tick(1.0);
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.1)));
    }

    #[test]
    fn media_stop_resets_position_and_pauses() {
        let mut h = MediaHost::new();
        h.invoke("toggle_play");
        h.tick(2.0);
        h.invoke("stop");
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.0)));
    }

    #[test]
    fn unknown_key_and_action_are_inert() {
        let mut h = MediaHost::new();
        assert_eq!(h.get("nope"), None);
        h.invoke("nope"); // must not panic
    }

    #[test]
    fn sysmon_sampling_toggles_and_freezes_cpu() {
        let mut h = SysmonHost::new();
        h.invoke("toggle_sampling"); // now false
        let before = h.get("cpu");
        h.tick(1.0);
        assert_eq!(h.get("cpu"), before, "cpu frozen while not sampling");
        assert_eq!(h.get("sampling"), Some(StateValue::Bool(false)));
    }
}

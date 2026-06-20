use std::time::Duration;

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;

pub struct DemoHost {
    playing: bool,
    position: f32,
    track_title: String,
}

impl DemoHost {
    pub fn new() -> Self {
        Self {
            playing: false,
            position: 0.0,
            track_title: "Headspace — Track 01".to_string(),
        }
    }
}

impl Default for DemoHost {
    fn default() -> Self {
        Self::new()
    }
}

const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "toggle_play",
    },
    ActionSpec { name: "stop" },
];

impl Host for DemoHost {
    fn name(&self) -> &str {
        "demo-media"
    }
    fn tick(&mut self, dt: Duration) {
        if self.playing {
            self.position = (self.position + dt.as_secs_f32() * 0.1).min(1.0);
        }
    }
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => Some(StateValue::Scalar(self.position)),
            "track_title" => Some(StateValue::Str(std::sync::Arc::from(self.track_title.as_str()))),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        ACTIONS
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_and_advance_and_stop() {
        let mut h = DemoHost::new();
        h.invoke("toggle_play", &[]);
        h.tick(Duration::from_secs(1));
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.1)));
        assert_eq!(h.get("playing"), Some(StateValue::Bool(true)));
        h.invoke("stop", &[]);
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.0)));
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
    }
}

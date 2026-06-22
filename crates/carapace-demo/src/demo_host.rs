use std::time::Duration;

use carapace::host::{ActionSpec, Host, Value};
use carapace::state::StateValue;

use crate::window::{WINDOW_ACTIONS, WindowOutbox, handle_window_action};

pub struct DemoHost {
    playing: bool,
    position: f32,
    track_title: String,
    window: WindowOutbox,
    actions: Vec<ActionSpec>,
}

const DOMAIN_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "toggle_play",
    },
    ActionSpec { name: "stop" },
];

impl DemoHost {
    pub fn with_outbox(window: WindowOutbox) -> Self {
        let mut actions = DOMAIN_ACTIONS.to_vec();
        actions.extend_from_slice(WINDOW_ACTIONS);
        Self {
            playing: false,
            position: 0.0,
            track_title: "Headspace — Track 01".to_string(),
            window,
            actions,
        }
    }
    pub fn new() -> Self {
        Self::with_outbox(WindowOutbox::default())
    }
}

impl Default for DemoHost {
    fn default() -> Self {
        Self::new()
    }
}

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
            "track_title" => Some(StateValue::Str(std::sync::Arc::from(
                self.track_title.as_str(),
            ))),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }
    fn invoke(&mut self, action: &str, _args: &[Value]) {
        if handle_window_action(action, &self.window) {
            return;
        }
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

    #[test]
    fn window_action_is_recorded_to_the_outbox() {
        use crate::window::{WindowOp, WindowOutbox};
        let out: WindowOutbox = Default::default();
        let mut h = DemoHost::with_outbox(out.clone());
        h.invoke("minimize", &[]);
        assert_eq!(&*out.borrow(), &[WindowOp::Minimize]);
        // domain actions still work
        h.invoke("toggle_play", &[]);
        assert_eq!(h.get("playing"), Some(StateValue::Bool(true)));
    }
}

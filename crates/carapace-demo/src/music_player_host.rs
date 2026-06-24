use std::path::PathBuf;
use std::time::Duration;

use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::state::StateValue;

use crate::audio::AudioBackend;
use carapace_demo::window::{WINDOW_ACTIONS, WindowOutbox, handle_window_action};

const DOMAIN_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "toggle_play",
    },
    ActionSpec { name: "stop" },
    ActionSpec { name: "next" },
    ActionSpec { name: "prev" },
    ActionSpec { name: "seek" },
    ActionSpec { name: "play_index" },
];

pub struct Track {
    pub title: String,
    pub path: PathBuf,
    pub duration: Option<Duration>,
}

pub struct MusicPlayerHost {
    backend: Box<dyn AudioBackend>,
    playlist: Vec<Track>,
    current: usize,
    playing: bool,
    started: bool,
    window: WindowOutbox,
    actions: Vec<ActionSpec>,
}

fn fmt_mmss(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

impl MusicPlayerHost {
    pub fn new(backend: Box<dyn AudioBackend>, playlist: Vec<Track>, window: WindowOutbox) -> Self {
        let mut actions = DOMAIN_ACTIONS.to_vec();
        actions.extend_from_slice(WINDOW_ACTIONS);
        Self {
            backend,
            playlist,
            current: 0,
            playing: false,
            started: false,
            window,
            actions,
        }
    }

    fn load_current(&mut self) {
        let Some(track) = self.playlist.get(self.current) else {
            return;
        };
        match self.backend.play(&track.path) {
            Ok(()) => {
                self.started = true;
                self.playing = true;
            }
            Err(e) => {
                eprintln!("carapace-demo: audio error: {e:?}");
                self.playing = false;
            }
        }
    }
}

impl Host for MusicPlayerHost {
    fn name(&self) -> &str {
        "music-player"
    }

    fn tick(&mut self, _dt: Duration) {
        // Auto-advance when the current track finishes (rodio owns the audio thread; we poll).
        if self.playing && self.backend.is_finished() {
            self.invoke("next", &[]);
        }
    }

    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "playing" => Some(StateValue::Bool(self.playing)),
            "position" => {
                let pos = self.backend.position().as_secs_f32();
                let dur = self
                    .backend
                    .duration()
                    .map(|d| d.as_secs_f32())
                    .unwrap_or(0.0);
                let frac = if dur > 0.0 {
                    (pos / dur).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                Some(StateValue::Scalar(frac))
            }
            "track_title" => {
                let title = self
                    .playlist
                    .get(self.current)
                    .map(|t| t.title.as_str())
                    .unwrap_or("");
                Some(StateValue::Str(title.into()))
            }
            "time" => {
                let pos = self.backend.position();
                let dur = self.backend.duration().unwrap_or(Duration::ZERO);
                Some(StateValue::Str(
                    format!("{} / {}", fmt_mmss(pos), fmt_mmss(dur)).into(),
                ))
            }
            _ => None,
        }
    }

    fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }

    fn invoke(&mut self, action: &str, args: &[Value]) {
        if handle_window_action(action, &self.window) {
            return;
        }
        match action {
            "toggle_play" => {
                if !self.started {
                    self.load_current();
                } else {
                    self.playing = !self.playing;
                    self.backend.set_paused(!self.playing);
                }
            }
            "stop" => {
                self.backend.stop();
                self.playing = false;
                self.started = false;
            }
            "next" => {
                if self.current + 1 < self.playlist.len() {
                    self.current += 1;
                    self.load_current();
                } else {
                    self.backend.stop();
                    self.playing = false;
                    self.started = false;
                }
            }
            "prev" => {
                self.current = self.current.saturating_sub(1);
                self.load_current();
            }
            "play_index" => {
                if let Some(Value::Num(n)) = args.first() {
                    let i = *n as usize;
                    if i < self.playlist.len() {
                        self.current = i;
                        self.load_current();
                    }
                }
            }
            "seek" => {
                if let Some(Value::Num(f)) = args.first() {
                    self.backend.seek(*f as f32);
                }
            }
            _ => {}
        }
    }

    fn rows(&self, collection: &str) -> Vec<Row> {
        if collection != "playlist" {
            return Vec::new();
        }
        self.playlist
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let now = if i == self.current { "▶" } else { "" };
                let dur = t
                    .duration
                    .map(fmt_mmss)
                    .unwrap_or_else(|| "--:--".to_string());
                Row::new()
                    .set("now", StateValue::Str(now.into()))
                    .set("title", StateValue::Str(t.title.as_str().into()))
                    .set("duration", StateValue::Str(dur.as_str().into()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::MockAudio;
    use carapace::host::Value;
    use carapace::state::StateValue;
    use std::path::PathBuf;
    use std::time::Duration;

    fn track(title: &str, secs: u64) -> Track {
        Track {
            title: title.to_string(),
            path: PathBuf::from(format!("/tmp/{title}.wav")),
            duration: Some(Duration::from_secs(secs)),
        }
    }

    fn host() -> (
        MusicPlayerHost,
        std::rc::Rc<std::cell::RefCell<crate::audio::MockAudioState>>,
    ) {
        let (mock, state) = MockAudio::new();
        let playlist = vec![track("one", 10), track("two", 20), track("three", 30)];
        (
            MusicPlayerHost::new(Box::new(mock), playlist, Default::default()),
            state,
        )
    }

    #[test]
    fn cold_toggle_play_loads_current_then_pauses_resumes() {
        let (mut h, state) = host();
        h.invoke("toggle_play", &[]);
        assert_eq!(h.get("playing"), Some(StateValue::Bool(true)));
        assert_eq!(
            state.borrow().last_played,
            Some(PathBuf::from("/tmp/one.wav"))
        );
        h.invoke("toggle_play", &[]); // pause
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
        assert!(state.borrow().paused);
    }

    #[test]
    fn play_index_and_next_prev_navigate() {
        let (mut h, state) = host();
        h.invoke("play_index", &[Value::Num(1.0)]);
        assert_eq!(
            state.borrow().last_played,
            Some(PathBuf::from("/tmp/two.wav"))
        );
        h.invoke("next", &[]);
        assert_eq!(
            state.borrow().last_played,
            Some(PathBuf::from("/tmp/three.wav"))
        );
        h.invoke("next", &[]); // past the end → stop
        assert_eq!(h.get("playing"), Some(StateValue::Bool(false)));
        assert!(state.borrow().stopped);
        h.invoke("play_index", &[Value::Num(2.0)]);
        h.invoke("prev", &[]);
        assert_eq!(
            state.borrow().last_played,
            Some(PathBuf::from("/tmp/two.wav"))
        );
    }

    #[test]
    fn seek_forwards_fraction_to_backend() {
        let (mut h, state) = host();
        h.invoke("seek", &[Value::Num(0.25)]);
        assert_eq!(state.borrow().last_seek, Some(0.25));
    }

    #[test]
    fn tick_auto_advances_when_finished() {
        let (mut h, state) = host();
        h.invoke("toggle_play", &[]); // play track 0
        state.borrow_mut().finished = true;
        h.tick(Duration::from_millis(16));
        assert_eq!(
            state.borrow().last_played,
            Some(PathBuf::from("/tmp/two.wav")),
            "advanced to track 1"
        );
    }

    #[test]
    fn position_is_backend_fraction_and_rows_mark_current() {
        let (mut h, state) = host();
        h.invoke("play_index", &[Value::Num(1.0)]);
        state.borrow_mut().position = Duration::from_secs(5);
        state.borrow_mut().duration = Some(Duration::from_secs(20));
        assert_eq!(h.get("position"), Some(StateValue::Scalar(0.25)));

        let rows = h.rows("playlist");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].get("now"), Some(&StateValue::Str("▶".into())));
        assert_eq!(rows[0].get("now"), Some(&StateValue::Str("".into())));
        assert_eq!(rows[1].get("title"), Some(&StateValue::Str("two".into())));
        assert_eq!(
            rows[1].get("duration"),
            Some(&StateValue::Str("0:20".into()))
        );
    }
}

use std::path::Path;
use std::time::Duration;

/// What can go wrong loading/decoding a track. Logged, never panics.
// The String payload is only used via Debug formatting; `Unsupported` is reserved.
#[allow(dead_code)]
#[derive(Debug)]
pub enum AudioError {
    Open(String),
    Decode(String),
    Unsupported,
}

/// One audio output sink. Real impl wraps rodio; tests use MockAudio.
pub trait AudioBackend {
    /// Load `path` and begin playing it, replacing any current track.
    fn play(&mut self, path: &Path) -> Result<(), AudioError>;
    fn set_paused(&mut self, paused: bool);
    fn stop(&mut self);
    /// Seek to `fraction` (0..1) of the current track.
    fn seek(&mut self, fraction: f32);
    fn position(&self) -> Duration;
    fn duration(&self) -> Option<Duration>;
    /// The current source has played to its end.
    fn is_finished(&self) -> bool;
}

/// A no-op backend used when no audio device is available, so the demo never panics.
pub struct NullAudio;
impl AudioBackend for NullAudio {
    fn play(&mut self, _path: &Path) -> Result<(), AudioError> {
        Ok(())
    }
    fn set_paused(&mut self, _paused: bool) {}
    fn stop(&mut self) {}
    fn seek(&mut self, _fraction: f32) {}
    fn position(&self) -> Duration {
        Duration::ZERO
    }
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn is_finished(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MockAudioState {
    pub last_played: Option<std::path::PathBuf>,
    pub paused: bool,
    pub stopped: bool,
    pub last_seek: Option<f32>,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub finished: bool,
}

#[cfg(test)]
pub struct MockAudio {
    state: std::rc::Rc<std::cell::RefCell<MockAudioState>>,
}

#[cfg(test)]
impl MockAudio {
    /// Returns the backend and a handle to its shared state for the test to drive.
    pub fn new() -> (Self, std::rc::Rc<std::cell::RefCell<MockAudioState>>) {
        let state = std::rc::Rc::new(std::cell::RefCell::new(MockAudioState::default()));
        (
            Self {
                state: state.clone(),
            },
            state,
        )
    }
}

#[cfg(test)]
impl AudioBackend for MockAudio {
    fn play(&mut self, path: &Path) -> Result<(), AudioError> {
        let mut s = self.state.borrow_mut();
        s.last_played = Some(path.to_path_buf());
        s.paused = false;
        s.stopped = false;
        s.finished = false;
        s.position = Duration::ZERO;
        Ok(())
    }
    fn set_paused(&mut self, paused: bool) {
        self.state.borrow_mut().paused = paused;
    }
    fn stop(&mut self) {
        self.state.borrow_mut().stopped = true;
    }
    fn seek(&mut self, fraction: f32) {
        self.state.borrow_mut().last_seek = Some(fraction);
    }
    fn position(&self) -> Duration {
        self.state.borrow().position
    }
    fn duration(&self) -> Option<Duration> {
        self.state.borrow().duration
    }
    fn is_finished(&self) -> bool {
        self.state.borrow().finished
    }
}

use std::fs::File;
use std::io::BufReader;

/// Real audio backend backed by `rodio`. Constructed once at startup; call
/// `RodioBackend::new()` which returns `Err(AudioError)` if no output device
/// is available (so the demo can fall back to `NullAudio` without panicking).
pub struct RodioBackend {
    // MixerDeviceSink must be kept alive for audio to play through the OS sink.
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
    duration: Option<Duration>,
}

impl RodioBackend {
    pub fn new() -> Result<Self, AudioError> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| AudioError::Open(e.to_string()))?;
        sink.log_on_drop(false); // suppress rodio's stderr message on shutdown drop
        let player = rodio::Player::connect_new(sink.mixer());
        Ok(Self {
            _sink: sink,
            player,
            duration: None,
        })
    }
}

impl AudioBackend for RodioBackend {
    fn play(&mut self, path: &Path) -> Result<(), AudioError> {
        use rodio::Source;
        let file = File::open(path).map_err(|e| AudioError::Open(e.to_string()))?;
        let decoder = rodio::Decoder::new(BufReader::new(file))
            .map_err(|e| AudioError::Decode(e.to_string()))?;
        self.duration = decoder.total_duration();
        // Stop the current track then queue the new one on the same player.
        self.player.stop();
        self.player.append(decoder);
        self.player.play();
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }
    }

    fn stop(&mut self) {
        self.player.stop();
        self.duration = None;
    }

    fn seek(&mut self, fraction: f32) {
        if let Some(dur) = self.duration {
            let target = dur.mul_f32(fraction.clamp(0.0, 1.0));
            let _ = self.player.try_seek(target); // best-effort; some formats can't seek
        }
    }

    fn position(&self) -> Duration {
        self.player.get_pos()
    }

    fn duration(&self) -> Option<Duration> {
        self.duration
    }

    fn is_finished(&self) -> bool {
        self.player.empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn mock_records_play_pause_seek_and_reports_state() {
        let (mut audio, state) = MockAudio::new();
        audio.play(&PathBuf::from("/tmp/a.wav")).unwrap();
        assert_eq!(
            state.borrow().last_played,
            Some(PathBuf::from("/tmp/a.wav"))
        );

        audio.set_paused(true);
        assert!(state.borrow().paused);

        audio.seek(0.5);
        assert_eq!(state.borrow().last_seek, Some(0.5));

        // Test drives position + finished through the shared state.
        state.borrow_mut().position = Duration::from_secs(3);
        state.borrow_mut().duration = Some(Duration::from_secs(10));
        state.borrow_mut().finished = true;
        assert_eq!(audio.position(), Duration::from_secs(3));
        assert_eq!(audio.duration(), Some(Duration::from_secs(10)));
        assert!(audio.is_finished());

        audio.stop();
        assert!(state.borrow().stopped);
    }
}

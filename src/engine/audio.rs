use rodio::{source::SineWave, OutputStream, OutputStreamHandle, Sink, Source};
use std::time::Duration;

pub struct AudioManager {
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    pub is_enabled: bool,
    pub volume: f32,
}

impl AudioManager {
    pub fn new() -> Self {
        // Safely attempt audio device initialization (fails gracefully on headless/silent setups)
        match OutputStream::try_default() {
            Ok((stream, stream_handle)) => {
                log::info!("Audio device initialized successfully");
                Self {
                    _stream: Some(stream),
                    stream_handle: Some(stream_handle),
                    is_enabled: true,
                    volume: 0.35,
                }
            }
            Err(e) => {
                log::warn!("Could not initialize default audio output device: {:?}", e);
                Self {
                    _stream: None,
                    stream_handle: None,
                    is_enabled: false,
                    volume: 0.35,
                }
            }
        }
    }

    /// Play a crisp, high-frequency hit "ding/pop" sound
    pub fn play_hit(&self) {
        if !self.is_enabled {
            return;
        }

        if let Some(handle) = &self.stream_handle {
            if let Ok(sink) = Sink::try_new(handle) {
                sink.set_volume(self.volume);
                // Crisp 880 Hz ding (A5 tone) with quick fade-out
                let source = SineWave::new(880.0)
                    .take_duration(Duration::from_millis(65))
                    .amplify(1.2);
                sink.append(source);
                sink.detach();
            }
        }
    }

    /// Play a subtle low-frequency "thud/miss" sound
    pub fn play_miss(&self) {
        if !self.is_enabled {
            return;
        }

        if let Some(handle) = &self.stream_handle {
            if let Ok(sink) = Sink::try_new(handle) {
                sink.set_volume(self.volume * 0.45);
                // Low 180 Hz soft thud
                let source = SineWave::new(180.0).take_duration(Duration::from_millis(45));
                sink.append(source);
                sink.detach();
            }
        }
    }

    /// Play countdown beep (e.g. 3, 2, 1)
    pub fn play_countdown_beep(&self, high_pitch: bool) {
        if !self.is_enabled {
            return;
        }

        if let Some(handle) = &self.stream_handle {
            if let Ok(sink) = Sink::try_new(handle) {
                sink.set_volume(self.volume * 0.6);
                let freq = if high_pitch { 900.0 } else { 450.0 };
                let dur = if high_pitch { 120 } else { 75 };
                let source = SineWave::new(freq).take_duration(Duration::from_millis(dur));
                sink.append(source);
                sink.detach();
            }
        }
    }

    /// Play task completion fanfare chime
    pub fn play_complete(&self) {
        if !self.is_enabled {
            return;
        }

        if let Some(handle) = &self.stream_handle {
            if let Ok(sink) = Sink::try_new(handle) {
                sink.set_volume(self.volume * 0.8);
                // Two-tone rising chime (587 Hz -> 880 Hz)
                let note1 = SineWave::new(587.33).take_duration(Duration::from_millis(100));
                let note2 = SineWave::new(880.00).take_duration(Duration::from_millis(220));
                sink.append(note1);
                sink.append(note2);
                sink.detach();
            }
        }
    }
}


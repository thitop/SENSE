//! Sound effects and music. Effects and music loops were generated with ElevenLabs
//! (`tools/generate_audio.py`) and are embedded in the binary. Effects are decoded once at
//! startup so playing one costs no decoding; music streams from the embedded MP3.

use rodio::buffer::SamplesBuffer;
use rodio::source::{ChannelVolume, SineWave};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::Cursor;
use std::time::Duration;

/// A sound effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Hit,
    Miss,
    CountdownTick,
    CountdownGo,
    /// A target appeared outside the view; played panned toward it.
    SpawnCue,
    SensChange,
    Complete,
    /// Soft tick repeated while the crosshair stays on a tracking target.
    TrackTick,
}

impl Sfx {
    pub const ALL: [Sfx; 8] = [
        Sfx::Hit,
        Sfx::Miss,
        Sfx::CountdownTick,
        Sfx::CountdownGo,
        Sfx::SpawnCue,
        Sfx::SensChange,
        Sfx::Complete,
        Sfx::TrackTick,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }

    pub fn bytes(self) -> &'static [u8] {
        match self {
            // The tracking tick is the start of the hit sound, cut short and played softly.
            Sfx::Hit | Sfx::TrackTick => include_bytes!("../../assets/sounds/hit.mp3"),
            Sfx::Miss => include_bytes!("../../assets/sounds/miss.mp3"),
            Sfx::CountdownTick => include_bytes!("../../assets/sounds/countdown_tick.mp3"),
            Sfx::CountdownGo => include_bytes!("../../assets/sounds/countdown_go.mp3"),
            Sfx::SpawnCue => include_bytes!("../../assets/sounds/spawn_cue.mp3"),
            Sfx::SensChange => include_bytes!("../../assets/sounds/sens_change.mp3"),
            Sfx::Complete => include_bytes!("../../assets/sounds/complete.mp3"),
        }
    }

    /// Longest the effect may ring. Shots come fast, so hit and miss stay short.
    pub fn max_secs(self) -> f32 {
        match self {
            Sfx::Hit | Sfx::Miss => 0.25,
            // Shorter than the tick interval, so ticks never overlap.
            Sfx::TrackTick => 0.06,
            Sfx::CountdownTick | Sfx::SpawnCue => 0.35,
            Sfx::CountdownGo | Sfx::SensChange => 0.8,
            Sfx::Complete => 1.5,
        }
    }

    /// Loudness relative to the effects volume, after peak normalising.
    fn gain(self) -> f32 {
        match self {
            Sfx::Hit => 0.9,
            Sfx::Miss => 0.45,
            Sfx::CountdownTick => 0.6,
            Sfx::CountdownGo => 0.75,
            Sfx::SpawnCue => 0.8,
            Sfx::SensChange => 0.5,
            Sfx::Complete => 0.8,
            Sfx::TrackTick => 0.35,
        }
    }

    /// Tone used if the embedded effect can't be decoded.
    fn fallback(self) -> (f32, u64) {
        match self {
            Sfx::Hit => (880.0, 65),
            Sfx::Miss => (180.0, 45),
            Sfx::CountdownTick => (450.0, 75),
            Sfx::CountdownGo => (900.0, 120),
            Sfx::SpawnCue => (1318.5, 70),
            Sfx::SensChange => (660.0, 90),
            Sfx::Complete => (587.33, 220),
            Sfx::TrackTick => (1200.0, 30),
        }
    }
}

/// When to tick while tracking: once on getting onto the target, then every `INTERVAL_SECS`
/// while the crosshair stays on it; silent off it.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrackingTicker {
    /// Time since the last tick; `None` while off the target.
    since_tick: Option<f32>,
}

impl TrackingTicker {
    pub const INTERVAL_SECS: f32 = 0.1;

    /// Advance by `dt`; true when a tick should play this frame.
    pub fn update(&mut self, dt: f32, on_target: bool) -> bool {
        if !on_target {
            self.since_tick = None;
            return false;
        }
        match self.since_tick {
            None => {
                self.since_tick = Some(0.0);
                true
            }
            Some(t) if t + dt >= Self::INTERVAL_SECS => {
                self.since_tick = Some(t + dt - Self::INTERVAL_SECS);
                true
            }
            Some(t) => {
                self.since_tick = Some(t + dt);
                false
            }
        }
    }
}

/// A music loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Music {
    Menu,
    Game,
}

impl Music {
    pub const ALL: [Music; 2] = [Music::Menu, Music::Game];

    pub fn bytes(self) -> &'static [u8] {
        match self {
            Music::Menu => include_bytes!("../../assets/sounds/music_menu.mp3"),
            Music::Game => include_bytes!("../../assets/sounds/music_game.mp3"),
        }
    }

    /// In-game music sits under the effects so cues stay clear.
    pub fn level(self) -> f32 {
        match self {
            Music::Menu => 0.5,
            Music::Game => 0.3,
        }
    }
}

/// Decoded audio, ready to play.
#[derive(Clone, Debug)]
pub struct Clip {
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples.
    pub samples: Vec<f32>,
}

impl Clip {
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / (self.channels as f32 * self.sample_rate as f32)
    }

    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }
}

/// Decode an effect: drop the silence before it starts, cut it to `max_secs` with a short fade
/// out, and normalise its peak to 1.
pub fn decode_effect(bytes: &'static [u8], max_secs: f32) -> Option<Clip> {
    let decoder = Decoder::new(Cursor::new(bytes)).ok()?;
    let channels = decoder.channels().max(1);
    let sample_rate = decoder.sample_rate();
    let raw: Vec<f32> = decoder.convert_samples().collect();
    let ch = channels as usize;
    let peak = raw.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if peak <= 0.0 {
        return None;
    }

    // Start 2 ms before the first frame above 5% of the peak.
    let loud = |frame: &[f32]| frame.iter().any(|s| s.abs() > peak * 0.05);
    let first = raw.chunks(ch).position(loud)?;
    let lead = (sample_rate as f32 * 0.002) as usize;
    let start = first.saturating_sub(lead) * ch;
    // End at the last frame above 0.5% of the peak, or at `max_secs`.
    let last = raw.chunks(ch).rposition(|f| f.iter().any(|s| s.abs() > peak * 0.005))?;
    let max_frames = (max_secs * sample_rate as f32) as usize;
    let end = ((last + 1) * ch).min(start + max_frames * ch);

    let mut samples: Vec<f32> = raw[start..end].iter().map(|s| s / peak).collect();
    let fade = ((sample_rate as f32 * 0.015) as usize).min(samples.len() / ch);
    let frames = samples.len() / ch;
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        for c in 0..ch {
            samples[(frames - 1 - i) * ch + c] *= g;
        }
    }
    Some(Clip { channels, sample_rate, samples })
}

pub struct AudioManager {
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    /// Sound effects on. Music has its own volume.
    pub is_enabled: bool,
    /// Sound effects volume.
    pub volume: f32,
    music_volume: f32,
    clips: Vec<Option<Clip>>,
    music: Option<(Music, Sink)>,
}

impl AudioManager {
    pub const DEFAULT_MUSIC_VOLUME: f32 = 0.5;

    pub fn new() -> Self {
        // Safely attempt audio device initialization (fails gracefully on headless/silent setups)
        let (stream, handle) = match OutputStream::try_default() {
            Ok((stream, handle)) => {
                log::info!("Audio device initialized successfully");
                (Some(stream), Some(handle))
            }
            Err(e) => {
                log::warn!("Could not initialize default audio output device: {:?}", e);
                (None, None)
            }
        };
        let clips = Sfx::ALL
            .iter()
            .map(|s| {
                let clip = decode_effect(s.bytes(), s.max_secs());
                if clip.is_none() {
                    log::warn!("Could not decode {s:?}; using a tone instead");
                }
                clip
            })
            .collect();
        Self {
            is_enabled: handle.is_some(),
            _stream: stream,
            stream_handle: handle,
            volume: 0.35,
            music_volume: Self::DEFAULT_MUSIC_VOLUME,
            clips,
            music: None,
        }
    }

    /// Play `sfx`; `pan` (-1 left .. 1 right) places it to one side.
    pub fn play(&self, sfx: Sfx, pan: Option<f32>) {
        if !self.is_enabled {
            return;
        }
        let Some(handle) = &self.stream_handle else { return };
        let Ok(sink) = Sink::try_new(handle) else { return };
        sink.set_volume(self.volume * sfx.gain());
        let source: Box<dyn Source<Item = f32> + Send> = match &self.clips[sfx.index()] {
            Some(c) => Box::new(SamplesBuffer::new(c.channels, c.sample_rate, c.samples.clone())),
            None => {
                let (freq, ms) = sfx.fallback();
                Box::new(SineWave::new(freq).take_duration(Duration::from_millis(ms)).amplify(0.6))
            }
        };
        match pan {
            Some(p) => {
                let (left, right) = pan_gains(p);
                sink.append(ChannelVolume::new(source, vec![left, right]));
            }
            None => sink.append(source),
        }
        sink.detach();
    }

    pub fn play_hit(&self) {
        self.play(Sfx::Hit, None);
    }

    pub fn play_miss(&self) {
        self.play(Sfx::Miss, None);
    }

    /// `go`: the countdown reached zero.
    pub fn play_countdown_beep(&self, go: bool) {
        self.play(if go { Sfx::CountdownGo } else { Sfx::CountdownTick }, None);
    }

    /// A target appeared outside the view, `pan` toward it.
    pub fn play_spawn_cue(&self, pan: f32) {
        self.play(Sfx::SpawnCue, Some(pan));
    }

    pub fn play_complete(&self) {
        self.play(Sfx::Complete, None);
    }

    pub fn music_volume(&self) -> f32 {
        self.music_volume
    }

    pub fn set_music_volume(&mut self, volume: f32) {
        self.music_volume = volume.clamp(0.0, 1.0);
        if let Some((track, sink)) = &self.music {
            sink.set_volume(self.music_volume * track.level());
        }
    }

    /// Loop `track`, or stop the music with `None`. Changing tracks fades the new one in.
    pub fn set_music(&mut self, track: Option<Music>) {
        if self.music.as_ref().map(|(t, _)| *t) == track {
            return;
        }
        if let Some((_, sink)) = self.music.take() {
            sink.stop();
        }
        let (Some(track), Some(handle)) = (track, &self.stream_handle) else { return };
        let Ok(sink) = Sink::try_new(handle) else { return };
        let Ok(source) = Decoder::new_looped(Cursor::new(track.bytes())) else {
            log::warn!("Could not decode {track:?} music");
            return;
        };
        sink.set_volume(self.music_volume * track.level());
        sink.append(source.fade_in(Duration::from_millis(1200)));
        self.music = Some((track, sink));
    }
}

/// Equal-power stereo gains for `pan` in -1 (left) ..= 1 (right).
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (a.cos(), a.sin())
}

//! The embedded ElevenLabs sounds decode, start right away, and stay within their lengths.

use aim_trainer::engine::audio::{decode_effect, Music, Sfx};
use rodio::{Decoder, Source};
use std::io::Cursor;

#[test]
fn every_effect_decodes_and_is_normalised() {
    for sfx in Sfx::ALL {
        let clip = decode_effect(sfx.bytes(), sfx.max_secs()).unwrap_or_else(|| panic!("{sfx:?}"));
        let secs = clip.duration_secs();
        println!("{sfx:?}: {secs:.3} s, {} ch, {} Hz", clip.channels, clip.sample_rate);
        assert!(secs > 0.02 && secs <= sfx.max_secs() + 0.001, "{sfx:?}: {secs} s");
        assert!((clip.peak() - 1.0).abs() < 0.05, "{sfx:?}: peak {}", clip.peak());
    }
}

#[test]
fn effects_start_within_a_few_milliseconds() {
    for sfx in Sfx::ALL {
        let clip = decode_effect(sfx.bytes(), sfx.max_secs()).unwrap();
        let frames_5ms = (clip.sample_rate as f32 * 0.005) as usize * clip.channels as usize;
        let early = clip.samples[..frames_5ms].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(early > 0.04, "{sfx:?} is silent for its first 5 ms ({early})");
    }
}

#[test]
fn effects_fade_out_instead_of_clicking() {
    for sfx in Sfx::ALL {
        let clip = decode_effect(sfx.bytes(), sfx.max_secs()).unwrap();
        let last = clip.samples[clip.samples.len() - clip.channels as usize..].iter();
        assert!(last.fold(0.0_f32, |m, s| m.max(s.abs())) < 0.01, "{sfx:?}");
    }
}

#[test]
fn music_loops_decode_and_last_long_enough_to_loop() {
    for track in Music::ALL {
        let d = Decoder::new(Cursor::new(track.bytes())).unwrap();
        let (ch, rate) = (d.channels() as f32, d.sample_rate() as f32);
        let samples: Vec<f32> = d.convert_samples().collect();
        let secs = samples.len() as f32 / ch / rate;
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        println!("{track:?}: {secs:.1} s, rms {rms:.3}");
        assert!(secs >= 20.0, "{track:?}: {secs} s");
        assert!(rms > 0.01, "{track:?} is nearly silent");
        assert!(Decoder::new_looped(Cursor::new(track.bytes())).is_ok());
    }
}

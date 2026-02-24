pub mod analyzer;
pub mod beat;
pub mod capture;
pub mod features;
pub mod normalizer;
pub mod smoother;

pub use features::AudioFeatures;

use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use self::analyzer::FftAnalyzer;
use self::beat::BeatDetector;
use self::capture::AudioCapture;
use self::normalizer::AdaptiveNormalizer;
use self::smoother::FeatureSmoother;

/// Manages the audio pipeline: capture -> FFT -> normalize -> beat detect -> smooth -> send to main thread.
pub struct AudioSystem {
    receiver: Receiver<AudioFeatures>,
    latest: Option<AudioFeatures>,
    pub device_name: String,
    pub active: bool,
}

impl AudioSystem {
    pub fn new() -> Self {
        let (tx, rx): (Sender<AudioFeatures>, Receiver<AudioFeatures>) =
            crossbeam_channel::bounded(4);

        let mut device_name = "None".to_string();
        let mut active = false;

        match AudioCapture::new() {
            Ok(capture) => {
                device_name = capture.device_name.clone();
                active = true;
                let sample_rate = capture.sample_rate as f32;

                thread::Builder::new()
                    .name("phosphor-audio".into())
                    .spawn(move || {
                        audio_thread(capture, sample_rate, tx);
                    })
                    .expect("Failed to spawn audio thread");
            }
            Err(e) => {
                log::warn!("Audio capture unavailable: {e}");
            }
        }

        Self {
            receiver: rx,
            latest: None,
            device_name,
            active,
        }
    }

    /// Drain the channel and return the most recent features.
    pub fn latest_features(&mut self) -> Option<AudioFeatures> {
        while let Ok(features) = self.receiver.try_recv() {
            self.latest = Some(features);
        }
        self.latest
    }
}

fn audio_thread(capture: AudioCapture, sample_rate: f32, tx: Sender<AudioFeatures>) {
    let mut analyzer = FftAnalyzer::new(sample_rate);
    let mut normalizer = AdaptiveNormalizer::new();
    let mut beat_detector = BeatDetector::new(sample_rate);
    let mut smoother = FeatureSmoother::new();
    let mut read_buf = vec![0.0f32; 8192]; // larger for 4096-pt FFT
    let mut last_time = Instant::now();
    let start_time = Instant::now();

    loop {
        thread::sleep(Duration::from_millis(10));

        let available = capture.ring.available();
        if available == 0 {
            continue;
        }

        let to_read = available.min(read_buf.len());
        let read = capture.ring.read(&mut read_buf[..to_read]);
        if read == 0 {
            continue;
        }

        let now = Instant::now();
        let dt = now.duration_since(last_time).as_secs_f32();
        let timestamp = now.duration_since(start_time).as_secs_f64();
        last_time = now;

        // Multi-resolution FFT + feature extraction
        let mut raw = analyzer.analyze(&read_buf[..read]);

        // Adaptive normalization (replaces fixed gains)
        raw = normalizer.normalize(&raw);

        // Beat detection (on raw magnitude spectra)
        let beat_result = beat_detector.process(
            analyzer.bass_magnitude(),
            analyzer.mid_magnitude(),
            analyzer.high_magnitude(),
            raw.rms,
            timestamp,
        );
        raw.onset = beat_result.onset_strength;
        raw.beat = beat_result.beat;
        raw.beat_phase = beat_result.beat_phase;
        raw.bpm = beat_result.bpm / 300.0; // normalize to 0-1
        raw.beat_strength = beat_result.beat_strength;

        // Smoothing (per-feature asymmetric EMA; beat/beat_phase pass through)
        let smoothed = smoother.smooth(&raw, dt);

        // Non-blocking send; drop if main thread is behind
        let _ = tx.try_send(smoothed);
    }
}

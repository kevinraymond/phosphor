pub mod analyzer;
pub mod capture;
pub mod features;
pub mod smoother;

pub use features::AudioFeatures;

use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use self::analyzer::FftAnalyzer;
use self::capture::AudioCapture;
use self::smoother::FeatureSmoother;

/// Manages the audio pipeline: capture -> FFT -> smooth -> send to main thread.
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
    let mut smoother = FeatureSmoother::new();
    let mut read_buf = vec![0.0f32; 4096];
    let mut last_time = Instant::now();

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
        last_time = now;

        let raw = analyzer.analyze(&read_buf[..read]);
        let smoothed = smoother.smooth(&raw, dt);

        // Non-blocking send; drop if main thread is behind
        let _ = tx.try_send(smoothed);
    }
}

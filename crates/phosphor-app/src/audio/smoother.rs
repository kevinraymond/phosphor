use super::features::{AudioFeatures, NUM_FEATURES};

/// Per-feature attack/release time constants (seconds).
struct SmoothParams {
    attack: f32,
    release: f32,
    bypass: bool, // pass-through without smoothing
}

/// Asymmetric attack/release EMA smoother for 20 audio features.
pub struct FeatureSmoother {
    state: [f32; NUM_FEATURES],
    params: [SmoothParams; NUM_FEATURES],
}

impl FeatureSmoother {
    pub fn new() -> Self {
        let params = [
            // Frequency bands (7)
            SmoothParams { attack: 0.02, release: 0.15, bypass: false },   // sub_bass
            SmoothParams { attack: 0.02, release: 0.15, bypass: false },   // bass
            SmoothParams { attack: 0.01, release: 0.10, bypass: false },   // low_mid
            SmoothParams { attack: 0.01, release: 0.10, bypass: false },   // mid
            SmoothParams { attack: 0.005, release: 0.08, bypass: false },  // upper_mid
            SmoothParams { attack: 0.005, release: 0.08, bypass: false },  // presence
            SmoothParams { attack: 0.005, release: 0.08, bypass: false },  // brilliance
            // Aggregates (2)
            SmoothParams { attack: 0.01, release: 0.12, bypass: false },   // rms
            SmoothParams { attack: 0.002, release: 0.06, bypass: false },  // kick (fast attack)
            // Spectral shape (6)
            SmoothParams { attack: 0.03, release: 0.15, bypass: false },   // centroid
            SmoothParams { attack: 0.005, release: 0.06, bypass: false },  // flux
            SmoothParams { attack: 0.05, release: 0.20, bypass: false },   // flatness
            SmoothParams { attack: 0.03, release: 0.15, bypass: false },   // rolloff
            SmoothParams { attack: 0.03, release: 0.15, bypass: false },   // bandwidth
            SmoothParams { attack: 0.02, release: 0.10, bypass: false },   // zcr
            // Beat detection (5)
            SmoothParams { attack: 0.001, release: 0.05, bypass: false },  // onset (very fast)
            SmoothParams { attack: 0.0, release: 0.0, bypass: true },      // beat (pass-through)
            SmoothParams { attack: 0.0, release: 0.0, bypass: true },      // beat_phase (pass-through)
            SmoothParams { attack: 0.5, release: 1.0, bypass: false },     // bpm (very slow)
            SmoothParams { attack: 0.001, release: 0.08, bypass: false },  // beat_strength (fast attack)
        ];

        Self {
            state: [0.0; NUM_FEATURES],
            params,
        }
    }

    /// Smooth raw features with asymmetric EMA.
    /// dt is time since last call in seconds.
    pub fn smooth(&mut self, raw: &AudioFeatures, dt: f32) -> AudioFeatures {
        let raw_slice = raw.as_slice();
        let mut out = AudioFeatures::default();
        let out_slice = out.as_slice_mut();

        for i in 0..NUM_FEATURES {
            if self.params[i].bypass {
                out_slice[i] = raw_slice[i];
                self.state[i] = raw_slice[i];
                continue;
            }

            let target = raw_slice[i];
            let rising = target > self.state[i];
            let tau = if rising {
                self.params[i].attack
            } else {
                self.params[i].release
            };
            // EMA coefficient: alpha = 1 - exp(-dt/tau)
            let alpha = 1.0 - (-dt / tau.max(0.001)).exp();
            self.state[i] += alpha * (target - self.state[i]);
            out_slice[i] = self.state[i];
        }

        out
    }
}

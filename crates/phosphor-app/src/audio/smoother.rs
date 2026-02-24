use super::features::AudioFeatures;

/// Per-feature attack/release time constants (seconds).
struct SmoothParams {
    attack: f32,
    release: f32,
}

/// Asymmetric attack/release EMA smoother.
/// Ported from spectral-senses/src/audio/feature_smoother.cpp
pub struct FeatureSmoother {
    state: [f32; 12],
    params: [SmoothParams; 12],
}

impl FeatureSmoother {
    pub fn new() -> Self {
        // Time constants from spectral-senses/src/audio/feature_smoother.cpp:11-22
        let params = [
            SmoothParams { attack: 0.02, release: 0.15 },   // bass
            SmoothParams { attack: 0.01, release: 0.10 },   // mid
            SmoothParams { attack: 0.005, release: 0.08 },  // treble
            SmoothParams { attack: 0.01, release: 0.12 },   // rms
            SmoothParams { attack: 0.05, release: 0.20 },   // phase
            SmoothParams { attack: 0.001, release: 0.05 },  // onset (very fast attack)
            SmoothParams { attack: 0.03, release: 0.15 },   // centroid
            SmoothParams { attack: 0.005, release: 0.06 },  // flux
            SmoothParams { attack: 0.05, release: 0.20 },   // flatness
            SmoothParams { attack: 0.03, release: 0.15 },   // rolloff
            SmoothParams { attack: 0.03, release: 0.15 },   // bandwidth
            SmoothParams { attack: 0.02, release: 0.10 },   // zcr
        ];

        Self {
            state: [0.0; 12],
            params,
        }
    }

    /// Smooth raw features with asymmetric EMA.
    /// dt is time since last call in seconds.
    pub fn smooth(&mut self, raw: &AudioFeatures, dt: f32) -> AudioFeatures {
        let raw_slice = raw.as_slice();
        let mut out = AudioFeatures::default();
        let out_slice = out.as_slice_mut();

        for i in 0..12 {
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

impl Default for AudioFeatures {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

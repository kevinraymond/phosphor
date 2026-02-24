use super::features::{AudioFeatures, NUM_FEATURES};

/// Per-feature running min/max for adaptive normalization.
/// Replaces all fixed gain multipliers with auto-leveling.
/// Ported from easey-glyph's adaptive normalization.
pub struct AdaptiveNormalizer {
    running_min: [f32; NUM_FEATURES],
    running_max: [f32; NUM_FEATURES],
    decay: f32,
}

impl AdaptiveNormalizer {
    pub fn new() -> Self {
        Self {
            running_min: [0.0; NUM_FEATURES],
            running_max: [0.01; NUM_FEATURES], // small initial range to avoid div-by-zero
            decay: 0.005,
        }
    }

    /// Normalize all features to 0-1 using adaptive running min/max.
    /// Beat detection fields (indices 16-19: beat, beat_phase, bpm, beat_strength)
    /// are passed through unchanged — they're already normalized by the beat detector.
    pub fn normalize(&mut self, raw: &AudioFeatures) -> AudioFeatures {
        let raw_slice = raw.as_slice();
        let mut out = AudioFeatures::default();
        let out_slice = out.as_slice_mut();

        for i in 0..NUM_FEATURES {
            let v = raw_slice[i];

            // Skip beat detection fields (onset through beat_strength, indices 16-19)
            // onset (16) gets normalized, beat (17), beat_phase (18), bpm (19), beat_strength (20-1=19)
            // Actually: onset=16, beat=17, beat_phase=18, bpm=19, beat_strength=19
            // Let's just skip beat, beat_phase, bpm — they come pre-normalized from beat detector
            if i >= 17 && i <= 19 {
                out_slice[i] = v;
                continue;
            }

            // Running min: decays toward value, instantly jumps down
            self.running_min[i] += self.decay * (v - self.running_min[i]);
            self.running_min[i] = self.running_min[i].min(v);

            // Running max: decays toward value, instantly jumps up
            self.running_max[i] += self.decay * (v - self.running_max[i]);
            self.running_max[i] = self.running_max[i].max(v);

            let span = self.running_max[i] - self.running_min[i];
            if span > 0.01 {
                out_slice[i] = ((v - self.running_min[i]) / span).clamp(0.0, 1.0);
            } else {
                out_slice[i] = 0.0;
            }
        }

        out
    }
}

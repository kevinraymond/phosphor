use bytemuck::{Pod, Zeroable};

/// 20 audio features, all normalized to 0.0-1.0 range.
/// Multi-resolution FFT bands + spectral shape + beat detection.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct AudioFeatures {
    // Frequency bands (7) — multi-resolution FFT
    pub sub_bass: f32,    // 20-60 Hz (kick fundamentals)
    pub bass: f32,        // 60-250 Hz (bass guitar/synth)
    pub low_mid: f32,     // 250-500 Hz (lower vocals/snare body)
    pub mid: f32,         // 500-2000 Hz (vocal/snare presence)
    pub upper_mid: f32,   // 2000-4000 Hz (harmonic presence)
    pub presence: f32,    // 4000-6000 Hz (hi-hat attack)
    pub brilliance: f32,  // 6000-20000 Hz (cymbal shimmer)

    // Aggregates (2)
    pub rms: f32,         // Overall amplitude
    pub kick: f32,        // Dedicated kick drum detection (30-120Hz spectral flux)

    // Spectral shape (6)
    pub centroid: f32,    // Brightness/timbre
    pub flux: f32,        // Spectral change rate
    pub flatness: f32,    // Noise vs tone (Wiener entropy)
    pub rolloff: f32,     // 85% energy frequency
    pub bandwidth: f32,   // Spectral spread
    pub zcr: f32,         // Zero crossing rate

    // Beat detection (5)
    pub onset: f32,       // Onset strength (continuous 0-1, for envelope effects)
    pub beat: f32,        // 1.0 on beat frame, 0.0 otherwise (trigger)
    pub beat_phase: f32,  // 0-1 sawtooth cycling at detected tempo
    pub bpm: f32,         // BPM / 300 (normalized 0-1)
    pub beat_strength: f32, // How strong the detected beat was
}

pub const NUM_FEATURES: usize = 20;

impl AudioFeatures {
    pub fn as_slice(&self) -> &[f32; NUM_FEATURES] {
        bytemuck::cast_ref(self)
    }

    pub fn as_slice_mut(&mut self) -> &mut [f32; NUM_FEATURES] {
        bytemuck::cast_mut(self)
    }
}

impl Default for AudioFeatures {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

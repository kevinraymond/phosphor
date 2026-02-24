use bytemuck::{Pod, Zeroable};

/// 12 spectral features, all normalized to 0.0-1.0 range.
/// Ported from spectral-senses/src/audio/audio_features.h
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct AudioFeatures {
    pub bass: f32,      // 0-250 Hz band energy
    pub mid: f32,       // 250-4000 Hz band energy
    pub treble: f32,    // 4000+ Hz band energy
    pub rms: f32,       // Root mean square amplitude
    pub phase: f32,     // Phase coherence
    pub onset: f32,     // Onset detection (spectral flux, boosted)
    pub centroid: f32,  // Spectral centroid (normalized)
    pub flux: f32,      // Spectral flux
    pub flatness: f32,  // Spectral flatness (noise vs tone)
    pub rolloff: f32,   // Spectral rolloff (85% energy)
    pub bandwidth: f32, // Spectral bandwidth
    pub zcr: f32,       // Zero crossing rate
}

impl AudioFeatures {
    pub fn as_slice(&self) -> &[f32; 12] {
        bytemuck::cast_ref(self)
    }

    pub fn as_slice_mut(&mut self) -> &mut [f32; 12] {
        bytemuck::cast_mut(self)
    }
}

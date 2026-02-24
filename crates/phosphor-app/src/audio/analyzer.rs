use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use super::features::AudioFeatures;

const FFT_SIZE: usize = 2048;
const SAMPLE_RATE: f32 = 44100.0;

/// FFT-based spectral feature extractor.
/// Ported from spectral-senses/src/audio/fft_analyzer.cpp
pub struct FftAnalyzer {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
    time_domain: Vec<f32>,
    fft_buffer: Vec<Complex<f32>>,
    magnitude: Vec<f32>,
    prev_magnitude: Vec<f32>,
    num_bins: usize,
    bin_hz: f32,
    sample_rate: f32,
}

impl FftAnalyzer {
    pub fn new(sample_rate: f32) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        let num_bins = FFT_SIZE / 2 + 1;
        let bin_hz = sample_rate / FFT_SIZE as f32;

        // Hann window
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
            })
            .collect();

        log::info!(
            "FFT analyzer: {FFT_SIZE}-point, {:.1} Hz/bin, {num_bins} bins",
            bin_hz
        );

        Self {
            fft,
            window,
            time_domain: vec![0.0; FFT_SIZE],
            fft_buffer: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            magnitude: vec![0.0; num_bins],
            prev_magnitude: vec![0.0; num_bins],
            num_bins,
            bin_hz,
            sample_rate,
        }
    }

    /// Feed new samples and compute features.
    pub fn analyze(&mut self, samples: &[f32]) -> AudioFeatures {
        // Shift time-domain buffer left, append new samples
        let shift = samples.len().min(FFT_SIZE);
        if shift < FFT_SIZE {
            self.time_domain.copy_within(shift.., 0);
        }
        let src_offset = if samples.len() > FFT_SIZE {
            samples.len() - FFT_SIZE
        } else {
            0
        };
        self.time_domain[FFT_SIZE - shift..].copy_from_slice(&samples[src_offset..src_offset + shift]);

        self.compute_fft();
        self.extract_features()
    }

    fn compute_fft(&mut self) {
        // Apply window and prepare complex buffer
        for i in 0..FFT_SIZE {
            self.fft_buffer[i] = Complex::new(self.time_domain[i] * self.window[i], 0.0);
        }

        self.fft.process(&mut self.fft_buffer);

        // Save previous magnitude
        std::mem::swap(&mut self.magnitude, &mut self.prev_magnitude);

        // Compute magnitude spectrum
        let scale = 2.0 / FFT_SIZE as f32;
        for i in 0..self.num_bins {
            self.magnitude[i] = self.fft_buffer[i].norm() * scale;
        }
    }

    fn extract_features(&self) -> AudioFeatures {
        let mut out = AudioFeatures::zeroed();

        // Band boundaries in bins
        let bass_hi = (250.0 / self.bin_hz) as usize;
        let mid_hi = (4000.0 / self.bin_hz) as usize;

        // Band energies with gain calibration
        out.bass = (self.band_energy(0, bass_hi) * 25.0).min(1.0);
        out.mid = (self.band_energy(bass_hi, mid_hi) * 40.0).min(1.0);
        out.treble = (self.band_energy(mid_hi, self.num_bins) * 60.0).min(1.0);

        // RMS
        let sum_sq: f32 = self.time_domain.iter().map(|s| s * s).sum();
        out.rms = ((sum_sq / FFT_SIZE as f32).sqrt() * 8.0).min(1.0);

        // Phase coherence
        let mut phase_sum = 0.0f32;
        for i in 1..self.num_bins {
            let phase = self.fft_buffer[i].im.atan2(self.fft_buffer[i].re);
            phase_sum += phase.cos().abs();
        }
        let phase_raw = phase_sum / (self.num_bins - 1) as f32;
        out.phase = ((phase_raw - 0.45) * 4.0).clamp(0.0, 1.0);

        // Spectral flux (half-wave rectified)
        let flux = self.spectral_flux();
        out.onset = (flux * 30.0).min(1.0);
        out.flux = (flux * 20.0).min(1.0);

        // Spectral centroid
        let centroid_hz = self.spectral_centroid();
        out.centroid = centroid_hz / (self.sample_rate * 0.5);

        // Spectral flatness
        out.flatness = (self.spectral_flatness() * 3.0).min(1.0);

        // Spectral rolloff
        out.rolloff = self.spectral_rolloff() / (self.sample_rate * 0.5);

        // Spectral bandwidth
        out.bandwidth = (self.spectral_bandwidth(centroid_hz) / (self.sample_rate * 0.5)).min(1.0);

        // Zero crossing rate
        out.zcr = (self.zero_crossing_rate() * 4.0).min(1.0);

        out
    }

    fn band_energy(&self, bin_low: usize, bin_high: usize) -> f32 {
        let bin_high = bin_high.min(self.num_bins);
        let count = bin_high.saturating_sub(bin_low).max(1);
        let sum: f32 = self.magnitude[bin_low..bin_high]
            .iter()
            .map(|m| m * m)
            .sum();
        (sum / count as f32).sqrt()
    }

    fn spectral_centroid(&self) -> f32 {
        let mut weighted_sum = 0.0f32;
        let mut mag_sum = 0.0f32;
        for i in 0..self.num_bins {
            let freq = i as f32 * self.bin_hz;
            weighted_sum += freq * self.magnitude[i];
            mag_sum += self.magnitude[i];
        }
        if mag_sum > 1e-10 {
            weighted_sum / mag_sum
        } else {
            0.0
        }
    }

    fn spectral_flux(&self) -> f32 {
        let mut flux = 0.0f32;
        for i in 0..self.num_bins {
            let diff = self.magnitude[i] - self.prev_magnitude[i];
            if diff > 0.0 {
                flux += diff;
            }
        }
        flux
    }

    fn spectral_flatness(&self) -> f32 {
        let mut log_sum = 0.0f64;
        let mut linear_sum = 0.0f64;
        let mut count = 0u32;
        for i in 1..self.num_bins {
            let m = self.magnitude[i] as f64;
            if m > 1e-10 {
                log_sum += m.ln();
                linear_sum += m;
                count += 1;
            }
        }
        if count == 0 || linear_sum < 1e-10 {
            return 0.0;
        }
        let geometric_mean = (log_sum / count as f64).exp();
        let arithmetic_mean = linear_sum / count as f64;
        (geometric_mean / arithmetic_mean) as f32
    }

    fn spectral_rolloff(&self) -> f32 {
        let total_energy: f32 = self.magnitude.iter().map(|m| m * m).sum();
        let threshold = total_energy * 0.85;
        let mut cumulative = 0.0f32;
        for i in 0..self.num_bins {
            cumulative += self.magnitude[i] * self.magnitude[i];
            if cumulative >= threshold {
                return i as f32 * self.bin_hz;
            }
        }
        (self.num_bins - 1) as f32 * self.bin_hz
    }

    fn spectral_bandwidth(&self, centroid_hz: f32) -> f32 {
        let mut weighted_sum = 0.0f32;
        let mut mag_sum = 0.0f32;
        for i in 0..self.num_bins {
            let freq = i as f32 * self.bin_hz;
            let diff = freq - centroid_hz;
            weighted_sum += diff * diff * self.magnitude[i];
            mag_sum += self.magnitude[i];
        }
        if mag_sum > 1e-10 {
            (weighted_sum / mag_sum).sqrt()
        } else {
            0.0
        }
    }

    fn zero_crossing_rate(&self) -> f32 {
        let mut crossings = 0u32;
        for i in 1..FFT_SIZE {
            if (self.time_domain[i] >= 0.0) != (self.time_domain[i - 1] >= 0.0) {
                crossings += 1;
            }
        }
        crossings as f32 / (FFT_SIZE - 1) as f32
    }
}

impl AudioFeatures {
    fn zeroed() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

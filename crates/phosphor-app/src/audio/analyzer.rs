use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use super::features::AudioFeatures;

/// FFT sizes for multi-resolution analysis.
const FFT_LARGE: usize = 4096;  // 10.8 Hz/bin — sub_bass, bass, kick
const FFT_MED: usize = 1024;    // 43 Hz/bin — low_mid, mid, upper_mid
const FFT_SMALL: usize = 512;   // 86 Hz/bin — presence, brilliance

/// A single FFT resolution with its own window and buffers.
struct FftResolution {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    size: usize,
    window: Vec<f32>,
    fft_buffer: Vec<Complex<f32>>,
    magnitude: Vec<f32>,
    prev_magnitude: Vec<f32>,
    num_bins: usize,
    bin_hz: f32,
}

impl FftResolution {
    fn new(planner: &mut FftPlanner<f32>, size: usize, sample_rate: f32) -> Self {
        let fft = planner.plan_fft_forward(size);
        let num_bins = size / 2 + 1;
        let bin_hz = sample_rate / size as f32;

        // Hann window
        let window: Vec<f32> = (0..size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32).cos())
            })
            .collect();

        Self {
            fft,
            size,
            window,
            fft_buffer: vec![Complex::new(0.0, 0.0); size],
            magnitude: vec![0.0; num_bins],
            prev_magnitude: vec![0.0; num_bins],
            num_bins,
            bin_hz,
        }
    }

    /// Compute FFT from the tail of the time-domain buffer.
    fn compute(&mut self, time_domain: &[f32]) {
        let td_len = time_domain.len();
        let offset = td_len.saturating_sub(self.size);
        let samples = &time_domain[offset..];
        let n = samples.len().min(self.size);

        // Apply window
        for i in 0..self.size {
            let s = if i < n { samples[i] } else { 0.0 };
            let w = self.window[i];
            self.fft_buffer[i] = Complex::new(s * w, 0.0);
        }

        self.fft.process(&mut self.fft_buffer);

        // Save previous magnitude
        std::mem::swap(&mut self.magnitude, &mut self.prev_magnitude);

        // Compute magnitude spectrum
        let scale = 2.0 / self.size as f32;
        for i in 0..self.num_bins {
            self.magnitude[i] = self.fft_buffer[i].norm() * scale;
        }
    }

    fn bin_range(&self, lo_hz: f32, hi_hz: f32) -> (usize, usize) {
        let lo = (lo_hz / self.bin_hz).round() as usize;
        let hi = ((hi_hz / self.bin_hz).round() as usize).min(self.num_bins);
        (lo, hi)
    }

    /// RMS band energy (linear).
    fn band_energy_linear(&self, lo_hz: f32, hi_hz: f32) -> f32 {
        let (lo, hi) = self.bin_range(lo_hz, hi_hz);
        let hi = hi.min(self.num_bins);
        let count = hi.saturating_sub(lo).max(1);
        let sum: f32 = self.magnitude[lo..hi].iter().map(|m| m * m).sum();
        (sum / count as f32).sqrt()
    }

    /// dB-scaled band energy over 80dB range, normalized to 0-1.
    fn band_energy_db(&self, lo_hz: f32, hi_hz: f32) -> f32 {
        let linear = self.band_energy_linear(lo_hz, hi_hz);
        if linear < 1e-10 {
            return 0.0;
        }
        let db = 20.0 * linear.log10();
        // Map -80dB..0dB → 0..1
        ((db + 80.0) / 80.0).clamp(0.0, 1.0)
    }

    /// Half-wave rectified spectral flux in a frequency range.
    fn spectral_flux_range(&self, lo_hz: f32, hi_hz: f32) -> f32 {
        let (lo, hi) = self.bin_range(lo_hz, hi_hz);
        let hi = hi.min(self.num_bins);
        let count = hi.saturating_sub(lo).max(1);
        let mut flux = 0.0f32;
        for i in lo..hi {
            let diff = self.magnitude[i] - self.prev_magnitude[i];
            if diff > 0.0 {
                flux += diff;
            }
        }
        flux / count as f32
    }
}

/// Multi-resolution FFT analyzer with 7 frequency bands and spectral features.
pub struct FftAnalyzer {
    large: FftResolution,  // 4096-pt for bass
    medium: FftResolution, // 1024-pt for mids
    small: FftResolution,  // 512-pt for highs

    time_domain: Vec<f32>, // Shared sample accumulator (FFT_LARGE length)
    sample_rate: f32,

    // Kick detection state
    prev_kick_flux: f32,
    kick_max: f32,
}

impl FftAnalyzer {
    pub fn new(sample_rate: f32) -> Self {
        let mut planner = FftPlanner::new();

        let large = FftResolution::new(&mut planner, FFT_LARGE, sample_rate);
        let medium = FftResolution::new(&mut planner, FFT_MED, sample_rate);
        let small = FftResolution::new(&mut planner, FFT_SMALL, sample_rate);

        log::info!(
            "Multi-resolution FFT: {FFT_LARGE}/{FFT_MED}/{FFT_SMALL}-pt, {:.1}/{:.1}/{:.1} Hz/bin",
            large.bin_hz, medium.bin_hz, small.bin_hz
        );

        Self {
            large,
            medium,
            small,
            time_domain: vec![0.0; FFT_LARGE],
            sample_rate,
            prev_kick_flux: 0.0,
            kick_max: 0.001,
        }
    }

    /// Feed new samples and compute all features.
    pub fn analyze(&mut self, samples: &[f32]) -> AudioFeatures {
        // Shift time-domain buffer left, append new samples
        let shift = samples.len().min(FFT_LARGE);
        if shift < FFT_LARGE {
            self.time_domain.copy_within(shift.., 0);
        }
        let src_offset = if samples.len() > FFT_LARGE {
            samples.len() - FFT_LARGE
        } else {
            0
        };
        self.time_domain[FFT_LARGE - shift..]
            .copy_from_slice(&samples[src_offset..src_offset + shift]);

        // Run all three FFTs
        self.large.compute(&self.time_domain);
        self.medium.compute(&self.time_domain);
        self.small.compute(&self.time_domain);

        self.extract_features()
    }

    /// Expose the large (4096-pt) magnitude spectrum for beat detection.
    pub fn bass_magnitude(&self) -> &[f32] {
        &self.large.magnitude
    }

    /// Expose the medium (1024-pt) magnitude spectrum for beat detection.
    pub fn mid_magnitude(&self) -> &[f32] {
        &self.medium.magnitude
    }

    /// Expose the small (512-pt) magnitude spectrum for beat detection.
    pub fn high_magnitude(&self) -> &[f32] {
        &self.small.magnitude
    }

    fn extract_features(&mut self) -> AudioFeatures {
        let mut out = AudioFeatures::default();

        // 7-band energy extraction:
        // Bass bands (linear RMS) — from large FFT
        out.sub_bass = self.large.band_energy_linear(20.0, 60.0);
        out.bass = self.large.band_energy_linear(60.0, 250.0);

        // Mid bands (linear) — from medium FFT
        out.low_mid = self.medium.band_energy_linear(250.0, 500.0);
        out.mid = self.medium.band_energy_linear(500.0, 2000.0);
        out.upper_mid = self.medium.band_energy_db(2000.0, 4000.0);

        // High bands (dB-scaled) — from small FFT
        out.presence = self.small.band_energy_db(4000.0, 6000.0);
        out.brilliance = self.small.band_energy_db(6000.0, 20000.0);

        // RMS from time domain (use last 2048 samples for reasonable window)
        let td_start = FFT_LARGE - 2048;
        let sum_sq: f32 = self.time_domain[td_start..].iter().map(|s| s * s).sum();
        out.rms = (sum_sq / 2048.0).sqrt();

        // Kick detection: half-wave rectified spectral flux in 30-120 Hz (from large FFT)
        let kick_flux = self.large.spectral_flux_range(30.0, 120.0);
        // Normalize by running max with decay
        self.kick_max = (self.kick_max * 0.999).max(kick_flux).max(0.001);
        out.kick = (kick_flux / self.kick_max).clamp(0.0, 1.0);
        self.prev_kick_flux = kick_flux;

        // Spectral features (from large FFT for best frequency resolution)
        let centroid_hz = self.spectral_centroid();
        out.centroid = centroid_hz / (self.sample_rate * 0.5);

        out.flux = self.spectral_flux();

        out.flatness = self.spectral_flatness();

        out.rolloff = self.spectral_rolloff() / (self.sample_rate * 0.5);

        out.bandwidth = (self.spectral_bandwidth(centroid_hz) / (self.sample_rate * 0.5)).min(1.0);

        out.zcr = self.zero_crossing_rate();

        // Beat fields left at 0.0 — filled by beat detector in audio thread
        out
    }

    fn spectral_centroid(&self) -> f32 {
        let mag = &self.large.magnitude;
        let bin_hz = self.large.bin_hz;
        let mut weighted_sum = 0.0f32;
        let mut mag_sum = 0.0f32;
        for (i, &m) in mag.iter().enumerate() {
            let freq = i as f32 * bin_hz;
            weighted_sum += freq * m;
            mag_sum += m;
        }
        if mag_sum > 1e-10 {
            weighted_sum / mag_sum
        } else {
            0.0
        }
    }

    fn spectral_flux(&self) -> f32 {
        let mag = &self.large.magnitude;
        let prev = &self.large.prev_magnitude;
        let mut flux = 0.0f32;
        for i in 0..mag.len() {
            let diff = mag[i] - prev[i];
            if diff > 0.0 {
                flux += diff;
            }
        }
        flux
    }

    fn spectral_flatness(&self) -> f32 {
        let mag = &self.large.magnitude;
        let mut log_sum = 0.0f64;
        let mut linear_sum = 0.0f64;
        let mut count = 0u32;
        for &m in &mag[1..] {
            let m = m as f64;
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
        let mag = &self.large.magnitude;
        let bin_hz = self.large.bin_hz;
        let total_energy: f32 = mag.iter().map(|m| m * m).sum();
        let threshold = total_energy * 0.85;
        let mut cumulative = 0.0f32;
        for (i, &m) in mag.iter().enumerate() {
            cumulative += m * m;
            if cumulative >= threshold {
                return i as f32 * bin_hz;
            }
        }
        (mag.len() - 1) as f32 * bin_hz
    }

    fn spectral_bandwidth(&self, centroid_hz: f32) -> f32 {
        let mag = &self.large.magnitude;
        let bin_hz = self.large.bin_hz;
        let mut weighted_sum = 0.0f32;
        let mut mag_sum = 0.0f32;
        for (i, &m) in mag.iter().enumerate() {
            let freq = i as f32 * bin_hz;
            let diff = freq - centroid_hz;
            weighted_sum += diff * diff * m;
            mag_sum += m;
        }
        if mag_sum > 1e-10 {
            (weighted_sum / mag_sum).sqrt()
        } else {
            0.0
        }
    }

    fn zero_crossing_rate(&self) -> f32 {
        let td_start = FFT_LARGE - 2048;
        let td = &self.time_domain[td_start..];
        let mut crossings = 0u32;
        for i in 1..td.len() {
            if (td[i] >= 0.0) != (td[i - 1] >= 0.0) {
                crossings += 1;
            }
        }
        crossings as f32 / (td.len() - 1) as f32
    }
}

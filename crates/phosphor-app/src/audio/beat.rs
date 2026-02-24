//! 3-stage beat detection pipeline: OnsetDetector → TempoEstimator → BeatScheduler.
//! Ported from easey-glyph's Python implementation.

// ---------------------------------------------------------------------------
// Circular buffer (fixed-size ring buffer with statistical methods)
// ---------------------------------------------------------------------------

struct CircularBuffer {
    buf: Vec<f64>,
    cap: usize,
    write: usize,
    count: usize,
}

impl CircularBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity],
            cap: capacity,
            write: 0,
            count: 0,
        }
    }

    fn push(&mut self, value: f64) {
        self.buf[self.write] = value;
        self.write = (self.write + 1) % self.cap;
        if self.count < self.cap {
            self.count += 1;
        }
    }

    fn len(&self) -> usize {
        self.count
    }

    fn values(&self) -> Vec<f64> {
        if self.count == 0 {
            return Vec::new();
        }
        if self.count < self.cap {
            self.buf[..self.count].to_vec()
        } else {
            let start = self.write;
            let mut v = Vec::with_capacity(self.cap);
            v.extend_from_slice(&self.buf[start..]);
            v.extend_from_slice(&self.buf[..start]);
            v
        }
    }

    fn median(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let mut vals = self.values();
        vals.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = vals.len() / 2;
        if vals.len() % 2 == 0 {
            (vals[mid - 1] + vals[mid]) / 2.0
        } else {
            vals[mid]
        }
    }

    fn mad(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let med = self.median();
        let mut abs_devs: Vec<f64> = self.values().iter().map(|v| (v - med).abs()).collect();
        abs_devs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = abs_devs.len() / 2;
        if abs_devs.len() % 2 == 0 {
            (abs_devs[mid - 1] + abs_devs[mid]) / 2.0
        } else {
            abs_devs[mid]
        }
    }

    fn max(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        if self.count < self.cap {
            self.buf[..self.count]
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
        } else {
            self.buf
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max)
        }
    }

    fn mean(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        if self.count < self.cap {
            self.buf[..self.count].iter().sum::<f64>() / self.count as f64
        } else {
            self.buf.iter().sum::<f64>() / self.cap as f64
        }
    }
}

// ---------------------------------------------------------------------------
// Stage 1: Multi-band onset detection
// ---------------------------------------------------------------------------

/// Frequency band definition: (lo_hz, hi_hz, weight)
const ONSET_BANDS: [(f32, f32, f32); 4] = [
    (20.0, 80.0, 0.4),     // sub-bass (kick drums)
    (80.0, 250.0, 0.3),    // bass (bass guitar/synth)
    (500.0, 2000.0, 0.2),  // mid (snares/vocals)
    (2000.0, 4000.0, 0.1), // high-mid (hi-hats/cymbals)
];

struct OnsetDetector {
    sample_rate: f32,
    threshold_mult: f32,
    silence_threshold: f32,
    threshold_ceiling: f32,

    prev_mags: [Option<Vec<f64>>; 4],
    onset_history: CircularBuffer,
    long_term_history: CircularBuffer,
    silent_frames: u32,
}

impl OnsetDetector {
    fn new(sample_rate: f32, history_size: usize, long_term_size: usize) -> Self {
        Self {
            sample_rate,
            threshold_mult: 2.0,
            silence_threshold: 0.002,
            threshold_ceiling: 0.5,
            prev_mags: [None, None, None, None],
            onset_history: CircularBuffer::new(history_size),
            long_term_history: CircularBuffer::new(long_term_size),
            silent_frames: 0,
        }
    }

    fn bin_range(&self, lo_hz: f32, hi_hz: f32, fft_size: usize) -> (usize, usize) {
        let bin_width = self.sample_rate / fft_size as f32;
        let lo_bin = (lo_hz / bin_width).round().max(0.0) as usize;
        let hi_bin = ((hi_hz / bin_width).round() as usize).min(fft_size / 2);
        (lo_bin, hi_bin)
    }

    /// Process multi-resolution spectra and return (is_onset, onset_strength, combined_flux).
    fn process(
        &mut self,
        bass_spectrum: &[f32],  // 4096-pt FFT magnitudes (num_bins)
        mid_spectrum: &[f32],   // 1024-pt FFT magnitudes
        high_spectrum: &[f32],  // 512-pt FFT magnitudes
        rms: f32,
    ) -> (bool, f32, f64) {
        // Silence gate
        if rms < self.silence_threshold {
            self.silent_frames += 1;
            return (false, 0.0, 0.0);
        }
        self.silent_frames = 0;

        // Map bands to spectra: sub-bass & bass → 4096, mid → 1024, high-mid → 512
        let spectra: [&[f32]; 4] = [bass_spectrum, bass_spectrum, mid_spectrum, high_spectrum];
        // Reconstruct fft_size from num_bins: num_bins = fft_size/2 + 1
        let fft_sizes: [usize; 4] = [
            (bass_spectrum.len() - 1) * 2,
            (bass_spectrum.len() - 1) * 2,
            (mid_spectrum.len() - 1) * 2,
            (high_spectrum.len() - 1) * 2,
        ];

        let mut band_flux = [0.0f64; 4];

        for (i, &(lo_hz, hi_hz, _weight)) in ONSET_BANDS.iter().enumerate() {
            let spectrum = spectra[i];
            let fft_size = fft_sizes[i];
            let (lo_bin, hi_bin) = self.bin_range(lo_hz, hi_hz, fft_size);

            if hi_bin <= lo_bin || hi_bin > spectrum.len() {
                continue;
            }

            let current_mags: Vec<f64> = spectrum[lo_bin..hi_bin]
                .iter()
                .map(|&m| m as f64)
                .collect();

            if let Some(ref prev) = self.prev_mags[i] {
                if prev.len() == current_mags.len() {
                    let count = current_mags.len().max(1) as f64;
                    let flux: f64 = current_mags
                        .iter()
                        .zip(prev.iter())
                        .map(|(&c, &p)| (c - p).max(0.0))
                        .sum();
                    band_flux[i] = flux / count;
                }
            }

            self.prev_mags[i] = Some(current_mags);
        }

        // Weighted combination
        let weight_sum: f32 = ONSET_BANDS.iter().map(|b| b.2).sum();
        let combined_flux: f64 = band_flux
            .iter()
            .zip(ONSET_BANDS.iter())
            .map(|(&flux, &(_, _, w))| flux * w as f64)
            .sum::<f64>()
            / weight_sum as f64;

        self.onset_history.push(combined_flux);
        self.long_term_history.push(combined_flux);

        // Adaptive threshold: median + k * MAD
        let threshold = self.compute_threshold();
        let is_onset = combined_flux > threshold;

        // Normalize onset strength to 0-1
        let recent_max = self.long_term_history.max();
        let onset_strength = (combined_flux / recent_max.max(1e-6)).min(1.0) as f32;

        (is_onset, onset_strength, combined_flux)
    }

    fn compute_threshold(&self) -> f64 {
        let median = self.onset_history.median();
        let mad = self.onset_history.mad();
        let base_threshold = median + self.threshold_mult as f64 * mad;

        let min_threshold = 0.001;

        // Cap at proportion of long-term max
        let mut max_threshold = f64::INFINITY;
        if self.long_term_history.len() > self.onset_history.len() {
            let lt_max = self.long_term_history.max();
            max_threshold = lt_max * self.threshold_ceiling as f64;
        }

        // Also cap at 80% of recent max
        let recent_max = self.onset_history.max();
        let recent_ceiling = recent_max * 0.8;

        let capped = base_threshold.min(max_threshold).min(recent_ceiling);
        capped.max(min_threshold)
    }

    fn is_sustained_silence(&self) -> bool {
        self.silent_frames >= 30
    }
}

// ---------------------------------------------------------------------------
// Stage 2: Autocorrelation-based tempo estimation
// ---------------------------------------------------------------------------

struct TempoEstimator {
    bpm_range: (f32, f32),
    smoothing: f32,

    onset_history: CircularBuffer,
    frame_time_history: CircularBuffer,
    frame_rate: f64,
    frame_time: f64,
    initial_frame_time: f64,
    last_time: f64,
    frame_count: u32,

    current_bpm: f64,
    current_confidence: f64,
    current_period_frames: f64,

    stable_bpm: f64,
    stable_period_frames: f64,
    stability_counter: u32,
}

impl TempoEstimator {
    fn new(history_seconds: f64, frame_rate: f64) -> Self {
        let history_size = (history_seconds * frame_rate).ceil() as usize;
        let frame_time = 1.0 / frame_rate;

        Self {
            bpm_range: (40.0, 300.0),
            smoothing: 0.15,
            onset_history: CircularBuffer::new(history_size),
            frame_time_history: CircularBuffer::new(30),
            frame_rate,
            frame_time,
            initial_frame_time: frame_time,
            last_time: 0.0,
            frame_count: 0,
            current_bpm: 0.0,
            current_confidence: 0.0,
            current_period_frames: 0.0,
            stable_bpm: 0.0,
            stable_period_frames: 0.0,
            stability_counter: 0,
        }
    }

    /// Update tempo estimate. Returns (bpm, confidence, period_seconds).
    fn update(&mut self, onset_value: f64, timestamp: f64) -> (f64, f64, f64) {
        // Track frame timing
        if self.last_time > 0.0 {
            let dt = timestamp - self.last_time;
            if dt > 0.0 && dt < 0.1 {
                self.frame_time_history.push(dt);
                if self.frame_time_history.len() >= 10 {
                    let measured = self.frame_time_history.mean();
                    let lo = self.initial_frame_time * 0.85;
                    let hi = self.initial_frame_time * 1.15;
                    self.frame_time = measured.clamp(lo, hi);
                    self.frame_rate = 1.0 / self.frame_time;
                }
            }
        }
        self.last_time = timestamp;

        self.onset_history.push(onset_value);
        self.frame_count += 1;

        // Need enough history (at least 2s)
        let min_frames = (2.0 * self.frame_rate).ceil() as usize;
        if self.onset_history.len() < min_frames {
            return (0.0, 0.0, 0.0);
        }

        // Compute autocorrelation every ~6 frames (~10Hz update rate)
        if self.frame_count % 6 != 0 {
            let period_s = self.current_period_frames * self.frame_time;
            return (self.current_bpm, self.current_confidence, period_s);
        }

        let (raw_bpm, confidence, period_frames) = self.compute_tempo();

        // Smooth BPM
        if self.current_bpm > 0.0 && raw_bpm > 0.0 {
            self.current_bpm = self.smooth_bpm(self.current_bpm, raw_bpm, confidence);
        } else if raw_bpm > 0.0 {
            self.current_bpm = raw_bpm;
        }

        self.current_confidence = confidence;
        self.current_period_frames = period_frames;

        // Track stability
        if confidence > 0.5 && self.stable_bpm > 0.0 {
            let bpm_diff = (self.current_bpm - self.stable_bpm).abs() / self.stable_bpm;
            if bpm_diff < 0.08 {
                self.stability_counter += 1;
            } else if bpm_diff > 0.3 && self.stability_counter < 60 {
                self.stable_bpm = self.current_bpm;
                self.stable_period_frames = period_frames;
                self.stability_counter = 0;
            }
        } else if confidence > 0.5 && self.current_bpm > 0.0 {
            self.stable_bpm = self.current_bpm;
            self.stable_period_frames = period_frames;
            self.stability_counter = 1;
        }

        // Use stable tempo when current is erratic
        let is_jumping = self.stable_bpm > 0.0
            && (self.current_bpm - self.stable_bpm).abs() / self.stable_bpm.max(1.0) > 0.15;
        if (confidence < 0.5 || is_jumping)
            && self.stable_bpm > 0.0
            && self.stability_counter > 60
        {
            self.current_bpm = self.stable_bpm;
            self.current_period_frames = self.stable_period_frames;
        }

        let period_s = self.current_period_frames * self.frame_time;
        (self.current_bpm, self.current_confidence, period_s)
    }

    fn compute_tempo(&self) -> (f64, f64, f64) {
        let history = self.onset_history.values();
        let n = history.len();

        // Convert BPM range to lag range (frames)
        let max_lag = ((60.0 / (self.bpm_range.0 as f64 * self.frame_time)) as usize).min(n / 2);
        let min_lag = ((60.0 / (self.bpm_range.1 as f64 * self.frame_time)) as usize).max(1);

        if max_lag <= min_lag {
            return (0.0, 0.0, 0.0);
        }

        // Autocorrelation
        let mut autocorr = vec![0.0f64; max_lag + 1];
        let energy: f64 = history.iter().map(|v| v * v).sum();
        autocorr[0] = energy;

        for lag in min_lag..=max_lag {
            let mut sum = 0.0f64;
            for j in 0..(n - lag) {
                sum += history[j] * history[j + lag];
            }
            autocorr[lag] = sum;
        }

        // Harmonic enhancement
        let mut enhanced = vec![0.0f64; max_lag + 1];
        for lag in min_lag..=max_lag {
            let mut val = autocorr[lag];
            if lag * 2 <= max_lag {
                val += 0.5 * autocorr[lag * 2];
            }
            if lag * 3 <= max_lag {
                val += 0.33 * autocorr[lag * 3];
            }
            if lag * 4 <= max_lag {
                val += 0.25 * autocorr[lag * 4];
            }
            enhanced[lag] = val;
        }

        // Find peak
        let mut best_lag = min_lag;
        let mut best_value = enhanced[min_lag];
        for lag in (min_lag + 1)..=max_lag {
            if enhanced[lag] > best_value {
                best_value = enhanced[lag];
                best_lag = lag;
            }
        }

        // Octave correction: check one octave down (2x lag) only.
        // Only accept if the half-tempo peak is nearly as strong (≥95%).
        // Limited to 1 step to prevent cascading (e.g. 230→115→58).
        if best_lag * 2 <= max_lag {
            let center = best_lag * 2;
            let search_lo = min_lag.max(center.saturating_sub(2));
            let search_hi = max_lag.min(center + 2);

            let mut best_candidate: Option<usize> = None;
            let mut best_candidate_val = 0.0f64;

            for dl in search_lo..=search_hi {
                let dv = enhanced[dl];
                // Must be a local peak
                if dl > min_lag && dv < enhanced[dl - 1] {
                    continue;
                }
                if dl < max_lag && dv < enhanced[dl + 1] {
                    continue;
                }
                if dv > best_candidate_val {
                    best_candidate = Some(dl);
                    best_candidate_val = dv;
                }
            }

            if let Some(candidate) = best_candidate {
                if best_candidate_val >= best_value * 0.95 {
                    best_lag = candidate;
                    best_value = best_candidate_val;
                }
            }
        }

        // Parabolic interpolation for sub-frame precision
        let mut refined_lag = best_lag as f64;
        if best_lag > min_lag && best_lag < max_lag {
            let alpha = enhanced[best_lag - 1];
            let beta = enhanced[best_lag];
            let gamma = enhanced[best_lag + 1];
            let denom = 2.0 * (2.0 * beta - alpha - gamma);
            if denom.abs() > 1e-10 {
                let p = (alpha - gamma) / denom;
                refined_lag = best_lag as f64 + p.clamp(-0.5, 0.5);
            }
        }

        // Convert to BPM
        let period_s = refined_lag * self.frame_time;
        let bpm = if period_s > 0.0 {
            60.0 / period_s
        } else {
            0.0
        };

        // Confidence
        let raw_conf = if energy > 0.0 {
            (best_value / energy).min(1.0)
        } else {
            0.0
        };
        let confidence = if raw_conf > 0.05 {
            raw_conf.max(0.15)
        } else {
            raw_conf
        };

        if bpm < self.bpm_range.0 as f64 || bpm > self.bpm_range.1 as f64 {
            return (0.0, 0.0, 0.0);
        }

        (bpm, confidence, refined_lag)
    }

    fn smooth_bpm(&self, current: f64, new: f64, confidence: f64) -> f64 {
        let ratio = if current > 0.0 { new / current } else { 1.0 };
        let is_half = (ratio - 0.5).abs() < 0.15;
        let is_double = (ratio - 2.0).abs() < 0.15;
        let change = (new - current).abs() / current.max(1.0);

        // Octave shifts
        if is_half || is_double {
            if confidence >= 0.7 {
                return current + 0.5 * (new - current);
            } else {
                return current;
            }
        }

        if change > 0.25 && confidence < 0.7 {
            return current;
        }

        let mut effective = self.smoothing as f64;
        if change > 0.1 && confidence < 0.5 {
            effective *= 0.3;
        } else if confidence > 0.7 && change < 0.05 {
            effective *= 1.5;
        }

        current + effective * (new - current)
    }
}

// ---------------------------------------------------------------------------
// Stage 3: Beat scheduler (prediction + confirmation)
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
enum BeatState {
    Waiting,
    Expecting,
    Confirmed,
    Missed,
}

struct BeatScheduler {
    beat_window: f64,
    refractory: f64,
    min_confidence: f64,
    phase_correction: f64,
    max_misses: u32,
    beat_timeout: f64,

    state: BeatState,
    last_beat_time: f64,
    next_predicted: f64,
    phase: f64,

    bpm: f64,
    period: f64,
    tempo_confidence: f64,

    beat_strength: f64,
    tracking_confidence: f64,
    consecutive_misses: u32,
    last_fired_time: f64,
    stored_period: f64,
}

impl BeatScheduler {
    fn new() -> Self {
        Self {
            beat_window: 0.08,
            refractory: 0.15,
            min_confidence: 0.4,
            phase_correction: 0.3,
            max_misses: 4,
            beat_timeout: 3.0,

            state: BeatState::Waiting,
            last_beat_time: 0.0,
            next_predicted: 0.0,
            phase: 0.0,

            bpm: 0.0,
            period: 0.0,
            tempo_confidence: 0.0,

            beat_strength: 0.0,
            tracking_confidence: 0.0,
            consecutive_misses: 0,
            last_fired_time: 0.0,
            stored_period: 0.0,
        }
    }

    fn update_tempo(&mut self, bpm: f64, period: f64, confidence: f64) {
        self.bpm = bpm;
        self.period = period;
        self.tempo_confidence = confidence;
        if confidence >= self.min_confidence && period > 0.0 {
            self.stored_period = period;
        }
    }

    /// Main beat decision. Returns (is_beat, beat_phase, smoothed_bpm).
    fn process(
        &mut self,
        is_onset: bool,
        onset_strength: f32,
        timestamp: f64,
        is_silence: bool,
    ) -> (bool, f64, f64) {
        let time_since_beat = if self.last_beat_time > 0.0 {
            timestamp - self.last_beat_time
        } else {
            f64::INFINITY
        };
        let in_refractory = time_since_beat < self.refractory;

        if is_onset {
            self.beat_strength = onset_strength as f64;
        }

        // Update phase and count missed predicted beats
        let missed = if self.period > 0.0 {
            self.update_phase(timestamp)
        } else {
            0
        };

        // Beat timeout recovery
        let time_since_fired = if self.last_fired_time > 0.0 {
            timestamp - self.last_fired_time
        } else {
            0.0
        };
        if time_since_fired > self.beat_timeout && self.last_fired_time > 0.0 {
            self.tracking_confidence *= 0.3;
            self.consecutive_misses = 0;
            self.state = BeatState::Waiting;
            self.last_fired_time = timestamp;
        }

        // Silence → pause beat detection
        if is_silence {
            if self.state != BeatState::Waiting {
                self.state = BeatState::Waiting;
                self.consecutive_misses = 0;
            }
            return (false, 0.0, self.bpm);
        }

        // Determine if we should fire a beat
        let mut is_beat = false;

        if missed > 0 && self.tempo_confidence >= self.min_confidence {
            is_beat = true;
            self.beat_strength = 0.5;
            self.state = BeatState::Missed;
        } else if self.tempo_confidence < self.min_confidence || self.period == 0.0 {
            // Low confidence: onset-only mode
            is_beat = self.onset_only(is_onset, in_refractory);

            // Backup prediction
            if !is_beat && self.stored_period > 0.0 && self.last_fired_time > 0.0 {
                if time_since_fired >= self.stored_period * 0.9 && !in_refractory {
                    is_beat = true;
                    self.beat_strength = 0.5;
                }
            }
        } else {
            // High confidence: predictive mode
            is_beat = self.predictive(is_onset, timestamp, in_refractory);
        }

        if is_beat {
            self.last_beat_time = timestamp;
            self.last_fired_time = timestamp;
            self.phase = 0.0;
            self.consecutive_misses = 0;
            if self.period > 0.0 {
                self.next_predicted = timestamp + self.period;
            }
            self.tracking_confidence = (self.tracking_confidence + 0.15).min(1.0);
        } else {
            self.tracking_confidence = (self.tracking_confidence - 0.0005).max(0.0);
        }

        (is_beat, self.phase, self.bpm)
    }

    fn onset_only(&mut self, is_onset: bool, in_refractory: bool) -> bool {
        self.state = BeatState::Waiting;
        if is_onset && !in_refractory {
            self.state = BeatState::Confirmed;
            return true;
        }
        false
    }

    fn predictive(&mut self, is_onset: bool, timestamp: f64, in_refractory: bool) -> bool {
        let dist = if self.next_predicted > 0.0 {
            timestamp - self.next_predicted
        } else {
            f64::INFINITY
        };
        let in_window = dist.abs() <= self.beat_window;
        let window_expired = dist > self.beat_window;

        match self.state {
            BeatState::Waiting => {
                if self.next_predicted == 0.0 {
                    if is_onset && !in_refractory {
                        self.next_predicted = timestamp + self.period;
                        self.state = BeatState::Confirmed;
                        return true;
                    }
                } else {
                    self.state = BeatState::Expecting;
                }
                false
            }
            BeatState::Expecting => {
                if is_onset && in_window && !in_refractory {
                    self.state = BeatState::Confirmed;
                    if self.period > 0.0 {
                        let phase_error = dist / self.period;
                        self.apply_phase_correction(phase_error);
                    }
                    return true;
                } else if window_expired {
                    self.state = BeatState::Missed;
                    return true;
                }
                false
            }
            BeatState::Confirmed | BeatState::Missed => {
                self.state = BeatState::Expecting;
                if is_onset && in_window && !in_refractory {
                    self.state = BeatState::Confirmed;
                    return true;
                }
                false
            }
        }
    }

    fn update_phase(&mut self, timestamp: f64) -> u32 {
        if self.last_beat_time == 0.0 || self.period == 0.0 {
            return 0;
        }

        let elapsed = timestamp - self.last_beat_time;
        self.phase = (elapsed / self.period) % 1.0;

        let mut missed = 0u32;
        while self.next_predicted > 0.0
            && timestamp > self.next_predicted + self.beat_window
        {
            self.consecutive_misses += 1;
            missed += 1;
            self.next_predicted += self.period;
            if self.consecutive_misses == self.max_misses {
                self.tracking_confidence *= 0.7;
            }
        }
        missed
    }

    fn apply_phase_correction(&mut self, phase_error: f64) {
        let clamped = phase_error.clamp(-0.5, 0.5);
        let correction = clamped * self.period * self.phase_correction;
        self.next_predicted += correction;
    }
}

// ---------------------------------------------------------------------------
// Main BeatDetector facade
// ---------------------------------------------------------------------------

/// Result from beat detection for one frame.
pub struct BeatResult {
    pub onset_strength: f32,
    pub beat: f32,
    pub beat_phase: f32,
    pub bpm: f32,
    pub beat_strength: f32,
}

/// 3-stage beat detection pipeline.
pub struct BeatDetector {
    onset_detector: OnsetDetector,
    tempo_estimator: TempoEstimator,
    beat_scheduler: BeatScheduler,

    // Onset hold+decay
    held_onset: f32,
    onset_decay_tau: f32,
    last_timestamp: f64,

    // Onset cooldown
    onset_cooldown: f64,
    last_onset_time: f64,
}

impl BeatDetector {
    pub fn new(sample_rate: f32) -> Self {
        // Approximate frame rate (~100 Hz for 10ms audio loop)
        let frame_rate = 100.0;

        let history_size = (0.5 * frame_rate) as usize; // ~0.5s
        let long_term_size = (4.0 * frame_rate) as usize; // ~4s

        Self {
            onset_detector: OnsetDetector::new(sample_rate, history_size, long_term_size),
            tempo_estimator: TempoEstimator::new(4.0, frame_rate),
            beat_scheduler: BeatScheduler::new(),
            held_onset: 0.0,
            onset_decay_tau: 0.20,
            last_timestamp: 0.0,
            onset_cooldown: 0.05,
            last_onset_time: 0.0,
        }
    }

    /// Process one frame of audio data.
    ///
    /// Arguments:
    /// - bass_spectrum: magnitude spectrum from 4096-pt FFT (num_bins)
    /// - mid_spectrum: magnitude spectrum from 1024-pt FFT
    /// - high_spectrum: magnitude spectrum from 512-pt FFT
    /// - rms: current RMS energy
    /// - timestamp: current time in seconds
    pub fn process(
        &mut self,
        bass_spectrum: &[f32],
        mid_spectrum: &[f32],
        high_spectrum: &[f32],
        rms: f32,
        timestamp: f64,
    ) -> BeatResult {
        let dt = if self.last_timestamp > 0.0 {
            (timestamp - self.last_timestamp).max(0.0)
        } else {
            0.0
        };
        self.last_timestamp = timestamp;

        // Stage 1: Onset detection
        let (is_onset, onset_strength, combined_flux) = self.onset_detector.process(
            bass_spectrum,
            mid_spectrum,
            high_spectrum,
            rms,
        );

        // Apply onset cooldown
        let mut onset_gated = is_onset;
        if is_onset && (timestamp - self.last_onset_time) < self.onset_cooldown {
            onset_gated = false;
        }
        if onset_gated {
            self.last_onset_time = timestamp;
        }

        // Stage 2: Tempo estimation
        let (bpm, confidence, period_s) =
            self.tempo_estimator.update(combined_flux, timestamp);

        // Stage 3: Beat scheduling
        self.beat_scheduler.update_tempo(bpm, period_s, confidence);
        let (is_beat, beat_phase, smoothed_bpm) = self.beat_scheduler.process(
            onset_gated,
            onset_strength,
            timestamp,
            self.onset_detector.is_sustained_silence(),
        );

        // Onset hold+decay (instant attack, exponential release)
        if onset_strength > self.held_onset {
            self.held_onset = onset_strength;
        } else if dt > 0.0 {
            self.held_onset *= (-dt as f32 / self.onset_decay_tau).exp();
        }

        // Freeze phase at 0 during silence
        let phase = if rms < 1e-4 { 0.0 } else { beat_phase as f32 };

        BeatResult {
            onset_strength: self.held_onset,
            beat: if is_beat { 1.0 } else { 0.0 },
            beat_phase: phase,
            bpm: if smoothed_bpm > 0.0 {
                smoothed_bpm as f32
            } else {
                0.0
            },
            beat_strength: if is_beat {
                self.beat_scheduler.beat_strength as f32
            } else {
                0.0
            },
        }
    }
}

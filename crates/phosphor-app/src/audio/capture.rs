use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

/// Ring buffer size (power of 2 for fast modular arithmetic).
const RING_SIZE: usize = 65536;
const RING_MASK: u32 = (RING_SIZE - 1) as u32;

/// Lock-free single-producer single-consumer ring buffer for audio samples.
pub struct RingBuffer {
    data: Box<[f32; RING_SIZE]>,
    write_pos: AtomicU32,
    read_pos: AtomicU32,
}

impl RingBuffer {
    pub fn new() -> Self {
        Self {
            data: Box::new([0.0; RING_SIZE]),
            write_pos: AtomicU32::new(0),
            read_pos: AtomicU32::new(0),
        }
    }

    /// Push samples (called from cpal callback thread).
    /// Safety: Only one thread should call push at a time (the cpal callback).
    pub fn push(&self, samples: &[f32]) {
        let mut wp = self.write_pos.load(Ordering::Relaxed);
        for &sample in samples {
            // Safety: we're the only writer, and RING_SIZE is a power of 2
            let idx = (wp & RING_MASK) as usize;
            // This is safe because we're the only writer and readers
            // can tolerate stale data gracefully.
            unsafe {
                let ptr = self.data.as_ptr() as *mut f32;
                *ptr.add(idx) = sample;
            }
            wp = wp.wrapping_add(1);
        }
        self.write_pos.store(wp, Ordering::Release);
    }

    /// Read available samples into dst. Returns number of samples read.
    pub fn read(&self, dst: &mut [f32]) -> usize {
        let wp = self.write_pos.load(Ordering::Acquire);
        let rp = self.read_pos.load(Ordering::Relaxed);
        let available = wp.wrapping_sub(rp) as usize;
        let to_read = available.min(dst.len());

        for i in 0..to_read {
            let idx = (rp.wrapping_add(i as u32) & RING_MASK) as usize;
            dst[i] = self.data[idx];
        }

        self.read_pos
            .store(rp.wrapping_add(to_read as u32), Ordering::Release);
        to_read
    }

    /// Number of samples available to read.
    pub fn available(&self) -> usize {
        let wp = self.write_pos.load(Ordering::Acquire);
        let rp = self.read_pos.load(Ordering::Relaxed);
        wp.wrapping_sub(rp) as usize
    }
}

// Safety: RingBuffer uses atomics for synchronization
unsafe impl Send for RingBuffer {}
unsafe impl Sync for RingBuffer {}

pub struct AudioCapture {
    _stream: Stream,
    pub ring: Arc<RingBuffer>,
    pub sample_rate: u32,
    pub device_name: String,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No audio input device found"))?;

        let device_name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Unknown".into());
        log::info!("Audio capture device: {device_name}");

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate();
        let channels = config.channels() as usize;
        log::info!("Audio config: {sample_rate}Hz, {channels}ch, {:?}", config.sample_format());

        let ring = Arc::new(RingBuffer::new());
        let ring_clone = ring.clone();

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Downmix to mono
                if channels == 1 {
                    ring_clone.push(data);
                } else {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    ring_clone.push(&mono);
                }
            },
            |err| {
                log::error!("Audio stream error: {err}");
            },
            None,
        )?;

        stream.play()?;
        log::info!("Audio capture started");

        Ok(Self {
            _stream: stream,
            ring,
            sample_rate,
            device_name,
        })
    }

    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

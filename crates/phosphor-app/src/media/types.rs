/// A decoded frame ready for GPU upload.
pub struct DecodedFrame {
    pub data: Vec<u8>,   // RGBA8
    pub width: u32,
    pub height: u32,
}

/// Playback direction for media layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayDirection {
    Forward,
    Reverse,
    PingPong,
}

/// Transport state for media playback.
#[derive(Debug, Clone)]
pub struct TransportState {
    pub playing: bool,
    pub looping: bool,
    pub speed: f32,
    pub direction: PlayDirection,
    /// Current position in seconds (for video) or frame index (for GIF).
    pub position: f64,
    /// Total duration in seconds (for video) or total frames (for GIF as f64).
    pub duration: f64,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            playing: true,
            looping: true,
            speed: 1.0,
            direction: PlayDirection::Forward,
            position: 0.0,
            duration: 0.0,
        }
    }
}

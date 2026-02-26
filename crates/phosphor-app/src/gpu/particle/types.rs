use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use super::emitter::EmitterDef;

/// GPU particle: 64 bytes (4 x vec4f).
/// pos_life: xy = position (screen-space -1..1), z = reserved, w = life (1.0 = just born, 0.0 = dead)
/// vel_size: xy = velocity, z = reserved, w = size
/// color: rgba
/// flags: x = age (0..lifetime), y = lifetime, z = emitter_id, w = reserved
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Particle {
    pub pos_life: [f32; 4],
    pub vel_size: [f32; 4],
    pub color: [f32; 4],
    pub flags: [f32; 4],
}

/// Particle simulation uniforms: 128 bytes.
/// Separate from the main 256-byte ShaderUniforms.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ParticleUniforms {
    // Simulation params (16 bytes)
    pub delta_time: f32,
    pub time: f32,
    pub max_particles: u32,
    pub emit_count: u32,

    // Emitter config (16 bytes)
    pub emitter_pos: [f32; 2],
    pub emitter_radius: f32,
    pub emitter_shape: u32, // 0=point, 1=ring, 2=line, 3=screen

    // Particle defaults (16 bytes)
    pub lifetime: f32,
    pub initial_speed: f32,
    pub initial_size: f32,
    pub size_end: f32,

    // Forces (16 bytes)
    pub gravity: [f32; 2],
    pub drag: f32,
    pub turbulence: f32,

    // Attraction (16 bytes)
    pub attraction_point: [f32; 2],
    pub attraction_strength: f32,
    pub seed: f32,

    // Audio features (40 bytes = 10 floats)
    pub sub_bass: f32,
    pub bass: f32,
    pub mid: f32,
    pub rms: f32,
    pub kick: f32,
    pub onset: f32,
    pub centroid: f32,
    pub flux: f32,
    pub beat: f32,
    pub beat_phase: f32,

    // Resolution (8 bytes) — needed for aspect ratio correction in orbital mechanics
    pub resolution: [f32; 2],
    // Total: 32 fields = 128 bytes (no padding needed)
}

/// Auxiliary particle data: 16 bytes (1 x vec4f).
/// home.xy = home position (for image decomposition reform), home.z = packed RGBA (bitcast u32→f32),
/// home.w = sprite_index (for animated sprite sheets).
/// Stored in a separate storage buffer at compute binding 4.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ParticleAux {
    pub home: [f32; 4],
}

/// Particle render uniforms: 32 bytes.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct ParticleRenderUniforms {
    pub resolution: [f32; 2],
    pub time: f32,
    /// 0 = soft circle (default), 1 = sprite texture, 2 = animated sprite
    pub render_mode: u32,
    /// Sprite atlas columns, rows, total frames, padding
    pub sprite_cols: u32,
    pub sprite_rows: u32,
    pub sprite_frames: u32,
    pub _pad: u32,
}

/// Sprite atlas definition for textured particles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpriteDef {
    /// Texture file path relative to assets/images/
    pub texture: String,
    #[serde(default = "default_one_u32")]
    pub cols: u32,
    #[serde(default = "default_one_u32")]
    pub rows: u32,
    #[serde(default)]
    pub animated: bool,
    #[serde(default)]
    pub frames: u32,
}

/// Image sampling configuration for image-to-particle decomposition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSampleDef {
    /// Sampling mode: "grid", "threshold", or "random"
    #[serde(default = "default_sample_mode")]
    pub mode: String,
    /// Brightness threshold (for "threshold" mode)
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    /// Scale factor for mapping image to screen space
    #[serde(default = "default_scale")]
    pub scale: f32,
}

fn default_sample_mode() -> String {
    "grid".to_string()
}
fn default_threshold() -> f32 {
    0.1
}
fn default_scale() -> f32 {
    1.0
}
fn default_one_u32() -> u32 {
    1
}

/// .pfx particle definition (JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleDef {
    #[serde(default = "default_max_count")]
    pub max_count: u32,
    /// Custom compute shader (relative to shaders dir). If empty, uses builtin.
    #[serde(default)]
    pub compute_shader: String,
    #[serde(default)]
    pub emitter: EmitterDef,
    #[serde(default = "default_lifetime")]
    pub lifetime: f32,
    #[serde(default = "default_initial_speed")]
    pub initial_speed: f32,
    #[serde(default = "default_initial_size")]
    pub initial_size: f32,
    #[serde(default)]
    pub size_end: f32,
    #[serde(default)]
    pub gravity: [f32; 2],
    #[serde(default = "default_drag")]
    pub drag: f32,
    #[serde(default)]
    pub turbulence: f32,
    #[serde(default)]
    pub attraction_strength: f32,
    #[serde(default = "default_emit_rate")]
    pub emit_rate: f32,
    #[serde(default)]
    pub burst_on_beat: u32,
    /// Sprite texture definition (optional)
    #[serde(default)]
    pub sprite: Option<SpriteDef>,
    /// Image sampling for image-to-particle decomposition (optional)
    #[serde(default)]
    pub image_sample: Option<ImageSampleDef>,
    /// Blend mode: "additive" (default) or "alpha"
    #[serde(default = "default_blend")]
    pub blend: String,
}

fn default_blend() -> String {
    "additive".to_string()
}

fn default_max_count() -> u32 {
    10000
}
fn default_lifetime() -> f32 {
    3.0
}
fn default_initial_speed() -> f32 {
    0.3
}
fn default_initial_size() -> f32 {
    0.02
}
fn default_drag() -> f32 {
    0.98
}
fn default_emit_rate() -> f32 {
    100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_size_64() {
        assert_eq!(std::mem::size_of::<Particle>(), 64);
    }

    #[test]
    fn particle_uniforms_size_128() {
        assert_eq!(std::mem::size_of::<ParticleUniforms>(), 128);
    }

    #[test]
    fn particle_render_uniforms_size_32() {
        assert_eq!(std::mem::size_of::<ParticleRenderUniforms>(), 32);
    }

    #[test]
    fn particle_def_serde_defaults() {
        let json = r#"{"emitter":{}}"#;
        let def: ParticleDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.max_count, 10000);
        assert!((def.lifetime - 3.0).abs() < 1e-6);
        assert!((def.initial_speed - 0.3).abs() < 1e-6);
        assert!((def.initial_size - 0.02).abs() < 1e-6);
        assert!((def.drag - 0.98).abs() < 1e-6);
        assert!((def.emit_rate - 100.0).abs() < 1e-6);
        assert_eq!(def.blend, "additive");
        assert!(def.sprite.is_none());
        assert!(def.image_sample.is_none());
    }
}

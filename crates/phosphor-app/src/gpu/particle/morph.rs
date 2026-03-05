use std::path::Path;

use super::image_source;
use super::types::{ImageSampleDef, MorphTargetDef, ParticleAux};

/// Maximum number of morph targets per effect.
pub const MORPH_MAX_TARGETS: u32 = 4;

/// Auto-cycle mode for morph transitions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AutoCycle {
    Off,
    OnBeat,
    Timed(f32),
}

/// CPU-side state for morph (shape target morphing).
pub struct MorphState {
    pub target_count: u32,
    pub target_aux: [Vec<ParticleAux>; 4],
    pub source_index: u32,
    pub dest_index: u32,
    pub progress: f32,
    pub transitioning: bool,
    pub transition_duration: f32,
    pub transition_style: u32, // 0=spring, 1=explode, 2=flow, 3=cascade, 4=direct
    pub auto_cycle: AutoCycle,
    pub cycle_timer: f32,
    /// Cooldown after a morph completes — particles hold shape before next trigger.
    pub hold_timer: f32,
    pub hold_duration: f32,
}

impl MorphState {
    pub fn new() -> Self {
        Self {
            target_count: 0,
            target_aux: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            source_index: 0,
            dest_index: 0,
            progress: 1.0,
            transitioning: false,
            transition_duration: 1.0,
            transition_style: 0,
            auto_cycle: AutoCycle::OnBeat,
            cycle_timer: 0.0,
            hold_timer: 0.0,
            hold_duration: 2.0,
        }
    }

    /// Load target data into a slot.
    pub fn load_target(&mut self, slot: u32, data: Vec<ParticleAux>) {
        let slot = slot.min(MORPH_MAX_TARGETS - 1) as usize;
        self.target_aux[slot] = data;
        let mut count = 0u32;
        for i in 0..MORPH_MAX_TARGETS as usize {
            if !self.target_aux[i].is_empty() {
                count = i as u32 + 1;
            }
        }
        self.target_count = count;
    }

    /// Trigger morph to the next loaded target.
    pub fn trigger_next(&mut self) {
        if self.target_count < 2 || self.transitioning {
            return;
        }
        let next = (self.dest_index + 1) % self.target_count;
        self.trigger_morph(next);
    }

    /// Trigger morph to a specific target.
    pub fn trigger_morph(&mut self, dest: u32) {
        if dest >= self.target_count || self.transitioning {
            return;
        }
        self.source_index = self.dest_index;
        self.dest_index = dest;
        self.progress = 0.0;
        self.transitioning = true;
    }

    /// Advance progress and handle auto-cycle.
    pub fn update(&mut self, dt: f32, onset: f32) {
        // Advance transition progress
        if self.transitioning {
            self.progress += dt / self.transition_duration;
            if self.progress >= 1.0 {
                self.progress = 1.0;
                self.transitioning = false;
                // Start hold timer — particles settle into shape before next morph
                self.hold_timer = 0.0;
            }
        }

        // Hold timer — must expire before next auto-cycle trigger
        let holding = !self.transitioning && self.hold_timer < self.hold_duration;
        if holding {
            self.hold_timer += dt;
        }

        // Auto-cycle (only if not transitioning and hold period has elapsed)
        if !self.transitioning && !holding && self.target_count >= 2 {
            match self.auto_cycle {
                AutoCycle::OnBeat => {
                    if onset > 0.8 {
                        self.trigger_next();
                    }
                }
                AutoCycle::Timed(interval) => {
                    self.cycle_timer += dt;
                    if self.cycle_timer >= interval {
                        self.cycle_timer = 0.0;
                        self.trigger_next();
                    }
                }
                AutoCycle::Off => {}
            }
        }
    }

    /// Build a strided aux buffer interleaving all targets:
    /// [t0_p0, t1_p0, t2_p0, t3_p0, t0_p1, t1_p1, t2_p1, t3_p1, ...]
    pub fn build_strided_aux(&self, max_particles: u32) -> Vec<ParticleAux> {
        let stride = MORPH_MAX_TARGETS as usize;
        let total = max_particles as usize * stride;
        let mut buf = vec![ParticleAux { home: [0.0; 4] }; total];

        for target_idx in 0..stride {
            let target = &self.target_aux[target_idx];
            for particle_idx in 0..max_particles as usize {
                let src = if particle_idx < target.len() {
                    target[particle_idx]
                } else {
                    // Pad with zeroed aux (transparent, at origin)
                    ParticleAux { home: [0.0; 4] }
                };
                buf[particle_idx * stride + target_idx] = src;
            }
        }

        buf
    }

    /// Write morph uniforms into ParticleUniforms fields.
    pub fn write_uniforms(
        &self,
        morph_progress: &mut f32,
        morph_source: &mut u32,
        morph_dest: &mut u32,
        morph_flags: &mut u32,
    ) {
        *morph_progress = self.progress;
        *morph_source = self.source_index;
        *morph_dest = self.dest_index;
        let mut flags = 0u32;
        if self.transitioning {
            flags |= 1;
        }
        flags |= (self.transition_style & 0x7) << 1;
        *morph_flags = flags;
    }
}

/// Generate geometry target positions for common shapes.
/// `particle_size` is used to compute per-particle alpha that prevents accumulation blowout.
pub fn generate_geometry(shape: &str, max_particles: u32, particle_size: f32) -> Vec<ParticleAux> {
    let n = max_particles as usize;
    let nu = max_particles;
    let ps = particle_size;
    let mut result = Vec::with_capacity(n);

    match shape {
        "circle" => {
            // Filled disc — area = π × 0.75²
            let area = std::f32::consts::PI * 0.75 * 0.75;
            let golden = 137.508_f32.to_radians();
            for i in 0..n {
                let r = (i as f32 / n as f32).sqrt() * 0.75;
                let angle = i as f32 * golden;
                let x = angle.cos() * r;
                let y = angle.sin() * r;
                let packed = pack_color_for_density(0.4, 0.6, 1.0, nu, area, ps);
                result.push(ParticleAux {
                    home: [x, y, packed, 0.0],
                });
            }
        }
        "ring" => {
            // Thick ring — area ≈ π × (0.75² - 0.55²)
            let area = std::f32::consts::PI * (0.75 * 0.75 - 0.55 * 0.55);
            for i in 0..n {
                let t = i as f32 / n as f32;
                let angle = t * std::f32::consts::TAU;
                let r = 0.55 + hash_f32(i as f32 * 7.31) * 0.2;
                let x = angle.cos() * r;
                let y = angle.sin() * r;
                let packed = pack_color_for_density(0.3, 0.8, 0.9, nu, area, ps);
                result.push(ParticleAux {
                    home: [x, y, packed, 0.0],
                });
            }
        }
        "grid" => {
            // Grid — area = 1.6 × 1.6
            let area = 1.6 * 1.6;
            let side = (n as f32).sqrt().ceil() as usize;
            for i in 0..n {
                let gx = (i % side) as f32 / side as f32 * 2.0 - 1.0;
                let gy = (i / side) as f32 / side as f32 * 2.0 - 1.0;
                let checker = ((i % side) + (i / side)) % 2 == 0;
                let packed = if checker {
                    pack_color_for_density(0.9, 0.5, 0.2, nu, area, ps)
                } else {
                    pack_color_for_density(0.2, 0.5, 0.9, nu, area, ps)
                };
                result.push(ParticleAux {
                    home: [gx * 0.8, gy * 0.8, packed, 0.0],
                });
            }
        }
        "spiral" => {
            // Filled spiral — area ≈ π × 0.8²
            let area = std::f32::consts::PI * 0.8 * 0.8;
            let golden = 137.508_f32.to_radians();
            for i in 0..n {
                let t = i as f32 / n as f32;
                let r = t.sqrt() * 0.8;
                let angle = i as f32 * golden + t * std::f32::consts::TAU * 3.0;
                let x = angle.cos() * r;
                let y = angle.sin() * r;
                let hue = t;
                let (cr, cg, cb) = hsv_to_rgb(hue, 0.7, 0.9);
                let packed = pack_color_for_density(cr, cg, cb, nu, area, ps);
                result.push(ParticleAux {
                    home: [x, y, packed, 0.0],
                });
            }
        }
        "heart" => {
            // Heart — area ≈ 1.0
            let area = 1.0;
            for i in 0..n {
                let r = (i as f32 / n as f32).sqrt() * 0.85;
                let angle = i as f32 * 137.508_f32.to_radians();
                let dx = angle.cos() * r;
                let dy = angle.sin() * r;
                let t = dy.atan2(dx);
                let hr = 0.7 * r;
                let x = hr * 16.0 * t.sin().powi(3) / 16.0;
                let y = hr
                    * (13.0 * t.cos()
                        - 5.0 * (2.0 * t).cos()
                        - 2.0 * (3.0 * t).cos()
                        - (4.0 * t).cos())
                    / 17.0;
                let packed = pack_color_for_density(0.9, 0.2 + r * 0.3, 0.3, nu, area, ps);
                result.push(ParticleAux {
                    home: [x, y, packed, 0.0],
                });
            }
        }
        "star" => {
            // Star — area ≈ 1.2
            let area = 1.2;
            let golden = 137.508_f32.to_radians();
            for i in 0..n {
                let base_r = (i as f32 / n as f32).sqrt();
                let angle = i as f32 * golden;
                let points = 5.0;
                let star_mod = 0.6 + 0.4 * (angle * points).cos().abs();
                let r = base_r * star_mod * 0.7;
                let x = angle.cos() * r;
                let y = angle.sin() * r;
                let packed = pack_color_for_density(1.0, 0.85, 0.2, nu, area, ps);
                result.push(ParticleAux {
                    home: [x, y, packed, 0.0],
                });
            }
        }
        _ => {
            // Default: filled disc
            let area = std::f32::consts::PI * 0.8 * 0.8;
            let golden = 137.508_f32.to_radians();
            for i in 0..n {
                let r = (i as f32 / n as f32).sqrt() * 0.8;
                let angle = i as f32 * golden;
                let x = angle.cos() * r;
                let y = angle.sin() * r;
                let packed = pack_color_for_density(0.7, 0.7, 0.7, nu, area, ps);
                result.push(ParticleAux {
                    home: [x, y, packed, 0.0],
                });
            }
        }
    }

    result
}

/// Generate random scatter positions.
pub fn generate_random(max_particles: u32, particle_size: f32) -> Vec<ParticleAux> {
    let n = max_particles as usize;
    // Random fills ~1.8×1.8 area
    let area = 1.8 * 1.8;
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let x = hash_f32(i as f32 * 1.37) * 2.0 - 1.0;
        let y = hash_f32(i as f32 * 2.71 + 7.0) * 2.0 - 1.0;
        let packed = pack_color_for_density(
            hash_f32(i as f32 * 3.14),
            hash_f32(i as f32 * 5.67 + 3.0),
            hash_f32(i as f32 * 8.91 + 7.0),
            max_particles,
            area,
            particle_size,
        );
        result.push(ParticleAux {
            home: [x * 0.9, y * 0.9, packed, 0.0],
        });
    }
    result
}

/// Load a morph target from a MorphTargetDef.
pub fn load_morph_target(
    def: &MorphTargetDef,
    max_particles: u32,
    particle_size: f32,
    assets_dir: &Path,
) -> Result<Vec<ParticleAux>, String> {
    if def.source == "random" {
        return Ok(generate_random(max_particles, particle_size));
    }

    if let Some(shape) = def.source.strip_prefix("geometry:") {
        return Ok(generate_geometry(shape, max_particles, particle_size));
    }

    if let Some(image_name) = def.source.strip_prefix("image:") {
        let image_path = assets_dir.join("images").join(image_name);
        let sample_def = ImageSampleDef {
            mode: "grid".to_string(),
            threshold: 0.1,
            scale: 1.0,
        };
        return image_source::sample_image(&image_path, &sample_def, max_particles);
    }

    Err(format!("Unknown morph target source: {}", def.source))
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if hp < 1.0 {
        (c, x, 0.0)
    } else if hp < 2.0 {
        (x, c, 0.0)
    } else if hp < 3.0 {
        (0.0, c, x)
    } else if hp < 4.0 {
        (0.0, x, c)
    } else if hp < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (r + m, g + m, b + m)
}

fn hash_f32(n: f32) -> f32 {
    (n * 43758.5453).sin().fract().abs()
}

/// Pack RGB with alpha computed from expected particle overlap density.
/// `n` particles spread over `area` clip-space units² at the given `particle_size`.
fn pack_color_for_density(r: f32, g: f32, b: f32, n: u32, area: f32, particle_size: f32) -> f32 {
    // Estimate particles per pixel: n / (area_pixels), where each particle covers ~size² pixels
    // Target: accumulated alpha ≈ 1.0, so per-particle alpha = 1 / overlap_count
    let area_in_particles = area / (particle_size * particle_size);
    let overlap = (n as f32 / area_in_particles).max(1.0);
    let alpha = (1.0 / overlap).clamp(0.05, 1.0);
    pack_color_a(r, g, b, alpha)
}

fn pack_color_a(r: f32, g: f32, b: f32, a: f32) -> f32 {
    let r = (r * 255.0).clamp(0.0, 255.0) as u32;
    let g = (g * 255.0).clamp(0.0, 255.0) as u32;
    let b = (b * 255.0).clamp(0.0, 255.0) as u32;
    let a = (a * 255.0).clamp(0.0, 255.0) as u32;
    let packed = r | (g << 8) | (b << 16) | (a << 24);
    f32::from_bits(packed)
}

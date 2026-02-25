use std::path::Path;

use super::types::{ImageSampleDef, ParticleAux};

/// Maximum image dimension after resize (keeps particle count reasonable).
const MAX_DIM: u32 = 512;

/// Sample pixels from an image into ParticleAux data.
/// Returns aux data with home positions (clip space) and packed RGBA colors.
pub fn sample_image(
    path: &Path,
    sample_def: &ImageSampleDef,
    max_particles: u32,
) -> Result<Vec<ParticleAux>, String> {
    let img = image::open(path)
        .map_err(|e| format!("Failed to load image: {e}"))?;

    // Resize to cap dimensions while preserving aspect ratio
    let img = if img.width() > MAX_DIM || img.height() > MAX_DIM {
        img.resize(MAX_DIM, MAX_DIM, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let samples = match sample_def.mode.as_str() {
        "threshold" => sample_threshold(&rgba, w, h, sample_def.threshold, max_particles),
        "random" => sample_random(&rgba, w, h, max_particles),
        _ => sample_grid(&rgba, w, h, max_particles), // "grid" is default
    };

    // Map pixel coords to clip space with aspect ratio correction
    let aspect = w as f32 / h as f32;
    let scale = sample_def.scale;
    let aux: Vec<ParticleAux> = samples
        .into_iter()
        .map(|(px, py, r, g, b, a)| {
            // Map pixel (0..w, 0..h) to clip space (-1..1, -1..1)
            let cx = ((px as f32 / w as f32) * 2.0 - 1.0) * scale;
            let cy = -((py as f32 / h as f32) * 2.0 - 1.0) * scale; // flip Y
            // Correct for non-square aspect ratio
            let cx = if aspect > 1.0 { cx } else { cx * aspect };
            let cy = if aspect > 1.0 { cy / aspect } else { cy };
            // Pack RGBA into f32 via bitcast
            let packed = pack_rgba(r, g, b, a);
            ParticleAux {
                home: [cx, cy, packed, 0.0],
            }
        })
        .collect();

    log::info!(
        "Sampled {} particles from {}x{} image (mode: {}, scale: {:.1})",
        aux.len(),
        w,
        h,
        sample_def.mode,
        scale
    );
    Ok(aux)
}

/// Grid sampling: every Nth pixel to fit within max_particles.
fn sample_grid(
    img: &image::RgbaImage,
    w: u32,
    h: u32,
    max_particles: u32,
) -> Vec<(u32, u32, u8, u8, u8, u8)> {
    let total_pixels = w * h;
    let step = ((total_pixels as f32 / max_particles as f32).sqrt().ceil() as u32).max(1);
    let mut samples = Vec::new();

    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            if samples.len() >= max_particles as usize {
                break;
            }
            let pixel = img.get_pixel(x, y);
            // Skip fully transparent pixels
            if pixel[3] < 10 {
                continue;
            }
            samples.push((x, y, pixel[0], pixel[1], pixel[2], pixel[3]));
        }
    }
    samples
}

/// Threshold sampling: pixels above brightness threshold.
fn sample_threshold(
    img: &image::RgbaImage,
    w: u32,
    h: u32,
    threshold: f32,
    max_particles: u32,
) -> Vec<(u32, u32, u8, u8, u8, u8)> {
    let mut candidates = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let pixel = img.get_pixel(x, y);
            if pixel[3] < 10 {
                continue;
            }
            let brightness =
                (pixel[0] as f32 * 0.299 + pixel[1] as f32 * 0.587 + pixel[2] as f32 * 0.114)
                    / 255.0;
            if brightness > threshold {
                candidates.push((x, y, pixel[0], pixel[1], pixel[2], pixel[3]));
            }
        }
    }

    // Subsample if too many
    if candidates.len() > max_particles as usize {
        let step = candidates.len() / max_particles as usize;
        candidates
            .into_iter()
            .step_by(step.max(1))
            .take(max_particles as usize)
            .collect()
    } else {
        candidates
    }
}

/// Random sampling: uniform random subset.
fn sample_random(
    img: &image::RgbaImage,
    w: u32,
    h: u32,
    max_particles: u32,
) -> Vec<(u32, u32, u8, u8, u8, u8)> {
    // Simple deterministic pseudo-random using hash
    let total = w * h;
    let count = max_particles.min(total);
    let mut samples = Vec::with_capacity(count as usize);
    let mut seen = std::collections::HashSet::new();

    // LCG pseudo-random
    let mut rng: u64 = 42;
    let a: u64 = 6364136223846793005;
    let c: u64 = 1442695040888963407;

    while samples.len() < count as usize {
        rng = rng.wrapping_mul(a).wrapping_add(c);
        let idx = (rng >> 33) as u32 % total;
        if seen.contains(&idx) {
            continue;
        }
        seen.insert(idx);

        let x = idx % w;
        let y = idx / w;
        let pixel = img.get_pixel(x, y);
        if pixel[3] < 10 {
            continue;
        }
        samples.push((x, y, pixel[0], pixel[1], pixel[2], pixel[3]));
    }
    samples
}

/// Pack RGBA bytes into a single f32 via bitcast from u32.
fn pack_rgba(r: u8, g: u8, b: u8, a: u8) -> f32 {
    let packed = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24);
    f32::from_bits(packed)
}

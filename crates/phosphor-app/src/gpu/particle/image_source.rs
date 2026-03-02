use std::path::Path;

use super::types::{ImageSampleDef, ParticleAux};

/// Maximum image dimension after resize.
/// 2048 supports up to ~1M particles with grid sampling (step=2 at 2048²).
const MAX_DIM: u32 = 2048;

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

/// Sample pixels from raw RGBA buffer data into ParticleAux data.
/// Like `sample_image()`, but skips file I/O and image crate decode — used for
/// per-frame updates from video or webcam sources where data is already decoded RGBA.
pub fn sample_rgba_buffer(
    data: &[u8],
    width: u32,
    height: u32,
    sample_def: &ImageSampleDef,
    max_particles: u32,
) -> Vec<ParticleAux> {
    if data.is_empty() || width == 0 || height == 0 {
        return Vec::new();
    }

    // Resize if needed (reuse image crate for downscale)
    let (rgba_data, w, h) = if width > MAX_DIM || height > MAX_DIM {
        let img = image::RgbaImage::from_raw(width, height, data.to_vec());
        match img {
            Some(img) => {
                let resized = image::imageops::resize(
                    &img,
                    MAX_DIM.min(width),
                    MAX_DIM.min(height),
                    image::imageops::FilterType::Nearest,
                );
                let (rw, rh) = resized.dimensions();
                (resized.into_raw(), rw, rh)
            }
            None => return Vec::new(),
        }
    } else {
        (data.to_vec(), width, height)
    };

    // Build a temporary RgbaImage for sampling functions
    let img = match image::RgbaImage::from_raw(w, h, rgba_data) {
        Some(img) => img,
        None => return Vec::new(),
    };

    let samples = match sample_def.mode.as_str() {
        "threshold" => sample_threshold(&img, w, h, sample_def.threshold, max_particles),
        "random" => sample_random(&img, w, h, max_particles),
        _ => sample_grid(&img, w, h, max_particles),
    };

    // Map pixel coords to clip space with aspect ratio correction
    let aspect = w as f32 / h as f32;
    let scale = sample_def.scale;
    samples
        .into_iter()
        .map(|(px, py, r, g, b, a)| {
            let cx = ((px as f32 / w as f32) * 2.0 - 1.0) * scale;
            let cy = -((py as f32 / h as f32) * 2.0 - 1.0) * scale;
            let cx = if aspect > 1.0 { cx } else { cx * aspect };
            let cy = if aspect > 1.0 { cy / aspect } else { cy };
            let packed = pack_rgba(r, g, b, a);
            ParticleAux {
                home: [cx, cy, packed, 0.0],
            }
        })
        .collect()
}

/// Pack RGBA bytes into a single f32 via bitcast from u32.
fn pack_rgba(r: u8, g: u8, b: u8, a: u8) -> f32 {
    let packed = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24);
    f32::from_bits(packed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rgba_buffer_empty() {
        let def = ImageSampleDef {
            mode: "grid".to_string(),
            threshold: 0.1,
            scale: 1.0,
        };
        assert!(sample_rgba_buffer(&[], 0, 0, &def, 100).is_empty());
    }

    #[test]
    fn sample_rgba_buffer_single_pixel() {
        // 1x1 white pixel
        let data = vec![255, 255, 255, 255];
        let def = ImageSampleDef {
            mode: "grid".to_string(),
            threshold: 0.1,
            scale: 1.0,
        };
        let result = sample_rgba_buffer(&data, 1, 1, &def, 100);
        assert_eq!(result.len(), 1);
        // Position should be at origin (single pixel maps to center)
        assert!(result[0].home[0].abs() < 1.1);
        assert!(result[0].home[1].abs() < 1.1);
    }

    #[test]
    fn sample_rgba_buffer_grid_mode() {
        // 4x4 opaque red image
        let mut data = Vec::with_capacity(4 * 4 * 4);
        for _ in 0..16 {
            data.extend_from_slice(&[255, 0, 0, 255]);
        }
        let def = ImageSampleDef {
            mode: "grid".to_string(),
            threshold: 0.1,
            scale: 1.0,
        };
        let result = sample_rgba_buffer(&data, 4, 4, &def, 100);
        assert!(!result.is_empty());
        assert!(result.len() <= 16);
    }

    #[test]
    fn sample_rgba_buffer_transparent_skipped() {
        // 2x2 fully transparent image
        let data = vec![0u8; 2 * 2 * 4];
        let def = ImageSampleDef {
            mode: "grid".to_string(),
            threshold: 0.1,
            scale: 1.0,
        };
        let result = sample_rgba_buffer(&data, 2, 2, &def, 100);
        assert!(result.is_empty());
    }

    #[test]
    fn sample_rgba_buffer_respects_max_particles() {
        // 100x100 opaque image = 10000 pixels, limit to 50
        let data = vec![255u8; 100 * 100 * 4];
        let def = ImageSampleDef {
            mode: "grid".to_string(),
            threshold: 0.1,
            scale: 1.0,
        };
        let result = sample_rgba_buffer(&data, 100, 100, &def, 50);
        assert!(result.len() <= 50);
    }
}

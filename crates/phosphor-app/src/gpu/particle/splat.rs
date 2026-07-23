//! GPU packing + per-frame driver for the Splat effect (#1800).
//!
//! [`SplatStatic`] is the 32-byte per-splat attribute record the sim reads at
//! `@group(2) @binding(1)` (binding 0 is the lib's trail-buffer declaration).
//! Positions stay f32 (f16 quantization jitters at close zoom); the 3D
//! covariance is PRECOMPUTED here from the source quat+scales (Σ3 = R·S·SᵀRᵀ
//! is camera-independent — building it per frame per splat cost ~9 ms at 2M
//! in the sim, live perf finding) and rides six f16 halves ×[`COV_SCALE`];
//! color+opacity are 8-bit unorm. Zero-padding is the count mechanism: slots
//! past the scene unpack `opacity == 0` and the sim parks them dead, so no
//! separate splat-count uniform exists.
//!
//! [`SplatDriver`] owns the CPU-side camera/envelope state (yaw accumulator,
//! centroid EMA, drop-explode envelope) and writes the `cam_*`/`splat_*`
//! fields appended to `ParticleUniforms` in the #1800 ABI bump.

use bytemuck::{Pod, Zeroable};

use super::splat_source::SplatCloud;
use super::types::ParticleUniforms;
use crate::gpu::half::f32_to_f16;

/// f16 range helper for covariance entries: world-unit σ² values sit around
/// 1e-6..1e-2 (subnormal territory for f16), so Σ3 is stored ×1024 and the
/// sim folds 1/1024 back in after projection. Must match `COV_SCALE` in
/// `splat_sim.wgsl`.
pub const COV_SCALE: f32 = 1024.0;

/// Packed per-splat static attributes: 32 bytes, uploaded once per scene.
/// WGSL mirror (declared in `splat_sim.wgsl`):
/// `struct SplatStatic { pos: vec3f, color: u32, cov_a: u32, cov_b: u32, cov_c: u32, _spare: u32 }`
/// (vec3f align 16 / size 12 puts `color` at offset 12; struct stride 32 —
/// byte-identical to this layout.)
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Pod, Zeroable)]
pub struct SplatStatic {
    /// World position, scene-normalized (median-centered, p95 radius ≈ 1).
    pub pos: [f32; 3],
    /// `pack4x8unorm(r, g, b, opacity)` — r in the low byte.
    pub color: u32,
    /// `pack2x16float(Σxx, Σyy)` — 3D covariance × [`COV_SCALE`].
    pub cov_a: u32,
    /// `pack2x16float(Σzz, Σxy)`.
    pub cov_b: u32,
    /// `pack2x16float(Σxz, Σyz)`.
    pub cov_c: u32,
    /// Reserved.
    pub _spare: u32,
}

/// Per-splat view-dependent SH coefficients: 96 bytes, uploaded once per scene
/// at `@group(2) @binding(2)`. 45 f16 halves (bands 1–3 × RGB, channel-major —
/// the PLY `f_rest` order) in 24 u32 words; the trailing 3 halves are padding.
///
/// **Record 0 of the buffer is a header, not a splat** — splat *i* lives at
/// index *i + 1*. The header carries the inverse scene rotation as a 3×3
/// (words 0–8, one f32 each, row-major), because SH lobes are defined in the
/// source frame while `normalize_cloud` rotates the geometry into the render
/// frame; the sim rotates the view direction back through it rather than
/// SH-rotating every coefficient (bands 2–3 would need Wigner-D matrices).
///
/// f16 rather than the i8-plus-scale the compressed-PLY ecosystem uses: at the
/// scene sizes in play (≤1.5M splats ⇒ ≤144 MB) the 4× saving does not pay for
/// a quantization-scale surface to get wrong. i8 is the lever if it ever does.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Pod, Zeroable)]
pub struct SplatShRec {
    pub data: [u32; 24],
}

/// Bands present per channel at each degree — 3, 8, 15 for degrees 1–3.
fn bands_for_degree(degree: u8) -> usize {
    match degree {
        1 => 3,
        2 => 8,
        3 => 15,
        _ => 0,
    }
}

/// Pack a scene's SH coefficients for upload: a header record followed by one
/// record per splat. Returns `None` for DC-only scenes — the caller binds a
/// dummy buffer instead, so a scene without `f_rest` costs 96 bytes, not
/// 96 MB.
pub fn pack_sh(cloud: &SplatCloud) -> Option<Vec<SplatShRec>> {
    if cloud.sh_degree == 0 || cloud.sh.len() < cloud.count {
        return None;
    }
    let mut out = Vec::with_capacity(cloud.count + 1);

    // Header: inverse scene rotation, row-major, as raw f32 bits.
    let mut header = SplatShRec { data: [0; 24] };
    let cols = cloud.sh_rot_inv.to_cols_array();
    for row in 0..3 {
        for col in 0..3 {
            // glam is column-major; transpose into row-major for the shader.
            header.data[row * 3 + col] = cols[col * 3 + row].to_bits();
        }
    }
    out.push(header);

    // The GPU record is fixed-width, so a degree-1 or -2 capture fills a prefix
    // per channel and leaves the rest zero — matching the shader, which skips
    // the absent bands entirely via `splat_sh_degree`.
    let bands = bands_for_degree(cloud.sh_degree);
    for sh in &cloud.sh[..cloud.count] {
        let mut rec = SplatShRec { data: [0; 24] };
        for ch in 0..3 {
            for b in 0..bands {
                // Source is channel-major with a per-channel stride of `bands`;
                // the GPU record uses the canonical 15-wide stride so the
                // shader indexes the same way at every degree.
                let half = sh[ch * bands + b];
                let slot = ch * 15 + b;
                rec.data[slot / 2] |= (half as u32) << (16 * (slot % 2));
            }
        }
        out.push(rec);
    }
    Some(out)
}

/// Matches WGSL `pack2x16float`: `a` in the low half.
fn pack2x16float(a: f32, b: f32) -> u32 {
    (f32_to_f16(a) as u32) | ((f32_to_f16(b) as u32) << 16)
}

/// Matches WGSL `pack4x8unorm`: `x` in the low byte, values clamped to 0..1.
fn pack4x8unorm(x: f32, y: f32, z: f32, w: f32) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    q(x) | (q(y) << 8) | (q(z) << 16) | (q(w) << 24)
}

/// Pack a decoded scene into the GPU layout, precomputing each splat's 3D
/// covariance Σ3 = R·S·SᵀRᵀ (camera-independent). Length equals
/// `cloud.count`; the upload path zero-fills the remaining `max_particles`
/// slots (dead).
pub fn pack_cloud(cloud: &SplatCloud) -> Vec<SplatStatic> {
    (0..cloud.count)
        .map(|i| {
            let c = cloud.colors[i];
            let s = cloud.scales[i];
            let r = cloud.rotations[i];
            let m = glam::Mat3::from_quat(glam::Quat::from_xyzw(r[0], r[1], r[2], r[3]))
                * glam::Mat3::from_diagonal(glam::Vec3::new(s[0], s[1], s[2]));
            let sigma3 = m * m.transpose();
            SplatStatic {
                pos: cloud.positions[i],
                color: pack4x8unorm(c[0], c[1], c[2], cloud.opacities[i]),
                cov_a: pack2x16float(
                    sigma3.x_axis.x * COV_SCALE, // Σxx
                    sigma3.y_axis.y * COV_SCALE, // Σyy
                ),
                cov_b: pack2x16float(
                    sigma3.z_axis.z * COV_SCALE, // Σzz
                    sigma3.y_axis.x * COV_SCALE, // Σxy
                ),
                cov_c: pack2x16float(
                    sigma3.z_axis.x * COV_SCALE, // Σxz
                    sigma3.z_axis.y * COV_SCALE, // Σyz
                ),
                _spare: 0,
            }
        })
        .collect()
}

/// CPU-side per-frame state for the splat orbit camera and audio envelopes.
/// Lives on `ParticleSystem` (gated on `def.splat`), updated in `dispatch`
/// just before the uniform upload.
///
/// UI params arrive as .pfx slots 8–11 (`ParticleSystem::splat_ui_params`,
/// forwarded from the param store like `effect_params`):
/// `[orbit_speed, cam_distance, cam_pitch, focal_bias]`.
pub struct SplatDriver {
    yaw: f32,
    centroid_ema: f32,
    explode_env: f32,
}

impl SplatDriver {
    pub fn new() -> Self {
        Self {
            yaw: 0.0,
            centroid_ema: 0.5,
            explode_env: 0.0,
        }
    }

    /// Advance camera + envelopes and write the `cam_*`/`splat_*` uniform
    /// fields. Reads audio (`rms`, `buildup`, `drop`, `centroid`) and
    /// `delta_time` from the uniforms already populated this frame.
    pub fn update(&mut self, u: &mut ParticleUniforms, ui_params: [f32; 4]) {
        let dt = u.delta_time;
        let [orbit_speed, cam_distance, cam_pitch, focal_bias] = ui_params;

        // Audio-scaled orbit: still audible motion at silence, gentle push
        // with level. Yaw stays bounded (it feeds sin/cos in the sim).
        self.yaw += dt * orbit_speed * 0.6 * (0.4 + 0.6 * u.rms);
        self.yaw %= std::f32::consts::TAU;

        // Buildup leans the camera in — released by the drop with the riser.
        let dist = cam_distance.clamp(0.5, 6.0) * (1.0 - 0.15 * u.buildup);

        // Focal plane follows spectral centroid (EMA — focus pulls, never
        // snaps): bright timbre → focus forward (nearer), dark → far field.
        let k = 1.0 - (-dt / 0.35).exp();
        self.centroid_ema += (u.centroid - self.centroid_ema) * k;

        // Drop envelope: latch the one-frame trigger, decay τ = 0.45 s.
        self.explode_env = (self.explode_env * (-dt / 0.45).exp()).max(u.drop);

        u.cam_yaw = self.yaw;
        u.cam_pitch = cam_pitch.clamp(-1.35, 1.35);
        u.cam_distance = dist;
        u.cam_focal = 1.5; // cot(fov/2), volumetric-default field of view
        // View depth spans [dist−1, dist+1] for the unit-radius scene.
        let focal = dist + (1.0 - 2.0 * self.centroid_ema) + focal_bias;
        u.splat_focal_depth = focal.clamp(dist - 1.2, dist + 1.2);
        u.splat_explode = self.explode_env;
    }
}

/// Deterministic procedural test scene: `count` gaussians jittered along a
/// (2,3) torus knot, hue by arc-length — the render probe and perf tests use
/// it so CI never depends on a downloaded asset. Already normalized
/// (radius ≈ 1), matching what `load_splat_file` produces.
#[cfg(test)]
pub fn generate_test_scene(count: usize) -> SplatCloud {
    use super::splat_source::SplatTransform;
    let mut rng: u64 = 0x70CA_11ED_5EED_0001;
    let mut next = move || {
        rng = rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32 // 0..1
    };

    let mut cloud = SplatCloud {
        count,
        positions: Vec::with_capacity(count),
        scales: Vec::with_capacity(count),
        rotations: Vec::with_capacity(count),
        colors: Vec::with_capacity(count),
        opacities: Vec::with_capacity(count),
        sh: Vec::new(),
        sh_degree: 0,
        sh_rot_inv: glam::Mat3::IDENTITY,
        source_path: "procedural:torus-knot".to_string(),
        total_in_file: count as u32,
        transform: SplatTransform::default(),
    };

    use std::f32::consts::TAU;
    for i in 0..count {
        let t = i as f32 / count as f32;
        let (r_major, r_minor) = (0.68, 0.3);
        let ring = r_major + r_minor * (3.0 * TAU * t).cos();
        let jitter = 0.035;
        cloud.positions.push([
            ring * (2.0 * TAU * t).cos() + (next() - 0.5) * jitter,
            r_minor * (3.0 * TAU * t).sin() + (next() - 0.5) * jitter,
            ring * (2.0 * TAU * t).sin() + (next() - 0.5) * jitter,
        ]);
        let s = 0.006 + next() * 0.010;
        cloud.scales.push([s, s * (0.5 + next()), s]);
        cloud.rotations.push([0.0, 0.0, 0.0, 1.0]);
        cloud.colors.push([
            0.5 + 0.5 * (TAU * t).cos(),
            0.5 + 0.5 * (TAU * (t + 0.33)).cos(),
            0.5 + 0.5 * (TAU * (t + 0.67)).cos(),
        ]);
        cloud.opacities.push(0.55 + 0.4 * next());
    }
    cloud
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    /// Test-local f16 → f32 (production only packs).
    fn f16_to_f32(h: u16) -> f32 {
        let sign = ((h >> 15) & 1) as u32;
        let exp = ((h >> 10) & 0x1F) as u32;
        let frac = (h & 0x3FF) as u32;
        let bits = if exp == 0 {
            if frac == 0 {
                sign << 31
            } else {
                // Subnormal: renormalize.
                let mut e = 127 - 15 + 1;
                let mut f = frac;
                while f & 0x400 == 0 {
                    f <<= 1;
                    e -= 1;
                }
                (sign << 31) | ((e as u32) << 23) | ((f & 0x3FF) << 13)
            }
        } else if exp == 0x1F {
            (sign << 31) | 0x7F80_0000 | (frac << 13)
        } else {
            (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13)
        };
        f32::from_bits(bits)
    }

    fn unpack2x16(v: u32) -> (f32, f32) {
        (f16_to_f32(v as u16), f16_to_f32((v >> 16) as u16))
    }

    /// A cloud of `n` splats carrying `degree` SH bands, coefficient c of splat
    /// i set to a distinct, f16-exact value.
    fn sh_cloud(n: usize, degree: u8, rot: glam::Quat) -> SplatCloud {
        use super::super::splat_source::{SH_COEFFS, SplatTransform};
        let bands = bands_for_degree(degree);
        let sh = (0..n)
            .map(|i| {
                let mut rec = [0u16; SH_COEFFS];
                for c in 0..bands * 3 {
                    rec[c] = f32_to_f16(1.0 + i as f32 + c as f32 / 8.0);
                }
                rec
            })
            .collect();
        SplatCloud {
            count: n,
            positions: vec![[0.0; 3]; n],
            scales: vec![[0.01; 3]; n],
            rotations: vec![[0.0, 0.0, 0.0, 1.0]; n],
            colors: vec![[0.5; 3]; n],
            opacities: vec![1.0; n],
            sh,
            sh_degree: degree,
            sh_rot_inv: glam::Mat3::from_quat(rot.inverse()),
            source_path: String::new(),
            total_in_file: n as u32,
            transform: SplatTransform::default(),
        }
    }

    /// Read GPU-record coefficient `slot` the way `sh_coeff` does in WGSL.
    fn sh_slot(rec: &SplatShRec, slot: usize) -> f32 {
        let pair = unpack2x16(rec.data[slot / 2]);
        if slot.is_multiple_of(2) {
            pair.0
        } else {
            pair.1
        }
    }

    #[test]
    fn pack_sh_none_for_dc_only_scene() {
        // A DC-only capture must not allocate 96 B × N; the caller binds a dummy.
        assert!(pack_sh(&sh_cloud(4, 0, glam::Quat::IDENTITY)).is_none());
    }

    #[test]
    fn pack_sh_header_is_the_inverse_scene_rotation() {
        let rot = glam::Quat::from_euler(glam::EulerRot::XYZ, 180f32.to_radians(), 0.0, 0.0);
        let recs = pack_sh(&sh_cloud(1, 3, rot)).unwrap();
        // Row-major in words 0–8, so row r dotted with a vector is (M·v).r —
        // this is exactly how splat_sim.wgsl reconstructs it.
        let m = glam::Mat3::from_cols_array(&[
            f32::from_bits(recs[0].data[0]),
            f32::from_bits(recs[0].data[3]),
            f32::from_bits(recs[0].data[6]),
            f32::from_bits(recs[0].data[1]),
            f32::from_bits(recs[0].data[4]),
            f32::from_bits(recs[0].data[7]),
            f32::from_bits(recs[0].data[2]),
            f32::from_bits(recs[0].data[5]),
            f32::from_bits(recs[0].data[8]),
        ]);
        // Undoing the scene rotation must return a rendered direction to source.
        let back = m * (rot * glam::Vec3::Y);
        assert!((back - glam::Vec3::Y).length() < 1e-5, "got {back:?}");
    }

    #[test]
    fn pack_sh_offsets_splats_past_the_header() {
        let recs = pack_sh(&sh_cloud(3, 3, glam::Quat::IDENTITY)).unwrap();
        assert_eq!(recs.len(), 4, "header + one record per splat");
        // Splat i at index i+1 — an off-by-one here shades every splat with its
        // neighbour's coefficients, which looks plausible and is nearly
        // invisible on a dense capture.
        for i in 0..3 {
            assert!(
                (sh_slot(&recs[i + 1], 0) - (1.0 + i as f32)).abs() < 1e-3,
                "splat {i} landed at the wrong record"
            );
        }
    }

    #[test]
    fn pack_sh_widens_low_degree_to_the_canonical_stride() {
        // Source is channel-major with a per-channel stride of `bands`; the GPU
        // record always strides 15 so the shader indexes identically at every
        // degree. Degree 1: source [R0 R1 R2 G0 G1 G2 B0 B1 B2] must land at
        // GPU slots 0,1,2 / 15,16,17 / 30,31,32 with everything else zero.
        let recs = pack_sh(&sh_cloud(1, 1, glam::Quat::IDENTITY)).unwrap();
        let r = &recs[1];
        for ch in 0..3 {
            for b in 0..3 {
                let expect = 1.0 + (ch * 3 + b) as f32 / 8.0;
                assert!(
                    (sh_slot(r, ch * 15 + b) - expect).abs() < 1e-3,
                    "channel {ch} band {b}: got {}",
                    sh_slot(r, ch * 15 + b)
                );
            }
            // The bands this capture does not carry stay zero, so the shader's
            // degree guard and the data agree even if one of them is wrong.
            for b in 3..15 {
                assert_eq!(sh_slot(r, ch * 15 + b), 0.0);
            }
        }
    }

    #[test]
    fn splat_sh_rec_layout() {
        // 96 B, 24 words — must match `struct SplatSh` in splat_sim.wgsl.
        assert_eq!(std::mem::size_of::<SplatShRec>(), 96);
        assert_eq!(std::mem::align_of::<SplatShRec>(), 4);
    }

    #[test]
    fn splat_static_layout() {
        assert_eq!(std::mem::size_of::<SplatStatic>(), 32);
        assert_eq!(offset_of!(SplatStatic, pos), 0);
        assert_eq!(offset_of!(SplatStatic, color), 12);
        assert_eq!(offset_of!(SplatStatic, cov_a), 16);
        assert_eq!(offset_of!(SplatStatic, cov_b), 20);
        assert_eq!(offset_of!(SplatStatic, cov_c), 24);
        assert_eq!(offset_of!(SplatStatic, _spare), 28);
    }

    #[test]
    fn pack2x16float_roundtrip() {
        for v in [
            0.0f32,
            0.01,
            0.5,
            1.0,
            -std::f32::consts::FRAC_1_SQRT_2,
            3.5,
        ] {
            let (a, b) = unpack2x16(pack2x16float(v, -v));
            let tol = (v.abs() * 1e-3).max(1e-4);
            assert!((a - v).abs() < tol, "{v}: got {a}");
            assert!((b + v).abs() < tol, "{v}: got {b}");
        }
    }

    #[test]
    fn pack4x8unorm_matches_wgsl_layout() {
        // x in the low byte, w in the high byte; clamped.
        assert_eq!(pack4x8unorm(1.0, 0.0, 0.0, 0.0), 0x0000_00FF);
        assert_eq!(pack4x8unorm(0.0, 0.0, 0.0, 1.0), 0xFF00_0000);
        assert_eq!(pack4x8unorm(2.0, -1.0, 0.0, 0.0), 0x0000_00FF);
        let mid = pack4x8unorm(0.5, 0.5, 0.5, 0.5);
        assert_eq!(mid & 0xFF, 128);
    }

    #[test]
    fn pack_cloud_roundtrip() {
        let cloud = generate_test_scene(64);
        let packed = pack_cloud(&cloud);
        assert_eq!(packed.len(), 64);
        for (i, p) in packed.iter().enumerate() {
            assert_eq!(p.pos, cloud.positions[i]);
            // Test scene uses identity quats → Σ3 = diag(sx², sy², sz²).
            let s = cloud.scales[i];
            let (xx, yy) = unpack2x16(p.cov_a);
            let (zz, xy) = unpack2x16(p.cov_b);
            let tol = |v: f32| (v.abs() * 2e-3).max(1e-4);
            assert!((xx - s[0] * s[0] * COV_SCALE).abs() < tol(xx), "xx");
            assert!((yy - s[1] * s[1] * COV_SCALE).abs() < tol(yy), "yy");
            assert!((zz - s[2] * s[2] * COV_SCALE).abs() < tol(zz), "zz");
            assert!(xy.abs() < 1e-3, "off-diagonal must vanish for identity");
            let a = ((p.color >> 24) & 0xFF) as f32 / 255.0;
            assert!((a - cloud.opacities[i]).abs() < 1.0 / 255.0 + 1e-6);
        }
    }

    #[test]
    fn pack_cloud_rotated_covariance() {
        // Z+90° quat with anisotropic scales: x/y variances swap, off-diag ~0.
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let cloud = SplatCloud {
            count: 1,
            positions: vec![[0.0, 0.0, 0.0]],
            scales: vec![[0.02, 0.005, 0.005]],
            rotations: vec![[0.0, 0.0, q, q]],
            colors: vec![[1.0, 1.0, 1.0]],
            opacities: vec![1.0],
            sh: Vec::new(),
            sh_degree: 0,
            sh_rot_inv: glam::Mat3::IDENTITY,
            source_path: String::new(),
            total_in_file: 1,
            transform: Default::default(),
        };
        let p = &pack_cloud(&cloud)[0];
        let (xx, yy) = unpack2x16(p.cov_a);
        assert!((xx - 0.005 * 0.005 * COV_SCALE).abs() < 1e-3, "xx got {xx}");
        assert!((yy - 0.02 * 0.02 * COV_SCALE).abs() < 2e-3, "yy got {yy}");
    }

    #[test]
    fn zeroed_static_is_dead_slot() {
        // The zero-fill count mechanism: an all-zero record must unpack to
        // opacity 0 so the sim parks the slot.
        let z = SplatStatic::zeroed();
        assert_eq!((z.color >> 24) & 0xFF, 0);
    }

    fn uniforms_with(dt: f32) -> ParticleUniforms {
        let mut u = ParticleUniforms::zeroed();
        u.delta_time = dt;
        u
    }

    #[test]
    fn driver_explode_latches_and_decays() {
        let mut d = SplatDriver::new();
        let mut u = uniforms_with(1.0 / 60.0);
        u.drop = 1.0;
        d.update(&mut u, [0.0, 1.6, 0.0, 0.0]);
        assert!((u.splat_explode - 1.0).abs() < 1e-6);
        u.drop = 0.0;
        d.update(&mut u, [0.0, 1.6, 0.0, 0.0]);
        let after_one = u.splat_explode;
        assert!(after_one < 1.0 && after_one > 0.9); // τ=0.45s ≫ one frame
        for _ in 0..120 {
            d.update(&mut u, [0.0, 1.6, 0.0, 0.0]);
        }
        assert!(u.splat_explode < 0.02, "envelope must decay out");
    }

    #[test]
    fn driver_yaw_advances_only_with_orbit() {
        let mut d = SplatDriver::new();
        let mut u = uniforms_with(1.0 / 60.0);
        d.update(&mut u, [0.0, 1.6, 0.0, 0.0]);
        assert_eq!(u.cam_yaw, 0.0);
        for _ in 0..60 {
            d.update(&mut u, [0.5, 1.6, 0.0, 0.0]);
        }
        assert!(u.cam_yaw > 0.05, "yaw should accumulate: {}", u.cam_yaw);
    }

    #[test]
    fn driver_centroid_ema_converges_and_focal_clamps() {
        let mut d = SplatDriver::new();
        let mut u = uniforms_with(1.0 / 60.0);
        u.centroid = 1.0;
        for _ in 0..300 {
            d.update(&mut u, [0.0, 1.6, 0.0, 0.0]);
        }
        // Bright timbre pulls focus forward: focal below cam_distance,
        // clamped to the near edge of the scene depth range.
        assert!(u.splat_focal_depth <= u.cam_distance - 0.99);
        assert!(u.splat_focal_depth >= u.cam_distance - 1.21);
    }

    #[test]
    fn driver_distance_clamps_and_buildup_pulls_in() {
        let mut d = SplatDriver::new();
        let mut u = uniforms_with(1.0 / 60.0);
        d.update(&mut u, [0.0, 100.0, 0.0, 0.0]);
        assert!((u.cam_distance - 6.0).abs() < 1e-5);
        u.buildup = 1.0;
        d.update(&mut u, [0.0, 2.0, 0.0, 0.0]);
        assert!((u.cam_distance - 2.0 * 0.85).abs() < 1e-5);
    }
}

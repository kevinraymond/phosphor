// Splat — audio-reactive 3D Gaussian-splat playback (#1800).
//
// Persistent particles: slot i ↔ splat i of the scene in `splat_static`
// (group 2 binding 1, uploaded once per scene; zeroed slots unpack opacity 0
// and park dead — the buffer's zero-fill IS the count mechanism). Nothing is
// emitted or dies; every frame recomputes screen state from the static home
// plus a world-space displacement offset — the only true state, persisted in
// vel_size.xyz even for culled splats.
//
// Camera: scalar orbit camera (u.cam_yaw/pitch/distance/focal, driven CPU-side
// by SplatDriver) using the volumetric ray-marcher basis convention, inverted:
// world → view → perspective divide → NDC in pos_life.xy, view depth in
// pos_life.z (free in the raster path), projected radius in vel_size.w.
//
// Audio: u.splat_explode (drop envelope) throws splats radially outward and a
// spring reforms the scene; onsets scatter a hash-picked subset; u.centroid
// (via u.splat_focal_depth) pulls a depth-of-field focal plane — defocused
// splats render larger and dimmer (energy-conserving circle of confusion),
// so the DoF is drawn by the splatter itself, no post pass.
//
// Blend "oit": color.a carries α · depth-weight · OIT_ALPHA_SCALE; the
// compute raster accumulates Σc·w and Σw and the resolve divides (mode 2).
//
// param(0) audio_reactivity  param(1) explode_amount  param(2) scatter_amount
// param(3) focus_strength    param(4) splat_scale     param(5) opacity_gain
// param(6) idle_breathe      param(7) exposure
// (slots 8–11 are CPU-side camera params — not visible to the sim)

struct SplatStatic {
    pos: vec3f,   // world position, scene normalized       (offset 0)
    color: u32,   // pack4x8unorm(r, g, b, opacity)         (offset 12)
    cov_a: u32,   // pack2x16float(Σxx, Σyy) × COV_SCALE    (offset 16)
    cov_b: u32,   // pack2x16float(Σzz, Σxy) × COV_SCALE    (offset 20)
    cov_c: u32,   // pack2x16float(Σxz, Σyz) × COV_SCALE    (offset 24)
    _spare: u32,  //                                        (offset 28)
}
@group(2) @binding(1) var<storage, read> splat_static: array<SplatStatic>;

// Σ3 is precomputed CPU-side (R·S·SᵀRᵀ is camera-independent; building it
// per frame per splat cost ~9 ms at 2M — live perf finding) and stored
// ×COV_SCALE to keep world-unit σ² values out of f16 subnormals.
const COV_SCALE: f32 = 1024.0;

// Symmetric 3×3 (6 unique entries) times a vector.
fn sym3_mul(d: vec3f, o: vec3f, v: vec3f) -> vec3f {
    // d = (xx, yy, zz), o = (xy, xz, yz)
    return vec3f(
        d.x * v.x + o.x * v.y + o.y * v.z,
        o.x * v.x + d.y * v.y + o.z * v.z,
        o.y * v.x + o.z * v.y + d.z * v.z,
    );
}

// Integer hash (lowbias32) — the lib's fract-sin hash() degrades on GPU for
// idx-scaled arguments (#1856): a band of indices rolls near-constant values
// that pass any threshold every frame. All per-index randomness here uses
// exact u32 mixing (copied from cleave_sim).
fn uhash(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

fn uhash_f(x: u32) -> f32 {
    return f32(uhash(x)) / 4294967296.0;
}

// Uniform direction on the unit sphere from one seed.
fn rand_dir3(seed: u32) -> vec3f {
    let z = uhash_f(seed) * 2.0 - 1.0;
    let phi = uhash_f(seed ^ 0x9e3779b9u) * 6.2831853;
    let r = sqrt(max(0.0, 1.0 - z * z));
    return vec3f(r * cos(phi), z, r * sin(phi));
}


// Fold factor for every accumulated weight: bounds the worst case (3M splats
// collapsing onto one pixel) inside the i32/4096 fixed-point headroom
// (3M × 0.125 = 375k < 524k). The OIT resolve divides it back out; only
// coverage needs the matching COVERAGE_GAIN compensation (resolve shader).
const OIT_ALPHA_SCALE: f32 = 0.125;
// OIT-only opacity accumulation boost. Weighted-average OIT must lean on the
// splats' own weight to build a solid surface, and real captures are very
// low-opacity (median sigmoid ≈ 0.03); the fallback was tuned around an
// opacity_gain of 4.0. The sorted path now owns the shared `opacity_gain`
// default at 1.0 (raw, SuperSplat-faithful), so the OIT branch reintroduces the
// 4× accumulation here — one .pfx default serves both paths without regressing
// the fallback. Applied only when !splat_sorted.
const OIT_OPACITY_BOOST: f32 = 4.0;
// Reform time constant ≈ 0.45 s (matches the drop envelope decay).
const RETURN_RATE: f32 = 2.2;
// World-space displacement clamp — keeps exploded splats recoverable.
const MAX_OFF: f32 = 3.0;
// Final-alpha floor below which a splat cannot contribute a fixed-point
// quantum — skip mark_alive so the raster never sees it.
const ALPHA_CULL: f32 = 1.0 / 512.0;
// Standard 3DGS antialiasing dilation (Kerbl et al.): a fixed px² added to the
// 2D covariance diagonal so every splat — including the degenerate thin
// needles/surfels typical of real captures (anisotropy ~1000:1, sub-pixel
// minor axis) — covers at least ~1px and merges into a surface instead of
// aliasing into speckle. 0.3 is the canonical value; do NOT drop it, it is
// load-bearing for surface smoothness, not fog.
const AA_FLOOR: f32 = 0.3;
// Front-favouring OIT depth weight. Since the resolve averages Σc·w/Σw with no
// sorting, a near-bias lets the nearest surface lead the colour → less
// front+back grey blend. FLOORED (WZ_FLOOR) so far splats still register in the
// framebuffer and contribute coverage/opacity — the weight only reshades the
// average, it must never gate visibility (that is the intrinsic-alpha cull
// below). K≈4 over the [dist−1, dist+1] span gives a ~6× front:back lean; too
// steep risks nearest-splat flicker as the camera orbits.
const OIT_DEPTH_K: f32 = 4.0;
const WZ_FLOOR: f32 = 0.15;

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles {
        return;
    }

    let s = splat_static[idx];
    let base = unpack4x8unorm(s.color); // rgb + opacity
    if base.a < 0.004 {
        // Zero-padded slot past the scene (or a fully transparent splat):
        // park dead, offscreen (image_scatter pattern).
        var dead: Particle;
        dead.pos_life = vec4f(99.0, 99.0, 0.0, 0.0);
        dead.vel_size = vec4f(0.0);
        dead.color = vec4f(0.0);
        dead.flags = vec4f(0.0, u.lifetime, 0.0, 0.0);
        write_particle(idx, dead);
        return;
    }

    let react = param(0u);

    // ---- world-space audio physics (state = vel_size_in[idx].xyz) ----
    let pin = read_particle(idx);
    var off = pin.vel_size.xyz;
    let dir = normalize(s.pos + vec3f(1e-5, 0.0, 0.0)); // radial from scene center

    // Spring home — held open while the drop envelope is hot, so the scene
    // hangs shattered for the phrase and re-coalesces as the envelope decays.
    let env = u.splat_explode * react;
    off *= exp(-RETURN_RATE * u.delta_time * (1.0 - 0.7 * clamp(env, 0.0, 1.0)));
    // Radial throw, per-splat speed variance so the shell has depth.
    off += dir * env * param(1u) * 2.5
        * (0.6 + 0.8 * uhash_f(idx * 0x9e3779b9u)) * u.delta_time;

    // Onset scatter: a hash-picked percentage kicks in a random direction.
    if u.onset > 0.35 {
        let ev = uhash(u.frame_index);
        if uhash_f(idx ^ ev) < param(2u) * react * 0.3 {
            off += rand_dir3(uhash(idx * 747796405u) ^ ev) * 0.05 * (0.5 + u.onset);
        }
    }
    off = clamp(off, vec3f(-MAX_OFF), vec3f(MAX_OFF));

    // Silence idle: non-integrated breathing (never accumulates into state).
    let breath_amp = param(6u) * (0.010 * u.rms + 0.008 * u.beat * react);
    let breath = dir * breath_amp
        * sin(u.time * 1.7 + uhash_f(idx ^ 0x85ebca6bu) * 6.2831853);
    let world = s.pos + off + breath;

    // ---- orbit camera (volumetric basis convention, inverted) ----
    let cy = u.cam_yaw;
    let cp = u.cam_pitch;
    let ro = vec3f(cos(cy) * cos(cp), sin(cp), sin(cy) * cos(cp))
        * max(u.cam_distance, 0.2);
    let fwd = normalize(-ro);
    let right = normalize(cross(vec3f(0.0, 1.0, 0.0), fwd));
    let up = cross(fwd, right);
    let rel = world - ro;
    let t = vec3f(dot(rel, right), dot(rel, up), dot(rel, fwd)); // t.z = view depth

    var age = pin.flags.x + u.delta_time;
    if age > u.lifetime {
        age = 0.0;
    }

    if t.z < 0.05 {
        // Behind the camera: keep the displacement state, invisible to the
        // raster (life 0, not marked alive).
        var back: Particle;
        back.pos_life = vec4f(2.0, 2.0, t.z, 0.0);
        back.vel_size = vec4f(off, 0.0);
        back.color = vec4f(0.0);
        back.flags = vec4f(age, u.lifetime, 0.0, 0.0);
        write_particle(idx, back);
        return;
    }

    let focal = max(u.cam_focal, 0.1);
    let asp = aspect();
    let ndc = vec2f(focal * t.x / (t.z * asp), focal * t.y / t.z);

    // ---- anisotropic EWA footprint (3DGS): Σ2 = (J·W)·Σ3·(J·W)ᵀ ----
    // Σ3 comes precomputed from the upload; the J·W rows are the world-space
    // gradients of the two screen-pixel axes (aspect-symmetric because size
    // is height-relative; the y row is negated for the raster's y-flip).
    let cov_d = vec3f(unpack2x16float(s.cov_a), unpack2x16float(s.cov_b).x); // Σxx, Σyy, Σzz
    let cov_o = vec3f(unpack2x16float(s.cov_b).y, unpack2x16float(s.cov_c)); // Σxy, Σxz, Σyz
    let focal_px = focal * u.resolution.y * 0.5;
    let iz = 1.0 / t.z;
    let jw0 = (right - fwd * (t.x * iz)) * (focal_px * iz);
    let jw1 = (fwd * (t.y * iz) - up) * (focal_px * iz);
    // splat_scale scales σ linearly → covariance by s²; COV_SCALE unfolds.
    let ps4 = max(param(4u), 0.05);
    let s2 = ps4 * ps4 * (1.0 / COV_SCALE);
    let sv0 = sym3_mul(cov_d, cov_o, jw0);
    var caa = dot(jw0, sv0) * s2; // Σ2 in px²
    var cbb = dot(jw1, sv0) * s2;
    var ccc = dot(jw1, sym3_mul(cov_d, cov_o, jw1)) * s2;
    let det_core = max(caa * ccc - cbb * cbb, 1e-8);

    // DoF circle of confusion from the centroid-driven focal plane + AA
    // low-pass, both isotropic covariance adds; energy-conserving via the
    // √det ratio (peak of a 2D gaussian ∝ 1/√det), so blur never brightens.
    // The VISUAL blur is capped: beyond it, heavy defocus keeps dimming
    // (energy comp uses the uncapped determinant) while the footprint stays
    // bounded — far-defocus reads as fade at a fraction of the raster cost.
    let coc = param(3u) * 6.0 * abs(t.z - u.splat_focal_depth) / max(t.z, 0.1);
    let blur = AA_FLOOR + coc * coc;
    let det_full = max((caa + blur) * (ccc + blur) - cbb * cbb, 1e-8);
    let blur_vis = min(blur, 3.0);
    caa += blur_vis;
    ccc += blur_vis;
    var det = max(caa * ccc - cbb * cbb, 1e-8);
    var alpha = base.a * param(5u) * sqrt(det_core / det_full);

    // Footprint radius from the major eigenvalue, cutoff q = 12 (exp(−6)).
    // The cap shrinks the covariance uniformly (keeps eccentricity) instead of
    // clipping. OIT keeps the 8px 3×3-tile scatter bound (load-bearing for the
    // raster); the sorted billboard draw has no tile bound, so it matches
    // SuperSplat's per-axis vmin = min(1024, viewport) — the 8px cap was the
    // pointillist-crust culprit at close zoom (surface splats want 20–100px).
    let lmax = 0.5 * (caa + ccc) + sqrt(max(0.25 * (caa - ccc) * (caa - ccc) + cbb * cbb, 0.0));
    var r_px = sqrt(12.0 * lmax);
    let r_cap = select(8.0, min(1024.0, min(u.resolution.x, u.resolution.y)), u.splat_sorted > 0.5);
    if r_px > r_cap {
        let shrink = (r_cap / r_px) * (r_cap / r_px);
        caa *= shrink;
        cbb *= shrink;
        ccc *= shrink;
        r_px = r_cap;
    }
    // Keep the 2D covariance diagonal f16-safe even after a pathological shrink
    // (SuperSplat floors the minor eigenvalue at 0.1) — else a thin-needle conic
    // overflows f16 and NaN-poisons the sorted blend (the black-square artifact).
    // No-op for normal splats (diagonals already ≥ AA floor 0.3).
    caa = max(caa, 0.05);
    ccc = max(ccc, 0.05);
    det = max(caa * ccc - cbb * cbb, 1e-8);
    // Conic = inverse covariance, packed f16 for the raster (flags.zw).
    let inv_det = 1.0 / det;
    let conic_a = ccc * inv_det;
    let conic_b = -cbb * inv_det;
    let conic_c = caa * inv_det;
    let r_ndc = r_px * 2.0 / u.resolution.y; // raster: radius_px = vel_size.w·H/2

    // ---- OIT weight + obstacle carve + cull ----
    // Front-favouring depth weight (scene spans [dist−1, dist+1]), floored so
    // the far half of the figure still registers coverage instead of averaging
    // toward the background. Near splats lead the OIT colour average.
    let dnorm = clamp((t.z - (u.cam_distance - 1.0)) * 0.5, 0.0, 1.0);
    let wz = WZ_FLOOR + (1.0 - WZ_FLOOR) * exp(-OIT_DEPTH_K * dnorm);
    // Crowd silhouettes punch holes through the captured scene: alpha-carve
    // at the projected position (a 2D clip-space bounce is meaningless for a
    // re-projected 3D point). obstacle_alpha() is 0 when no obstacle is armed.
    let carve = 1.0 - smoothstep(
        u.obstacle_threshold - 0.1,
        u.obstacle_threshold + 0.1,
        obstacle_alpha(ndc)
    );
    // Intrinsic (depth-INDEPENDENT) visibility gates the cull, so the depth
    // weight only reshades the average and can never delete the far half of
    // the figure — that regression came from culling on the weighted a_out.
    // OIT accumulates the 4×-boosted alpha; sorted composites raw opacity.
    let alpha_oit = alpha * OIT_OPACITY_BOOST;
    let vis = alpha_oit * carve * OIT_ALPHA_SCALE;
    let a_out = vis * wz;
    let off_screen = any(abs(ndc) > vec2f(1.0 + r_ndc * 2.0 + 0.05, 1.0 + r_ndc * 2.0 + 0.05));
    // Sorted path composites raw intrinsic alpha, so it must cull on that (not the
    // boosted, ×0.125-folded `vis` the OIT accumulation uses).
    let cull_alpha = select(vis, alpha * carve, u.splat_sorted > 0.5);
    let culled = cull_alpha < ALPHA_CULL || off_screen;

    var p: Particle;
    p.pos_life = vec4f(ndc, t.z, select(1.0, 0.0, culled));
    p.vel_size = vec4f(off, r_ndc); // state ALWAYS persisted, even culled
    // Sorted path composites front-to-back with hardware alpha-over, so it wants
    // the RAW intrinsic alpha (opacity·falloff·carve) — no OIT depth-weight wz
    // and no OIT_ALPHA_SCALE fold. The OIT path keeps a_out.
    let a_sorted = clamp(alpha * carve, 0.0, 1.0);
    p.color = vec4f(
        clamp(base.rgb * (0.5 + param(7u) * 1.5), vec3f(0.0), vec3f(1.0)),
        select(a_out, a_sorted, u.splat_sorted > 0.5)
    );
    // .zw = packed screen conic for the raster's anisotropic branch.
    p.flags = vec4f(
        age,
        u.lifetime,
        bitcast<f32>(pack2x16float(vec2f(conic_a, conic_c))),
        bitcast<f32>(pack2x16float(vec2f(conic_b, 0.0)))
    );
    write_particle(idx, p);
    if !culled {
        mark_alive(idx);
    }
}

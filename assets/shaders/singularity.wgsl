// Singularity — SDF raymarched bioluminescent orb with organic tendrils
// Ported from spectral-senses-old/src/shaders/scenes/singularity.frag

fn map_singularity(p: vec3f, time: f32) -> f32 {
    // Core breathing sphere — bass drives scale
    let core_scale = 1.0 + u.bass * 0.5;
    var core = phosphor_sd_sphere(p, core_scale);

    // Noise displacement — treble controls detail
    let octaves = 2 + i32(u.presence * 1.0);
    let noise_gain = 0.4 + u.presence * 0.15;
    let displacement = phosphor_fbm3(p * 2.0 + time * 0.3, octaves, noise_gain) - 0.5;
    let roughness_param = param(1u);
    let roughness = mix(0.05, 0.35, mix(u.flatness, roughness_param, 0.5));
    core += displacement * roughness;

    // Tendrils: torus rings whose count grows with bandwidth (max 3)
    var tendrils = 1e10;
    let num_tendrils = 1 + i32(u.bandwidth * 2.0);

    for (var i = 0; i < 3; i++) {
        if (i >= num_tendrils) { break; }
        let angle = f32(i) * 3.14159 / f32(max(num_tendrils, 1)) + time * 0.2;
        let ca = cos(angle);
        let sa = sin(angle);

        var tp = p;
        if (i % 3 == 0) {
            tp = vec3f(tp.x * ca - tp.z * sa, tp.y, tp.x * sa + tp.z * ca);
        } else if (i % 3 == 1) {
            tp = vec3f(tp.x, tp.y * ca - tp.z * sa, tp.y * sa + tp.z * ca);
        } else {
            tp = vec3f(tp.x * ca - tp.y * sa, tp.x * sa + tp.y * ca, tp.z);
        }

        let torus_r = 1.2 + u.bandwidth * 0.6 + f32(i) * 0.15;
        let tube_r = 0.08 + u.bass * 0.06;
        let t = phosphor_sd_torus(tp, vec2f(torus_r, tube_r));
        tendrils = min(tendrils, t);
    }

    // Smooth union of core and tendrils
    let k = 0.3 + u.bandwidth * 0.3;
    return phosphor_smin(core, tendrils, k);
}

fn calc_normal(p: vec3f, time: f32) -> vec3f {
    let e = vec2f(0.005, 0.0);
    return normalize(vec3f(
        map_singularity(p + e.xyy, time) - map_singularity(p - e.xyy, time),
        map_singularity(p + e.yxy, time) - map_singularity(p - e.yxy, time),
        map_singularity(p + e.yyx, time) - map_singularity(p - e.yyx, time)
    ));
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = (frag_coord.xy - 0.5 * u.resolution) / u.resolution.y;
    let time = u.time;

    // Camera — params[0] controls distance, zcr adds variation
    let cam_dist_param = param(0u);
    let cam_dist = 3.0 + cam_dist_param * 6.0 + u.zcr * 3.0;
    let rotation_speed = param(3u) * 0.5;
    let cam_angle = time * rotation_speed + u.beat_phase * 6.28318;
    let ro = vec3f(
        cos(cam_angle) * cam_dist,
        sin(time * 0.1) * 0.5,
        sin(cam_angle) * cam_dist
    );
    let look_at = vec3f(0.0);
    let fwd = normalize(look_at - ro);
    let right = normalize(cross(fwd, vec3f(0.0, 1.0, 0.0)));
    let up_vec = cross(right, fwd);
    let rd = normalize(fwd * 1.8 + right * uv.x + up_vec * uv.y);

    // Raymarching
    var t = 0.0;
    var d = 0.0;
    var p = vec3f(0.0);
    var hit = false;

    for (var i = 0; i < 48; i++) {
        p = ro + rd * t;
        d = map_singularity(p, time);
        if (d < 0.003) { hit = true; break; }
        t += d * 0.95;
        if (t > 20.0) { break; }
    }

    var col = vec3f(0.0);

    if (hit) {
        let n = calc_normal(p, time);

        // Lighting
        let light_dir = normalize(vec3f(0.5, 1.0, -0.3));
        let diff = max(dot(n, light_dir), 0.0);
        let spec = pow(max(dot(reflect(-light_dir, n), -rd), 0.0), 16.0 + u.flatness * 48.0);

        // Fresnel for bioluminescent glow
        let fresnel = pow(1.0 - max(dot(n, -rd), 0.0), 3.0);

        // Audio-driven color
        let color_t = length(p) * 0.3 + time * 0.1;
        let base_color = phosphor_audio_palette(color_t, u.centroid, u.beat_phase);

        // Surface
        let smoothness = 1.0 - u.flatness;
        col = base_color * (0.15 + diff * 0.6);
        col += vec3f(1.0) * spec * smoothness * 0.4;
        col += base_color * fresnel * 0.8;

        // Bioluminescent glow — mid drives intensity, param controls boost
        let glow_param = param(2u);
        let glow = 0.3 + u.mid * 0.7 * (0.5 + glow_param);
        col += base_color * glow * 0.2;

        // RMS global brightness
        col *= 0.5 + u.rms * 1.0;

        // Distance fog — rolloff controls density
        let fog_dist = 0.05 + (1.0 - u.rolloff) * 0.15;
        let fog = exp(-t * fog_dist);
        col *= fog;
    }

    // Onset flash
    col += vec3f(1.0) * u.onset * 0.15 * exp(-length(uv) * 2.0);

    // Tonemap
    col = phosphor_aces_tonemap(col);

    return vec4f(col, 1.0);
}

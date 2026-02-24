// Membrane — Bioluminescent ocean surface with iridescent waves
// Ported from spectral-senses-old/src/shaders/scenes/membrane.frag

fn waves(p: vec2f, time: f32) -> f32 {
    var w = 0.0;
    let wave_scale = param(0u) * 2.0 + 0.5;

    // Deep swells — base motion + bass driven
    w += sin(p.x * 0.3 * wave_scale + time * 0.4)
       * sin(p.y * 0.2 * wave_scale + time * 0.3) * (0.4 + u.bass * 1.6);
    w += sin(p.x * 0.15 * wave_scale - time * 0.25 + p.y * 0.1) * (0.3 + u.bass * 1.2);

    // Medium waves — bandwidth complexity
    let med_freq = 0.8 + u.bandwidth * 1.5;
    w += sin(p.x * med_freq + time * 0.6 + p.y * 0.5) * 0.3;
    w += cos(p.y * med_freq * 0.7 - time * 0.5 + p.x * 0.3) * 0.25;

    // Surface ripples — treble driven
    let ripple_freq = 2.0 + u.presence * 4.0;
    let ripple_amp = 0.05 + u.presence * 0.2;
    w += sin(p.x * ripple_freq + time * 2.0) * ripple_amp;
    w += sin(p.y * ripple_freq * 1.3 - time * 1.8 + p.x * 0.5) * ripple_amp * 0.7;

    // FBM for organic texture
    let octaves = 2 + i32(u.presence * 3.0);
    w += phosphor_fbm2(p * 0.5 + time * 0.1, octaves, 0.45) * 0.3;

    return w;
}

fn surface_normal(p: vec2f, time: f32) -> vec3f {
    let eps = 0.05;
    let h = waves(p, time);
    let hx = waves(p + vec2f(eps, 0.0), time);
    let hy = waves(p + vec2f(0.0, eps), time);
    return normalize(vec3f(h - hx, eps * 2.0, h - hy));
}

// Thin-film iridescence (simplified)
fn thin_film(cos_theta: f32, thickness: f32) -> vec3f {
    let delta = thickness * cos_theta * 12.0;
    return vec3f(0.5) + 0.5 * vec3f(
        cos(delta),
        cos(delta + 2.094),
        cos(delta + 4.189)
    );
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = (frag_coord.xy - 0.5 * u.resolution) / u.resolution.y;
    let time = u.time;

    // Camera above looking down at angle — zcr controls height
    let cam_height = 6.0 + u.zcr * 4.0;
    let cam_angle = time * 0.05 + u.beat_phase * 3.14159;
    let ro = vec3f(
        cos(cam_angle) * 3.0,
        cam_height,
        sin(cam_angle) * 3.0 + time * (0.5 + u.flux * 2.0)
    );

    let look_at = ro + vec3f(0.0, -4.0, 5.0);
    let fwd = normalize(look_at - ro);
    let right = normalize(cross(fwd, vec3f(0.0, 1.0, 0.0)));
    let up_vec = cross(right, fwd);
    let rd = normalize(fwd * 1.2 + right * uv.x + up_vec * uv.y);

    var col = vec3f(0.0);

    // Ray-plane intersection (y=0 ocean surface)
    if (rd.y < -0.001) {
        var t = -ro.y / rd.y;
        var hit_pos = ro + rd * t;
        var surface_pos = hit_pos.xz;

        // Wave height and normal
        let h = waves(surface_pos, time);
        let t2 = -(ro.y - h) / rd.y;
        if (t2 > 0.0) { t = t2; }
        hit_pos = ro + rd * t;
        surface_pos = hit_pos.xz;

        let n = surface_normal(surface_pos, time);

        // Lighting
        let light_dir = normalize(vec3f(0.3, 0.8, 0.4));
        let diff = max(dot(n, light_dir), 0.0);
        let ref_dir = reflect(rd, n);
        let spec = pow(max(ref_dir.y, 0.0), 32.0);

        // Fresnel
        let cos_theta = max(dot(n, -rd), 0.0);
        let fresnel = pow(1.0 - cos_theta, 4.0);

        // Base water color
        let deep_color = mix(
            vec3f(0.02, 0.08, 0.2),
            vec3f(0.05, 0.15, 0.1),
            u.centroid * 0.5
        );

        // Bioluminescent glow from centroid
        let bio_glow = phosphor_audio_palette(
            surface_pos.x * 0.05 + time * 0.1, u.centroid, u.beat_phase
        );

        // Thin-film iridescence — flatness controls character
        let irid_param = param(1u);
        let film_thickness = mix(
            0.5 + sin(surface_pos.x * 0.5 + time * 0.3) * 0.3,
            phosphor_noise2(surface_pos * 2.0 + time * 0.5),
            u.flatness * irid_param * 2.0
        );
        let iridescence = thin_film(cos_theta, film_thickness);
        let irid_strength = 0.2 + u.presence * 0.3;

        // Compose surface color
        col = deep_color * (0.3 + diff * 0.5);
        col += iridescence * irid_strength * fresnel;
        col += bio_glow * (0.1 + u.mid * 0.3);
        col += vec3f(0.9, 0.95, 1.0) * spec * 0.5;

        // Caustic sparkle — param controls strength
        let caustic_param = param(3u);
        let caustic = smoothstep(0.65, 0.85,
            phosphor_noise2(surface_pos * 3.0 + time * vec2f(1.2, 0.8)));
        col += bio_glow * caustic * u.presence * 0.8 * (0.5 + caustic_param);

        // Onset splash eruption
        if (u.onset > 0.1) {
            let splash_dist = length(surface_pos - ro.xz);
            let splash = exp(-splash_dist * 0.3) * u.onset;
            col += vec3f(0.8, 0.9, 1.0) * splash * 0.5;
        }

        // RMS brightness
        col *= 0.4 + u.rms * 1.0;

        // Distance fog — param controls density
        let fog_param = param(2u);
        let fog_density = 0.01 + (1.0 - u.rolloff) * 0.04 * (0.5 + fog_param);
        let fog = exp(-t * fog_density);
        let fog_col = mix(
            vec3f(0.01, 0.03, 0.08),
            vec3f(0.02, 0.06, 0.04),
            u.centroid * 0.5
        );
        col = mix(fog_col, col, fog);
    } else {
        // Sky / horizon
        let horizon = exp(-abs(rd.y) * 20.0);
        let sky_col = mix(
            vec3f(0.01, 0.02, 0.05),
            vec3f(0.05, 0.1, 0.15),
            u.centroid * 0.3
        );
        let horizon_col = phosphor_audio_palette(time * 0.02, u.centroid, u.beat_phase) * 0.2;
        col = sky_col + horizon_col * horizon;
    }

    // Tonemap
    col = phosphor_aces_tonemap(col);

    return vec4f(col, 1.0);
}

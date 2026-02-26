// Phosphor — Signature effect: two intertwining luminous Lissajous curves
// with distance-field glow, noise-based sparkle, and feedback trails.
// Inspired by the logo: teal-green + golden-amber light painting on black.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;
    let aspect = res.x / res.y;
    let p = (uv - 0.5) * vec2f(aspect, 1.0);
    let t = u.time;

    // param(0) = flow_speed, param(1) = glow_width, param(2) = sparkle, param(3) = color_shift
    let flow_speed = param(0u) * 2.0 + 0.3;
    let glow_width = param(1u) * 0.025 + 0.004;
    let sparkle_amount = param(2u);
    let color_shift = param(3u);

    // Audio modulation
    let bass_amp = 0.28 + u.bass * 0.15;        // bass drives curve amplitude
    let mid_complex = 2.0 + u.mid * 3.0;         // mid drives harmonic complexity
    let rms_bright = 1.0 + u.rms * 0.6;          // loudness drives brightness

    // Feedback: read previous frame with decay
    let prev = feedback(uv);
    let decay = 0.85;
    var col = prev.rgb * decay;

    // --- Curve A: Teal-green Lissajous ---
    let num_samples_a = 80;
    var min_dist_a = 1.0;
    for (var s = 0; s < num_samples_a; s++) {
        let frac = f32(s) / f32(num_samples_a);
        let angle = frac * 6.28318 * 2.0;

        // Lissajous with audio-driven frequency ratio
        let freq_a = mid_complex;
        let freq_b = mid_complex + 1.0;
        let cx = sin(angle * freq_a + t * flow_speed) * bass_amp;
        let cy = sin(angle * freq_b + t * flow_speed * 0.7) * bass_amp * 0.7;

        // Noise perturbation for organic wobble
        let noise_offset = phosphor_noise2(vec2f(frac * 4.0, t * 0.3)) * 0.04;
        let curve_pt = vec2f(cx + noise_offset, cy + noise_offset * 0.8);

        let d = length(p - curve_pt);
        min_dist_a = min(min_dist_a, d);
    }

    // --- Curve B: Golden-amber Lissajous (offset phase) ---
    var min_dist_b = 1.0;
    for (var s = 0; s < num_samples_a; s++) {
        let frac = f32(s) / f32(num_samples_a);
        let angle = frac * 6.28318 * 2.0;

        let freq_a = mid_complex + 0.5;
        let freq_b = mid_complex + 1.5;
        let cx = sin(angle * freq_a + t * flow_speed * 0.8 + 1.5) * bass_amp * 0.9;
        let cy = sin(angle * freq_b + t * flow_speed * 0.6 + 2.0) * bass_amp * 0.65;

        let noise_offset = phosphor_noise2(vec2f(frac * 4.0 + 10.0, t * 0.25)) * 0.04;
        let curve_pt = vec2f(cx + noise_offset, cy + noise_offset * 0.8);

        let d = length(p - curve_pt);
        min_dist_b = min(min_dist_b, d);
    }

    // Glow from distance field — inverse-square soft falloff
    let glow_a = glow_width / (min_dist_a * min_dist_a + glow_width);
    let glow_b = glow_width / (min_dist_b * min_dist_b + glow_width);

    // Colors: teal-green and golden-amber, with color_shift crossfade
    let hue_a = color_shift * 0.3;
    let color_a = phosphor_palette(
        0.3 + hue_a,
        vec3f(0.2, 0.6, 0.6), // teal base
        vec3f(0.2, 0.3, 0.3), // teal amplitude
        vec3f(1.0, 1.0, 1.0),
        vec3f(0.0, 0.33, 0.67)
    );
    let color_b = phosphor_palette(
        0.7 + hue_a,
        vec3f(0.7, 0.5, 0.2), // amber base
        vec3f(0.3, 0.2, 0.1), // amber amplitude
        vec3f(1.0, 1.0, 1.0),
        vec3f(0.0, 0.1, 0.2)
    );

    // Add curve glow to accumulated color
    col += color_a * glow_a * rms_bright * 1.8;
    col += color_b * glow_b * rms_bright * 1.8;

    // Sparkle: noise-based bright pinpoints, triggered by beat + onset
    let sparkle_base = sparkle_amount * 0.5;
    let sparkle_beat = sparkle_amount * u.beat * 2.0;
    let sparkle_onset = sparkle_amount * u.onset * 0.8;
    let sparkle_str = sparkle_base + sparkle_beat + sparkle_onset;
    if sparkle_str > 0.01 {
        let spark_noise = phosphor_noise2(p * 40.0 + vec2f(t * 2.0, t * 1.7));
        let spark_threshold = 1.0 - sparkle_str * 0.15;
        if spark_noise > spark_threshold {
            let spark_intensity = (spark_noise - spark_threshold) / (1.0 - spark_threshold);
            let spark_color = mix(color_a, color_b, phosphor_noise2(p * 10.0));
            col += spark_color * spark_intensity * spark_intensity * 3.0;
        }
    }

    // Beat phase pulse: gentle overall brightness modulation
    let beat_pulse = 1.0 + sin(u.beat_phase * 6.28318) * 0.08 * u.beat_strength;
    col *= beat_pulse;

    // Onset flash: brief white-ish brightening
    col += vec3f(0.15, 0.12, 0.10) * u.onset * 0.5;

    // Prevent feedback blowout
    col = min(col, vec3f(1.5));

    return vec4f(col, 1.0);
}

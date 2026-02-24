// Plasma Wave — 2D plasma with audio-reactive palette and motion

fn plasma(uv: vec2f, t: f32) -> f32 {
    let speed = param(0u) * 2.0 + 0.5;
    let scale = param(1u) * 4.0 + 2.0;
    let distortion = param(3u) * 2.0;

    var v = 0.0;
    v += sin(uv.x * scale + t * speed);
    v += sin((uv.y * scale + t * speed * 0.7) * 0.7);
    v += sin((uv.x * scale * 0.5 + uv.y * scale * 0.5 + t * speed * 0.5) * 1.1);
    v += sin(length(uv * scale) * 1.5 - t * speed * 1.2);

    // Audio distortion: bass warps the field
    let warp = phosphor_noise2(uv * 3.0 + t * 0.2) * u.bass * distortion;
    v += warp;

    return v * 0.25 + 0.5;
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = (frag_coord.xy - 0.5 * u.resolution) / u.resolution.y;
    let t = u.time;

    let p = plasma(uv, t);

    // Audio-driven palette
    let palette_shift = param(2u);
    var col = phosphor_audio_palette(p + palette_shift, u.centroid, u.beat_phase);

    // Bass pulses intensity
    col *= 0.7 + u.bass * 0.6 + u.rms * 0.3;

    // Presence adds shimmer
    let shimmer = phosphor_noise2(uv * 20.0 + t * 3.0) * u.presence * 0.15;
    col += vec3f(shimmer);

    // Onset flash
    col += vec3f(1.0) * u.onset * 0.2 * exp(-length(uv) * 3.0);

    // Tonemap
    col = phosphor_aces_tonemap(col);

    return vec4f(col, 1.0);
}

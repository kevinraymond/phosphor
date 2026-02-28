// Flux background shader — subtle dark smoke with feedback trails.
// Particles do the heavy lifting; background provides atmosphere and trail persistence.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;
    let aspect = u.resolution.x / u.resolution.y;

    // Feedback (previous frame) with decay
    let decay = param(0u); // trail_decay
    let prev = feedback(uv);

    // Subtle UV distortion on feedback — gives smoke-like smearing
    let flow_param = param(1u); // flow_intensity
    let warp_str = 0.002 + flow_param * 0.003 + u.bass * 0.002;
    let warp_x = phosphor_noise2(p * 3.0 + vec2f(u.time * 0.1, 0.0)) - 0.5;
    let warp_y = phosphor_noise2(p * 3.0 + vec2f(0.0, u.time * 0.08)) - 0.5;
    let warped_uv = uv + vec2f(warp_x, warp_y) * warp_str;
    let warped_prev = feedback(clamp(warped_uv, vec2f(0.001), vec2f(0.999)));

    // Blend warped and straight feedback for smoother trails
    var trail = mix(clamp(prev.rgb, vec3f(0.0), vec3f(1.0)), clamp(warped_prev.rgb, vec3f(0.0), vec3f(1.0)), 0.6) * decay;
    trail = min(trail, vec3f(0.50));

    // Ambient smoke glow based on audio
    let density_param = param(3u);
    let ambient_n = phosphor_noise2(p * 2.0 + vec2f(u.time * 0.05));
    let ambient = ambient_n * ambient_n * (0.08 + 0.12 * density_param) * (0.4 + u.rms * 1.2);

    // Color: muted blue-green ambient
    let color_shift = param(2u);
    let hue = 0.55 + color_shift * 0.3 + u.centroid * 0.15;
    let r = abs(hue * 6.0 - 3.0) - 1.0;
    let g = 2.0 - abs(hue * 6.0 - 2.0);
    let b = 2.0 - abs(hue * 6.0 - 4.0);
    let ambient_color = clamp(vec3f(r, g, b), vec3f(0.0), vec3f(1.0)) * ambient;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = (trail + ambient_color) * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

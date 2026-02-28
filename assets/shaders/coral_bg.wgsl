// Coral background shader — dark canvas with Turing pattern hints and organic feedback.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    let decay = param(0u);
    let prev = feedback(uv);
    let trail = prev.rgb * decay;

    // Subtle visualization of Turing-like pattern as background guide
    let scale_param = param(1u);
    let k = 6.0 + scale_param * 10.0 + u.bass * 4.0;
    let pi = 3.14159;

    // Hexagonal Turing pattern approximation (3 wave directions at 60° intervals)
    let d1 = cos(k * p.x);
    let d2 = cos(k * (-0.5 * p.x + 0.866 * p.y));
    let d3 = cos(k * (-0.5 * p.x - 0.866 * p.y));
    let turing = (d1 + d2 + d3) / 3.0;

    let line_dist = abs(turing);
    let line_glow = exp(-line_dist * 8.0) * 0.015 * scale_param * u.rms;

    // Warm organic tones
    let warmth = param(3u);
    let bg_color = vec3f(0.12, 0.06, 0.03) * line_glow * (1.0 + warmth);

    let result = min(trail + bg_color, vec3f(1.5));
    let alpha = max(result.r, max(result.g, result.b)) * 2.0;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

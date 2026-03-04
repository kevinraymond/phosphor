// Cymatics background shader — dark plate with subtle nodal line hints and feedback.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    let decay = param(0u);
    let prev = feedback(uv);
    let trail = prev.rgb * decay;

    // Subtle visualization of Chladni pattern as background guide
    let scale_param = param(1u);
    let n = 2.0 + u.bass * 3.0;
    let m = 3.0 + u.mid * 4.0;
    let pi = 3.14159;
    let chladni = cos(n * pi * p.x) * cos(m * pi * p.y) - cos(m * pi * p.x) * cos(n * pi * p.y);
    let line_dist = abs(chladni);
    let line_glow = exp(-line_dist * 15.0) * 0.02 * scale_param * u.rms;
    let bg_color = vec3f(0.08, 0.12, 0.2) * line_glow;

    let result = min(trail + bg_color, vec3f(1.5));
    let alpha = max(result.r, max(result.g, result.b)) * 2.0;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

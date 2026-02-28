// Cymatics background shader — dark plate with subtle nodal line hints and feedback.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    let decay = param(0u);
    let prev = feedback(uv);
    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.0)) * decay;
    // Hard cap — prevents additive accumulation blowout with screen emitter
    trail = min(trail, vec3f(0.50));

    // Visualization of Chladni nodal line pattern
    let scale_param = param(1u);
    let n = 2.0 + u.bass * 3.0;
    let m = 3.0 + u.mid * 4.0;
    let pi = 3.14159;
    let chladni = cos(n * pi * p.x) * cos(m * pi * p.y) - cos(m * pi * p.x) * cos(n * pi * p.y);
    let line_dist = abs(chladni);
    let line_glow = exp(-line_dist * 12.0) * (0.12 + 0.15 * scale_param) * (0.5 + u.rms * 1.0);
    let bg_color = vec3f(0.15, 0.2, 0.35) * line_glow;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = (trail + bg_color) * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

// Cymatics background shader — dark plate with nodal line hints, rotation, and feedback.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    let decay = param(0u);
    let prev = feedback(uv);
    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.0)) * decay;
    // Raised cap for brighter pattern visibility
    trail = min(trail, vec3f(0.85));

    // Rotation param
    let rotation_param = param(4u);
    let rot_angle = u.time * rotation_param * 0.5;
    let c = cos(rot_angle);
    let s = sin(rot_angle);
    var rp = vec2f(p.x * c - p.y * s, p.x * s + p.y * c);

    // Symmetry folding
    let symmetry_param = param(5u);
    if symmetry_param > 0.25 {
        rp.x = abs(rp.x);
    }
    if symmetry_param > 0.75 {
        rp.y = abs(rp.y);
    }

    // Visualization of Chladni nodal line pattern
    let scale_param = param(1u);
    let glow_param = param(6u);
    let n = 2.0 + u.bass * 3.0;
    let m = 3.0 + u.mid * 4.0;
    let pi = 3.14159;
    let chladni = cos(n * pi * rp.x) * cos(m * pi * rp.y) - cos(m * pi * rp.x) * cos(n * pi * rp.y);
    let line_dist = abs(chladni);
    let glow_mult = 0.5 + glow_param * 1.5;
    let line_glow = exp(-line_dist * 18.0) * (0.18 + 0.22 * scale_param) * (0.5 + u.rms * 1.0) * glow_mult;
    let bg_color = vec3f(0.15, 0.2, 0.35) * line_glow;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = (trail + bg_color) * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

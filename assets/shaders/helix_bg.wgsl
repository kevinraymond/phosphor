// Helix background shader — dark field with magnetic field line hints and feedback trails.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    // Feedback decay
    let decay = param(0u);
    let prev = feedback(uv);

    // Upward drift in feedback — particle beams flow upward
    let drift_uv = uv + vec2f(0.0, -0.001);
    let drifted = feedback(clamp(drift_uv, vec2f(0.001), vec2f(0.999)));
    var trail = mix(clamp(prev.rgb, vec3f(0.0), vec3f(1.0)), clamp(drifted.rgb, vec3f(0.0), vec3f(1.0)), 0.3) * decay;
    trail = min(trail, vec3f(0.60));

    // Magnetic field line visualization (vertical streaks)
    let field_vis = param(1u) * 0.15;
    let field_lines = sin(p.x * 20.0 + u.time * 0.2) * 0.5 + 0.5;
    let field_glow = field_lines * field_vis * (0.3 + u.bass * 0.7) * exp(-abs(p.y + 0.3) * 1.5);
    let field_color = vec3f(0.15, 0.25, 0.5) * field_glow;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = (trail + field_color) * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

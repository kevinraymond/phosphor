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
    let trail = mix(prev.rgb, drifted.rgb, 0.3) * decay;

    // Subtle magnetic field line visualization (vertical streaks)
    let field_vis = param(1u) * 0.015;
    let field_lines = sin(p.x * 20.0 + u.time * 0.2) * 0.5 + 0.5;
    let field_glow = field_lines * field_vis * u.bass * exp(-abs(p.y + 0.3) * 2.0);
    let field_color = vec3f(0.1, 0.2, 0.5) * field_glow;

    let result = min(trail + field_color, vec3f(1.5));
    let alpha = max(result.r, max(result.g, result.b)) * 2.0;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

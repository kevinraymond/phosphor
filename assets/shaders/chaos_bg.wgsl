// Chaos background shader — dark void with attractor trail persistence.
// Long-decay feedback preserves particle trails creating attractor visualization.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;

    // Feedback with long decay for trail persistence
    let decay = param(0u); // trail_decay
    let prev = feedback(uv);

    // Very subtle radial distortion on feedback — creates spiral motion in trails
    let p = uv * 2.0 - 1.0;
    let r = length(p);
    let twist = 0.001 * (1.0 + u.bass * 0.5);
    let angle = twist / (r + 0.1);
    let ca = cos(angle);
    let sa = sin(angle);
    let rotated_p = vec2f(p.x * ca - p.y * sa, p.x * sa + p.y * ca);
    let warped_uv = rotated_p * 0.5 + 0.5;
    let warped_prev = feedback(clamp(warped_uv, vec2f(0.001), vec2f(0.999)));

    var trail = mix(clamp(prev.rgb, vec3f(0.0), vec3f(1.0)), clamp(warped_prev.rgb, vec3f(0.0), vec3f(1.0)), 0.3) * decay;
    // Hard cap — attractor trails need persistence for visualization
    trail = min(trail, vec3f(0.60));
    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = trail * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

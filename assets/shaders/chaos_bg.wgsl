// Chaos background shader — dark void with attractor trail persistence.
// Clean feedback preserves particle trails to trace the attractor shape.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;

    // Clean feedback — no distortion, preserves tight attractor lines
    let decay = param(0u); // trail_decay
    let prev = feedback(uv);

    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.0)) * decay;
    trail = min(trail, vec3f(0.55));

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 0.5;
    let result = trail * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

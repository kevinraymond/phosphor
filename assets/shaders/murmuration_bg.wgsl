// Murmuration background shader — dark sky with subtle feedback trails.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;

    let decay = param(0u);
    let prev = feedback(uv);
    let trail = prev.rgb * decay;

    // Very subtle dark blue ambient — twilight sky
    let p = uv * 2.0 - 1.0;
    let sky = vec3f(0.01, 0.015, 0.03) * (1.0 - abs(p.y) * 0.3);

    let result = min(trail + sky, vec3f(1.5));
    let alpha = max(result.r, max(result.g, result.b)) * 2.0;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

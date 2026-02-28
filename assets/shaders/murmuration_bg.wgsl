// Murmuration background shader — dark sky with subtle feedback trails.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;

    let decay = param(0u);
    let prev = feedback(uv);
    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.0)) * decay;
    trail = min(trail, vec3f(0.40));

    // Dark blue ambient — twilight sky
    let p = uv * 2.0 - 1.0;
    let sky = vec3f(0.02, 0.03, 0.06) * (1.0 - abs(p.y) * 0.3);

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = (trail + sky) * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

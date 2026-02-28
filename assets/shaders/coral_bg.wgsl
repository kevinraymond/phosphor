// Coral background shader — organic dark substrate with feedback trails.
// Turing pattern hints in the background, particles do the heavy lifting.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    let decay = param(0u);
    let prev = feedback(uv);
    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.0)) * decay;
    trail = min(trail, vec3f(0.50));

    // Organic background glow — warm bioluminescent substrate
    let warmth = param(3u);
    let n = phosphor_noise2(p * 3.0 + vec2f(u.time * 0.05, 0.0));
    let ambient = n * n * (0.08 + 0.12 * warmth) * (0.4 + u.rms * 1.2);
    let bg_color = vec3f(0.25, 0.12, 0.06) * ambient;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.5;
    let result = (trail + bg_color) * max(vignette, 0.0);
    let alpha = max(result.r, max(result.g, result.b)) * 1.5;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

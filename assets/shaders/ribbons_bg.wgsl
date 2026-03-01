// Ribbons background shader — dark canvas with subtle feedback trails.
// Particles and trails do the visual work; background provides persistence and atmosphere.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    // Feedback with decay
    let decay = param(0u); // trail_decay
    let prev = feedback(uv);

    // Very subtle UV warp on feedback for ribbon-like smearing
    let flow_param = param(1u);
    let warp_str = 0.001 + flow_param * 0.002;
    let warp_x = phosphor_noise2(p * 2.0 + vec2f(u.time * 0.08, 0.0)) - 0.5;
    let warp_y = phosphor_noise2(p * 2.0 + vec2f(0.0, u.time * 0.06)) - 0.5;
    let warped_uv = uv + vec2f(warp_x, warp_y) * warp_str;
    let warped_prev = feedback(clamp(warped_uv, vec2f(0.001), vec2f(0.999)));

    let trail = mix(prev.rgb, warped_prev.rgb, 0.4) * decay;
    let result = min(trail, vec3f(0.8));
    let alpha = max(result.r, max(result.g, result.b)) * 2.0;
    return vec4f(result, clamp(alpha, 0.0, 1.0));
}

// Ribbons background shader — subtle feedback ghosting behind trail ribbons.
// Trails provide the main persistence (16-point strips); feedback just adds soft glow.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    // Feedback provides subtle ghosting only — trails do the heavy lifting
    let base_decay = param(0u); // trail_decay (controls ghost persistence)
    let loudness = clamp(u.rms + u.bass * 0.3, 0.0, 1.0);
    let decay = base_decay * (1.0 - loudness * 0.2);
    let prev = feedback(uv);

    // Subtle UV warp on feedback for organic smearing
    let flow_param = param(1u);
    let warp_str = 0.001 + flow_param * 0.002;
    let warp_x = phosphor_noise2(p * 2.0 + vec2f(u.time * 0.08, 0.0)) - 0.5;
    let warp_y = phosphor_noise2(p * 2.0 + vec2f(0.0, u.time * 0.06)) - 0.5;
    let warped_uv = uv + vec2f(warp_x, warp_y) * warp_str;
    let warped_prev = feedback(clamp(warped_uv, vec2f(0.001), vec2f(0.999)));

    let trail = mix(prev.rgb, warped_prev.rgb, 0.4) * decay;

    // Hard ceiling — with 128K additive trail draws per frame,
    // feedback must stay very low or it accumulates to solid color
    let clamped = min(trail, vec3f(0.06));

    // Vignette helps edges stay dark
    let center = uv - 0.5;
    let vign = 1.0 - dot(center, center) * 1.2;
    let result = clamped * max(vign, 0.0);

    let alpha = clamp(max(result.r, max(result.g, result.b)) * 2.0, 0.0, 1.0);
    return vec4f(result, alpha);
}

// Veil background pass — feedback with UV-warped motion blur for flowing silk trails.
// Particles render on top via LoadOp::Load.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;

    // Subtle UV warp: shift feedback UVs based on noise for organic motion blur
    let flow_speed = param(0u);
    let warp_time = u.time * flow_speed * 0.3;
    let warp_x = phosphor_noise2(uv * 3.0 + vec2f(warp_time, 0.0)) - 0.5;
    let warp_y = phosphor_noise2(uv * 3.0 + vec2f(0.0, warp_time * 0.7)) - 0.5;
    let wind = param(2u); // wind_strength
    let warp_amount = 0.003 + wind * 0.005;
    let warped_uv = uv + vec2f(warp_x, warp_y) * warp_amount;

    // Read previous frame with warped UVs
    let prev = feedback(warped_uv);

    // Decay from param (trail_decay = param(1))
    let decay = param(1u);

    // Apply decay
    var col = prev.rgb * decay;

    // Hard cap — very tight for 6000 additive particles covering the whole screen
    col = min(col, vec3f(0.35));

    // Gentle horizontal gradient tint based on color_shift param
    let color_shift = param(3u);
    let tint = mix(vec3f(1.0), vec3f(0.95, 0.98, 1.05), color_shift * 0.1);
    col *= tint;

    // Soft vignette
    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.2;
    col *= max(vignette, 0.0);

    let alpha = clamp(max(col.r, max(col.g, col.b)) * 2.0, 0.0, 1.0);
    return vec4f(col, alpha);
}

// Nova background pass — dripping trail decay + ground glow for fireworks.
// Particles render on top via LoadOp::Load.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;

    // Shift feedback UVs slightly downward for "dripping" trail effect
    let drip_amount = 0.002 + param(1u) * 0.001; // gravity_strength influences drip
    let drip_uv = uv + vec2f(0.0, drip_amount);

    // Read previous frame with drip offset
    let prev = feedback(drip_uv);

    // Decay from param (trail_decay = param(0))
    let decay = param(0u);

    // Apply decay
    var col = prev.rgb * decay;

    // Hard cap to prevent feedback runaway
    col = min(col, vec3f(1.5));

    // Ground glow — warm light pooling at screen bottom from fallen sparks
    let ground_y = 1.0 - uv.y; // 0 at top, 1 at bottom
    let ground_glow = smoothstep(0.7, 1.0, ground_y) * 0.03;
    let glow_color = vec3f(1.0, 0.6, 0.2) * ground_glow * u.rms;
    col += glow_color;

    // Subtle sparkle noise overlay
    let sparkle_param = param(3u);
    let sparkle_n = phosphor_hash2(uv * 800.0 + vec2f(u.time * 10.0, 0.0));
    let sparkle = step(1.0 - sparkle_param * 0.005, sparkle_n) * 0.02;
    col += vec3f(sparkle);

    let alpha = clamp(max(col.r, max(col.g, col.b)) * 2.0, 0.0, 1.0);
    return vec4f(col, alpha);
}

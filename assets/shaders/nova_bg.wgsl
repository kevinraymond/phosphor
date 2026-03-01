// Nova background pass — dripping trail decay + ground glow for fireworks.
// Particles render on top via LoadOp::Load.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;

    // Shift feedback UVs slightly downward for "dripping" trail effect
    let drip_amount = 0.002 + param(1u) * 0.001;
    let drip_uv = uv + vec2f(0.0, drip_amount);
    let prev = feedback(drip_uv);
    let decay = param(0u);
    var col = prev.rgb * decay;
    col = min(col, vec3f(1.5));

    // Ground glow — warm light pooling at screen bottom from bouncing sparks
    let ground_y = 1.0 - uv.y;
    let ground_glow = smoothstep(0.65, 1.0, ground_y) * 0.06;
    let glow_pulse = 1.0 + u.beat * 0.5;
    let glow_color = vec3f(1.0, 0.55, 0.15) * ground_glow * (u.rms + 0.1) * glow_pulse;
    col += glow_color;

    // Sky ambient — very faint to frame the fireworks
    let sky_fade = smoothstep(0.6, 0.0, uv.y);
    col += vec3f(0.005, 0.003, 0.01) * sky_fade;

    // Sparkle overlay
    let sparkle_param = param(3u);
    let sparkle_n = phosphor_hash2(uv * 800.0 + vec2f(u.time * 10.0, 0.0));
    let sparkle = step(1.0 - sparkle_param * 0.005, sparkle_n) * 0.02;
    col += vec3f(sparkle);

    let alpha = clamp(max(col.r, max(col.g, col.b)) * 2.0, 0.0, 1.0);
    return vec4f(col, alpha);
}

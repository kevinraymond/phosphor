// Spirograph background shader — feedback with subtle rotational UV warp + center glow.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    let decay = param(0u); // trail_decay

    // Subtle rotational UV warp on feedback for spiraling trail persistence
    let drift_param = param(4u);
    let center = uv - 0.5;
    let r = length(center);
    let warp_angle = 0.002 + drift_param * 0.004;
    let c = cos(warp_angle);
    let s = sin(warp_angle);
    let warped_center = vec2f(center.x * c - center.y * s, center.x * s + center.y * c);
    let warped_uv = warped_center + 0.5;

    let prev = feedback(clamp(warped_uv, vec2f(0.001), vec2f(0.999)));
    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.2)) * decay;

    // Soft center glow (hub of the spirograph)
    let center_glow = exp(-r * r * 8.0) * 0.02 * (1.0 + u.rms * 0.5);
    let glow_color = vec3f(0.3, 0.4, 0.6) * center_glow;

    // Vignette
    let vign = 1.0 - dot(center, center) * 0.8;
    let result = (trail + glow_color) * max(vign, 0.0);

    let alpha = clamp(max(result.r, max(result.g, result.b)) * 2.0, 0.0, 1.0);
    return vec4f(result, alpha);
}

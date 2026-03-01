// Vortex background pass — gravitational lensing with chromatic aberration + dark void.
// Particles render on top via LoadOp::Load.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;
    let aspect = res.x / res.y;

    let centered = (uv - 0.5) * vec2f(aspect, 1.0);
    let dist = length(centered);
    let dir = select(normalize(centered), vec2f(0.0, 1.0), dist < 0.001);

    let lensing = param(4u);
    let event_horizon = param(2u) * 0.15;

    // Radial pull and tangential twist
    let radial_strength = lensing * 0.02 / (dist + 0.1);
    let tangent = vec2f(-dir.y, dir.x);
    let tangential_strength = lensing * 0.01 / (dist + 0.15);

    let base_warp = dir * radial_strength + tangent * tangential_strength;
    let warp_scale = vec2f(1.0 / aspect, 1.0);

    // Chromatic aberration: separate R/G/B offsets near center
    let ca_strength = lensing * 0.003 / (dist + 0.15);
    let ca_offset = dir * ca_strength * warp_scale;

    let warp_r = uv + (base_warp * 1.08) * warp_scale + ca_offset;
    let warp_g = uv + base_warp * warp_scale;
    let warp_b = uv + (base_warp * 0.92) * warp_scale - ca_offset;

    // Sample each channel separately for chromatic split
    let prev_r = feedback(warp_r).r;
    let prev_g = feedback(warp_g).g;
    let prev_b = feedback(warp_b).b;

    let decay = param(0u);
    var col = vec3f(prev_r, prev_g, prev_b) * decay;
    col = min(col, vec3f(1.5));

    // Dark void at center
    let void_mask = smoothstep(event_horizon * 0.5, event_horizon * 1.5, dist);
    col *= void_mask;

    // Accretion disk glow: bright ring just outside event horizon
    let inner = event_horizon * 1.2;
    let outer = event_horizon * 3.5;
    let disk_ring = smoothstep(inner, inner + 0.02, dist) * (1.0 - smoothstep(outer - 0.02, outer, dist));
    // Temperature coloring: white-hot inner → orange → red outer
    let temp = 1.0 - smoothstep(inner, outer, dist);
    let disk_col = mix(vec3f(0.8, 0.2, 0.05), vec3f(0.5, 0.6, 1.0), temp * temp);
    col += disk_col * disk_ring * 0.04 * (u.rms + 0.15);

    // Beat flash near event horizon
    if u.beat > 0.5 {
        let flash_ring = smoothstep(inner, inner + 0.01, dist) * (1.0 - smoothstep(inner + 0.01, inner + 0.04, dist));
        col += vec3f(0.6, 0.7, 1.0) * flash_ring * 0.15;
    }

    let vig_center = uv - 0.5;
    let vignette = 1.0 - dot(vig_center, vig_center) * 1.8;
    col *= max(vignette, 0.0);

    let alpha = clamp(max(col.r, max(col.g, col.b)) * 2.0, 0.0, 1.0);
    return vec4f(col, alpha);
}

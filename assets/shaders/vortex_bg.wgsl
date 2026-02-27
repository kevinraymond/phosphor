// Vortex background pass — gravitational lensing UV distortion + dark void at center.
// Particles render on top via LoadOp::Load.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;
    let aspect = res.x / res.y;

    // Center-relative coordinates (aspect-corrected)
    let centered = (uv - 0.5) * vec2f(aspect, 1.0);
    let dist = length(centered);
    let dir = select(normalize(centered), vec2f(0.0, 1.0), dist < 0.001);

    // Lensing param
    let lensing = param(4u);
    let event_horizon = param(2u) * 0.15; // scaled to reasonable screen radius

    // Radial pull: warp UVs toward center (stronger near center)
    let radial_strength = lensing * 0.02 / (dist + 0.1);
    // Tangential twist: rotate UVs around center
    let tangent = vec2f(-dir.y, dir.x);
    let tangential_strength = lensing * 0.01 / (dist + 0.15);

    let warp = (dir * radial_strength + tangent * tangential_strength) * vec2f(1.0 / aspect, 1.0);
    let warped_uv = uv + warp;

    // Read previous frame with lensed UVs
    let prev = feedback(warped_uv);

    // Decay from param (trail_decay = param(0))
    let decay = param(0u);

    // Apply decay
    var col = prev.rgb * decay;

    // Hard cap to prevent feedback runaway
    col = min(col, vec3f(1.5));

    // Dark void at center — event horizon swallows light
    let void_mask = smoothstep(event_horizon * 0.5, event_horizon * 1.5, dist);
    col *= void_mask;

    // Subtle accretion disk glow at mid-range
    let disk_glow = smoothstep(event_horizon * 1.2, event_horizon * 2.0, dist)
                  * (1.0 - smoothstep(event_horizon * 2.0, event_horizon * 5.0, dist));
    let disk_color = vec3f(0.3, 0.15, 0.6) * disk_glow * 0.02 * u.rms;
    col += disk_color;

    // Vignette
    let vig_center = uv - 0.5;
    let vignette = 1.0 - dot(vig_center, vig_center) * 1.8;
    col *= max(vignette, 0.0);

    let alpha = clamp(max(col.r, max(col.g, col.b)) * 2.0, 0.0, 1.0);
    return vec4f(col, alpha);
}

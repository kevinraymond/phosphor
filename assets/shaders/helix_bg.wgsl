// Helix background — dark field with feedback warped along dipole field lines.
// Particle trails are the main visual; background just carries and stretches them.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;
    let aspect = res.x / res.y;

    // Particle-space coordinates (match sim shader)
    let p = vec2f((uv.x * 2.0 - 1.0) * aspect, 1.0 - uv.y * 2.0);

    // Dipole field direction (same center as sim shader: y = -0.7)
    let dp = p - vec2f(0.0, -0.7);
    let r2 = dot(dp, dp) + 0.02;
    let r4 = r2 * r2;
    let bx = 2.0 * dp.x * dp.y / r4;
    let by = (dp.y * dp.y - dp.x * dp.x) / r4;
    let b_mag = sqrt(bx * bx + by * by) + 0.001;
    let b_dir = vec2f(bx, by) / b_mag;

    // Feedback with field-aligned warping — trails flow along field lines
    let decay = param(0u);
    let warp_amount = param(2u); // beam_spread controls warp intensity
    let warp_str = 0.004 * (0.3 + warp_amount * 0.7) * (0.6 + u.rms * 0.4);
    // Convert field direction from particle space to UV space
    let warp = vec2f(b_dir.x / aspect, -b_dir.y) * warp_str;

    // Chromatic aberration along field direction
    let ca = vec2f(b_dir.x / aspect, -b_dir.y) * 0.0012;

    let uv_r = clamp(uv + warp * 1.1 + ca, vec2f(0.001), vec2f(0.999));
    let uv_g = clamp(uv + warp, vec2f(0.001), vec2f(0.999));
    let uv_b = clamp(uv + warp * 0.9 - ca, vec2f(0.001), vec2f(0.999));

    var trail = vec3f(
        feedback(uv_r).r,
        feedback(uv_g).g,
        feedback(uv_b).b
    ) * decay;
    trail = min(trail, vec3f(3.0));

    // Very subtle field line hints — barely visible, just breaks up pure black
    let psi = dp.x / r2; // dipole stream function
    let field_vis = param(1u) * 0.025; // field_strength controls hint intensity
    let line_hint = pow(abs(cos(psi * 10.0)), 30.0);
    let dist_fade = exp(-sqrt(r2) * 0.4);
    let hint = line_hint * dist_fade * field_vis * (0.3 + u.bass * 0.5);
    let hint_color = vec3f(0.05, 0.10, 0.22) * hint;

    // Beat flash on field lines (subtle)
    var flash = vec3f(0.0);
    if u.beat > 0.5 {
        flash = vec3f(0.08, 0.15, 0.25) * line_hint * dist_fade * 0.06;
    }

    // Vignette
    let vc = uv - 0.5;
    let vignette = 1.0 - dot(vc, vc) * 1.6;
    let result = (trail + hint_color + flash) * max(vignette, 0.0);

    let alpha = clamp(max(result.r, max(result.g, result.b)) * 2.0, 0.0, 1.0);
    return vec4f(result, alpha);
}

// Orbital Trails background pass.
// Layers feedback_test's spinning dot on top of spectral_eye's feedback decay.
// Particles render on top via LoadOp::Load.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let res = u.resolution;
    let uv = frag_coord.xy / res;
    let aspect = res.x / res.y;
    let centered = (uv - 0.5) * vec2f(aspect, 1.0);

    // param(0) = trail_length (dot trails, from feedback_test)
    // param(1) = trail_decay (particle/overall feedback, from spectral_eye)
    let dot_decay = param(0u);
    let particle_decay = param(1u);

    // Read and decay previous frame (covers both dot trails and particle trails)
    let prev = feedback(uv);
    var col = prev.rgb * max(dot_decay, particle_decay);

    // Hard cap to prevent additive-blend feedback runaway
    col = min(col, vec3f(1.5));

    // --- Feedback Test spinning dot ---
    let t = u.time;
    let speed = 1.5 + u.bass * 2.0;
    let radius = 0.2 + u.mid * 0.15;
    let dot_pos = vec2f(cos(t * speed), sin(t * speed)) * radius;
    let dist = length(centered - dot_pos);

    let dot_size = 0.03 + u.rms * 0.02;
    let glow = exp(-dist * dist / (dot_size * dot_size));

    let hue = t * 0.3 + u.centroid;
    let dot_col = vec3f(
        0.5 + 0.5 * cos(hue * 6.28),
        0.5 + 0.5 * cos((hue + 0.33) * 6.28),
        0.5 + 0.5 * cos((hue + 0.67) * 6.28),
    );

    let new_dot = dot_col * glow * (1.5 + u.onset * 3.0);

    // Max blend: brighter of decayed trails or fresh dot
    let combined = max(col, new_dot);

    // Subtle vignette to darken edges
    let vig = uv - 0.5;
    let vignette = 1.0 - dot(vig, vig) * 1.5;

    return vec4f(combined * max(vignette, 0.0), 1.0);
}

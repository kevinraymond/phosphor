// Murmuration background shader — twilight sky gradient with subtle feedback trails.
// Brighter sky provides contrast for dark alpha-blended bird silhouettes.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let p = uv * 2.0 - 1.0;

    // Feedback trails — subtle motion ghosting
    let decay = param(0u);
    let prev = feedback(uv);
    var trail = clamp(prev.rgb, vec3f(0.0), vec3f(1.0)) * decay;
    trail = min(trail, vec3f(0.20));

    // Twilight sky gradient: deep blue top, warm amber horizon
    let sky_top = vec3f(0.025, 0.035, 0.09);
    let sky_mid = vec3f(0.045, 0.04, 0.06);
    let sky_low = vec3f(0.09, 0.055, 0.03);
    let y = uv.y;
    var sky: vec3f;
    if y > 0.5 {
        sky = mix(sky_mid, sky_top, (y - 0.5) * 2.0);
    } else {
        sky = mix(sky_low, sky_mid, y * 2.0);
    }

    // Color mode: shift sky warmth
    let color_mode = param(3u);
    sky = mix(sky, sky.brg * 0.8, (color_mode - 0.5) * 0.3);

    // Audio-reactive sky glow
    sky *= 1.0 + u.rms * 0.3;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.2;
    let result = (trail + sky) * max(vignette, 0.0);
    return vec4f(result, 1.0);
}

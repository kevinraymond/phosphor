// Murmur background shader — twilight sky gradient, no feedback.
// Dark bird silhouettes get contrast from the bright sky alone.
// No trails needed: 40K particles provide frame-to-frame continuity.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;

    // Twilight sky gradient: deep blue top, warm amber horizon (brighter than before)
    let sky_top = vec3f(0.06, 0.08, 0.18);
    let sky_mid = vec3f(0.10, 0.08, 0.12);
    let sky_low = vec3f(0.18, 0.11, 0.06);
    let y = uv.y;
    var sky: vec3f;
    if y > 0.5 {
        sky = mix(sky_mid, sky_top, (y - 0.5) * 2.0);
    } else {
        sky = mix(sky_low, sky_mid, y * 2.0);
    }

    // Color mode: shift sky warmth (param 2 now, was param 3)
    let color_mode = param(2u);
    sky = mix(sky, sky.brg * 0.8, (color_mode - 0.5) * 0.3);

    // Audio-reactive sky glow
    sky *= 1.0 + u.rms * 0.3;

    let center = uv - 0.5;
    let vignette = 1.0 - dot(center, center) * 1.2;
    let result = sky * max(vignette, 0.0);
    return vec4f(result, 1.0);
}

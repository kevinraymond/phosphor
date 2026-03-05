// Morph background — dark field with subtle feedback fade.

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;

    // Feedback from previous frame
    let prev = feedback(uv);
    let decay = 0.85;

    // Dark background with radial gradient
    let center = uv - 0.5;
    let dist = length(center);
    let bg = mix(vec3f(0.015, 0.015, 0.025), vec3f(0.005, 0.005, 0.01), dist * 1.5);

    // Blend feedback with background
    let faded = prev.rgb * decay;
    let color = max(faded, bg);

    return vec4f(color, 1.0);
}

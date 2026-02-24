// Phosphor palette library — ported from spectral-senses-old palette.glsl

// iq cosine palette: a + b * cos(2*pi*(c*t + d))
fn phosphor_palette(t: f32, a: vec3f, b: vec3f, c: vec3f, d: vec3f) -> vec3f {
    return a + b * cos(6.28318 * (c * t + d));
}

// Audio-driven palette interpolating warm (low centroid) to cool (high centroid)
fn phosphor_audio_palette(t: f32, cent: f32, phase: f32) -> vec3f {
    // Warm palette: deep reds, oranges, golds
    let a_warm = vec3f(0.5, 0.3, 0.2);
    let b_warm = vec3f(0.5, 0.3, 0.2);
    let c_warm = vec3f(1.0, 0.7, 0.4);
    let d_warm = vec3f(0.0, 0.15, 0.2);

    // Cool palette: electric blues, cyans, magentas
    let a_cool = vec3f(0.3, 0.4, 0.6);
    let b_cool = vec3f(0.4, 0.3, 0.4);
    let c_cool = vec3f(1.0, 1.0, 1.2);
    let d_cool = vec3f(0.0, 0.33, 0.67);

    let a = mix(a_warm, a_cool, cent);
    let b = mix(b_warm, b_cool, cent);
    let c = mix(c_warm, c_cool, cent);
    let d = mix(d_warm, d_cool, cent);

    return a + b * cos(6.28318 * (c * t + d + phase));
}

// Simplified bioluminescent palette
fn phosphor_bioluminescent(t: f32, cent: f32) -> vec3f {
    let warm = vec3f(1.0, 0.3, 0.05) * (0.5 + 0.5 * sin(t * 3.14159));
    let cool = vec3f(0.1, 0.7, 1.0) * (0.5 + 0.5 * cos(t * 3.14159 + 1.0));
    return mix(warm, cool, cent);
}

// Hue shift via Rodrigues rotation around (1,1,1) axis
fn phosphor_hue_shift(c: vec3f, shift: f32) -> vec3f {
    let a = shift * 6.28318;
    let co = cos(a);
    let si = sin(a);
    let k = vec3f(0.57735);
    return c * co + cross(k, c) * si + k * dot(k, c) * (1.0 - co);
}

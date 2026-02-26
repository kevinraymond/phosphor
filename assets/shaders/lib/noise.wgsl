// Phosphor noise library — ported from spectral-senses-old sdf-lib.glsl

// Hash without sin (3D)
fn phosphor_hash3(p_in: vec3f) -> f32 {
    var p = fract(p_in * 0.3183099 + 0.1);
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

// Hash without sin (2D)
fn phosphor_hash2(p: vec2f) -> f32 {
    let p3 = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    let p3b = p3 + dot(p3, p3 + vec3f(33.33));
    return fract((p3b.x + p3b.y) * p3b.z);
}

// Value noise 3D
fn phosphor_noise3(p: vec3f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    return mix(
        mix(
            mix(phosphor_hash3(i + vec3f(0.0, 0.0, 0.0)),
                phosphor_hash3(i + vec3f(1.0, 0.0, 0.0)), u.x),
            mix(phosphor_hash3(i + vec3f(0.0, 1.0, 0.0)),
                phosphor_hash3(i + vec3f(1.0, 1.0, 0.0)), u.x),
            u.y
        ),
        mix(
            mix(phosphor_hash3(i + vec3f(0.0, 0.0, 1.0)),
                phosphor_hash3(i + vec3f(1.0, 0.0, 1.0)), u.x),
            mix(phosphor_hash3(i + vec3f(0.0, 1.0, 1.0)),
                phosphor_hash3(i + vec3f(1.0, 1.0, 1.0)), u.x),
            u.y
        ),
        u.z
    );
}

// Value noise 2D
fn phosphor_noise2(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let a = phosphor_hash2(i);
    let b = phosphor_hash2(i + vec2f(1.0, 0.0));
    let c = phosphor_hash2(i + vec2f(0.0, 1.0));
    let d = phosphor_hash2(i + vec2f(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// FBM 3D (max 8 octaves, loop uses break)
fn phosphor_fbm3(p: vec3f, octaves: i32, gain: f32) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i = 0; i < 8; i++) {
        if (i >= octaves) { break; }
        val += amp * phosphor_noise3(p * freq);
        freq *= 2.0;
        amp *= gain;
    }
    return val;
}

// FBM 2D
fn phosphor_fbm2(p: vec2f, octaves: i32, gain: f32) -> f32 {
    var val = 0.0;
    var amp = 0.5;
    var freq = 1.0;
    for (var i = 0; i < 8; i++) {
        if (i >= octaves) { break; }
        val += amp * phosphor_noise2(p * freq);
        freq *= 2.0;
        amp *= gain;
    }
    return val;
}

// Phosphor noise library — gradient noise (Perlin-style)
// Uses integer hashing for proper distribution at grid points,
// and gradient dot products to eliminate grid-aligned artifacts.

// Integer hash (xorshift + Knuth multiplicative)
fn phosphor_ihash(n_in: u32) -> u32 {
    var n = n_in;
    n = n ^ (n >> 16u);
    n = n * 2654435769u;
    n = n ^ (n >> 16u);
    return n;
}

// Hash without sin (3D → scalar) — kept for Worley noise etc.
fn phosphor_hash3(p_in: vec3f) -> f32 {
    var p = fract(p_in * 0.3183099 + 0.1);
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

// Hash without sin (2D → scalar) — kept for Worley noise etc.
fn phosphor_hash2(p: vec2f) -> f32 {
    let p3 = fract(vec3f(p.x, p.y, p.x) * 0.1031);
    let p3b = p3 + dot(p3, p3 + vec3f(33.33));
    return fract((p3b.x + p3b.y) * p3b.z);
}

// 2D gradient hash — integer hash → unit vector
fn phosphor_grad2(p: vec2f) -> vec2f {
    let n = phosphor_ihash(bitcast<u32>(i32(p.x)) + phosphor_ihash(bitcast<u32>(i32(p.y))));
    let a = f32(n) * (6.2831853 / 4294967296.0);
    return vec2f(cos(a), sin(a));
}

// 3D gradient hash — integer hash → uniform point on unit sphere
fn phosphor_grad3(p: vec3f) -> vec3f {
    let n = phosphor_ihash(
        bitcast<u32>(i32(p.x)) + phosphor_ihash(
            bitcast<u32>(i32(p.y)) + phosphor_ihash(
                bitcast<u32>(i32(p.z))
            )
        )
    );
    let a = f32(n) * (6.2831853 / 4294967296.0);
    let n2 = phosphor_ihash(n);
    let z = f32(n2) / 4294967295.0 * 2.0 - 1.0;
    let r = sqrt(max(0.0, 1.0 - z * z));
    return vec3f(r * cos(a), r * sin(a), z);
}

// Gradient noise 3D
fn phosphor_noise3(p: vec3f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let ga = dot(phosphor_grad3(i + vec3f(0.0, 0.0, 0.0)), f - vec3f(0.0, 0.0, 0.0));
    let gb = dot(phosphor_grad3(i + vec3f(1.0, 0.0, 0.0)), f - vec3f(1.0, 0.0, 0.0));
    let gc = dot(phosphor_grad3(i + vec3f(0.0, 1.0, 0.0)), f - vec3f(0.0, 1.0, 0.0));
    let gd = dot(phosphor_grad3(i + vec3f(1.0, 1.0, 0.0)), f - vec3f(1.0, 1.0, 0.0));
    let ge = dot(phosphor_grad3(i + vec3f(0.0, 0.0, 1.0)), f - vec3f(0.0, 0.0, 1.0));
    let gf = dot(phosphor_grad3(i + vec3f(1.0, 0.0, 1.0)), f - vec3f(1.0, 0.0, 1.0));
    let gg = dot(phosphor_grad3(i + vec3f(0.0, 1.0, 1.0)), f - vec3f(0.0, 1.0, 1.0));
    let gh = dot(phosphor_grad3(i + vec3f(1.0, 1.0, 1.0)), f - vec3f(1.0, 1.0, 1.0));

    return mix(
        mix(mix(ga, gb, u.x), mix(gc, gd, u.x), u.y),
        mix(mix(ge, gf, u.x), mix(gg, gh, u.x), u.y),
        u.z
    ) * 0.5 + 0.5;
}

// Gradient noise 2D
fn phosphor_noise2(p: vec2f) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let va = dot(phosphor_grad2(i + vec2f(0.0, 0.0)), f - vec2f(0.0, 0.0));
    let vb = dot(phosphor_grad2(i + vec2f(1.0, 0.0)), f - vec2f(1.0, 0.0));
    let vc = dot(phosphor_grad2(i + vec2f(0.0, 1.0)), f - vec2f(0.0, 1.0));
    let vd = dot(phosphor_grad2(i + vec2f(1.0, 1.0)), f - vec2f(1.0, 1.0));

    return mix(mix(va, vb, u.x), mix(vc, vd, u.x), u.y) * 0.5 + 0.5;
}

// FBM 3D (max 8 octaves)
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

// FBM 2D (max 8 octaves)
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

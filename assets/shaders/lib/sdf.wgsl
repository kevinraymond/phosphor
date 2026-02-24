// Phosphor SDF library — ported from spectral-senses-old sdf-lib.glsl

// Primitives
fn phosphor_sd_sphere(p: vec3f, r: f32) -> f32 {
    return length(p) - r;
}

fn phosphor_sd_torus(p: vec3f, t: vec2f) -> f32 {
    let q = vec2f(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

fn phosphor_sd_box(p: vec3f, b: vec3f) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3f(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn phosphor_sd_cylinder(p: vec3f, h: f32, r: f32) -> f32 {
    let d = abs(vec2f(length(p.xz), p.y)) - vec2f(r, h);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2f(0.0)));
}

fn phosphor_sd_plane(p: vec3f, n: vec3f, h: f32) -> f32 {
    return dot(p, n) + h;
}

// Boolean operations
fn phosphor_op_union(d1: f32, d2: f32) -> f32 {
    return min(d1, d2);
}

fn phosphor_op_subtract(d1: f32, d2: f32) -> f32 {
    return max(-d1, d2);
}

fn phosphor_op_intersect(d1: f32, d2: f32) -> f32 {
    return max(d1, d2);
}

// Smooth min (polynomial)
fn phosphor_smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// Smooth subtraction
fn phosphor_smax(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 - 0.5 * (b + a) / k, 0.0, 1.0);
    return mix(b, -a, h) + k * h * (1.0 - h);
}

// Domain operations
fn phosphor_op_rep(p: vec3f, c: vec3f) -> vec3f {
    return ((p + 0.5 * c) % c) - 0.5 * c;
}

fn phosphor_op_rep_lim(p: vec3f, c: f32, l: vec3f) -> vec3f {
    return p - c * clamp(round(p / c), -l, l);
}

// Twist around Y axis
fn phosphor_op_twist(p: vec3f, k: f32) -> vec3f {
    let c = cos(k * p.y);
    let s = sin(k * p.y);
    let xz = vec2f(c * p.x - s * p.z, s * p.x + c * p.z);
    return vec3f(xz.x, p.y, xz.y);
}

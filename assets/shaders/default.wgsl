struct PhosphorUniforms {
    time: f32,
    delta_time: f32,
    resolution: vec2f,
    bass: f32,
    mid: f32,
    treble: f32,
    rms: f32,
    phase: f32,
    onset: f32,
    centroid: f32,
    flux: f32,
    flatness: f32,
    rolloff: f32,
    bandwidth: f32,
    zcr: f32,
    params: array<vec4f, 4>,
    feedback_decay: f32,
    frame_index: f32,
}

@group(0) @binding(0) var<uniform> u: PhosphorUniforms;
@group(0) @binding(1) var prev_frame: texture_2d<f32>;
@group(0) @binding(2) var prev_sampler: sampler;

fn param(i: u32) -> f32 {
    return u.params[i / 4u][i % 4u];
}

fn feedback(uv: vec2f) -> vec4f {
    return textureSample(prev_frame, prev_sampler, uv);
}

// Simple plasma shader
fn plasma(uv: vec2f, t: f32) -> f32 {
    var v = 0.0;
    let speed = param(0u) * 2.0 + 0.5;
    let scale = param(1u) * 4.0 + 2.0;

    v += sin(uv.x * scale + t * speed);
    v += sin((uv.y * scale + t * speed * 0.7) * 0.7);
    v += sin((uv.x * scale * 0.5 + uv.y * scale * 0.5 + t * speed * 0.5) * 1.1);
    v += sin(length(uv * scale) * 1.5 - t * speed * 1.2);

    return v * 0.25 + 0.5;
}

fn pal(t: f32) -> vec3f {
    let a = vec3f(0.5, 0.5, 0.5);
    let b = vec3f(0.5, 0.5, 0.5);
    let c = vec3f(1.0, 1.0, 1.0);
    let d = vec3f(0.0, 0.33, 0.67);
    return a + b * cos(6.28318 * (c * t + d));
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = (frag_coord.xy - 0.5 * u.resolution) / u.resolution.y;
    let t = u.time;

    let p = plasma(uv, t);

    let color_shift = u.centroid * 0.5 + u.phase * 0.3;
    var col = pal(p + color_shift);

    col *= 0.7 + u.bass * 0.6 + u.rms * 0.3;

    col += vec3f(1.0) * u.onset * 0.2 * exp(-length(uv) * 3.0);

    return vec4f(col, 1.0);
}

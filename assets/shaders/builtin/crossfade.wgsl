// Fullscreen crossfade shader for dissolve transitions.
// Blends texture A (outgoing) with texture B (incoming) using mix(A, B, progress).

@group(0) @binding(0) var tex_a: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex_b: texture_2d<f32>;
@group(0) @binding(3) var<uniform> params: vec4f; // x = progress

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Fullscreen triangle
    var out: VertexOutput;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4f(x, y, 0.0, 1.0);
    out.uv = vec2f((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let a = textureSample(tex_a, samp, in.uv);
    let b = textureSample(tex_b, samp, in.uv);
    let t = params.x;
    return mix(a, b, t);
}

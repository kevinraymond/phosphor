// Blit shader — copies an HDR texture to the surface with simple passthrough.

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

@fragment
fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {
    return textureSample(src_texture, src_sampler, uv);
}

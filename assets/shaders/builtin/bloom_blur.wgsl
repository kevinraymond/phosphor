// 9-tap separable Gaussian blur for bloom.
// Direction controlled via uniform (horizontal or vertical pass).

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct BlurParams {
    direction: vec2f,  // (1/w, 0) for H or (0, 1/h) for V
    _pad: vec2f,
}
@group(0) @binding(2) var<uniform> blur: BlurParams;

@fragment
fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {
    // 9-tap Gaussian weights (sigma ~= 4)
    let weights = array<f32, 5>(0.2270270, 0.1945946, 0.1216216, 0.0540541, 0.0162162);

    var result = textureSample(src_texture, src_sampler, uv) * weights[0];

    for (var i = 1; i < 5; i++) {
        let offset = blur.direction * f32(i);
        result += textureSample(src_texture, src_sampler, uv + offset) * weights[i];
        result += textureSample(src_texture, src_sampler, uv - offset) * weights[i];
    }

    return result;
}

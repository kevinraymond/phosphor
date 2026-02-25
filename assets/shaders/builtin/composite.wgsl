// Layer compositor — blends a foreground layer onto a background accumulator.
// Operates in HDR space (before tonemapping).

struct CompositeUniforms {
    blend_mode: u32,
    opacity: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var bg_texture: texture_2d<f32>;
@group(0) @binding(1) var bg_sampler: sampler;
@group(0) @binding(2) var fg_texture: texture_2d<f32>;
@group(0) @binding(3) var fg_sampler: sampler;
@group(0) @binding(4) var<uniform> comp: CompositeUniforms;

// --- Blend mode functions (operate per-channel in HDR) ---

fn blend_normal(bg: vec3f, fg: vec3f) -> vec3f {
    return fg;
}

fn blend_add(bg: vec3f, fg: vec3f) -> vec3f {
    return bg + fg;
}

fn blend_multiply(bg: vec3f, fg: vec3f) -> vec3f {
    return bg * fg;
}

fn blend_screen(bg: vec3f, fg: vec3f) -> vec3f {
    return bg + fg - bg * fg;
}

fn blend_overlay_ch(bg: f32, fg: f32) -> f32 {
    if bg < 0.5 {
        return 2.0 * bg * fg;
    } else {
        return 1.0 - 2.0 * (1.0 - bg) * (1.0 - fg);
    }
}

fn blend_overlay(bg: vec3f, fg: vec3f) -> vec3f {
    return vec3f(
        blend_overlay_ch(bg.x, fg.x),
        blend_overlay_ch(bg.y, fg.y),
        blend_overlay_ch(bg.z, fg.z),
    );
}

fn blend_soft_light_ch(bg: f32, fg: f32) -> f32 {
    if fg < 0.5 {
        return bg - (1.0 - 2.0 * fg) * bg * (1.0 - bg);
    } else {
        let d = select(sqrt(bg), ((16.0 * bg - 12.0) * bg + 4.0) * bg, bg < 0.25);
        return bg + (2.0 * fg - 1.0) * (d - bg);
    }
}

fn blend_soft_light(bg: vec3f, fg: vec3f) -> vec3f {
    return vec3f(
        blend_soft_light_ch(bg.x, fg.x),
        blend_soft_light_ch(bg.y, fg.y),
        blend_soft_light_ch(bg.z, fg.z),
    );
}

fn blend_difference(bg: vec3f, fg: vec3f) -> vec3f {
    return abs(bg - fg);
}

@fragment
fn fs_main(@location(0) uv: vec2f) -> @location(0) vec4f {
    let bg = textureSample(bg_texture, bg_sampler, uv);
    let fg = textureSample(fg_texture, fg_sampler, uv);

    var blended: vec3f;
    switch comp.blend_mode {
        case 1u: { blended = blend_add(bg.rgb, fg.rgb); }
        case 2u: { blended = blend_multiply(bg.rgb, fg.rgb); }
        case 3u: { blended = blend_screen(bg.rgb, fg.rgb); }
        case 4u: { blended = blend_overlay(bg.rgb, fg.rgb); }
        case 5u: { blended = blend_soft_light(bg.rgb, fg.rgb); }
        case 6u: { blended = blend_difference(bg.rgb, fg.rgb); }
        default: { blended = blend_normal(bg.rgb, fg.rgb); }
    }

    // Mix with opacity: lerp between background and blended result
    let result = mix(bg.rgb, blended, comp.opacity * fg.a);
    return vec4f(result, max(bg.a, fg.a * comp.opacity));
}

// Instanced quad particle renderer.
// Vertex-pulling from storage buffer, 6 vertices per particle (two triangles).
// Supports: soft circle glow (mode 0), static sprite (mode 1), animated sprite (mode 2).

struct Particle {
    pos_life: vec4f,
    vel_size: vec4f,
    color: vec4f,
    flags: vec4f,
}

struct RenderUniforms {
    resolution: vec2f,
    time: f32,
    render_mode: u32,   // 0=circle, 1=sprite, 2=animated sprite
    sprite_cols: u32,
    sprite_rows: u32,
    sprite_frames: u32,
    _pad: u32,
}

@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(1) var<uniform> ru: RenderUniforms;

@group(1) @binding(0) var sprite_tex: texture_2d<f32>;
@group(1) @binding(1) var sprite_samp: sampler;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
    @location(1) quad_uv: vec2f,
    @location(2) @interpolate(flat) sprite_frame: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let p = particles[instance_index];
    var out: VertexOutput;

    // Dead particles: collapse to degenerate triangle (GPU clips trivially)
    if p.pos_life.w <= 0.0 {
        out.position = vec4f(0.0, 0.0, 0.0, 1.0);
        out.color = vec4f(0.0);
        out.quad_uv = vec2f(0.0);
        out.sprite_frame = 0u;
        return out;
    }

    // Quad corners: 2 triangles from 6 vertices
    // 0,1,2, 2,3,0 pattern
    var corner: vec2f;
    let vi = vertex_index % 6u;
    switch vi {
        case 0u: { corner = vec2f(-1.0, -1.0); }
        case 1u: { corner = vec2f( 1.0, -1.0); }
        case 2u: { corner = vec2f( 1.0,  1.0); }
        case 3u: { corner = vec2f( 1.0,  1.0); }
        case 4u: { corner = vec2f(-1.0,  1.0); }
        case 5u: { corner = vec2f(-1.0, -1.0); }
        default: { corner = vec2f(0.0); }
    }

    let size = p.vel_size.w;
    // Correct aspect ratio so particles are circular
    let aspect = ru.resolution.x / ru.resolution.y;
    let offset = corner * size * vec2f(1.0 / aspect, 1.0);

    let pos = p.pos_life.xy + offset;
    out.position = vec4f(pos, 0.0, 1.0);
    out.color = p.color;
    out.quad_uv = corner;

    // Compute sprite frame from particle age/lifetime
    if ru.render_mode == 2u && ru.sprite_frames > 0u {
        let age = p.flags.x;
        let lifetime = p.flags.y;
        let life_frac = clamp(age / max(lifetime, 0.001), 0.0, 0.999);
        out.sprite_frame = u32(life_frac * f32(ru.sprite_frames));
    } else {
        out.sprite_frame = 0u;
    }

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    if ru.render_mode == 0u {
        // Mode 0: Soft circle glow (original behavior)
        let dist = length(in.quad_uv);
        let glow = exp(-dist * dist * 3.0);
        if glow < 0.01 {
            discard;
        }
        return vec4f(in.color.rgb * glow, in.color.a * glow);
    } else {
        // Mode 1/2: Sprite texture sampling
        // Map quad_uv from [-1,1] to [0,1]
        let uv01 = in.quad_uv * 0.5 + 0.5;

        // Compute tile UV within atlas
        let col = in.sprite_frame % ru.sprite_cols;
        let row = in.sprite_frame / ru.sprite_cols;
        let tile_w = 1.0 / f32(ru.sprite_cols);
        let tile_h = 1.0 / f32(ru.sprite_rows);
        let atlas_uv = vec2f(
            (f32(col) + uv01.x) * tile_w,
            (f32(row) + (1.0 - uv01.y)) * tile_h  // flip Y for texture coords
        );

        let tex_color = textureSample(sprite_tex, sprite_samp, atlas_uv);
        let final_color = tex_color * in.color;

        if final_color.a < 0.01 {
            discard;
        }
        return final_color;
    }
}

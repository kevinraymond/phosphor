// Sorted anisotropic 3DGS billboard renderer (Splat #1800).
//
// Draws each splat as a screen-aligned quad in back-to-front sorted order
// (splat_sorted_indices, produced by the depth counting sort) with hardware
// SrcAlpha / OneMinusSrcAlpha blending. Because a single non-indexed instanced
// draw emits primitives in ascending instance_index and the ROP blends in
// primitive order, far→near instance order yields exact front-to-back src-over
// — real occlusion, matching SuperSplat/PlayCanvas (unlike the order-independent
// weighted-average OIT resolve this replaces).
//
// The anisotropic EWA conic (inverse 2D covariance) is computed once per splat
// by splat_sim.wgsl and packed f16 into flags.zw; the fragment evaluates the
// Gaussian exp(-0.5·q) exactly as the compute rasterizer does.

struct RenderUniforms {
    resolution: vec2f,
    time: f32,
    render_mode: u32,
    sprite_cols: u32,
    sprite_rows: u32,
    sprite_frames: u32,
    frame_index: u32,
    trail_length: u32,
    trail_width: f32,
    _pad: vec2f,
}

@group(0) @binding(0) var<storage, read> pos_life: array<vec4f>;
@group(0) @binding(1) var<storage, read> vel_size: array<vec4f>;
@group(0) @binding(2) var<storage, read> color: array<vec4f>;
@group(0) @binding(3) var<storage, read> flags: array<vec4f>;
@group(0) @binding(4) var<uniform> ru: RenderUniforms;
@group(0) @binding(5) var<storage, read> sorted_indices: array<u32>;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) col: vec4f,
    @location(1) @interpolate(flat) conic: vec3f, // A, B, C (1/px²)
    @location(2) local_px: vec2f,                 // pixel offset from center (x-right, y-down)
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let idx = sorted_indices[instance_index];
    let pl = pos_life[idx];
    let vs = vel_size[idx];
    var out: VertexOutput;

    // Park offscreen if not alive. The scatter only places marked-alive indices,
    // so this is defensive against a stale slot, never the common path.
    if pl.w <= 0.0 {
        out.position = vec4f(2.0, 2.0, 2.0, 1.0);
        out.col = vec4f(0.0);
        out.conic = vec3f(0.0);
        out.local_px = vec2f(0.0);
        return out;
    }

    // Quad corners: 2 triangles from 6 vertices.
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

    let r_ndc = vs.w;
    let aspect = ru.resolution.x / max(ru.resolution.y, 1.0);
    let offset_ndc = corner * r_ndc * vec2f(1.0 / aspect, 1.0);
    out.position = vec4f(pl.xy + offset_ndc, 0.0, 1.0);

    // Pixel-space offset from the splat center. Both axes span ±r_px (aspect is
    // absorbed by the NDC offset above). y is negated so the space is x-right /
    // y-DOWN, matching the pixel convention the sim baked into the conic — else
    // anisotropic splats render mirrored.
    let r_px = r_ndc * ru.resolution.y * 0.5;
    out.local_px = vec2f(corner.x, -corner.y) * r_px;

    // Screen-space conic = inverse 2D covariance, packed f16 by splat_sim:
    //   flags.z = pack2x16float(A, C),  flags.w = pack2x16float(B, 0)
    let ac = unpack2x16float(bitcast<u32>(flags[idx].z));
    let b = unpack2x16float(bitcast<u32>(flags[idx].w)).x;
    out.conic = vec3f(ac.x, b, ac.y);
    out.col = color[idx];
    return out;
}

const EXP4: f32 = 0.018315639; // exp(-4), the SuperSplat falloff renorm constant

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let dx = in.local_px.x;
    let dy = in.local_px.y;
    let q = in.conic.x * dx * dx + 2.0 * in.conic.y * dx * dy + in.conic.z * dy * dy;
    // NaN/Inf-safe cutoff at q = 8 (SuperSplat's 2.83σ quad extent, A = q/8). A
    // poisoned conic (f16 overflow on a degenerate splat) yields NaN q, which
    // fails this test and discards — no black-square blend poisoning.
    if !(q <= 8.0) {
        discard;
    }
    // SuperSplat's renormalized falloff: normExp(A) = (exp(-4A) − e⁻⁴)/(1 − e⁻⁴)
    // with A = q/8, so exp(-4A) = exp(-q/2). Reaches exactly 0 at the q=8 edge —
    // no hard exp(-6) ring, which was the per-splat speckle.
    let norm = (exp(-0.5 * q) - EXP4) / (1.0 - EXP4);
    let a = min(in.col.a * norm, 1.0);
    if !(a >= 1.0 / 512.0) {
        discard;
    }
    // Straight (non-premultiplied) alpha; hardware (SrcAlpha, 1−SrcAlpha) blend
    // gives src-over. col.a is the raw intrinsic alpha (sim, sorted branch).
    return vec4f(in.col.rgb, a);
}

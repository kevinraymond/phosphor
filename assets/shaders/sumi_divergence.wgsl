// Sumi — divergence of the velocity field.
// Non-feedback pass; input0 = velocity (PREVIOUS frame, a forward prev_input).
// Divergence of that field is what the pressure solve removes so the velocity the
// velocity pass projects becomes incompressible. h = 1 grid units (half-difference).
//
// prev_frame is the 1x1 placeholder here (non-feedback pass), so the working size
// comes from the velocity texture, not textureDimensions(prev_frame).
@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let dims = vec2f(textureDimensions(input0_tex));
    let texel = 1.0 / dims;
    let uv = frag_coord.xy / dims;

    let vl = input0(uv - vec2f(texel.x, 0.0)).xy;
    let vr = input0(uv + vec2f(texel.x, 0.0)).xy;
    let vb = input0(uv - vec2f(0.0, texel.y)).xy;
    let vt = input0(uv + vec2f(0.0, texel.y)).xy;

    let div = 0.5 * ((vr.x - vl.x) + (vt.y - vb.y));
    return vec4f(div, 0.0, 0.0, 1.0);
}

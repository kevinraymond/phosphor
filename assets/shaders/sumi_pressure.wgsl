// Sumi — one Jacobi iteration of the pressure Poisson solve ∇²p = div.
// feedback() = the previous iteration's pressure (ping-ponged in-encoder by the
// pass's `iterations` count); input0 = divergence, fixed across the whole loop.
// On the first iteration of a frame, feedback() is last frame's converged pressure,
// so the solve warm-starts and only needs a handful of iterations to settle.
@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let dims = vec2f(textureDimensions(prev_frame));
    let texel = 1.0 / dims;
    let uv = frag_coord.xy / dims;

    let pl = feedback(uv - vec2f(texel.x, 0.0)).x;
    let pr = feedback(uv + vec2f(texel.x, 0.0)).x;
    let pb = feedback(uv - vec2f(0.0, texel.y)).x;
    let pt = feedback(uv + vec2f(0.0, texel.y)).x;
    let div = input0(uv).x;

    let p = (pl + pr + pb + pt - div) * 0.25;
    return vec4f(p, 0.0, 0.0, 1.0);
}

/// Fullscreen triangle vertex shader snippet (WGSL).
/// Uses the vertex_index trick: 3 vertices cover the entire screen
/// without needing a vertex buffer.
pub const FULLSCREEN_TRIANGLE_VS: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    let x = f32(i32(vi & 1u) * 4) - 1.0;
    let y = f32(i32(vi & 2u) * 2) - 1.0;
    return vec4f(x, y, 0.0, 1.0);
}
"#;

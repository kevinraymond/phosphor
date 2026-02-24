use wgpu::{Device, ShaderModule};

/// Compile WGSL source into a ShaderModule.
/// wgpu validates internally via naga; we just create the module
/// and let pipeline creation catch errors.
pub fn compile_shader(
    device: &Device,
    source: &str,
    label: &str,
) -> ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

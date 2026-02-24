use anyhow::Result;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferBindingType, ColorTargetState, Device, FragmentState, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState, RenderPipeline,
    ShaderModule, ShaderStages, TextureFormat, VertexState,
};

use super::fullscreen_quad::FULLSCREEN_TRIANGLE_VS;

pub struct ShaderPipeline {
    pub pipeline: RenderPipeline,
    pub bind_group_layout: BindGroupLayout,
}

impl ShaderPipeline {
    pub fn new(device: &Device, format: TextureFormat, fragment_source: &str) -> Result<Self> {
        let bind_group_layout = Self::create_bind_group_layout(device);

        // Combine vertex + fragment into one module
        let full_source = format!("{}\n{}", FULLSCREEN_TRIANGLE_VS, fragment_source);
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("phosphor-shader"),
            source: wgpu::ShaderSource::Wgsl(full_source.into()),
        });

        let pipeline = Self::create_pipeline(device, format, &bind_group_layout, &shader_module);

        Ok(Self {
            pipeline,
            bind_group_layout,
        })
    }

    pub fn recreate_pipeline(
        &mut self,
        device: &Device,
        format: TextureFormat,
        fragment_source: &str,
    ) -> Result<(), String> {
        let full_source = format!("{}\n{}", FULLSCREEN_TRIANGLE_VS, fragment_source);
        // wgpu defers shader validation; we need to use the module in a pipeline
        // to catch errors. We create the module and pipeline, relying on wgpu's
        // error handling.
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("phosphor-shader"),
            source: wgpu::ShaderSource::Wgsl(full_source.into()),
        });

        let pipeline =
            Self::create_pipeline(device, format, &self.bind_group_layout, &shader_module);
        self.pipeline = pipeline;
        Ok(())
    }

    fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("phosphor-bind-group-layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_pipeline(
        device: &Device,
        format: TextureFormat,
        bind_group_layout: &BindGroupLayout,
        shader_module: &ShaderModule,
    ) -> RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("phosphor-pipeline-layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("phosphor-render-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }
}

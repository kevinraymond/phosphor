use bytemuck::{Pod, Zeroable};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BufferBindingType, ColorTargetState, CommandEncoder,
    ComputePipeline, Device, FragmentState, PipelineCompilationOptions, PipelineLayoutDescriptor,
    PrimitiveState, Queue, RenderPipeline, ShaderStages, TextureFormat, TextureView, VertexState,
};

const WORKGROUP_SIZE: u32 = 256;

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct DrawUniforms {
    width: u32,
    height: u32,
    _pad0: u32,
    _pad1: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct ResolveUniforms {
    width: u32,
    height: u32,
    mode: u32, // 0 = additive, 1 = alpha
    _pad: u32,
}

/// Compute rasterizer: 3-pass atomic framebuffer for sub-pixel particles.
///
/// 1. Clear: `encoder.clear_buffer()` DMA zeros on 4 storage buffers
/// 2. Draw: each alive particle writes to framebuffer via atomicAdd (fixed-point)
/// 3. Resolve: fullscreen triangle reads buffers, decodes, tonemaps, outputs to render target
pub struct ComputeRasterizer {
    width: u32,
    height: u32,

    // 4 atomic storage buffers (one per channel)
    fb_buffers: [wgpu::Buffer; 4], // R, G, B, A

    // Uniform buffers
    draw_uniform_buffer: wgpu::Buffer,
    resolve_uniform_buffer: wgpu::Buffer,

    // Draw pass
    draw_pipeline: ComputePipeline,
    draw_bind_groups: [BindGroup; 2], // ping-pong for particle data
    draw_bgl: BindGroupLayout,

    // Resolve pass (render pipelines with hardware blend)
    resolve_pipeline_additive: RenderPipeline,
    resolve_pipeline_alpha: RenderPipeline,
    resolve_bind_group: BindGroup,
    resolve_bgl: BindGroupLayout,
}

impl ComputeRasterizer {
    pub fn new(
        device: &Device,
        hdr_format: TextureFormat,
        width: u32,
        height: u32,
        pos_life_buffers: &[wgpu::Buffer; 2],
        vel_size_buffers: &[wgpu::Buffer; 2],
        color_buffers: &[wgpu::Buffer; 2],
        alive_index_buffers: &[wgpu::Buffer; 2],
        counter_buffer: &wgpu::Buffer,
    ) -> Self {
        let pixel_count = (width * height) as u64;
        let fb_size = pixel_count * 4; // 4 bytes per i32

        let fb_buffers = std::array::from_fn(|i| {
            let label = ["fb-r", "fb-g", "fb-b", "fb-a"][i];
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: fb_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let draw_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cr-draw-uniforms"),
            size: std::mem::size_of::<DrawUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let resolve_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cr-resolve-uniforms"),
            size: std::mem::size_of::<ResolveUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Draw pipeline ---
        let draw_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("cr-draw-bgl"),
            entries: &[
                storage_ro_entry(0), // pos_life
                storage_ro_entry(1), // vel_size
                storage_ro_entry(2), // color
                storage_ro_entry(3), // alive_indices
                storage_ro_entry(4), // counters
                uniform_entry(5),    // draw uniforms
                storage_rw_entry(6), // fb_r
                storage_rw_entry(7), // fb_g
                storage_rw_entry(8), // fb_b
                storage_rw_entry(9), // fb_a
            ],
        });

        let draw_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cr-draw"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../../assets/shaders/builtin/compute_raster_draw.wgsl")
                    .into(),
            ),
        });

        let draw_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("cr-draw-layout"),
            bind_group_layouts: &[&draw_bgl],
            push_constant_ranges: &[],
        });

        let draw_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cr-draw-pipeline"),
            layout: Some(&draw_layout),
            module: &draw_shader,
            entry_point: Some("cs_draw"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        let draw_bind_groups = create_draw_bind_groups(
            device,
            &draw_bgl,
            pos_life_buffers,
            vel_size_buffers,
            color_buffers,
            alive_index_buffers,
            counter_buffer,
            &draw_uniform_buffer,
            &fb_buffers,
        );

        // --- Resolve pipeline (render) ---
        let resolve_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("cr-resolve-bgl"),
            entries: &[
                fragment_storage_ro_entry(0),
                fragment_storage_ro_entry(1),
                fragment_storage_ro_entry(2),
                fragment_storage_ro_entry(3),
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let resolve_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cr-resolve"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../../assets/shaders/builtin/compute_raster_resolve.wgsl")
                    .into(),
            ),
        });

        let resolve_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("cr-resolve-layout"),
            bind_group_layouts: &[&resolve_bgl],
            push_constant_ranges: &[],
        });

        let resolve_pipeline_additive = create_resolve_render_pipeline(
            device,
            &resolve_layout,
            &resolve_shader,
            hdr_format,
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            "cr-resolve-additive",
        );

        let resolve_pipeline_alpha = create_resolve_render_pipeline(
            device,
            &resolve_layout,
            &resolve_shader,
            hdr_format,
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            },
            "cr-resolve-alpha",
        );

        let resolve_bind_group = create_resolve_bind_group(
            device,
            &resolve_bgl,
            &fb_buffers,
            &resolve_uniform_buffer,
        );

        Self {
            width,
            height,
            fb_buffers,
            draw_uniform_buffer,
            resolve_uniform_buffer,
            draw_pipeline,
            draw_bind_groups,
            draw_bgl,
            resolve_pipeline_additive,
            resolve_pipeline_alpha,
            resolve_bind_group,
            resolve_bgl,
        }
    }

    /// Clear all framebuffer channels via DMA fill (no compute shader overhead).
    pub fn dispatch_clear(&self, encoder: &mut CommandEncoder) {
        for fb in &self.fb_buffers {
            encoder.clear_buffer(fb, 0, None);
        }
    }

    /// Dispatch the draw compute pass (particles write to atomic framebuffer).
    /// `output_idx` is the ping-pong index of the particle output buffers (1 - current).
    /// `max_particles` sets the dispatch size (shader exits early for thread_idx >= alive_count).
    pub fn dispatch_draw(
        &self,
        encoder: &mut CommandEncoder,
        queue: &Queue,
        output_idx: usize,
        max_particles: u32,
    ) {
        let uniforms = DrawUniforms {
            width: self.width,
            height: self.height,
            _pad0: 0,
            _pad1: 0,
        };
        queue.write_buffer(&self.draw_uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let workgroups = max_particles.div_ceil(WORKGROUP_SIZE);

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cr-draw"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.draw_pipeline);
        pass.set_bind_group(0, &self.draw_bind_groups[output_idx], &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    /// Render the resolve pass: reads atomic framebuffer, outputs to target.
    pub fn render_resolve(
        &self,
        encoder: &mut CommandEncoder,
        queue: &Queue,
        target: &TextureView,
        blend_mode: &str,
    ) {
        let mode = if blend_mode == "alpha" { 1u32 } else { 0u32 };
        let uniforms = ResolveUniforms {
            width: self.width,
            height: self.height,
            mode,
            _pad: 0,
        };
        queue.write_buffer(
            &self.resolve_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        let pipeline = if blend_mode == "alpha" {
            &self.resolve_pipeline_alpha
        } else {
            &self.resolve_pipeline_additive
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cr-resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.resolve_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Resize framebuffer if dimensions changed. Returns true if resized.
    pub fn ensure_size(
        &mut self,
        device: &Device,
        width: u32,
        height: u32,
        pos_life_buffers: &[wgpu::Buffer; 2],
        vel_size_buffers: &[wgpu::Buffer; 2],
        color_buffers: &[wgpu::Buffer; 2],
        alive_index_buffers: &[wgpu::Buffer; 2],
        counter_buffer: &wgpu::Buffer,
    ) -> bool {
        if self.width == width && self.height == height {
            return false;
        }

        self.width = width;
        self.height = height;

        let pixel_count = (width * height) as u64;
        let fb_size = pixel_count * 4;

        self.fb_buffers = std::array::from_fn(|i| {
            let label = ["fb-r", "fb-g", "fb-b", "fb-a"][i];
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: fb_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        self.draw_bind_groups = create_draw_bind_groups(
            device,
            &self.draw_bgl,
            pos_life_buffers,
            vel_size_buffers,
            color_buffers,
            alive_index_buffers,
            counter_buffer,
            &self.draw_uniform_buffer,
            &self.fb_buffers,
        );

        self.resolve_bind_group = create_resolve_bind_group(
            device,
            &self.resolve_bgl,
            &self.fb_buffers,
            &self.resolve_uniform_buffer,
        );

        true
    }

    /// Recreate draw bind groups (e.g. when particle buffers change due to upload_aux_data).
    pub fn recreate_draw_bind_groups(
        &mut self,
        device: &Device,
        pos_life_buffers: &[wgpu::Buffer; 2],
        vel_size_buffers: &[wgpu::Buffer; 2],
        color_buffers: &[wgpu::Buffer; 2],
        alive_index_buffers: &[wgpu::Buffer; 2],
        counter_buffer: &wgpu::Buffer,
    ) {
        self.draw_bind_groups = create_draw_bind_groups(
            device,
            &self.draw_bgl,
            pos_life_buffers,
            vel_size_buffers,
            color_buffers,
            alive_index_buffers,
            counter_buffer,
            &self.draw_uniform_buffer,
            &self.fb_buffers,
        );
    }
}

// --- Helper functions ---

fn storage_rw_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_ro_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn fragment_storage_ro_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_draw_bind_groups(
    device: &Device,
    layout: &BindGroupLayout,
    pos_life_buffers: &[wgpu::Buffer; 2],
    vel_size_buffers: &[wgpu::Buffer; 2],
    color_buffers: &[wgpu::Buffer; 2],
    alive_index_buffers: &[wgpu::Buffer; 2],
    counter_buffer: &wgpu::Buffer,
    draw_uniform_buffer: &wgpu::Buffer,
    fb_buffers: &[wgpu::Buffer; 4],
) -> [BindGroup; 2] {
    let make_bg = |idx: usize, label: &str| {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: pos_life_buffers[idx].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: vel_size_buffers[idx].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: color_buffers[idx].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: alive_index_buffers[idx].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: counter_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: draw_uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: fb_buffers[0].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: fb_buffers[1].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: fb_buffers[2].as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 9,
                    resource: fb_buffers[3].as_entire_binding(),
                },
            ],
        })
    };

    [make_bg(0, "cr-draw-bg-0"), make_bg(1, "cr-draw-bg-1")]
}

fn create_resolve_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    fb_buffers: &[wgpu::Buffer; 4],
    uniform_buffer: &wgpu::Buffer,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("cr-resolve-bg"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: fb_buffers[0].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: fb_buffers[1].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: fb_buffers[2].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: fb_buffers[3].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    })
}

fn create_resolve_render_pipeline(
    device: &Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: TextureFormat,
    blend: wgpu::BlendState,
    label: &str,
) -> RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        primitive: PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_uniforms_size() {
        assert_eq!(std::mem::size_of::<DrawUniforms>(), 16);
    }

    #[test]
    fn resolve_uniforms_size() {
        assert_eq!(std::mem::size_of::<ResolveUniforms>(), 16);
    }
}

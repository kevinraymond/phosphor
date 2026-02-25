use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, ColorTargetState,
    CommandEncoder, ComputePipeline, Device, FragmentState, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPipeline, ShaderStages, TextureFormat,
    TextureView, VertexState,
};

use super::sprite::SpriteAtlas;
use super::types::{Particle, ParticleAux, ParticleDef, ParticleRenderUniforms, ParticleUniforms};

const WORKGROUP_SIZE: u32 = 256;

/// GPU compute particle system with ping-pong storage buffers.
pub struct ParticleSystem {
    pub max_particles: u32,
    pub uniforms: ParticleUniforms,
    pub render_uniforms: ParticleRenderUniforms,
    pub alive_count: u32,

    // Ping-pong storage buffers
    storage_buffers: [wgpu::Buffer; 2],
    current: usize,
    // Atomic emission counter
    emit_counter_buffer: wgpu::Buffer,
    // Auxiliary data buffer (home positions, packed colors for image decomposition)
    aux_buffer: wgpu::Buffer,
    pub has_aux_data: bool,
    // Uniform buffers
    uniform_buffer: wgpu::Buffer,
    render_uniform_buffer: wgpu::Buffer,

    // Compute
    compute_pipeline: ComputePipeline,
    compute_bind_groups: [BindGroup; 2],
    compute_bgl: BindGroupLayout,

    // Render (additive blend — default)
    render_pipeline_additive: RenderPipeline,
    // Render (alpha blend — for non-glowing sprites)
    render_pipeline_alpha: RenderPipeline,
    render_bind_groups: [BindGroup; 2],
    render_bgl: BindGroupLayout,
    // Sprite texture bind group (bind group 1)
    sprite_bind_group: BindGroup,
    sprite_bgl: BindGroupLayout,
    pub sprite: Option<SpriteAtlas>,
    /// Active blend mode: "additive" or "alpha"
    pub blend_mode: String,

    // Emission accumulator (fractional particles per frame)
    emit_accumulator: f32,
    pub emit_rate: f32,
    pub burst_on_beat: u32,
    pub def: ParticleDef,
    /// Tracked for content-change detection in hot-reload.
    pub current_compute_source: String,
}

impl ParticleSystem {
    pub fn new(
        device: &Device,
        queue: &Queue,
        hdr_format: TextureFormat,
        def: &ParticleDef,
        compute_source: &str,
    ) -> Result<Self, String> {
        let max_particles = def.max_count;
        let particle_size = std::mem::size_of::<Particle>() as u64;
        let buffer_size = particle_size * max_particles as u64;

        // Create storage buffers (ping-pong)
        let storage_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particles-a"),
                size: buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particles-b"),
                size: buffer_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];

        // Atomic emission counter (single u32)
        let emit_counter_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("emit-counter"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Auxiliary buffer (home positions for image decomposition)
        // Placeholder: 1 element = 16 bytes (real data uploaded when image is loaded)
        let aux_size = std::mem::size_of::<ParticleAux>() as u64;
        let aux_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-aux"),
            size: aux_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Uniform buffers
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-uniforms"),
            size: std::mem::size_of::<ParticleUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let render_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-render-uniforms"),
            size: std::mem::size_of::<ParticleRenderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Compute pipeline ---
        let compute_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-compute-bgl"),
            entries: &[
                // binding 0: uniforms
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: particles_in (read)
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: particles_out (write)
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 3: emit counter (atomic)
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 4: auxiliary data (home positions, packed colors)
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle-compute"),
            source: wgpu::ShaderSource::Wgsl(compute_source.into()),
        });

        let compute_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle-compute-layout"),
            bind_group_layouts: &[&compute_bgl],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("particle-compute-pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("cs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        let compute_bind_groups = create_compute_bind_groups(
            device,
            &compute_bgl,
            &uniform_buffer,
            &storage_buffers,
            &emit_counter_buffer,
            &aux_buffer,
        );

        // --- Render pipeline ---
        let render_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-render-bgl"),
            entries: &[
                // binding 0: particles (read)
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: render uniforms
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Sprite texture bind group layout (bind group 1)
        let sprite_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-sprite-bgl"),
            entries: &[
                // binding 0: sprite texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 1: sprite sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create placeholder sprite for when no sprite texture is loaded
        let placeholder_sprite = super::sprite::create_placeholder_sprite(device, queue);
        let sprite_bind_group = create_sprite_bind_group(device, &sprite_bgl, &placeholder_sprite);

        let render_source =
            include_str!("../../../../../assets/shaders/builtin/particle_render.wgsl");
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle-render"),
            source: wgpu::ShaderSource::Wgsl(render_source.into()),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle-render-layout"),
            bind_group_layouts: &[&render_bgl, &sprite_bgl],
            push_constant_ranges: &[],
        });

        // Additive blend (default: SrcAlpha + One)
        let render_pipeline_additive =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("particle-render-additive"),
                layout: Some(&render_pipeline_layout),
                vertex: VertexState {
                    module: &render_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: PipelineCompilationOptions::default(),
                },
                fragment: Some(FragmentState {
                    module: &render_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(ColorTargetState {
                        format: hdr_format,
                        blend: Some(wgpu::BlendState {
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
                        }),
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
            });

        // Alpha blend (SrcAlpha + OneMinusSrcAlpha) for non-glowing sprites
        let render_pipeline_alpha =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("particle-render-alpha"),
                layout: Some(&render_pipeline_layout),
                vertex: VertexState {
                    module: &render_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: PipelineCompilationOptions::default(),
                },
                fragment: Some(FragmentState {
                    module: &render_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(ColorTargetState {
                        format: hdr_format,
                        blend: Some(wgpu::BlendState {
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
                        }),
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
            });

        let blend_mode = def.blend.clone();

        let render_bind_groups = create_render_bind_groups(
            device,
            &render_bgl,
            &storage_buffers,
            &render_uniform_buffer,
        );

        Ok(Self {
            max_particles,
            uniforms: bytemuck::Zeroable::zeroed(),
            render_uniforms: bytemuck::Zeroable::zeroed(),
            alive_count: 0,
            storage_buffers,
            current: 0,
            emit_counter_buffer,
            aux_buffer,
            has_aux_data: false,
            uniform_buffer,
            render_uniform_buffer,
            compute_pipeline,
            compute_bind_groups,
            compute_bgl,
            render_pipeline_additive,
            render_pipeline_alpha,
            render_bind_groups,
            render_bgl,
            sprite_bind_group,
            sprite_bgl,
            sprite: None,
            blend_mode,
            emit_accumulator: 0.0,
            emit_rate: def.emit_rate,
            burst_on_beat: def.burst_on_beat,
            def: def.clone(),
            current_compute_source: compute_source.to_string(),
        })
    }

    /// Recompile the compute pipeline (for hot-reload).
    pub fn recompile_compute(
        &mut self,
        device: &Device,
        source: &str,
    ) -> Result<(), String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle-compute-hotreload"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle-compute-layout"),
            bind_group_layouts: &[&self.compute_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("particle-compute-pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            cache: None,
        });

        self.compute_pipeline = pipeline;
        Ok(())
    }

    /// Update uniforms from app state. Call before dispatch().
    pub fn update_uniforms(
        &mut self,
        dt: f32,
        time: f32,
        resolution: [f32; 2],
        beat: f32,
    ) {
        // Accumulate emissions
        self.emit_accumulator += self.emit_rate * dt;

        // Beat burst — use dedicated beat trigger instead of onset threshold
        if beat > 0.5 && self.burst_on_beat > 0 {
            self.emit_accumulator += self.burst_on_beat as f32;
        }

        let emit_count = self.emit_accumulator as u32;
        self.emit_accumulator -= emit_count as f32;

        self.uniforms.delta_time = dt;
        self.uniforms.time = time;
        self.uniforms.max_particles = self.max_particles;
        self.uniforms.emit_count = emit_count;

        // Emitter config from def
        self.uniforms.emitter_pos = self.def.emitter.position;
        self.uniforms.emitter_radius = self.def.emitter.radius;
        self.uniforms.emitter_shape = self.def.emitter.shape_index();
        self.uniforms.lifetime = self.def.lifetime;
        self.uniforms.initial_speed = self.def.initial_speed;
        self.uniforms.initial_size = self.def.initial_size;
        self.uniforms.size_end = self.def.size_end;
        self.uniforms.gravity = self.def.gravity;
        self.uniforms.drag = self.def.drag;
        self.uniforms.turbulence = self.def.turbulence;
        self.uniforms.attraction_point = [0.0, 0.0];
        self.uniforms.attraction_strength = self.def.attraction_strength;

        // Seed for randomness (different each frame)
        self.uniforms.seed = time * 1000.0 % 65536.0;

        // Resolution for aspect ratio correction in compute shader
        self.uniforms.resolution = resolution;

        self.render_uniforms.resolution = resolution;
        self.render_uniforms.time = time;
    }

    /// Copy audio features into particle uniforms.
    pub fn update_audio(
        &mut self,
        sub_bass: f32,
        bass: f32,
        mid: f32,
        rms: f32,
        kick: f32,
        onset: f32,
        centroid: f32,
        flux: f32,
        beat: f32,
        beat_phase: f32,
    ) {
        self.uniforms.sub_bass = sub_bass;
        self.uniforms.bass = bass;
        self.uniforms.mid = mid;
        self.uniforms.rms = rms;
        self.uniforms.kick = kick;
        self.uniforms.onset = onset;
        self.uniforms.centroid = centroid;
        self.uniforms.flux = flux;
        self.uniforms.beat = beat;
        self.uniforms.beat_phase = beat_phase;
    }

    /// Run the compute dispatch (particle simulation).
    pub fn dispatch(&self, encoder: &mut CommandEncoder, queue: &Queue) {
        // Reset emit counter to 0
        queue.write_buffer(&self.emit_counter_buffer, 0, &[0u8; 4]);

        // Upload uniforms
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));

        let workgroups = (self.max_particles + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("particle-sim"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.compute_pipeline);
        pass.set_bind_group(0, &self.compute_bind_groups[self.current], &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    /// Render particles into the given target (with LoadOp::Load to composite on top).
    pub fn render(&self, encoder: &mut CommandEncoder, queue: &Queue, target: &TextureView) {
        // Upload render uniforms
        queue.write_buffer(
            &self.render_uniform_buffer,
            0,
            bytemuck::bytes_of(&self.render_uniforms),
        );

        // The output buffer is the one we just wrote to in compute
        let output_idx = 1 - self.current;

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particle-render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Composite on top of existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let pipeline = if self.blend_mode == "alpha" {
            &self.render_pipeline_alpha
        } else {
            &self.render_pipeline_additive
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.render_bind_groups[output_idx], &[]);
        pass.set_bind_group(1, &self.sprite_bind_group, &[]);
        // 6 vertices per particle (two triangles = instanced quad)
        pass.draw(0..6, 0..self.max_particles);
    }

    /// Load a sprite atlas and update the sprite bind group.
    pub fn set_sprite(&mut self, device: &Device, atlas: SpriteAtlas) {
        self.sprite_bind_group = create_sprite_bind_group(device, &self.sprite_bgl, &atlas);
        self.render_uniforms.render_mode = if atlas.animated { 2 } else { 1 };
        self.render_uniforms.sprite_cols = atlas.cols;
        self.render_uniforms.sprite_rows = atlas.rows;
        self.render_uniforms.sprite_frames = atlas.frames;
        self.sprite = Some(atlas);
    }

    /// Upload auxiliary data (home positions, packed colors) for image decomposition.
    /// Recreates aux buffer and compute bind groups.
    pub fn upload_aux_data(&mut self, device: &Device, queue: &Queue, data: &[ParticleAux]) {
        let aux_size = (std::mem::size_of::<ParticleAux>() * data.len().max(1)) as u64;
        self.aux_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-aux"),
            size: aux_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !data.is_empty() {
            queue.write_buffer(&self.aux_buffer, 0, bytemuck::cast_slice(data));
        }
        self.has_aux_data = !data.is_empty();

        // Recreate compute bind groups with new aux buffer
        self.compute_bind_groups = create_compute_bind_groups(
            device,
            &self.compute_bgl,
            &self.uniform_buffer,
            &self.storage_buffers,
            &self.emit_counter_buffer,
            &self.aux_buffer,
        );
    }

    /// Flip ping-pong buffers for next frame.
    pub fn flip(&mut self) {
        self.current = 1 - self.current;
    }
}

fn create_compute_bind_groups(
    device: &Device,
    layout: &BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    storage_buffers: &[wgpu::Buffer; 2],
    emit_counter: &wgpu::Buffer,
    aux_buffer: &wgpu::Buffer,
) -> [BindGroup; 2] {
    // bind_group[0]: read from storage[0], write to storage[1]
    let bg0 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-compute-bg-0"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: storage_buffers[0].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: storage_buffers[1].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: emit_counter.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: aux_buffer.as_entire_binding(),
            },
        ],
    });
    // bind_group[1]: read from storage[1], write to storage[0]
    let bg1 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-compute-bg-1"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: storage_buffers[1].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: storage_buffers[0].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: emit_counter.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 4,
                resource: aux_buffer.as_entire_binding(),
            },
        ],
    });
    [bg0, bg1]
}

fn create_sprite_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    sprite: &SpriteAtlas,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-sprite-bg"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&sprite.view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&sprite.sampler),
            },
        ],
    })
}

fn create_render_bind_groups(
    device: &Device,
    layout: &BindGroupLayout,
    storage_buffers: &[wgpu::Buffer; 2],
    render_uniform_buffer: &wgpu::Buffer,
) -> [BindGroup; 2] {
    let bg0 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-render-bg-0"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: storage_buffers[0].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: render_uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    });
    let bg1 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-render-bg-1"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: storage_buffers[1].as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: render_uniform_buffer,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    });
    [bg0, bg1]
}

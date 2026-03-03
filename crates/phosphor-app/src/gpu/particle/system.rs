use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, ColorTargetState,
    CommandEncoder, ComputePipeline, Device, FragmentState, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPipeline, ShaderStages, TextureFormat,
    TextureView, VertexState,
};

use super::flow_field::FlowFieldTexture;
use super::obstacle::ObstacleTexture;
use super::spatial_hash::SpatialHashGrid;
use super::sprite::SpriteAtlas;
use super::types::{
    ImageSampleDef, ParticleAux, ParticleDef, ParticleImageSource,
    ParticleRenderUniforms, ParticleUniforms, SourceTransition,
};

const WORKGROUP_SIZE: u32 = 256;

/// GPU compute particle system with ping-pong storage buffers,
/// alive/dead index lists, and indirect draw for GPU-driven rendering.
pub struct ParticleSystem {
    pub max_particles: u32,
    pub uniforms: ParticleUniforms,
    pub render_uniforms: ParticleRenderUniforms,
    pub alive_count: u32,

    // Ping-pong SoA particle storage buffers (4 components × 2 ping-pong)
    pos_life_buffers: [wgpu::Buffer; 2],
    vel_size_buffers: [wgpu::Buffer; 2],
    color_buffers: [wgpu::Buffer; 2],
    flags_buffers: [wgpu::Buffer; 2],
    current: usize,

    // Counter buffer: 4 x atomic<u32> = [alive_count, dead_count, emit_used, reserved]
    counter_buffer: wgpu::Buffer,

    // Alive index buffers (ping-pong): tightly packed indices of living particles
    alive_index_buffers: [wgpu::Buffer; 2],

    // Dead index buffer: indices of dead particles (CPU-initialized, read-only in sim)
    dead_index_buffer: wgpu::Buffer,

    // Indirect draw args: DrawIndirectArgs [vertex_count, instance_count, first_vertex, first_instance]
    indirect_args_buffer: wgpu::Buffer,

    // Auxiliary data buffer (home positions, packed colors for image decomposition)
    aux_buffer: wgpu::Buffer,
    pub has_aux_data: bool,

    // Uniform buffers
    uniform_buffer: wgpu::Buffer,
    render_uniform_buffer: wgpu::Buffer,

    // Compute (sim)
    compute_pipeline: ComputePipeline,
    compute_bind_groups: [BindGroup; 2],
    compute_bgl: BindGroupLayout,

    // Compute (prepare indirect args)
    prepare_indirect_pipeline: ComputePipeline,
    prepare_indirect_bind_groups: [BindGroup; 2],

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

    // Flow field + obstacle (group 1 for compute)
    flow_field: FlowFieldTexture,
    obstacle: ObstacleTexture,
    flow_field_bgl: BindGroupLayout,
    flow_field_bind_group: BindGroup,

    // Obstacle collision state
    pub obstacle_enabled: bool,
    pub obstacle_mode: super::types::ObstacleMode,
    pub obstacle_threshold: f32,
    pub obstacle_elasticity: f32,
    /// "image", "video", "webcam", or "" (none)
    pub obstacle_source: String,
    /// Path to obstacle image/video file (for preset save/load)
    pub obstacle_image_path: Option<String>,

    // Obstacle video playback
    obstacle_video_frames: Vec<crate::media::types::DecodedFrame>,
    obstacle_video_delays_ms: Vec<u32>,
    obstacle_video_frame: usize,
    obstacle_video_elapsed_ms: f64,
    pub obstacle_video_playing: bool,
    pub obstacle_video_looping: bool,
    pub obstacle_video_speed: f32,

    // Trail rendering
    trail_buffer: Option<wgpu::Buffer>,
    trail_length: u32,
    trail_width: f32,
    frame_index: u32,
    trail_render_pipeline: Option<RenderPipeline>,
    trail_render_bgl: Option<BindGroupLayout>,
    trail_render_bind_groups: Option<[BindGroup; 2]>,
    trail_compute_bgl: Option<BindGroupLayout>,
    trail_compute_bind_group: Option<BindGroup>,
    trail_indirect_args_buffer: Option<wgpu::Buffer>,
    trail_prepare_indirect_pipeline: Option<ComputePipeline>,
    trail_prepare_indirect_bind_group: Option<BindGroup>,

    // Spatial hash grid for particle-particle interaction
    spatial_hash: Option<SpatialHashGrid>,

    // Depth sort (bitonic merge sort on alive indices by particle size)
    sort_key_buffer: Option<wgpu::Buffer>,
    sort_params_buffer: Option<wgpu::Buffer>,
    sort_keygen_pipeline: Option<ComputePipeline>,
    sort_keygen_bind_groups: Option<[BindGroup; 2]>,
    sort_pipeline: Option<ComputePipeline>,
    sort_bind_groups: Option<[BindGroup; 2]>,
    sort_passes: Vec<(u32, u32)>, // (block_size, sub_block_size) per pass
    sort_n: u32,                   // next power of 2 >= max_particles

    // Placeholder empty BGL + bind group for padding contiguous bind group indices
    empty_bgl: BindGroupLayout,
    empty_bind_group: BindGroup,

    // Counter readback: staging buffer + async map state
    counter_readback: wgpu::Buffer,
    counter_map_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
    counter_map_ready: std::sync::Arc<std::sync::atomic::AtomicBool>,

    // Emission accumulator (fractional particles per frame)
    emit_accumulator: f32,
    pub emit_rate: f32,
    pub burst_on_beat: u32,
    pub def: ParticleDef,
    /// Tracked for content-change detection in hot-reload.
    pub current_compute_source: String,

    // --- Particle image source (video/webcam/static) ---
    pub image_source: ParticleImageSource,
    pub source_transition: Option<SourceTransition>,
    pub sample_def: ImageSampleDef,
    /// Path to the video file (for preset save/load).
    pub video_path: Option<String>,
    /// Cached aux data for the current static source (used as transition "from").
    pub current_aux: Vec<ParticleAux>,
}

impl ParticleSystem {
    pub fn new(
        device: &Device,
        queue: &Queue,
        hdr_format: TextureFormat,
        def: &ParticleDef,
        compute_source: &str,
        interaction: bool,
    ) -> Result<Self, String> {
        // Clamp max_count to device storage buffer binding limit
        // With SoA, each buffer is one vec4f per particle (16 bytes), so the limit is higher
        let max_storage = device.limits().max_storage_buffer_binding_size as u64;
        let device_max_particles = (max_storage / super::types::PARTICLE_COMPONENT_STRIDE) as u32;
        let max_particles = if def.max_count > device_max_particles {
            log::warn!(
                "Particle max_count {} exceeds device limit ({} particles @ {}MB max binding). Clamped to {}.",
                def.max_count,
                device_max_particles,
                max_storage / (1024 * 1024),
                device_max_particles,
            );
            device_max_particles
        } else {
            def.max_count
        };

        let component_size = super::types::PARTICLE_COMPONENT_STRIDE * max_particles as u64;

        // Create SoA storage buffers (4 components × 2 ping-pong) — GPU-cleared to zero
        // to ensure all particles start dead (life=0). Without this, recycled GPU memory
        // from a previous effect's freed buffers can contain alive particles with high
        // brightness, causing blowout through additive blend + feedback accumulation.
        let create_component_buffers = |label_a: &str, label_b: &str| -> [wgpu::Buffer; 2] {
            [
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label_a),
                    size: component_size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label_b),
                    size: component_size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            ]
        };
        let pos_life_buffers = create_component_buffers("pos-life-a", "pos-life-b");
        let vel_size_buffers = create_component_buffers("vel-size-a", "vel-size-b");
        let color_buffers = create_component_buffers("color-a", "color-b");
        let flags_buffers = create_component_buffers("flags-a", "flags-b");

        // GPU-side zero-init — avoids allocating 2×buffer_size on CPU (128MB+ at 1M particles)
        let mut init_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("particle-init-clear"),
        });
        for buf in [&pos_life_buffers, &vel_size_buffers, &color_buffers, &flags_buffers] {
            init_encoder.clear_buffer(&buf[0], 0, None);
            init_encoder.clear_buffer(&buf[1], 0, None);
        }
        queue.submit(std::iter::once(init_encoder.finish()));

        // Counter buffer: 4 x u32 = 16 bytes
        // [0]=alive_count, [1]=dead_count, [2]=emit_used, [3]=reserved
        let counter_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-counters"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Alive index buffers (ping-pong): max_particles * 4 bytes each
        let alive_index_size = max_particles as u64 * 4;
        let alive_index_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("alive-indices-a"),
                size: alive_index_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("alive-indices-b"),
                size: alive_index_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];

        // Dead index buffer: pre-populated with [0, 1, 2, ..., max_particles-1]
        let dead_indices: Vec<u32> = (0..max_particles).collect();
        let dead_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dead-indices"),
            size: alive_index_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&dead_index_buffer, 0, bytemuck::cast_slice(&dead_indices));

        // Indirect draw args buffer: DrawIndirectArgs = 4 x u32 = 16 bytes
        let indirect_args_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-indirect-args"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Initialize with zero instances
        queue.write_buffer(&indirect_args_buffer, 0, bytemuck::cast_slice(&[6u32, 0, 0, 0]));

        // Auxiliary buffer (home positions for image decomposition)
        // Pre-allocate at max_particles size so updates can use write_buffer without
        // recreating the buffer or bind groups (enables per-frame video source updates).
        let aux_size = (std::mem::size_of::<ParticleAux>() * max_particles as usize).max(16) as u64;
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

        // --- Compute pipeline (sim) — SoA layout: 13 entries ---
        let compute_storage_ro = |binding: u32| -> BindGroupLayoutEntry {
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
        };
        let compute_storage_rw = |binding: u32| -> BindGroupLayoutEntry {
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
        };
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
                compute_storage_ro(1),  // pos_life_in
                compute_storage_ro(2),  // vel_size_in
                compute_storage_ro(3),  // color_in
                compute_storage_ro(4),  // flags_in
                compute_storage_rw(5),  // pos_life_out
                compute_storage_rw(6),  // vel_size_out
                compute_storage_rw(7),  // color_out
                compute_storage_rw(8),  // flags_out
                compute_storage_rw(9),  // counters
                compute_storage_ro(10), // aux
                compute_storage_ro(11), // dead_indices
                compute_storage_rw(12), // alive_indices_out
            ],
        });

        // --- Flow field (group 1 for compute) ---
        let flow_field = if def.flow_field {
            FlowFieldTexture::new(device, queue)
        } else {
            FlowFieldTexture::placeholder(device, queue)
        };

        let flow_field_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-flow-field-bgl"),
            entries: &[
                // binding 0: flow field 3D texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 1: flow field sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding 2: obstacle 2D texture
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 3: obstacle sampler
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let obstacle = ObstacleTexture::placeholder(device, queue);
        let flow_field_bind_group = create_flow_field_bind_group(device, &flow_field_bgl, &flow_field, &obstacle);

        // Empty BGL + bind group for padding contiguous bind group indices
        let empty_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("empty-bgl"),
            entries: &[],
        });
        let empty_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("empty-bg"),
            layout: &empty_bgl,
            entries: &[],
        });

        // Create trail compute BGL before pipeline if trails are needed,
        // so the shader's @group(2) bindings validate at pipeline creation.
        let trail_compute_bgl = if def.trail_length >= 2 {
            Some(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("particle-trail-compute-bgl"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            }))
        } else {
            None
        };

        // Create spatial hash before pipeline if interaction is enabled,
        // so the query BGL is included in the initial compute pipeline layout.
        let spatial_hash = if interaction {
            Some(SpatialHashGrid::new(
                device,
                max_particles,
                &pos_life_buffers,
                &uniform_buffer,
            ))
        } else {
            None
        };

        // Build compute pipeline layout matching compute_bind_group_layouts() logic:
        // groups 0=core, 1=flow field, 2=trails (or empty placeholder), 3=spatial hash
        let mut bgl_refs: Vec<&BindGroupLayout> = vec![&compute_bgl, &flow_field_bgl];
        if spatial_hash.is_some() {
            // Spatial hash at group 3 requires group 2 to exist (contiguous)
            if let Some(ref trail_bgl) = trail_compute_bgl {
                bgl_refs.push(trail_bgl);
            } else {
                bgl_refs.push(&empty_bgl);
            }
            bgl_refs.push(&spatial_hash.as_ref().unwrap().query_bgl);
        } else if let Some(ref trail_bgl) = trail_compute_bgl {
            bgl_refs.push(trail_bgl);
        }

        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle-compute"),
            source: wgpu::ShaderSource::Wgsl(compute_source.into()),
        });

        let compute_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle-compute-layout"),
            bind_group_layouts: &bgl_refs,
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
            &pos_life_buffers,
            &vel_size_buffers,
            &color_buffers,
            &flags_buffers,
            &counter_buffer,
            &aux_buffer,
            &dead_index_buffer,
            &alive_index_buffers,
        );

        // --- Prepare indirect pipeline ---
        let (prepare_indirect_pipeline, prepare_indirect_bind_groups) =
            create_prepare_indirect_pipeline(
                device,
                &counter_buffer,
                &indirect_args_buffer,
            );

        // --- Render pipeline ---
        let render_storage_ro = |binding: u32| -> BindGroupLayoutEntry {
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
        };
        let render_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-render-bgl"),
            entries: &[
                render_storage_ro(0), // pos_life
                render_storage_ro(1), // vel_size
                render_storage_ro(2), // color
                render_storage_ro(3), // flags
                // binding 4: render uniforms
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 5: alive_indices (read) — for GPU-driven indirect draw
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
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
            &pos_life_buffers,
            &vel_size_buffers,
            &color_buffers,
            &flags_buffers,
            &render_uniform_buffer,
            &alive_index_buffers,
        );

        // --- Depth sort (optional) ---
        // Auto-disable above 65K: bitonic sort is O(n log²n) dispatches — at 1M (2^20)
        // that's 210 dispatches/frame which is too expensive.
        const MAX_SORT_PARTICLES: u32 = 65536;
        let depth_sort_enabled = if def.depth_sort && max_particles > MAX_SORT_PARTICLES {
            log::warn!(
                "Depth sort auto-disabled: {} particles exceeds {} limit (would need {} dispatches/frame)",
                max_particles,
                MAX_SORT_PARTICLES,
                {
                    let n = next_power_of_2(max_particles);
                    let log_n = (n as f32).log2() as u32;
                    log_n * (log_n + 1) / 2
                },
            );
            false
        } else {
            def.depth_sort
        };
        let (
            sort_key_buffer,
            sort_params_buffer,
            sort_keygen_pipeline,
            sort_keygen_bind_groups,
            sort_pipeline,
            sort_bind_groups,
            sort_passes,
            sort_n,
        ) = if depth_sort_enabled {
            create_sort_resources(
                device,
                max_particles,
                &counter_buffer,
                &vel_size_buffers,
                &alive_index_buffers,
            )
        } else {
            (None, None, None, None, None, None, Vec::new(), 0)
        };

        Ok(Self {
            max_particles,
            uniforms: bytemuck::Zeroable::zeroed(),
            render_uniforms: bytemuck::Zeroable::zeroed(),
            alive_count: 0,
            pos_life_buffers,
            vel_size_buffers,
            color_buffers,
            flags_buffers,
            current: 0,
            counter_buffer,
            alive_index_buffers,
            dead_index_buffer,
            indirect_args_buffer,
            aux_buffer,
            has_aux_data: false,
            uniform_buffer,
            render_uniform_buffer,
            compute_pipeline,
            compute_bind_groups,
            compute_bgl,
            prepare_indirect_pipeline,
            prepare_indirect_bind_groups,
            render_pipeline_additive,
            render_pipeline_alpha,
            render_bind_groups,
            render_bgl,
            sprite_bind_group,
            sprite_bgl,
            sprite: None,
            blend_mode,
            flow_field,
            obstacle,
            flow_field_bgl,
            flow_field_bind_group,
            obstacle_enabled: false,
            obstacle_mode: super::types::ObstacleMode::Bounce,
            obstacle_threshold: 0.5,
            obstacle_elasticity: 0.7,
            obstacle_source: String::new(),
            obstacle_image_path: None,
            obstacle_video_frames: Vec::new(),
            obstacle_video_delays_ms: Vec::new(),
            obstacle_video_frame: 0,
            obstacle_video_elapsed_ms: 0.0,
            obstacle_video_playing: true,
            obstacle_video_looping: true,
            obstacle_video_speed: 1.0,
            trail_buffer: None,
            trail_length: 0,
            trail_width: 0.005,
            frame_index: 0,
            trail_render_pipeline: None,
            trail_render_bgl: None,
            trail_render_bind_groups: None,
            trail_compute_bgl,
            trail_compute_bind_group: None,
            trail_indirect_args_buffer: None,
            trail_prepare_indirect_pipeline: None,
            trail_prepare_indirect_bind_group: None,
            spatial_hash,
            sort_key_buffer,
            sort_params_buffer,
            sort_keygen_pipeline,
            sort_keygen_bind_groups,
            sort_pipeline,
            sort_bind_groups,
            sort_passes,
            sort_n,
            empty_bgl,
            empty_bind_group,
            counter_readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particle-counter-readback"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            counter_map_pending: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            counter_map_ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            emit_accumulator: 0.0,
            emit_rate: def.emit_rate,
            burst_on_beat: def.burst_on_beat,
            def: def.clone(),
            current_compute_source: compute_source.to_string(),
            image_source: ParticleImageSource::Static,
            source_transition: None,
            sample_def: def.image_sample.clone().unwrap_or(ImageSampleDef {
                mode: "grid".to_string(),
                threshold: 0.1,
                scale: 1.0,
            }),
            video_path: None,
            current_aux: Vec::new(),
        })
    }

    /// Build the list of bind group layouts for compute pipeline creation.
    /// Groups: 0=core, 1=flow field, 2=trails (or empty placeholder), 3=spatial hash (if enabled).
    fn compute_bind_group_layouts(&self) -> Vec<&BindGroupLayout> {
        let mut layouts: Vec<&BindGroupLayout> = vec![&self.compute_bgl, &self.flow_field_bgl];

        if let Some(hash) = &self.spatial_hash {
            // Group 3 requires group 2 to exist (contiguous indices).
            // Use trail BGL if available, otherwise empty placeholder.
            if let Some(trail_bgl) = &self.trail_compute_bgl {
                layouts.push(trail_bgl);
            } else {
                layouts.push(&self.empty_bgl);
            }
            layouts.push(&hash.query_bgl);
        } else if let Some(trail_bgl) = &self.trail_compute_bgl {
            layouts.push(trail_bgl);
        }

        layouts
    }

    /// Replace the compute shader and rebuild the pipeline (for switching shader at runtime).
    /// Unlike recompile_compute, this also updates current_compute_source.
    #[allow(dead_code)]
    pub fn set_compute_shader(
        &mut self,
        device: &Device,
        source: &str,
    ) -> Result<(), String> {
        self.recompile_compute(device, source)?;
        self.current_compute_source = source.to_string();
        Ok(())
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

        // Build layout with optional trail (group 2) and spatial hash (group 3)
        let bind_group_layouts = self.compute_bind_group_layouts();
        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle-compute-layout"),
            bind_group_layouts: &bind_group_layouts,
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

        // Track previous emitter position for velocity inheritance
        self.uniforms.prev_emitter_pos = self.uniforms.emitter_pos;

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

        // Flow field params from def
        self.uniforms.flow_strength = self.def.flow_strength;
        self.uniforms.flow_scale = self.def.flow_scale;
        self.uniforms.flow_speed = self.def.flow_speed;
        self.uniforms.flow_enabled = if self.def.flow_field { 1.0 } else { 0.0 };

        // Trail params
        self.uniforms.trail_length = self.trail_length;
        self.uniforms.trail_width = self.trail_width;

        // Wind + vortex + ground
        self.uniforms.wind = self.def.wind;
        self.uniforms.vortex_center = self.def.vortex_center;
        self.uniforms.vortex_strength = self.def.vortex_strength;
        self.uniforms.vortex_radius = self.def.vortex_radius;
        self.uniforms.ground_y = self.def.ground_y;
        self.uniforms.ground_bounce = self.def.ground_bounce;

        // Noise params
        self.uniforms.noise_octaves = self.def.noise_octaves;
        self.uniforms.noise_lacunarity = self.def.noise_lacunarity;
        self.uniforms.noise_persistence = self.def.noise_persistence;
        self.uniforms.noise_mode = self.def.noise_mode;
        self.uniforms.noise_speed = self.def.noise_speed;

        // Emitter enhancements
        self.uniforms.emitter_angle = self.def.emitter_angle;
        self.uniforms.emitter_spread = self.def.emitter_spread;
        self.uniforms.speed_variance = self.def.speed_variance;
        self.uniforms.life_variance = self.def.life_variance;
        self.uniforms.size_variance = self.def.size_variance;
        self.uniforms.velocity_inherit = self.def.velocity_inherit;

        // Lifetime curves (pack Vec<f32> into [f32; 8] LUTs)
        self.uniforms.size_curve = pack_curve_lut(&self.def.size_curve);
        self.uniforms.opacity_curve = pack_curve_lut(&self.def.opacity_curve);
        self.uniforms.curve_flags = 0;
        if !self.def.size_curve.is_empty() {
            self.uniforms.curve_flags |= 1;
        }
        if !self.def.opacity_curve.is_empty() {
            self.uniforms.curve_flags |= 2;
        }

        // Color gradient (pack hex strings into [u32; 8])
        self.uniforms.gradient_count = self.def.color_gradient.len().min(8) as u32;
        self.uniforms.color_gradient = [0u32; 8];
        for (i, hex) in self.def.color_gradient.iter().take(8).enumerate() {
            self.uniforms.color_gradient[i] = super::types::parse_hex_color(hex);
        }

        // Spin
        self.uniforms.spin_speed = self.def.spin_speed;

        // Depth sort
        self.uniforms.depth_sort = if self.def.depth_sort { 1 } else { 0 };

        self.render_uniforms.resolution = resolution;
        self.render_uniforms.time = time;
        self.render_uniforms.frame_index = self.frame_index;
        self.render_uniforms.trail_length = self.trail_length;
        self.render_uniforms.trail_width = self.trail_width;
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
        low_mid: f32,
        upper_mid: f32,
        presence: f32,
        brilliance: f32,
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
        self.uniforms.low_mid = low_mid;
        self.uniforms.upper_mid = upper_mid;
        self.uniforms.presence = presence;
        self.uniforms.brilliance = brilliance;
    }

    /// Run the compute dispatch (particle simulation + prepare indirect args).
    pub fn dispatch(&self, encoder: &mut CommandEncoder, queue: &Queue) {
        // Reset counters to 0 (alive_count, dead_count, emit_used, reserved)
        queue.write_buffer(&self.counter_buffer, 0, &[0u8; 16]);

        // Upload uniforms
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));

        let workgroups = (self.max_particles + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;

        // 0. Spatial hash (if interaction enabled) — build before sim
        if let Some(hash) = &self.spatial_hash {
            // Read from the input buffer (current side of ping-pong)
            hash.dispatch(encoder, queue, self.current);
        }

        // 1. Particle simulation
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("particle-sim"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &self.compute_bind_groups[self.current], &[]);
            pass.set_bind_group(1, &self.flow_field_bind_group, &[]);
            if let Some(trail_bg) = &self.trail_compute_bind_group {
                pass.set_bind_group(2, trail_bg, &[]);
            } else if self.spatial_hash.is_some() {
                // Spatial hash at group 3 requires group 2 to exist (contiguous indices)
                pass.set_bind_group(2, &self.empty_bind_group, &[]);
            }
            if let Some(hash) = &self.spatial_hash {
                pass.set_bind_group(3, &hash.query_bind_group, &[]);
            }
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        // 1b. Depth sort (if enabled): keygen then bitonic sort passes
        if let (
            Some(keygen_pipeline),
            Some(keygen_bgs),
            Some(sort_pl),
            Some(sort_bgs),
            Some(params_buf),
        ) = (
            &self.sort_keygen_pipeline,
            &self.sort_keygen_bind_groups,
            &self.sort_pipeline,
            &self.sort_bind_groups,
            &self.sort_params_buffer,
        ) {
            let sort_workgroups = (self.sort_n + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE;
            let min_align = 256u64;

            // Write all sort pass parameters (static data, could cache but tiny)
            for (i, &(block_size, sub_block_size)) in self.sort_passes.iter().enumerate() {
                let offset = i as u64 * min_align;
                let params: [u32; 4] = [block_size, sub_block_size, self.sort_n, 0];
                queue.write_buffer(params_buf, offset, bytemuck::cast_slice(&params));
            }

            // Key generation pass
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("particle-sort-keygen"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(keygen_pipeline);
                pass.set_bind_group(0, &keygen_bgs[self.current], &[]);
                pass.dispatch_workgroups(sort_workgroups, 1, 1);
            }

            // Bitonic sort passes (each reads from pre-computed params buffer at dynamic offset)
            for (i, _) in self.sort_passes.iter().enumerate() {
                let offset = (i as u64 * min_align) as u32;
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("particle-sort-step"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(sort_pl);
                pass.set_bind_group(0, &sort_bgs[self.current], &[offset]);
                pass.dispatch_workgroups(sort_workgroups, 1, 1);
            }
        }

        // 2. Prepare indirect draw args (reads alive_count from counters, writes DrawIndirectArgs)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("particle-prepare-indirect"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.prepare_indirect_pipeline);
            // Use the bind group matching current ping-pong state
            // (counter_buffer is shared, but we pick the right alive buffer)
            pass.set_bind_group(0, &self.prepare_indirect_bind_groups[self.current], &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }

        // 3. Prepare trail indirect draw args (if trails active)
        if let (Some(pipeline), Some(bg)) = (
            &self.trail_prepare_indirect_pipeline,
            &self.trail_prepare_indirect_bind_group,
        ) {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("particle-trail-prepare-indirect"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }

        // 4. Copy counter buffer to staging for CPU readback (1-frame latency)
        // Skip if previous map is still pending (buffer would be mapped → submit error)
        if !self.counter_map_pending.load(std::sync::atomic::Ordering::Relaxed) {
            encoder.copy_buffer_to_buffer(&self.counter_buffer, 0, &self.counter_readback, 0, 16);
        }
    }

    /// Request async map of the counter readback buffer.
    /// Call once per frame after queue.submit().
    pub fn request_counter_readback(&self) {
        use std::sync::atomic::Ordering;
        if self.counter_map_pending.load(Ordering::Relaxed) {
            return; // Previous map still pending
        }
        // Set pending BEFORE map_async — wgpu considers buffer mapped immediately
        self.counter_map_pending.store(true, Ordering::Release);
        let pending = self.counter_map_pending.clone();
        let ready = self.counter_map_ready.clone();
        self.counter_readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    ready.store(true, Ordering::Release);
                } else {
                    // Map failed — reset pending so we can retry next frame
                    pending.store(false, Ordering::Release);
                }
            });
    }

    /// Poll the counter readback. If the map completed, read alive count and unmap.
    /// Call once per frame before dispatch.
    pub fn poll_counter_readback(&mut self) {
        use std::sync::atomic::Ordering;
        if !self.counter_map_ready.load(Ordering::Acquire) {
            return; // Map not yet complete
        }
        {
            let view = self.counter_readback.slice(..).get_mapped_range();
            let data: &[u32] = bytemuck::cast_slice(&view);
            self.alive_count = data[0];
        }
        self.counter_readback.unmap();
        self.counter_map_ready.store(false, Ordering::Release);
        self.counter_map_pending.store(false, Ordering::Release);
    }

    /// Render particles into the given target using indirect draw.
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

        // Render trails first (behind particles) if active
        if let (Some(trail_pipeline), Some(trail_bgs), Some(trail_indirect)) = (
            &self.trail_render_pipeline,
            &self.trail_render_bind_groups,
            &self.trail_indirect_args_buffer,
        ) {
            pass.set_pipeline(trail_pipeline);
            pass.set_bind_group(0, &trail_bgs[output_idx], &[]);
            pass.draw_indirect(trail_indirect, 0);
        }

        let pipeline = if self.blend_mode == "alpha" {
            &self.render_pipeline_alpha
        } else {
            &self.render_pipeline_additive
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.render_bind_groups[output_idx], &[]);
        pass.set_bind_group(1, &self.sprite_bind_group, &[]);
        // GPU-driven indirect draw: instance_count set by prepare_indirect shader
        pass.draw_indirect(&self.indirect_args_buffer, 0);
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
        let aux_size = (std::mem::size_of::<ParticleAux>() * (data.len().max(self.max_particles as usize)).max(1)) as u64;
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
            &self.pos_life_buffers,
            &self.vel_size_buffers,
            &self.color_buffers,
            &self.flags_buffers,
            &self.counter_buffer,
            &self.aux_buffer,
            &self.dead_index_buffer,
            &self.alive_index_buffers,
        );
    }

    /// Update aux data via write_buffer without recreating buffer or bind groups.
    /// Buffer must already be pre-allocated at max_particles size (done in `new()`).
    /// Used for per-frame updates when video/webcam source changes particle home positions.
    pub fn update_aux_in_place(&self, queue: &Queue, data: &[ParticleAux]) {
        if data.is_empty() {
            return;
        }
        let byte_len = (std::mem::size_of::<ParticleAux>() * data.len()) as u64;
        if byte_len <= self.aux_buffer.size() {
            queue.write_buffer(&self.aux_buffer, 0, bytemuck::cast_slice(data));
        } else {
            log::warn!(
                "Aux buffer too small: need {} bytes, have {} bytes ({} vs {} particles)",
                byte_len, self.aux_buffer.size(),
                data.len(), self.aux_buffer.size() as usize / std::mem::size_of::<ParticleAux>()
            );
        }
    }

    /// Advance the particle image source (video playback). If the frame changed,
    /// re-samples aux data and uploads to GPU. Returns true if aux was updated.
    pub fn update_source(&mut self, queue: &Queue, dt_secs: f64) -> bool {
        if self.image_source.is_static() {
            return false;
        }

        let frame_changed = self.image_source.advance(dt_secs);
        if !frame_changed {
            return false;
        }

        if let Some(frame) = self.image_source.current_frame_data() {
            let aux = super::image_source::sample_rgba_buffer(
                &frame.data,
                frame.width,
                frame.height,
                &self.sample_def,
                self.max_particles,
            );
            if !aux.is_empty() {
                self.update_aux_in_place(queue, &aux);
                self.current_aux = aux;
            }
            return true;
        }
        false
    }

    /// Set a video file as the particle source. Initiates a transition from current aux.
    #[cfg(feature = "video")]
    pub fn set_video_source(
        &mut self,
        queue: &Queue,
        frames: Vec<crate::media::types::DecodedFrame>,
        delays_ms: Vec<u32>,
        path: String,
    ) {
        // Sample first frame for immediate display + transition target
        let first_aux = if let Some(frame) = frames.first() {
            super::image_source::sample_rgba_buffer(
                &frame.data,
                frame.width,
                frame.height,
                &self.sample_def,
                self.max_particles,
            )
        } else {
            Vec::new()
        };

        // Start transition from current aux to new
        if !self.current_aux.is_empty() && !first_aux.is_empty() {
            self.source_transition = Some(SourceTransition {
                from_aux: self.current_aux.clone(),
                to_aux: first_aux.clone(),
                progress: 0.0,
                duration_secs: 0.5,
            });
        } else if !first_aux.is_empty() {
            // No transition — just upload directly
            self.update_aux_in_place(queue, &first_aux);
        }

        self.current_aux = first_aux;
        self.has_aux_data = true;
        self.video_path = Some(path);
        self.image_source = ParticleImageSource::Video {
            frames,
            delays_ms,
            current_frame: 0,
            frame_elapsed_ms: 0.0,
            playing: true,
            looping: true,
            speed: 1.0,
        };
    }

    /// Set a webcam as the particle source. Frames will be provided via `update_webcam_frame()`.
    #[cfg(feature = "webcam")]
    pub fn set_webcam_source(&mut self, queue: &Queue, width: u32, height: u32) {
        // Start transition from current aux if we have any
        let empty_aux: Vec<ParticleAux> = Vec::new();
        if !self.current_aux.is_empty() {
            // Transition will complete once first webcam frame arrives
            self.source_transition = Some(SourceTransition {
                from_aux: self.current_aux.clone(),
                to_aux: empty_aux,
                progress: 0.0,
                duration_secs: 0.5,
            });
        }

        self.has_aux_data = true;
        self.video_path = None;
        let _ = queue; // used for API consistency
        self.image_source = ParticleImageSource::Webcam { width, height };
    }

    /// Update aux data from a webcam frame. Called per-frame from the webcam drain loop.
    pub fn update_webcam_frame(&mut self, queue: &Queue, data: &[u8], width: u32, height: u32) {
        let aux = super::image_source::sample_rgba_buffer(
            data,
            width,
            height,
            &self.sample_def,
            self.max_particles,
        );
        if !aux.is_empty() {
            // If we have an active transition with empty to_aux (first webcam frame), fill it
            if let Some(ref mut trans) = self.source_transition {
                if trans.to_aux.is_empty() {
                    trans.to_aux = aux.clone();
                }
            }
            self.update_aux_in_place(queue, &aux);
            self.current_aux = aux;
        }
    }

    /// Advance source transition animation. Uploads blended aux data.
    pub fn advance_transition(&mut self, queue: &Queue, dt_secs: f32) {
        // Take transition out to avoid borrow conflict with self methods
        let mut trans = match self.source_transition.take() {
            Some(t) => t,
            None => return,
        };

        trans.progress += dt_secs / trans.duration_secs;
        if trans.progress >= 1.0 {
            // Upload final target positions
            if !trans.to_aux.is_empty() {
                self.update_aux_in_place(queue, &trans.to_aux);
                self.current_aux = trans.to_aux;
            }
            // transition is dropped (not put back)
        } else {
            // Upload interpolated positions
            let blended = trans.interpolated();
            if !blended.is_empty() {
                self.update_aux_in_place(queue, &blended);
            }
            self.source_transition = Some(trans);
        }
    }

    /// Store current aux data (called after initial image load so transitions have a "from").
    pub fn store_current_aux(&mut self, aux: Vec<ParticleAux>) {
        self.current_aux = aux;
    }

    /// Reset sprite, aux data, and compute shader to defaults.
    #[allow(dead_code)]
    pub fn clear_customization(&mut self, device: &Device, queue: &Queue) {
        // Reset sprite to placeholder
        let placeholder = super::sprite::create_placeholder_sprite(device, queue);
        self.sprite_bind_group = create_sprite_bind_group(device, &self.sprite_bgl, &placeholder);
        self.sprite = None;
        self.render_uniforms.render_mode = 0;
        self.render_uniforms.sprite_cols = 1;
        self.render_uniforms.sprite_rows = 1;
        self.render_uniforms.sprite_frames = 1;
        self.blend_mode = "additive".to_string();

        // Clear aux data
        self.upload_aux_data(device, queue, &[]);

        // Restore default compute shader
        // NOTE: caller should provide source with libraries prepended via
        // EffectLoader::prepend_compute_libraries(). This fallback uses raw source.
        let default_source =
            include_str!("../../../../../assets/shaders/builtin/particle_sim.wgsl");
        if let Err(e) = self.recompile_compute(device, default_source) {
            log::error!("Failed to restore default compute shader: {e}");
        } else {
            self.current_compute_source = default_source.to_string();
        }

        // Reset emitter shape
        self.def.emitter.shape = "point".to_string();
    }

    /// Enable or upgrade the flow field texture. If enabling for the first time,
    /// bakes the full 64x64x64 curl noise texture.
    pub fn set_flow_field(&mut self, device: &Device, queue: &Queue, enabled: bool) {
        self.def.flow_field = enabled;
        let new_field = if enabled {
            FlowFieldTexture::new(device, queue)
        } else {
            FlowFieldTexture::placeholder(device, queue)
        };
        self.flow_field = new_field;
        self.rebuild_flow_field_bind_group(device);
    }

    /// Set obstacle from RGBA image data. Enables obstacle collision.
    pub fn set_obstacle_image(
        &mut self,
        device: &Device,
        queue: &Queue,
        data: &[u8],
        w: u32,
        h: u32,
        path: Option<String>,
    ) {
        self.obstacle = ObstacleTexture::from_rgba(device, queue, data, w, h);
        self.obstacle_enabled = true;
        self.obstacle_source = "image".to_string();
        self.obstacle_image_path = path;
        self.rebuild_flow_field_bind_group(device);
    }

    /// Update obstacle texture from webcam frame data (per-frame).
    pub fn update_obstacle_webcam(&mut self, device: &Device, queue: &Queue, data: &[u8], w: u32, h: u32) {
        let dims_changed = w != self.obstacle.width || h != self.obstacle.height;
        self.obstacle.update(device, queue, data, w, h);
        if dims_changed {
            self.rebuild_flow_field_bind_group(device);
        }
    }

    /// Set obstacle from pre-decoded video frames.
    /// Video frames have alpha=1.0 everywhere, so we convert luminance to alpha
    /// so bright areas become solid obstacles and dark areas are passable.
    pub fn set_obstacle_video(
        &mut self,
        device: &Device,
        queue: &Queue,
        frames: Vec<crate::media::types::DecodedFrame>,
        delays_ms: Vec<u32>,
        path: String,
    ) {
        if let Some(frame) = frames.first() {
            let converted = Self::luminance_to_alpha(&frame.data);
            self.obstacle = ObstacleTexture::from_rgba(device, queue, &converted, frame.width, frame.height);
            self.rebuild_flow_field_bind_group(device);
        }
        self.obstacle_video_frames = frames;
        self.obstacle_video_delays_ms = delays_ms;
        self.obstacle_video_frame = 0;
        self.obstacle_video_elapsed_ms = 0.0;
        self.obstacle_video_playing = true;
        self.obstacle_video_looping = true;
        self.obstacle_video_speed = 1.0;
        self.obstacle_enabled = true;
        self.obstacle_source = "video".to_string();
        self.obstacle_image_path = Some(path);
    }

    /// Convert RGBA data so alpha = luminance. Videos from ffmpeg have alpha=1.0
    /// everywhere, which makes the entire frame an obstacle. This maps brightness
    /// to alpha so bright regions become solid obstacles and dark regions are passable.
    fn luminance_to_alpha(data: &[u8]) -> Vec<u8> {
        let mut out = data.to_vec();
        for pixel in out.chunks_exact_mut(4) {
            // Perceptual luminance: 0.299R + 0.587G + 0.114B
            let lum = (pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8;
            pixel[3] = lum as u8;
        }
        out
    }

    /// Advance obstacle video playback and update texture if frame changed.
    /// Call from the main update loop. Returns true if texture was updated.
    pub fn advance_obstacle_video(&mut self, device: &Device, queue: &Queue, dt_secs: f64) -> bool {
        if !self.obstacle_video_playing || self.obstacle_video_frames.is_empty() {
            return false;
        }
        self.obstacle_video_elapsed_ms += dt_secs * 1000.0 * self.obstacle_video_speed as f64;
        let delay = self.obstacle_video_delays_ms
            .get(self.obstacle_video_frame)
            .copied()
            .unwrap_or(33) as f64;
        if self.obstacle_video_elapsed_ms < delay {
            return false;
        }
        self.obstacle_video_elapsed_ms -= delay;
        let next = self.obstacle_video_frame + 1;
        if next >= self.obstacle_video_frames.len() {
            if self.obstacle_video_looping {
                self.obstacle_video_frame = 0;
            } else {
                self.obstacle_video_playing = false;
                return false;
            }
        } else {
            self.obstacle_video_frame = next;
        }
        // Upload new frame (with luminance-to-alpha conversion)
        if let Some(frame) = self.obstacle_video_frames.get(self.obstacle_video_frame) {
            let dims_changed = frame.width != self.obstacle.width || frame.height != self.obstacle.height;
            let converted = Self::luminance_to_alpha(&frame.data);
            self.obstacle.update(device, queue, &converted, frame.width, frame.height);
            if dims_changed {
                self.rebuild_flow_field_bind_group(device);
            }
        }
        true
    }

    /// Clear obstacle texture, disabling collision.
    pub fn clear_obstacle(&mut self, device: &Device, queue: &Queue) {
        self.obstacle = ObstacleTexture::placeholder(device, queue);
        self.obstacle_enabled = false;
        self.obstacle_source.clear();
        self.obstacle_image_path = None;
        self.obstacle_video_frames.clear();
        self.obstacle_video_delays_ms.clear();
        self.obstacle_video_frame = 0;
        self.rebuild_flow_field_bind_group(device);
    }

    /// Rebuild group 1 bind group (flow field + obstacle).
    fn rebuild_flow_field_bind_group(&mut self, device: &Device) {
        self.flow_field_bind_group =
            create_flow_field_bind_group(device, &self.flow_field_bgl, &self.flow_field, &self.obstacle);
    }

    /// Set up spatial hash grid for particle-particle interaction.
    pub fn setup_spatial_hash(&mut self, device: &Device) {
        let hash = SpatialHashGrid::new(
            device,
            self.max_particles,
            &self.pos_life_buffers,
            &self.uniform_buffer,
        );
        self.spatial_hash = Some(hash);

        // Recompile pipeline with spatial hash BGL (group 3) now included
        if let Err(e) = self.recompile_compute(device, &self.current_compute_source.clone()) {
            log::error!("Failed to recompile compute pipeline with spatial hash: {e}");
        }
    }

    /// Get the spatial hash query bind group layout, if interaction is enabled.
    pub fn spatial_hash_query_bgl(&self) -> Option<&BindGroupLayout> {
        self.spatial_hash.as_ref().map(|h| &h.query_bgl)
    }

    /// Get the spatial hash query bind group, if interaction is enabled.
    pub fn spatial_hash_query_bg(&self) -> Option<&BindGroup> {
        self.spatial_hash.as_ref().map(|h| &h.query_bind_group)
    }

    /// Set up trail rendering with the given trail length.
    /// Creates trail buffer, compute bind group for trail writes, and render pipeline.
    pub fn setup_trails(
        &mut self,
        device: &Device,
        hdr_format: TextureFormat,
        trail_length: u32,
        trail_width: f32,
    ) {
        if trail_length < 2 {
            self.trail_buffer = None;
            self.trail_length = 0;
            self.trail_render_pipeline = None;
            self.trail_render_bgl = None;
            self.trail_render_bind_groups = None;
            self.trail_compute_bgl = None;
            self.trail_compute_bind_group = None;
            return;
        }

        // Cap trail length to stay within device storage buffer binding limit.
        // Trail buffer = max_particles × trail_length × 16 bytes.
        // Disable trails entirely above 500K particles to avoid massive allocations.
        const MAX_TRAIL_PARTICLES: u32 = 500_000;
        if self.max_particles > MAX_TRAIL_PARTICLES {
            log::warn!(
                "Trails disabled: {} particles exceeds {} limit for trail rendering",
                self.max_particles,
                MAX_TRAIL_PARTICLES,
            );
            self.trail_buffer = None;
            self.trail_length = 0;
            self.trail_render_pipeline = None;
            self.trail_render_bgl = None;
            self.trail_render_bind_groups = None;
            self.trail_compute_bgl = None;
            self.trail_compute_bind_group = None;
            return;
        }

        let max_trail_buf: u64 = device.limits().max_storage_buffer_binding_size as u64;
        let effective_trail_length = {
            let max_len = max_trail_buf / (self.max_particles as u64 * 16);
            let capped = trail_length.min(max_len as u32);
            if capped < trail_length {
                log::warn!(
                    "Trail length capped from {} to {} ({}×{}×16 would exceed {}MB binding limit)",
                    trail_length,
                    capped,
                    self.max_particles,
                    trail_length,
                    max_trail_buf / (1024 * 1024),
                );
            }
            if capped < 2 {
                log::warn!("Trail length too short after capping — trails disabled");
                self.trail_buffer = None;
                self.trail_length = 0;
                self.trail_render_pipeline = None;
                self.trail_render_bgl = None;
                self.trail_render_bind_groups = None;
                self.trail_compute_bgl = None;
                self.trail_compute_bind_group = None;
                return;
            }
            capped
        };

        self.trail_length = effective_trail_length;
        self.trail_width = trail_width;

        // Trail buffer: max_particles * trail_length * 16 bytes (vec4f per point: xy=pos, z=size, w=alpha)
        let trail_buf_size = self.max_particles as u64 * effective_trail_length as u64 * 16;
        let trail_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-trail-buffer"),
            size: trail_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Compute bind group for trail writes (group 2)
        let trail_compute_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-trail-compute-bgl"),
            entries: &[
                // binding 0: trail buffer (read_write)
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let trail_compute_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("particle-trail-compute-bg"),
            layout: &trail_compute_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: trail_buffer.as_entire_binding(),
            }],
        });

        // Trail render bind group layout — SoA: 4 component buffers + uniforms + alive + trail
        let trail_render_storage_ro = |binding: u32| -> BindGroupLayoutEntry {
            BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        };
        let trail_render_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("particle-trail-render-bgl"),
            entries: &[
                trail_render_storage_ro(0), // pos_life
                trail_render_storage_ro(1), // vel_size
                trail_render_storage_ro(2), // color
                trail_render_storage_ro(3), // flags
                // binding 4: render uniforms
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                trail_render_storage_ro(5), // alive_indices
                trail_render_storage_ro(6), // trail buffer
            ],
        });

        // Trail render bind groups (ping-pong: read from output side)
        let trail_render_bind_groups = create_trail_render_bind_groups(
            device,
            &trail_render_bgl,
            &self.pos_life_buffers,
            &self.vel_size_buffers,
            &self.color_buffers,
            &self.flags_buffers,
            &self.render_uniform_buffer,
            &self.alive_index_buffers,
            &trail_buffer,
        );

        // Trail render pipeline
        let render_source =
            include_str!("../../../../../assets/shaders/builtin/particle_render_trail.wgsl");
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle-trail-render"),
            source: wgpu::ShaderSource::Wgsl(render_source.into()),
        });

        let render_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("particle-trail-render-layout"),
            bind_group_layouts: &[&trail_render_bgl],
            push_constant_ranges: &[],
        });

        let trail_render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("particle-trail-render"),
                layout: Some(&render_layout),
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

        // Store trail compute BGL/BG now so recompile can see them
        self.trail_compute_bgl = Some(trail_compute_bgl);
        self.trail_compute_bind_group = Some(trail_compute_bind_group);

        // Recompile compute pipeline with trail bind group (group 2)
        if let Err(e) = self.recompile_compute(device, &self.current_compute_source.clone()) {
            log::error!("Failed to recompile compute pipeline with trails: {e}");
        }

        // Trail indirect args buffer (vertex_count = 6*(trail_length-1), instance_count from alive)
        let trail_indirect_args_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle-trail-indirect-args"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Trail prepare indirect pipeline (reads counters, writes trail-specific DrawIndirectArgs)
        let (trail_prepare_pipeline, trail_prepare_bg) = create_trail_prepare_indirect_pipeline(
            device,
            &self.counter_buffer,
            &trail_indirect_args_buffer,
            trail_length,
        );

        self.trail_buffer = Some(trail_buffer);
        self.trail_render_pipeline = Some(trail_render_pipeline);
        self.trail_render_bgl = Some(trail_render_bgl);
        self.trail_render_bind_groups = Some(trail_render_bind_groups);
        // trail_compute_bgl and trail_compute_bind_group already stored above (before recompile)
        self.trail_indirect_args_buffer = Some(trail_indirect_args_buffer);
        self.trail_prepare_indirect_pipeline = Some(trail_prepare_pipeline);
        self.trail_prepare_indirect_bind_group = Some(trail_prepare_bg);
    }

    /// Flip ping-pong buffers for next frame.
    pub fn flip(&mut self) {
        self.current = 1 - self.current;
        self.frame_index = self.frame_index.wrapping_add(1);
    }
}

/// Create the prepare-indirect compute pipeline and bind groups.
/// This is a simple 1-thread shader that reads counters and writes DrawIndirectArgs.
fn create_prepare_indirect_pipeline(
    device: &Device,
    counter_buffer: &wgpu::Buffer,
    indirect_args_buffer: &wgpu::Buffer,
) -> (ComputePipeline, [BindGroup; 2]) {
    let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("prepare-indirect-bgl"),
        entries: &[
            // binding 0: counters (read — non-atomic view for prepare shader)
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // binding 1: indirect_args (write)
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let source = include_str!("../../../../../assets/shaders/builtin/particle_prepare_indirect.wgsl");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("prepare-indirect"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("prepare-indirect-layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("prepare-indirect-pipeline"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: PipelineCompilationOptions::default(),
        cache: None,
    });

    // Same bind group for both ping-pong states (counter and indirect buffers are shared)
    let bg = device.create_bind_group(&BindGroupDescriptor {
        label: Some("prepare-indirect-bg"),
        layout: &bgl,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: counter_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: indirect_args_buffer.as_entire_binding(),
            },
        ],
    });

    // Clone the bind group reference — both ping-pong states use the same bind group
    let bg2 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("prepare-indirect-bg-1"),
        layout: &bgl,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: counter_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: indirect_args_buffer.as_entire_binding(),
            },
        ],
    });

    (pipeline, [bg, bg2])
}

fn create_compute_bind_groups(
    device: &Device,
    layout: &BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    pos_life_buffers: &[wgpu::Buffer; 2],
    vel_size_buffers: &[wgpu::Buffer; 2],
    color_buffers: &[wgpu::Buffer; 2],
    flags_buffers: &[wgpu::Buffer; 2],
    counter_buffer: &wgpu::Buffer,
    aux_buffer: &wgpu::Buffer,
    dead_index_buffer: &wgpu::Buffer,
    alive_index_buffers: &[wgpu::Buffer; 2],
) -> [BindGroup; 2] {
    // bind_group[0]: read from [0], write to [1]
    let bg0 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-compute-bg-0"),
        layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: pos_life_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 2, resource: vel_size_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 3, resource: color_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 4, resource: flags_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 5, resource: pos_life_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 6, resource: vel_size_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 7, resource: color_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 8, resource: flags_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 9, resource: counter_buffer.as_entire_binding() },
            BindGroupEntry { binding: 10, resource: aux_buffer.as_entire_binding() },
            BindGroupEntry { binding: 11, resource: dead_index_buffer.as_entire_binding() },
            BindGroupEntry { binding: 12, resource: alive_index_buffers[1].as_entire_binding() },
        ],
    });
    // bind_group[1]: read from [1], write to [0]
    let bg1 = device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-compute-bg-1"),
        layout,
        entries: &[
            BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            BindGroupEntry { binding: 1, resource: pos_life_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 2, resource: vel_size_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 3, resource: color_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 4, resource: flags_buffers[1].as_entire_binding() },
            BindGroupEntry { binding: 5, resource: pos_life_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 6, resource: vel_size_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 7, resource: color_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 8, resource: flags_buffers[0].as_entire_binding() },
            BindGroupEntry { binding: 9, resource: counter_buffer.as_entire_binding() },
            BindGroupEntry { binding: 10, resource: aux_buffer.as_entire_binding() },
            BindGroupEntry { binding: 11, resource: dead_index_buffer.as_entire_binding() },
            BindGroupEntry { binding: 12, resource: alive_index_buffers[0].as_entire_binding() },
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

/// Pack a variable-length curve into a fixed [f32; 8] LUT.
/// Empty input → all zeros. Shorter input is stretched, longer is truncated.
fn pack_curve_lut(values: &[f32]) -> [f32; 8] {
    if values.is_empty() {
        return [0.0; 8];
    }
    let mut lut = [0.0f32; 8];
    if values.len() == 1 {
        lut.fill(values[0]);
        return lut;
    }
    // Resample to 8 points via linear interpolation
    for i in 0..8 {
        let t = i as f32 / 7.0;
        let src_idx = t * (values.len() - 1) as f32;
        let lo = (src_idx as usize).min(values.len() - 1);
        let hi = (lo + 1).min(values.len() - 1);
        let frac = src_idx - lo as f32;
        lut[i] = values[lo] * (1.0 - frac) + values[hi] * frac;
    }
    lut
}

fn create_flow_field_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    flow_field: &FlowFieldTexture,
    obstacle: &ObstacleTexture,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("particle-flow-field-bg"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&flow_field.view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&flow_field.sampler),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&obstacle.view),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Sampler(&obstacle.sampler),
            },
        ],
    })
}

fn create_render_bind_groups(
    device: &Device,
    layout: &BindGroupLayout,
    pos_life_buffers: &[wgpu::Buffer; 2],
    vel_size_buffers: &[wgpu::Buffer; 2],
    color_buffers: &[wgpu::Buffer; 2],
    flags_buffers: &[wgpu::Buffer; 2],
    render_uniform_buffer: &wgpu::Buffer,
    alive_index_buffers: &[wgpu::Buffer; 2],
) -> [BindGroup; 2] {
    let make_bg = |i: usize, label: &str| -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: pos_life_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 1, resource: vel_size_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 2, resource: color_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 3, resource: flags_buffers[i].as_entire_binding() },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: render_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                BindGroupEntry { binding: 5, resource: alive_index_buffers[i].as_entire_binding() },
            ],
        })
    };
    [make_bg(0, "particle-render-bg-0"), make_bg(1, "particle-render-bg-1")]
}

fn create_trail_render_bind_groups(
    device: &Device,
    layout: &BindGroupLayout,
    pos_life_buffers: &[wgpu::Buffer; 2],
    vel_size_buffers: &[wgpu::Buffer; 2],
    color_buffers: &[wgpu::Buffer; 2],
    flags_buffers: &[wgpu::Buffer; 2],
    render_uniform_buffer: &wgpu::Buffer,
    alive_index_buffers: &[wgpu::Buffer; 2],
    trail_buffer: &wgpu::Buffer,
) -> [BindGroup; 2] {
    let make_bg = |i: usize, label: &str| -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                BindGroupEntry { binding: 0, resource: pos_life_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 1, resource: vel_size_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 2, resource: color_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 3, resource: flags_buffers[i].as_entire_binding() },
                BindGroupEntry {
                    binding: 4,
                    resource: BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: render_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                BindGroupEntry { binding: 5, resource: alive_index_buffers[i].as_entire_binding() },
                BindGroupEntry { binding: 6, resource: trail_buffer.as_entire_binding() },
            ],
        })
    };
    [make_bg(0, "particle-trail-render-bg-0"), make_bg(1, "particle-trail-render-bg-1")]
}

fn create_trail_prepare_indirect_pipeline(
    device: &Device,
    counter_buffer: &wgpu::Buffer,
    trail_indirect_args_buffer: &wgpu::Buffer,
    trail_length: u32,
) -> (ComputePipeline, BindGroup) {
    let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("trail-prepare-indirect-bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    // Bake trail_length into shader source for vertex count calculation
    let verts_per_instance = 6 * (trail_length.max(2) - 1);
    let source = format!(
        r#"
@group(0) @binding(0) var<storage, read> counters: array<u32, 4>;
@group(0) @binding(1) var<storage, read_write> indirect_args: array<u32, 4>;

@compute @workgroup_size(1)
fn cs_main() {{
    let alive_count = counters[0];
    indirect_args[0] = {verts_per_instance}u;
    indirect_args[1] = alive_count;
    indirect_args[2] = 0u;
    indirect_args[3] = 0u;
}}
"#
    );

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("trail-prepare-indirect"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("trail-prepare-indirect-layout"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("trail-prepare-indirect-pipeline"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: PipelineCompilationOptions::default(),
        cache: None,
    });

    let bg = device.create_bind_group(&BindGroupDescriptor {
        label: Some("trail-prepare-indirect-bg"),
        layout: &bgl,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: counter_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: trail_indirect_args_buffer.as_entire_binding(),
            },
        ],
    });

    (pipeline, bg)
}

/// Compute bitonic sort pass parameters: (block_size, sub_block_size) for each step.
fn bitonic_sort_passes(n: u32) -> Vec<(u32, u32)> {
    let mut passes = Vec::new();
    let mut k = 2u32;
    while k <= n {
        let mut j = k / 2;
        while j > 0 {
            passes.push((k, j));
            j /= 2;
        }
        k *= 2;
    }
    passes
}

/// Next power of 2 >= n.
fn next_power_of_2(n: u32) -> u32 {
    if n <= 1 {
        return 1;
    }
    1u32 << (32 - (n - 1).leading_zeros())
}

/// Create all GPU resources for depth-sorted particle rendering.
#[allow(clippy::type_complexity)]
fn create_sort_resources(
    device: &Device,
    max_particles: u32,
    counter_buffer: &wgpu::Buffer,
    vel_size_buffers: &[wgpu::Buffer; 2],
    alive_index_buffers: &[wgpu::Buffer; 2],
) -> (
    Option<wgpu::Buffer>,          // sort_key_buffer
    Option<wgpu::Buffer>,          // sort_params_buffer
    Option<ComputePipeline>,       // sort_keygen_pipeline
    Option<[BindGroup; 2]>,        // sort_keygen_bind_groups
    Option<ComputePipeline>,       // sort_pipeline
    Option<[BindGroup; 2]>,        // sort_bind_groups
    Vec<(u32, u32)>,               // sort_passes
    u32,                           // sort_n
) {
    let sort_n = next_power_of_2(max_particles);
    let passes = bitonic_sort_passes(sort_n);

    // Sort key buffer: f32 per slot (padded to sort_n)
    let sort_key_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particle-sort-keys"),
        size: sort_n as u64 * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Sort params buffer: one 16-byte entry per pass, 256-byte aligned for dynamic offsets
    let min_align = 256u64;
    let params_buffer_size = passes.len() as u64 * min_align;
    let sort_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particle-sort-params"),
        size: params_buffer_size.max(min_align), // at least one entry
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Write all pass parameters at creation time
    {
        let mut data = vec![0u8; params_buffer_size as usize];
        for (i, &(block_size, sub_block_size)) in passes.iter().enumerate() {
            let offset = i * min_align as usize;
            let params: [u32; 4] = [block_size, sub_block_size, sort_n, 0];
            data[offset..offset + 16].copy_from_slice(bytemuck::cast_slice(&params));
        }
        // Note: buffer write happens via queue in dispatch, but we can init here via mapped_at_creation
        // Actually, we'll write in the dispatch path. Skip for now — the buffer is zero-initialized.
        // We write all params once in the calling code after creation.
        // For simplicity, store the data and write it later.
        // Actually, we need queue here. Let's defer writing to the first dispatch.
        // Instead, store passes for CPU-side reference and write per-frame.
        // UPDATE: Actually, the params are static — we can write them at creation if we have the queue.
        // Since we don't have queue in this function, we'll handle it differently.
        let _ = data; // Params written at first dispatch
    }

    // --- Keygen pipeline ---
    let keygen_source = include_str!("../../../../../assets/shaders/builtin/particle_sort_keygen.wgsl");
    let keygen_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("particle-sort-keygen-shader"),
        source: wgpu::ShaderSource::Wgsl(keygen_source.into()),
    });

    let keygen_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sort-keygen-bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
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
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
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
        ],
    });

    let keygen_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("particle-sort-keygen"),
        layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("sort-keygen-layout"),
            bind_group_layouts: &[&keygen_bgl],
            push_constant_ranges: &[],
        })),
        module: &keygen_shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Keygen bind groups: one per ping-pong state
    // After sim, output particles are in storage_buffers[1-current], alive_indices in alive_index_buffers[1-current]
    // But keygen runs before flip, so output = [1-current]
    // bg[0] is for when current=0 (output in buffers[1])
    // bg[1] is for when current=1 (output in buffers[0])
    let keygen_bind_groups = [
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("sort-keygen-bg-0"),
            layout: &keygen_bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: counter_buffer.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: vel_size_buffers[1].as_entire_binding() },
                BindGroupEntry { binding: 2, resource: alive_index_buffers[1].as_entire_binding() },
                BindGroupEntry { binding: 3, resource: sort_key_buffer.as_entire_binding() },
            ],
        }),
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("sort-keygen-bg-1"),
            layout: &keygen_bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: counter_buffer.as_entire_binding() },
                BindGroupEntry { binding: 1, resource: vel_size_buffers[0].as_entire_binding() },
                BindGroupEntry { binding: 2, resource: alive_index_buffers[0].as_entire_binding() },
                BindGroupEntry { binding: 3, resource: sort_key_buffer.as_entire_binding() },
            ],
        }),
    ];

    // --- Sort pipeline ---
    let sort_source = include_str!("../../../../../assets/shaders/builtin/particle_sort.wgsl");
    let sort_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("particle-sort-shader"),
        source: wgpu::ShaderSource::Wgsl(sort_source.into()),
    });

    let sort_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sort-bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
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
        ],
    });

    let sort_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("particle-sort"),
        layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("sort-layout"),
            bind_group_layouts: &[&sort_bgl],
            push_constant_ranges: &[],
        })),
        module: &sort_shader,
        entry_point: Some("cs_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    // Sort bind groups: one per alive_index buffer
    // bg[0] is for when current=0 (alive_indices output in buffers[1])
    // bg[1] is for when current=1 (alive_indices output in buffers[0])
    let sort_bind_groups = [
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("sort-bg-0"),
            layout: &sort_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &sort_params_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(16),
                    }),
                },
                BindGroupEntry { binding: 1, resource: sort_key_buffer.as_entire_binding() },
                BindGroupEntry { binding: 2, resource: alive_index_buffers[1].as_entire_binding() },
            ],
        }),
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("sort-bg-1"),
            layout: &sort_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &sort_params_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(16),
                    }),
                },
                BindGroupEntry { binding: 1, resource: sort_key_buffer.as_entire_binding() },
                BindGroupEntry { binding: 2, resource: alive_index_buffers[0].as_entire_binding() },
            ],
        }),
    ];

    (
        Some(sort_key_buffer),
        Some(sort_params_buffer),
        Some(keygen_pipeline),
        Some(keygen_bind_groups),
        Some(sort_pipeline),
        Some(sort_bind_groups),
        passes,
        sort_n,
    )
}

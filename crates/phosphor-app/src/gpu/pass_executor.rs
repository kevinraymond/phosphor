use wgpu::{CommandEncoder, Device, Queue, Sampler, TextureFormat, TextureView};

use crate::effect::EffectLoader;
use crate::effect::format::PassDef;

use super::ShaderPipeline;
use super::audio_textures::AudioTextures;
use super::particle::ParticleSystem;
use super::placeholder::PlaceholderTexture;
use super::render_target::{PingPongTarget, RenderTarget};
use super::uniforms::UniformBuffer;

/// A compiled pass: pipeline + render target + bind groups.
struct CompiledPass {
    name: String,
    pipeline: ShaderPipeline,
    /// Ping-pong target for this pass (feedback-capable).
    target: PingPongTarget,
    /// Bind groups indexed by the executor's global flip parity (#1481), not by
    /// this pass's own `target.current`: a non-feedback pass must still read a
    /// feedback input at the right parity.
    bind_groups: [wgpu::BindGroup; 2],
    has_feedback: bool,
    /// Indices (into the pass list) of the prior passes this pass samples as
    /// `input0..inputN-1`. Resolved from `PassDef.inputs`; always earlier passes.
    input_srcs: Vec<usize>,
}

/// Everything the bind-group builder needs about each pass, borrowed. Lets one
/// builder serve both construction (from freshly prepared passes) and rebuilds
/// (from the live `CompiledPass` list) without a per-pass mutable/immutable
/// aliasing conflict — see `rebuild_all_bind_groups`.
struct PassView<'a> {
    layout: &'a wgpu::BindGroupLayout,
    target: &'a PingPongTarget,
    has_feedback: bool,
    input_srcs: &'a [usize],
}

/// A pass after pipeline + target creation but before its bind groups exist
/// (which need every pass's target to be resolvable). Construction two-phase.
struct PreparedPass {
    name: String,
    pipeline: ShaderPipeline,
    target: PingPongTarget,
    has_feedback: bool,
    input_srcs: Vec<usize>,
}

/// Executes a sequence of render passes for a multi-pass effect.
pub struct PassExecutor {
    passes: Vec<CompiledPass>,
    pub particle_system: Option<ParticleSystem>,
    /// Global ping-pong parity. All feedback passes flip in lockstep, so each
    /// feedback pass's `target.current` equals this value; bind groups are indexed
    /// by it so cross-pass reads land on the correct target every frame (#1481).
    flip_parity: usize,
}

impl PassExecutor {
    /// Build a PassExecutor from a list of PassDefs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &Device,
        hdr_format: TextureFormat,
        width: u32,
        height: u32,
        pass_defs: &[PassDef],
        effect_loader: &EffectLoader,
        uniform_buffer: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
        queue: &Queue,
        pipeline_cache: Option<&wgpu::PipelineCache>,
    ) -> Result<Self, String> {
        // Phase 1: resolve inputs, compile pipelines, create targets.
        let mut prepared: Vec<PreparedPass> = Vec::with_capacity(pass_defs.len());

        for (idx, def) in pass_defs.iter().enumerate() {
            // Resolve each declared input name to an earlier pass. Forward
            // references and unknown names are a hard error — the pass graph is a
            // DAG evaluated in declaration order.
            let mut input_srcs = Vec::with_capacity(def.inputs.len());
            for name in &def.inputs {
                let src = pass_defs[..idx]
                    .iter()
                    .position(|p| &p.name == name)
                    .ok_or_else(|| {
                        format!(
                            "Pass '{}' input '{name}' does not name an earlier pass",
                            def.name
                        )
                    })?;
                input_srcs.push(src);
            }

            let source = effect_loader
                .load_effect_source_with_inputs(&def.shader, def.inputs.len())
                .map_err(|e| format!("Failed to load shader '{}': {e}", def.shader))?;

            let pipeline = ShaderPipeline::new(
                device,
                hdr_format,
                &source,
                pipeline_cache,
                def.inputs.len(),
            )
            .map_err(|e| format!("Failed to compile shader '{}': {e}", def.shader))?;

            // Clear feedback targets to prevent NaN/garbage from uninitialized GPU memory
            let target = if def.feedback {
                PingPongTarget::new_cleared(device, queue, width, height, hdr_format, def.scale)
            } else {
                PingPongTarget::new(device, width, height, hdr_format, def.scale)
            };

            prepared.push(PreparedPass {
                name: def.name.clone(),
                pipeline,
                target,
                has_feedback: def.feedback,
                input_srcs,
            });
        }

        // Phase 2: build every pass's bind groups now that all targets exist.
        let views: Vec<PassView> = prepared
            .iter()
            .map(|p| PassView {
                layout: &p.pipeline.bind_group_layout,
                target: &p.target,
                has_feedback: p.has_feedback,
                input_srcs: &p.input_srcs,
            })
            .collect();
        let bind_groups: Vec<[wgpu::BindGroup; 2]> = (0..views.len())
            .map(|i| build_bind_groups(&views, i, device, uniform_buffer, placeholder, audio))
            .collect();
        drop(views);

        let passes = prepared
            .into_iter()
            .zip(bind_groups)
            .map(|(p, bg)| CompiledPass {
                name: p.name,
                pipeline: p.pipeline,
                target: p.target,
                bind_groups: bg,
                has_feedback: p.has_feedback,
                input_srcs: p.input_srcs,
            })
            .collect();

        Ok(Self {
            passes,
            particle_system: None,
            flip_parity: 0,
        })
    }

    /// Build a single-pass executor (the common case for backward-compatible effects).
    pub fn single_pass(
        pipeline: ShaderPipeline,
        feedback: PingPongTarget,
        uniform_buffer: &UniformBuffer,
        device: &Device,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
    ) -> Self {
        let bind_groups = {
            let views = [PassView {
                layout: &pipeline.bind_group_layout,
                target: &feedback,
                has_feedback: true, // always enable feedback for single-pass mode
                input_srcs: &[],
            }];
            build_bind_groups(&views, 0, device, uniform_buffer, placeholder, audio)
        };

        Self {
            passes: vec![CompiledPass {
                name: "main".to_string(),
                pipeline,
                target: feedback,
                bind_groups,
                has_feedback: true,
                input_srcs: Vec::new(),
            }],
            particle_system: None,
            flip_parity: 0,
        }
    }

    /// Execute all passes. Returns a reference to the final pass's write target.
    /// `viewport`: optional (width, height) to restrict rendering to a sub-region.
    pub fn execute(
        &self,
        encoder: &mut CommandEncoder,
        uniform_buffer: &UniformBuffer,
        queue: &Queue,
        uniforms: &super::ShaderUniforms,
    ) -> &RenderTarget {
        uniform_buffer.update(queue, uniforms);

        // 1. Particle compute dispatch (before fragment passes)
        if let Some(ref ps) = self.particle_system {
            ps.dispatch(encoder, queue);
        }

        // 2. Fragment shader passes
        for pass in &self.passes {
            let write_view = &pass.target.write_target().view;
            // Index by the global parity, not the pass's own `current`: a
            // non-feedback pass reading a feedback input must pick the bind group
            // that points at that input's current-frame target (#1481).
            let bind_group = &pass.bind_groups[self.flip_parity];

            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&pass.name),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: write_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rp.set_pipeline(&pass.pipeline.pipeline);
            rp.set_bind_group(0, bind_group, &[]);
            rp.draw(0..3, 0..1);
        }

        let final_target = self
            .passes
            .last()
            .expect("pipeline always has at least one pass")
            .target
            .write_target();

        // 3. Particle render pass — composites on top of last fragment pass with LoadOp::Load
        if let Some(ref ps) = self.particle_system {
            ps.render(encoder, queue, &final_target.view);
        }

        final_target
    }

    /// Flip all feedback-enabled passes for next frame, and advance the global
    /// parity in lockstep so cross-pass reads stay aligned.
    pub fn flip(&mut self) {
        self.flip_parity = 1 - self.flip_parity;
        for pass in &mut self.passes {
            if pass.has_feedback {
                pass.target.flip();
            }
        }
        if let Some(ref mut ps) = self.particle_system {
            ps.flip();
        }
    }

    /// Resize all pass targets (clears feedback targets to prevent NaN from uninitialized GPU memory).
    pub fn resize(
        &mut self,
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        uniform_buffer: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
    ) {
        // Phase 1: resize every target (recreates the textures behind them).
        for pass in &mut self.passes {
            if pass.has_feedback {
                pass.target.resize_cleared(device, queue, width, height);
            } else {
                pass.target.resize(device, width, height);
            }
        }
        // Phase 2: rebuild all bind groups against the new targets (a pass may
        // sample another pass's just-recreated target).
        self.rebuild_all_bind_groups(device, uniform_buffer, placeholder, audio);

        // Resize compute rasterizer framebuffer if active
        if let Some(ref mut ps) = self.particle_system {
            ps.resize_compute_raster(device, width, height);
            ps.resize_wboit(device, width, height);
        }
    }

    /// Rebuild every pass's bind groups from the current targets/layouts. Reads
    /// `&self.passes` to a `Vec<PassView>`, collects the new groups, then assigns
    /// — so cross-pass target references never alias a mutable borrow.
    fn rebuild_all_bind_groups(
        &mut self,
        device: &Device,
        uniform_buffer: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
    ) {
        let new_groups: Vec<[wgpu::BindGroup; 2]> = {
            let views: Vec<PassView> = self
                .passes
                .iter()
                .map(|p| PassView {
                    layout: &p.pipeline.bind_group_layout,
                    target: &p.target,
                    has_feedback: p.has_feedback,
                    input_srcs: &p.input_srcs,
                })
                .collect();
            (0..views.len())
                .map(|i| build_bind_groups(&views, i, device, uniform_buffer, placeholder, audio))
                .collect()
        };
        for (pass, bg) in self.passes.iter_mut().zip(new_groups) {
            pass.bind_groups = bg;
        }
    }

    /// Try to recompile a specific pass's shader (for hot-reload).
    /// NOTE: This blocks the main thread during compilation. Prefer using
    /// `ShaderCompiler` for background compilation + `swap_pass_pipeline()`.
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub fn recompile_pass(
        &mut self,
        pass_index: usize,
        device: &Device,
        hdr_format: TextureFormat,
        source: &str,
        uniform_buffer: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
        pipeline_cache: Option<&wgpu::PipelineCache>,
    ) -> Result<(), String> {
        if pass_index >= self.passes.len() {
            return Err(format!("Pass index {pass_index} out of range"));
        }
        // recreate_pipeline reuses the existing layout (same input_count), so the
        // rebuilt bind groups stay valid.
        self.passes[pass_index].pipeline.recreate_pipeline(
            device,
            hdr_format,
            source,
            pipeline_cache,
        )?;
        self.rebuild_all_bind_groups(device, uniform_buffer, placeholder, audio);
        Ok(())
    }

    /// Swap in a pre-compiled pipeline for a specific pass (used after background compilation).
    /// Recreates bind groups to match the new pipeline's layout.
    pub fn swap_pass_pipeline(
        &mut self,
        pass_index: usize,
        pipeline: ShaderPipeline,
        device: &Device,
        uniform_buffer: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
    ) -> Result<(), String> {
        if pass_index >= self.passes.len() {
            return Err(format!("Pass index {pass_index} out of range"));
        }
        // Install the new pipeline first so the rebuild reads its layout.
        self.passes[pass_index].pipeline = pipeline;
        self.rebuild_all_bind_groups(device, uniform_buffer, placeholder, audio);
        Ok(())
    }
}

/// Build the `[BindGroup; 2]` (one per global flip parity) for `views[i]`.
///
/// For parity `g`, the group binds: this pass's own previous frame (feedback →
/// the *other* target `targets[1-g]`; non-feedback → the 1x1 placeholder), the
/// three A17 audio textures + sampler, then each declared input pass `P` at
/// `P.targets[P.has_feedback ? g : 0]` — i.e. the target `P` writes this frame.
fn build_bind_groups(
    views: &[PassView],
    i: usize,
    device: &Device,
    uniform_buffer: &UniformBuffer,
    placeholder: &PlaceholderTexture,
    audio: &AudioTextures,
) -> [wgpu::BindGroup; 2] {
    let view = &views[i];
    let layout = view.layout;
    let waveform_view = &audio.waveform_view;
    let spectrum_view = &audio.spectrum_view;
    let spectrogram_view = &audio.spectrogram_view;
    let audio_sampler = &audio.sampler;

    let make = |g: usize| -> wgpu::BindGroup {
        // Own previous frame.
        let (prev_view, prev_sampler): (&TextureView, &Sampler) = if view.has_feedback {
            let other = &view.target.targets[1 - g];
            (&other.view, &other.sampler)
        } else {
            (&placeholder.view, &placeholder.sampler)
        };

        // Declared inputs → each source pass's current-frame target.
        let input_refs: Vec<(&TextureView, &Sampler)> = view
            .input_srcs
            .iter()
            .map(|&src| {
                let sp = &views[src];
                let ti = if sp.has_feedback { g } else { 0 };
                let rt = &sp.target.targets[ti];
                (&rt.view, &rt.sampler)
            })
            .collect();

        uniform_buffer.create_bind_group(
            device,
            layout,
            prev_view,
            prev_sampler,
            waveform_view,
            spectrum_view,
            spectrogram_view,
            audio_sampler,
            &input_refs,
        )
    };

    [make(0), make(1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::frame_capture::FrameCapture;
    use crate::gpu::fullscreen_quad::FULLSCREEN_TRIANGLE_VS;
    use crate::gpu::test_gpu::{gpu_guard, test_gpu};

    const FMT: TextureFormat = TextureFormat::Rgba8Unorm;

    // Pass A: a self-feedback accumulator that adds 0.25 to its previous value.
    const FRAG_ACCUM: &str = "@fragment\n\
        fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {\n\
            let prev = feedback(vec2f(0.5, 0.5)).r;\n\
            return vec4f(prev + 0.25, 0.0, 0.0, 1.0);\n\
        }";
    // Pass B: reads pass A's output (input0) and returns its complement.
    const FRAG_INVERT: &str = "@fragment\n\
        fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {\n\
            let a = input0(vec2f(0.5, 0.5)).r;\n\
            return vec4f(1.0 - a, 0.0, 0.0, 1.0);\n\
        }";

    /// A minimal blit pipeline: `textureLoad` the source at the fragment position
    /// and write it out. Lets a probe pull an executor target (which is only
    /// TEXTURE_BINDING) into a FrameCapture texture (which is COPY_SRC).
    fn blit_pipeline(device: &Device) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("probe-blit-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("probe-blit-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let src = format!(
            "{FULLSCREEN_TRIANGLE_VS}\n\
             @group(0) @binding(0) var src_tex: texture_2d<f32>;\n\
             @fragment fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {{\n\
                 return textureLoad(src_tex, vec2i(i32(pos.x), i32(pos.y)), 0);\n\
             }}"
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("probe-blit"),
            source: wgpu::ShaderSource::Wgsl(src.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("probe-blit-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FMT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        (pipeline, bgl)
    }

    /// Assemble a `PassExecutor` from pre-built pipelines without touching disk —
    /// the same two-phase bind-group build `PassExecutor::new` performs.
    fn assemble(
        device: &Device,
        queue: &Queue,
        w: u32,
        h: u32,
        ubuf: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
        specs: Vec<(&str, ShaderPipeline, bool, Vec<usize>)>,
    ) -> PassExecutor {
        let prepared: Vec<PreparedPass> = specs
            .into_iter()
            .map(|(name, pipeline, feedback, input_srcs)| {
                let target = if feedback {
                    PingPongTarget::new_cleared(device, queue, w, h, FMT, 1.0)
                } else {
                    PingPongTarget::new(device, w, h, FMT, 1.0)
                };
                PreparedPass {
                    name: name.to_string(),
                    pipeline,
                    target,
                    has_feedback: feedback,
                    input_srcs,
                }
            })
            .collect();
        let bind_groups: Vec<[wgpu::BindGroup; 2]> = {
            let views: Vec<PassView> = prepared
                .iter()
                .map(|p| PassView {
                    layout: &p.pipeline.bind_group_layout,
                    target: &p.target,
                    has_feedback: p.has_feedback,
                    input_srcs: &p.input_srcs,
                })
                .collect();
            (0..views.len())
                .map(|i| build_bind_groups(&views, i, device, ubuf, placeholder, audio))
                .collect()
        };
        let passes = prepared
            .into_iter()
            .zip(bind_groups)
            .map(|(p, bg)| CompiledPass {
                name: p.name,
                pipeline: p.pipeline,
                target: p.target,
                bind_groups: bg,
                has_feedback: p.has_feedback,
                input_srcs: p.input_srcs,
            })
            .collect();
        PassExecutor {
            passes,
            particle_system: None,
            flip_parity: 0,
        }
    }

    // An input naming a pass that was not declared earlier is a hard error, caught
    // before any shader is loaded (so `shader` never has to resolve on disk).
    #[test]
    #[ignore = "requires a wgpu adapter"]
    fn passgraph_rejects_unknown_input() {
        let _guard = gpu_guard();
        let (device, queue) = test_gpu();
        let loader = EffectLoader::for_test("");
        let ubuf = UniformBuffer::new(&device);
        let placeholder = PlaceholderTexture::new(&device, &queue, FMT);
        let audio = AudioTextures::new(&device, &queue);

        let passes = vec![PassDef {
            name: "b".into(),
            shader: "unused.wgsl".into(),
            scale: 1.0,
            inputs: vec!["missing".into()],
            feedback: false,
        }];
        let res = PassExecutor::new(
            &device,
            FMT,
            4,
            4,
            &passes,
            &loader,
            &ubuf,
            &placeholder,
            &audio,
            &queue,
            None,
        );
        let err = res.err().expect("unknown input must be rejected");
        assert!(
            err.contains("missing"),
            "error should name the bad input: {err}"
        );
    }

    // Two-pass probe of the multi-input pass graph (#1481):
    //   pass A  — self-feedback accumulator (prev + 0.25)
    //   pass B  — NON-feedback, reads A as input0, returns 1 - A
    // Frame 1 proves cross-pass sampling works at all (B sees A's 0.25 → 0.75).
    // Frame 2, after a flip, proves the GLOBAL flip parity: A now writes its other
    // ping-pong target (0.50), and B — though its own `current` never moved — must
    // still read A's fresh target (→ 0.50). The pre-fix code, indexing B's bind
    // group by B's own `current`, would read A's stale target and yield ~0.75.
    #[test]
    #[ignore = "requires a wgpu adapter; renders offscreen"]
    fn passgraph_cross_pass_and_parity() {
        let _guard = gpu_guard();
        let (device, queue) = test_gpu();
        let loader = EffectLoader::for_test("");
        let (w, h) = (4u32, 4u32);

        let ubuf = UniformBuffer::new(&device);
        let placeholder = PlaceholderTexture::new(&device, &queue, FMT);
        let audio = AudioTextures::new(&device, &queue);

        let pipe_a = ShaderPipeline::new(
            &device,
            FMT,
            &loader.prepend_library_with_inputs(FRAG_ACCUM, 0),
            None,
            0,
        )
        .expect("pass A pipeline");
        let pipe_b = ShaderPipeline::new(
            &device,
            FMT,
            &loader.prepend_library_with_inputs(FRAG_INVERT, 1),
            None,
            1,
        )
        .expect("pass B pipeline");

        let mut executor = assemble(
            &device,
            &queue,
            w,
            h,
            &ubuf,
            &placeholder,
            &audio,
            vec![("A", pipe_a, true, vec![]), ("B", pipe_b, false, vec![0])],
        );

        let mut uniforms = crate::gpu::ShaderUniforms::zeroed();
        uniforms.resolution = [w as f32, h as f32];

        let (blit, blit_bgl) = blit_pipeline(&device);

        let read_final_red = |executor: &PassExecutor| -> f32 {
            let mut fc = FrameCapture::new(&device, w, h, FMT, "probe-cap");
            let mut enc = device.create_command_encoder(&Default::default());
            {
                let final_rt = executor.execute(&mut enc, &ubuf, &queue, &uniforms);
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("probe-blit-bg"),
                    layout: &blit_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&final_rt.view),
                    }],
                });
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("probe-blit-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &fc.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&blit);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..3, 0..1);
            }
            fc.copy_to_staging(&mut enc);
            queue.submit([enc.finish()]);
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
            fc.request_map();
            let data = loop {
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    })
                    .unwrap();
                if let Some(d) = fc.take_mapped_data(&device) {
                    break d;
                }
            };
            data[0] as f32 / 255.0
        };

        let b1 = read_final_red(&executor);
        executor.flip();
        let b2 = read_final_red(&executor);

        // Frame 1: B saw A's first value (0.25) → 0.75. Cross-pass sampling works.
        assert!(
            (0.65..0.85).contains(&b1),
            "frame 1: B should read A=0.25 and output ~0.75, got {b1:.3}"
        );
        // Frame 2: global parity picked A's freshly-written target (0.50) → 0.50.
        // A stale read would land near 0.75, so the upper bound is the real guard.
        assert!(
            (0.40..0.60).contains(&b2),
            "frame 2: B should read A's flipped target (0.50) and output ~0.50, \
             got {b2:.3} (a value near 0.75 means the parity fix regressed)"
        );
    }
}

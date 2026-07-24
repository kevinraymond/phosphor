use wgpu::{CommandEncoder, Device, Queue, Sampler, TextureFormat, TextureView};

use crate::effect::EffectLoader;
use crate::effect::format::PassDef;

use super::ShaderPipeline;
use super::audio_textures::AudioTextures;
use super::particle::ParticleSystem;
use super::placeholder::PlaceholderTexture;
use super::render_target::{PingPongTarget, RenderTarget};
use super::uniforms::UniformBuffer;

/// One resolved pass-graph input: which pass supplies it, and whether we sample
/// that pass's *previous* frame (`prev`, from `PassDef.prev_inputs`) or its
/// *current* frame (`PassDef.inputs`). Current inputs come first in the WGSL
/// `input0..` numbering, then prev inputs (matching declaration order).
#[derive(Clone, Copy)]
struct InputSrc {
    pass: usize,
    prev: bool,
}

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
    /// Prior passes this pass samples as `input0..inputN-1` (current + prev frame).
    input_srcs: Vec<InputSrc>,
    /// Per-frame draw count. `>1` ping-pongs this pass's own target between draws
    /// (Jacobi/relaxation loops); requires `has_feedback`. `1` = single draw.
    iterations: u32,
}

/// Everything the bind-group builder needs about each pass, borrowed. Lets one
/// builder serve both construction (from freshly prepared passes) and rebuilds
/// (from the live `CompiledPass` list) without a per-pass mutable/immutable
/// aliasing conflict — see `rebuild_all_bind_groups`.
struct PassView<'a> {
    layout: &'a wgpu::BindGroupLayout,
    target: &'a PingPongTarget,
    has_feedback: bool,
    input_srcs: &'a [InputSrc],
}

/// A pass after pipeline + target creation but before its bind groups exist
/// (which need every pass's target to be resolvable). Construction two-phase.
struct PreparedPass {
    name: String,
    pipeline: ShaderPipeline,
    target: PingPongTarget,
    has_feedback: bool,
    input_srcs: Vec<InputSrc>,
    iterations: u32,
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
            // Resolve inputs into `input0..` order: current-frame inputs first,
            // then previous-frame inputs.
            let mut input_srcs = Vec::with_capacity(def.inputs.len() + def.prev_inputs.len());

            // `inputs`: current-frame output of an EARLIER pass. Forward/unknown
            // references are a hard error — that half of the graph is a DAG.
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
                input_srcs.push(InputSrc {
                    pass: src,
                    prev: false,
                });
            }

            // `prev_inputs`: previous-frame output of ANY feedback pass (later refs
            // allowed — previous-frame data has no intra-frame ordering constraint;
            // this is the edge that cuts a solver's velocity→div→pressure→velocity
            // cycle). A non-feedback pass has no distinct previous frame, so require
            // `feedback: true`.
            for name in &def.prev_inputs {
                let src = pass_defs
                    .iter()
                    .position(|p| &p.name == name)
                    .ok_or_else(|| {
                        format!("Pass '{}' prev_input '{name}' names no pass", def.name)
                    })?;
                if !pass_defs[src].feedback {
                    return Err(format!(
                        "Pass '{}' prev_input '{name}' must name a feedback pass",
                        def.name
                    ));
                }
                input_srcs.push(InputSrc {
                    pass: src,
                    prev: true,
                });
            }

            let input_count = input_srcs.len();
            let source = effect_loader
                .load_effect_source_with_inputs(&def.shader, input_count)
                .map_err(|e| format!("Failed to load shader '{}': {e}", def.shader))?;

            let pipeline =
                ShaderPipeline::new(device, hdr_format, &source, pipeline_cache, input_count)
                    .map_err(|e| format!("Failed to compile shader '{}': {e}", def.shader))?;

            // Clear feedback targets to prevent NaN/garbage from uninitialized GPU memory
            let target = if def.feedback {
                PingPongTarget::new_cleared(device, queue, width, height, hdr_format, def.scale)
            } else {
                PingPongTarget::new(device, width, height, hdr_format, def.scale)
            };

            // Iterations only ping-pong a feedback target; ignore on non-feedback passes.
            let iterations = if def.feedback {
                def.iterations.max(1)
            } else {
                1
            };

            prepared.push(PreparedPass {
                name: def.name.clone(),
                pipeline,
                target,
                has_feedback: def.feedback,
                input_srcs,
                iterations,
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
                iterations: p.iterations,
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
                iterations: 1,
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
            // Single-draw passes render into `write_target()` (= targets[flip_parity]
            // for a feedback pass) with the parity-indexed bind group. An iterated
            // (Jacobi) pass ping-pongs its own two targets in-encoder: draw `k` uses
            // bind_group[g] and writes targets[g] (bind_group[g] reads targets[1-g] via
            // feedback(), so consecutive draws chain), with `g` alternating so the FINAL
            // draw lands in targets[flip_parity] — what downstream readers (indexed by
            // flip_parity) and next frame's warm-start expect. Non-feedback inputs stay
            // fixed in targets[0] across the loop, so a stable divergence feeds every
            // pressure iteration.
            let n = pass.iterations.max(1);
            for k in 0..n {
                // (write index, bind-group index). Single draw: write our own
                // `current` target (flip_parity for feedback, 0 for non-feedback) and
                // read with the parity-indexed bind group — a non-feedback pass reading
                // a feedback input must pick the group pointing at that input's
                // current-frame target (#1481). Iterated (feedback only): both indices
                // are `g`, alternating so the FINAL draw lands in targets[flip_parity].
                let (write_idx, bind_idx) = if pass.has_feedback && n > 1 {
                    let g0 = self.flip_parity ^ ((n as usize - 1) & 1);
                    let g = g0 ^ (k as usize & 1);
                    (g, g)
                } else {
                    (pass.target.current, self.flip_parity)
                };
                let write_view = &pass.target.targets[write_idx].view;
                let bind_group = &pass.bind_groups[bind_idx];

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

        // Declared inputs → each source pass's target. Current-frame inputs read the
        // target the source writes THIS frame (targets[g] if feedback, else targets[0]);
        // prev-frame inputs read the source feedback pass's OTHER target, targets[1-g],
        // which still holds last frame's output when this pass executes (#1481).
        let input_refs: Vec<(&TextureView, &Sampler)> = view
            .input_srcs
            .iter()
            .map(|&src| {
                let sp = &views[src.pass];
                let ti = if src.prev {
                    1 - g
                } else if sp.has_feedback {
                    g
                } else {
                    0
                };
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
    // Passthrough: echo input0 (used to observe a prev-frame input directly).
    const FRAG_ECHO0: &str = "@fragment\n\
        fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {\n\
            return vec4f(input0(vec2f(0.5, 0.5)).r, 0.0, 0.0, 1.0);\n\
        }";
    // Self-feedback accumulator, +0.1 per invocation (for the iterations loop).
    const FRAG_STEP: &str = "@fragment\n\
        fn fs_main(@builtin(position) pos: vec4f) -> @location(0) vec4f {\n\
            let prev = feedback(vec2f(0.5, 0.5)).r;\n\
            return vec4f(prev + 0.1, 0.0, 0.0, 1.0);\n\
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
    /// One `assemble` pass spec: (name, pipeline, feedback, input srcs, iterations, scale).
    type PassSpec<'a> = (&'a str, ShaderPipeline, bool, Vec<InputSrc>, u32, f32);

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        device: &Device,
        queue: &Queue,
        w: u32,
        h: u32,
        fmt: TextureFormat,
        ubuf: &UniformBuffer,
        placeholder: &PlaceholderTexture,
        audio: &AudioTextures,
        specs: Vec<PassSpec>,
    ) -> PassExecutor {
        let prepared: Vec<PreparedPass> = specs
            .into_iter()
            .map(
                |(name, pipeline, feedback, input_srcs, iterations, scale)| {
                    let target = if feedback {
                        PingPongTarget::new_cleared(device, queue, w, h, fmt, scale)
                    } else {
                        PingPongTarget::new(device, w, h, fmt, scale)
                    };
                    PreparedPass {
                        name: name.to_string(),
                        pipeline,
                        target,
                        has_feedback: feedback,
                        input_srcs,
                        iterations,
                    }
                },
            )
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
                iterations: p.iterations,
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
            prev_inputs: vec![],
            iterations: 1,
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
            FMT,
            &ubuf,
            &placeholder,
            &audio,
            vec![
                ("A", pipe_a, true, vec![], 1, 1.0),
                (
                    "B",
                    pipe_b,
                    false,
                    vec![InputSrc {
                        pass: 0,
                        prev: false,
                    }],
                    1,
                    1.0,
                ),
            ],
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

    /// Execute the graph, then blit `executor.passes[pass_idx]`'s freshly-written
    /// target into a FrameCapture and read back the full RGBA8 bytes. Unlike the
    /// parity test's `read_final_red`, this inspects an arbitrary pass (not just the
    /// last), which the prev-input and Sumi probes need.
    #[allow(clippy::too_many_arguments)]
    fn capture_pass_rgba(
        device: &Device,
        queue: &Queue,
        ubuf: &UniformBuffer,
        blit: &wgpu::RenderPipeline,
        blit_bgl: &wgpu::BindGroupLayout,
        executor: &PassExecutor,
        uniforms: &crate::gpu::ShaderUniforms,
        pass_idx: usize,
        w: u32,
        h: u32,
    ) -> Vec<u8> {
        let mut fc = FrameCapture::new(device, w, h, FMT, "probe-cap");
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let _ = executor.execute(&mut enc, ubuf, queue, uniforms);
            let src = &executor.passes[pass_idx].target.write_target().view;
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("probe-blit-bg"),
                layout: blit_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
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
            pass.set_pipeline(blit);
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
        loop {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
            if let Some(d) = fc.take_mapped_data(device) {
                break d;
            }
        }
    }

    /// `capture_pass_rgba` reduced to a single pixel's red channel (0..1).
    #[allow(clippy::too_many_arguments)]
    fn render_pass_red(
        device: &Device,
        queue: &Queue,
        ubuf: &UniformBuffer,
        blit: &wgpu::RenderPipeline,
        blit_bgl: &wgpu::BindGroupLayout,
        executor: &PassExecutor,
        uniforms: &crate::gpu::ShaderUniforms,
        pass_idx: usize,
        w: u32,
        h: u32,
    ) -> f32 {
        let data = capture_pass_rgba(
            device, queue, ubuf, blit, blit_bgl, executor, uniforms, pass_idx, w, h,
        );
        data[0] as f32 / 255.0
    }

    // Previous-frame cross-pass input (#1481): pass 0 "reader" samples pass 1 "gen"'s
    // PREVIOUS frame via prev_inputs — a *forward* reference (gen is declared later),
    // legal only for prev inputs. gen is a +0.25 accumulator (0.25, 0.50, 0.75, …).
    // reader echoes gen's prior frame, so it must LAG by one: {0.00, 0.25, 0.50}. Had
    // the bind resolved to gen's current frame, reader would read {0.25, 0.50, 0.75}.
    #[test]
    #[ignore = "requires a wgpu adapter; renders offscreen"]
    fn passgraph_prev_input_reads_previous_frame() {
        let _guard = gpu_guard();
        let (device, queue) = test_gpu();
        let loader = EffectLoader::for_test("");
        let (w, h) = (4u32, 4u32);

        let ubuf = UniformBuffer::new(&device);
        let placeholder = PlaceholderTexture::new(&device, &queue, FMT);
        let audio = AudioTextures::new(&device, &queue);

        let pipe_reader = ShaderPipeline::new(
            &device,
            FMT,
            &loader.prepend_library_with_inputs(FRAG_ECHO0, 1),
            None,
            1,
        )
        .expect("reader pipeline");
        let pipe_gen = ShaderPipeline::new(
            &device,
            FMT,
            &loader.prepend_library_with_inputs(FRAG_ACCUM, 0),
            None,
            0,
        )
        .expect("gen pipeline");

        // Declaration order: reader (0) then gen (1). reader's only input is gen's
        // previous frame — a forward reference that only prev_inputs allow.
        let mut executor = assemble(
            &device,
            &queue,
            w,
            h,
            FMT,
            &ubuf,
            &placeholder,
            &audio,
            vec![
                (
                    "reader",
                    pipe_reader,
                    false,
                    vec![InputSrc {
                        pass: 1,
                        prev: true,
                    }],
                    1,
                    1.0,
                ),
                ("gen", pipe_gen, true, vec![], 1, 1.0),
            ],
        );

        let mut uniforms = crate::gpu::ShaderUniforms::zeroed();
        uniforms.resolution = [w as f32, h as f32];
        let (blit, blit_bgl) = blit_pipeline(&device);

        let read = |ex: &PassExecutor| {
            render_pass_red(
                &device, &queue, &ubuf, &blit, &blit_bgl, ex, &uniforms, 0, w, h,
            )
        };

        let f1 = read(&executor);
        executor.flip();
        let f2 = read(&executor);
        executor.flip();
        let f3 = read(&executor);

        assert!(
            f1 < 0.1,
            "frame 1: gen had no prior frame, reader ~0.0, got {f1:.3}"
        );
        assert!(
            (0.15..0.35).contains(&f2),
            "frame 2: reader should lag to gen's frame-1 value (0.25), got {f2:.3} \
             (~0.50 means it read the current frame, not the previous)"
        );
        assert!(
            (0.40..0.60).contains(&f3),
            "frame 3: reader should lag to gen's frame-2 value (0.50), got {f3:.3}"
        );
    }

    // Iterations (#1481): a single feedback pass with `iterations: 5` and a +0.1
    // self-accumulator must run five draws per frame, ping-ponging its own target, and
    // leave the fifth result in targets[flip_parity] (what the reader/warm-start read).
    // Frame 1 from cleared 0 → 0.5; frame 2 warm-starts from 0.5 → 1.0.
    #[test]
    #[ignore = "requires a wgpu adapter; renders offscreen"]
    fn passgraph_iterations_accumulate_within_frame() {
        let _guard = gpu_guard();
        let (device, queue) = test_gpu();
        let loader = EffectLoader::for_test("");
        let (w, h) = (4u32, 4u32);

        let ubuf = UniformBuffer::new(&device);
        let placeholder = PlaceholderTexture::new(&device, &queue, FMT);
        let audio = AudioTextures::new(&device, &queue);

        let pipe = ShaderPipeline::new(
            &device,
            FMT,
            &loader.prepend_library_with_inputs(FRAG_STEP, 0),
            None,
            0,
        )
        .expect("step pipeline");

        let mut executor = assemble(
            &device,
            &queue,
            w,
            h,
            FMT,
            &ubuf,
            &placeholder,
            &audio,
            vec![("acc", pipe, true, vec![], 5, 1.0)],
        );

        let mut uniforms = crate::gpu::ShaderUniforms::zeroed();
        uniforms.resolution = [w as f32, h as f32];
        let (blit, blit_bgl) = blit_pipeline(&device);
        let read = |ex: &PassExecutor| {
            render_pass_red(
                &device, &queue, &ubuf, &blit, &blit_bgl, ex, &uniforms, 0, w, h,
            )
        };

        let f1 = read(&executor);
        executor.flip();
        let f2 = read(&executor);

        // 5 × 0.1 from a cleared start. A single draw would give 0.1.
        assert!(
            (0.45..0.55).contains(&f1),
            "frame 1: 5 iterations of +0.1 should reach ~0.5, got {f1:.3} \
             (~0.1 means the loop ran once)"
        );
        // Warm-started from frame 1's 0.5, five more steps → ~1.0.
        assert!(
            (0.95..1.01).contains(&f2),
            "frame 2: warm-started 0.5 + 5×0.1 should reach ~1.0, got {f2:.3}"
        );
    }

    // End-to-end probe of the real Sumi stable-fluids graph (#1481) through the actual
    // PassExecutor: divergence(prev velocity) → pressure×24 → velocity(project+advect+
    // forces) → dye. Injects colored onset splats for the first ~20 frames, then coasts
    // with buoyancy for ~140 more. Captures the dye pass early (just after injection) and
    // late, and asserts the ink is present, not blown out, and actually MOVED — a dead
    // sim (static splat) would leave the two frames identical.
    // Run: SUMI_PNG_DIR=/tmp cargo test -p phosphor-app --release -- --ignored sumi_render_previews
    #[test]
    #[ignore = "requires a wgpu adapter; renders offscreen, writes PNGs"]
    fn sumi_render_previews() {
        let out_dir = std::env::var("SUMI_PNG_DIR").ok();
        let _guard = gpu_guard();
        let (device, queue) = test_gpu();

        // Real production preamble: uniform block + libs + injected input bindings.
        let noise = include_str!("../../../../assets/shaders/lib/noise.wgsl");
        let palette = include_str!("../../../../assets/shaders/lib/palette.wgsl");
        let sdf = include_str!("../../../../assets/shaders/lib/sdf.wgsl");
        let tonemap = include_str!("../../../../assets/shaders/lib/tonemap.wgsl");
        let loader = EffectLoader::for_test(&format!("{noise}\n{palette}\n{sdf}\n{tonemap}"));
        let fmt = TextureFormat::Rgba16Float;
        // 16:9 so the probe reproduces the real window's aspect (a square target hides
        // whether the injection ring fills a wide frame).
        let (w, h) = (480u32, 270u32);

        let ubuf = UniformBuffer::new(&device);
        let placeholder = PlaceholderTexture::new(&device, &queue, fmt);
        let audio = AudioTextures::new(&device, &queue);

        let mk = |shader: &str, count: usize| {
            ShaderPipeline::new(
                &device,
                fmt,
                &loader.prepend_library_with_inputs(shader, count),
                None,
                count,
            )
            .expect("sumi pass pipeline")
        };
        let pipe_div = mk(
            include_str!("../../../../assets/shaders/sumi_divergence.wgsl"),
            1,
        );
        let pipe_pres = mk(
            include_str!("../../../../assets/shaders/sumi_pressure.wgsl"),
            1,
        );
        let pipe_vel = mk(
            include_str!("../../../../assets/shaders/sumi_velocity.wgsl"),
            2,
        );
        let pipe_dye = mk(include_str!("../../../../assets/shaders/sumi_dye.wgsl"), 1);

        // Same wiring as sumi.pfx: passes 0..3 = divergence, pressure, velocity, dye.
        let src = |pass: usize, prev: bool| InputSrc { pass, prev };
        let mut executor = assemble(
            &device,
            &queue,
            w,
            h,
            fmt,
            &ubuf,
            &placeholder,
            &audio,
            vec![
                ("divergence", pipe_div, false, vec![src(2, true)], 1, 0.5),
                ("pressure", pipe_pres, true, vec![src(0, false)], 24, 0.5),
                (
                    "velocity",
                    pipe_vel,
                    true,
                    vec![src(1, false), src(3, true)],
                    1,
                    0.5,
                ),
                ("dye", pipe_dye, true, vec![src(2, false)], 1, 1.0),
            ],
        );
        let dye_idx = 3;

        let (blit, blit_bgl) = blit_pipeline(&device);

        let mut u = crate::gpu::ShaderUniforms::zeroed();
        u.resolution = [w as f32, h as f32];
        u.delta_time = 1.0 / 60.0;
        // Sumi param defaults (see sumi.pfx), indices 0..9.
        // Sumi param defaults (see sumi.pfx), indices 0..9.
        for (i, v) in [0.5, 0.5, 0.55, 0.45, 0.7, 0.5, 0.6, 0.5, 0.7, 0.55]
            .into_iter()
            .enumerate()
        {
            u.params[i] = v;
        }

        // Luminance stats over an RGBA8 frame: (mean 0..1, centroid uv, coverage) where
        // coverage is the fraction of pixels lit above a small threshold — the "fills the
        // screen" measure.
        let stats = |data: &[u8]| -> (f64, f64, f64, f64) {
            let (mut sum, mut sx, mut sy, mut lit) = (0.0f64, 0.0f64, 0.0f64, 0u32);
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    let l = 0.299 * data[i] as f64
                        + 0.587 * data[i + 1] as f64
                        + 0.114 * data[i + 2] as f64;
                    sum += l;
                    sx += l * x as f64;
                    sy += l * y as f64;
                    if l > 8.0 {
                        lit += 1;
                    }
                }
            }
            let mean = sum / (w * h) as f64 / 255.0;
            let coverage = lit as f64 / (w * h) as f64;
            if sum > 0.0 {
                (mean, sx / sum / w as f64, sy / sum / h as f64, coverage)
            } else {
                (mean, 0.5, 0.5, coverage)
            }
        };

        const FRAMES: u32 = 96;
        const EARLY: u32 = 24;
        const LATE: u32 = 92;
        let mut early: Vec<u8> = Vec::new();
        let mut late: Vec<u8> = Vec::new();

        for f in 0..FRAMES {
            u.time = f as f32 / 60.0;
            u.frame_index = f as f32;
            // Onsets fire periodically (as real music does), so the LATE frame measures
            // the steady-state fill, not a single coasting burst.
            u.onset = if f % 6 == 0 { 1.0 } else { 0.0 };
            u.beat = if f % 12 == 0 { 1.0 } else { 0.0 };
            u.bass = 0.7; // LOUD — buoyancy must stay a drift, not surge the bottom over the top
            u.flux = 0.6; // vorticity confinement
            u.dominant_chroma = 0.0; // C
            // Broad chroma so all twelve ring sites inject; every class stays lit.
            u.chroma = [0.6; 12];

            if f == EARLY || f == LATE {
                let data = capture_pass_rgba(
                    &device, &queue, &ubuf, &blit, &blit_bgl, &executor, &u, dye_idx, w, h,
                );
                if f == EARLY {
                    early = data;
                } else {
                    late = data;
                }
            } else {
                let mut enc = device.create_command_encoder(&Default::default());
                let _ = executor.execute(&mut enc, &ubuf, &queue, &u);
                queue.submit([enc.finish()]);
            }
            executor.flip();
        }

        let (em, _ex, _ey, ec) = stats(&early);
        let (lm, _lx, _ly, lc) = stats(&late);

        if let Some(dir) = out_dir {
            for (name, data) in [("early", &early), ("late", &late)] {
                let path = format!("{dir}/sumi_{name}.png");
                image::RgbaImage::from_raw(w, h, data.clone())
                    .expect("raw->image")
                    .save(&path)
                    .expect("save png");
                eprintln!("wrote {path}");
            }
        }
        // Fraction of near-white pixels — a saturated wash blows this up.
        let hot = late
            .chunks_exact(4)
            .filter(|p| p[0] > 220 && p[1] > 220 && p[2] > 220)
            .count() as f64
            / (w * h) as f64;
        eprintln!("early mean {em:.4} cover {ec:.3}; late mean {lm:.4} cover {lc:.3} hot {hot:.3}");

        // Ink present, and NOT a saturated wash (Kevin's blowout had mean ~0.6 and most of
        // the frame near-white with no fluid detail left).
        assert!(
            lm > 0.004,
            "late frame near-black (mean {lm:.4}) — ink died out"
        );
        assert!(lm < 0.5, "late frame blew out (mean {lm:.4})");
        assert!(
            hot < 0.15,
            "late frame is a saturated wash ({:.0}% near-white) — no fluid detail left",
            hot * 100.0
        );

        // Fills the frame: a good fraction of the 16:9 target is lit at steady state, not
        // a narrow central band. (The pre-fix aspect-squished ring covered ~10%.)
        assert!(
            lc > 0.25,
            "late frame covers only {:.0}% of the frame — ink is too localized",
            lc * 100.0
        );

        // Top/bottom balance under LOUD bass: the lower ring colours must not surge up and
        // dominate the upper half. Compare luminance in the top third vs the bottom third.
        let band_lum = |y0: u32, y1: u32| -> f64 {
            let mut s = 0.0f64;
            for y in y0..y1 {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    s += 0.299 * late[i] as f64
                        + 0.587 * late[i + 1] as f64
                        + 0.114 * late[i + 2] as f64;
                }
            }
            s / ((y1 - y0) * w) as f64
        };
        let top = band_lum(0, h / 3);
        let bottom = band_lum(2 * h / 3, h);
        let ratio = (bottom + 1.0) / (top + 1.0);
        eprintln!("top-third lum {top:.2}, bottom-third lum {bottom:.2}, ratio {ratio:.2}");
        assert!(
            ratio < 3.0,
            "bottom third is {ratio:.1}x the top under loud bass — buoyancy is surging the \
             lower colours over the upper ring"
        );

        // The fluid must be LIVE: early and late differ substantially. A static splat
        // (dead advection/projection) would leave them near-identical.
        let mut sad = 0.0f64;
        for i in (0..early.len()).step_by(4) {
            let el =
                0.299 * early[i] as f64 + 0.587 * early[i + 1] as f64 + 0.114 * early[i + 2] as f64;
            let ll =
                0.299 * late[i] as f64 + 0.587 * late[i + 1] as f64 + 0.114 * late[i + 2] as f64;
            sad += (el - ll).abs();
        }
        let sad = sad / (w * h) as f64 / 255.0;
        assert!(
            sad > 0.01,
            "early and late frames are nearly identical (SAD {sad:.4}) — the fluid isn't moving"
        );
    }
}

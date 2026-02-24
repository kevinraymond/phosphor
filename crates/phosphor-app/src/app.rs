use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use winit::window::Window;

use crate::audio::AudioSystem;
use crate::effect::EffectLoader;
use crate::gpu::{GpuContext, ShaderPipeline, ShaderUniforms, UniformBuffer};
use crate::params::ParamStore;
use crate::shader::ShaderWatcher;
use crate::ui::EguiOverlay;

pub struct App {
    pub gpu: GpuContext,
    pub pipeline: ShaderPipeline,
    pub uniform_buffer: UniformBuffer,
    pub bind_group: wgpu::BindGroup,
    pub uniforms: ShaderUniforms,
    pub start_time: Instant,
    pub last_frame: Instant,
    pub shader_watcher: ShaderWatcher,
    pub current_shader_source: String,
    pub shader_error: Option<String>,
    pub param_store: ParamStore,
    pub audio: AudioSystem,
    pub egui_overlay: EguiOverlay,
    pub effect_loader: EffectLoader,
    pub window: Arc<Window>,
}

impl App {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let gpu = GpuContext::new(window.clone())?;

        // Load default effect or fall back to default shader
        let mut effect_loader = EffectLoader::new();
        effect_loader.scan_effects_directory();

        // Prefer plasma_wave as default, fall back to first effect
        let default_idx = effect_loader
            .effects
            .iter()
            .position(|e| e.name == "Plasma Wave")
            .or_else(|| if effect_loader.effects.is_empty() { None } else { Some(0) });
        let (shader_source, param_store) = if let Some(idx) = default_idx {
            let effect = &effect_loader.effects[idx];
            match effect_loader.load_effect_source(&effect.shader) {
                Ok(source) => {
                    let mut store = ParamStore::new();
                    store.load_from_defs(&effect.inputs);
                    effect_loader.current_effect = Some(idx);
                    (source, store)
                }
                Err(e) => {
                    log::warn!("Failed to load effect: {e}, using default shader");
                    let source = std::fs::read_to_string("assets/shaders/default.wgsl")
                        .unwrap_or_else(|_| include_str!("../../../assets/shaders/default.wgsl").to_string());
                    (source, ParamStore::new())
                }
            }
        } else {
            let source = std::fs::read_to_string("assets/shaders/default.wgsl")
                .unwrap_or_else(|_| include_str!("../../../assets/shaders/default.wgsl").to_string());
            (source, ParamStore::new())
        };

        let pipeline = ShaderPipeline::new(&gpu.device, gpu.format, &shader_source)?;
        let uniform_buffer = UniformBuffer::new(&gpu.device);
        let bind_group =
            uniform_buffer.create_bind_group(&gpu.device, &pipeline.bind_group_layout);

        let shader_watcher = ShaderWatcher::new()?;

        let audio = AudioSystem::new();

        let egui_overlay = EguiOverlay::new(&gpu.device, gpu.format, &window);

        let now = Instant::now();
        Ok(Self {
            gpu,
            pipeline,
            uniform_buffer,
            bind_group,
            uniforms: ShaderUniforms::zeroed(),
            start_time: now,
            last_frame: now,
            shader_watcher,
            current_shader_source: shader_source,
            shader_error: None,
            param_store,
            audio,
            egui_overlay,
            effect_loader,
            window,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        self.egui_overlay
            .resize(width, height, self.window.scale_factor() as f32);
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Update time uniforms
        self.uniforms.time = now.duration_since(self.start_time).as_secs_f32();
        self.uniforms.delta_time = dt;
        self.uniforms.resolution = [
            self.gpu.surface_config.width as f32,
            self.gpu.surface_config.height as f32,
        ];

        // Drain audio features
        if let Some(features) = self.audio.latest_features() {
            self.uniforms.bass = features.bass;
            self.uniforms.mid = features.mid;
            self.uniforms.treble = features.treble;
            self.uniforms.rms = features.rms;
            self.uniforms.phase = features.phase;
            self.uniforms.onset = features.onset;
            self.uniforms.centroid = features.centroid;
            self.uniforms.flux = features.flux;
            self.uniforms.flatness = features.flatness;
            self.uniforms.rolloff = features.rolloff;
            self.uniforms.bandwidth = features.bandwidth;
            self.uniforms.zcr = features.zcr;
        }

        // Pack params
        self.uniforms.params = self.param_store.pack_to_buffer();

        // Check for shader changes — only reload if a relevant file changed
        let changes = self.shader_watcher.drain_changes();
        if !changes.is_empty() {
            if let Some(idx) = self.effect_loader.current_effect {
                let effect = &self.effect_loader.effects[idx];
                let is_relevant = changes.iter().any(|p| {
                    p.ends_with(&effect.shader)
                        || p.to_string_lossy().contains("/lib/")
                });
                if is_relevant {
                    match self.effect_loader.load_effect_source(&effect.shader) {
                        Ok(source) if source != self.current_shader_source => {
                            log::info!(
                                "Shader file changed: {}",
                                changes.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
                            );
                            self.try_recompile_shader(&source);
                        }
                        Ok(_) => {} // content unchanged, skip
                        Err(e) => {
                            log::error!("Failed to reload shader: {e}");
                            self.shader_error = Some(format!("Read error: {e}"));
                        }
                    }
                }
            }
        }
    }

    pub fn try_recompile_shader(&mut self, source: &str) {
        match self.pipeline.recreate_pipeline(
            &self.gpu.device,
            self.gpu.format,
            source,
        ) {
            Ok(()) => {
                self.current_shader_source = source.to_string();
                self.shader_error = None;
                // Recreate bind group for new pipeline
                self.bind_group = self.uniform_buffer.create_bind_group(
                    &self.gpu.device,
                    &self.pipeline.bind_group_layout,
                );
                log::info!("Shader recompiled successfully");
            }
            Err(e) => {
                log::error!("Shader compilation failed: {e}");
                self.shader_error = Some(e);
            }
        }
    }

    pub fn load_effect(&mut self, index: usize) {
        if let Some(effect) = self.effect_loader.effects.get(index).cloned() {
            match self.effect_loader.load_effect_source(&effect.shader) {
                Ok(source) => {
                    self.param_store.load_from_defs(&effect.inputs);
                    self.try_recompile_shader(&source);

                    self.effect_loader.current_effect = Some(index);
                    // Drain stale watcher events to prevent spurious recompiles
                    self.shader_watcher.drain_changes();
                    log::info!("Loaded effect: {}", effect.name);
                }
                Err(e) => {
                    log::error!("Failed to load effect '{}': {e}", effect.name);
                    self.shader_error = Some(format!("Load error: {e}"));
                }
            }
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.gpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.uniform_buffer.update(&self.gpu.queue, &self.uniforms);

        let mut encoder =
            self.gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("phosphor-encoder"),
                });

        // Effect render pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("phosphor-effect-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

            pass.set_pipeline(&self.pipeline.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // egui render pass
        self.egui_overlay
            .render(&self.gpu.device, &self.gpu.queue, &mut encoder, &view);

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl ShaderUniforms {
    pub fn zeroed() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

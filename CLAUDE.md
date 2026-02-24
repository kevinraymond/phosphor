# Phosphor

Cross-platform particle and shader engine for live VJ performance. Built with raw winit + wgpu + egui (not Bevy).

## Project Status

**Phase 1 (Core Rendering MVP): COMPLETE** — committed as `fa187b5`

### What's Built
- winit 0.30 window with wgpu 27 Vulkan rendering (fullscreen triangle technique)
- Shader hot-reload (notify file watcher, 100ms debounce, error recovery keeps old pipeline)
- Parameter system: ParamDef (Float/Color/Bool/Point2D), ParamStore, uniform packing
- Audio pipeline: cpal capture → lock-free ring buffer → dedicated thread → 2048-pt FFT (rustfft) → 12 spectral features → asymmetric EMA smoothing → crossbeam channel to main thread
- egui overlay (D key toggle): WCAG 2.2 AA dark/light themes, audio spectrum bars, auto-generated param controls, effect browser, status bar
- .pfx JSON effect format with WGSL shader library (noise, palette, sdf, tonemap) auto-prepended
- 3 demo effects: plasma_wave, singularity (SDF raymarched), membrane (ocean surface)

### Known Issues (from Phase 1)
- Effect switching via UI is slightly glitchy (works but may flicker)
- 26 compiler warnings (mostly unused items reserved for future phases)
- `compiler.rs` has an unused `compile_shader` function (naga validation was removed since wgpu validates internally; could be used for pre-validation with better error messages if naga is added as a direct dependency)
- Fonts directory (`assets/fonts/`) is empty — Inter and JetBrains Mono not yet bundled
- Reduced motion detection (`ui/accessibility/motion.rs`) is stubbed for macOS/Windows

### Architecture
```
Main Thread: winit event loop → drain audio/shader channels → update uniforms → render effect → render egui → present
Audio Thread: cpal callback → ring buffer → FFT → smooth → send AudioFeatures
File Watcher Thread: notify → debounce → send changed paths
```

No mutexes in hot path. Three threads + cpal callback.

### Key Design Decisions
- WGSL uniform arrays must be `array<vec4f, N>` not `array<f32, N>` (16-byte alignment requirement). Params accessed via `param(i)` helper function in shaders.
- `target` is a WGSL reserved word — use `look_at` instead in shaders.
- wgpu 27 (not 28) because Rust 1.90 doesn't support wgpu 28's MSRV of 1.92.
- egui 0.33: `CornerRadius` not `Rounding`, `corner_radius` not `rounding` field, `Renderer::new` takes `RendererOptions` struct, `RenderPass` needs `.forget_lifetime()` for egui's `'static` requirement.
- cpal 0.17: `SampleRate` is `u32` (not tuple struct), `description()` returns `Result<DeviceDescription>`, field access via `.name()` method.

### Controls
- `D` — Toggle egui overlay
- `F` — Toggle fullscreen
- `Esc` — Quit
- `Tab` — Cycle widgets (when overlay visible)
- Sliders have +/- buttons for WCAG 2.5.7 compliance

### Build & Run
```bash
cargo run                          # debug build
cargo run --release                # release build (much faster shaders)
RUST_LOG=phosphor_app=debug cargo run  # verbose logging
```

### Reference Projects (for porting)
- `~/ai/audio/spectral-senses/` — C++ audio analysis (12 features, EMA smoothing)
- `~/ai/audio/spectral-senses-old/` — GLSL shader library (SDF, noise, palette, tonemap) + scene shaders
- `~/ai/audio/easey-glyph/` — Python adaptive normalization, beat detection

### Full Plan
The complete 28-week, 4-phase plan is at `~/ai/audio/phosphor-internal/cross-platform particle and shader engine.md`. Phase 1 is done. Phases 2-4 cover: particle system, multi-pass rendering, performance profiling, preset management, plugin architecture.

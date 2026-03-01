# Changelog

<!-- Release workflow extracts notes between ## vX.Y.Z headers via awk. -->
<!-- Keep the "## vX.Y.Z — date" format for automatic release notes. -->

## Unreleased

### Chaos Effect Overhaul
- **Visible Lorenz butterfly**: fixed base rho from 22 (below chaos threshold) to 28 (canonical chaotic regime); particles now trace the iconic butterfly attractor pattern
- **XZ butterfly projection**: replaced Y-axis rotation with XZ plane view (the classic Lorenz butterfly orientation) plus gentle ±8° tilt oscillation for 3D depth feel
- **Stable attractor dynamics**: reduced audio modulation of sigma/rho/beta to subtle range (was dramatically reshaping the attractor every frame); fixed `time * audio-varying` rotation bug that caused violent camera jumps
- **Trail-based visualization**: dim particles (alpha × 0.2) with high trail persistence (decay 0.92) accumulate into visible trajectory lines; clean feedback without twist distortion preserves tight attractor lines
- **Density increase**: 20K particles at 500/s emit (was 6K at 80/s) for dense attractor coverage
- **Removed dead params**: attractor_mix, zoom, speed inputs were not wired to compute shader (compute shaders cannot access `param()`)
- **Brightness fixes**: removed × 0.18 fallback color dim, raised initial alpha 0.3→1.0, unclamped color from 0.5→1.0 max

### Particle Effect Review & Enhancement
- **Murmuration** (hero effect): full Boids flocking model with separation, cohesion, and alignment; angular heading smoothing eliminates jitter; alpha blend for opaque bird silhouettes; twilight sky gradient background; depth-based sizing; 15K particles; audio: beat→cohesion spike, bass→disorder, onset→flock split
- **Coral** (fix): replaced numerical central-difference gradient with analytical derivative of Turing cosine waves; added soft exponential boundary repulsion (no more hard clamp collapse); FBM curl noise for organic diffusion; reduced attraction_strength 2.5→1.2; aspect ratio correction; color gradient and size curve
- **Chaos** (fix): adaptive sub-stepping (4 steps) prevents divergence; NaN/divergence detection auto-kills bad particles; proper Rossler scaling (native scale then rescale); centered perspective projection around attractor center; color gradient for temperature-based trajectory coloring; size/opacity curves
- **Helix** (fix): increased E-field upward from 0.05→0.20; reduced B_z from 3.0→1.5 base (wider spirals that actually rise); beat adds upward levitation pulse; moved emitter from y=-0.6 to y=-0.4; RMS-reactive upward drift; two-strand DNA color gradient; size/opacity curves
- **Nova** (overhaul): color gradient lifecycle (white-hot→vivid→orange→dim red); size/opacity lifetime curves; ground bounce for trailing sparks (restitution 0.3); speed/size/life variance for natural variation; 15→85% shell/spark ratio; staggered burst emission; stronger ground glow with beat pulse
- **Phosphor** (polish): reduced sparkle overbright (mix 0.55→0.3, mult 1.6→1.2); tightened feedback decay 0.88→0.84; stronger beat inward pull 0.02→0.06; size/opacity curves for crisp particle falloff; reduced bloom intensity 1.2→1.1
- **Veil** (bloom fix): raised background cap 0.2→0.6 for richer fabric; lowered bloom_threshold 0.9→0.7
- **Ribbons** (bloom fix): raised bloom_threshold 0.4→0.60; reduced bloom_intensity 0.6→0.40; reduced trail_width 0.008→0.005; capped feedback at 0.8
- **Cymatics** (polish): analytical Chladni gradient (no numerical eps); smooth bilinear mode crossfade between integer modes; raised background cap 0.5→0.85; sharper nodal line glow (exp factor 12→18); soft boundary repulsion; color gradient and size curve
- **Vortex**: chromatic aberration in gravitational lensing (R/G/B channel separation); accretion glow flash near event horizon on beat; temperature-based color gradient for disk coloring
- **Swarm**: wider orbit radius 0.3→0.5; reduced radial damping 8.0→4.5; stronger beat orbit expansion
- **Flux**: sparkle system (8% of particles get 2.5x brightness with flicker); opacity curve for gentle fade-out; raised background cap 0.5→0.8

### Advanced Particle Forces
- ParticleUniforms extended from 192 to 384 bytes with new force, emitter, curve, and sort fields
- FBM noise forces: configurable octaves, lacunarity, persistence with turbulence and curl noise modes
- Wind force: constant directional force via `wind: [x, y]`
- Vortex field: rotational force with center, strength, and falloff radius
- Ground bounce: configurable y-level and restitution coefficient
- New emitter shapes: disc (uniform area fill), cone (directional spread with angle control)
- Emitter velocity inheritance: particles inherit emitter motion for trailing effects
- Speed, lifetime, and size variance: per-particle randomization (0-1 range)
- Lifetime curves: 8-point LUT for size and opacity over particle lifetime
- Color gradient: up to 8 hex colors interpolated over lifetime
- Particle spin: per-particle rotation with configurable speed, rendered via vertex rotation
- `apply_builtin_forces()` helper in `particle_lib.wgsl`: single call applies all forces (gravity, wind, drag, noise, attraction, vortex, flow field)
- Curve/gradient evaluation helpers: `eval_size_curve()`, `eval_opacity_curve()`, `eval_color_gradient()`
- Bitonic depth sort: optional GPU merge-sort on alive indices for correct alpha-blended rendering
- `parse_hex_color()` utility for `#RRGGBB` / `#RRGGBBAA` to packed u32
- All new fields default to zero/disabled — existing .pfx files and custom shaders work unchanged

### PFX Hot-Reload + Editor Integration
- `.pfx` file hot-reload: editing effect metadata (params, postprocess, passes, particles) live-updates without restarting
- Differential updates: param-only or postprocess-only changes skip GPU pipeline rebuilds
- `merge_from_defs` on ParamStore preserves slider positions when param definitions change
- Editor tab switching: Shader and Effect tabs in code editor for `.wgsl` and `.pfx` side by side
- JSON syntax highlighting for `.pfx` editing
- "Edit Shader" → "Edit Effect", "Copy Shader" → "Copy Effect" button labels
- 12 new unit tests for PfxDiff and merge_from_defs

## v1.2.0 — 2026-02-28

### GPU Particle System
- GPU-driven pipeline: counter buffer, indirect draw, alive/dead index lists — zero dead particle processing
- `particle_lib.wgsl` auto-prepended to all compute shaders: shared structs, bindings, hash/rand, emit/alive helpers
- Alive/dead protocol: atomic emission claims, compact alive index rendering
- 3D curl noise flow field: 64x64x64 baked texture at `@group(1)`, `sample_flow_field()` helper
- Trail rendering: per-particle ring buffer, ribbon triangle strips with tapering width/alpha, separate indirect draw
- Spatial hash grid: 40x40 GPU 3-pass pipeline (count → prefix sum → scatter), neighbor query at `@group(3)`
- ParticleUniforms extended from 128 to 192 bytes (flow field + trail params)
- ParticleRenderUniforms extended from 32 to 48 bytes (frame_index + trail params)
- Particle UI panel: alive/max count with utilization bar, emit rate, burst, lifetime, speed, size, drag sliders, feature badges

### New Effects
- **Flux**: 25K particles following curl noise flow field, audio-reactive flow strength and speed
- **Ribbons**: 8K particles with flow field + 16-point trail ribbons, audio-reactive width/opacity
- **Chaos**: 40K particles tracing Lorenz/Rossler strange attractors with RK4 integration, 3D perspective projection, audio-reactive bifurcation parameters
- **Helix**: 15K charged particles spiraling via Lorentz force F=q(E+v×B), positive/negative charges, audio-reactive B_z and E fields
- **Murmuration**: 60K flocking particles with Vicsek model, spatial hash neighbor query, audio-reactive order↔disorder phase transition
- **Cymatics**: 30K particles forming Chladni nodal line patterns via gradient descent, audio frequency bands select mode numbers
- **Coral**: 40K particles tracing Turing-like organic growth patterns, hexagonal spots morphing to labyrinthine stripes

### Bug Fixes
- Fix Murmuration crash: create spatial hash before compute pipeline so shader bindings validate at pipeline creation
- Fix particle size exponential blowout in all 6 new effects: size calculation read back previous frame's computed size (`p.vel_size.w`), compounding scale factors >1.0 each frame causing particles to grow until they fill the screen. Fix stores initial size in `pos_life.z` and uses that as base instead of the accumulated value
- Tune bloom thresholds (0.70–0.85), reduce particle counts (4K–10K), lower feedback decay, add hard caps to bg shaders
- Fix all 6 new effects washing out to uniform brightness: reduce particle counts 3-8x (into 4K-10K working range), raise bloom thresholds to 0.70-0.85, lower bloom intensity to 0.30-0.35, reduce HDR clamp from 1.5 to 1.0 in bg shaders, lower alpha multiplier from 2.0 to 1.5, restore per-particle brightness/alpha to visible levels, reduce feedback decay for faster trail fade
- Fix Chaos visibility: increase projection zoom 50% so attractor shape fills screen instead of concentrating in a small region
- Fix Helix vertical bands: widen emission spread and add oscillating horizontal E-field to create interweaving spirals
- Tune all new effects: lower trail_decay defaults to 0.78-0.80, reduce particle counts and emit rates

## v1.1.0 — 2026-02-28

### Scene System
- Scene = ordered cue list referencing presets with transitions (Cut, Dissolve, ParamMorph)
- SceneStore: save/load/delete scenes to `~/.config/phosphor/scenes/*.json`
- Timeline state machine: Idle → Holding → Transitioning with auto-advance and beat sync
- ParamMorph transitions: smooth interpolation of all params and layer opacities
- Dissolve transitions: GPU crossfade via fullscreen shader with snapshot capture
- Scene panel in left sidebar: cue list management, transport controls, save/load/delete
- Per-cue transition type editing: click to cycle Cut → Dissolve → Morph
- Per-cue transition duration editing via DragValue
- Advance mode selector: Manual / Timer / Beat Sync with per-cue hold times and beats-per-cue
- Auto-save: cue and timeline changes persist to disk immediately
- Timeline bar above status bar: cue blocks, playhead during hold and transitions, click-to-jump
- Scene status indicator (SCN) in status bar with cue counter
- Keyboard: Space (next cue), T (toggle timeline)
- MIDI triggers: SceneGoNext, SceneGoPrev, ToggleTimeline
- OSC: `/phosphor/trigger/scene_go_next`, `scene_go_prev`, `toggle_timeline`
- Web control: Prev Cue / Next Cue / Timeline trigger buttons
- MIDI Clock sync: parse 0xF8/FA/FB/FC system realtime, derive external BPM, beat-synced advance
- MIDI Clock → Timeline: auto-follow transport (play/stop), MIDI clock beats drive BeatSync mode with audio fallback
- OSC scene control: `/phosphor/scene/goto_cue`, `/scene/load` (int or string), `/scene/loop_mode`, `/scene/advance_mode`
- OSC outbound timeline state: `/phosphor/state/timeline/active`, `cue_index`, `cue_count`, `transition_progress` at TX rate

### UI
- Redesign scene panel: card-framed scene list, section headers (TRANSPORT / CUE LIST), sized transport buttons (PLAY ghost-border, STOP|PREV|GO), transition type badges with color per type, ghost-border "+ Cue" button
- Widen side panels from 270px to 315px

## v1.0.1 — 2026-02-27

### UI
- Shader compilation errors in status bar now have a dismiss (×) button

### Documentation
- Add comprehensive TUTORIALS.md covering all features
- Quick Start now leads with binary downloads, build-from-source in collapsible details

## v1.0.0 — 2026-02-27

### Rendering
- HDR multi-pass pipeline with Rgba16Float render targets and ping-pong feedback
- Post-processing chain: bloom extract, separable Gaussian blur, chromatic aberration, ACES tonemapping, vignette, film grain
- Audio-reactive post-processing (RMS → bloom, onset → chromatic aberration, flatness → film grain)
- Shader hot-reload via file watcher with 100ms debounce and error recovery

### Effects
- 12 curated audio-reactive effects: Aurora, Drift, Tunnel, Prism, Shards, Pulse, Iris, Swarm, Storm, Veil, Nova, Vortex
- .pfx JSON effect format with multi-pass pipeline support
- WGSL shader library auto-prepended (noise, palette, SDF, tonemap)
- In-app WGSL shader editor with live hot-reload, built-in/user sections, copy-shader

### GPU Particles
- Compute shader particle simulation with ping-pong storage buffers and atomic emission
- Vertex-pulling instanced rendering with additive and alpha-blend pipelines
- Sprite atlas textures, image decomposition (grid/threshold/random sampling)
- Per-particle aux buffer for home positions and packed RGBA
- Configurable emitters: point, ring, line, screen

### Audio
- Multi-resolution FFT (4096/1024/512-pt) with 20 audio features across 7 frequency bands
- Adaptive normalization with per-feature running min/max
- 3-stage beat detection: onset detector → FFT autocorrelation tempo estimator → predictive beat scheduler
- Kalman filter BPM tracking with octave disambiguation
- Audio input device selector with runtime switching and persistence
- PulseAudio capture on Linux with dlopen (no libpulse build dependency), cpal/ALSA fallback
- `--audio-test` CLI flag for standalone audio diagnostics (no GPU required)

### Layer Composition
- Up to 8 compositing layers, each with independent effect/params/particles
- 10 blend modes: Normal, Add, Screen, ColorDodge, Multiply, Overlay, HardLight, Difference, Exclusion, Subtract
- Per-layer opacity, enable/disable, lock (prevent changes), pin (prevent reorder)
- Drag-and-drop layer reordering
- GPU compositor with single-layer fast path (zero overhead for one layer)

### MIDI + OSC
- MIDI input via midir: MIDI learn, auto-connect, hot-plug detection, CC mapping for params and triggers
- OSC input/output via rosc: RX on port 9000, TX on port 9001, OSC learn, layer-targeted messages
- Config persistence for both MIDI and OSC bindings

### Web Control Surface
- Same-port HTTP + WebSocket server with embedded single-file touch UI
- Bidirectional JSON state sync with full snapshot on connect
- Multi-client support, audio broadcast at 10Hz, mobile-first dark theme
- Auto-reconnect with exponential backoff

### Media Layers
- PNG/JPEG/GIF/WebP as compositing layers with aspect-ratio-correct letterbox
- Animated GIF/WebP playback with forward/reverse/ping-pong, speed control, loop toggle
- Video playback (feature-gated `video`): MP4/MOV/AVI/MKV/WebM via ffmpeg pre-decode, seek slider, 60s max

### Webcam Input
- Live camera feed as compositing layer (feature-gated `webcam`) via nokhwa
- Cross-platform: v4l2, AVFoundation, MediaFoundation
- Capture thread with frame drain, device controls, mirror, preset save/load with device reconnection
- Corrupted frame recovery, dead thread detection, panic hardening

### NDI Output
- Runtime-loaded NDI SDK via libloading (feature-gated `ndi`), no build-time dependency
- GPU capture with double-buffered staging and 1-frame latency readback
- Per-effect alpha preserved end-to-end through compositor and post-processing
- Luma-to-alpha toggle, configurable source name and resolution
- Sender thread with bounded channel (drops frames to maintain VJ performance)

### Preset System
- Save/load multi-layer compositions with effect params, blend modes, opacity, lock/pin state
- Async preset loading for media/video layers with background decoding thread
- Generation-based cancellation for rapid MIDI preset cycling
- MIDI triggers for next/prev preset navigation
- Bundled "Crucible" preset showcasing all 8 layers

### UI & Accessibility
- egui overlay (D key toggle) with WCAG 2.2 AA dark/light themes plus Midnight, Ember, Neon VJ themes
- Auto-generated parameter controls, 7-band audio spectrum, BPM display with beat flash
- Status bar with keyboard hints, particle count, audio health, NDI/MIDI/OSC/Web status dots
- macOS app icon and polished DMG installer

### Testing
- ~251 unit tests across 17 modules covering audio, params, effects, layers, presets, MIDI, OSC, web, NDI, themes

## v0.2.0

Initial development release.

---
NDI® is a registered trademark of Vizrt NDI AB.

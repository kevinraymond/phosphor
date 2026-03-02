# Changelog

<!-- Release workflow extracts notes between ## vX.Y.Z headers via awk. -->
<!-- Keep the "## vX.Y.Z — date" format for automatic release notes. -->

## Unreleased

### Particle Video/Webcam Source + Transitions
- **Video as particle source**: Raster (and any image-emitter effect) can now use video files as particle source — particles update home positions per-frame tracking video content
- **Webcam as particle source**: live webcam feed drives particle positions in real-time (feature-gated `webcam`)
- **Source transitions**: smooth 0.5s interpolation of particle home positions when switching between image/video/webcam sources (no hard snap)
- **Per-frame aux updates**: pre-allocated aux buffer at `max_particles` size enables `write_buffer` updates without recreating GPU bind groups (8MB DMA transfer for 500K particles)
- **`sample_rgba_buffer()`**: new function samples raw RGBA byte data directly, skipping file I/O — used for video frame and webcam frame particle sampling
- **UI source controls**: particle panel shows source type badge (IMG/VIDEO/CAM), source name, Load Video / Webcam buttons (feature-gated), video transport (play/pause, speed, loop, seek)
- **Preset save/load**: particle video path, speed, looping, and webcam source persisted in `LayerPreset` with serde defaults for backward compat
- **Load Image button**: users can load custom PNG/JPEG/WebP images as particle source (replaces default image)
- **Animated GIF as particle source**: GIFs with multiple frames auto-detect as animated source with per-frame particle position updates (same as video, no feature gate required)
- **Background loading**: all particle source loading (image, GIF, video) happens on a background thread via `ParticleSourceLoader` — no UI freeze during decode
- **EmitterDef `video` field**: `.pfx` files can specify `"video": "clip.mp4"` or `"video": "webcam"` for built-in video/webcam source at effect load time
- Video decode reuses existing `media/video.rs` infrastructure (ffmpeg pre-decode to RAM, 60s max)
- **Built-in image selector**: combo box in particle panel for quick-switching between bundled raster images (skull, phoenix, jellyfish, hand, samurai_mask) without file dialog
- **Morph-safe preset loading**: when a preset layer has the same effect already loaded, skip the full effect reload — keeps particle systems alive so morph transitions interpolate params smoothly instead of destroying and rebuilding all particles
- 12 new tests: `sample_rgba_buffer` (5), `ParticleImageSource` (2), `SourceTransition` (3), `LayerPreset` serde (3), `EmitterDef` serde with video (2)

### Raster Effect (New)
- **Raster** (video wall): 500K particles map to image pixel positions, colored by source image
- Voronoi shard fragmentation: 32 seed points create irregular fragments with per-shard rotation, translation, and depth variation
- Frayed edges: particles near Voronoi cell boundaries break free with individual jitter
- Three bass displacement modes via `bass_mode` param: shards (default), tangential swirl, radial push
- 2D sinusoidal wave displacement from mids, onset-driven per-particle scatter from highs
- Spring-return physics pull particles back to displaced home positions (sustained audio holds displacement)
- Beat onset impulse with shard-coherent burst direction
- Luminance-based particle sizing: brighter pixels render larger for visual depth
- 8 exposed params: trail_decay, spring_k, bass_strength, mid_wave, high_scatter, burst_force, depth_scale, bass_mode
- Optional feedback trails via trail_decay for motion blur during scatter
- Image source `MAX_DIM` raised from 512 to 2048 for higher-resolution sampling

### Particle Effects Overhaul
- **Delete 4 effects**: removed Coral, Helix, Nova, Vortex (low quality, never worked well)
- **Murmuration → Murmur**: renamed, removed feedback trails (root cause of "sperm" look), brighter sky for contrast, larger/darker/more opaque particles (0.014 size, 0.9 alpha), reduced count 70K→40K
- **Ribbons fix**: lower feedback clamp 0.12→0.06, stronger vignette, halved particle brightness and alpha to prevent additive washout
- **Veil fix**: lower feedback cap 0.4→0.15, stronger decay dampening, halved particle brightness and alpha, minimum dampening floor even without audio
- **Cymatics enhancement**: 25K→50K particles, emit rate 1700→3500, burst 100→500, 3 new params (rotation, symmetry, glow), more vivid color gradient, stronger nodal line contrast
- **Swarm → Spirograph**: complete reimagine — hypotrochoid parametric curves with 5 arms of different petal-count patterns, drifting centers, audio-reactive ratio morphing. 6 params: trail_decay, draw_speed, pattern_scale, complexity, drift, color_spread
- **Spirograph** (new): multi-pattern hypotrochoid curves with 5 arms of distinct petal-count patterns, drifting centers, audio-reactive ratio morphing. Replaces Swarm.
- **Compute shader `param()` access**: particle compute shaders can now access effect params 0-7 via `param(i)` helper, forwarded from ParamStore through `effect_params` fields in ParticleUniforms (repurposed padding bytes, no size change)
- Updated Crucible and Spectral Eye presets (Swarm→Spirograph)
- Effect count: 15 curated effects (was 12 before particle additions, peaked at 19, now curated down to 15)

### Particle System Hardening for 1M Support
- **GPU-side buffer zero-init**: replaced CPU `vec![0u8; 128MB]` + `write_buffer` with `encoder.clear_buffer()` — eliminates 128MB+ CPU allocation at 1M particles
- **Device limits validation**: `max_count` clamped to device `max_storage_buffer_binding_size` with log warning
- **Bitonic sort auto-cap**: depth sort auto-disabled above 65K particles (would require 210 dispatches/frame at 1M)
- **Trail buffer safety**: trails disabled above 500K particles; trail length capped to fit device binding limit

### Parameter Persistence
- **Slider values persist to .pfx**: adjusting parameters in the Parameters panel now updates the `default` values in the .pfx file for user effects, so values survive effect reload and app restart
- Debounced 500ms save — writes only after slider stops moving, not on every frame
- Editor's Effect tab stays in sync when params are saved from UI

### Shader Editor
- **Fix: Save now persists both tabs**: Save (Ctrl+S) writes both the active and paired file when either has unsaved changes — previously only saved the currently active tab, losing edits to the other file

### Effect Browser
- **Particle badge**: small accent-colored dot in top-right corner of effect buttons that use particles
- **Particle count in hover**: hover text shows particle count (e.g. "70K particles", "1M particles")

### Veil Effect
- Fix feedback blowout on loud audio: particle brightness now decreases with volume, stronger loudness dampening on alpha, audio-reactive decay, lowered feedback clamp

### Helix Effect Overhaul
- **Dipole field-guided particles**: replaced uniform B_z Lorentz force with magnetic dipole field — particles follow curved field lines from emitter, creating visible force field pattern through trails
- **Helical oscillation**: perpendicular oscillation around field lines (charge-dependent CW/CCW), bass tightens helix frequency
- **Feedback warps along field**: particle trails flow along magnetic field direction with chromatic aberration, creating persistent field line visualization
- **Dark background**: replaced overpowering painted field lines with near-black background + very subtle dipole hints; particles are the main visual
- **Bright particles**: brightness 0.22→0.70, alpha 0.30→0.55, particle size 0.010→0.018; emit rate 100→200 for dense field coverage
- **Beat burst**: speed pulse along field lines on beat detection; onset scatters particles laterally off field lines
- **Tuned post-processing**: bloom threshold 0.50→0.35, bloom intensity 0.45→0.60 for force-field glow; brighter saturated color gradient

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

# Credits & Acknowledgments

Fosfora stands on a lot of other people's work — open-source crates, published research, and
reference implementations that were read closely while building it. This page lists them.

## Rendering & GPU

- [wgpu](https://github.com/gfx-rs/wgpu) — WebGPU implementation (Vulkan/Metal/DX12)
- [egui](https://github.com/emilk/egui) — Immediate-mode GUI
- [naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga) — WGSL shader validation
- [glam](https://github.com/bitshifter/glam-rs) — Linear algebra

## Audio

- [cpal](https://github.com/RustAudio/cpal) — Cross-platform audio I/O
- [rustfft](https://github.com/ejmahler/RustFFT) — FFT for spectral analysis and beat detection
- [midir](https://github.com/Boddlnagg/midir) — Cross-platform MIDI I/O

## Networking & Control

- [rosc](https://github.com/klingtnet/rosc) — Open Sound Control protocol
- [tungstenite](https://github.com/snapview/tungstenite-rs) — WebSocket server
- [NDI](https://ndi.video) — Network Device Interface (runtime-loaded)
- [MediaPipe](https://github.com/google-ai-edge/mediapipe) (Google) — Hand, pose and face
  tracking behind the Python control bridges

## Depth Estimation

- [MiDaS](https://github.com/isl-org/MiDaS) (Intel ISL) — Monocular depth estimation model
- [ONNX Runtime](https://onnxruntime.ai) via [ort](https://github.com/pykeio/ort) — ML inference

## Gaussian Splatting (Splat effect)

- [SuperSplat](https://github.com/playcanvas/supersplat) (PlayCanvas) — Reference 3DGS
  viewer/editor; the Splat effect's sorted renderer was matched against it side-by-side, and its
  [PlayCanvas engine](https://github.com/playcanvas/engine) gsplat implementation (renormalized
  Gaussian falloff, EWA covariance projection, front-to-back alpha compositing) guided the render math
- [3D Gaussian Splatting for Real-Time Radiance Field Rendering](https://repo-sam.inria.fr/fungraph/3d-gaussian-splatting/)
  (Kerbl, Kopanas, Leimkühler, Drettakis — INRIA) — The technique itself, incl. the
  anti-aliasing covariance dilation

## Algorithms & Techniques

**Flocking and particle life**

- [Reynolds Boids](https://www.red3d.com/cwr/boids/) (Craig Reynolds) — Flocking behavior baseline for Murmur
- Vicsek model — Noise-driven order-chaos phase transitions in Murmur
- Topological interaction (K=7 nearest neighbors) — Scale-free correlations in Murmur
- [Particle Lenia](https://google-research.github.io/self-organising-systems/particle-lenia/)
  (Mordvintsev et al., Google Research, 2023) — Continuous cellular automata behind Genesis
- Particle Life / asymmetric force matrices — Multi-species emergence in Symbiosis

**Fields, noise and volume**

- [Inigo Quilez](https://iquilezles.org/) — Smooth Worley noise (log-sum-exp) in Storm
- Beer-Lambert law — Volumetric light absorption in Storm, the Volumetric render mode, and Lattice
- Curl noise — Divergence-free particle advection in Flux
- Gray-Scott reaction-diffusion — Chemical field sculpting in Turing
- Chladni plate figures — Standing-wave nodal lines in Cymatics
- Strange attractors (Lorenz, Rössler, Halvorsen, Thomas, Chen) with RK4 integration — Chaos

**Cellular automata**

- [Softology](https://softologyblog.wordpress.com/) (Jason Rampe) — the *Generations* 3D CA rule
  notation (S/B/states/neighborhood) and rule catalogue behind the eight Lattice presets
- Jeff Jones — the *Physarum polycephalum* sense-rotate agent model that Polycephalum runs
  twelve times over, one species per pitch class

**Audio analysis**

SuperFlux onsets, YIN pitch, constant-Q chroma, Krumhansl-Kessler key profiles, Fitzgerald
median-filter HPSS, Foote novelty, EBU R128 / ITU-R BS.1770 loudness, MFCC and spectral contrast.
Full per-feature citations in [AUDIO-FEATURES.md](AUDIO-FEATURES.md#further-reading).

Beat detection pipeline ported from [EASEy-GLYPH](https://github.com/kevinraymond/easey-glyph).

## Fonts (SIL Open Font License 1.1)

- [Inter](https://github.com/rsms/inter) — Rasmus Andersson
- [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) — JetBrains

## License

Fosfora itself is dual-licensed under [MIT](../LICENSE-MIT) or [Apache 2.0](../LICENSE-APACHE),
at your option.

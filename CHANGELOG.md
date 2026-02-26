# Changelog

## v0.3.0 — 2026-02-26

### New Features
- In-app WGSL shader editor with live hot-reload
- Audio input device selector with runtime switching and persistence
- Built-in/user sections in effects panel with delete and copy-shader
- NDI luma-to-alpha toggle for downstream compositing
- Per-effect shader alpha for NDI compositing transparency

### Improvements
- macOS app icon and polished DMG installer with drag-to-Applications
- Shader editor UI refinements (transparent background, vector icons, minimize/expand)
- NDI feature enabled in release builds
- Hide default Phosphor effect from UI, reduce particle count

### Testing
- 236 new unit tests across 27+ modules (coverage 11% → 13%)

### Fixes
- Clippy approx_constant warnings
- Auto-release CI when Cargo.toml version changes

## v0.2.0

Initial public release with multi-layer composition, GPU particles, audio-reactive
shaders, MIDI/OSC input, web control surface, preset system, media layers, video
playback, and NDI output.

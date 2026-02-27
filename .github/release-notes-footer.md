## Downloads

| Platform | File | Notes |
|----------|------|-------|
| **macOS Universal** | `.dmg` | Apple Silicon + Intel (recommended) |
| macOS arm64 | `.dmg` | Apple Silicon only |
| macOS x86_64 | `.dmg` | Intel only |
| Linux x86_64 | `.tar.gz` | glibc 2.35+ (Ubuntu 22.04+, Debian 12+) |
| Windows x64 | `.zip` | Windows 10+ |

## Getting started

**macOS**: Open the DMG, drag Phosphor.app to Applications (or run directly). Signed and notarized.

**Linux / Windows**: Extract, run `phosphor` from the extracted directory. The `assets/` folder must be next to the binary.

## Requirements

- **GPU**: Vulkan (Linux/Windows) or Metal (macOS)
- **Audio**: Built-in mic or line-in for audio-reactive visuals
- **NDI®** (optional): [NDI® SDK runtime](https://ndi.video) for network video output
- **Video playback** (optional): `ffmpeg` on PATH for media layers

---
NDI® is a registered trademark of Vizrt NDI AB.

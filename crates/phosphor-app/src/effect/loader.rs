use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;

use super::format::PfxEffect;

/// Resolve the assets directory once (CWD-relative → exe-relative → macOS bundle).
pub fn assets_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        // 1. CWD-relative (dev workflow)
        let cwd = PathBuf::from("assets");
        if cwd.join("effects").is_dir() {
            log::info!("Assets: CWD-relative ({})", cwd.display());
            return cwd;
        }

        // 2. Exe-relative (installed binary)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let beside = exe_dir.join("assets");
                if beside.join("effects").is_dir() {
                    log::info!("Assets: exe-relative ({})", beside.display());
                    return beside;
                }

                // 3. macOS .app bundle: exe is in Foo.app/Contents/MacOS/
                let bundle = exe_dir.join("../Resources/assets");
                if bundle.join("effects").is_dir() {
                    let canonical = bundle.canonicalize().unwrap_or(bundle);
                    log::info!("Assets: macOS bundle ({})", canonical.display());
                    return canonical;
                }
            }
        }

        // Fallback — will surface as "Effects directory not found" later
        log::warn!("Assets directory not found; using CWD-relative fallback");
        cwd
    })
}

const LIB_FILENAMES: &[&str] = &[
    "shaders/lib/noise.wgsl",
    "shaders/lib/palette.wgsl",
    "shaders/lib/sdf.wgsl",
    "shaders/lib/tonemap.wgsl",
];

/// Standard uniform block prepended to all effect shaders.
const UNIFORM_BLOCK: &str = r#"
struct PhosphorUniforms {
    time: f32,
    delta_time: f32,
    resolution: vec2f,

    sub_bass: f32,
    bass: f32,
    low_mid: f32,
    mid: f32,
    upper_mid: f32,
    presence: f32,
    brilliance: f32,
    rms: f32,

    kick: f32,
    centroid: f32,
    flux: f32,
    flatness: f32,
    rolloff: f32,
    bandwidth: f32,
    zcr: f32,
    onset: f32,
    beat: f32,
    beat_phase: f32,
    bpm: f32,
    beat_strength: f32,

    params: array<vec4f, 4>,
    feedback_decay: f32,
    frame_index: f32,
}

@group(0) @binding(0) var<uniform> u: PhosphorUniforms;
@group(0) @binding(1) var prev_frame: texture_2d<f32>;
@group(0) @binding(2) var prev_sampler: sampler;

fn param(i: u32) -> f32 {
    return u.params[i / 4u][i % 4u];
}

fn feedback(uv: vec2f) -> vec4f {
    return textureSample(prev_frame, prev_sampler, uv);
}
"#;

pub struct EffectLoader {
    pub effects: Vec<PfxEffect>,
    pub current_effect: Option<usize>,
    lib_source: String,
}

impl EffectLoader {
    pub fn new() -> Self {
        let base = assets_dir();
        // Load library sources
        let mut lib_source = String::new();
        for filename in LIB_FILENAMES {
            let path = base.join(filename);
            match std::fs::read_to_string(&path) {
                Ok(src) => {
                    lib_source.push_str(&src);
                    lib_source.push('\n');
                }
                Err(e) => {
                    log::warn!("Failed to load shader library {}: {e}", path.display());
                }
            }
        }

        Self {
            effects: Vec::new(),
            current_effect: None,
            lib_source,
        }
    }

    /// Reload shader library sources from disk (called when lib/*.wgsl changes).
    pub fn reload_library(&mut self) {
        let base = assets_dir();
        self.lib_source.clear();
        for filename in LIB_FILENAMES {
            let path = base.join(filename);
            match std::fs::read_to_string(&path) {
                Ok(src) => {
                    self.lib_source.push_str(&src);
                    self.lib_source.push('\n');
                }
                Err(e) => {
                    log::warn!("Failed to reload shader library {}: {e}", path.display());
                }
            }
        }
        log::info!("Reloaded shader library sources");
    }

    pub fn scan_effects_directory(&mut self) {
        self.effects.clear();
        let dir = assets_dir().join("effects");
        if !dir.exists() {
            log::warn!("Effects directory not found: {}", dir.display());
            return;
        }

        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "pfx")
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            match std::fs::read_to_string(entry.path()) {
                Ok(json) => match serde_json::from_str::<PfxEffect>(&json) {
                    Ok(effect) => {
                        log::info!("Found effect: {} ({})", effect.name, entry.path().display());
                        self.effects.push(effect);
                    }
                    Err(e) => {
                        log::warn!("Failed to parse {}: {e}", entry.path().display());
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read {}: {e}", entry.path().display());
                }
            }
        }

        log::info!("Found {} effects", self.effects.len());
    }

    pub fn resolve_shader_path(&self, shader_rel: &str) -> PathBuf {
        assets_dir().join("shaders").join(shader_rel)
    }

    pub fn load_effect_source(&self, shader_rel: &str) -> Result<String> {
        let path = self.resolve_shader_path(shader_rel);
        let source = std::fs::read_to_string(&path)?;
        Ok(self.prepend_library(&source))
    }

    /// Load a compute shader source. Prepends the noise library but NOT the fragment
    /// uniform block (compute shaders have their own uniform struct).
    pub fn load_compute_source(&self, shader_rel: &str) -> Result<String> {
        let path = self.resolve_shader_path(shader_rel);
        let source = std::fs::read_to_string(&path)?;
        // Compute shaders get the noise library but not the fragment uniform block
        Ok(format!("{}\n{}", self.lib_source, source))
    }

    /// Prepend the uniform block and library functions to a shader source.
    pub fn prepend_library(&self, source: &str) -> String {
        // Check if the source already has the uniform block
        if source.contains("PhosphorUniforms") {
            // Already has uniforms, just prepend library
            format!("{}\n{}", self.lib_source, source)
        } else {
            format!("{}\n{}\n{}", UNIFORM_BLOCK, self.lib_source, source)
        }
    }
}

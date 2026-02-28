// Phosphor particle library — shared structs, bindings, and helpers.
// Auto-prepended to all particle compute shaders (same pattern as noise/palette/sdf libs).

struct ParticleUniforms {
    delta_time: f32,
    time: f32,
    max_particles: u32,
    emit_count: u32,

    emitter_pos: vec2f,
    emitter_radius: f32,
    emitter_shape: u32,

    lifetime: f32,
    initial_speed: f32,
    initial_size: f32,
    size_end: f32,

    gravity: vec2f,
    drag: f32,
    turbulence: f32,

    attraction_point: vec2f,
    attraction_strength: f32,
    seed: f32,

    sub_bass: f32,
    bass: f32,
    mid: f32,
    rms: f32,
    kick: f32,
    onset: f32,
    centroid: f32,
    flux: f32,
    beat: f32,
    beat_phase: f32,

    resolution: vec2f,

    // Flow field params
    flow_strength: f32,
    flow_scale: f32,
    flow_speed: f32,
    flow_enabled: f32,

    // Trail params (reserved for Phase 1B)
    trail_length: u32,
    trail_width: f32,
    _reserved_trail: vec2f,

    // Padding to 192 bytes
    _pad192a: vec4f,
    _pad192b: vec4f,
}

struct Particle {
    pos_life: vec4f,  // xy=position, z=reserved, w=life (1=alive, 0=dead)
    vel_size: vec4f,  // xy=velocity, z=reserved, w=size
    color: vec4f,     // rgba
    flags: vec4f,     // x=age, y=lifetime, z=effect-specific, w=effect-specific
}

struct ParticleAux {
    home: vec4f,  // xy=home position, z=packed RGBA (bitcast u32->f32), w=sprite_index
}

// --- Bindings (group 0) ---

@group(0) @binding(0) var<uniform> u: ParticleUniforms;
@group(0) @binding(1) var<storage, read> particles_in: array<Particle>;
@group(0) @binding(2) var<storage, read_write> particles_out: array<Particle>;
// counters: [0]=alive_count, [1]=dead_count, [2]=emit_used, [3]=reserved
@group(0) @binding(3) var<storage, read_write> counters: array<atomic<u32>, 4>;
@group(0) @binding(4) var<storage, read> aux: array<ParticleAux>;
@group(0) @binding(5) var<storage, read> dead_indices: array<u32>;
@group(0) @binding(6) var<storage, read_write> alive_indices_out: array<u32>;

// --- Flow field bindings (group 1) ---

@group(1) @binding(0) var flow_field_tex: texture_3d<f32>;
@group(1) @binding(1) var flow_field_sampler: sampler;

// Sample the 3D curl noise flow field at a position.
// pos: particle position in clip space [-1,1]
// Returns velocity offset in clip space.
fn sample_flow_field(pos: vec2f) -> vec2f {
    if u.flow_enabled < 0.5 {
        return vec2f(0.0);
    }
    // Map clip space [-1,1] to UV [0,1] for texture sampling
    let uv = (pos * 0.5 + 0.5) * u.flow_scale;
    // Scroll z-axis over time for animation
    let w = fract(u.time * u.flow_speed * 0.1);
    let sample = textureSampleLevel(flow_field_tex, flow_field_sampler, vec3f(uv, w), 0.0);
    // xyz = curl velocity, scale by strength
    return sample.xy * u.flow_strength;
}

// --- Trail buffer (group 2, optional) ---

@group(2) @binding(0) var<storage, read_write> trail_buffer: array<vec4f>;

// Write current position to the trail ring buffer.
// Call this after position integration for alive particles.
// trail_point: vec4f(pos.x, pos.y, size, alpha)
fn trail_write(idx: u32, trail_point: vec4f) {
    if u.trail_length < 2u {
        return;
    }
    // Ring buffer: write to slot = frame_index % trail_length
    // frame_index is passed via reserved field in flags
    // We use a simple approach: write to (global frame counter % trail_length)
    // The frame counter is embedded in the seed's integer part
    let frame = u32(u.time * 60.0); // ~60fps frame counter
    let slot = frame % u.trail_length;
    let base = idx * u.trail_length + slot;
    trail_buffer[base] = trail_point;
}

// --- Spatial hash neighbor query (group 3, optional) ---

@group(3) @binding(0) var<storage, read> sh_cell_offsets: array<u32>;
@group(3) @binding(1) var<storage, read> sh_cell_counts: array<u32>;
@group(3) @binding(2) var<storage, read> sh_sorted_indices: array<u32>;

const SH_GRID_W: u32 = 40u;
const SH_GRID_H: u32 = 40u;

fn sh_pos_to_cell(pos: vec2f) -> vec2i {
    let gx = i32(clamp((pos.x * 0.5 + 0.5) * f32(SH_GRID_W), 0.0, f32(SH_GRID_W - 1u)));
    let gy = i32(clamp((pos.y * 0.5 + 0.5) * f32(SH_GRID_H), 0.0, f32(SH_GRID_H - 1u)));
    return vec2i(gx, gy);
}

fn sh_cell_index(gx: i32, gy: i32) -> u32 {
    return u32(gy) * SH_GRID_W + u32(gx);
}

// Get the start index and count for a grid cell.
// Returns vec2u(offset, count). If cell is out of bounds, returns (0, 0).
fn sh_cell_range(gx: i32, gy: i32) -> vec2u {
    if gx < 0 || gx >= i32(SH_GRID_W) || gy < 0 || gy >= i32(SH_GRID_H) {
        return vec2u(0u, 0u);
    }
    let cell = sh_cell_index(gx, gy);
    return vec2u(sh_cell_offsets[cell], sh_cell_counts[cell]);
}

// --- Hash / random utilities ---

fn hash(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453123);
}

fn hash2(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn rand_vec2(seed: f32) -> vec2f {
    return vec2f(hash(seed), hash(seed + 1.0)) * 2.0 - 1.0;
}

// --- Aspect ratio helpers ---

fn aspect() -> f32 {
    return u.resolution.x / u.resolution.y;
}

fn to_screen(p: vec2f) -> vec2f {
    return vec2f(p.x * aspect(), p.y);
}

fn to_clip(v: vec2f) -> vec2f {
    return vec2f(v.x / aspect(), v.y);
}

// --- Alive/dead list management ---

// Claim an emission slot. Returns the slot index.
// Compare against u.emit_count to check if emission is allowed.
fn emit_claim() -> u32 {
    return atomicAdd(&counters[2], 1u);
}

// Mark a particle index as alive (append to alive output list).
fn mark_alive(idx: u32) {
    let pos = atomicAdd(&counters[0], 1u);
    alive_indices_out[pos] = idx;
}

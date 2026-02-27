// Image-to-particle decomposition compute shader.
// Particles scatter on beat and reform to image positions via spring force.
// Auxiliary buffer provides home positions and packed RGBA colors.

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
}

struct Particle {
    pos_life: vec4f,
    vel_size: vec4f,
    color: vec4f,
    flags: vec4f,
}

struct ParticleAux {
    home: vec4f,  // xy = home position, z = packed RGBA, w = sprite_index
}

@group(0) @binding(0) var<uniform> u: ParticleUniforms;
@group(0) @binding(1) var<storage, read> particles_in: array<Particle>;
@group(0) @binding(2) var<storage, read_write> particles_out: array<Particle>;
@group(0) @binding(3) var<storage, read_write> emit_counter: atomic<u32>;
@group(0) @binding(4) var<storage, read> aux: array<ParticleAux>;

fn hash(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453123);
}

fn unpack_rgba(packed: f32) -> vec4f {
    let bits = bitcast<u32>(packed);
    return vec4f(
        f32(bits & 0xFFu) / 255.0,
        f32((bits >> 8u) & 0xFFu) / 255.0,
        f32((bits >> 16u) & 0xFFu) / 255.0,
        f32((bits >> 24u) & 0xFFu) / 255.0,
    );
}

// Hardcoded spring-damper constants — independent of effect's ParticleDef.
const SPRING_K: f32 = 12.0;       // Spring stiffness — strong pull home
const DAMPING: f32 = 0.85;        // Per-frame velocity retention at 60fps
const SCATTER_SCALE: f32 = 0.12;  // Beat scatter impulse
const MAX_VEL: f32 = 1.0;         // Velocity cap

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles {
        return;
    }

    let home = aux[idx].home;
    let home_pos = home.xy;
    let home_color = unpack_rgba(home.z);

    // Skip transparent particles (padding beyond sampled image pixels)
    if home_color.a < 0.01 {
        // Park invisible particles offscreen, dead
        var p = particles_in[idx];
        p.pos_life = vec4f(99.0, 99.0, 0.0, 0.0);
        p.color = vec4f(0.0);
        particles_out[idx] = p;
        return;
    }

    var p = particles_in[idx];

    // Initial emit: particles start at home position
    if p.pos_life.w <= 0.0 {
        let slot = atomicAdd(&emit_counter, 1u);
        if slot < u.emit_count {
            let seed_base = u.seed + f32(idx) * 7.31;
            p.pos_life = vec4f(home_pos + vec2f(hash(seed_base), hash(seed_base + 1.0)) * 0.01, 0.0, 1.0);
            p.vel_size = vec4f(0.0, 0.0, 0.0, u.initial_size);
            p.color = home_color;
            p.flags = vec4f(hash(seed_base + 2.0) * u.lifetime * 0.5, u.lifetime, 0.0, 0.0);
        }
        particles_out[idx] = p;
        return;
    }

    var pos = p.pos_life.xy;
    var vel = p.vel_size.xy;
    let dt = u.delta_time;

    // Spring force toward home position
    let to_home = home_pos - pos;
    vel += to_home * SPRING_K * dt;

    // Damping
    vel *= pow(DAMPING, dt * 60.0);

    // Beat scatter: gentle impulse away from home
    if u.beat > 0.5 {
        let seed_base = f32(idx) * 3.17 + u.time;
        let random_dir = vec2f(hash(seed_base) - 0.5, hash(seed_base + 7.0) - 0.5);
        let scatter_dir = normalize(pos - home_pos + random_dir * 0.5 + vec2f(0.001));
        vel += scatter_dir * SCATTER_SCALE * (1.0 + u.kick * 0.5);
    }

    // Velocity cap
    let speed = length(vel);
    if speed > MAX_VEL {
        vel = vel * (MAX_VEL / speed);
    }

    // Integrate
    pos += vel * dt;

    // Preserve original image color (no audio color shift)
    let color = home_color;

    // Size: slight bass pulse
    let size = u.initial_size * (1.0 + u.bass * 0.2);

    p.pos_life = vec4f(pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = color;
    p.flags.x += dt;

    // Wrap-around instead of death (image particles are persistent)
    if p.flags.x >= p.flags.y {
        p.flags.x = 0.0;
    }

    particles_out[idx] = p;
}

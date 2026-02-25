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

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles {
        return;
    }

    let home = aux[idx].home;
    let home_pos = home.xy;
    let home_color = unpack_rgba(home.z);

    var p = particles_in[idx];

    // Initial emit: particles start at home position
    if p.pos_life.w <= 0.0 {
        let slot = atomicAdd(&emit_counter, 1u);
        if slot < u.emit_count {
            let seed_base = u.seed + f32(idx) * 7.31;
            // Start at home with slight random offset + staggered age
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

    // Reform force: spring toward home position
    // Stronger when audio is quiet, weaker when loud
    let reform_strength = u.attraction_strength * (1.0 - u.rms * 0.8);
    let to_home = home_pos - pos;
    let dist = length(to_home);
    if dist > 0.001 {
        // Spring force + damping
        vel += to_home * reform_strength * dt * 60.0;
        vel *= pow(0.92, dt * 60.0); // damping
    }

    // Beat scatter: impulse away from home
    if u.beat > 0.5 {
        let scatter_dir = normalize(pos - home_pos + vec2f(0.001));
        let scatter_power = u.initial_speed * (1.0 + u.kick * 2.0);
        vel += scatter_dir * scatter_power;
    }

    // Audio-reactive jitter
    if u.turbulence > 0.0 {
        let seed_base = f32(idx) * 3.17 + u.time * 2.0;
        let jitter = vec2f(hash(seed_base) - 0.5, hash(seed_base + 17.0) - 0.5);
        vel += jitter * u.turbulence * u.rms * dt;
    }

    // Integrate
    pos += vel * dt;

    // Color: blend between home color and audio-reactive shift
    var color = home_color;
    let audio_hue = u.centroid * 0.5 + u.beat_phase * 0.3;
    let shift = vec3f(
        sin(audio_hue * 6.28) * 0.15,
        sin((audio_hue + 0.33) * 6.28) * 0.15,
        sin((audio_hue + 0.67) * 6.28) * 0.15,
    );
    color = vec4f(clamp(color.rgb + shift * u.rms, vec3f(0.0), vec3f(1.0)), color.a);

    // Size pulsing with bass
    let size = u.initial_size * (1.0 + u.bass * 0.5);

    p.pos_life = vec4f(pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = color;
    p.flags.x += dt; // age

    // Wrap-around instead of death for image particles (they're persistent)
    if p.flags.x >= p.flags.y {
        p.flags.x = 0.0;
    }

    particles_out[idx] = p;
}

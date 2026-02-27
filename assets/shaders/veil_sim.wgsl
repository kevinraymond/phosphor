// Veil particle simulation — flowing silk curtain.
// Particles have home positions; a multi-layer displacement field moves the homes,
// spring forces keep particles tracking their displaced homes for coherent sheet motion.

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
    flags: vec4f, // x=age, y=lifetime, z=home_x, w=home_y
}

@group(0) @binding(0) var<uniform> u: ParticleUniforms;
@group(0) @binding(1) var<storage, read> particles_in: array<Particle>;
@group(0) @binding(2) var<storage, read_write> particles_out: array<Particle>;
@group(0) @binding(3) var<storage, read_write> emit_counter: atomic<u32>;

fn hash(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453123);
}

fn aspect() -> f32 {
    return u.resolution.x / u.resolution.y;
}

fn to_screen(p: vec2f) -> vec2f {
    return vec2f(p.x * aspect(), p.y);
}

fn to_clip(v: vec2f) -> vec2f {
    return vec2f(v.x / aspect(), v.y);
}

// Multi-layer displacement field for fabric-like coherent motion
fn displacement_field(home: vec2f, t: f32) -> vec2f {
    // Layer 1: bass-driven slow undulation (large scale billow)
    let billow_freq = 1.5;
    let billow = vec2f(
        phosphor_noise2(home * billow_freq + vec2f(t * 0.2, 0.0)) - 0.5,
        phosphor_noise2(home * billow_freq + vec2f(0.0, t * 0.15)) - 0.5
    ) * u.bass * 0.3;

    // Layer 2: mid-driven ripple (medium scale)
    let ripple_freq = 4.0;
    let ripple = vec2f(
        phosphor_noise2(home * ripple_freq + vec2f(t * 0.5, 3.7)) - 0.5,
        phosphor_noise2(home * ripple_freq + vec2f(7.1, t * 0.4)) - 0.5
    ) * u.mid * 0.15;

    // Layer 3: noise flutter (small scale, always present)
    let flutter_freq = 8.0;
    let flutter = vec2f(
        phosphor_noise2(home * flutter_freq + vec2f(t * 0.8, 11.3)) - 0.5,
        phosphor_noise2(home * flutter_freq + vec2f(13.7, t * 0.7)) - 0.5
    ) * 0.04;

    return billow + ripple + flutter;
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Screen emitter: random position across the full screen
    let home_x = hash(seed_base) * 2.0 - 1.0;
    let home_y = hash(seed_base + 1.0) * 2.0 - 1.0;
    let home = vec2f(home_x, home_y);

    // Start at home position
    let pos = home;

    // Small random initial velocity
    let angle = hash(seed_base + 2.0) * 6.2831853;
    let vel = vec2f(cos(angle), sin(angle)) * u.initial_speed * 0.1;

    // Color: cool-toned flowing gradient based on position
    // Span full hue range, biased toward cyans/blues/purples for silk feel
    let hue = fract((home_y * 0.5 + 0.5) * 0.6 + 0.5 + u.centroid * 0.3);
    let r_c = abs(hue * 6.0 - 3.0) - 1.0;
    let g_c = 2.0 - abs(hue * 6.0 - 2.0);
    let b_c = 2.0 - abs(hue * 6.0 - 4.0);
    let brightness = 0.04 + u.rms * 0.02;

    // Stagger initial age
    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.5;

    p.pos_life = vec4f(pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, u.initial_size * (0.8 + hash(seed_base + 3.0) * 0.4));
    p.color = vec4f(clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0)) * brightness, 0.08);
    // Store home position in flags.z/w
    p.flags = vec4f(initial_age, u.lifetime, home_x, home_y);
    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles {
        return;
    }

    var p = particles_in[idx];
    let life = p.pos_life.w;
    let age = p.flags.x;
    let max_life = p.flags.y;

    if life <= 0.0 {
        let slot = atomicAdd(&emit_counter, 1u);
        if slot < u.emit_count {
            p = emit_particle(idx);
        }
        particles_out[idx] = p;
        return;
    }

    let new_age = age + u.delta_time;
    if new_age >= max_life {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    let life_frac = new_age / max_life;
    let dt = u.delta_time;

    // Retrieve home position
    let home = vec2f(p.flags.z, p.flags.w);

    // Compute displaced target via displacement field
    let disp = displacement_field(to_screen(home), u.time);
    let displaced_home = home + disp;
    let disp_mag = length(disp);

    // Spring force toward displaced home (k=3.0 from attraction_strength)
    let to_home = displaced_home - p.pos_life.xy;
    let spring_k = u.attraction_strength;
    var vel = p.vel_size.xy;
    vel += to_home * spring_k * dt;

    // Onset → horizontal gust
    if u.onset > 0.3 {
        let gust_dir = vec2f(1.0, 0.2);
        vel += gust_dir * u.onset * 0.15 * dt;
    }

    // Beat → upward billow
    if u.beat > 0.5 {
        vel += vec2f(0.0, 0.3) * dt;
    }

    // Drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    // Integrate position
    let new_pos = p.pos_life.xy + vel * dt;

    // Size: gentle breathing with audio
    let base_size = mix(p.vel_size.w, u.size_end, life_frac * 0.5);
    let size = base_size * (1.0 + u.rms * 0.2);

    // Alpha: fade in/out, boosted at fabric folds (high displacement)
    // Dampen by audio energy so loud parts don't blow out to uniform fill
    let fade_in = smoothstep(0.0, 0.1, life_frac);
    let fade_out = 1.0 - smoothstep(0.7, 1.0, life_frac);
    let fold_boost = smoothstep(0.02, 0.2, disp_mag);
    let loudness_dampen = 1.0 / (1.0 + (u.bass + u.mid) * 1.5);
    let alpha = (0.01 + 0.05 * fold_boost) * fade_in * fade_out * loudness_dampen;

    // Gentle color evolution
    var col = p.color.rgb;
    let shift = u.centroid * 0.05 * life_frac;
    col = clamp(vec3f(col.r + shift, col.g, col.b - shift * 0.5), vec3f(0.0), vec3f(1.0));

    p.pos_life = vec4f(new_pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
}

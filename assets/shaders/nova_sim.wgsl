// Nova particle simulation — fireworks display.
// Burst emission from random points, gravity pulls sparks down,
// two particle types: shells (large, bright) and sparks (small, flickering).

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
    flags: vec4f, // x=age, y=lifetime, z=is_shell (0 or 1), w=burst_id
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

// HSV to RGB
fn hsv2rgb(h: f32, s: f32, v: f32) -> vec3f {
    let c = v * s;
    let hp = h * 6.0;
    let x = c * (1.0 - abs(hp % 2.0 - 1.0));
    var rgb: vec3f;
    if hp < 1.0 { rgb = vec3f(c, x, 0.0); }
    else if hp < 2.0 { rgb = vec3f(x, c, 0.0); }
    else if hp < 3.0 { rgb = vec3f(0.0, c, x); }
    else if hp < 4.0 { rgb = vec3f(0.0, x, c); }
    else if hp < 5.0 { rgb = vec3f(x, 0.0, c); }
    else { rgb = vec3f(c, 0.0, x); }
    let m = v - c;
    return rgb + vec3f(m);
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Burst center: quantize seed so particles in the same frame share a center
    let burst_id = floor(u.seed * 0.01);
    let burst_seed = burst_id * 7.31;
    let center_x = (hash(burst_seed) * 2.0 - 1.0) * 0.6; // keep bursts in central area
    let center_y = hash(burst_seed + 1.0) * 0.6 + 0.1; // upper half of screen
    let burst_center = vec2f(center_x, center_y);

    // Particle type: 20% shells, 80% sparks
    let is_shell = select(0.0, 1.0, hash(seed_base + 5.0) > 0.8);

    // Radial emission from burst center
    let angle = hash(seed_base) * 6.2831853;
    let spread = 0.3 + hash(seed_base + 1.0) * 0.7; // vary spread per particle
    let speed = u.initial_speed * spread;
    let vel = vec2f(cos(angle), sin(angle)) * speed;

    // Burst color: shared hue per burst, shifted by centroid
    let hue = fract(hash(burst_seed + 3.0) + u.centroid * 0.3);
    let sat = select(0.9, 0.4, is_shell > 0.5); // shells are less saturated (whiter)
    let val = select(0.8, 1.2, is_shell > 0.5); // shells are brighter
    let col = hsv2rgb(hue, sat, val) * (0.3 + u.rms * 0.2);

    // Size: shells are larger
    let size = select(
        u.initial_size * (0.5 + hash(seed_base + 2.0) * 0.5),
        u.initial_size * 2.0,
        is_shell > 0.5
    );

    // Lifetime: shells live shorter (they're the bright core)
    let life = select(u.lifetime, u.lifetime * 0.6, is_shell > 0.5);

    // Stagger initial age slightly
    let initial_age = hash(seed_base + 9.0) * 0.1;

    p.pos_life = vec4f(burst_center, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, select(0.6, 0.9, is_shell > 0.5));
    p.flags = vec4f(initial_age, life, is_shell, burst_id);
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
    let is_shell = p.flags.z;

    var vel = p.vel_size.xy;

    // Gravity (from pfx: [0, -0.4]), modulated by bass
    let grav = u.gravity * (1.0 + u.bass * 0.3);
    vel += grav * dt;

    // Drag: shells have less drag (0.99), sparks more (0.97)
    let drag_val = select(u.drag, 0.99, is_shell > 0.5);
    vel *= 1.0 - (1.0 - drag_val) * dt * 60.0;

    // Kill particles that fall below screen
    let new_pos = p.pos_life.xy + vel * dt;
    if new_pos.y < -1.2 {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    // Size: shrink over lifetime
    let base_size = mix(p.vel_size.w, u.size_end, life_frac);
    let size = base_size * (1.0 + u.rms * 0.1);

    // Alpha: shells fade smoothly, sparks flicker
    var alpha = p.color.a;
    let fade = 1.0 - smoothstep(0.4, 1.0, life_frac);

    if is_shell > 0.5 {
        alpha = 0.9 * fade;
    } else {
        // Flicker: hash(idx + time) creates random twinkle
        let flicker = hash(f32(idx) * 0.37 + u.time * 8.0);
        alpha = 0.6 * fade * (0.5 + flicker * 0.5);
    }

    // Color: fade to warm orange/red as particles cool
    var col = p.color.rgb;
    let cool_color = vec3f(1.0, 0.3, 0.05) * 0.15;
    col = mix(col, cool_color, life_frac * 0.6);

    p.pos_life = vec4f(new_pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
}

// Vortex particle simulation — gravity well / black hole.
// Newtonian 1/r² gravity creates spiral orbits forming an accretion disk.
// Particles inside event horizon are killed. Beat triggers polar jets.

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
    flags: vec4f, // x=age, y=lifetime, z=is_jet (0 or 1), w=rotation_dir (-1 or 1)
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

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Determine if this is a jet particle (30% on beat, 0% otherwise)
    let is_jet = select(0.0, select(0.0, 1.0, hash(seed_base + 7.0) < 0.3), u.beat > 0.5);

    if is_jet > 0.5 {
        // Polar jet: spawn near center, launch vertically
        let jet_dir = select(-1.0, 1.0, hash(seed_base + 8.0) > 0.5); // up or down
        let spread_x = (hash(seed_base) - 0.5) * 0.05;
        let pos = vec2f(spread_x, 0.0);
        let speed = u.initial_speed * 3.0;
        let vel = vec2f(spread_x * 2.0, jet_dir * speed);

        // Jet color: bright blue-white
        let brightness = 0.5 + u.rms * 0.3;
        let col = vec3f(0.4, 0.6, 1.0) * brightness;

        let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.1;

        p.pos_life = vec4f(pos, 0.0, 1.0);
        p.vel_size = vec4f(vel, 0.0, u.initial_size * 1.5);
        p.color = vec4f(col, 0.7);
        p.flags = vec4f(initial_age, u.lifetime * 0.5, 1.0, 0.0);
    } else {
        // Disk particle: spawn on ring with tangential velocity
        let angle = hash(seed_base) * 6.2831853;
        let r = u.emitter_radius * (0.85 + 0.3 * hash(seed_base + 1.0));
        let screen_offset = vec2f(cos(angle), sin(angle)) * r;
        let pos = u.emitter_pos + to_clip(screen_offset);

        // Tangential velocity with slight inward bias
        // 65% rotate one way, 35% the other for asymmetric disk
        let rot_dir = select(-1.0, 1.0, hash(seed_base + 5.0) > 0.35);
        let tangent = vec2f(-sin(angle), cos(angle));
        let inward = vec2f(-cos(angle), -sin(angle));
        let vel_screen = tangent * u.initial_speed * rot_dir + inward * u.initial_speed * 0.08;
        let vel = to_clip(vel_screen);

        // Color: warm → cool gradient based on angle, centroid shifts hue
        let hue = fract(angle / 6.2831853 + u.centroid * 0.3);
        let r_c = abs(hue * 6.0 - 3.0) - 1.0;
        let g_c = 2.0 - abs(hue * 6.0 - 2.0);
        let b_c = 2.0 - abs(hue * 6.0 - 4.0);
        let brightness = 0.15 + u.rms * 0.1;

        let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.3;

        p.pos_life = vec4f(pos, 0.0, 1.0);
        p.vel_size = vec4f(vel, 0.0, u.initial_size * (0.7 + hash(seed_base + 2.0) * 0.6));
        p.color = vec4f(clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0)) * brightness, 0.5);
        p.flags = vec4f(initial_age, u.lifetime, 0.0, rot_dir);
    }

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
    let is_jet = p.flags.z;

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

    var vel = p.vel_size.xy;

    if is_jet > 0.5 {
        // Jet particles: no gravity, just drag and slight spread
        vel *= 1.0 - 0.01 * dt * 60.0;
    } else {
        // Disk particles: Newtonian 1/r² gravity toward center
        let screen_pos = to_screen(p.pos_life.xy);
        let screen_center = to_screen(u.emitter_pos);
        let to_center = screen_center - screen_pos;
        let dist = length(to_center);

        // Event horizon: kill particles that fall in
        let eh_radius = 0.06 + u.sub_bass * 0.02; // sub_bass pulses the horizon
        if dist < eh_radius {
            p.pos_life.w = 0.0;
            particles_out[idx] = p;
            return;
        }

        let dir = to_center / max(dist, 0.001);
        let min_dist = 0.06; // stability floor
        let clamped_dist = max(dist, min_dist);

        // F = G * (1 + bass) / r²
        let gravity_force = u.attraction_strength * (1.0 + u.bass) / (clamped_dist * clamped_dist);
        let screen_accel = dir * gravity_force;

        vel += to_clip(screen_accel) * dt;

        // Onset: orbital perturbation
        if u.onset > 0.3 {
            let tangent = vec2f(-dir.y, dir.x);
            vel += to_clip(tangent * u.onset * 0.05) * dt;
        }
    }

    // Drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    let new_pos = p.pos_life.xy + vel * dt;

    // Kill particles that leave the screen by a margin
    let screen_new = to_screen(new_pos);
    if length(screen_new) > 2.0 {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    // Size
    let base_size = mix(p.vel_size.w, u.size_end, life_frac);
    let size = base_size * (1.0 + u.rms * 0.15);

    // Alpha
    let fade = 1.0 - smoothstep(0.6, 1.0, life_frac);
    let alpha = select(0.4, 0.6, is_jet > 0.5) * fade;

    // Color: disk particles blueshift toward center
    var col = p.color.rgb;
    if is_jet < 0.5 {
        let screen_pos = to_screen(new_pos);
        let dist_to_center = length(screen_pos);
        let blue_amount = smoothstep(0.4, 0.05, dist_to_center);
        let blue_tint = vec3f(0.3, 0.5, 1.0) * 0.2;
        col = mix(col, blue_tint, blue_amount * 0.5);
    }

    p.pos_life = vec4f(new_pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
}

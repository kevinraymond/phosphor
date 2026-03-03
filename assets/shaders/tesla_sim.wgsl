// Tesla particle simulation v6 — magnetic flow field
//
// PHILOSOPHY: Particles follow magnetic field lines as a flow field.
// Each dipole acts as a magnetic monopole. The superposition of all poles
// creates the classic iron-filings field line pattern.
// Charge sign determines direction of travel along field lines:
// positive follows B, negative follows -B → interweaving streams.
// Helical oscillation perpendicular to flow creates spiral appearance.
// Near poles: orbital motion prevents sink convergence.
//
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

fn get_dipole_positions(t: f32, mode: f32, rotation: f32, out_count: ptr<function, u32>) -> array<vec2f, 5> {
    var positions: array<vec2f, 5>;
    let angle = t * rotation * 0.5;
    let ca = cos(angle);
    let sa = sin(angle);

    if mode < 0.5 {
        *out_count = 2u;
        positions[0] = vec2f(ca * 0.4, sa * 0.4);
        positions[1] = vec2f(-ca * 0.4, -sa * 0.4);
    } else if mode < 0.75 {
        *out_count = 4u;
        for (var i = 0u; i < 4u; i++) {
            let a = angle + f32(i) * 1.5707963;
            positions[i] = vec2f(cos(a), sin(a)) * 0.35;
        }
    } else {
        *out_count = 5u;
        for (var i = 0u; i < 4u; i++) {
            let a = angle + f32(i) * 1.5707963;
            positions[i] = vec2f(cos(a), sin(a)) * 0.35;
        }
        positions[4] = vec2f(0.0);
    }

    return positions;
}

fn get_dipole_pos(dipole_idx: u32, t: f32, mode: f32, rotation: f32) -> vec2f {
    let angle = t * rotation * 0.5;
    if mode < 0.5 {
        let ca = cos(angle);
        let sa = sin(angle);
        if dipole_idx == 0u { return vec2f(ca * 0.4, sa * 0.4); }
        return vec2f(-ca * 0.4, -sa * 0.4);
    }
    let a = angle + f32(dipole_idx % 4u) * 1.5707963;
    return vec2f(cos(a), sin(a)) * 0.35;
}

// Find nearest dipole: returns (dir_from_pole.xy, distance)
fn nearest_dipole_info(pos: vec2f, t: f32, mode: f32, rotation: f32) -> vec3f {
    var dcount: u32;
    let dipoles = get_dipole_positions(t, mode, rotation, &dcount);
    var best_dist = 999.0;
    var best_dir = vec2f(0.0);

    for (var i = 0u; i < dcount; i++) {
        let r = pos - dipoles[i];
        let d = length(r);
        if d < best_dist {
            best_dist = d;
            best_dir = r / (d + 0.001);
        }
    }

    return vec3f(best_dir, best_dist);
}

// Magnetic field at position p: sum of monopole contributions.
// Each dipole is a monopole with sign determined by arrangement mode.
// Antiparallel (mode 0.25-0.5) gives classic N-S field lines.
fn magnetic_field(p: vec2f, t: f32, mode: f32, rotation: f32) -> vec2f {
    var dcount: u32;
    let dipoles = get_dipole_positions(t, mode, rotation, &dcount);
    var B = vec2f(0.0);

    for (var i = 0u; i < dcount; i++) {
        let r = p - dipoles[i];
        let dist2 = dot(r, r) + 0.008;
        let dist = sqrt(dist2);

        // Pole sign determines field direction
        var pole_sign = 1.0;
        if mode < 0.25 {
            // Parallel: both same sign (radial fans)
        } else if mode < 0.5 {
            // Antiparallel: classic N-S bar magnet
            if i == 1u { pole_sign = -1.0; }
        } else {
            // Ring/quad: alternating
            if i % 2u == 1u { pole_sign = -1.0; }
        }

        B += pole_sign * r / (dist2 * dist);
    }

    return B;
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 17.31;
    let mode = param(3u);
    let rotation = param(4u);

    var num_sources = 2u;
    if mode >= 0.5 { num_sources = 4u; }
    let source_idx = u32(hash(seed_base + 10.0) * f32(num_sources));
    let dipole_pos = get_dipole_pos(source_idx, u.time, mode, rotation);

    // Emit in ring around dipole
    let emit_angle = hash(seed_base) * 6.2831853;
    let emit_r = 0.06 + hash(seed_base + 1.0) * 0.18;
    let offset = vec2f(cos(emit_angle), sin(emit_angle)) * emit_r;
    let pos = dipole_pos + offset;

    // Charge determines flow direction along field lines
    let charge_ratio = param(2u) + (u.centroid - 0.5) * 0.15;
    let is_positive = hash(seed_base + 3.0) < clamp(charge_ratio, 0.1, 0.9);
    let charge = select(-1.0, 1.0, is_positive);

    // Initial velocity: field direction at emission point
    let B = magnetic_field(pos, u.time, mode, rotation);
    let B_dir = normalize(B + vec2f(0.0001));
    let speed = u.initial_speed * (0.8 + 0.4 * hash(seed_base + 2.0));
    let vel = B_dir * charge * speed;

    // Color: cyan for positive charge, magenta for negative
    let color_mode = param(5u);
    var col: vec3f;
    if color_mode < 0.33 {
        col = select(vec3f(1.0, 0.3, 0.8), vec3f(0.3, 0.8, 1.0), is_positive);
    } else if color_mode < 0.66 {
        col = vec3f(0.4, 0.7, 1.0);
    } else {
        col = vec3f(0.3, 0.6, 1.0);
    }
    col *= 0.5 + u.rms * 0.2;

    let helix_phase = hash(seed_base + 8.0) * 6.2831853;
    let init_size = u.initial_size * (0.7 + hash(seed_base + 5.0) * 0.6);
    let life_var = 1.0 + (hash(seed_base + 6.0) - 0.5) * 0.4;

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    p.color = vec4f(col, 0.35 + hash(seed_base + 7.0) * 0.2);
    // flags: x=age, y=max_life, z=charge (+1/-1), w=helix_phase
    p.flags = vec4f(0.0, u.lifetime * life_var, charge, helix_phase);
    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles {
        return;
    }

    var p = read_particle(idx);
    let life = p.pos_life.w;
    let age = p.flags.x;
    let max_life = p.flags.y;

    if life <= 0.0 {
        let slot = emit_claim();
        if slot < u.emit_count {
            p = emit_particle(idx);
            write_particle(idx, p);
            mark_alive(idx);
        } else {
            write_particle(idx, p);
        }
        return;
    }

    let new_age = age + u.delta_time;
    if new_age >= max_life {
        p.pos_life.w = 0.0;
        write_particle(idx, p);
        return;
    }

    let life_frac = new_age / max_life;
    let dt = u.delta_time;
    var pos = p.pos_life.xy;
    let charge = p.flags.z;
    let helix_phase = p.flags.w;

    let field_str = param(1u);
    let mode = param(3u);
    let rotation = param(4u);
    let tightness = param(6u);

    // === MAGNETIC FLOW FIELD ===
    // Compute field at current position, follow its direction
    let B = magnetic_field(pos, u.time, mode, rotation);
    let B_mag = length(B);
    let B_dir = B / (B_mag + 0.0001);

    // Flow speed: field_strength param + field magnitude for spatial variation
    // Near poles: stronger field → faster (prevents accumulation)
    let flow_str = (0.3 + field_str * 1.2) * (1.0 + u.bass * 0.5);
    let mag_scale = 0.5 + clamp(B_mag * 0.4, 0.0, 1.0);
    var vel = B_dir * charge * u.initial_speed * flow_str * mag_scale;

    // === ORBITAL CORRECTION near poles ===
    // Prevents particles converging at sink poles:
    // Smoothly blend from field-following to tangential orbit
    let nearest = nearest_dipole_info(pos, u.time, mode, rotation);
    let dist_to_pole = nearest.z;
    let dir_from_pole = vec2f(nearest.x, nearest.y);
    let orbital_tangent = vec2f(-dir_from_pole.y, dir_from_pole.x) * charge;
    let orbital_blend = smoothstep(0.18, 0.03, dist_to_pole);
    vel = mix(vel, orbital_tangent * length(vel) * 1.5, orbital_blend);

    // === HELIX: perpendicular oscillation ===
    // Sine wave perpendicular to field direction creates helical motion
    let perp = vec2f(-B_dir.y, B_dir.x);
    let helix_freq = 12.0 + tightness * 25.0;
    let helix_amp = (0.01 + tightness * 0.04) * mag_scale;
    let helix_vel = perp * helix_amp * helix_freq * cos(new_age * helix_freq + helix_phase);

    // === Beat polarity flip ===
    let flip_sens = param(7u);
    if u.beat > 0.5 && flip_sens > 0.05 {
        let flip_hash = hash(f32(idx) * 3.77 + floor(u.time * 4.0));
        if flip_hash < flip_sens * 0.5 {
            p.flags.z = -p.flags.z;
        }
    }

    // === Onset jitter ===
    if u.onset > 0.3 {
        let jitter_angle = hash(f32(idx) * 7.13 + u.time) * 6.2831853;
        vel += vec2f(cos(jitter_angle), sin(jitter_angle)) * u.onset * 0.05;
    }

    // Integrate
    pos += (vel + helix_vel) * dt;

    // Boundary wrap
    if pos.x > 1.3 { pos.x -= 2.6; }
    if pos.x < -1.3 { pos.x += 2.6; }
    if pos.y > 1.3 { pos.y -= 2.6; }
    if pos.y < -1.3 { pos.y += 2.6; }

    // Size
    let init_size = p.pos_life.z;
    let size = init_size * eval_size_curve(life_frac) * (1.0 + u.rms * 0.3);

    // Alpha: fade in/out + proximity dimming near poles
    let fade_in = smoothstep(0.0, 0.08, life_frac);
    let fade_out = 1.0 - smoothstep(0.8, 1.0, life_frac);
    let prox_dim = smoothstep(0.0, 0.15, dist_to_pole);
    let alpha = p.color.a * fade_in * fade_out * eval_opacity_curve(life_frac) * (0.2 + 0.8 * prox_dim);

    // Color: re-derive from current charge (handles beat flips)
    var col: vec3f;
    let color_mode = param(5u);
    let spd = length(vel);

    if color_mode < 0.33 {
        // Charge-based: cyan for +, magenta for -
        if p.flags.z > 0.0 {
            col = vec3f(0.3, 0.8, 1.0);
        } else {
            col = vec3f(1.0, 0.3, 0.8);
        }
        col *= 0.5 + u.rms * 0.2;
    } else if color_mode < 0.66 {
        // Speed-based
        let speed_t = clamp(spd / 1.0, 0.0, 1.0);
        col = mix(vec3f(0.2, 0.4, 1.0), vec3f(1.0, 0.5, 0.1), speed_t) * 0.5;
    } else {
        // Lifetime-based
        col = mix(vec3f(0.2, 0.5, 1.0), vec3f(1.0, 0.4, 0.2), life_frac) * 0.5;
    }

    col *= 1.0 + spd * 0.1;
    col = clamp(col, vec3f(0.0), vec3f(1.0));

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel + helix_vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    write_particle(idx, p);
    mark_alive(idx);
}

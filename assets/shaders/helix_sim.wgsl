// Helix particle simulation — particles follow magnetic dipole field lines
// with helical oscillation. Charge determines helix direction (CW vs CCW).
// Dipole at (0, -0.7): center particles go straight up, edge particles curve
// outward and downward, tracing the classic iron-filings pattern.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

fn dipole_field(pos: vec2f) -> vec2f {
    let dp = pos - vec2f(0.0, -0.7);
    let r2 = dot(dp, dp) + 0.02;
    let r4 = r2 * r2;
    return vec2f(
        2.0 * dp.x * dp.y / r4,
        (dp.y * dp.y - dp.x * dp.x) / r4
    );
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Line emitter — wide spread to hit outer field lines
    let t = hash(seed_base) * 2.0 - 1.0;
    let pos = u.emitter_pos + vec2f(t * u.emitter_radius, 0.0);

    // Initial velocity along local field direction
    let b = dipole_field(pos);
    let b_mag = length(b) + 0.001;
    let b_dir = b / b_mag;
    // Small upward bias so edge particles launch outward rather than straight down
    let emit_dir = normalize(b_dir + vec2f(0.0, 0.2));
    let speed = u.initial_speed * (0.8 + 0.4 * hash(seed_base + 3.0));
    let vel = emit_dir * speed;

    // Charge: 50/50 positive/negative — determines helix direction
    let charge = select(-1.0, 1.0, hash(seed_base + 4.0) > 0.5);
    let mass = 0.5 + hash(seed_base + 5.0) * 0.6;

    // Color from gradient or charge-based
    var col: vec3f;
    if u.gradient_count > 0u {
        let t_grad = select(0.0, 0.5, charge > 0.0) + hash(seed_base + 8.0) * 0.4;
        let grad = eval_color_gradient(clamp(t_grad, 0.0, 1.0));
        col = grad.rgb;
    } else {
        if charge > 0.0 {
            let hue = 0.55 + hash(seed_base + 8.0) * 0.15;
            let r_c = abs(hue * 6.0 - 3.0) - 1.0;
            let g_c = 2.0 - abs(hue * 6.0 - 2.0);
            let b_c = 2.0 - abs(hue * 6.0 - 4.0);
            col = clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0));
        } else {
            let hue = 0.0 + hash(seed_base + 8.0) * 0.1;
            let r_c = abs(hue * 6.0 - 3.0) - 1.0;
            let g_c = 2.0 - abs(hue * 6.0 - 2.0);
            let b_c = 2.0 - abs(hue * 6.0 - 4.0);
            col = clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0));
        }
    }
    let brightness = 1.5 + u.rms * 0.5;
    col *= brightness;

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.05;
    let init_size = u.initial_size * (0.6 + hash(seed_base + 6.0) * 0.8);

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    p.color = vec4f(col, 0.80 + hash(seed_base + 7.0) * 0.20);
    p.flags = vec4f(initial_age, u.lifetime, charge, mass);
    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles { return; }

    var p = particles_in[idx];
    let life = p.pos_life.w;
    let age = p.flags.x;
    let max_life = p.flags.y;

    if life <= 0.0 {
        let slot = emit_claim();
        if slot < u.emit_count {
            p = emit_particle(idx);
            particles_out[idx] = p;
            mark_alive(idx);
        } else {
            particles_out[idx] = p;
        }
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
    let charge = p.flags.z;
    let mass = p.flags.w;
    var vel = p.vel_size.xy;
    var pos = p.pos_life.xy;

    // Local dipole field — NO upward clamp, follow true field geometry
    let b = dipole_field(pos);
    let b_mag = length(b) + 0.001;
    let flow_dir = b / b_mag;
    let flow_perp = vec2f(-flow_dir.y, flow_dir.x);

    // Decompose velocity along/across field line
    let v_along = dot(vel, flow_dir);
    let v_across = dot(vel, flow_perp);

    // Target drift speed along field line
    var target_speed = 0.50 + u.rms * 0.15;

    // Beat: burst of speed
    if u.beat > 0.5 {
        target_speed += 0.30;
    }

    // Guide particle along field line
    let guidance = 3.5;
    let along_accel = (target_speed - v_along) * guidance;

    // Helical oscillation perpendicular to field line
    let helix_freq = 8.0 + u.bass * 4.0;
    let helix_amp = 1.0 * (1.0 - u.bass * 0.3);
    let phase = new_age * helix_freq + charge * 3.14159 + hash(f32(idx) * 7.13) * 6.283;
    let helix_force = sin(phase) * helix_amp;

    // Cross-field: damping + helix
    let cross_accel = -v_across * 2.0 + helix_force;

    // Onset: lateral scatter
    var perturb = vec2f(0.0);
    if u.onset > 0.3 {
        perturb = flow_perp * (hash(f32(idx) * 0.1 + u.time) - 0.5) * u.onset * 0.4;
    }

    vel += (flow_dir * along_accel + flow_perp * cross_accel + perturb) * dt;

    // Light drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    let new_pos = pos + vel * dt;

    // Kill if off screen (wide bounds for curving trajectories)
    if abs(new_pos.y) > 1.3 || abs(new_pos.x) > 2.0 {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    // Size with curve
    let height_frac = clamp((new_pos.y + 1.0) * 0.5, 0.0, 1.0);
    let init_size = p.pos_life.z;
    let base_size = mix(init_size, u.size_end, life_frac * 0.4);
    let size = base_size * (0.7 + 0.3 * height_frac) * eval_size_curve(life_frac) * (1.0 + u.rms * 0.2);

    // Alpha with curves
    let fade_in = smoothstep(0.0, 0.05, life_frac);
    let fade_out = 1.0 - smoothstep(0.7, 1.0, life_frac);
    let alpha = p.color.a * fade_in * fade_out * eval_opacity_curve(life_frac);

    let col = p.color.rgb;

    p.pos_life = vec4f(new_pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
    mark_alive(idx);
}

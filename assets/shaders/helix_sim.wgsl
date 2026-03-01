// Helix particle simulation — Lorentz force electromagnetic field.
// F = q(E + v x B) produces helical spiraling trajectories.
// In 2D+: F_x = q * (E_x + v_y * B_z), F_y = q * (E_y - v_x * B_z)
// Mixed positive/negative charges create interweaving spirals (DNA-like).
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Line emitter at lower area
    let t = hash(seed_base) * 2.0 - 1.0;
    let pos = u.emitter_pos + vec2f(t * u.emitter_radius, 0.0);

    // Initial velocity: strong upward with some spread
    let spread = 0.25 + hash(seed_base + 1.0) * 0.4;
    let angle = 1.5708 + (hash(seed_base + 2.0) - 0.5) * spread;
    let speed = u.initial_speed * (0.8 + 0.4 * hash(seed_base + 3.0));
    let vel = vec2f(cos(angle), sin(angle)) * speed;

    // Charge: 50/50 positive/negative
    let charge = select(-1.0, 1.0, hash(seed_base + 4.0) > 0.5);
    let mass = 0.6 + hash(seed_base + 5.0) * 0.8;

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
    let brightness = 0.22 + u.rms * 0.10;
    col *= brightness;

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.05;
    let init_size = u.initial_size * (0.6 + hash(seed_base + 6.0) * 0.8);

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    p.color = vec4f(col, 0.30 + hash(seed_base + 7.0) * 0.15);
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

    // Magnetic field B_z — reduced base for wider spirals
    let b_z = (1.5 + u.bass * 4.0) * (1.0 + u.mid * 0.2);

    // Electric field E — strong upward drift + oscillating horizontal
    var e_field = vec2f(sin(u.time * 0.5) * 0.04, 0.20);

    // Beat: upward levitation pulse
    if u.beat > 0.5 {
        e_field.y += 0.25;
        // Also add outward radial push for visual flourish
        let dir = normalize(pos + vec2f(0.001, 0.001));
        e_field += dir * 0.15;
    }

    // Onset: lateral perturbation
    if u.onset > 0.3 {
        e_field += vec2f(
            (hash(f32(idx) * 0.1 + u.time) - 0.5) * u.onset * 0.3,
            u.onset * 0.1
        );
    }

    // RMS: slight additional upward drift during loud passages
    e_field.y += u.rms * 0.08;

    // Lorentz force: F = q(E + v x B) / m
    let lorentz_x = charge * (e_field.x + vel.y * b_z) / mass;
    let lorentz_y = charge * (e_field.y - vel.x * b_z) / mass;
    vel += vec2f(lorentz_x, lorentz_y) * dt;

    // Light drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    let new_pos = pos + vel * dt;

    // Kill if off screen
    if new_pos.y > 1.3 || abs(new_pos.x) > 1.3 {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    // Size: height-modulated with curve
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

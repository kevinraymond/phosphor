// Nova particle simulation — spectacular fireworks display.
// Burst emission from random points, gravity pulls sparks down,
// two particle types: shells (large, bright) and sparks (small, flickering).
// Ground bounce for trailing sparks, color gradient over lifecycle.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

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
    return rgb + vec3f(v - c);
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Burst center: particles in same frame share a center
    let burst_id = floor(u.seed * 0.01);
    let burst_seed = burst_id * 7.31;
    let center_x = (hash(burst_seed) * 2.0 - 1.0) * 0.65;
    let center_y = hash(burst_seed + 1.0) * 0.5 + 0.15;
    let burst_center = vec2f(center_x, center_y);

    // Particle type: 15% shells, 85% sparks
    let is_shell = select(0.0, 1.0, hash(seed_base + 5.0) > 0.85);

    // Radial emission with variance
    let angle = hash(seed_base) * 6.2831853;
    let speed_base = u.initial_speed;
    let speed_var = 1.0 + u.speed_variance * (hash(seed_base + 1.0) - 0.5) * 2.0;
    let spread = 0.25 + hash(seed_base + 1.0) * 0.75;
    let speed = speed_base * spread * speed_var;
    var vel = vec2f(cos(angle), sin(angle)) * speed;

    // Shells get extra upward bias
    if is_shell > 0.5 {
        vel.y += 0.1;
    }

    // Burst hue: shared per burst, shifted by centroid
    let hue = fract(hash(burst_seed + 3.0) + u.centroid * 0.3);

    // Color: shells start white-hot, sparks start vivid
    var col: vec3f;
    if is_shell > 0.5 {
        col = vec3f(1.0, 0.95, 0.85) * 0.5; // white-hot core
    } else {
        let sat = 0.85 + hash(seed_base + 8.0) * 0.15;
        col = hsv2rgb(hue, sat, 0.9) * (0.3 + u.rms * 0.15);
    }

    // Size with variance: shells are 2-3x larger
    let size_var = 1.0 + u.size_variance * (hash(seed_base + 2.0) - 0.5) * 2.0;
    let size = select(
        u.initial_size * (0.5 + hash(seed_base + 2.0) * 0.5) * size_var,
        u.initial_size * 2.5 * size_var,
        is_shell > 0.5
    );

    // Lifetime with variance: shells die faster
    let life_var = 1.0 + u.life_variance * (hash(seed_base + 10.0) - 0.5);
    let life = select(u.lifetime * life_var, u.lifetime * 0.5 * life_var, is_shell > 0.5);

    // Stagger initial age for less uniform explosions
    let initial_age = hash(seed_base + 9.0) * 0.15;

    p.pos_life = vec4f(burst_center, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, select(0.65, 0.95, is_shell > 0.5));
    p.flags = vec4f(initial_age, life, is_shell, burst_id);
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
    let is_shell = p.flags.z;
    var vel = p.vel_size.xy;
    var pos = p.pos_life.xy;

    // Gravity modulated by bass
    let grav = u.gravity * (1.0 + u.bass * 0.3);
    vel += grav * dt;

    // Drag: shells less drag, sparks more
    let drag_val = select(u.drag, 0.995, is_shell > 0.5);
    vel *= 1.0 - (1.0 - drag_val) * dt * 60.0;

    // Ground bounce for sparks
    var new_pos = pos + vel * dt;
    if u.ground_bounce > 0.0 {
        let bounced = apply_ground_bounce(new_pos, vel);
        new_pos = bounced.xy;
        vel = bounced.zw;
    }

    // Kill if below screen
    if new_pos.y < -1.3 {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    // Size with lifetime curve
    let base_size = p.vel_size.w;
    let size = base_size * eval_size_curve(life_frac) * (1.0 + u.rms * 0.1);

    // Alpha with lifetime curve
    var alpha: f32;
    let opacity = eval_opacity_curve(life_frac);
    if is_shell > 0.5 {
        alpha = 0.95 * opacity;
    } else {
        // Flicker for sparks
        let flicker = hash(f32(idx) * 0.37 + u.time * 8.0);
        alpha = 0.65 * opacity * (0.5 + flicker * 0.5);
    }

    // Color: lifecycle gradient (white-hot → vivid → orange → dim)
    var col: vec3f;
    if u.gradient_count > 0u {
        let grad = eval_color_gradient(life_frac);
        col = grad.rgb * (0.3 + u.rms * 0.15);
        // Shells stay brighter
        if is_shell > 0.5 {
            col *= 1.5;
        }
    } else {
        // Fallback: fade to warm orange/red
        col = p.color.rgb;
        let cool_color = vec3f(1.0, 0.3, 0.05) * 0.15;
        col = mix(col, cool_color, life_frac * 0.6);
    }

    p.pos_life = vec4f(new_pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
    mark_alive(idx);
}

// Spirograph particle simulation — hypotrochoid parametric curves.
// Each particle traces a hypotrochoid path: position computed directly from parametric equation.
// 5 arms with distinct k-ratios for multi-petal patterns.
// vel_size.xy repurposed: x = parametric phase (angle), y = speed multiplier.
// flags.z = arm index, flags.w = pen distance offset.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

const TAU: f32 = 6.2831853;

// Hypotrochoid: point on curve where small circle (radius r) rolls inside big circle (radius R)
// pen at distance d from center of small circle.
// x = (R-r)*cos(t) + d*cos((R-r)/r * t)
// y = (R-r)*sin(t) - d*sin((R-r)/r * t)
// k = R/r determines petal count: petals = k when k is integer
fn hypotrochoid(t: f32, k: f32, d: f32, scale: f32) -> vec2f {
    let km1 = k - 1.0;
    let x = (km1 * cos(t) + d * cos(km1 * t)) / k * scale;
    let y = (km1 * sin(t) - d * sin(km1 * t)) / k * scale;
    return vec2f(x, y);
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Assign to one of 5 arms
    let arm = u32(hash(seed_base) * 5.0) % 5u;
    let arm_f = f32(arm);

    // Random starting phase, spread across the full circle
    let phase = hash(seed_base + 1.0) * TAU;

    // Speed variation per particle
    let speed_mult = 0.8 + hash(seed_base + 2.0) * 0.4;

    // Pen distance variation
    let pen_offset = hash(seed_base + 3.0) * 0.3;

    // Color from gradient
    var col: vec3f;
    if u.gradient_count > 0u {
        let t = fract(arm_f * 0.2 + hash(seed_base + 5.0) * 0.15);
        let grad = eval_color_gradient(t);
        col = grad.rgb;
    } else {
        let hue = fract(arm_f * 0.2 + 0.55);
        let r_c = abs(hue * 6.0 - 3.0) - 1.0;
        let g_c = 2.0 - abs(hue * 6.0 - 2.0);
        let b_c = 2.0 - abs(hue * 6.0 - 4.0);
        col = clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0)) * 0.7;
    }

    // Stagger initial age for smooth fill
    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.8;

    // Start at parametric position (will be computed in update)
    p.pos_life = vec4f(0.0, 0.0, 0.0, 1.0);
    // vel_size: x = phase, y = speed_mult, z unused, w = initial_size
    p.vel_size = vec4f(phase, speed_mult, 0.0, u.initial_size * (0.7 + hash(seed_base + 6.0) * 0.6));
    p.color = vec4f(col, 0.5 + hash(seed_base + 7.0) * 0.3);
    // flags: x = age, y = max_life, z = arm index, w = pen offset
    p.flags = vec4f(initial_age, u.lifetime, arm_f, pen_offset);
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
            // Compute initial position
            let arm = u32(p.flags.z) % 5u;
            let k_ratios = array<f32, 5>(3.0, 4.0, 3.5, 5.0, 4.667);
            let k = k_ratios[arm] + u.bass * 1.5;
            let scale = u.emitter_radius * (0.8 + param(2u) * 0.8);
            let d = 0.6 + p.flags.w + u.mid * 0.2;
            let pos = hypotrochoid(p.vel_size.x, k, d, scale);
            p.pos_life = vec4f(pos, 0.0, 1.0);
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

    // Retrieve parametric state
    let phase = p.vel_size.x;
    let speed_mult = p.vel_size.y;
    let arm = u32(p.flags.z) % 5u;
    let pen_offset = p.flags.w;

    // Params
    let draw_speed_param = param(1u);
    let scale_param = param(2u);
    let complexity_param = param(3u);
    let drift_param = param(4u);
    let color_spread = param(5u);

    // k-ratios per arm — bass morphs them
    let k_ratios = array<f32, 5>(3.0, 4.0, 3.5, 5.0, 4.667);
    var k = k_ratios[arm];
    // Complexity param blends toward higher modes
    k += complexity_param * 2.0;
    // Bass morphs petal count
    k += u.bass * 1.5;

    // Drawing speed: base + param + mid + beat burst
    var spd = (0.5 + draw_speed_param * 1.5) * speed_mult;
    spd *= (1.0 + u.mid * 0.8);
    if u.beat > 0.5 {
        spd *= 2.5;
    }

    // Advance phase
    let new_phase = phase + spd * dt;

    // Pen distance: mid affects it
    let d = 0.6 + pen_offset + u.mid * 0.2;

    // Pattern scale: onset pulses it
    var scale = u.emitter_radius * (0.8 + scale_param * 0.8);
    scale *= (1.0 + u.onset * 0.15);

    // Compute position on hypotrochoid
    var pos = hypotrochoid(new_phase, k, d, scale);

    // Drifting center — each arm orbits slowly
    let arm_f = f32(arm);
    let drift_r = drift_param * 0.15;
    let drift_angle = u.time * 0.1 * (1.0 + arm_f * 0.3) + arm_f * TAU / 5.0;
    pos += vec2f(cos(drift_angle), sin(drift_angle)) * drift_r;

    // Size: RMS responsive
    let init_size = p.vel_size.w;
    let base_size = mix(init_size, u.size_end, life_frac * 0.5);
    let size = base_size * (1.0 + u.rms * 0.4);

    // Alpha: fade in/out
    let fade_in = smoothstep(0.0, 0.05, life_frac);
    let fade_out = 1.0 - smoothstep(0.8, 1.0, life_frac);
    let alpha = p.color.a * fade_in * fade_out * (0.6 + u.rms * 0.4);

    // Color: shift by centroid and spread param
    var col = p.color.rgb;
    let hue_shift = u.centroid * color_spread * 0.15;
    col = clamp(vec3f(col.r + hue_shift, col.g, col.b - hue_shift * 0.5), vec3f(0.0), vec3f(1.0));

    p.pos_life = vec4f(pos, 0.0, 1.0);
    p.vel_size = vec4f(new_phase, speed_mult, 0.0, init_size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
    mark_alive(idx);
}

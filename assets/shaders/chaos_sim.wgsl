// Chaos particle simulation — Lorenz and Rossler strange attractors.
// Particles trace attractor trajectories using RK4 integration.
// 3D position stored in: flags.zw = (x, y), vel_size.z = z
// Audio-reactive attractor parameters create dramatic bifurcation transitions.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

// Lorenz attractor: dx/dt = sigma*(y-x), dy/dt = x*(rho-z)-y, dz/dt = x*y - beta*z
fn lorenz(p: vec3f, sigma: f32, rho: f32, beta: f32) -> vec3f {
    return vec3f(
        sigma * (p.y - p.x),
        p.x * (rho - p.z) - p.y,
        p.x * p.y - beta * p.z
    );
}

// Rossler attractor: dx/dt = -y-z, dy/dt = x + a*y, dz/dt = b + z*(x-c)
fn rossler(p: vec3f, a: f32, b: f32, c: f32) -> vec3f {
    return vec3f(
        -p.y - p.z,
        p.x + a * p.y,
        b + p.z * (p.x - c)
    );
}

fn rk4_lorenz(p: vec3f, dt: f32, sigma: f32, rho: f32, beta: f32) -> vec3f {
    let k1 = lorenz(p, sigma, rho, beta);
    let k2 = lorenz(p + k1 * dt * 0.5, sigma, rho, beta);
    let k3 = lorenz(p + k2 * dt * 0.5, sigma, rho, beta);
    let k4 = lorenz(p + k3 * dt, sigma, rho, beta);
    return p + (k1 + k2 * 2.0 + k3 * 2.0 + k4) * dt / 6.0;
}

fn rk4_rossler(p: vec3f, dt: f32, a: f32, b: f32, c: f32) -> vec3f {
    let k1 = rossler(p, a, b, c);
    let k2 = rossler(p + k1 * dt * 0.5, a, b, c);
    let k3 = rossler(p + k2 * dt * 0.5, a, b, c);
    let k4 = rossler(p + k3 * dt, a, b, c);
    return p + (k1 + k2 * 2.0 + k3 * 2.0 + k4) * dt / 6.0;
}

// 3D projection with Y-axis rotation
fn project_rotated(p: vec3f, angle: f32, zoom: f32) -> vec2f {
    let ca = cos(angle);
    let sa = sin(angle);
    let rotated = vec3f(p.x * ca + p.z * sa, p.y, -p.x * sa + p.z * ca);
    // Centered perspective: Lorenz attractor center is roughly (0, 0, rho)
    let centered = rotated - vec3f(0.0, 0.0, 27.0);
    let depth = 1.0 / max(1.0 + centered.z * 0.015, 0.3);
    return vec2f(centered.x, centered.y) * zoom * depth;
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Start near the Lorenz attractor basin: perturbed from a known point on the attractor
    let px = (hash(seed_base) - 0.5) * 20.0;
    let py = (hash(seed_base + 1.0) - 0.5) * 20.0;
    let pz = hash(seed_base + 2.0) * 30.0 + 10.0;

    // Color from gradient or default warm spectrum
    var col: vec3f;
    if u.gradient_count > 0u {
        let t = hash(seed_base + 5.0);
        let grad = eval_color_gradient(t);
        col = grad.rgb;
    } else {
        let hue = fract(hash(seed_base + 5.0) * 0.4 + 0.1 + u.centroid * 0.3);
        let r_c = abs(hue * 6.0 - 3.0) - 1.0;
        let g_c = 2.0 - abs(hue * 6.0 - 2.0);
        let b_c = 2.0 - abs(hue * 6.0 - 4.0);
        col = clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0)) * 0.18;
    }

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.1;
    let init_size = u.initial_size * (0.6 + hash(seed_base + 6.0) * 0.8);

    let rot_angle = u.time * 0.15;
    let zoom = 0.035 + u.emitter_radius * 0.04;
    let projected = project_rotated(vec3f(px, py, pz), rot_angle, zoom);

    p.pos_life = vec4f(projected, init_size, 1.0);
    p.vel_size = vec4f(0.0, 0.0, pz, init_size);
    p.color = vec4f(col, 0.3);
    p.flags = vec4f(initial_age, u.lifetime, px, py);
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

    // Recover 3D position
    var pos3d = vec3f(p.flags.z, p.flags.w, p.vel_size.z);

    // Check for NaN/divergence — reset particle if needed
    if any(abs(pos3d) > vec3f(200.0)) || any(pos3d != pos3d) {
        p.pos_life.w = 0.0;
        particles_out[idx] = p;
        return;
    }

    // Audio-reactive Lorenz parameters
    let sigma = 10.0 + u.centroid * 5.0;
    let rho = 22.0 + u.bass * 12.0 + u.mid * 4.0;
    let beta = 2.667 + u.mid * 0.5;

    // Rossler parameters (only used when mix > 0)
    let rossler_a = 0.2 + u.centroid * 0.1;
    let rossler_b = 0.2;
    let rossler_c = 5.7 + u.bass * 2.0;

    let mix_param = u.attraction_strength;

    // Adaptive sub-stepping: take multiple small steps for stability
    let speed = 0.5 + u.initial_speed * 2.0;
    let total_dt = u.delta_time * speed;
    let sub_steps = 4u;
    let step_dt = total_dt / f32(sub_steps);

    for (var s = 0u; s < sub_steps; s++) {
        if mix_param < 0.01 {
            pos3d = rk4_lorenz(pos3d, step_dt, sigma, rho, beta);
        } else if mix_param > 0.99 {
            // Rossler in its native scale (much smaller than Lorenz)
            let r_scaled = pos3d * vec3f(0.1, 0.1, 0.05);
            let r_result = rk4_rossler(r_scaled, step_dt, rossler_a, rossler_b, rossler_c);
            pos3d = r_result * vec3f(10.0, 10.0, 20.0);
        } else {
            let lorenz_next = rk4_lorenz(pos3d, step_dt, sigma, rho, beta);
            let r_scaled = pos3d * vec3f(0.1, 0.1, 0.05);
            let r_result = rk4_rossler(r_scaled, step_dt, rossler_a, rossler_b, rossler_c);
            let rossler_next = r_result * vec3f(10.0, 10.0, 20.0);
            pos3d = mix(lorenz_next, rossler_next, mix_param);
        }
    }

    // Onset perturbation
    if u.onset > 0.3 {
        let kick = vec3f(
            (hash(f32(idx) * 0.1 + u.time) - 0.5) * 3.0,
            (hash(f32(idx) * 0.2 + u.time) - 0.5) * 3.0,
            (hash(f32(idx) * 0.3 + u.time) - 0.5) * 3.0
        );
        pos3d += kick * u.onset * 0.2;
    }

    // Soft clamp with damping near boundaries
    let limit = 60.0;
    let soft = 50.0;
    for (var axis = 0u; axis < 3u; axis++) {
        let v = pos3d[axis];
        if v > soft { pos3d[axis] = soft + (v - soft) * 0.5; }
        if v < -soft { pos3d[axis] = -soft + (v + soft) * 0.5; }
    }
    pos3d = clamp(pos3d, vec3f(-limit), vec3f(limit));

    // Project to 2D with camera rotation
    let zoom = 0.035 + u.emitter_radius * 0.04;
    let rot_speed = 0.12 + u.mid * 0.08;
    let rot_angle = u.time * rot_speed;
    let projected = project_rotated(pos3d, rot_angle, zoom);

    // Depth-based size modulation
    let ca = cos(rot_angle);
    let sa = sin(rot_angle);
    let rotated_z = -pos3d.x * sa + pos3d.z * ca;
    let depth_norm = clamp((rotated_z + 40.0) / 80.0, 0.0, 1.0);
    let depth_mod = 0.5 + 0.5 * depth_norm;

    let init_size = p.pos_life.z;
    let base_size = mix(init_size, u.size_end, life_frac * 0.4);
    let size = base_size * depth_mod * eval_size_curve(life_frac) * (1.0 + u.rms * 0.3);

    let fade_in = smoothstep(0.0, 0.05, life_frac);
    let fade_out = 1.0 - smoothstep(0.8, 1.0, life_frac);
    let alpha = p.color.a * fade_in * fade_out * depth_mod * eval_opacity_curve(life_frac);

    // Color: temperature-based — use z-position for heat coloring
    var col = p.color.rgb;
    let temp = clamp(pos3d.z / 50.0, 0.0, 1.0);
    col *= 0.7 + temp * 0.6;
    col = clamp(col, vec3f(0.0), vec3f(0.5));

    p.flags = vec4f(new_age, max_life, pos3d.x, pos3d.y);
    p.vel_size = vec4f(0.0, 0.0, pos3d.z, size);
    p.pos_life = vec4f(projected, init_size, 1.0);
    p.color = vec4f(col, alpha);

    particles_out[idx] = p;
    mark_alive(idx);
}

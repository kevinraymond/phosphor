// Murmur particle simulation — Full Boids flocking model.
// Three forces: separation (avoid crowding), alignment (match heading), cohesion (steer to center).
// Heading smoothing via angular low-pass filter eliminates jitter.
// Audio-reactive order-disorder phase transitions.
// Uses spatial hash (group 3) for efficient neighbor queries.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Screen emitter — fill the sky
    let pos = rand_vec2(seed_base) * 0.85;

    // Random initial heading
    let angle = hash(seed_base + 2.0) * 6.2831853;
    let speed = u.initial_speed * (0.8 + 0.4 * hash(seed_base + 3.0));
    let vel = vec2f(cos(angle), sin(angle)) * speed;

    // Dark bird silhouette colors via gradient or default
    var col: vec3f;
    if u.gradient_count > 0u {
        let t = hash(seed_base + 5.0);
        let grad = eval_color_gradient(t);
        col = grad.rgb;
    } else {
        let tone = 0.04 + hash(seed_base + 5.0) * 0.03;
        col = vec3f(tone);
    }

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.5;
    let init_size = u.initial_size * (0.7 + hash(seed_base + 6.0) * 0.6);

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    p.color = vec4f(col, 0.9);
    p.flags = vec4f(initial_age, u.lifetime, angle, 0.0);
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
    let pos = p.pos_life.xy;
    let heading = p.flags.z;

    // --- Boids parameters ---
    let separation_radius = 0.025;
    let interaction_radius = 0.08;
    let separation_weight = u.turbulence;
    let cohesion_weight = u.attraction_strength;
    let smoothing = clamp(u.noise_speed, 0.05, 0.3);

    // --- Neighbor query: separation + alignment + cohesion in one pass ---
    let my_cell = sh_pos_to_cell(pos);
    var sep_force = vec2f(0.0);
    var sum_cos = 0.0;
    var sum_sin = 0.0;
    var center_of_mass = vec2f(0.0);
    var neighbor_count = 0u;
    let max_neighbors = 12u;

    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let cx = my_cell.x + dx;
            let cy = my_cell.y + dy;
            let range = sh_cell_range(cx, cy);
            let start = range.x;
            let count = range.y;

            for (var i = 0u; i < count && neighbor_count < max_neighbors; i++) {
                let ni = sh_sorted_indices[start + i];
                if ni == idx { continue; }

                let np = particles_in[ni];
                if np.pos_life.w <= 0.0 { continue; }

                let diff = pos - np.pos_life.xy;
                let dist = length(diff);
                if dist > interaction_radius || dist < 0.0001 { continue; }

                // Separation: exponential repulsion for close neighbors
                if dist < separation_radius {
                    sep_force += normalize(diff) * (separation_radius / dist - 1.0);
                }

                // Alignment: accumulate heading vectors
                let n_heading = np.flags.z;
                sum_cos += cos(n_heading);
                sum_sin += sin(n_heading);

                // Cohesion: accumulate neighbor positions
                center_of_mass += np.pos_life.xy;
                neighbor_count++;
            }
        }
    }

    // --- Compute target velocity from Boids forces ---

    // Alignment: average heading including self
    sum_cos += cos(heading);
    sum_sin += sin(heading);
    var target_vel = vec2f(sum_cos, sum_sin);

    // Cohesion: steer toward center of mass
    if neighbor_count > 0u {
        let com = center_of_mass / f32(neighbor_count);
        let to_com = com - pos;
        let com_dist = length(to_com);
        if com_dist > 0.001 {
            target_vel += normalize(to_com) * cohesion_weight * min(com_dist * 10.0, 1.0);
        }
    }

    // Separation: steer away from crowding
    let sep_len = length(sep_force);
    if sep_len > 0.001 {
        target_vel += normalize(sep_force) * separation_weight;
    }

    // --- Audio modulation ---
    let order_base = u.emitter_radius;
    let base_eta = (1.0 - order_base) * 2.5;
    var eta = base_eta + u.bass * 2.0;

    // Beat: sharp cohesion spike — flock suddenly tightens
    if u.beat > 0.5 {
        eta *= 0.15;
        if neighbor_count > 0u {
            let com = center_of_mass / f32(neighbor_count);
            target_vel += normalize(com - pos) * 3.0;
        }
    }

    // Onset: flock splits — separation burst
    if u.onset > 0.3 {
        target_vel += sep_force * u.onset * 2.0;
        eta += u.onset * 1.0;
    }

    eta = clamp(eta, 0.0, 3.14159);

    // Add angular noise
    let noise = (hash(f32(idx) * 0.37 + u.time * 3.0) - 0.5) * 2.0 * eta;
    let raw_heading = atan2(target_vel.y, target_vel.x) + noise;

    // --- Heading smoothing: angular low-pass filter ---
    let angle_delta = atan2(sin(raw_heading - heading), cos(raw_heading - heading));
    let new_heading = heading + angle_delta * smoothing;

    // --- Speed with per-bird variation ---
    let base_speed = u.initial_speed * (1.0 + u.mid * 0.5);
    let speed = base_speed * (0.85 + hash(f32(idx) * 7.1) * 0.3);
    let new_vel = vec2f(cos(new_heading), sin(new_heading)) * speed;

    // --- Boundary: soft repulsion from edges ---
    var boundary = vec2f(0.0);
    let edge = 0.15;
    let bstr = 4.0;
    if pos.x > 1.0 - edge  { boundary.x -= (pos.x - (1.0 - edge)) * bstr; }
    if pos.x < -1.0 + edge { boundary.x -= (pos.x + 1.0 - edge) * bstr; }
    if pos.y > 1.0 - edge  { boundary.y -= (pos.y - (1.0 - edge)) * bstr; }
    if pos.y < -1.0 + edge { boundary.y -= (pos.y + 1.0 - edge) * bstr; }

    let vel = new_vel + boundary * dt;
    let new_pos = pos + vel * dt;

    // --- Depth-based sizing: higher = further = smaller ---
    let depth = 1.0 - (new_pos.y + 1.0) * 0.15;
    let init_size = p.pos_life.z;
    let base_size = init_size * depth;
    let density_mod = 1.0 + f32(neighbor_count) * 0.03;
    let size = base_size * density_mod * (1.0 + u.rms * 0.15);

    // --- Alpha: opaque birds with fade in/out ---
    let fade_in = smoothstep(0.0, 0.03, life_frac);
    let fade_out = 1.0 - smoothstep(0.85, 1.0, life_frac);
    var alpha = 0.9 * fade_in * fade_out;
    // Apply opacity curve if available
    alpha *= eval_opacity_curve(life_frac);

    // --- Color: truly dark silhouettes, no brightness boost ---
    var col = p.color.rgb;
    let warmth = u.centroid * 0.03;
    col += vec3f(warmth, 0.0, -warmth * 0.5);
    col = clamp(col, vec3f(0.0), vec3f(0.15));

    p.pos_life = vec4f(new_pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags = vec4f(new_age, max_life, new_heading, 0.0);

    particles_out[idx] = p;
    mark_alive(idx);
}

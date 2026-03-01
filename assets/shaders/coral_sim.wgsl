// Coral particle simulation — pattern-aware sparkle for Gray-Scott reaction-diffusion.
// Particles can't read the feedback texture, so they use an analytical Turing approximation
// to concentrate on pattern areas. Their warm glow (zero blue, low alpha) contaminates the
// B channel in feedback, creating a self-reinforcing loop where coral grows toward particles.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

const PI: f32 = 3.14159265;

// Analytical Turing pattern approximation — 3 cosine waves at 60-degree intervals.
// Returns [-1, 1]: +1 at hexagonal spot centers, -1 in gaps.
// Approximates where the Gray-Scott RD will form patterns.
fn turing(p: vec2f, k: f32, morph: f32) -> f32 {
    let d1 = cos(k * p.x);
    let d2 = cos(k * (-0.5 * p.x + 0.866 * p.y));
    let d3 = cos(k * (-0.5 * p.x - 0.866 * p.y));
    let spots = (d1 + d2 + d3) / 3.0;
    let stripes = d1;
    return mix(spots, stripes, morph);
}

// Analytical gradient for edge detection and flow.
fn turing_gradient(p: vec2f, k: f32, morph: f32) -> vec2f {
    let s1 = -sin(k * p.x);
    let s2 = -sin(k * (-0.5 * p.x + 0.866 * p.y));
    let s3 = -sin(k * (-0.5 * p.x - 0.866 * p.y));

    let spots_dx = (s1 * k + s2 * (-0.5 * k) + s3 * (-0.5 * k)) / 3.0;
    let spots_dy = (s2 * (0.866 * k) + s3 * (-0.866 * k)) / 3.0;

    let stripes_dx = s1 * k;
    let stripes_dy = 0.0;

    return vec2f(
        mix(spots_dx, stripes_dx, morph),
        mix(spots_dy, stripes_dy, morph)
    );
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    let aspect_r = u.resolution.x / max(u.resolution.y, 1.0);
    var pos = rand_vec2(seed_base) * 0.9;
    pos.x *= min(aspect_r, 1.0);

    let angle = hash(seed_base + 2.0) * 2.0 * PI;
    let speed = u.initial_speed * (0.3 + 0.5 * hash(seed_base + 3.0));
    let vel = vec2f(cos(angle), sin(angle)) * speed;

    // Zero-blue warm colors from gradient, subtle brightness
    var col: vec3f;
    if u.gradient_count > 0u {
        let t = hash(seed_base + 5.0);
        let grad = eval_color_gradient(t);
        col = vec3f(grad.r, grad.g, 0.0) * 0.8;
    } else {
        let hue_t = hash(seed_base + 5.0);
        col = vec3f(
            0.8 + hue_t * 0.4,
            0.3 + hue_t * 0.3,
            0.0
        );
    }

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.15;
    let init_size = u.initial_size * (0.8 + hash(seed_base + 6.0) * 0.4);

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    // Very low alpha — subtle sparkle, minimal B contamination
    p.color = vec4f(col, 0.008);
    p.flags = vec4f(initial_age, u.lifetime, 0.0, 0.0);
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
    var vel = p.vel_size.xy;
    var pos = p.pos_life.xy;

    // Evaluate analytical pattern at particle position
    let aspect_r = u.resolution.x / max(u.resolution.y, 1.0);
    let pattern_pos = vec2f(pos.x / min(aspect_r, 1.0), pos.y);
    let drift = vec2f(sin(u.time * 0.03), cos(u.time * 0.04)) * 0.3;
    let eval_pos = pattern_pos + drift;

    let k = 10.0 + u.bass * 5.0 + u.mid * 3.0;
    let morph = clamp(u.centroid * 1.5, 0.0, 1.0);

    let val = turing(eval_pos, k, morph);
    let grad = turing_gradient(eval_pos, k, morph);
    let grad_len = length(grad);

    // Pattern field: 0 in gaps, 1 on spot centers
    let on_feature = clamp(val * 0.5 + 0.5, 0.0, 1.0);

    // PRIMARY MOTION: curl noise for organic flow
    if u.noise_octaves > 0u {
        let curl = fbm_curl_2d(pos + u.time * u.noise_speed, u.noise_octaves, u.noise_lacunarity, u.noise_persistence);
        vel += curl * u.turbulence * dt;
    }

    // Attraction toward pattern spots (gradient ascent)
    if grad_len > 0.01 {
        vel += normalize(grad) * u.attraction_strength * dt;
    }

    // Tangential flow along pattern edges (alternating direction)
    if grad_len > 0.01 {
        let ccw = select(-1.0, 1.0, idx % 2u == 0u);
        let tangent = vec2f(-grad.y * ccw, grad.x * ccw) / grad_len;
        vel += tangent * 0.15 * dt;
    }

    // Onset: scatter particles outward
    if u.onset > 0.4 {
        let scatter_angle = hash(f32(idx) * 0.71 + u.time) * 2.0 * PI;
        vel += vec2f(cos(scatter_angle), sin(scatter_angle)) * u.onset * 0.05 * dt;
    }

    // Beat: extra attraction pulse
    if u.beat > 0.5 && grad_len > 0.01 {
        vel += normalize(grad) * u.attraction_strength * 0.6 * dt;
    }

    // Drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    // Integrate position
    var new_pos = pos + vel * dt;

    // Soft boundary repulsion
    let edge_margin = 0.88;
    let repulse_k = 4.0;
    if new_pos.x > edge_margin  { vel.x -= (new_pos.x - edge_margin) * repulse_k * dt; }
    if new_pos.x < -edge_margin { vel.x -= (new_pos.x + edge_margin) * repulse_k * dt; }
    if new_pos.y > edge_margin  { vel.y -= (new_pos.y - edge_margin) * repulse_k * dt; }
    if new_pos.y < -edge_margin { vel.y -= (new_pos.y + edge_margin) * repulse_k * dt; }
    new_pos = clamp(new_pos, vec2f(-1.1), vec2f(1.1));

    // === Appearance modulated by pattern ===

    // Size: modestly larger on pattern features
    let init_size = p.pos_life.z;
    let base_size = mix(init_size, u.size_end, life_frac * 0.3);
    let feature_size = smoothstep(0.2, 0.7, on_feature);
    let feature_mod = 0.3 + 1.0 * feature_size;
    let size = base_size * feature_mod * eval_size_curve(life_frac);

    // Very subtle alpha — particles are barely-there sparkle, RD pattern dominates
    let fade_in = smoothstep(0.0, 0.1, life_frac);
    let fade_out = 1.0 - smoothstep(0.85, 1.0, life_frac);
    let feature_alpha = smoothstep(0.15, 0.6, on_feature);
    let alpha = 0.008 * fade_in * fade_out * (0.2 + 0.8 * feature_alpha) * eval_opacity_curve(life_frac);

    // Ensure zero blue in color output (critical for A channel preservation)
    let out_col = vec3f(p.color.r, p.color.g, 0.0);

    p.pos_life = vec4f(new_pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(out_col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
    mark_alive(idx);
}

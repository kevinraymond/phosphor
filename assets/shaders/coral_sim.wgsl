// Coral particle simulation — Turing pattern attraction with analytical gradients.
// Particles are attracted to features of reaction-diffusion-like patterns:
// hexagonal spots that morph to labyrinthine stripes based on audio parameters.
// Uses analytical derivatives (no numerical central-difference artifacts).
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

const PI: f32 = 3.14159265;

// Turing-like pattern function: sum of 3 cosine waves at 60-degree intervals
fn turing(p: vec2f, k: f32, morph: f32) -> f32 {
    let d1 = cos(k * p.x);
    let d2 = cos(k * (-0.5 * p.x + 0.866 * p.y));
    let d3 = cos(k * (-0.5 * p.x - 0.866 * p.y));
    let spots = (d1 + d2 + d3) / 3.0;
    let stripes = d1;
    return mix(spots, stripes, morph);
}

// Analytical gradient of Turing function (exact, no epsilon)
fn turing_gradient(p: vec2f, k: f32, morph: f32) -> vec2f {
    // Derivatives of each cosine wave
    let s1 = -sin(k * p.x);
    let s2 = -sin(k * (-0.5 * p.x + 0.866 * p.y));
    let s3 = -sin(k * (-0.5 * p.x - 0.866 * p.y));

    // Gradient of spots = (d1+d2+d3)/3
    let spots_dx = (s1 * k + s2 * (-0.5 * k) + s3 * (-0.5 * k)) / 3.0;
    let spots_dy = (s2 * (0.866 * k) + s3 * (-0.866 * k)) / 3.0;

    // Gradient of stripes = d1
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

    // Screen emitter with aspect correction
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    var pos = rand_vec2(seed_base) * 0.85;
    pos.x *= min(aspect, 1.0);

    // Small random velocity
    let angle = hash(seed_base + 2.0) * 6.2831853;
    let speed = u.initial_speed * (0.3 + 0.7 * hash(seed_base + 3.0));
    let vel = vec2f(cos(angle), sin(angle)) * speed;

    // Color: warm coral/organic tones
    var col: vec3f;
    if u.gradient_count > 0u {
        let t = hash(seed_base + 5.0);
        let grad = eval_color_gradient(t);
        col = grad.rgb;
    } else {
        let hue = fract(0.08 + hash(seed_base + 5.0) * 0.12 + u.centroid * 0.15);
        let r_c = abs(hue * 6.0 - 3.0) - 1.0;
        let g_c = 2.0 - abs(hue * 6.0 - 2.0);
        let b_c = 2.0 - abs(hue * 6.0 - 4.0);
        let brightness = 0.15 + u.rms * 0.08;
        col = clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0)) * brightness;
    }

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.3;
    let init_size = u.initial_size * (0.7 + hash(seed_base + 6.0) * 0.6);
    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    p.color = vec4f(col, 0.25 + hash(seed_base + 7.0) * 0.15);
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

    // Aspect ratio correction for pattern evaluation
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let pattern_pos = vec2f(pos.x / min(aspect, 1.0), pos.y);

    // Pattern parameters from audio
    let k = 6.0 + u.bass * 10.0 + u.mid * 4.0;
    let morph = clamp(u.centroid * 1.5, 0.0, 1.0);
    let time_shift = sin(u.time * 0.1) * 0.5;
    let k_eff = k + time_shift;

    // Analytical gradient descent toward pattern features
    let val = turing(pattern_pos, k_eff, morph);
    let grad = turing_gradient(pattern_pos, k_eff, morph);
    let grad_len = length(grad);

    let attract_k = u.attraction_strength;
    if grad_len > 0.01 {
        let attract = normalize(grad) * (1.0 - val) * attract_k * dt;
        vel += attract;
    }

    // Organic diffusion via FBM curl noise
    if u.noise_octaves > 0u {
        let curl = fbm_curl_2d(pos, u.time * u.noise_speed, u.noise_octaves, u.noise_lacunarity, u.noise_persistence);
        vel += curl * u.turbulence * dt;
    } else {
        // Legacy random walk
        let turb_angle = (hash(f32(idx) * 0.37 + u.time * 1.5) - 0.5) * 6.28318;
        let turb_str = 0.015 + u.onset * 0.04;
        vel += vec2f(cos(turb_angle), sin(turb_angle)) * turb_str * dt;
    }

    // Onset: dissolution — scatter particles
    if u.onset > 0.3 {
        let scatter_dir = normalize(pos + vec2f(0.001, 0.001));
        vel += scatter_dir * u.onset * 0.06 * dt;
    }

    // Beat: growth pulse — attract to pattern + settle
    if u.beat > 0.5 {
        if grad_len > 0.01 {
            vel += normalize(grad) * 0.04 * dt;
        }
        vel *= 0.75;
    }

    // Drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    // Integrate position
    var new_pos = pos + vel * dt;

    // Soft boundary repulsion (exponential falloff, NOT hard clamp)
    let edge_margin = 0.85;
    let repulse_k = 5.0;
    if new_pos.x > edge_margin  { vel.x -= (new_pos.x - edge_margin) * repulse_k * dt; }
    if new_pos.x < -edge_margin { vel.x -= (new_pos.x + edge_margin) * repulse_k * dt; }
    if new_pos.y > edge_margin  { vel.y -= (new_pos.y - edge_margin) * repulse_k * dt; }
    if new_pos.y < -edge_margin { vel.y -= (new_pos.y + edge_margin) * repulse_k * dt; }
    // Hard boundary as safety net (wide)
    new_pos = clamp(new_pos, vec2f(-1.1), vec2f(1.1));

    // Size: larger on pattern features, with curve
    let on_feature = clamp(val * 0.5 + 0.5, 0.0, 1.0);
    let init_size = p.pos_life.z;
    let base_size = mix(init_size, u.size_end, life_frac * 0.3);
    let feature_mod = 0.6 + 0.8 * on_feature;
    let size = base_size * feature_mod * eval_size_curve(life_frac) * (1.0 + u.rms * 0.2);

    // Alpha: brighter on pattern features
    let fade_in = smoothstep(0.0, 0.05, life_frac);
    let fade_out = 1.0 - smoothstep(0.8, 1.0, life_frac);
    let alpha = p.color.a * fade_in * fade_out * (0.4 + 0.6 * on_feature) * eval_opacity_curve(life_frac);

    let col = p.color.rgb;

    p.pos_life = vec4f(new_pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    particles_out[idx] = p;
    mark_alive(idx);
}

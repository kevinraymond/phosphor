// Ribbons particle simulation — curl noise flow + trail writing.
// Particles follow flow field at moderate speed, writing positions to trail buffer
// for ribbon rendering. Fewer but longer-lived particles than Flux.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 13.37;

    // Screen emitter: random position
    let pos = rand_vec2(seed_base);

    // Small initial velocity
    let angle = hash(seed_base + 2.0) * 6.2831853;
    let speed = u.initial_speed * (0.5 + 0.5 * hash(seed_base + 3.0));
    let vel = vec2f(cos(angle), sin(angle)) * speed;

    // Color: rich palette varying by position and audio
    let hue = fract(hash(seed_base + 5.0) * 0.5 + 0.3 + u.centroid * 0.3);
    let r_c = abs(hue * 6.0 - 3.0) - 1.0;
    let g_c = 2.0 - abs(hue * 6.0 - 2.0);
    let b_c = 2.0 - abs(hue * 6.0 - 4.0);
    let brightness = 0.06 / (1.0 + u.rms * 1.5);
    let col = clamp(vec3f(r_c, g_c, b_c), vec3f(0.0), vec3f(1.0)) * brightness;

    let initial_age = hash(seed_base + 9.0) * u.lifetime * 0.3;

    p.pos_life = vec4f(pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, u.initial_size * (0.8 + hash(seed_base + 6.0) * 0.4));
    p.color = vec4f(col, 0.06 + hash(seed_base + 7.0) * 0.04);
    p.flags = vec4f(initial_age, u.lifetime, 0.0, 0.0);
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

    // Flow field: primary mover
    let flow_vel = sample_flow_field(p.pos_life.xy);
    let audio_flow = 1.0 + u.bass * 0.6 + u.mid * 0.2;
    vel += flow_vel * audio_flow * dt;

    // Beat: brief speed boost
    if u.beat > 0.5 {
        vel += flow_vel * 0.3;
    }

    // Onset: radial scatter
    if u.onset > 0.3 {
        let dir = normalize(p.pos_life.xy + vec2f(0.001, 0.001));
        vel += dir * u.onset * 0.03 * dt;
    }

    // Drag
    vel *= 1.0 - (1.0 - u.drag) * dt * 60.0;

    // Wrap
    var new_pos = p.pos_life.xy + vel * dt;
    if new_pos.x > 1.1 { new_pos.x -= 2.2; }
    if new_pos.x < -1.1 { new_pos.x += 2.2; }
    if new_pos.y > 1.1 { new_pos.y -= 2.2; }
    if new_pos.y < -1.1 { new_pos.y += 2.2; }

    // Size
    let base_size = mix(p.vel_size.w, u.size_end, life_frac * life_frac);
    let size = base_size * (1.0 + u.rms * 0.2);

    // Alpha
    let fade_in = smoothstep(0.0, 0.05, life_frac);
    let fade_out = 1.0 - smoothstep(0.75, 1.0, life_frac);
    let alpha = p.color.a * fade_in * fade_out;

    // Color shift with age
    var col = p.color.rgb;
    let warm = life_frac * 0.2 + u.mid * 0.08;
    col = clamp(vec3f(col.r + warm * 0.15, col.g, col.b - warm * 0.1), vec3f(0.0), vec3f(1.0));

    p.pos_life = vec4f(new_pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags.x = new_age;

    // Write trail point (position, size, alpha)
    trail_write(idx, vec4f(new_pos, size, alpha));

    particles_out[idx] = p;
    mark_alive(idx);
}

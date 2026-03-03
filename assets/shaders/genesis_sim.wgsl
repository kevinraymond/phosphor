// Genesis particle simulation — Particle Lenia continuous cellular automaton.
//
// Energy-based formulation: E = R(x) - G(U(x))
// Particles minimize energy: dp/dt = -∇E = ∇G(U) - ∇R
//
// Reference: Mordvintsev et al., "Particle Lenia" (Google Research, 2023)
// Key parameters calibrated from znah.net/lenia reference implementation.
//
// Uses spatial hash (group 3) for efficient neighbor queries.
// Structs, bindings, and helpers are in particle_lib.wgsl (auto-prepended).
//
// --- Param mapping ---
// param(0) = trail_decay      (bg shader only)
// param(1) = radius           (interaction radius R, mapped 0.05–0.20)
// param(2) = kernel_peak      (μ_K, ring kernel peak as fraction of R)
// param(3) = kernel_width     (σ_K, ring kernel width — reference uses ~0.79×peak)
// param(4) = target_density   (μ_G, growth function center)
// param(5) = density_width    (σ_G, growth function width)
// param(6) = step_size        (growth force multiplier)
// param(7) = color_mode       (0=density, 0.5=velocity, 1.0=growth)

const CELL_SIZE: f32 = 0.05;
const MAX_NEIGHBORS: u32 = 256u;
const W_K: f32 = 0.02;        // Kernel weight (tuned for 3K particles: ~50 neighbors × avg K ~0.5 × 0.02 = U ≈ 0.5)
const MAX_SPEED: f32 = 0.25;

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let seed_base = u.seed + f32(idx) * 17.31;

    // 9 clusters in a 3×3 grid across the screen
    let cluster = u32(hash(seed_base) * 9.0) % 9u;
    let cx = f32(cluster % 3u) - 1.0;  // -1, 0, 1
    let cy = f32(cluster / 3u) - 1.0;  // -1, 0, 1
    let cluster_center = vec2f(cx, cy) * 0.55;

    let angle = hash(seed_base + 1.0) * 6.2831853;
    let r = sqrt(hash(seed_base + 2.0)) * 0.14;
    let pos = cluster_center + vec2f(cos(angle), sin(angle)) * r;

    // Small random velocity for symmetry breaking
    let vel = vec2f(hash(seed_base + 5.0) - 0.5, hash(seed_base + 6.0) - 0.5) * 0.005;

    let init_size = u.initial_size * (0.8 + hash(seed_base + 4.0) * 0.4);

    p.pos_life = vec4f(pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, init_size);
    p.color = vec4f(0.3, 0.25, 0.1, 0.1); // warm gold base, overwritten in sim
    p.flags = vec4f(0.0, u.lifetime, 0.0, 0.0);
    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles { return; }

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

    let dt = u.delta_time;
    let pos = p.pos_life.xy;
    var vel = p.vel_size.xy;

    // ===================================================================
    // LENIA PARAMS + AUDIO
    // ===================================================================

    let R = 0.05 + param(1u) * 0.15;

    let mu_K = clamp(param(2u) + u.bass * 0.12, 0.05, 0.95);

    // σ_K: reference uses σ/μ ≈ 0.79. Default 0.35 with peak 0.5 → ratio 0.70
    let sigma_K = max(param(3u) + (u.mid - 0.5) * 0.06, 0.03);
    let sigma_K_sq = sigma_K * sigma_K;

    let mu_G = clamp(param(4u) + u.sub_bass * 0.1, 0.05, 0.95);
    let sigma_G = max(param(5u) + (u.centroid - 0.5) * 0.02, 0.02);
    let sigma_G_sq = sigma_G * sigma_G;

    let step = param(6u) * 0.10;

    // ===================================================================
    // NEIGHBOR LOOP
    // ===================================================================

    let cells_r = min(i32(ceil(R / CELL_SIZE)), 5);
    let my_cell = sh_pos_to_cell(pos);

    var U = 0.0;
    var grad_U = vec2f(0.0);
    var repulsion = vec2f(0.0);
    var neighbor_count = 0u;

    // Repulsion at 25% of R (reference ratio: 1.0/4.27 ≈ 0.23)
    let d_rep = 0.25;
    let r_rep = d_rep * R;
    let c_rep = 0.12;

    for (var dy = -cells_r; dy <= cells_r; dy++) {
        for (var dx = -cells_r; dx <= cells_r; dx++) {
            let cx = my_cell.x + dx;
            let cy = my_cell.y + dy;
            let range = sh_cell_range(cx, cy);
            let start = range.x;
            let count = range.y;

            for (var i = 0u; i < count && neighbor_count < MAX_NEIGHBORS; i++) {
                let ni = sh_sorted_indices[start + i];
                if ni == idx { continue; }

                let n_pl = pos_life_in[ni];
                if n_pl.w <= 0.0 { continue; }

                let diff = pos - n_pl.xy;
                let r_ij = length(diff);
                if r_ij >= R || r_ij < 0.0001 { continue; }

                let d_ij = r_ij / R;
                let dir_ij = diff / r_ij; // FROM neighbor TO self

                // Ring kernel: K(d) = w_K · exp(-((d-μ)/σ)²)
                let dd = d_ij - mu_K;
                let K_val = W_K * exp(-(dd * dd) / sigma_K_sq);

                U += K_val;

                // ∇U: K'(d)·dir/R, where K'(d) = K·(-2(d-μ)/σ²)
                let dK_dd = K_val * (-2.0 * dd / sigma_K_sq);
                grad_U += dK_dd * dir_ij / R;

                // Repulsion: c_rep/2 · max(1-d/d_rep, 0)²
                // -∇R pushes AWAY from close neighbors (dir_ij direction)
                if r_ij < r_rep {
                    let overlap = 1.0 - r_ij / r_rep;
                    repulsion += dir_ij * overlap * c_rep;
                }

                neighbor_count++;
            }
        }
    }

    // ===================================================================
    // GROWTH FUNCTION + FORCE
    // ===================================================================

    let dU_val = U - mu_G;
    let G_val = exp(-(dU_val * dU_val) / sigma_G_sq);
    let dG_dU = G_val * (-2.0 * dU_val / sigma_G_sq);

    // Growth force (radial: seeks optimal density)
    // At 3K particles the density field is granular enough for natural instabilities
    // — no curl rotation needed (reference doesn't use it either).
    let growth_force = dG_dU * grad_U * step;

    var force = growth_force + repulsion;

    // --- Audio ---
    if u.onset > 0.3 {
        let jitter_angle = hash(f32(idx) * 5.17 + u.time) * 6.2831853;
        force += vec2f(cos(jitter_angle), sin(jitter_angle)) * u.onset * 0.04;
    }

    let breath = sin(u.beat_phase * 6.2831853) * 0.003;
    if length(pos) > 0.01 {
        force -= normalize(pos) * breath;
    }

    // Perpetual noise for symmetry breaking (paired hashes cancel systematic bias)
    let ns_t = u.time * 5.0;
    let nx = (hash(f32(idx) * 1.37 + ns_t) - hash(f32(idx) * 3.71 + ns_t)) * 0.5;
    let ny = (hash(f32(idx) * 2.91 + ns_t) - hash(f32(idx) * 4.53 + ns_t)) * 0.5;
    force += vec2f(nx, ny) * 0.025;

    // ===================================================================
    // VELOCITY UPDATE
    // ===================================================================

    vel += force * dt;
    vel *= pow(u.drag, dt * 60.0);

    let speed = length(vel);
    if speed > MAX_SPEED {
        vel *= MAX_SPEED / speed;
    }

    // ===================================================================
    // BEAT SEED DROP — teleport a small cluster to a new random location
    // ===================================================================

    // On beat, ~3% of particles get relocated to a fresh seed position
    // Uses floor(time) so the seed location is stable for the whole beat frame
    let seed_hash = hash(f32(idx) * 7.13 + floor(u.time * 2.0));
    if u.beat > 0.5 && seed_hash < 0.03 {
        // Pick a random screen position for the new seed
        let seed_t = floor(u.time * 2.0);
        let sx = (hash(seed_t * 3.17 + 1.0) - 0.5) * 1.6;
        let sy = (hash(seed_t * 5.31 + 2.0) - 0.5) * 1.6;
        let seed_center = vec2f(sx, sy);

        // Scatter around center with small radius
        let sa = hash(f32(idx) * 11.3 + seed_t) * 6.2831853;
        let sr = sqrt(hash(f32(idx) * 13.7 + seed_t)) * 0.12;
        let new_seed_pos = seed_center + vec2f(cos(sa), sin(sa)) * sr;

        // Fresh small velocity
        let sv = vec2f(hash(f32(idx) * 17.1 + seed_t) - 0.5,
                       hash(f32(idx) * 19.3 + seed_t) - 0.5) * 0.01;

        p.pos_life = vec4f(new_seed_pos, p.pos_life.z, 1.0);
        p.vel_size = vec4f(sv, 0.0, p.vel_size.w);
        p.flags = vec4f(0.0, max_life, 0.0, 0.0); // reset age
        write_particle(idx, p);
        mark_alive(idx);
        return;
    }

    // ===================================================================
    // BOUNDARY + POSITION
    // ===================================================================

    // Soft radial containment — gentle spring past screen edge, hard wall off-screen
    let d_center = length(pos);
    let soft_edge = 0.9;
    if d_center > soft_edge {
        let overshoot = d_center - soft_edge;
        // Quadratic spring: gentle near edge, strong far off-screen
        let pull = overshoot * overshoot * 8.0;
        force -= normalize(pos) * pull;
    }

    var new_pos = pos + vel * dt;
    // Hard clamp well outside viewport as safety net
    let max_r = 1.5;
    let new_d = length(new_pos);
    if new_d > max_r {
        new_pos = new_pos / new_d * max_r;
        vel *= 0.5;
    }

    // ===================================================================
    // RENDERING
    // ===================================================================

    let init_size = p.pos_life.z;
    let density_size = 1.0 + clamp(U * 1.5, 0.0, 1.5);
    let size = init_size * density_size * (1.0 + u.rms * 0.15);

    let color_mode = param(7u);
    var col: vec3f;

    // G_val drives organism visibility: high G = "alive" region
    let structure_t = clamp(dG_dU * 0.15 + 0.5, 0.0, 1.0);

    if color_mode < 0.33 {
        // Bioluminescent: dark teal background → bright cyan-green organism
        let density_t = clamp(U / max(mu_G * 1.5, 0.1), 0.0, 1.0);
        let outer = mix(vec3f(0.02, 0.05, 0.08), vec3f(0.06, 0.25, 0.2), density_t);
        let inner = mix(vec3f(0.08, 0.3, 0.25), vec3f(0.15, 0.5, 0.35), structure_t);
        col = mix(outer, inner, G_val);
    } else if color_mode < 0.66 {
        // Velocity: cool → warm
        let speed_t = clamp(length(vel) * 15.0, 0.0, 1.0);
        col = mix(vec3f(0.05, 0.15, 0.35), vec3f(0.5, 0.25, 0.06), speed_t);
        col *= 0.3 + G_val * 0.5;
    } else {
        // Growth derivative: shows internal forces
        col = mix(vec3f(0.3, 0.04, 0.4), vec3f(0.04, 0.4, 0.15), structure_t);
        col *= 0.3 + G_val * 0.5;
    }

    // Alpha for alpha blending: larger particles need stronger alpha
    let alpha = clamp(0.05 + G_val * 0.20, 0.05, 0.25);

    p.pos_life = vec4f(new_pos, init_size, 1.0);
    p.vel_size = vec4f(vel, 0.0, size);
    p.color = vec4f(col, alpha);
    p.flags = vec4f(new_age, max_life, U, G_val);

    write_particle(idx, p);
    mark_alive(idx);
}

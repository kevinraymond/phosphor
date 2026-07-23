// Ascend particle simulation — spectral-brightness altitude field (#1801)
//
// A horizon of particles that rises and falls with the BRIGHTNESS of the
// sound rather than its loudness: a cymbal swell lifts the field skyward, a
// sub-heavy passage sinks it to the floor. Because it tracks timbre, it keeps
// moving through a passage that holds a constant level — where an rms-driven
// band would sit still.
//
// Scoped deliberately as a UTILITY BAND (#1801): something to layer underneath
// another effect to give it a horizon and a sense of register, not a headliner.
// It stays in the lower half of the frame by default and leaves the middle of
// the screen clear for whatever sits on top.
//
// First real consumer of `bandwidth`, the last of the dormant spectral-shape
// features (#1490).
//
// NO TRAIL RING: this sim never calls trail_write, and the .pfx sets
// trail_length 0. Those two must agree — particle_lib declares trail_buffer at
// @group(2) @binding(0) and the system only creates that bind-group layout when
// trail_length >= 2 (system.rs), so a sim that references trail_write while its
// .pfx says 0 fails compute-pipeline validation every frame and renders nothing.
// --- Param mapping ---
// param(0) = altitude      (how far rolloff lifts the field)
// param(1) = thickness     (how far bandwidth spreads it vertically)
// param(2) = shimmer       (zcr-driven horizontal jitter)
// param(3) = flow          (lateral drift speed)
// param(4) = hue           (palette anchor offset)
// param(5) = glow          (brightness gain)
// param(6) = baseline      (resting height of the horizon)
// param(7) = trail_decay   (bg shader: feedback decay)

// Integer hash (lowbias32) — the lib's fract-sin hash() degenerates on GPU for
// idx-scaled arguments (finding #1856), banding a range of indices onto
// near-identical values. In a field this wide that reads as vertical seams of
// clumped particles, so all per-index randomness here uses exact u32 mixing.
fn uhash(x: u32) -> u32 {
    var h = x;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return h;
}

fn uhash_f(x: u32) -> f32 {
    return f32(uhash(x)) / 4294967296.0;
}

// Height of the horizon at a given x, in clip space.
//
// rolloff is the 85%-energy frequency, already normalized 0..1 by the analyzer
// (FixedRange, so it holds its last value on silence instead of collapsing) —
// that is what makes this track timbre rather than level. A slow travelling
// swell keeps the line alive on static material.
fn horizon_y(x: f32) -> f32 {
    let base = -0.85 + param(6u) * 0.7;
    let lift = clamp(u.rolloff, 0.0, 1.0) * param(0u) * 1.15;
    // Two incommensurate waves so the crest never repeats visibly across the
    // width; amplitude leans on presence so the line ripples when there is
    // actually high-frequency content to justify it.
    let swell = sin(x * 2.3 + u.time * 0.35) * 0.045
        + sin(x * 5.7 - u.time * 0.22) * 0.022;
    return base + lift + swell * (0.4 + 1.2 * u.presence);
}

fn ascend_color(height_t: f32, energy: f32) -> vec3f {
    // Hue rises with altitude: the field warms as it sinks and cools as it
    // climbs, so height is readable even in a still frame.
    let hue_t = fract(param(4u) * 0.5 + 0.55 + height_t * 0.30);
    var col = phosphor_audio_palette(hue_t, 0.5 + 0.5 * clamp(u.centroid, 0.0, 1.0), u.time * 0.015);
    col = mix(col, vec3f(0.85, 0.93, 1.0), clamp(height_t, 0.0, 1.0) * 0.30);
    return col * (0.15 + 1.0 * param(5u) * (0.25 + 0.75 * clamp(energy, 0.0, 1.0)));
}

fn emit_particle(idx: u32) -> Particle {
    var p: Particle;
    let h = uhash(idx * 0x9e3779b9u + u32(u.seed * 4096.0));
    let r0 = f32(uhash(h)) / 4294967296.0;
    let r1 = f32(uhash(h + 1u)) / 4294967296.0;
    let r2 = f32(uhash(h + 2u)) / 4294967296.0;
    let r3 = f32(uhash(h + 3u)) / 4294967296.0;

    // Spread across the full width, a little past the edges so the field has
    // no visible ends.
    let x = r0 * 2.4 - 1.2;

    // bandwidth (spectral spread) sets how THICK the band is: a pure tone
    // gathers it into a taut line, broadband noise inflates it into a haze.
    // This is the feature's first real consumer.
    // The floor matters: below ~0.05 the field collapses to a one-pixel line
    // and the additive stack saturates it to a flat white bar regardless of
    // per-particle alpha. A taut line still needs somewhere to put its light.
    let thickness = (0.05 + clamp(u.bandwidth, 0.0, 1.0) * 0.40) * param(1u);
    // Signed power keeps density concentrated at the horizon and thins with
    // distance, so the band has an edge instead of a uniform slab.
    let t = r1 * 2.0 - 1.0;
    let offset = sign(t) * pow(abs(t), 1.7) * thickness;

    let y = horizon_y(x) + offset;

    let vel = vec2f((r2 - 0.5) * 0.05 * param(3u), (r3 - 0.5) * 0.02);
    let init_size = u.initial_size * (0.5 + 0.9 * r2);
    let life = u.lifetime * (0.65 + 0.7 * r3);

    p.pos_life = vec4f(x, y, 0.0, 1.0);
    p.vel_size = vec4f(vel, init_size, init_size);
    p.color = vec4f(ascend_color(0.5, u.rms), 0.0);
    // flags: (age, lifetime, offset-from-horizon, spare)
    p.flags = vec4f(0.0, life, offset, 0.0);
    return p;
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if idx >= u.max_particles { return; }

    var p = read_particle(idx);

    if p.pos_life.w <= 0.0 {
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

    let age = p.flags.x;
    let max_life = p.flags.y;
    let new_age = age + u.delta_time;
    if new_age >= max_life {
        p.pos_life.w = 0.0;
        write_particle(idx, p);
        return;
    }

    let dt = u.delta_time;
    let life_frac = new_age / max_life;
    var pos = p.pos_life.xy;
    var vel = p.vel_size.xy;
    let init_size = p.vel_size.z;
    let offset = p.flags.z;

    // Lateral flow, with zcr shimmering the field sideways. zcr rises on noisy
    // / breathy material, so hiss visibly agitates the surface while a clean
    // tone leaves it gliding.
    let shimmer = clamp(u.zcr, 0.0, 1.0) * param(2u);
    vel.x += (uhash_f(idx * 2654435761u + u32(u.time * 60.0)) - 0.5) * shimmer * 0.9 * dt;
    vel.x += 0.04 * param(3u) * dt;

    // Follow the horizon as it moves. The particle keeps its own offset from
    // the line, so the whole field rises and falls as one surface rather than
    // each particle chasing an absolute height and shearing the band apart.
    let target_y = horizon_y(pos.x) + offset;
    vel.y += (target_y - pos.y) * 5.0 * dt;
    vel *= exp(-dt * 2.2);

    pos += vel * dt;

    if abs(pos.x) > 1.35 {
        p.pos_life.w = 0.0;
        write_particle(idx, p);
        return;
    }

    // Height above the resting floor, for colour. Clamped so a very high
    // horizon does not run off the end of the ramp.
    let height_t = clamp((pos.y + 0.9) / 1.6, 0.0, 1.0);
    let env = smoothstep(0.0, 0.12, life_frac) * (1.0 - smoothstep(0.6, 1.0, life_frac));
    // Thin the field as it climbs: a lifted horizon reads as airy rather than
    // as the same slab moved upward.
    // Low per-particle alpha — see panorama_sim for the same reasoning; the
    // additive stack, not the individual particle, carries the brightness.
    let alpha = env * (0.03 + 0.10 * u.rms) * (1.0 - 0.35 * height_t);

    p.pos_life = vec4f(pos, 0.0, 1.0);
    p.vel_size = vec4f(vel, init_size, init_size * (0.6 + 0.4 * env));
    p.color = vec4f(ascend_color(height_t, u.rms), alpha);
    p.flags = vec4f(new_age, max_life, offset, 0.0);

    write_particle(idx, p);
    mark_alive(idx);
}

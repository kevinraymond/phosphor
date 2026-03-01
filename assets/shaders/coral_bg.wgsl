// Coral background — Gray-Scott reaction-diffusion in the feedback texture.
// Channel encoding:
//   R, G = visual color (coral palette from B concentration)
//   B    = (1 - A) * 0.5 (half-scale complement)
//   Alpha = B concentration (pattern chemical + compositing alpha)
// Recovery: A = 1.0 - feedback.b * 2.0, B = feedback.a

const LAP_CENTER: f32 = -1.0;
const LAP_EDGE:   f32 = 0.2;
const LAP_CORNER: f32 = 0.05;

fn recover_A(sample: vec4f) -> f32 {
    return clamp(1.0 - sample.b * 2.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let uv = frag_coord.xy / u.resolution;
    let px = 1.0 / u.resolution;

    let evolution_speed = 0.5 + param(0u) * 1.5;
    let pattern_scale = param(1u);
    let growth_rate_p = param(2u);
    let warmth = param(3u);

    // === Recover A, B ===
    let center_sample = feedback(uv);
    var A = recover_A(center_sample);
    var B = center_sample.a;

    // === Seeding: full-screen structured noise ===
    // GS self-organizes from noisy initial state — no need for sparse blobs
    let frame = u32(u.frame_index);
    if frame < 3u {
        // Multi-octave noise at different scales for varied pattern nucleation
        let n1 = phosphor_noise2(uv * 25.0 + vec2f(1.7));
        let n2 = phosphor_noise2(uv * 50.0 + vec2f(3.1));
        let seed_pattern = n1 * 0.7 + n2 * 0.3;

        // Threshold to create blob-like seed regions (not uniform noise)
        let seed_B = smoothstep(0.42, 0.58, seed_pattern) * 0.3;
        B = max(B, seed_B);
        A = min(A, 1.0 - seed_B * 0.7);
    }

    // Beat: inject seed blob at random position
    if u.beat > 0.5 {
        let bx = fract(u.time * 0.7137) * 0.7 + 0.15;
        let by = fract(u.time * 0.5113) * 0.7 + 0.15;
        let d = length(uv - vec2f(bx, by));
        if d < 0.06 {
            B = max(B, 0.3 * (1.0 - d / 0.06));
        }
    }

    // === 9-point Laplacian ===
    let s_up    = feedback(uv + vec2f(0.0, px.y));
    let s_down  = feedback(uv - vec2f(0.0, px.y));
    let s_left  = feedback(uv - vec2f(px.x, 0.0));
    let s_right = feedback(uv + vec2f(px.x, 0.0));
    let s_ul = feedback(uv + vec2f(-px.x, px.y));
    let s_ur = feedback(uv + vec2f(px.x, px.y));
    let s_dl = feedback(uv + vec2f(-px.x, -px.y));
    let s_dr = feedback(uv + vec2f(px.x, -px.y));

    var lap_A = A * LAP_CENTER;
    lap_A += recover_A(s_up)    * LAP_EDGE;
    lap_A += recover_A(s_down)  * LAP_EDGE;
    lap_A += recover_A(s_left)  * LAP_EDGE;
    lap_A += recover_A(s_right) * LAP_EDGE;
    lap_A += recover_A(s_ul) * LAP_CORNER;
    lap_A += recover_A(s_ur) * LAP_CORNER;
    lap_A += recover_A(s_dl) * LAP_CORNER;
    lap_A += recover_A(s_dr) * LAP_CORNER;

    var lap_B = B * LAP_CENTER;
    lap_B += s_up.a    * LAP_EDGE;
    lap_B += s_down.a  * LAP_EDGE;
    lap_B += s_left.a  * LAP_EDGE;
    lap_B += s_right.a * LAP_EDGE;
    lap_B += s_ul.a * LAP_CORNER;
    lap_B += s_ur.a * LAP_CORNER;
    lap_B += s_dl.a * LAP_CORNER;
    lap_B += s_dr.a * LAP_CORNER;

    // === Gray-Scott with audio modulation ===
    let f_base = 0.034 + growth_rate_p * 0.016;
    let k_base = 0.057 + pattern_scale * 0.009;

    let f = f_base + u.bass * 0.008 + u.rms * 0.003;
    let k = k_base + u.mid * 0.004 + u.onset * 0.006;

    let Da = 0.2097;
    let Db = 0.105;

    let ABB = A * B * B;
    let dt = evolution_speed;

    var new_A = A + dt * (Da * lap_A - ABB + f * (1.0 - A));
    var new_B = B + dt * (Db * lap_B + ABB - (k + f) * B);

    new_A = clamp(new_A, 0.0, 1.0);
    new_B = clamp(new_B, 0.0, 1.0);

    // === Visual ===
    let b_val = new_B;

    // Edge glow
    let grad_x = (s_right.a - s_left.a) * 0.5;
    let grad_y = (s_up.a - s_down.a) * 0.5;
    let edge = length(vec2f(grad_x, grad_y)) * 12.0;

    // Coral palette (HDR)
    let t = clamp(b_val * 3.5, 0.0, 1.0);
    let deep = vec3f(0.3, 0.06, 0.02);
    let coral_c = vec3f(1.2, 0.35, 0.08);
    let tip = vec3f(1.5, 0.9, 0.4);
    var warm_col = mix(deep, coral_c, smoothstep(0.0, 0.35, t));
    warm_col = mix(warm_col, tip, smoothstep(0.35, 1.0, t));

    let cool_deep = vec3f(0.04, 0.1, 0.18);
    let cool_mid = vec3f(0.15, 0.6, 0.65);
    let cool_tip = vec3f(0.6, 1.0, 0.85);
    var cool_col = mix(cool_deep, cool_mid, smoothstep(0.0, 0.35, t));
    cool_col = mix(cool_col, cool_tip, smoothstep(0.35, 1.0, t));

    let effective_warmth = clamp(warmth + (u.centroid - 0.5) * 0.4, 0.0, 1.0);
    var col = mix(cool_col, warm_col, effective_warmth);

    // Audio-reactive visuals
    col *= 0.6 + u.rms * 1.2;
    col += col * u.bass * 0.6;

    // Beat flash
    let beat_envelope = pow(max(1.0 - u.beat_phase * 3.0, 0.0), 2.0);
    col += vec3f(0.4, 0.25, 0.1) * beat_envelope * b_val * 3.0;

    // Onset edge flash
    col += edge * u.onset * vec3f(1.0, 0.5, 0.2) * 0.8;

    // Edge highlight
    let edge_col = mix(vec3f(1.2, 0.7, 0.3), vec3f(0.5, 0.9, 1.1), 1.0 - effective_warmth);
    col += edge_col * edge * 0.4;

    // Mask
    let mask = smoothstep(0.01, 0.05, b_val);
    col *= mask;

    // Vignette
    let ctr = uv - 0.5;
    let vig = max(1.0 - dot(ctr, ctr) * 1.2, 0.0);
    col *= vig;

    col = min(col, vec3f(2.5));

    return vec4f(col.r, col.g, (1.0 - new_A) * 0.5, new_B);
}

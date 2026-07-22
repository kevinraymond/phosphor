// Splat depth sort — pass 3: scatter (Splat #1800).
//
// Each alive splat recomputes its 16-bit depth key (identical to the histogram
// pass) and atomic-bumps its bucket's running offset (seeded from the scanned
// bucket offsets) to claim a slot in the sorted index buffer. Within-bucket
// order is nondeterministic, but a bucket spans <50µunits of depth so src-over
// commutes visually. Dispatch ceil(max_particles / 256).

struct SplatSortUniforms {
    depth_near: f32,
    depth_far: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var<storage, read> pos_life: array<vec4f>;
@group(0) @binding(1) var<storage, read> alive_indices: array<u32>;
@group(0) @binding(2) var<storage, read> counters: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> u: SplatSortUniforms;
@group(0) @binding(4) var<storage, read_write> scatter_offsets: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> sorted_indices: array<u32>;

fn depth_key(z: f32) -> u32 {
    let t = clamp((z - u.depth_near) / max(u.depth_far - u.depth_near, 1e-4), 0.0, 1.0);
    return u32((1.0 - t) * 65535.0 + 0.5);
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let thread_idx = gid.x;
    let alive_count = atomicLoad(&counters[0]);
    if thread_idx >= alive_count {
        return;
    }
    let idx = alive_indices[thread_idx];
    let slot = atomicAdd(&scatter_offsets[depth_key(pos_life[idx].z)], 1u);
    sorted_indices[slot] = idx;
}

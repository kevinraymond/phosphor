// Splat depth sort — pass 1: 16-bit histogram (Splat #1800).
//
// Each alive splat quantizes its view depth (pos_life.z) to a far→near 16-bit
// bucket and atomicAdds into the 65536-entry histogram. The bucket scan
// (spatial_hash_prefix_sum.wgsl, patched to 65536) + scatter then produce a
// back-to-front index order. Dispatch ceil(max_particles / 256).

struct SplatSortUniforms {
    depth_near: f32, // view depth mapped to the NEAR end (key 65535, drawn last)
    depth_far: f32,  // view depth mapped to the FAR end  (key 0, drawn first)
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var<storage, read> pos_life: array<vec4f>;
@group(0) @binding(1) var<storage, read> alive_indices: array<u32>;
@group(0) @binding(2) var<storage, read> counters: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> u: SplatSortUniforms;
@group(0) @binding(4) var<storage, read_write> histogram: array<atomic<u32>>;

// far → 0 (blended first = back), near → 65535 (blended last = front).
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
    atomicAdd(&histogram[depth_key(pos_life[idx].z)], 1u);
}

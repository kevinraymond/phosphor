// Prepare indirect draw/dispatch arguments from particle counters.
// Single-thread dispatch: reads alive_count, writes DrawIndirectArgs.

// counters: [0]=alive_count, [1]=dead_count, [2]=emit_used, [3]=reserved
@group(0) @binding(0) var<storage, read> counters: array<u32, 4>;
// indirect_args: DrawIndirectArgs = [vertex_count, instance_count, first_vertex, first_instance]
@group(0) @binding(1) var<storage, read_write> indirect_args: array<u32, 4>;

@compute @workgroup_size(1)
fn cs_main() {
    let alive_count = counters[0];
    indirect_args[0] = 6u;           // vertex_count (6 vertices per quad)
    indirect_args[1] = alive_count;  // instance_count (only alive particles)
    indirect_args[2] = 0u;           // first_vertex
    indirect_args[3] = 0u;           // first_instance
}

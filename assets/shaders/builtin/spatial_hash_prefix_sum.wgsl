// Spatial hash pass 2: Exclusive prefix sum over cell_counts → cell_offsets.
// Simple sequential scan since NUM_CELLS=1600 fits in one workgroup.
// cell_offsets[i] = sum of cell_counts[0..i-1]

const NUM_CELLS: u32 = 1600u; // 40 * 40

@group(0) @binding(0) var<storage, read> cell_counts: array<u32>;
@group(0) @binding(1) var<storage, read_write> cell_offsets: array<u32>;

// Single-thread sequential prefix sum (1600 cells is tiny).
// More complex parallel Blelloch scan not needed at this scale.
@compute @workgroup_size(1)
fn cs_main() {
    var running_sum = 0u;
    for (var i = 0u; i < NUM_CELLS; i++) {
        cell_offsets[i] = running_sum;
        running_sum += cell_counts[i];
    }
}

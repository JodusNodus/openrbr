//! Replays the original's debug-graph pushes (`harness/vector_specs/g_graph.py`, direct calls) through
//! `rbr_physics::graph`. Needs record files: `RBR_GRAPH_VECTORS=<file>[:<file>...]`. Without the variable the tests are
//! skipped.

mod common;

use common::{check_windows, load, map_windows};
use rbr_physics::graph::layout;

const VAR: &str = "RBR_GRAPH_VECTORS";

/// `CGraph1D::Push` (0x494cf0) and `CGraph2D::Push` (0x495090): the counter, the sample, the cursor and its wrap, and
/// eax (the quotient, or the ring after a sample).
#[test]
fn graph_push_matches_original_calls() {
    for (addr, name) in [(0x494cf0, "CGraph1D::Push"), (0x495090, "CGraph2D::Push")] {
        let Some(calls) = load(VAR, addr) else {
            eprintln!("skipping {name}: {VAR} not set");
            return;
        };
        assert!(!calls.is_empty(), "{name}: no calls in the vector files");
        let mut samples = 0;
        for call in &calls {
            let mut mem = map_windows(call);
            let graph = call.args[0];
            let eax = if addr == 0x494cf0 {
                layout::push_1d(&mut mem, graph, call.args[1])
            } else {
                layout::push_2d(&mut mem, graph, call.args[1], call.args[2])
            };
            assert_eq!(eax, Some(call.eax), "{name} call {}: eax", call.index);
            check_windows(call, &mem, name);
            samples += usize::from(call.eax == mem.u32(graph + 0x18));
        }
        eprintln!("{name}: {} calls match, {samples} with a sample", calls.len());
    }
}

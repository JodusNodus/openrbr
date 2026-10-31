//! Replays the original's `E01Log` calls (`harness/direct_calls.py --set g-log`, in the physics core's store) through
//! `rbr_physics::game_log`: the sink, the text in the buffer, the debugger sink's `operator new` and `delete`, and eax.

mod common;

use rbr_physics::game_log::{self, LogHost};
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_PHYSICS_CORE_VECTORS";
const E01_LOG: u32 = 0x43c8e0;
const OPERATOR_NEW: u32 = 0x70bec2;
const OPERATOR_DELETE: u32 = 0x70c260;
/// Where the test puts the variadic words (the records hold them as arguments).
const ARGS: u32 = 0x7f00_0000;

/// The recorded `operator new` blocks and `delete` results, and the calls the port makes.
struct Calls {
    returns: Vec<u32>,
    calls: Vec<(u32, Vec<u32>)>,
}

impl LogHost for Calls {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        self.calls.push((OPERATOR_NEW, vec![size]));
        let block = self.returns.remove(0);
        mem.map(block, &vec![0; size as usize]);
        block
    }

    fn free(&mut self, _: &mut Mem, at: u32) -> u32 {
        self.calls.push((OPERATOR_DELETE, vec![at]));
        self.returns.remove(0)
    }
}

#[test]
fn e01_log_matches() {
    common::replay(VAR, E01_LOG, "E01Log", |call, mem| {
        let words: Vec<u8> = call.args[3..].iter().flat_map(|w| w.to_le_bytes()).collect();
        mem.map(ARGS, &words);
        let mut host = Calls { returns: call.inner.iter().map(|i| i.eax).collect(), calls: Vec::new() };
        let eax = game_log::log(mem, &mut host, call.args[0], call.args[1], call.args[2], ARGS)
            .unwrap_or_else(|e| panic!("E01Log call {}: {e:?}", call.index));
        let expected: Vec<_> = call.inner.iter().map(|i| (i.addr, i.args.clone())).collect();
        assert_eq!(host.calls, expected, "E01Log call {}: calls", call.index);
        assert_eq!(eax, call.eax, "E01Log call {}: eax", call.index);
    });
}

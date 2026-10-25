//! NGP 7.4.778's stage state: its stage init (`0x1004e360`, after the per-car init as the race starts) and its copies
//! of the collision system, supplied directly to the standalone's typed runtime settings.

use crate::mem::Mem;
use crate::stage::layout::{collision_system_ctor, COLLISION_SYSTEM, SYSTEM_SIZE};
use rbr_formats::ngp::Tables;

/// `0x1004e360` on the physics definition `def`: gravity, `MaxTimeStep` 1/720 s and the material table
/// (read from the DLL), and the timestep's copy in the replay object (`*0x893634`, `+0x18b908`) when it has
/// one. Outside the replay mode only (the game object's `+0x10` is 1 in it, and the step is then the replay's).
pub fn stage_init(mem: &mut Mem, def: u32, tables: &Tables) {
    let words = &tables.stage[usize::from(!REVISIONS.contains(&mem.u32(def + 0x1607c)))];
    for &(offset, word) in words {
        mem.set_u32(def + offset, word);
    }
    let replay = mem.u32(REPLAY);
    if replay != 0 && mem.try_read(replay + REPLAY_STEP, 4).is_some() {
        mem.set_u32(replay + REPLAY_STEP, mem.u32(def + MAX_TIME_STEP));
    }
}

/// The `PhysicsRevision_NGP` values that take the first table. 7.4.778 takes it for its own 778 only; RSF's
/// 7.5.779 with its 779 drives as the first does, and its `physics.rbz` differs from 778's only in that line, so
/// a 779 archive under 778's code drives as RSF does (rsf-input1, an RSF drive on that archive, exact only so).
const REVISIONS: [u32; 2] = [778, 779];

const REPLAY: u32 = 0x0089_3634;
const REPLAY_STEP: u32 = 0x18_b908;
const MAX_TIME_STEP: u32 = 0x1_608c;

/// NGP's three copies of the stage's collision system at `copies` (each [`SYSTEM_SIZE`] bytes): each constructed
/// with unlinked connections owned by the copy, then the system's words up to its connections (`+4..+0x1814`).
pub fn collision_copies(mem: &mut Mem, copies: [u32; 3]) {
    let system = mem.u32(COLLISION_SYSTEM);
    let words = mem.read(system + 4, 0x1810);
    for copy in copies {
        mem.map(copy, &vec![0; SYSTEM_SIZE as usize]);
        collision_system_ctor(mem, copy);
        mem.map(copy + 4, &words);
    }
}

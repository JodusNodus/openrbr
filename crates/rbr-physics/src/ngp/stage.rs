//! NGP 7.4.778's stage state: its stage init (`0x1004e360`, after the per-car init as the race starts) and its copies
//! of the collision system, which NGP's exe patches point the physics' reads of `g_7c755c` at.

use super::stage_materials::{KIND, WORDS_30A, WORDS_OTHER};
use crate::mem::Mem;
use crate::stage::layout::{collision_system_ctor, COLLISION_SYSTEM, SYSTEM_SIZE};

/// `0x1004e360` on the physics definition `def`: gravity, `MaxTimeStep` 1/720 s and the material table
/// ([`super::stage_materials`]), and the timestep's copy in the replay object (`*0x893634`, `+0x18b908`) when it has
/// one. Outside the replay mode only (the game object's `+0x10` is 1 in it, and the step is then the replay's).
pub fn stage_init(mem: &mut Mem, def: u32) {
    let words: &[(u32, &[u32])] = if mem.u32(def + KIND) == 0x30a { &WORDS_30A } else { &WORDS_OTHER };
    for &(at, run) in words {
        for (k, &word) in run.iter().enumerate() {
            mem.set_u32(def + at + 0x10 * k as u32, word);
        }
    }
    let replay = mem.u32(REPLAY);
    if replay != 0 && mem.try_read(replay + REPLAY_STEP, 4).is_some() {
        mem.set_u32(replay + REPLAY_STEP, mem.u32(def + MAX_TIME_STEP));
    }
}

const REPLAY: u32 = 0x0089_3634;
const REPLAY_STEP: u32 = 0x18_b908;
const MAX_TIME_STEP: u32 = 0x1_608c;

/// The variable NGP's patches point the reads of `g_7c755c` at (on the heap in the reference runs; the boot places it
/// at the same address, `super::exe_patches`): three collision systems, one per group of readers.
pub const COLLISION_VARIABLE: u32 = 0x01a8_f228;

/// NGP's three copies of the stage's collision system at `copies` (each [`SYSTEM_SIZE`] bytes), and the variable
/// pointing at them: each constructed (unlinked connections, owned by the copy), then the system's words up to its
/// connections (`+4..+0x1814`).
pub fn collision_copies(mem: &mut Mem, copies: [u32; 3]) {
    let system = mem.u32(COLLISION_SYSTEM);
    let words = mem.read(system + 4, 0x1810);
    for (k, &copy) in copies.iter().enumerate() {
        mem.map(copy, &vec![0; SYSTEM_SIZE as usize]);
        collision_system_ctor(mem, copy);
        mem.map(copy + 4, &words);
        mem.set_u32(COLLISION_VARIABLE + 4 * k as u32, copy);
    }
}

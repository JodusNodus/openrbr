//! The E36 light system's pieces: the light block (`E36LightBlock`, exe 0x4f19b0), its sub-block
//! (`NTweak::CE36SubBlock`, 0x4f3e80), the CDef's static table (`CCarLightSystem::CDef::Vf20x`, 0x4f17e0), and the
//! per-light emit (`EmitLights` 0x4f20c0 and `Update` 0x4f2150). The constructors are not ported yet.

use crate::car_body::store::CarStore;
use crate::physics_core::layout::SIGNAL_MANAGER;

/// `CE36SubBlock::Init()` (0x4f3e80): zeroes the sub-block's two words (+0 and +4) and the float at +8.
pub fn sub_block_init<S: CarStore>(mem: &mut S, at: u32) {
    mem.set_u32(at, 0);
    mem.set_u32(at + 4, 0);
    mem.set_f32(at + 8, 0.0);
}

/// The static Def table `CCarLightSystem::CDef::Vf20x` returns (0x8ef7b4).
const DEF_TABLE: u32 = 0x008E_F7B4;

/// `CCarLightSystem::CDef::Vf20x()` (0x4f17e0): the address of the static Def table.
pub fn def_table() -> u32 {
    DEF_TABLE
}

/// The words `E36LightBlock::Reset` sets to 1 in each group, relative to the group's base (`this + 0x20`, which the
/// next group moves 8 bytes on). The asm stores them in this order.
const RESET_WORDS: [i32; 12] = [-4, 0, 0x1C, 0x20, 0x3C, 0x40, 0x5C, 0x60, 0x7C, 0x80, 0x9C, 0xA0];
const RESET_GROUPS: u32 = 4;
const RESET_BASE: u32 = 0x20;
const RESET_FLAG: u32 = 0xDC;

/// `E36LightBlock::Reset()` (0x4f19b0): sets the four groups of words to 1, then the flag word (+0xdc) to 1.
pub fn block_reset<S: CarStore>(mem: &mut S, block: u32) {
    for group in 0..RESET_GROUPS {
        let base = block + RESET_BASE + 8 * group;
        for offset in RESET_WORDS {
            mem.set_u32(base.wrapping_add_signed(offset), 1);
        }
    }
    mem.set_u32(block + RESET_FLAG, 1);
}

/// The light emitter's offset in the signal manager (`g_signalManager`).
const LIGHT_EMITTER: u32 = 0x9AF0;

/// The calls the light emit makes: the emitter's `Emit` (0x4f2000, `CMultiSignal1<const CSigCarLight&>`), answered from the
/// records. The payload is the five words the asm stores on the stack: {1, i, s2, s3, kind}.
pub trait LightCalls {
    /// `Emit` on the light emitter, with the payload words.
    fn emit_light(&mut self, emitter: u32, payload: [u32; 5]);
}

/// `CCarLightSystem::EmitLights(kind, lights, flags, scaled)` (0x4f20c0, `ret 0x10`, thiscall; `this` is not read): for each of
/// the four lights whose head (`lights + 0xc + 0x10 * i`) is set, emits the light `i` with its state: `s2` is 1 when `scaled`
/// is nonzero and the light's first flag word (`flags + 8 * i`) is set, else 2; `s3` is 2 when the second flag word is zero,
/// else 1.
pub fn emit_lights<S: CarStore>(calls: &mut impl LightCalls, mem: &S, kind: u32, lights: u32, flags: u32, scaled: u32) {
    for i in 0..4u32 {
        if mem.u32(lights + 0xC + 0x10 * i) == 0 {
            continue;
        }
        let on = scaled != 0 && mem.u32(flags + 8 * i) != 0;
        let s2 = if on { 1 } else { 2 };
        let s3 = u32::from(mem.u32(flags + 4 + 8 * i) == 0) + 1;
        let emitter = mem.u32(SIGNAL_MANAGER) + LIGHT_EMITTER;
        calls.emit_light(emitter, [1, i, s2, s3, kind]);
    }
}

/// One of `Update`'s six gates: the flag word that enables it (and that it clears), the emit's kind, the offsets of the
/// lights (`this + 4`), the flag block (`flags`) and the def's scale word (`this + 8`).
struct Gate {
    word: u32,
    kind: u32,
    lights: u32,
    block: u32,
    def: u32,
}

const GATES: [Gate; 6] = [
    Gate { word: 0x0C, kind: 4, lights: 0x13C, block: 0x7C, def: 0x5C },
    Gate { word: 0x04, kind: 1, lights: 0x07C, block: 0x1C, def: 0x2C },
    Gate { word: 0x08, kind: 2, lights: 0x0BC, block: 0x3C, def: 0x3C },
    Gate { word: 0x18, kind: 3, lights: 0x0FC, block: 0x5C, def: 0x4C },
    Gate { word: 0x10, kind: 5, lights: 0x17C, block: 0x9C, def: 0x6C },
    Gate { word: 0x14, kind: 6, lights: 0x1BC, block: 0xBC, def: 0x7C },
];

/// `CCarLightSystem::Update()` (0x4f2150, `ret 0`): for each gate, when its flag word or the first flag word (`flags + 0`) is
/// set, emits its lights with the scale `def word * mult` (`mult` is the flag block's +0xdc, read once), and clears the
/// gate's word; the first flag word is cleared at the end. The flag block is `this + 0x14`, read again for each gate.
pub fn update<S: CarStore>(calls: &mut impl LightCalls, mem: &mut S, this: u32) {
    let mult = mem.u32(mem.u32(this + 0x14) + 0xDC);
    for gate in GATES {
        let flags = mem.u32(this + 0x14);
        if mem.u32(flags + gate.word) != 0 || mem.u32(flags) != 0 {
            let scale = mem.u32(mem.u32(this + 8) + gate.def).wrapping_mul(mult);
            let lights = mem.u32(this + 4) + gate.lights;
            emit_lights(calls, mem, gate.kind, lights, flags + gate.block, scale);
            let flags = mem.u32(this + 0x14);
            mem.set_u32(flags + gate.word, 0);
        }
    }
    let flags = mem.u32(this + 0x14);
    mem.set_u32(flags, 0);
}

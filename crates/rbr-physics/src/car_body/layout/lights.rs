//! The light system's memory: the light system's pointers, its flag block and the six gates that map a flag word to
//! the lights it drives.

/// The light system's lights array (+4), def (+8) and flag block (+0x14).
pub fn lights_at(system: u32) -> u32 {
    system + 4
}

pub fn def_at(system: u32) -> u32 {
    system + 8
}

pub fn flags_at(system: u32) -> u32 {
    system + 0x14
}

/// The flag block's first word: set to light every gate.
pub const FLAGS_ALL: u32 = 0;
/// The flag block's multiplier word, which the scale of every gate takes.
pub const FLAGS_MULTIPLIER: u32 = 0xDC;

/// Whether light `i` of the array at `lights` is present: its head word (+0xc, stride 0x10).
pub fn light_head_at(lights: u32, i: u32) -> u32 {
    lights + 0xC + 0x10 * i
}

/// Light `i`'s two state words in the states block (8 bytes each).
pub fn state_first_at(states: u32, i: u32) -> u32 {
    states + 8 * i
}

pub fn state_second_at(states: u32, i: u32) -> u32 {
    states + 4 + 8 * i
}

/// The static table the light system's def returns.
pub const DEF_TABLE: u32 = 0x008E_F7B4;

/// The light emitter's offset in the signal manager.
pub const LIGHT_EMITTER_AT: u32 = 0x9AF0;

/// The words the reset sets to 1 in each group, relative to the group's base (the block + 0x20, which the next group
/// moves 8 bytes on), in the original's order; then the flag word.
pub const RESET_WORDS: [i32; 12] = [-4, 0, 0x1C, 0x20, 0x3C, 0x40, 0x5C, 0x60, 0x7C, 0x80, 0x9C, 0xA0];
pub const RESET_GROUPS: u32 = 4;
pub const RESET_BASE: u32 = 0x20;
pub const RESET_FLAG: u32 = 0xDC;

/// The sub-block's words: two integers and a float.
pub const SUB_BLOCK_FIRST: u32 = 0;
pub const SUB_BLOCK_SECOND: u32 = 4;
pub const SUB_BLOCK_WEIGHT: u32 = 8;

/// One of the update's six gates: the flag word that enables it (and that the update clears), the emit's kind, and the
/// offsets of its lights in the lights array, of its states in the flag block and of its scale word in the def.
pub struct Gate {
    pub flag: u32,
    pub kind: u32,
    pub lights: u32,
    pub states: u32,
    pub def: u32,
}

/// Kind 4 is the brake light and kind 5 the neutral light (`set_brake_light`, `set_neutral_light`).
pub const GATES: [Gate; 6] = [
    Gate { flag: 0x0C, kind: 4, lights: 0x13C, states: 0x7C, def: 0x5C },
    Gate { flag: 0x04, kind: 1, lights: 0x07C, states: 0x1C, def: 0x2C },
    Gate { flag: 0x08, kind: 2, lights: 0x0BC, states: 0x3C, def: 0x3C },
    Gate { flag: 0x18, kind: 3, lights: 0x0FC, states: 0x5C, def: 0x4C },
    Gate { flag: 0x10, kind: 5, lights: 0x17C, states: 0x9C, def: 0x6C },
    Gate { flag: 0x14, kind: 6, lights: 0x1BC, states: 0xBC, def: 0x7C },
];

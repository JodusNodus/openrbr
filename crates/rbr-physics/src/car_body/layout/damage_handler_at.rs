//! Where the damage handler keeps its members (offsets from the handler, which is the car's damage member), and the
//! original's functions `reset_damage` calls on them.

pub const RESET_COUNT_AT: u32 = 0x3A10;
pub const SET_LEVEL_COUNT_AT: u32 = 0x3A14;
pub const LEVEL_AT: u32 = 0x3DF0;
pub const LEVEL_THRESHOLD_AT: u32 = 0x3DEC;

/// The windshield gate: the enable word, the level float and the four counted words.
pub const WINDSHIELD_ENABLE_AT: u32 = 0x3B20;
pub const WINDSHIELD_LEVEL_AT: u32 = 0x3AA0;
pub const COUNTED_WORDS_AT: [u32; 4] = [0x3A30, 0x3A3C, 0x3A48, 0x3A54];

/// The zones (64, three words cleared each, `ZONE_STRIDE` apart from `ZONES_AT`) and their floats (64, from `FLOATS_AT`).
pub const ZONES: u32 = 0x40;
pub const ZONES_AT: u32 = 0x2214;
pub const ZONE_STRIDE: u32 = 0x58;
pub const FLOATS_AT: u32 = 0x3808;
/// The tail: 0xfa words from `TAIL_AT` (the level, the threshold and the counters are in it).
pub const TAIL_AT: u32 = 0x3A0C;
pub const TAIL_WORDS: u32 = 0xFA;

/// Members `reset_damage` resets by call: the function, the first member's offset and the stride between the members.
pub struct Member {
    pub function: u32,
    pub at: u32,
    pub stride: u32,
    pub count: u32,
}

pub const STATE_INIT_C70: u32 = 0x502C70;
pub const ENGINE_RESET_SUB78: u32 = 0x50C370;
pub const WHEEL_ZERO: u32 = 0x53B050;
pub const VALUE8_CLEAR: u32 = 0x5317F0;
pub const VALUE_CLEAR: u32 = 0x52D960;
pub const FILL_ONES: u32 = 0x51CD60;
pub const M1640: u32 = 0x521640;

pub const STATE_AT: u32 = 0x3A18;
pub const ENGINE_AT: u32 = 0x3AAC;
pub const LIGHT_BLOCK_AT: u32 = 0x3B24;
/// The four-wheel groups: the per-wheel function, first offset and stride, in the order the original calls them.
pub const WHEEL_GROUP: [Member; 3] = [
    Member { function: WHEEL_ZERO, at: 0x3C04, stride: 0x38, count: 4 },
    Member { function: VALUE8_CLEAR, at: 0x3D04, stride: 0x20, count: 4 },
    Member { function: VALUE_CLEAR, at: 0x3CE4, stride: 8, count: 4 },
];
pub const ONES_GROUP: Member = Member { function: FILL_ONES, at: 0x3D84, stride: 0xC, count: 4 };
pub const STATE_GROUP: Member = Member { function: M1640, at: 0x3DB4, stride: 4, count: 4 };

/// The flag words set to 1 (inclusive range, stride 4), the words and floats zeroed, and the ten (29, 0) pairs.
pub const FLAGS_FIRST_AT: u32 = 0x3DC4;
pub const FLAGS_LAST_AT: u32 = 0x3DE8;
pub const ZEROED_FLOATS_AT: [u32; 2] = [0x3E00, 0x3DFC];
pub const ZEROED_WORDS_AT: [u32; 4] = [0x3DF4, 0x3DF8, 0x3E04, 0x3E58];
pub const PAIRS_AT: u32 = 0x3E08;
pub const PAIRS: u32 = 10;
pub const PAIR_INITIAL: u32 = 0x1D;

/// The material lookup: the inner material id word, the world manager's offset in the world, its material table and its
/// flag word with the flag that enables the lookup.
pub const MATERIAL_ID_AT: u32 = 0x3E04;
pub const WORLD_MANAGER_AT: u32 = 0x24;
pub const MATERIAL_TABLE_AT: u32 = 0x18;
pub const MANAGER_FLAGS_AT: u32 = 0x160E0;
pub const MANAGER_MATERIAL_NAMES: u32 = 0x0100_0000;

/// The functor table: the count word, the entry size and slots, and the function each index of the original's switch
/// stores (0x4ef720's jump table); `None` where the switch leaves an index out.
pub const FUNCTOR_COUNT: u32 = 0x80;
pub const FUNCTOR_ENTRY: u32 = 0x20;
pub const FUNCTOR_SLOTS: u32 = 4;
pub const FUNCTORS: [Option<u32>; 29] = [
    Some(0x4EC660),
    Some(0x4ED020),
    Some(0x4EF5F0),
    Some(0x4EC720),
    Some(0x4EC7B0),
    Some(0x4ECFA0),
    Some(0x4ECFE0),
    Some(0x4ECAA0),
    Some(0x4ECB20),
    Some(0x4ED2F0),
    Some(0x4ECA00),
    Some(0x4EC820),
    Some(0x4ED300),
    Some(0x4EC9A0),
    Some(0x4EC890),
    Some(0x4EC930),
    Some(0x4ECEB0),
    Some(0x4ECF20),
    None,
    Some(0x4ECBB0),
    Some(0x4ECC30),
    Some(0x4ECD30),
    Some(0x4ECDB0),
    None,
    Some(0x4ECE30),
    Some(0x4ED160),
    Some(0x4ED0F0),
    Some(0x4ED200),
    Some(0x4ECCB0),
];

/// Where a functor entry keeps its zeroed words and its three arguments.
pub const FUNCTOR_ZEROED_AT: [u32; 3] = [4, 8, 0xC];
pub const FUNCTOR_B_AT: u32 = 0x10;
pub const FUNCTOR_A_AT: u32 = 0x14;
pub const FUNCTOR_C_AT: u32 = 0x18;

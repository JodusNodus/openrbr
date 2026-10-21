//! The drive and engine constructors (definitions, setups, options, states) and their resets.

use super::drive_calls::clear_lanes;
use super::worker::{CLUTCH_MATRIX_AT, STEP_MATRIX_AT, STEP_VEC_AT};
use crate::car_body::layout::FLOW_OBJECT;
use crate::math::ftol::ftol2;
use crate::mem::Mem;
use crate::ntweak::layout::NGP_TABLE_VTABLE;
use crate::ntweak::{construct_base, set_dirty};
use crate::physics_core::construct::Heap;
use crate::rigid_body::host::HostUnavailable;
use crate::{ntweak, physics_core, rigid_body, spring_damper};

/// A tweak a definition's member constructor builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tweak {
    Real,
    Unsigned,
    Integer,
}

fn construct_tweak(mem: &mut Mem, at: u32, tweak: Tweak) {
    match tweak {
        Tweak::Real => ntweak::construct_real(mem, at),
        Tweak::Unsigned => ntweak::construct_unsigned(mem, at),
        Tweak::Integer => ntweak::construct_integer(mem, at),
    }
}

/// Builds the tweaks of `members` (offset, kind) from `at`, in order.
fn construct_members(mem: &mut Mem, at: u32, members: &[(u32, Tweak)]) {
    for &(offset, tweak) in members {
        construct_tweak(mem, at + offset, tweak);
    }
}

/// The drive definition's compound members, by their constructors: `E39_CDefCtorA` (0x502780, a differential),
/// `E39_Hook5140` (0x505140, a wheel's brake; its nine reals at +0x30 are `0x47bf40`'s), `E39_Ctor870` (0x502870) and
/// `E13Mixed50` (0x47be00).
const DIFF: [(u32, Tweak); 6] = [
    (0x00, Tweak::Integer),
    (0x10, Tweak::Unsigned),
    (0x20, Tweak::Unsigned),
    (0x30, Tweak::Real),
    (0x40, Tweak::Real),
    (0x50, Tweak::Unsigned),
];
const BRAKE: [(u32, Tweak); 15] = [
    (0x00, Tweak::Unsigned),
    (0x10, Tweak::Real),
    (0x20, Tweak::Unsigned),
    (0x30, Tweak::Real),
    (0x40, Tweak::Real),
    (0x50, Tweak::Real),
    (0x60, Tweak::Real),
    (0x70, Tweak::Real),
    (0x80, Tweak::Real),
    (0x90, Tweak::Real),
    (0xA0, Tweak::Real),
    (0xB0, Tweak::Real),
    (0xC0, Tweak::Real),
    (0xD0, Tweak::Unsigned),
    (0xE0, Tweak::Real),
];
const CTOR870: [(u32, Tweak); 11] = [
    (0x00, Tweak::Unsigned),
    (0x10, Tweak::Unsigned),
    (0x20, Tweak::Unsigned),
    (0x30, Tweak::Unsigned),
    (0x40, Tweak::Real),
    (0x50, Tweak::Real),
    (0x60, Tweak::Real),
    (0x70, Tweak::Real),
    (0x80, Tweak::Real),
    (0x90, Tweak::Real),
    (0xA0, Tweak::Unsigned),
];
const MIXED50: [(u32, Tweak); 5] =
    [(0x00, Tweak::Real), (0x10, Tweak::Integer), (0x20, Tweak::Integer), (0x30, Tweak::Real), (0x40, Tweak::Real)];
/// A compound's tweaks: (offset, kind).
type Members = &'static [(u32, Tweak)];
/// The arrays of compounds: (offset, stride, count, members).
const DRIVE_ARRAYS: [(u32, u32, u32, Members); 6] = [
    (0xC, 0x60, 3, &DIFF),
    (0x12C, 0xF0, 4, &BRAKE),
    (0x4EC, 0xB0, 1, &CTOR870),
    (0x688, 0x50, 10, &MIXED50),
    (0x9A8, 0x50, 1, &MIXED50),
    (0x9F8, 0x50, 1, &MIXED50),
];
/// The plain tweaks after the compounds at +0x4ec and at +0xa48.
const DRIVE_TWEAKS: [(u32, Tweak); 11] = [
    (0x614, Tweak::Real),
    (0x624, Tweak::Real),
    (0x634, Tweak::Integer),
    (0x648, Tweak::Unsigned),
    (0x658, Tweak::Real),
    (0x668, Tweak::Real),
    (0x678, Tweak::Real),
    (0xA48, Tweak::Real),
    (0xA58, Tweak::Real),
    (0xA68, Tweak::Real),
    (0xA78, Tweak::Real),
];
/// Each brake's defaults (offset in the brake, value word), set in this order: 0.1, 0.95 and 400.0 (the floats at
/// 0x72b068, 0x7355d4 and 0x731e00), and the unsigned 0.
const BRAKE_DEFAULTS: [(u32, u32); 4] = [(0x90, 0x3DCC_CCCD), (0x80, 0x3F73_3333), (0xB0, 0x43C8_0000), (0xD0, 0)];
/// Each of the ten `E13Mixed50` at +0x688: 10000.0 and -10000.0 (0x72f420, 0x72f41c).
const MIXED50_DEFAULTS: [(u32, u32); 2] = [(0x40, 0x461C_4000), (0x30, 0xC61C_4000)];
/// The tweaks the constructor sets next (dirty, then the value word), in its order. The floats come from the exe's
/// constants (0x730888, 0x73a2dc, 0x72f440, 0x72b068, 0x730890, 0x73a2d8, 0x795e20, 0x795e14, 0x73a2d4, 0x73a2d0,
/// 0x731fe0, 0x7304a0, 0x73a2cc, 0x72dd78, 0x72fcb4).
const DRIVE_DEFAULTS: [(u32, u32); 62] = [
    (0x4EC, 0x0D4C_FFA9),
    (0x50C, 0x2E15_5529),
    (0x4FC, 0x3FF6_F829),
    (0x54C, 0x3E80_0000),
    (0x55C, 0x3EE6_6666),
    (0x56C, 0x3C23_D70A),
    (0x57C, 0x3DCC_CCCD),
    (0x53C, 0x4348_0000),
    (0x52C, 0x2AE1_2E13),
    (0x58C, 0x5968_2EBF),
    (0x13C, 0xBF80_0000),
    (0x22C, 0x3F80_0000),
    (0x31C, 0x3F80_0000),
    (0x40C, 0xBF80_0000),
    (0x14C, 1),
    (0x23C, 1),
    (0x32C, 2),
    (0x41C, 2),
    (0x12C, 1),
    (0x21C, 1),
    (0x30C, 1),
    (0x3FC, 1),
    (0x15C, 0x4046_6666),
    (0x24C, 0x4046_6666),
    (0x33C, 0x4046_6666),
    (0x42C, 0x4046_6666),
    (0x1EC, 0x3B16_BB99),
    (0x2DC, 0x3B16_BB99),
    (0x3CC, 0x3B16_BB99),
    (0x4BC, 0x3B16_BB99),
    (0x18C, 0x3E80_0000),
    (0x27C, 0x3E80_0000),
    (0x36C, 0x3E80_0000),
    (0x45C, 0x3E80_0000),
    (0x19C, 0x43E1_0000),
    (0x28C, 0x43E1_0000),
    (0x37C, 0x43E1_0000),
    (0x46C, 0x43E1_0000),
    (0xC, 1),
    (0x1C, 1),
    (0x2C, 2),
    (0x3C, 0x3F80_0000),
    (0x4C, 0x4248_0000),
    (0x5C, 1),
    (0x6C, 0xFFFF_FFFF),
    (0x7C, 0),
    (0x8C, 1),
    (0x9C, 0x3F80_0000),
    (0xAC, 0x437A_0000),
    (0xBC, 2),
    (0xCC, 1),
    (0xDC, 2),
    (0xEC, 3),
    (0xFC, 0x3F80_0000),
    (0x10C, 0x437A_0000),
    (0x11C, 3),
    (0x634, 1),
    (0x668, 0x3D4C_CCCD),
    (0x678, 0x3D4C_CCCD),
    (0x658, 0x3DCC_CCCD),
    (0xA68, 0x3C23_D70A),
    (0xA78, 0x3CA3_D70A),
];
/// The words the constructor stores without a tweak header, between the defaults of +0x634 and +0x668: a table of
/// floats at +0x59c..+0x610 (from 0x73a2c4, 0x72afcc, 0x73a2c8, 0x731d14, 0x73a2c0, 0x73a2bc, 0x73a2b8, 0x73a2b4,
/// 0x795e14, 0x739c8c, 0x739cb8, 0x7355d4, 0x795e90, 0x72f610, 0x795e18, 0x72f428, 0x72dd78, 0x738294, 0x72f664,
/// 0x72f674, 0x72dcdc, 0x731b0c, 0x72f614, 0x731cd4) and the word after the integer at +0x634.
const DRIVE_WORDS: [(u32, u32); 31] = [
    (0x59C, 0),
    (0x5A0, 0x3E0F_5C29),
    (0x5A4, 0x3E99_999A),
    (0x5A8, 0x3EF0_A3D7),
    (0x5AC, 0x3F26_6666),
    (0x5B0, 0x3F51_EB85),
    (0x5B4, 0x3F6E_147B),
    (0x5B8, 0x3F7D_F3B6),
    (0x5BC, 0x3F7F_7CEE),
    (0x5C0, 0x3F80_0000),
    (0x5C4, 0x3F80_0000),
    (0x5C8, 0x3F7E_B852),
    (0x5CC, 0x3F7A_E148),
    (0x5D0, 0x3F73_3333),
    (0x5D4, 0x3F66_6666),
    (0x5D8, 0x3F4C_CCCD),
    (0x5DC, 0x3F00_0000),
    (0x5E0, 0x3E4C_CCCD),
    (0x5E4, 0x3D4C_CCCD),
    (0x5E8, 0),
    (0x5EC, 0x3E99_999A),
    (0x5F0, 0x3EB3_3333),
    (0x5F4, 0x3ECC_CCCD),
    (0x5F8, 0x3F00_0000),
    (0x5FC, 0x3F33_3333),
    (0x600, 0x3F80_0000),
    (0x604, 0x3FC0_0000),
    (0x608, 0x40C0_0000),
    (0x60C, 0x41A0_0000),
    (0x610, 0x43FA_0000),
    (0x644, 0),
];
/// `E39_Ctor870`'s last unsigned (+0x4ec + 0xa0), whose vtable NGP replaces (`ntweak::layout::NGP_TABLE_VTABLE`).
const NGP_TABLE_UNSIGNED: u32 = 0x58C;
/// `CDrive::CDef`'s vtable (0x73a14c) and the name its base gets (`"Drive"`, 0x7380c0).
const DRIVE_DEF_VTABLE: u32 = 0x0073_A14C;
const DRIVE_DEF_NAME: u32 = 0x0073_80C0;
/// The unsigned the constructor sets last, from the two before it (+0x4fc and +0x50c), and the float it scales their
/// difference by (2.5, 0x72f57c).
const DRIVE_LAST: u32 = 0x51C;
const DRIVE_LAST_FROM: [u32; 2] = [0x4FC, 0x50C];
const DRIVE_LAST_SCALE: f32 = 2.5;

fn set_default(mem: &mut Mem, at: u32, value: u32) {
    set_dirty(mem, at);
    mem.set_u32(at + 0xC, value);
}

/// `CDrive::CDef::CDef` (0x506fe0) on the definition at `this`: the base, the vtable, the compounds and tweaks, then
/// the defaults in the asm's order. The last unsigned is the first minus the scaled difference of the first two, the
/// difference taken unsigned (`fild`, plus 2^32 when negative) and truncated by `_ftol2`. The unsigned at +0x58c is
/// NGP's, which reads the table after it.
pub fn construct_drive_def(mem: &mut Mem, this: u32) {
    ntweak::construct_base(mem, this, DRIVE_DEF_NAME);
    mem.set_u32(this, DRIVE_DEF_VTABLE);
    for (at, stride, count, members) in DRIVE_ARRAYS {
        for k in 0..count {
            construct_members(mem, this + at + k * stride, members);
        }
    }
    mem.set_u32(this + NGP_TABLE_UNSIGNED, NGP_TABLE_VTABLE);
    construct_members(mem, this, &DRIVE_TWEAKS);
    for k in 0..4 {
        for (at, value) in BRAKE_DEFAULTS {
            set_default(mem, this + 0x12C + k * 0xF0 + at, value);
        }
    }
    for k in 0..10 {
        for (at, value) in MIXED50_DEFAULTS {
            set_default(mem, this + 0x688 + k * 0x50 + at, value);
        }
    }
    for (at, value) in DRIVE_DEFAULTS {
        set_default(mem, this + at, value);
    }
    for (at, word) in DRIVE_WORDS {
        mem.set_u32(this + at, word);
    }
    let [a, b] = DRIVE_LAST_FROM.map(|at| mem.u32(this + at + 0xC));
    let difference = f64::from(a.wrapping_sub(b));
    let cut = ftol2(difference * f64::from(DRIVE_LAST_SCALE)) as u32;
    set_default(mem, this + DRIVE_LAST, a.wrapping_sub(cut));
}

/// `CEngine::CDef::CDef` (0x50f080) on the definition at `this`: the blocks it allocates from `host`, in its order,
/// then the words it writes (`engine_def`, generated from the exe: its 36 KB are constant stores), and NGP's patched
/// constructor's words over them.
pub fn construct_engine_def(mem: &mut Mem, host: &mut impl Heap, this: u32) {
    use crate::engine_driveline::engine_def::{ALLOCATIONS, IMAGES, NGP_WORDS};
    let blocks: Vec<u32> = ALLOCATIONS.iter().map(|&n| host.alloc(mem, n)).collect();
    for (image, at) in IMAGES.iter().zip(std::iter::once(this).chain(blocks.iter().copied())) {
        for &(offset, run) in image.runs {
            for (k, &word) in run.iter().enumerate() {
                mem.set_u32(at + offset + 4 * k as u32, word);
            }
        }
        for &(offset, block) in image.pointers {
            mem.set_u32(at + offset, blocks[block]);
        }
    }
    for &(offset, word) in NGP_WORDS {
        mem.set_u32(this + offset, word);
    }
}

/// A single tweak, for runs of them 0x10 apart.
const ONE_REAL: [(u32, Tweak); 1] = [(0, Tweak::Real)];
const ONE_UNSIGNED: [(u32, Tweak); 1] = [(0, Tweak::Unsigned)];
/// `CDrive::CSetup::CSetup` (0x47ed20)'s members (offset, stride, count, members), in its order: the three
/// differentials' torques, the twelve gear rows (`E13Mixed50`: ten gears, the final drive and the drop gear), their
/// twelve ids, then 24 reals (two brakes' reals with an `E13Real9` each, then the two brake choices at +0x61c and +0x62c).
const DRIVE_SETUP_ARRAYS: [(u32, u32, u32, Members); 4] = [
    (0xC, 0x10, 3, &ONE_REAL),
    (0x3C, 0x50, 12, &MIXED50),
    (0x3FC, 0x10, 12, &ONE_UNSIGNED),
    (0x4BC, 0x10, 24, &ONE_REAL),
];
/// `CDrive::CSetupOptions::CSetupOptions` (0x47d3c0)'s members: 37 gear rows, the gear count and ten
/// `E13UnsignedPair`s (each gear's choices), four unsigneds (the final drive's and drop gear's choices), then 57 reals
/// (`E13RealTriple`s, the brake choices and the brakes' `E13Real10`s).
const DRIVE_OPTIONS_ARRAYS: [(u32, u32, u32, Members); 4] = [
    (0xC, 0x50, 37, &MIXED50),
    (0xB9C, 0x10, 21, &ONE_UNSIGNED),
    (0xCEC, 0x10, 4, &ONE_UNSIGNED),
    (0xD2C, 0x10, 57, &ONE_REAL),
];
/// `CDrive::CSetup`'s vtable (0x72ec5c) and `CDrive::CSetupOptions`'s (0x72e774).
const DRIVE_SETUP_VTABLE: u32 = 0x0072_EC5C;
const DRIVE_OPTIONS_VTABLE: u32 = 0x0072_E774;

/// A setup object's constructor: the base with no name, its vtable, then its members.
fn construct_setup_object(mem: &mut Mem, this: u32, vtable: u32, arrays: &[(u32, u32, u32, Members)]) {
    construct_base(mem, this, 0);
    mem.set_u32(this, vtable);
    for &(at, stride, count, members) in arrays {
        for k in 0..count {
            construct_members(mem, this + at + k * stride, members);
        }
    }
}

/// `CDrive::CSetup::CSetup` (0x47ed20) on the setup at `this`.
pub fn construct_drive_setup(mem: &mut Mem, this: u32) {
    construct_setup_object(mem, this, DRIVE_SETUP_VTABLE, &DRIVE_SETUP_ARRAYS);
}

/// `CDrive::CSetupOptions::CSetupOptions` (0x47d3c0) on the options at `this`.
pub fn construct_drive_options(mem: &mut Mem, this: u32) {
    construct_setup_object(mem, this, DRIVE_OPTIONS_VTABLE, &DRIVE_OPTIONS_ARRAYS);
}

/// `CEngine::CSetup`'s vtable (0x72e4e4) and `CEngine::CSetupOptions`'s (0x72e814).
const ENGINE_SETUP_VTABLE: u32 = 0x0072_E4E4;
const ENGINE_OPTIONS_VTABLE: u32 = 0x0072_E814;

/// The engine setup of `CCompleteCarSetup::CCompleteCarSetup` (0x47f770, inline there) at `this`: an unnamed tweak, its
/// vtable and the unsigned at +0xc (`Dummy`).
pub fn construct_engine_setup(mem: &mut Mem, this: u32) {
    construct_base(mem, this, 0);
    mem.set_u32(this, ENGINE_SETUP_VTABLE);
    ntweak::construct_unsigned(mem, this + 0xC);
}

/// The engine options of `CCompleteCarSetupOptions::CCompleteCarSetupOptions` (0x47f0f0, inline there) at `this`: an
/// unnamed tweak, its vtable and an `E13UnsignedTriple` (`DummyMin`, `DummyMax`, `DummyStep`) from +0xc.
pub fn construct_engine_options(mem: &mut Mem, this: u32) {
    construct_base(mem, this, 0);
    mem.set_u32(this, ENGINE_OPTIONS_VTABLE);
    for at in (0xC..0x3C).step_by(0x10) {
        ntweak::construct_unsigned(mem, this + at);
    }
}

/// A word written as zero: the asm's float and integer zeros alike.
fn clear_words(mem: &mut Mem, this: u32, words: &[u32]) {
    for &at in words {
        mem.set_u32(this + at, 0);
    }
}

/// `E39_CDefCtor160`'s vtable (0x73a11c): the drive part at the drive's +0x10, whose slot 1 is [`clear_lanes`].
const DRIVE_PART_VTABLE: u32 = 0x0073_A11C;

/// `E39_CDefCtor160` (0x503ad0) on the drive's part at `this`: the vtable (no base), seven reals, two arrays of four
/// reals (`+0xd8`, `+0x118`), three cleared vectors and the words after them.
pub fn construct_drive_part(mem: &mut Mem, this: u32) {
    mem.set_u32(this, DRIVE_PART_VTABLE);
    for at in [0x10, 0x20, 0x30, 0x44, 0x54, 0x64, 0x74] {
        ntweak::construct_real(mem, this + at);
    }
    for at in (0xD8..0x158).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
    for at in [0x90, 0xA0, 0xB0] {
        rigid_body::layout::clear_vec(mem, this + at);
    }
    clear_words(mem, this, &[0xC0, 0xC4, 0x40]);
}

/// `CDrive::Init58D0(def, state, owner)` (0x5058d0, thiscall) on the car's drive at `this`: the part, the definition
/// and state words, the owner into the state (`+0x580`) when there is a definition, then the cleared words.
pub fn construct_drive(mem: &mut Mem, this: u32, def: u32, state: u32, owner: u32) {
    construct_drive_part(mem, this + 0x10);
    mem.set_u32(this + 4, def);
    mem.set_u32(this + 8, state);
    if def != 0 {
        mem.set_u32(state + 0x580, owner);
    }
    clear_words(mem, this, &[0x1A4, 0x1A8, 0x1BC, 0x1C4, 0x1C8, 0x1CC, 0x1D0, 0x1D4, 0x1D8, 0x170, 0x19C, 0x1A0]);
}

/// `CDrive::Set4510` (0x504510): the part's slot 1, [`clear_lanes`] (the asm's tail jump through the part's vtable) (name ours).
pub fn drive_reset_part(mem: &mut Mem, drive: u32) {
    clear_lanes(mem, drive + 0x10);
}

/// `CEngine::CScratch`'s vtable (0x73b62c) and `CEngine`'s (0x73b6ec).
const ENGINE_SCRATCH_VTABLE: u32 = 0x0073_B62C;
const ENGINE_VTABLE: u32 = 0x0073_B6EC;

/// `CEngine::CScratch::CScratch` (0x50d4f0) on the scratch at `this`: the vtable (no base), two reals and an unsigned.
pub fn construct_engine_scratch(mem: &mut Mem, this: u32) {
    mem.set_u32(this, ENGINE_SCRATCH_VTABLE);
    ntweak::construct_real(mem, this + 0x8);
    ntweak::construct_real(mem, this + 0x18);
    ntweak::construct_unsigned(mem, this + 0x28);
}

/// `CEngine::CEngine(def, state, owner)` (0x50df30, thiscall) on the car's engine at `this`: the vtable, the scratch
/// (`+0xc`), a real, the three words, then the cleared ones.
pub fn construct_engine(mem: &mut Mem, this: u32, def: u32, state: u32, owner: u32) {
    mem.set_u32(this, ENGINE_VTABLE);
    construct_engine_scratch(mem, this + 0xC);
    ntweak::construct_real(mem, this + 0x4C);
    mem.set_u32(this + 4, def);
    mem.set_u32(this + 0x44, owner);
    mem.set_u32(this + 8, state);
    clear_words(mem, this, &[0x184, 0x188, 0x18C, 0x190, 0x194, 0x198, 0x48]);
}

/// A differential's state (`0x5029c0`, 0xc8 bytes, four at the drive state's +0x48): its tweaks.
const DIFF_STATE: [(u32, Tweak); 12] = [
    (0x00, Tweak::Real),
    (0x10, Tweak::Real),
    (0x20, Tweak::Integer),
    (0x30, Tweak::Real),
    (0x40, Tweak::Real),
    (0x50, Tweak::Integer),
    (0x60, Tweak::Real),
    (0x70, Tweak::Unsigned),
    (0x80, Tweak::Unsigned),
    (0x90, Tweak::Real),
    (0xA0, Tweak::Real),
    (0xB8, Tweak::Unsigned),
];
/// The differential states' first values, set dirty in the asm's order (0x11791d29 is a float of about 1.96e-28).
const DIFF_STATE_DEFAULTS: [(u32, u32); 12] = [
    (0x00, 0),
    (0x10, 0),
    (0x40, 0),
    (0x20, 0),
    (0x50, 0),
    (0x70, 0x1179_1D29),
    (0x80, 0x1179_1D29),
    (0xB8, 0),
    (0x60, 0x3F80_0000),
    (0x30, 0x3F80_0000),
    (0x90, 0x3F80_0000),
    (0xA0, 0x3F80_0000),
];
const DIFF_STATES: u32 = 0x48;
const DIFF_STATE_SIZE: u32 = 0xC8;
/// `CDrive::CState`'s vtable (0x73a0cc) and its base's name (`"Drive"`, 0x7380c0).
const DRIVE_STATE_VTABLE: u32 = 0x0073_A0CC;
/// The drive state's reals set after the differentials' (dirty, then the value): each differential's +0x90 and +0xa0.
const DRIVE_STATE_DEFAULTS: [(u32, u32); 8] = [
    (0xD8, 0x3F80_0000),
    (0x1A0, 0x3F80_0000),
    (0x268, 0x3F00_0000),
    (0x330, 0x3F00_0000),
    (0xE8, 0),
    (0x1B0, 0),
    (0x278, 0x3F80_0000),
    (0x340, 0x3F80_0000),
];
/// `CDrive::CState::+0x45c`'s value outside the game mode 0x566690 accepts (400.0, 0x731e00).
const DRIVE_STATE_45C: f32 = 400.0;

/// `0x566690` on the game's options: mode 7 with sub-mode 0 or 10.
fn game_mode_6690(mem: &Mem) -> bool {
    let options = mem.u32(FLOW_OBJECT);
    mem.u32(options) == 7 && matches!(mem.u32(options + 0xCA4), 0 | 10)
}

/// `CDrive::CState::CState` (0x505480, thiscall, three stack arguments it does not read) on the state at `this`: the
/// base named `"Drive"`, the vtable, the four differential states, the tweaks, then the defaults and words (in the asm's
/// order where one is read after). `+0x45c` and `+0x458` are 0 in the game mode `0x566690` accepts and 400.0 otherwise; `+0x564`'s name, dirty
/// and value words are copied from `+0x540` once it is 1.
pub fn construct_drive_state(mem: &mut Mem, this: u32) {
    construct_base(mem, this, DRIVE_DEF_NAME);
    mem.set_u32(this, DRIVE_STATE_VTABLE);
    for k in 0..4 {
        construct_members(mem, this + DIFF_STATES + k * DIFF_STATE_SIZE, &DIFF_STATE);
    }
    ntweak::construct_real(mem, this + 0x36C);
    for at in [0x540, 0x550, 0x564] {
        ntweak::construct_integer(mem, this + at);
    }
    for at in [0x584, 0x594] {
        ntweak::construct_real(mem, this + at);
    }
    mem.set_u32(this + 0x57C, 0x0013_0807);
    mem.set_u32(this + 0x5BC, 1);
    clear_words(mem, this, &[0x5A4, 0x5A8, 0x5AC, 0x5B0, 0x5B4, 0x37C]);
    set_default(mem, this + 0x584, 0);
    mem.set_u32(this + 0x5B8, 2);
    for k in 0..4 {
        let diff = this + DIFF_STATES + k * DIFF_STATE_SIZE;
        for (at, value) in DIFF_STATE_DEFAULTS {
            set_default(mem, diff + at, value);
        }
        clear_words(mem, diff, &[0xB0, 0xB4]);
    }
    clear_words(mem, this, &(0xC..0x48).step_by(4).collect::<Vec<_>>());
    for (at, value) in DRIVE_STATE_DEFAULTS {
        set_default(mem, this + at, value);
    }
    clear_words(mem, this, &[0x384, 0x380]);
    let value = if game_mode_6690(mem) { 0.0 } else { DRIVE_STATE_45C };
    mem.set_f32(this + 0x45C, value);
    mem.set_f32(this + 0x458, value);
    mem.set_u32(this + 0x368, 0);
    set_default(mem, this + 0x540, 1);
    for k in 1..4 {
        let word = mem.u32(this + 0x540 + 4 * k);
        mem.set_u32(this + 0x564 + 4 * k, word);
    }
    clear_words(mem, this, &[0x574, 0x560, 0x578]);
    mem.set_u32(this + 0x388, 2);
    mem.set_u32(this + 0x38C, 0);
    mem.set_f32(this + 0x460, 1.0);
    clear_words(mem, this, &(0x390..0x458).step_by(4).collect::<Vec<_>>());
    for at in [CLUTCH_MATRIX_AT, STEP_MATRIX_AT] {
        clear_words(mem, this + at, &(0..0x40).step_by(4).collect::<Vec<_>>());
    }
    clear_words(mem, this + STEP_VEC_AT, &[0, 4, 8, 0xC]);
    mem.set_u32(this + 0x500, 0);
}

/// `CEngine::CState`'s vtable (0x73b5dc) and its base's name (0x734d0c).
const ENGINE_STATE_VTABLE: u32 = 0x0073_B5DC;
const ENGINE_STATE_NAME: u32 = 0x0073_4D0C;
/// The engine state's tweaks after the base.
const ENGINE_STATE: [(u32, Tweak); 13] = [
    (0x18C, Tweak::Real),
    (0x19C, Tweak::Real),
    (0x1AC, Tweak::Real),
    (0x1BC, Tweak::Real),
    (0x1CC, Tweak::Real),
    (0x1E4, Tweak::Real),
    (0x1F4, Tweak::Real),
    (0x204, Tweak::Real),
    (0x214, Tweak::Real),
    (0x228, Tweak::Unsigned),
    (0x240, Tweak::Real),
    (0x250, Tweak::Real),
    (0x260, Tweak::Real),
];
/// The four 0x38-byte blocks from the engine state's +0x88 (their flags at +0x168): the words that are not zero.
const ENGINE_BLOCKS: u32 = 0x88;
const ENGINE_BLOCK: u32 = 0x38;
const ENGINE_BLOCK_FLAGS: u32 = 0x168;
const ENGINE_BLOCK_WORDS: [(u32, u32); 3] = [(0x08, 0x3F80_0000), (0x18, 0x3C23_D70A), (0x30, 0x3F80_0000)];
/// The engine state's two air terms (`+0x60` and `+0x70`, four floats each, `DamperUpdateB` with 287.0): the words
/// +0xc, +8 and +4 (0.03, 110300.0 and 293.15: the floats at 0x739958, 0x73b5c0 and 0x73a13c), set before the update
/// writes the first.
const ENGINE_AIR: [u32; 2] = [0x60, 0x70];
const ENGINE_AIR_TERMS: [(u32, u32); 3] = [(0xC, 0x3CF5_C28F), (0x8, 0x47D7_6E00), (0x4, 0x4392_9333)];
const ENGINE_AIR_INPUT: f32 = 287.0;
/// The engine state's reals set (dirty, then the value) before the air terms, in the asm's order: 116495.0, 101300.0,
/// 293.14 and 816.814 (0x73b5d0, 0x73b5cc, 0x73b5c8, 0x73b5c4). 293.14 also goes to +0x1dc and +0x1e0.
const ENGINE_STATE_DEFAULTS: [(u32, u32); 4] =
    [(0x1BC, 0x47E3_8780), (0x1AC, 0x47C5_DA00), (0x19C, 0x4392_91EC), (0x18C, 0x444C_341A)];
/// The engine state's words after the air terms, in the asm's order (1.0, 373.0, 373.0, 0, 1.5, 0, 5.2, 0, 423.0).
const ENGINE_STATE_WORDS: [(u32, u32); 9] = [
    (0x80, 0x3F80_0000),
    (0x20, 0x43BA_8000),
    (0x28, 0x43BA_8000),
    (0x27C, 0),
    (0x24, 0x3FC0_0000),
    (0x84, 0),
    (0x1C, 0x40A6_6666),
    (0x238, 0),
    (0x23C, 0x43D3_8000),
];
/// The debug flag (`DbgFlags` of the physics definition) whose branch calls `CGraph::SetA` on a null graph.
const ENGINE_GRAPH_FLAG: u32 = 0x200;

/// `CEngine::CState::CState(engine, name, owner)` (0x50bed0, thiscall) on the state at `this`: the base, the vtable, the
/// tweaks, the owner at +0xc, then the defaults and words in the asm's order. `owner` is the only argument read. With
/// the physics definition's debug flag 0x200 the original calls `CGraph::SetA` on a null graph (a crash), so that
/// branch is `Err`.
pub fn construct_engine_state(mem: &mut Mem, this: u32, owner: u32) -> Result<(), HostUnavailable> {
    construct_base(mem, this, ENGINE_STATE_NAME);
    mem.set_u32(this, ENGINE_STATE_VTABLE);
    construct_members(mem, this, &ENGINE_STATE);
    mem.set_u32(this + 0xC, owner);
    clear_words(
        mem,
        this,
        &[0x2C, 0x30, 0x34, 0x38, 0x44, 0x48, 0x3C, 0x40, 0x4C, 0x50, 0x54, 0x58, 0x10, 0x14, 0x18, 0x270],
    );
    for k in 0..4 {
        let block = this + ENGINE_BLOCKS + k * ENGINE_BLOCK;
        clear_words(mem, block, &(0..ENGINE_BLOCK).step_by(4).collect::<Vec<_>>());
        for (at, value) in ENGINE_BLOCK_WORDS {
            mem.set_u32(block + at, value);
        }
        mem.set_u32(this + ENGINE_BLOCK_FLAGS + 4 * k, 1);
    }
    clear_words(mem, this, &[0x178, 0x17C, 0x180, 0x184, 0x188]);
    for (at, value) in ENGINE_STATE_DEFAULTS {
        set_default(mem, this + at, value);
    }
    let air = mem.u32(this + 0x19C + 0xC);
    mem.set_u32(this + 0x1DC, air);
    mem.set_u32(this + 0x1E0, air);
    mem.set_u32(this + 0x274, 0);
    if physics_core::layout::cdef_debug_flags(mem) & ENGINE_GRAPH_FLAG != 0 {
        return Err(HostUnavailable);
    }
    mem.set_u32(this + 0x278, 0);
    mem.set_u32(this + 0x5C, 0);
    for at in ENGINE_AIR {
        for (term, value) in ENGINE_AIR_TERMS {
            mem.set_u32(this + at + term, value);
        }
        spring_damper::layout::update_b(mem, this + at, ENGINE_AIR_INPUT);
    }
    for (at, word) in ENGINE_STATE_WORDS {
        mem.set_u32(this + at, word);
    }
    set_default(mem, this + 0x240, 0);
    Ok(())
}

/// `E39_StateInitC70` (0x502c70, fastcall `(self)`): the reset of the drive state's copy in the damage handler
/// (`ResetDamage` calls it at `+0x3a18`). Three (1.0, 1) pairs from `+0x0`, four (1, 1, 0) triples from `+0x18`, ten
/// floats 1.0 from `+0x48`, the words `+0x70` and `+0x78..+0x80` 1, the floats `+0x84..+0x8c` 1.0 and the word `+0x90`
/// 1. `+0x74` is left as it is.
pub fn state_init_c70(mem: &mut Mem, this: u32) {
    for pair in 0..3 {
        mem.set_f32(this + 8 * pair, 1.0);
        mem.set_u32(this + 8 * pair + 4, 1);
    }
    for triple in 0..4 {
        let at = this + 0x18 + 0xc * triple;
        mem.set_u32(at, 1);
        mem.set_u32(at + 4, 1);
        mem.set_u32(at + 8, 0);
    }
    for k in 0..10 {
        mem.set_f32(this + 0x48 + 4 * k, 1.0);
    }
    for at in [0x70, 0x78, 0x7c, 0x80, 0x90] {
        mem.set_u32(this + at, 1);
    }
    for at in [0x84, 0x88, 0x8c] {
        mem.set_f32(this + at, 1.0);
    }
}

/// `CEngine::CState::ResetSub78` (0x50c370, thiscall): the reset of the engine state's copy in the damage handler
/// (`ResetDamage` calls it at `+0x3aac`). Four vectors (0, 0, 1, 0) from `+0x8`, the word `+0x48` 1, the floats
/// `+0x4c`, `+0x50`, `+0x58` and `+0x5c` 1.0 and `+0x54`, `+0x60..+0x70` 0, and the word `+0x74` 1.
pub fn reset_sub78(mem: &mut Mem, this: u32) {
    for k in 0..4 {
        let at = this + 8 + 0x10 * k;
        mem.set_f32(at, 0.0);
        mem.set_f32(at + 4, 0.0);
        mem.set_f32(at + 8, 1.0);
        mem.set_f32(at + 0xc, 0.0);
    }
    mem.set_u32(this + 0x48, 1);
    for at in [0x4c, 0x50, 0x58, 0x5c] {
        mem.set_f32(this + at, 1.0);
    }
    for at in [0x54, 0x60, 0x64, 0x68, 0x6c, 0x70] {
        mem.set_f32(this + at, 0.0);
    }
    mem.set_u32(this + 0x74, 1);
}

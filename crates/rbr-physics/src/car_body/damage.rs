//! `CCarDamageHandler`, the damage handler inside the car (`this` is `car + 0x9f4`, the car's `m_damage`). Its member
//! functions take the handler as an address, as the car's other ports do. The asm (not the C++ in `src/rbr`) decides.

use super::layout::WORLD;
use super::light;
use crate::car_body::store::CarStore;
use crate::mem::Mem;
use crate::ntweak::layout::NULL_NAME;

/// `CCarDamageHandler::Set3FB0(level)` (0x4ebfb0, `ret 4`): counts the call (+0x3a14), stores the level (+0x3df0) and
/// stores the threshold for it (+0x3dec): 0.0 for level 0, 0.2 for 1, 0.7 for 2, and 1.0 for 3 and any other word
/// (the asm compares the level as unsigned).
pub fn set_level<S: CarStore>(mem: &mut S, handler: u32, level: u32) {
    mem.set_u32(handler + 0x3A14, mem.u32(handler + 0x3A14).wrapping_add(1));
    mem.set_u32(handler + 0x3DF0, level);
    let threshold = match level {
        0 => 0.0,
        1 => 0.2,
        2 => 0.7,
        _ => 1.0,
    };
    mem.set_f32(handler + 0x3DEC, threshold);
}

/// `CCarDamageHandler::Test3B20` (0x4ec0d0, the windshield gate, no stack argument): false when the enable word (+0x3b20)
/// is zero or the level float (+0x3aa0) is 0.0 (a NaN passes the test, as the asm's `ucomiss` branch takes it).
/// Otherwise true when fewer than three of the four counted words (+0x3a30, +0x3a3c, +0x3a48, +0x3a54) are zero.
pub fn test_3b20<S: CarStore>(mem: &S, handler: u32) -> bool {
    if mem.u32(handler + 0x3B20) == 0 || mem.f32(handler + 0x3AA0) == 0.0 {
        return false;
    }
    let zero_words = [0x3A30, 0x3A3C, 0x3A48, 0x3A54].iter().filter(|&&at| mem.u32(handler + at) == 0).count();
    zero_words < 3
}

/// The zones (64, the first word of each, `ZONE_STRIDE` apart from `ZONES_AT`) and the floats (64, from `FLOATS_AT`).
const ZONES: u32 = 0x40;
const ZONES_AT: u32 = 0x2214;
const ZONE_STRIDE: u32 = 0x58;
const FLOATS_AT: u32 = 0x3808;
/// The tail: 0xfa words from `TAIL_AT` (the level, the threshold and the counter are in it).
const TAIL_AT: u32 = 0x3A0C;
const TAIL_WORDS: u32 = 0xFA;

/// The calls `ResetDamage` makes on the handler's members (their bodies are other subsystems'; the test answers them
/// from the original's records).
pub trait ResetDamageCalls<S = Mem> {
    /// A call on a member: `callee` with `this` (the member's address, as ecx).
    fn call(&mut self, mem: &mut S, callee: u32, this: u32);
}

/// The members' functions `ResetDamage` calls, with the member's offset and stride (the loops' steps).
pub(crate) const STATE_INIT_C70: u32 = 0x502C70; // E39 DriveD StateInitC70, at +0x3a18
pub(crate) const ENGINE_RESET_SUB78: u32 = 0x50C370; // CEngine::CState::ResetSub78, at +0x3aac
pub(crate) const WHEEL_ZERO: u32 = 0x53B050; // E51WheelDamage::Zero, four at +0x3c04 (stride 0x38)
pub(crate) const VALUE8_CLEAR: u32 = 0x5317F0; // CU32Value8::Clear, four at +0x3d04 (stride 0x20)
pub(crate) const VALUE_CLEAR: u32 = 0x52D960; // CU32Value::Clear, four at +0x3ce4 (stride 8)
pub(crate) const FILL_ONES: u32 = 0x51CD60; // E44 FillOnes, four at +0x3d84 (stride 0xc)
pub(crate) const M1640: u32 = 0x521640; // E45 M1640, four at +0x3db4 (stride 4)

/// `CCarDamageHandler::ResetDamage()` (0x4ec1e0, `ret 0`): counts the call (+0x3a10), zeroes the zones (three words each,
/// stride 0x58 from 0x2214) and the 64 floats (from 0x3808), resets the members, then sets the flag words (+0x3dc4 to
/// +0x3de8 to 1), zeroes the floats and words at +0x3dfc to +0x3e04 and +0x3e58, and writes the 29 and 0 pairs at
/// +0x3e08 to +0x3e54. The light block (+0x3b24) is the car-body port `light::block_reset`; the other members are calls.
pub fn reset_damage<S: CarStore>(calls: &mut impl ResetDamageCalls<S>, mem: &mut S, handler: u32) {
    mem.set_u32(handler + 0x3A10, mem.u32(handler + 0x3A10).wrapping_add(1));
    for z in 0..ZONES {
        for word in 0..3u32 {
            mem.set_u32(handler + ZONES_AT + ZONE_STRIDE * z + 4 * word, 0);
        }
        mem.set_f32(handler + FLOATS_AT + 4 * z, 0.0);
    }
    calls.call(mem, STATE_INIT_C70, handler + 0x3A18);
    calls.call(mem, ENGINE_RESET_SUB78, handler + 0x3AAC);
    light::block_reset(mem, handler + 0x3B24);
    for k in 0..4 {
        calls.call(mem, WHEEL_ZERO, handler + 0x3C04 + 0x38 * k);
        calls.call(mem, VALUE8_CLEAR, handler + 0x3D04 + 0x20 * k);
        calls.call(mem, VALUE_CLEAR, handler + 0x3CE4 + 8 * k);
    }
    for k in 0..4 {
        calls.call(mem, FILL_ONES, handler + 0x3D84 + 0xC * k);
    }
    for k in 0..4 {
        calls.call(mem, M1640, handler + 0x3DB4 + 4 * k);
    }
    for at in (0x3DC4..=0x3DE8).step_by(4) {
        mem.set_u32(handler + at, 1);
    }
    mem.set_f32(handler + 0x3E00, 0.0);
    mem.set_f32(handler + 0x3DFC, 0.0);
    for at in [0x3DF4, 0x3DF8, 0x3E04, 0x3E58] {
        mem.set_u32(handler + at, 0);
    }
    for k in 0..10 {
        mem.set_u32(handler + 0x3E08 + 8 * k, 0x1D);
        mem.set_u32(handler + 0x3E0C + 8 * k, 0);
    }
}

/// The calls `Vf14x` makes out of the handler: the material lookup (the test answers it from the records).
pub trait Vf14xCalls {
    /// `CMaterialManager::Tag520940(id)` (0x520940, one stack argument) on `this` (the manager's material table, +0x18):
    /// the address of the material's entry.
    fn tag_520940(&mut self, this: u32, id: u32) -> u32;
}

/// `CCarDamageHandler::Vf14x()` (0x4ec440, the vtable's slot 5, `ret 0`): when the world's manager (the global at `WORLD`,
/// its +0x24) has the flag 0x1000000 at +0x160e0, the name of the material the handler's inner id (+0x3e04) selects: the
/// tag's word at +4, or the default string for a null tag (`GetNameOrNull`, which the asm tail-jumps to; read here). When
/// the flag is clear the asm returns the manager pointer (the C++ reconstruction returns 0).
pub fn vf14x<S: CarStore>(calls: &mut impl Vf14xCalls, mem: &S, handler: u32) -> u32 {
    let manager = mem.u32(mem.u32(WORLD) + 0x24);
    if mem.u32(manager + 0x160E0) & 0x0100_0000 == 0 {
        return manager;
    }
    let tag = calls.tag_520940(manager + 0x18, mem.u32(handler + 0x3E04));
    if tag == 0 {
        NULL_NAME
    } else {
        mem.u32(tag + 4)
    }
}

/// `CCarDamageHandler::CopyFrom(src)` (0x4ec470, `ret 4`): copies the damage state from `src` into the handler. The asm
/// runs eight groups of eight (a zone word, then a float, each pair in turn), then the tail, so the copy is one flat
/// sequence in that order (the source is read before its word is written, as the asm does).
pub fn copy_from<S: CarStore>(mem: &mut S, handler: u32, src: u32) {
    for z in 0..ZONES {
        copy_word(mem, handler + ZONES_AT + ZONE_STRIDE * z, src + ZONES_AT + ZONE_STRIDE * z);
        copy_word(mem, handler + FLOATS_AT + 4 * z, src + FLOATS_AT + 4 * z);
    }
    for i in 0..TAIL_WORDS {
        copy_word(mem, handler + TAIL_AT + 4 * i, src + TAIL_AT + 4 * i);
    }
}

/// One word, read from `src` and written to `dst`.
fn copy_word<S: CarStore>(mem: &mut S, dst: u32, src: u32) {
    let value = mem.u32(src);
    mem.set_u32(dst, value);
}

/// The functor count (+0x80 of the ops) and the entry size: four entries of 0x20 bytes from the ops' start.
const FUNCTOR_COUNT: u32 = 0x80;
const FUNCTOR_ENTRY: u32 = 0x20;
const FUNCTOR_SLOTS: u32 = 4;
/// The function each index of `AddFunctor`'s switch stores (0x4ef720's jump table). `None`: the switch leaves 18 and 23 out.
const FUNCTORS: [Option<u32>; 29] = [
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

/// `E36DamageOps::AddFunctor(idx, a, b, c)` (0x4ef720, `ret 0x10`): appends the entry (the function for `idx`, zeros, `b`
/// at +0x10, `a` at +0x14, `c` at +0x18) at the count (+0x80) and counts it. Returns false, and changes nothing, when the
/// table is full (count 4 or more), `idx` is outside 0..29, or the switch leaves it out.
pub fn add_functor<S: CarStore>(mem: &mut S, ops: u32, idx: i32, a: u32, b: u32, c: u32) -> bool {
    let count = mem.u32(ops + FUNCTOR_COUNT);
    if count >= FUNCTOR_SLOTS || idx >= FUNCTORS.len() as i32 {
        return false;
    }
    let Some(function) = usize::try_from(idx).ok().and_then(|i| FUNCTORS[i]) else {
        return false;
    };
    let entry = ops + count * FUNCTOR_ENTRY;
    mem.set_u32(entry, function);
    mem.set_u32(entry + 4, 0);
    mem.set_u32(entry + 8, 0);
    mem.set_u32(entry + 0xC, 0);
    mem.set_u32(entry + 0x10, b);
    mem.set_u32(entry + 0x14, a);
    mem.set_u32(entry + 0x18, c);
    mem.set_u32(ops + FUNCTOR_COUNT, count + 1);
    true
}

/// `CCarDamageHandler::ImpactScale`'s scale (0x4ec140, the slot 2 thunk): the impact's length times 0.08 (the asm's
/// `mulss` with the constant 0.08f), which slot 1 takes as its `dmg`.
pub fn impact_scale(length: f32) -> f32 {
    length * 0.08
}

/// The handler's random step (`E35_Rand`'s and `ApplyFunctors`' inline copy): the seed's next value and its draw, the
/// remainder by 100001 (the asm's unsigned `div`). Returns `(next, draw)`.
pub fn next_draw(seed: u32) -> (u32, u32) {
    let next = seed.wrapping_mul(0x0200_0081).wrapping_add(0xE623_F41D);
    (next, next % 100_001)
}

/// The draw as a fraction (`fild`, then `fmul` by 1e-5f at 24 bits).
fn fraction(draw: u32) -> f32 {
    draw as f32 * 0.000_01
}

/// `E35_Rand(lo, hi, seed)`'s value (0x4ebd70, x87 at 24 bits): `fraction * (hi - lo) + lo`.
pub fn rand(draw: u32, lo: f32, hi: f32) -> f32 {
    fraction(draw) * (hi - lo) + lo
}

/// The spread factor of `ApplyFunctors` (the constant 0x3e4cccd0, one ulp above 0.2f).
const SPREAD: f32 = f32::from_bits(0x3E4C_CCD0);

/// `ApplyFunctors`' value for an entry (0x4ec560, x87 at 24 bits): `(fraction * spread + 0.9) * scale`.
pub fn functor_value(draw: u32, scale: f32) -> f32 {
    (fraction(draw) * SPREAD + 0.9) * scale
}

/// `ApplyFunctors`' keep test: a functor that returned non-zero becomes the hit when its weight is at least the best
/// so far (`comiss`/`jb`: a NaN does not keep).
pub fn functor_keeps(weight: f32, best: f32) -> bool {
    weight >= best
}

/// A zone or table index the impact path uses: non-zero and below 64.
pub fn zone_index(idx: u32) -> bool {
    idx != 0 && idx < 0x40
}

/// `ZoneScaled`'s raise test: `value` above the zone's level (ordered: a NaN does not raise).
pub fn zone_raises(level: f32, value: f32) -> bool {
    value > level
}

/// The zone's damage float after a raise: `value` capped at 1.0 (1.0 for a NaN, the asm's `ja`), or None (unchanged)
/// when the float is not below 1.0.
pub fn zone_float(float: f32, value: f32) -> Option<f32> {
    (1.0 > float).then_some(if 1.0 > value { value } else { 1.0 })
}

/// The gate of `EngineGate` (`min` 3) and of `RingStore` (`min` 2): the level (signed) at least `min`, `a > b`
/// (ordered) and the enable word set.
pub fn gate_open(level: u32, min: i32, a: f32, b: f32, enabled: u32) -> bool {
    level as i32 >= min && a > b && enabled != 0
}

/// `EngineGate`'s fire test: the draw below 0.05 (x87 `fcomi`).
pub fn engine_fires(r: f32) -> bool {
    0.05 > r
}

/// `RingStore`'s next ring index: the current one plus one, modulo 10 (unsigned).
pub fn ring_next(cur: u32) -> u32 {
    cur.wrapping_add(1) % 10
}

/// `Slot1`'s two constants: `EngineGate`'s limit (the pushed 11000.0f at 0x4f13a6) and the impact above which a hit
/// emits (500.0f at 0x731cd4). NGP scales both per car at load (`notes/ngp.md` "Force feedback", the "0.94195 group" factor): it
/// patches the push's immediate and repoints the compare to its own `1017130c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageTuning {
    pub engine_gate_limit: f32,
    pub emit_impact: f32,
}

impl DamageTuning {
    pub const STOCK: DamageTuning = DamageTuning { engine_gate_limit: 11000.0, emit_impact: 500.0 };
}

/// `Slot1`'s emit test: a raised zone or an impact above `emit_impact`.
pub fn impact_emits(raised: bool, dmg: f32, emit_impact: f32) -> bool {
    raised || dmg > emit_impact
}

/// `Slot1`'s peak test: the impact above the stored peak (ordered).
pub fn new_peak(peak: f32, dmg: f32) -> bool {
    dmg > peak
}

/// `Slot1`'s inner id: `a3`, or 0 when it is above 0xff (unsigned).
pub fn inner_id(a3: u32) -> u32 {
    if a3 > 0xFF {
        0
    } else {
        a3
    }
}

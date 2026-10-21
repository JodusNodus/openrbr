//! The car's damage handler (`handler` is the address of the car's damage member): its level, its reset and copy, and the
//! functor table and random draws the impact path uses. Offsets are in `layout::damage_handler_at`.

use super::layout::damage_handler_at as at;
use super::layout::WORLD;
use super::light;
use crate::car_body::store::CarStore;
use crate::mem::Mem;
use crate::ntweak::layout::NULL_NAME;

/// Sets the damage level: counts the call, stores the level and stores the threshold for it: 0.0 for level 0, 0.2 for 1,
/// 0.7 for 2, and 1.0 for 3 and any other word (the level compares as unsigned).
pub fn set_level<S: CarStore>(mem: &mut S, handler: u32, level: u32) {
    mem.set_u32(handler + at::SET_LEVEL_COUNT_AT, mem.u32(handler + at::SET_LEVEL_COUNT_AT).wrapping_add(1));
    mem.set_u32(handler + at::LEVEL_AT, level);
    let threshold = match level {
        0 => 0.0,
        1 => 0.2,
        2 => 0.7,
        _ => 1.0,
    };
    mem.set_f32(handler + at::LEVEL_THRESHOLD_AT, threshold);
}

/// Whether the car is still drivable (the windshield gate): false when the enable word is zero or the level float is 0.0
/// (a NaN passes the test). Otherwise true when fewer than three of the four counted words are zero.
pub fn drivable<S: CarStore>(mem: &S, handler: u32) -> bool {
    if mem.u32(handler + at::WINDSHIELD_ENABLE_AT) == 0 || mem.f32(handler + at::WINDSHIELD_LEVEL_AT) == 0.0 {
        return false;
    }
    let zero_words = at::COUNTED_WORDS_AT.iter().filter(|&&word| mem.u32(handler + word) == 0).count();
    zero_words < 3
}

/// The calls `reset_damage` makes on the handler's members (their bodies are other subsystems'; the test answers them
/// from the original's records).
pub trait ResetDamageCalls<S = Mem> {
    /// A call on a member: `callee` with the member's address.
    fn call(&mut self, mem: &mut S, callee: u32, member: u32);
}

/// Resets the damage: counts the call, zeroes the zones (three words each) and their 64 floats, resets the members, then
/// sets the flag words to 1, zeroes the floats and words, and writes the ten (29, 0) pairs. The light block is
/// `light::block_reset`; the other members are calls.
pub fn reset_damage<S: CarStore>(calls: &mut impl ResetDamageCalls<S>, mem: &mut S, handler: u32) {
    mem.set_u32(handler + at::RESET_COUNT_AT, mem.u32(handler + at::RESET_COUNT_AT).wrapping_add(1));
    for z in 0..at::ZONES {
        for word in 0..3u32 {
            mem.set_u32(handler + at::ZONES_AT + at::ZONE_STRIDE * z + 4 * word, 0);
        }
        mem.set_f32(handler + at::FLOATS_AT + 4 * z, 0.0);
    }
    calls.call(mem, at::STATE_INIT_C70, handler + at::STATE_AT);
    calls.call(mem, at::ENGINE_RESET_SUB78, handler + at::ENGINE_AT);
    light::block_reset(mem, handler + at::LIGHT_BLOCK_AT);
    for k in 0..4 {
        for member in &at::WHEEL_GROUP {
            calls.call(mem, member.function, handler + member.at + member.stride * k);
        }
    }
    for member in [&at::ONES_GROUP, &at::STATE_GROUP] {
        for k in 0..member.count {
            calls.call(mem, member.function, handler + member.at + member.stride * k);
        }
    }
    for word in (at::FLAGS_FIRST_AT..=at::FLAGS_LAST_AT).step_by(4) {
        mem.set_u32(handler + word, 1);
    }
    for word in at::ZEROED_FLOATS_AT {
        mem.set_f32(handler + word, 0.0);
    }
    for word in at::ZEROED_WORDS_AT {
        mem.set_u32(handler + word, 0);
    }
    for k in 0..at::PAIRS {
        mem.set_u32(handler + at::PAIRS_AT + 8 * k, at::PAIR_INITIAL);
        mem.set_u32(handler + at::PAIRS_AT + 4 + 8 * k, 0);
    }
}

/// The calls `material_name` makes out of the handler: the material lookup (the test answers it from the records).
pub trait Vf14xCalls {
    /// The address of the material's entry in the manager's material table.
    fn tag_520940(&mut self, table: u32, id: u32) -> u32;
}

/// The name of the material the handler's inner id selects: when the world's manager has the material-names flag, the
/// entry's name, or the default string for a null entry. When the flag is clear, returns the manager pointer (the
/// original's value).
pub fn vf14x<S: CarStore>(calls: &mut impl Vf14xCalls, mem: &S, handler: u32) -> u32 {
    let manager = mem.u32(mem.u32(WORLD) + at::WORLD_MANAGER_AT);
    if mem.u32(manager + at::MANAGER_FLAGS_AT) & at::MANAGER_MATERIAL_NAMES == 0 {
        return manager;
    }
    let tag = calls.tag_520940(manager + at::MATERIAL_TABLE_AT, mem.u32(handler + at::MATERIAL_ID_AT));
    if tag == 0 {
        NULL_NAME
    } else {
        mem.u32(tag + 4)
    }
}

/// Copies the damage state from `src` into the handler: eight groups of eight (a zone word, then a float, each pair in
/// turn), then the tail, as one flat sequence in that order (the source is read before its word is written).
pub fn copy_from<S: CarStore>(mem: &mut S, handler: u32, src: u32) {
    for z in 0..at::ZONES {
        copy_word(mem, handler + at::ZONES_AT + at::ZONE_STRIDE * z, src + at::ZONES_AT + at::ZONE_STRIDE * z);
        copy_word(mem, handler + at::FLOATS_AT + 4 * z, src + at::FLOATS_AT + 4 * z);
    }
    for i in 0..at::TAIL_WORDS {
        copy_word(mem, handler + at::TAIL_AT + 4 * i, src + at::TAIL_AT + 4 * i);
    }
}

/// One word, read from `src` and written to `dst`.
fn copy_word<S: CarStore>(mem: &mut S, dst: u32, src: u32) {
    let value = mem.u32(src);
    mem.set_u32(dst, value);
}

/// Appends a functor entry to the damage-ops table: the function for `idx`, three zero words, then `b`, `a` and `c`, at
/// the table's count, and counts it. Returns false, and changes nothing, when the table is full, `idx` is outside 0..29,
/// or the original's switch leaves it out.
pub fn add_functor<S: CarStore>(mem: &mut S, ops: u32, idx: i32, a: u32, b: u32, c: u32) -> bool {
    let count = mem.u32(ops + at::FUNCTOR_COUNT);
    if count >= at::FUNCTOR_SLOTS || idx >= at::FUNCTORS.len() as i32 {
        return false;
    }
    let Some(function) = usize::try_from(idx).ok().and_then(|i| at::FUNCTORS[i]) else {
        return false;
    };
    let entry = ops + count * at::FUNCTOR_ENTRY;
    mem.set_u32(entry, function);
    for word in at::FUNCTOR_ZEROED_AT {
        mem.set_u32(entry + word, 0);
    }
    mem.set_u32(entry + at::FUNCTOR_B_AT, b);
    mem.set_u32(entry + at::FUNCTOR_A_AT, a);
    mem.set_u32(entry + at::FUNCTOR_C_AT, c);
    mem.set_u32(ops + at::FUNCTOR_COUNT, count + 1);
    true
}

/// The impact's damage scale: its length times 0.08.
pub fn impact_scale(length: f32) -> f32 {
    length * 0.08
}

/// The handler's random step: the seed's next value (a linear congruential step) and its draw, the remainder by 100001
/// (unsigned). Returns `(next, draw)`.
pub fn next_draw(seed: u32) -> (u32, u32) {
    let next = seed.wrapping_mul(0x0200_0081).wrapping_add(0xE623_F41D);
    (next, next % 100_001)
}

/// The draw as a fraction: the draw times 1e-5, rounded to 24 bits.
fn fraction(draw: u32) -> f32 {
    draw as f32 * 0.000_01
}

/// A random value between `lo` and `hi` for a draw (24-bit rounding): `fraction * (hi - lo) + lo`.
pub fn rand(draw: u32, lo: f32, hi: f32) -> f32 {
    fraction(draw) * (hi - lo) + lo
}

/// The spread factor of a functor's value: one ulp above 0.2.
const SPREAD: f32 = f32::from_bits(0x3E4C_CCD0);

/// A functor entry's value for a draw (24-bit rounding): `(fraction * spread + 0.9) * scale`.
pub fn functor_value(draw: u32, scale: f32) -> f32 {
    (fraction(draw) * SPREAD + 0.9) * scale
}

/// The keep test: a functor that returned non-zero becomes the hit when its weight is at least the best so far (a NaN
/// does not keep).
pub fn functor_keeps(weight: f32, best: f32) -> bool {
    weight >= best
}

/// A zone or table index the impact path uses: non-zero and below 64.
pub fn zone_index(idx: u32) -> bool {
    idx != 0 && idx < 0x40
}

/// The zone raise test: `value` above the zone's level (ordered: a NaN does not raise).
pub fn zone_raises(level: f32, value: f32) -> bool {
    value > level
}

/// The zone's damage float after a raise: `value` capped at 1.0 (1.0 for a NaN), or None (unchanged)
/// when the float is not below 1.0.
pub fn zone_float(float: f32, value: f32) -> Option<f32> {
    (1.0 > float).then_some(if 1.0 > value { value } else { 1.0 })
}

/// The gate of the engine damage (`min` 3) and of the ring store (`min` 2): the level (signed) at least `min`, `a > b`
/// (ordered) and the enable word set.
pub fn gate_open(level: u32, min: i32, a: f32, b: f32, enabled: u32) -> bool {
    level as i32 >= min && a > b && enabled != 0
}

/// The engine damage's fire test: the draw below 0.05.
pub fn engine_fires(r: f32) -> bool {
    0.05 > r
}

/// The ring store's next ring index: the current one plus one, modulo 10 (unsigned).
pub fn ring_next(cur: u32) -> u32 {
    cur.wrapping_add(1) % 10
}

/// The impact path's two constants: the engine damage's limit (11000.0) and the impact above which a hit emits (500.0). NGP scales both per car at load (`notes/ngp.md` "Force feedback", the "0.94195 group" factor): it
/// patches the push's immediate and repoints the compare to its own `1017130c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageTuning {
    pub engine_gate_limit: f32,
    pub emit_impact: f32,
}

impl DamageTuning {
    pub const STOCK: DamageTuning = DamageTuning { engine_gate_limit: 11000.0, emit_impact: 500.0 };
}

/// The impact's emit test: a raised zone or an impact above `emit_impact`.
pub fn impact_emits(raised: bool, dmg: f32, emit_impact: f32) -> bool {
    raised || dmg > emit_impact
}

/// The impact's peak test: the impact above the stored peak (ordered).
pub fn new_peak(peak: f32, dmg: f32) -> bool {
    dmg > peak
}

/// The impact's inner id: `a3`, or 0 when it is above 0xff (unsigned).
pub fn inner_id(a3: u32) -> u32 {
    if a3 > 0xFF {
        0
    } else {
        a3
    }
}

//! `CCarDamageHandler`: the impact scale, the zones, the functors, the engine gate, the speech ring, `Slot1` and the
//! damage functors (`Dmg*`).

use super::car::WORLD;
use super::damage_ctor::{DAMAGE_EMIT_RESET, DAMAGE_SIGNALS};
use crate::car_body::damage::{self, DamageTuning};
use crate::car_body::damage_functors::{self as functors, Event};
use crate::car_body::store::CarStore;
use crate::mem::Mem;

/// The call `ImpactScale` makes on the handler's slot 1 (`E36DamageOps::Slot1`, 0x4f12a0): thiscall with eight stack words.
pub trait ImpactCalls {
    /// `target` with `this` in ecx and `args` on the stack, in order; returns the callee's eax.
    fn call(&mut self, target: u32, this: u32, args: [u32; 8]) -> u32;
}

/// `CCarDamageHandler::ImpactScale(a1..a8)` (0x4ec140, slot 2, `ret 0x20`): calls the handler's slot 1 with the same
/// words, except that the length (`a4`) is scaled by 0.08. Returns the callee's eax.
pub fn impact_scale<S: CarStore>(calls: &mut impl ImpactCalls, mem: &S, this: u32, args: [u32; 8]) -> u32 {
    let target = mem.u32(mem.u32(this) + 4);
    let mut scaled = args;
    scaled[3] = damage::impact_scale(f32::from_bits(args[3])).to_bits();
    calls.call(target, this, scaled)
}

/// The calls the impact path (`Slot1`, `ApplyFunctors`, `EngineGate`, `RingStore`) makes out of the handler, each entered
/// at `sp` (its return slot, where the original's frame puts it).
pub trait DamageHitCalls {
    /// A functor entry's function: thiscall on `this` with the four stack words (`ret 0x10`); returns its eax.
    fn functor(&mut self, sp: u32, function: u32, this: u32, args: [u32; 4]) -> u32;
    /// `E01Vec4::CopyFrom(src)` (0x43c5b0) on `dst`.
    fn copy_vec4(&mut self, sp: u32, dst: u32, src: u32);
    /// `CMultiSignal1<T>::Emit(signal)` (the template at `emit`) on `emitter`.
    fn emit(&mut self, sp: u32, emit: u32, emitter: u32, signal: u32);
}

/// The handler's zones: 64 of 0x58 bytes from +0x2208, each with its engine share (+0), scale (+8), level (+0xc), four
/// spread links (+0x18: a factor, then the zone) and four functor links (+0x38: a factor, then the table).
const ZONE: u32 = 0x2208;
const ZONE_STRIDE: u32 = 0x58;
const ZONE_SCALE: u32 = 0x8;
const ZONE_LEVEL: u32 = 0xc;
const ZONE_SPREAD: u32 = 0x18;
const ZONE_FUNCTORS: u32 = 0x38;
const ZONE_LINKS: u32 = 4;
/// The zones' damage floats (+0x3808), their changed words (+0x3908) and the any-changed word (+0x3a08).
const DAMAGE_FLOATS: u32 = 0x3808;
const DAMAGE_CHANGED: u32 = 0x3908;
const DAMAGE_ANY_CHANGED: u32 = 0x3a08;
/// The functor tables: table `idx` at `8 + 0x88 * idx`, its entries 0x20 bytes each (the function at +0, the offset of
/// its `this` at +4, its words at +0x10, +0x14 (the weight) and +0x18 (the tag)) and its count at +0x80.
const FUNCTOR_TABLE: u32 = 0x8;
const FUNCTOR_TABLE_STRIDE: u32 = 0x88;
const FUNCTOR_TABLE_ENTRY: u32 = 0x20;
const FUNCTOR_TABLE_COUNT: u32 = 0x80;
/// The handler's seed, level and threshold.
const DAMAGE_SEED: u32 = 0x3de8;
const DAMAGE_LEVEL: u32 = 0x3df0;
const DAMAGE_THRESHOLD: u32 = 0x3dec;
/// The hit (`CSigCarDamage`'s payload from +0x3e60): the functor's result, the state (+0x3e64), the tag (+0x3e68), the
/// inner id (+0x3e6c), the impact and its second word (+0x3e70, +0x3e74), the two vectors and the floats' address.
const HIT: u32 = 0x3e60;
const HIT_STATE: u32 = 0x3e64;
const HIT_TAG: u32 = 0x3e68;
const HIT_INNER: u32 = 0x3e6c;
const HIT_IMPACT: u32 = 0x3e70;
const HIT_WORD: u32 = 0x3e74;
const HIT_VEC_A: u32 = 0x3e90;
const HIT_VEC_B: u32 = 0x3e80;
const HIT_FLOATS: u32 = 0x3ea0;
/// The world manager's flag word (+0x160cc) and its damage bit.
const MANAGER_FLAGS: u32 = 0x160cc;
const DAMAGE_ON: u32 = 0x80000;
/// `EngineGate`'s emit (`CSigEngineFire`, +0xb840); `Slot1` passes it the limit (`DamageTuning`), 1.0f and 0.
const ENGINE_FIRE_EMIT: u32 = 0x004e_f290;
const ENGINE_FIRE_EMITTER: u32 = 0xb840;
/// The speech ring (`RingStore`): ten pairs from +0x3e08 (a code, then the tag), the index at +0x3e58, and its emit
/// (`CSigCarDamageSpeech`, +0xcf48) with the code 8.
const RING: u32 = 0x3e08;
const RING_INDEX: u32 = 0x3e58;
const SPEECH_EMIT: u32 = 0x004a_e0d0;
const SPEECH_EMITTER: u32 = 0xcf48;
const SPEECH_CODE: u32 = 8;
/// `CSigCarDamage`'s emitter on the signal object.
const DAMAGE_EMITTER: u32 = 0x9f20;

/// Zone `idx`'s address (the asm's unchecked `imul`).
pub(super) fn zone(handler: u32, idx: u32) -> u32 {
    handler.wrapping_add(ZONE).wrapping_add(idx.wrapping_mul(ZONE_STRIDE))
}

/// Raises zone `idx` to `value` when that is above its level, and its damage float (capped at 1, `damage::zone_float`)
/// with the changed words. Returns whether it raised.
fn raise_zone<S: CarStore>(mem: &mut S, handler: u32, idx: u32, value: f32) -> bool {
    let level = zone(handler, idx) + ZONE_LEVEL;
    if !damage::zone_raises(mem.f32(level), value) {
        return false;
    }
    let word = handler.wrapping_add(4u32.wrapping_mul(idx));
    let old = mem.f32(word + DAMAGE_FLOATS);
    mem.set_f32(level, value);
    if let Some(new) = damage::zone_float(old, value) {
        mem.set_f32(word + DAMAGE_FLOATS, new);
        mem.set_u32(word + DAMAGE_CHANGED, 1);
        mem.set_u32(handler + DAMAGE_ANY_CHANGED, 1);
    }
    true
}

/// `CCarDamageHandler::ZoneScaled(idx, v)` (0x4edee0, `ret 8`): raises zone `idx` by its scale times `v`, then, until a
/// zone was raised, each spread link's zone by the link's factor times that value. The walk ends at the first null
/// link. Returns 1 when a zone was raised.
pub fn zone_scaled<S: CarStore>(mem: &mut S, handler: u32, idx: u32, v: f32) -> u32 {
    let at = zone(handler, idx);
    let value = mem.f32(at + ZONE_SCALE) * v;
    let mut raised = raise_zone(mem, handler, idx, value);
    for k in 0..ZONE_LINKS {
        let link = at + ZONE_SPREAD + 8 * k;
        let target = mem.u32(link + 4);
        if target == 0 {
            break;
        }
        if !raised {
            raised = raise_zone(mem, handler, target, mem.f32(link) * value);
        }
    }
    u32::from(raised)
}

/// `CCarDamageHandler::ApplyFunctors(idx, scale, best)` (0x4ec560, `ret 0xc`, entered at `sp`): each entry of table
/// `idx` in turn steps the seed and calls its function with the drawn value (`damage::functor_value`) and its words.
/// A non-zero result whose weight is at least the best (the float at `best`) becomes the hit (+0x3e60, its tag at
/// +0x3e68) and its weight the best. The entries and the count are read again after each call. Returns the count
/// (the asm's eax).
pub fn apply_functors<S: CarStore>(
    calls: &mut impl DamageHitCalls,
    mem: &mut S,
    handler: u32,
    sp: u32,
    idx: u32,
    scale: f32,
    best: u32,
) -> u32 {
    let table = handler.wrapping_add(FUNCTOR_TABLE).wrapping_add(idx.wrapping_mul(FUNCTOR_TABLE_STRIDE));
    let mut k = 0;
    while k < mem.u32(table + FUNCTOR_TABLE_COUNT) {
        let entry = table + FUNCTOR_TABLE_ENTRY * k;
        let (seed, draw) = damage::next_draw(mem.u32(handler + DAMAGE_SEED));
        mem.set_u32(handler + DAMAGE_SEED, seed);
        let args = [
            damage::functor_value(draw, scale).to_bits(),
            mem.u32(entry + 0x10),
            mem.u32(entry + 0x14),
            mem.u32(entry + 0x18),
        ];
        let this = handler.wrapping_add(mem.u32(entry + 4));
        let result = calls.functor(sp - 0x24, mem.u32(entry), this, args);
        if result != 0 && damage::functor_keeps(mem.f32(entry + 0x14), mem.f32(best)) {
            mem.set_u32(handler + HIT, result);
            mem.set_u32(handler + HIT_TAG, mem.u32(entry + 0x18));
            mem.set_u32(best, mem.u32(entry + 0x14));
        }
        k += 1;
    }
    mem.u32(table + FUNCTOR_TABLE_COUNT)
}

/// `E35_Rand(lo, hi, seed)` (0x4ebd70, cdecl): steps the seed at `seed` and returns the value in `lo..hi` and the
/// quotient the asm leaves in eax.
pub fn damage_rand<S: CarStore>(mem: &mut S, lo: f32, hi: f32, seed: u32) -> (f32, u32) {
    let (next, draw) = damage::next_draw(mem.u32(seed));
    mem.set_u32(seed, next);
    (damage::rand(draw, lo, hi), next / 100_001)
}

/// `E36DamageOps::EngineGate(a, b, c, d)` (0x4ef680, `ret 0x10`, entered at `sp`): at level 3 or more, with `a > b`
/// and the windshield word (+0x3b20) set, clears it, +0x3a94 and +0x3c00, sets +0x3b24, draws from the seed, and below
/// 0.05 emits `CSigEngineFire` with 1.0 (written to its first argument's slot, as the asm). Returns 5 when the gate
/// opens, else 0.
pub fn engine_gate<S: CarStore>(
    calls: &mut impl DamageHitCalls,
    mem: &mut S,
    handler: u32,
    sp: u32,
    a: f32,
    b: f32,
) -> u32 {
    let level = mem.u32(handler + DAMAGE_LEVEL);
    if !damage::gate_open(level, 3, a, b, mem.u32(handler + 0x3b20)) {
        return 0;
    }
    mem.set_u32(handler + 0x3b20, 0);
    mem.set_u32(handler + 0x3a94, 0);
    mem.set_u32(handler + 0x3c00, 0);
    mem.set_u32(handler + 0x3b24, 1);
    let (r, _) = damage_rand(mem, 0.0, 1.0, handler + DAMAGE_SEED);
    if damage::engine_fires(r) {
        let payload = sp + 4;
        mem.set_f32(payload, 1.0);
        let emitter = mem.u32(DAMAGE_SIGNALS) + ENGINE_FIRE_EMITTER;
        calls.emit(sp - 8, ENGINE_FIRE_EMIT, emitter, payload);
    }
    5
}

/// `E36DamageOps::RingStore(a, b, c, tag)` (0x4ef5f0, the speech functor, `ret 0x10`, entered at `sp`): at level 2 or
/// more, with `a > b` and the speech word (+0x3a90) set, clears it, steps the ring's index, stores the code 2 and `tag`
/// in its pair and emits `CSigCarDamageSpeech` with the code 8 (written to its first argument's slot, as the asm).
/// Returns 0.
pub fn ring_store<S: CarStore>(
    calls: &mut impl DamageHitCalls,
    mem: &mut S,
    handler: u32,
    sp: u32,
    args: [u32; 4],
) -> u32 {
    let [a, b, _, tag] = args;
    let level = mem.u32(handler + DAMAGE_LEVEL);
    let open = damage::gate_open(level, 2, f32::from_bits(a), f32::from_bits(b), mem.u32(handler + 0x3a90));
    if open {
        mem.set_u32(handler + 0x3a90, 0);
        let index = damage::ring_next(mem.u32(handler + RING_INDEX));
        mem.set_u32(handler + RING_INDEX, index);
        mem.set_u32(handler + RING + 8 * index, 2);
        let index = mem.u32(handler + RING_INDEX);
        mem.set_u32(handler + RING + 4 + 8 * index, tag);
        let payload = sp + 4;
        let emitter = mem.u32(DAMAGE_SIGNALS) + SPEECH_EMITTER;
        mem.set_u32(payload, SPEECH_CODE);
        calls.emit(sp - 0xc, SPEECH_EMIT, emitter, payload);
    }
    0
}

/// `E36DamageOps::Slot1(a1, a2, a3, dmg, a5, src_a, src_b, _)` (0x4f12a0, the handler's slot 1, `ret 0x20`, entered at
/// `sp`): the impact of `dmg` on zone `a2`. With damage on (the manager's flag), the zone is raised (`zone_scaled`), and
/// with a threshold above 0 the functor tables run with the threshold times `dmg`: table `a1`, then the zone's four
/// functor links (each by its factor), the best weight a local at `sp - 8`; then `engine_gate` with the zone's engine
/// share. A raised zone or an impact above `tuning.emit_impact` stores the hit and emits `CSigCarDamage`. Last the
/// handler's last zone and table, its last impact and peak, and its inner id (`a3`, 0 above 0xff). Returns the asm's
/// eax (0xff).
pub fn damage_slot1<S: CarStore>(
    calls: &mut impl DamageHitCalls,
    mem: &mut S,
    handler: u32,
    sp: u32,
    args: [u32; 8],
    tuning: DamageTuning,
) -> u32 {
    let [a1, a2, a3, impact, a5, src_a, src_b, _] = args;
    let dmg = f32::from_bits(impact);
    mem.set_u32(handler + HIT, 0);
    let manager = mem.u32(mem.u32(WORLD) + 0x24);
    let mut raised = false;
    if mem.u32(manager + MANAGER_FLAGS) & DAMAGE_ON != 0 {
        if damage::zone_index(a2) {
            raised = zone_scaled(mem, handler, a2, dmg) != 0;
        }
        let threshold = mem.f32(handler + DAMAGE_THRESHOLD);
        if threshold > 0.0 {
            let best = sp - 8;
            mem.set_f32(best, 0.0);
            let scaled = threshold * dmg;
            mem.set_u32(handler + HIT, 0);
            if damage::zone_index(a1) {
                apply_functors(calls, mem, handler, sp - 0x2c, a1, scaled, best);
            }
            let at = zone(handler, a2);
            for k in 0..ZONE_LINKS {
                let link = at + ZONE_FUNCTORS + 8 * k;
                let table = mem.u32(link + 4);
                if damage::zone_index(table) {
                    let value = mem.f32(link) * scaled;
                    apply_functors(calls, mem, handler, sp - 0x2c, table, value, best);
                }
            }
            let share = mem.f32(at) * scaled;
            engine_gate(calls, mem, handler, sp - 0x30, share, tuning.engine_gate_limit);
        }
    }
    if damage::impact_emits(raised, dmg, tuning.emit_impact) {
        mem.set_u32(handler + HIT_IMPACT, impact);
        mem.set_u32(handler + HIT_STATE, 2);
        mem.set_u32(handler + HIT_WORD, a5);
        mem.set_u32(handler + HIT_INNER, a3);
        calls.copy_vec4(sp - 0x24, handler + HIT_VEC_A, src_a);
        calls.copy_vec4(sp - 0x24, handler + HIT_VEC_B, src_b);
        mem.set_u32(handler + HIT_FLOATS, handler + DAMAGE_FLOATS);
        let emitter = mem.u32(DAMAGE_SIGNALS) + DAMAGE_EMITTER;
        calls.emit(sp - 0x24, DAMAGE_EMIT_RESET, emitter, handler + HIT);
    }
    if a1 != 0 {
        mem.set_u32(handler + 0x3df4, a1);
    }
    if a2 != 0 {
        mem.set_u32(handler + 0x3df8, a2);
    }
    let peak = mem.f32(handler + 0x3e00);
    mem.set_u32(handler + 0x3dfc, impact);
    if damage::new_peak(peak, dmg) {
        mem.set_u32(handler + 0x3e00, impact);
    }
    mem.set_u32(handler + 0x3e04, damage::inner_id(a3));
    0xff
}

/// The handler's damage level (+0x3df0), the ring's cursor (+0x3e58) and its entries (+0x3e08, `kind` then `value`).
const LEVEL: u32 = 0x3DF0;
const RING_CURSOR: u32 = 0x3E58;
const RING_AT: u32 = 0x3E08;
const RING_LEN: u32 = 10;

/// The word at `base + stride * i` of the handler (the functors index their tables with any `i`, as the asm does).
fn indexed(this: u32, base: u32, stride: u32, i: u32) -> u32 {
    this.wrapping_add(base).wrapping_add(i.wrapping_mul(stride))
}

/// `CCarDamageHandler`'s ring push (the asm's `head = (head + 1) % 10`, then the entry at the new head).
fn ring_push<S: CarStore>(mem: &mut S, this: u32, event: Event) {
    let cursor = mem.u32(this + RING_CURSOR).wrapping_add(1) % RING_LEN;
    mem.set_u32(this + RING_CURSOR, cursor);
    mem.set_u32(this + RING_AT + 8 * cursor, event.kind);
    mem.set_u32(this + RING_AT + 8 * cursor + 4, event.value);
}

/// The four stack words as floats (`a`, `b`, `c`) and the fourth as it is (`tag` or the index).
fn floats3([a, b, c, _]: [u32; 4]) -> (f32, f32, f32) {
    (f32::from_bits(a), f32::from_bits(b), f32::from_bits(c))
}

/// `CCarDamageHandler::Dmg660` (0x4ec660, thiscall with four stack words, `ret 0x10`): the wear word +0x3b00.
pub fn gearbox_wear<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, c) = floats3(args);
    let word = mem.f32(this + 0x3B00);
    if let Some(pass) = functors::wear(mem.u32(this + LEVEL) as i32, a, b, c, word, args[3]) {
        mem.set_f32(this + 0x3B00, pass.write);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::Dmg720` (0x4ec720): the cooling-wear word (+0x3b10) grows by `(a - b) * c`, capped at 1.0.
pub fn grow_cooling_wear<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, c) = floats3(args);
    let word = mem.f32(this + 0x3B10);
    if let Some(pass) = functors::cooling_wear(a, b, c, word, args[3]) {
        mem.set_f32(this + 0x3B10, pass.write);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgA00` (0x4eca00) (name ours): the zone float of wheel `args[3]` (+0x3c08 + 0x38 i) takes the scaled step.
pub fn grow_zone<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, c) = floats3(args);
    let zones = std::array::from_fn(|k| mem.f32(this + 0x3C08 + 0x38 * k as u32));
    if let Some(pass) = functors::zone(&zones, args[3], a, b, c) {
        let (k, value) = pass.write;
        mem.set_f32(this + 0x3C08 + 0x38 * k as u32, value);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgCF20` (0x4ecf20) (name ours): the triple's first word (+0x3a30 + 0xc i) clears, and the flag +0x3a88 is set.
pub fn clear_triple<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let i = args[3];
    let at = indexed(this, 0x3A30, 0xC, i);
    if let Some(pass) = functors::triple(a, b, mem.u32(at), i) {
        mem.set_u32(this + 0x3A88, 1);
        mem.set_u32(at, 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgCFE0` (0x4ecfe0) (name ours): the windshield word (+0x3dc4 + 4i) clears when it is 1; returns 4 on a pass.
pub fn clear_windshield<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let at = indexed(this, 0x3DC4, 4, args[3]);
    if functors::windshield(a, b, mem.u32(at)) {
        mem.set_u32(at, 0);
        return 4;
    }
    0
}

/// `CCarDamageHandler::DmgCFA0` (0x4ecfa0) (name ours): the head gasket word (+0x3dd8 + 4i) clears when it is 1; returns 3 on a pass.
pub fn clear_head_gasket<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let at = indexed(this, 0x3DD8, 4, args[3]);
    if functors::head_gasket(mem.u32(this + LEVEL) as i32, a, b, mem.u32(at)) {
        mem.set_u32(at, 0);
        return 3;
    }
    0
}

/// `CCarDamageHandler::Dmg9A0` (0x4ec9a0): the flag +0x3a98 clears.
pub fn dmg_9a0<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    if let Some(pass) = functors::clear_flag(a, b, mem.u32(this + 0x3A98), 0xD, args[3]) {
        mem.set_u32(this + 0x3A98, 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::Dmg7B0` (0x4ec7b0): the pair's word (+0x3a1c + 8 i) for i below 3 clears.
pub fn dmg_7b0<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let words = std::array::from_fn(|k| mem.u32(this + 0x3A1C + 8 * k as u32));
    if let Some(pass) = functors::clear_pair_word(a, b, &words, args[3]) {
        mem.set_u32(this + 0x3A1C + 8 * args[3], 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgAA0` (0x4ecaa0): the triple's second word (+0x3d88 + 0xc i) clears.
pub fn dmg_aa0<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let i = args[3];
    let at = indexed(this, 0x3D88, 0xC, i);
    let first = mem.u32(indexed(this, 0x3D84, 0xC, i));
    if let Some(pass) = functors::door(a, b, first, mem.u32(at), i) {
        mem.set_u32(at, 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgB20` (0x4ecb20): the triple's first word (+0x3d84 + 0xc i) clears, on the speed.
pub fn dmg_b20<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let i = args[3];
    let at = indexed(this, 0x3D84, 0xC, i);
    let threshold = mem.f32(this + 0x3DEC);
    if let Some(pass) = functors::door_speed(threshold, a, b, mem.u32(at), mem.u32(this + LEVEL) as i32, i) {
        mem.set_u32(at, 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::Dmg890` (0x4ec890) (name ours): the word +0x3b08 falls.
pub fn drain_word<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, c) = floats3(args);
    let word = mem.f32(this + 0x3B08);
    if let Some(pass) = functors::drain(mem.u32(this + LEVEL) as i32, a, b, c, word, args[3]) {
        mem.set_f32(this + 0x3B08, pass.write);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::Dmg930` (0x4ec930): the flag +0x3af4 clears at level 2 or more.
pub fn dmg_930<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let flag = mem.u32(this + 0x3AF4);
    if let Some(pass) = functors::clear_flag_level2(mem.u32(this + LEVEL) as i32, a, b, flag, 0xF, args[3]) {
        mem.set_u32(this + 0x3AF4, 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::Dmg820` (0x4ec820): the flag +0x3a94 clears at level 2 or more.
pub fn dmg_820<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let flag = mem.u32(this + 0x3A94);
    if let Some(pass) = functors::clear_flag_level2(mem.u32(this + LEVEL) as i32, a, b, flag, 0xB, args[3]) {
        mem.set_u32(this + 0x3A94, 0);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgCEB0` (0x4eceb0) (name ours): the word +0x3a9c caps at 0.3.
pub fn cap_word<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let word = mem.f32(this + 0x3A9C);
    if let Some(pass) = functors::cap_03(mem.u32(this + LEVEL) as i32, a, b, word, args[3]) {
        mem.set_f32(this + 0x3A9C, pass.write);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgBB0` (0x4ecbb0): the pair (+0x3b60 + 8 i) clears, the flag +0x3b2c is set; returns 1 on a pass.
pub fn dmg_bb0<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    clear_pair(mem, this, args, 0x3B60, 0x3B2C, 0x13)
}

/// `CCarDamageHandler::DmgCC30` (0x4ecc30): the pair (+0x3b40 + 8 i) clears, the flag +0x3b28 is set; returns 1 on a pass.
pub fn dmg_cc30<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    clear_pair(mem, this, args, 0x3B40, 0x3B28, 0x14)
}

/// `CCarDamageHandler::DmgCCB0` (0x4eccb0): the suspension-arm pair (+0x3ba0 + 8 i) clears; returns 2 on a pass.
pub fn fail_suspension_arm<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    clear_pair_with(mem, this, args, 0x3BA0, 0x3B30, functors::suspension_arm, 2)
}

/// `CCarDamageHandler::DmgCD30` (0x4ecd30): the steering-rod pair (+0x3b80 + 8 i) clears; returns 2 on a pass.
pub fn fail_steering_rod<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    clear_pair_with(mem, this, args, 0x3B80, 0x3B3C, functors::steering_rod, 2)
}

/// `CCarDamageHandler::DmgCDB0` (0x4ecdb0): the hub pair (+0x3bc0 + 8 i) clears; returns 2 on a pass.
pub fn fail_hub<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    clear_pair_with(mem, this, args, 0x3BC0, 0x3B34, functors::hub, 2)
}

/// `CCarDamageHandler::DmgCE30` (0x4ece30): the brake-disc pair (+0x3be0 + 8 i) clears; returns 1 on a pass.
pub fn fail_brake_disc<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    clear_pair_with(mem, this, args, 0x3BE0, 0x3B38, functors::brake_disc, 1)
}

/// The pair functors: both words of the pair at `base + 8 i` clear, the flag at `flag` is set, and the asm returns 1.
fn clear_pair<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4], base: u32, flag: u32, kind: u32) -> u32 {
    let (a, b, _) = floats3(args);
    let i = args[3];
    let (first_at, second_at) = (indexed(this, base, 8, i), indexed(this, base + 4, 8, i));
    let (first, second) = (mem.u32(first_at), mem.u32(second_at));
    let Some(pass) = functors::clear_pair(a, b, first, second, kind, i) else {
        return 0;
    };
    mem.set_u32(first_at, 0);
    mem.set_u32(second_at, 0);
    mem.set_u32(this + flag, 1);
    ring_push(mem, this, pass.event);
    1
}

/// The pair-failure layout for functors with their own pure pass rule.
fn clear_pair_with<S: CarStore>(
    mem: &mut S,
    this: u32,
    args: [u32; 4],
    base: u32,
    flag: u32,
    pass: fn(f32, f32, u32, u32, u32) -> Option<functors::Pass<()>>,
    result: u32,
) -> u32 {
    let (a, b, _) = floats3(args);
    let i = args[3];
    let (first_at, second_at) = (indexed(this, base, 8, i), indexed(this, base + 4, 8, i));
    let (first, second) = (mem.u32(first_at), mem.u32(second_at));
    let Some(event) = pass(a, b, first, second, i) else {
        return 0;
    };
    mem.set_u32(first_at, 0);
    mem.set_u32(second_at, 0);
    mem.set_u32(this + flag, 1);
    ring_push(mem, this, event.event);
    result
}

/// `CCarDamageHandler::DmgD0F0` (0x4ed0f0): the water-pump flag (+0x3aa8) clears; returns 5 on a pass.
pub fn fail_water_pump<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    if let Some(pass) = functors::water_pump(a, b, mem.u32(this + 0x3AA8), args[3]) {
        mem.set_u32(this + 0x3AA8, 0);
        ring_push(mem, this, pass.event);
        return 5;
    }
    0
}

/// `CCarDamageHandler::DmgD160` (0x4ed160): signed level 2 or more grows the intercooler-wear word (+0x3b0c), capped at 1.
pub fn grow_intercooler_wear<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, c) = floats3(args);
    let level = mem.u32(this + LEVEL) as i32;
    let word = mem.f32(this + 0x3B0C);
    if let Some(pass) = functors::intercooler_wear(level, a, b, c, word, args[3]) {
        mem.set_f32(this + 0x3B0C, pass.write);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgD020` (0x4ed020): the word +0x3aa0 falls, and the flag +0x3a88 is set; returns 5 on a pass.
pub fn dmg_d020<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, c) = floats3(args);
    let word = mem.f32(this + 0x3AA0);
    let Some(pass) = functors::drop_to_floor(mem.u32(this + LEVEL) as i32, a, b, c, word, args[3]) else {
        return 0;
    };
    mem.set_u32(this + 0x3A88, 1);
    mem.set_f32(this + 0x3AA0, pass.write);
    ring_push(mem, this, pass.event);
    5
}

/// `CCarDamageHandler::DmgD200` (0x4ed200) (name ours): the mounted body's state word (+0x3db4 + 4 i) moves to 1 or 2 on the speed.
pub fn set_body_state<S: CarStore>(mem: &mut S, this: u32, args: [u32; 4]) -> u32 {
    let (a, b, _) = floats3(args);
    let i = args[3];
    let at = indexed(this, 0x3DB4, 4, i);
    let threshold = mem.f32(this + 0x3DEC);
    if let Some(pass) = functors::state(threshold, a, b, mem.u32(at), i) {
        mem.set_u32(at, pass.write);
        ring_push(mem, this, pass.event);
    }
    0
}

/// `CCarDamageHandler::DmgD300` (0x4ed300): ignores the handler and stack words and returns zero.
pub fn dmg_d300(_mem: &mut Mem, _this: u32, _args: [u32; 4]) -> u32 {
    functors::zero()
}

/// `NPhysics::E35_Zero2F0` (0x4ed2f0): ignores the handler and stack words and returns zero.
pub fn zero2f0(_mem: &mut Mem, _this: u32, _args: [u32; 4]) -> u32 {
    functors::zero2f0()
}

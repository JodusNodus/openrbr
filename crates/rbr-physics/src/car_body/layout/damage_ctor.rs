//! The damage objects' constructors and tables: `E36DamageOps`, `E60DamageHost`, the functor and threshold tables
//! and `Slot4`'s level fractions.

use crate::car_body::damage::ResetDamageCalls;
use crate::car_body::damage_ops::{self, FunctorAppend, BOUND_APPENDS, HANDLER_FILLS, HANDLER_ONES, INLINE_APPENDS};
use crate::car_body::vf28x::{VEC4_CLEAR, VEC4_COPY_FROM};
use crate::mem::Mem;
pub(super) use crate::physics_core::layout::SIGNAL_MANAGER as DAMAGE_SIGNALS;
use crate::physics_core::mounted;
use crate::{engine_driveline, entities, spring_damper, tyre};

/// The `E36DamageOps` object constructed by `CCar::SetPtr3CC` (exe 0x4ef410). The constructor initializes the
/// damage zones, calls the fixed E35 threshold-table initializer (0x4ed370), and returns the same object.
pub const DAMAGE_OPS_SIZE: u32 = 0x3eb0;
const DAMAGE_OPS_VTABLE: u32 = 0x0073_9214;

/// `CCarDamageHandler::Ctor` / `E36DamageOps::Ctor` (0x4ef410), on the newly allocated handler.
pub fn construct_damage_ops(mem: &mut Mem, this: u32) -> u32 {
    mem.set_u32(this, DAMAGE_OPS_VTABLE);
    for k in 1..=0x40 {
        mem.set_u32(this + 0x88 * k, 0);
        mem.set_u32(this + 0x8c + 0x88 * (k - 1), 0);
    }
    for zone in 0..0x40 {
        let at = this + 0x2208 + 0x58 * zone;
        mem.set_f32(at, 0.5);
        for pair in [0x18, 0x38] {
            for word in 0..4 {
                mem.set_u32(at + pair + 8 * word, 0);
                mem.set_u32(at + pair + 8 * word + 4, 0);
            }
        }
    }
    mem.set_u32(this + 0x3ea4, 0x40);
    mem.set_u32(this + 0x3ea0, 0);
    mem.set_f32(this + 0x3dec, 1.0);
    mem.set_u32(this + 0x3df0, 3);
    for zone in 0..0x40 {
        let at = this + 0x2210 + 0x58 * zone;
        mem.set_f32(at, f32::from_bits(0x3a03_126f));
        mem.set_u32(at + 4, 0);
        mem.set_u32(at + 8, 0);
        mem.set_u32(at + 0xc, 0);
        mem.set_f32(this + 0x3808 + 4 * zone, 0.0);
    }
    for offset in [0x3de8, 0x3df4, 0x3df8, 0x3e04] {
        mem.set_u32(this + offset, 0);
    }
    mem.set_f32(this + 0x3e00, 0.0);
    mem.set_f32(this + 0x3dfc, 0.0);
    mem.set_u32(this + 0x3aac, this + 0x3de8);
    for pair in 0..10 {
        mem.set_u32(this + 0x3e08 + 8 * pair, 0x1d);
        mem.set_u32(this + 0x3e0c + 8 * pair, 0);
    }
    mem.set_u32(this + 0x3e58, 0);
    init_damage_handler_tables(mem, this);
    this
}

/// `E60DamageHost`'s size (`E60Flow::M569270` allocates it): a header, four handlers, two more handlers and a tail.
pub const DAMAGE_HOST_SIZE: u32 = 0x17860;
/// The host's first four handlers (`DAMAGE_OPS_SIZE` apart) and its two others.
const DAMAGE_HOST_SLOTS: u32 = 0x10;
const DAMAGE_HOST_OTHERS: [u32; 2] = [0xfad0, 0x13980];
/// A handler's index word and the two call counters after it (`ResetDamage`'s at +0x3a10, `Set3FB0`'s at +0x3a14).
const DAMAGE_INDEX: u32 = 0x3a0c;
/// The other handlers' index and counters: 999.
const DAMAGE_OTHER_INDEX: u32 = 999;
/// The host's tail: a word per slot (4 each) from +0x17840.
const DAMAGE_HOST_SLOT_WORDS: u32 = 0x17840;

/// `E60DamageHost::Ctor` (0x566840) on the block at `this`: the header cleared, the four handlers by the vector
/// iterator then the two others (`E36DamageOps::Ctor`), then each slot's index (0 to 3) with its counters 0 and its
/// tail word 4, the others' index and counters 999, and the tail's words cleared. Returns `this`.
pub fn construct_damage_host(mem: &mut Mem, this: u32) -> u32 {
    // The two 16-bit words at +0 and +2, then two words.
    mem.set_u32(this, 0);
    mem.set_u32(this + 4, 0);
    mem.set_u32(this + 8, 0);
    let slot = |k: u32| this + DAMAGE_HOST_SLOTS + DAMAGE_OPS_SIZE * k;
    for k in 0..4 {
        construct_damage_ops(mem, slot(k));
    }
    for at in DAMAGE_HOST_OTHERS {
        construct_damage_ops(mem, this + at);
    }
    mem.set_u32(this + 0x17830, 0);
    mem.set_u32(this + 0x17834, 0);
    mem.set_u8(this + 0x17838, 0);
    for k in 0..4 {
        mem.set_u32(this + DAMAGE_HOST_SLOT_WORDS + 4 * k, 4);
        for (word, value) in [k, 0, 0].into_iter().enumerate() {
            mem.set_u32(slot(k) + DAMAGE_INDEX + 4 * word as u32, value);
        }
    }
    for at in DAMAGE_HOST_OTHERS {
        for word in 0..3 {
            mem.set_u32(this + at + DAMAGE_INDEX + 4 * word, DAMAGE_OTHER_INDEX);
        }
    }
    mem.set_u32(this + 0x1785c, 0);
    this
}

/// `ResetDamage`'s member calls (`damage::reset_damage`), run by their ports.
pub struct ResetPorts;

impl ResetDamageCalls for ResetPorts {
    fn call(&mut self, mem: &mut Mem, callee: u32, this: u32) {
        use crate::car_body::damage::*;
        match callee {
            STATE_INIT_C70 => engine_driveline::layout::state_init_c70(mem, this),
            ENGINE_RESET_SUB78 => engine_driveline::layout::reset_sub78(mem, this),
            WHEEL_ZERO => crate::wheel::layout::damage_zero(mem, this),
            VALUE8_CLEAR => tyre::layout::value8_clear(mem, this),
            VALUE_CLEAR => spring_damper::layout::clear_unsigned_value(mem, this),
            FILL_ONES => entities::layout::fill_ones(mem, this),
            M1640 => mounted::clear_state(mem, this),
            _ => unreachable!("ResetDamage calls no other member ({callee:#x})"),
        }
    }
}

/// The first-free-id fill used by the handler's fixed tables. Each entry has a float value and an id, with four entries
/// per table; an occupied table leaves that fill untouched as in the original's scan.
pub fn init_damage_handler_tables(mem: &mut Mem, this: u32) {
    for fill in HANDLER_FILLS {
        for slot in 0..4 {
            let id_at = this + fill.table + 4 + 8 * slot;
            if mem.u32(id_at) == 0 {
                mem.set_u32(id_at, fill.id);
                mem.set_u32(this + fill.table + 8 * slot, fill.value);
                break;
            }
        }
    }
    for &offset in HANDLER_ONES {
        mem.set_f32(this + offset, 1.0);
    }
}

/// One inlined `E36_TableAppend` store group in `E36DamageOps::InitTables` (0x4ef910).
fn append_damage_functor(mem: &mut Mem, this: u32, append: FunctorAppend) {
    let count_at = this + append.table + 0x80;
    let count = mem.u32(count_at);
    let entry = this + append.table + 0x20 * count;
    mem.set_u32(entry, append.function);
    mem.set_u32(entry + 4, 0);
    mem.set_u32(entry + 8, 0);
    mem.set_u32(entry + 0xc, 0);
    mem.set_u32(entry + 0x10, append.value);
    mem.set_u32(entry + 0x14, append.scale);
    mem.set_u32(entry + 0x18, append.index);
    mem.set_u32(count_at, count + 1);
}

const DAMAGE_ADD_FUNCTOR: u32 = 0x004e_f720;
const DAMAGE_INIT_TABLES: u32 = 0x004e_f910;

/// The E36 functor and threshold tables (0x4ef910), including its 13 calls back to AddFunctor.
pub fn init_damage_ops_tables(calls: &mut impl DamageOpsCalls, mem: &mut Mem, this: u32) {
    for k in 1..=0x40 {
        mem.set_u32(this + 0x88 * k, 0);
    }
    for &append in INLINE_APPENDS {
        append_damage_functor(mem, this, append);
    }
    for call in BOUND_APPENDS {
        calls.call(mem, DAMAGE_ADD_FUNCTOR, this + call.table, &[call.index, call.scale, call.value, call.tag]);
    }
}

/// Calls `Slot4` makes out to signal and vector helpers.
pub trait DamageOpsCalls {
    fn call(&mut self, mem: &mut Mem, callee: u32, this: u32, args: &[u32]);
}

const DAMAGE_EMIT_INIT: u32 = 0x004e_f350;
pub(super) const DAMAGE_EMIT_RESET: u32 = 0x004e_f1d0;

/// `E36DamageOps::Slot4` (0x4f1000): rebuild the functor tables, clamp all 64 damage scalars, select the level fractions,
/// clear and copy its two signal vectors, emit the reset, then clear the per-zone change flags.
pub fn damage_level_fractions(calls: &mut impl DamageOpsCalls, mem: &mut Mem, this: u32, scratch: u32) {
    calls.call(mem, DAMAGE_INIT_TABLES, this, &[]);
    mem.set_u32(this + 0x3aac, this + 0x3de8);
    let fractions = damage_ops::level_fractions(mem.u32(this + 0x3df0));
    for at in [0x3c38, 0x3c70, 0x3ca8, 0x3ce0] {
        mem.set_f32(this + at, fractions.wheel);
    }
    for at in [0x3ce4, 0x3cec, 0x3cf4, 0x3cfc] {
        mem.set_f32(this + at, fractions.body);
    }
    mem.set_u32(this + 0x3ab0, fractions.level_three);
    mem.set_f32(this + 0x3a8c, fractions.scale);
    for zone in 0..0x40 {
        let value = mem.f32(this + 0x2214 + 0x58 * zone);
        mem.set_f32(this + 0x3808 + 4 * zone, damage_ops::clamp_damage(value));
    }

    let signals = mem.u32(DAMAGE_SIGNALS);
    let vector = scratch + 4;
    mem.set_u32(scratch, this);
    calls.call(mem, DAMAGE_EMIT_INIT, signals + 0xddf0, &[scratch]);
    calls.call(mem, VEC4_CLEAR, vector, &[]);
    for offset in [0x3e68, 0x3e60, 0x3e64, 0x3e6c] {
        mem.set_u32(this + offset, 0);
    }
    mem.set_f32(this + 0x3e70, 0.0);
    mem.set_f32(this + 0x3e74, 0.0);
    calls.call(mem, VEC4_COPY_FROM, this + 0x3e90, &[vector]);
    calls.call(mem, VEC4_COPY_FROM, this + 0x3e80, &[vector]);
    mem.set_u32(this + 0x3ea0, this + 0x3808);
    calls.call(mem, DAMAGE_EMIT_RESET, signals + 0x9f20, &[this + 0x3e60]);
    mem.set_u32(this + 0x3a08, 0);
    for zone in 0..0x40 {
        mem.set_u32(this + 0x3908 + 4 * zone, 0);
    }
}

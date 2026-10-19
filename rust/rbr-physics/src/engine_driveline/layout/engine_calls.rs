//! `CEngine`'s small calls: the option and state getters and setters, the torque curve, the slot roll and the
//! abs-rpm and compliance terms.

use super::drive_objects::Lane;
use super::engine_objects::{EngineDef, EngineObjects, EngineOptions, EngineState, EngineThis, EngineTuning};
use crate::engine_driveline::store::EngineStore;
use crate::engine_driveline::{engine, sim};
use crate::math::trig::one_minus_x87_cos;
use crate::ntweak::construct_base;
use crate::ntweak::layout::REAL_VTABLE;
use crate::physics_core::layout::PHYSICS_GLOBAL;
use crate::spring_damper::{self, WeightedValue};
use crate::Variant;

/// `E41Plain::InitRealLerp` (0x50f010, cdecl, one buffer argument): the buffer becomes a `CReal` holding the lerp
/// `(b - a) * t + a` at +0xc. Its header is `NTweak::CBase(0)` (name 0, dirty 0) with the real vtable, not the
/// `CReal` constructor's name. The three floats are the caller's frame words. Returns the buffer (eax).
pub fn init_real_lerp<S: EngineStore>(mem: &mut S, buf: u32, a: f32, b: f32, t: f32) -> u32 {
    construct_base(mem.mem_mut(), buf, 0);
    mem.set_u32(buf, REAL_VTABLE);
    mem.set_f32(buf + 0xc, (b - a) * t + a);
    buf
}

/// `CEngine::SimEC40` (0x50ec40) at the exhaust row `i`, with the twelve words at `q` as its inputs.
pub fn exhaust_blend_at<S: EngineStore>(mem: &S, this: u32, i: u32, q: u32, variant: Variant) -> f64 {
    let def = EngineObjects::at(mem, this).def;
    let row = mem.exhaust_row(def, i);
    let s: [f32; 12] = mem.f32s(q);
    engine::exhaust_blend(&row, &s, variant)
}

/// `CEngine::SimEBB0` (0x50ebb0, thiscall): the lane at `this+0x24` takes `1 - cos(angle)` and is set dirty (the
/// angle is `engine::ebb0_angle` of the engine's value and the definition's floor).
pub fn sim_ebb0<S: EngineStore>(mem: &mut S, this: u32) {
    let objects = EngineObjects::at(mem, this);
    let angle = engine::ebb0_angle(mem.f32(this + 0x20), mem.f32(objects.def + 0x2708));
    let value = one_minus_x87_cos(angle);
    Lane { dirty: 1, value }.store(mem, this + 0x24);
}

/// `CEngine::Set48` (0x50d310, thiscall `(v)`): the options pointer at `+0x48` takes `v` (name ours). Returns eax, which the asm
/// leaves as `v`.
pub fn set_options<S: EngineStore>(mem: &mut S, this: u32, v: u32) -> u32 {
    mem.set_u32(this + 0x48, v);
    v
}

/// `CEngine::SetState4` (0x50d340, thiscall `(a, b, c, d)`): the four words at the state's `+0x168`..`+0x174`. Returns
/// eax, which the asm leaves as the state pointer.
pub fn set_cut_flags<S: EngineStore>(mem: &mut S, this: u32, words: [u32; 4]) -> u32 {
    let state = mem.u32(this + 8);
    for (k, w) in words.into_iter().enumerate() {
        mem.set_u32(state + 0x168 + 4 * k as u32, w);
    }
    state
}

/// `CEngine::Get48_58` (0x50d380, thiscall, x87 float): the float at `+0x58` of the options block, or 0 when the
/// block's pointer at `+0x48` is 0.
pub fn options_gate_58<S: EngineStore>(mem: &S, this: u32) -> f32 {
    let o = mem.u32(this + 0x48);
    if o != 0 {
        mem.f32(o + 0x58)
    } else {
        0.0
    }
}

/// `CEngine::SimE970` (0x50e970, thiscall, x87 float): `DamperBiasSub` (0x52e390) of the state's float at `+0x28`.
pub fn sim_e970<S: EngineStore>(mem: &S, this: u32, variant: Variant) -> f64 {
    spring_damper::bias_sub(mem.f32(mem.u32(this + 8) + 0x28), variant)
}

/// `CEngine::SimE980` (0x50e980, thiscall): 1 when the state's float at `+0x188` is above 40 (a NaN gives 0), else 0.
pub fn rpm_above_40<S: EngineStore>(mem: &S, this: u32) -> u32 {
    u32::from(mem.f32(mem.u32(this + 8) + 0x188) > 40.0)
}

/// `CEngine::SimE9A0` (0x50e9a0, thiscall): under the keeper's flag `0x80000` it runs three `DamperBiasSub`s whose
/// results are popped, so nothing is written. Returns eax, which the asm leaves as the state pointer when the flag is
/// set and as the world object's pointer (the global at `0x1659b70`) when it is not.
pub fn sim_e9a0<S: EngineStore>(mem: &S, this: u32) -> u32 {
    let world = mem.u32(PHYSICS_GLOBAL);
    if mem.u32(mem.u32(world + 0x24) + 0x160e0) & 0x8_0000 != 0 {
        mem.u32(this + 8)
    } else {
        world
    }
}

/// `CEngine::SimEE60` (0x50ee60, thiscall, x87 float): the state's float at `+0x68` when the definition's word at
/// `+0x26f8` is set, else 0.
pub fn sim_ee60<S: EngineStore>(mem: &S, this: u32) -> f32 {
    if mem.u32(mem.u32(this + 4) + 0x26f8) != 0 {
        mem.f32(mem.u32(this + 8) + 0x68)
    } else {
        0.0
    }
}

/// `CEngine::GetState188` (0x50eb80, thiscall, x87 float): the state's float at `+0x188` (name ours).
pub fn engine_state_speed<S: EngineStore>(mem: &S, this: u32) -> f32 {
    mem.f32(mem.u32(this + 8) + 0x188)
}

/// The roll table `CEngine::SimD870` scales: one float per slot (0x7ca668).
pub const ROLL_TABLE: u32 = 0x007c_a668;

/// `CEngine::SimE470` (0x50e470, thiscall): the rev-limit gate from the speed, the pedal and the options' gate (name ours).
pub fn rev_gate<S: EngineStore>(mem: &S, this: u32, tuning: &EngineTuning) -> f64 {
    let objects = EngineObjects::at(mem, this);
    let state = EngineState::load(mem, objects.state);
    let options = EngineOptions::load(mem, objects.options);
    engine::rev_gate(
        state.speed,
        EngineThis::load(mem, this).pedal.value,
        options.rev_gate,
        tuning.rev_gate,
        tuning.variant,
    )
}

/// `CEngine::SimED70` (0x50ed70, thiscall `(x, q)`): the torque curve at `x`, written to the eight words at `q` (name ours).
pub fn torque_curve<S: EngineStore>(mem: &mut S, this: u32, x: f32, q_at: u32) {
    let def = EngineDef::load(mem, EngineObjects::at(mem, this).def);
    let mut q: [f32; 8] = mem.f32s(q_at);
    engine::torque_curve(&def, x, &mut q);
    for (k, v) in q.iter().enumerate() {
        mem.set_f32(q_at + 4 * k as u32, *v);
    }
}

/// `CEngine::SimD870` (0x50d870, thiscall `(i)`, slot `i < 4`): the roll of the slot (`engine::slot_roll`) (name ours). Returns
/// the eax the original leaves: the quotient, the slot's term address, the options block, or 0.
pub fn slot_roll<S: EngineStore>(mem: &mut S, this: u32, i: u32) -> u32 {
    let objects = EngineObjects::at(mem, this);
    let state = EngineState::load(mem, objects.state);
    let mut options = EngineOptions::load(mem, objects.options);
    let seed_at = mem.u32(objects.options);
    let mut seed = mem.u32(seed_at);
    let slot = i as usize;
    let mut option = options.stall_terms[slot];
    let mut global = mem.f32(ROLL_TABLE + 4 * i);
    let ret = engine::slot_roll(
        options.roll_gate,
        state.threshold_value,
        state.slot_a[slot],
        state.slot_b[slot],
        &mut option,
        &mut global,
        &mut seed,
    );
    mem.set_u32(seed_at, seed);
    mem.set_f32(ROLL_TABLE + 4 * i, global);
    options.stall_terms[slot] = option;
    options.store_stall_terms(mem, objects.options);
    match ret {
        engine::RollRet::Null => 0,
        engine::RollRet::Quotient(q) => q,
        engine::RollRet::Slot => objects.options + 8 + 0x10 * i,
        engine::RollRet::Options => objects.options,
    }
}

/// `CEngine::Sim410` (0x50d410, thiscall, st0): the abs-rpm term, stored back into the state.
pub fn abs_rpm_term<S: EngineStore>(mem: &mut S, this: u32) -> f32 {
    let objects = EngineObjects::at(mem, this);
    let def = EngineDef::load(mem, objects.def);
    let mut state = EngineState::load(mem, objects.state);
    let options = EngineOptions::load(mem, objects.options);
    let st0 = sim::abs_rpm_term(&def, &mut state, &options);
    state.store(mem, objects.state);
    st0
}

/// `CEngine::SimE9F0` (0x50e9f0, thiscall): the compliance terms, stored back into the state (name ours).
pub fn compliance<S: EngineStore>(mem: &mut S, this: u32) {
    let objects = EngineObjects::at(mem, this);
    let def = EngineDef::load(mem, objects.def);
    let options = EngineOptions::load(mem, objects.options);
    let mut state = EngineState::load(mem, objects.state);
    engine::compliance(&def, &options, &mut state);
    state.store(mem, objects.state);
}

/// `x` clamped to `[0, max]` the way the asm does it: below zero is zero, above `max` is `max`, and a NaN stays.
pub(super) fn clamp_asm(v: f32, max: f32) -> f32 {
    if v < 0.0 {
        0.0
    } else if v > max {
        max
    } else {
        v
    }
}

/// `DamperMerge` (0x52e460) on the weighted value at `target_at` (weight, then value), with `source` in registers.
pub(super) fn merge_into<S: EngineStore>(mem: &mut S, source: WeightedValue, target_at: u32) {
    let mut target = WeightedValue { weight: mem.f32(target_at), value: mem.f32(target_at + 4) };
    spring_damper::merge_value(source, &mut target);
    mem.set_f32(target_at, target.weight);
    mem.set_f32(target_at + 4, target.value);
}

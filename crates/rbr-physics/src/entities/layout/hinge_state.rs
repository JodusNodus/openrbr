//! `CHingedRigidBody`'s words: the limit, slip, pivot, signal, solve, `SimDB40`/`SimD860`/`SimEFE0`/`SimD160`
//! inputs and stores, the limit update, `SimF950` and the sequence functions over the frames.

use super::collide_init::IDENTITY_GLOBAL;
use super::entity_slots::load_vec4;
use crate::car_body;
use crate::entities::hinge_pose::{self, PoseBody, PoseCalls, PoseDefinition, PoseHandles, PoseState};
use crate::entities::hinge_offset::OffsetInput;
use crate::entities::hinge_frame_walks::FrameWalkInput;
use crate::entities::hinge_axes::{self, AxesHandles};
use crate::entities::hinge_refresh::{RefreshHandles, RefreshState};
use crate::entities::hinge_advance;
use crate::entities::hinge_pivot::{self, Pivot, PivotCalls, PivotFrame, PivotInput};
use crate::entities::hinge_signals::SignalInput;
use crate::entities::hinge_limits::{self, HingeLimit, HingeState, LimitStep, RotationBlock};
use crate::entities::hinge_slip::{self, SlipCalls, SlipFrame, SlipInput};
use crate::entities::hinge_solve::{self, EntryTerms, Outcome, SolveCalls, SolveFrame, SolveTerms};
use crate::entities::hinge_stops::{self, Limits};
use crate::math::Mat3;
use crate::mem::Mem;
use crate::physics_core;

/// Offset of the hinge link pointer (`CHingedRigidBody`, +0x650); its meaning is unknown (see README).
const HINGE_LINK: u32 = 0x650;

/// `CHingedRigidBody::Set650` (0x51da50): stores `link` at `at + 0x650`.
pub fn set_hinge_link(mem: &mut Mem, at: u32, link: u32) {
    mem.set_u32(at + HINGE_LINK, link);
}

/// The hinged body (`CHingedRigidBody`): its definition pointer (+0x34), the per-limit states (+0x3e8, +0x3ec), the hinge
/// angle (+0x3fc), the hinge rate (+0x40c) and the rotation block (+0x598..+0x5b8, nine words, row by row).
const HINGE_DEF: u32 = 0x34;
const HINGE_LIMIT_STATE: u32 = 0x3E8;
const HINGE_ANGLE: u32 = 0x3FC;
const HINGE_RATE: u32 = 0x40C;
const HINGE_ROTATION: u32 = 0x598;

/// The hinge definition's limit blocks (`+0x1f0 + 0x60 i`) and the values of their members (each member's value word is
/// at +0x0c of the member, so the block's words are at these offsets).
const LIMIT_BLOCK: u32 = 0x1F0;
const LIMIT_STRIDE: u32 = 0x60;
const LIMIT_MODE: u32 = 0x0C;
const LIMIT_STOP_COEF: u32 = 0x2C;
const LIMIT_DAMP_COEF: u32 = 0x3C;
const LIMIT_MAX_STEP: u32 = 0x4C;
const LIMIT_REF_ANGLE: u32 = 0x5C;

/// The two limits of a hinged body's definition.
pub fn load_hinge_limits(mem: &Mem, def: u32) -> [HingeLimit; 2] {
    std::array::from_fn(|i| load_hinge_limit(mem, def, i as u32))
}

/// The definition's limit block `idx` (any index: the asm's `idx * 0x60` wraps).
pub fn load_hinge_limit(mem: &Mem, def: u32, idx: u32) -> HingeLimit {
    let block = def.wrapping_add(LIMIT_BLOCK).wrapping_add(LIMIT_STRIDE.wrapping_mul(idx));
    HingeLimit {
        mode: mem.u32(block + LIMIT_MODE),
        stop_coef: mem.f32(block + LIMIT_STOP_COEF),
        damp_coef: mem.f32(block + LIMIT_DAMP_COEF),
        max_step: mem.f32(block + LIMIT_MAX_STEP),
        ref_angle: mem.f32(block + LIMIT_REF_ANGLE),
    }
}

/// The hinge state of the body at `this`: its limit states, angle and rate.
pub fn load_hinge_state(mem: &Mem, this: u32) -> HingeState {
    HingeState {
        limit_state: mem.f32s(this + HINGE_LIMIT_STATE),
        hinge_angle: mem.f32(this + HINGE_ANGLE),
        hinge_rate: mem.f32(this + HINGE_RATE),
    }
}

/// `CHingedRigidBody::SimCF10(idx)` at `this` (0x51cf10): the residual, and the limit state it moves (stored at
/// `this + 0x3e8 + 4 idx`). Any index reads the block and the state word its address gives (SimEA80 passes 0 and 1).
pub fn hinge_limit_residual(mem: &mut Mem, this: u32, idx: u32) -> LimitStep {
    let limit = load_hinge_limit(mem, mem.u32(this + HINGE_DEF), idx);
    let state_at = this.wrapping_add(HINGE_LIMIT_STATE).wrapping_add(idx.wrapping_mul(4));
    let (angle, rate) = (mem.f32(this + HINGE_ANGLE), mem.f32(this + HINGE_RATE));
    let step = hinge_limits::limit_residual(&limit, mem.f32(state_at), angle, rate);
    if let Some(next) = step.new_state {
        mem.set_f32(state_at, next);
    }
    step
}

/// `CHingedRigidBody::SimEA80` at `this` (0x51ea80): the sum of the active limits' residuals. The limit states the
/// limits move are stored back (both words, in order).
pub fn hinge_limit_sum(mem: &mut Mem, this: u32) -> f32 {
    let def = mem.u32(this + HINGE_DEF);
    let limits = load_hinge_limits(mem, def);
    let mut state = load_hinge_state(mem, this);
    let sum = hinge_limits::limit_sum(&limits, &mut state);
    for (i, value) in state.limit_state.into_iter().enumerate() {
        mem.set_f32(this + HINGE_LIMIT_STATE + 4 * i as u32, value);
    }
    sum
}

/// `CHingedRigidBody::SimC960` at `this` (0x51c960): the rotation block of the hinge angle, nine words row by row.
pub fn hinge_rotation(mem: &mut Mem, this: u32) -> RotationBlock {
    let block = hinge_limits::rotation_block(mem.f32(this + HINGE_ANGLE));
    for (i, value) in block.rows.iter().flatten().enumerate() {
        mem.set_f32(this + HINGE_ROTATION + 4 * i as u32, *value);
    }
    block
}

/// The hinged body's owner car (`CCar*`, +0x3d0), read by `SimCFF0` and the walks it makes.
pub(super) const HINGE_OWNER: u32 = 0x3D0;
/// The hinged body's position (`CSolid`, +0x140..+0x148).
const HINGE_POSITION: u32 = 0x140;
/// The definition's scale vector (+0x2c0..+0x2c8), axis object (+0x360, the walk's `a`), anchor (+0x320) and friction
/// (+0x34c).
const DEF_SCALE: u32 = 0x2C0;
const DEF_AXIS: u32 = 0x360;
const DEF_ANCHOR: u32 = 0x320;
const DEF_FRICTION: u32 = 0x34C;
/// The car's state object is at car+0xa00 (`car_body::layout::state_address`); its vector is at +0xc10 and the word
/// `SimCFF0` reads is at +0xc00.
const STATE_VECTOR: u32 = 0xC10;
const CAR_STATE_C00: u32 = 0xC00;

pub fn vec3(mem: &Mem, at: u32) -> [f32; 3] {
    [mem.f32(at), mem.f32(at + 4), mem.f32(at + 8)]
}

/// The inputs of `CHingedRigidBody::SimCFF0` (0x51cff0) at `this`.
pub fn slip_input(mem: &Mem, this: u32) -> SlipInput {
    let def = mem.u32(this + HINGE_DEF);
    let car = mem.u32(this + HINGE_OWNER);
    let state = car_body::layout::state_address(car);
    SlipInput {
        hinge: this,
        axis: def + DEF_AXIS,
        car,
        state,
        identity: mem.u32(IDENTITY_GLOBAL),
        state_vector: state + STATE_VECTOR,
        position: vec3(mem, this + HINGE_POSITION),
        anchor: vec3(mem, def + DEF_ANCHOR),
        friction: mem.f32(def + DEF_FRICTION),
        scale: vec3(mem, def + DEF_SCALE),
    }
}

/// The car state's word at +0xc00 that `SimCFF0` reads (`state` is the state object, from `slip_input`).
pub fn car_state_c00(mem: &Mem, state: u32) -> f32 {
    mem.f32(state + CAR_STATE_C00)
}

const HINGE_KEY: u32 = 0x30;
const HINGE_BODY_ROTATION: u32 = 0x100;
const HINGE_SECOND: u32 = 0x190;
const HINGE_FIRST: u32 = 0x1C0;
const HINGE_QUAT_MATRIX: u32 = 0x550;
const DEF_CONTEXT: u32 = 0x3C0;
const DEF_POSE: u32 = 0x300;
const DEF_VALUE: u32 = 0x4FC;
const DEF_REFERENCE: u32 = 0x49C;

const CAR_REFERENCE: u32 = 0x250;
const CAR_HEIGHT: u32 = 0x258;
const CAR_OFFSET: u32 = 0x260;
const CAR_PROBE: u32 = 0x190;
const CAR_PROBE_GATE: u32 = 0x180;
const DEF_COMPOSE_GATE: u32 = 0x2F0;
const DEF_COMPOSE: u32 = 0x300;
const DEF_SETTLE: u32 = 0x2C0;
const HINGE_MATRIX: u32 = 0x574;
/// The hinged body's vector at +0x5c0 (the meaning is unknown): `SimD860`'s offset.
const HINGE_OFFSET: u32 = 0x5C0;
const HINGE_GRID: u32 = 0x64;
const HINGE_POSITION_COPY: u32 = 0x330;
const HINGE_ROTATION_COPY: u32 = 0x2A0;
const HINGE_OFFSET_COPY: u32 = 0x5D0;
const HINGE_OUT_COPY: u32 = 0x5E0;
const HINGE_CBASE: u32 = 0x1D0;
const HINGE_CBASE_COUNTER: u32 = 0x1DC;
const HINGE_ANGLE_BLOCK: u32 = 0x3F4;
const DEF_GATE: u32 = 0x4EC;
const DEF_ANGLE_BLOCK: u32 = 0x494;
const DEF_47C: u32 = 0x47C;
const DEF_48C: u32 = 0x48C;
const DEF_20C: u32 = 0x20C;
const DEF_26C: u32 = 0x26C;
const DEF_VECTOR: u32 = 0x3D0;
/// The CDef pointer in the CPhysics (`g_1659b70`), and the CDef's debug flags.
const PHYSICS_DEF: u32 = 0x24;
const DBG_FLAGS: u32 = 0x160E0;
/// The debug flag `SimDB40` tests (its meaning is inferred from the guard).
const DBG_HINGE_FLAG: u32 = 0x40_0000;

/// The inputs `SimE1D0` reads (`PivotInput`). The probe and the compose's `a` are taken only when their gate word's
/// address is nonzero, as the asm tests the address (`leal`/`testl`), not the memory.
pub fn pivot_input(mem: &Mem, this: u32) -> PivotInput {
    let def = mem.u32(this + HINGE_DEF);
    let car = mem.u32(this + HINGE_OWNER);
    PivotInput {
        identity: mem.u32(IDENTITY_GLOBAL),
        car,
        reference: [mem.u32(car + CAR_REFERENCE), mem.u32(car + CAR_REFERENCE + 4)],
        height: mem.f32(car + CAR_HEIGHT),
        car_offset: car + CAR_OFFSET,
        car_probe: if car + CAR_PROBE_GATE != 0 { car + CAR_PROBE } else { 0 },
        compose_a: if def + DEF_COMPOSE_GATE != 0 { def + DEF_COMPOSE } else { 0 },
        settle_weights: vec3(mem, def + DEF_SETTLE),
    }
}

/// Stores `SimE1D0`'s pivot (+0x600..+0x608) and settle value (+0x620) in the hinged body.
pub fn store_pivot(mem: &mut Mem, this: u32, pivot: &Pivot) {
    for (i, &v) in pivot.pivot.iter().enumerate() {
        mem.set_f32(this + HINGE_PIVOT + 4 * i as u32, v);
    }
    mem.set_f32(this + HINGE_SETTLE, pivot.settle);
}

/// The inputs `SimFB40` reads (`SignalInput`). Its branches test the key and the owner; the addresses it forms are not
/// tested in memory, so every field is read here.
pub fn signal_input(mem: &Mem, this: u32) -> SignalInput {
    let definition = mem.u32(this + HINGE_DEF);
    let link = mem.u32(this + HINGE_LINK);
    SignalInput {
        hinge: this,
        quat_matrix: this + HINGE_QUAT_MATRIX,
        identity: mem.u32(IDENTITY_GLOBAL),
        compose_context: definition + DEF_CONTEXT,
        key: mem.u32(this + HINGE_KEY),
        owner: mem.u32(this + HINGE_OWNER),
        body_rotation: load_vec4(mem, this + HINGE_BODY_ROTATION),
        first: load_vec4(mem, this + HINGE_FIRST),
        second: load_vec4(mem, this + HINGE_SECOND),
        definition_pose: load_vec4(mem, definition + DEF_POSE),
        definition_value: mem.u32(definition + DEF_VALUE),
        angle: mem.f32(this + HINGE_ANGLE),
        reference: mem.f32(definition + DEF_REFERENCE),
        link_flag: mem.u32(link),
    }
}

/// The hinged body's link flag: `*(this+0x650) != 0` makes `SimEAD0` return at once.
pub fn hinge_linked(mem: &Mem, this: u32) -> bool {
    mem.u32(mem.u32(this + HINGE_LINK)) != 0
}

/// Whether the hinged body has an owner car (`this+0x3d0`); `SimEAD0` solves only for an owned body.
pub fn hinge_owned(mem: &Mem, this: u32) -> bool {
    mem.u32(this + HINGE_OWNER) != 0
}

/// The definition's `+0x4bc` (`b0`) and `+0x16c` (`mass`), read before `SimE1D0`.
pub fn solve_entry_terms(mem: &Mem, this: u32) -> EntryTerms {
    let def = mem.u32(this + HINGE_DEF);
    EntryTerms { b0: mem.f32(def + DEF_B0), mass: mem.f32(def + DEF_MASS) }
}

fn mat3(mem: &Mem, at: u32) -> [[f32; 3]; 3] {
    std::array::from_fn(|row| vec3(mem, at + 12 * row as u32))
}

/// The solve's inputs, read after `SimE1D0`.
pub fn solve_terms(mem: &Mem, this: u32) -> SolveTerms {
    let def = mem.u32(this + HINGE_DEF);
    SolveTerms {
        def_matrix: mat3(mem, def + DEF_MATRIX),
        rotation: mat3(mem, this + HINGE_ROTATION),
        pivot: vec3(mem, this + HINGE_PIVOT),
        column_weights: vec3(mem, def + DEF_COLUMN_WEIGHTS),
        pf: vec3(mem, def + DEF_SCALE),
        vote_weights: vec3(mem, def + DEF_VOTE_WEIGHTS),
        vote_lever: vec3(mem, def + DEF_VOTE_LEVER),
        ref_angle: mem.f32(def + DEF_REF_ANGLE),
        spring_gain: mem.f32(def + DEF_SPRING_GAIN),
        damping: mem.f32(def + DEF_DAMPING),
        angle: mem.f32(this + HINGE_ANGLE),
        rate: mem.f32(this + HINGE_RATE),
        settle: mem.f32(this + HINGE_SETTLE),
    }
}

/// The hinged body's pivot (+0x600) and settle (+0x620, the value `SimEAD0` writes +0x624 from) words.
const HINGE_PIVOT: u32 = 0x600;
const HINGE_SETTLE: u32 = 0x620;
/// The definition's +0x4bc (`b0`), +0x16c (mass), the 3x3 block at +0x3e0 (row-major), the vectors at +0x3d0, +0x380,
/// +0x3c0 and +0x2c0 (the `pf`), and the scalars +0x49c, +0x4cc and +0x4dc.
const DEF_B0: u32 = 0x4BC;
const DEF_MASS: u32 = 0x16C;
const DEF_MATRIX: u32 = 0x3E0;
const DEF_COLUMN_WEIGHTS: u32 = 0x3D0;
const DEF_VOTE_WEIGHTS: u32 = 0x380;
const DEF_VOTE_LEVER: u32 = 0x3C0;
const DEF_REF_ANGLE: u32 = 0x49C;
const DEF_SPRING_GAIN: u32 = 0x4CC;
const DEF_DAMPING: u32 = 0x4DC;

/// The inputs of `CHingedRigidBody::SimDB40` (0x51db40) at `this`, or `None` when its guard fails: the debug flag is
/// clear, or there is no owner car. The asm tests the flag first, then the car.
pub fn db40_input(mem: &Mem, this: u32) -> Option<FrameWalkInput> {
    let physics = mem.u32(car_body::layout::WORLD);
    let flags = mem.u32(mem.u32(physics + PHYSICS_DEF) + DBG_FLAGS);
    let car = mem.u32(this + HINGE_OWNER);
    if flags & DBG_HINGE_FLAG == 0 || car == 0 {
        return None;
    }
    let def = mem.u32(this + HINGE_DEF);
    Some(FrameWalkInput {
        identity: mem.u32(IDENTITY_GLOBAL),
        body: this,
        car,
        hinge_rotation: mat3(mem, this + HINGE_ROTATION),
        frame_matrix: mat3(mem, this + HINGE_MATRIX),
        axes_rf: mat3(mem, def + DEF_MATRIX),
        inverse_origin_bf: vec3(mem, def + DEF_VECTOR),
        hinge_point_rf: vec3(mem, def + DEF_POSE),
        face_normal_bf: vec3(mem, def + DEF_AXIS),
    })
}

/// The inputs of `CHingedRigidBody::SimD860` (0x51d860) at `this`.
pub fn d860_input(mem: &Mem, this: u32) -> OffsetInput {
    let def = mem.u32(this + HINGE_DEF);
    OffsetInput {
        identity: mem.u32(IDENTITY_GLOBAL),
        car: mem.u32(this + HINGE_OWNER),
        rotation_row: vec3(mem, this + HINGE_ROTATION + 12),
        axes_rf: mat3(mem, def + DEF_MATRIX),
        offset: vec3(mem, this + HINGE_OFFSET),
    }
}

pub(super) fn words<const N: usize>(mem: &Mem, at: u32) -> [u32; N] {
    std::array::from_fn(|i| mem.u32(at + 4 * i as u32))
}

fn set_words(mem: &mut Mem, at: u32, values: &[u32]) {
    for (i, value) in values.iter().enumerate() {
        mem.set_u32(at + 4 * i as u32, *value);
    }
}

/// The body and definition words `SimEFE0` reads or writes (`RefreshState`), as the body is in memory now.
pub fn efe0_state(mem: &Mem, this: u32) -> RefreshState {
    let def = mem.u32(this + HINGE_DEF);
    let link_address = mem.u32(this + HINGE_LINK);
    RefreshState {
        unlocked: mem.u32(def + DEF_GATE),
        definition_angle_block: words(mem, def + DEF_ANGLE_BLOCK),
        max_angle: mem.u32(def + DEF_47C),
        min_angle: mem.u32(def + DEF_48C),
        definition_limit_state: [mem.u32(def + DEF_20C), mem.u32(def + DEF_26C)],
        hinge_point_bf: words(mem, def + DEF_CONTEXT),
        link_address,
        link_flag: mem.u32(link_address),
        grid: mem.u32(this + HINGE_GRID),
        dirty: mem.u32(this + HINGE_CBASE + 8),
        dirty_counter: mem.u32(this + HINGE_CBASE_COUNTER),
        angle_block: words(mem, this + HINGE_ANGLE_BLOCK),
        limit_state: words(mem, this + HINGE_LIMIT_STATE),
        position: words(mem, this + HINGE_POSITION),
        position_copy: words(mem, this + HINGE_POSITION_COPY),
        orientation: words(mem, this + HINGE_BODY_ROTATION),
        orientation_copy: words(mem, this + HINGE_ROTATION_COPY),
        offset: words(mem, this + HINGE_OFFSET),
        offset_copy: words(mem, this + HINGE_OFFSET_COPY),
        offset_result_copy: words(mem, this + HINGE_OUT_COPY),
    }
}

/// Writes the words `SimEFE0` changes: the flag, the CBase and its counter, the angle block, the limit words and the copies.
pub fn store_efe0_state(mem: &mut Mem, this: u32, state: &RefreshState) {
    mem.set_u32(state.link_address, state.link_flag);
    set_words(mem, this + HINGE_CBASE + 8, &[state.dirty]);
    set_words(mem, this + HINGE_CBASE_COUNTER, &[state.dirty_counter]);
    set_words(mem, this + HINGE_ANGLE_BLOCK, &state.angle_block);
    set_words(mem, this + HINGE_LIMIT_STATE, &state.limit_state);
    set_words(mem, this + HINGE_POSITION_COPY, &state.position_copy);
    set_words(mem, this + HINGE_ROTATION_COPY, &state.orientation_copy);
    set_words(mem, this + HINGE_OFFSET_COPY, &state.offset_copy);
    set_words(mem, this + HINGE_OUT_COPY, &state.offset_result_copy);
}

/// The addresses `SimEFE0` passes to its calls: the definition's blocks (the `VfE070` pointers, `BodyCompose`'s `a` and
/// output) and the body's car. `d860_out` is the caller's scratch vector for `SimD860`.
pub fn efe0_frame(mem: &Mem, this: u32, d860_out: u32) -> RefreshHandles {
    let def = mem.u32(this + HINGE_DEF);
    RefreshHandles {
        body: this,
        car: mem.u32(this + HINGE_OWNER),
        first_axes: [def + DEF_POSE, def + DEF_SETTLE, def + 0x2E0, def + 0x310, def + DEF_MATRIX, def + 0x404],
        second_axes: [def + DEF_CONTEXT, def + 0x380, def + 0x3A0, def + DEF_VECTOR, def + 0x44C, def + 0x428],
        compose_input: def + DEF_POSE,
        compose_output: def + DEF_ANCHOR,
        offset_out: d860_out,
    }
}

/// Offset of `MB560`'s output in the body (`SimD160`'s second walk).
const HINGE_WALK_OUT: u32 = 0x5F0;
/// The definition's second matrix (+0x428, 3x3), read by `SimD160`.
const DEF_SECOND_MAT: u32 = 0x428;

/// The addresses `SimD160` passes to its calls.
pub fn d160_frame(mem: &Mem, this: u32) -> PoseHandles {
    let def = mem.u32(this + HINGE_DEF);
    PoseHandles {
        car: mem.u32(this + HINGE_OWNER),
        hinge_point_rf: def + DEF_POSE,
        hinge_axis_rf: def + DEF_SETTLE,
        identity: mem.u32(IDENTITY_GLOBAL),
        transformed_hinge_point: this + HINGE_OFFSET,
        carried_hinge_axis: this + HINGE_WALK_OUT,
        body: this,
        frame_matrix: this + HINGE_MATRIX,
        quaternion_out: this + HINGE_BODY_ROTATION,
    }
}

/// The definition's frame words, and the body's block and position, as they are in memory now (`SimD160`'s reads).
pub fn d160_words(mem: &Mem, this: u32) -> PoseState {
    let def = mem.u32(this + HINGE_DEF);
    PoseState {
        definition: PoseDefinition {
            composed_hinge_point: vec3(mem, def + DEF_ANCHOR),
            inverse_origin_bf: vec3(mem, def + DEF_COLUMN_WEIGHTS),
            axes_rf: mat3(mem, def + DEF_MATRIX),
            inverse_axes_bf: mat3(mem, def + DEF_SECOND_MAT),
        },
        body: PoseBody {
            orientation_matrix: mat3(mem, this + HINGE_QUAT_MATRIX),
            frame_matrix: mat3(mem, this + HINGE_MATRIX),
            hinge_rotation: mat3(mem, this + HINGE_ROTATION),
        },
        position: load_vec4(mem, this + HINGE_POSITION),
    }
}

/// Writes the body's products and its position (`SimD160`'s stores). `hinge_rotation` is not written: the port does not change it.
pub fn store_d160_words(mem: &mut Mem, this: u32, state: &PoseState) {
    set_mat3(mem, this + HINGE_QUAT_MATRIX, &state.body.orientation_matrix);
    set_mat3(mem, this + HINGE_MATRIX, &state.body.frame_matrix);
    for (i, value) in state.position.iter().enumerate() {
        mem.set_f32(this + HINGE_POSITION + 4 * i as u32, *value);
    }
}

fn set_mat3(mem: &mut Mem, at: u32, m: &Mat3) {
    for (i, value) in m.iter().flatten().enumerate() {
        mem.set_f32(at + 4 * i as u32, *value);
    }
}

/// `CHingedRigidBody::VfE070` (0x51e070) on the windows of `frame`. It stores `e`, then reads `f[9]` (the word after `f`: the
/// second call's is `e[0]`, which that store has just written), and stores `f` and `d`. The asm stores `f` before it reads
/// `f[9]` and reads `a` after that; `a` is read here before `f` is stored, which changes nothing: the windows don't overlap.
pub fn vf_e070(mem: &mut Mem, frame: &AxesHandles) {
    let axes = hinge_axes::axes(load_vec4(mem, frame.direction), vec3(mem, frame.reference));
    set_mat3(mem, frame.axes, &axes);
    let tail = mem.f32(frame.inverse_axes + 36);
    let (inverse_origin, inverse_axes) = hinge_axes::inverse(vec3(mem, frame.origin), &axes, tail);
    set_f32s(mem, frame.inverse_axes, &inverse_axes);
    set_f32s(mem, frame.inverse_origin, &inverse_origin);
}

fn set_f32s(mem: &mut Mem, at: u32, values: &[f32]) {
    for (i, value) in values.iter().enumerate() {
        mem.set_f32(at + 4 * i as u32, *value);
    }
}

/// The limit update's fields: the tweak block is the pointer at 0x34; its two windows are at +0x474 and +0x484 (the bound is
/// each window's third word), the factor at +0x4ac. `this` holds the window copy at +0x3f4, the value at +0x3fc, the
/// residual at +0x40c and the limit object at +0x400.
const LIMIT_TWEAK: u32 = 0x34;
const LIMIT_UPPER: u32 = 0x474;
const LIMIT_LOWER: u32 = 0x484;
const LIMIT_FACTOR: u32 = 0x4ac;
const LIMIT_WINDOW: u32 = 0x3f4;
const LIMIT_VALUE: u32 = 0x3fc;
const LIMIT_OBJECT: u32 = 0x400;
const LIMIT_RESIDUAL: u32 = 0x40c;

fn load_window(mem: &Mem, at: u32) -> [u32; 3] {
    std::array::from_fn(|i| mem.u32(at + 4 * i as u32))
}

/// `CEntity` 0x51f280 on `this`: reads the fields, takes the window, stores it, calls the limit object and stores the
/// residual, in the original's order. Returns eax.
pub fn limit_update_at(mem: &mut Mem, this: u32, calls: &mut impl hinge_stops::Calls) -> u32 {
    let tweak = mem.u32(this + LIMIT_TWEAK);
    let limits = Limits {
        angle: mem.f32(this + LIMIT_VALUE),
        rate: mem.f32(this + LIMIT_RESIDUAL),
        restitution: mem.f32(tweak + LIMIT_FACTOR),
        max_angle: load_window(mem, tweak + LIMIT_UPPER),
        min_angle: load_window(mem, tweak + LIMIT_LOWER),
    };
    let Some(update) = hinge_stops::apply_stops(&limits) else {
        return tweak;
    };
    for (i, word) in update.record.into_iter().enumerate() {
        mem.set_u32(this + LIMIT_WINDOW + 4 * i as u32, word);
    }
    if let Some(rate) = update.rate {
        calls.mark_dirty();
        mem.set_f32(this + LIMIT_RESIDUAL, rate);
    }
    update.result
}

/// The limit object the flag call is made on.
pub fn limit_object(this: u32) -> u32 {
    this + LIMIT_OBJECT
}

/// The solve's value (`SimEAD0` writes it), the hinge rate's and the angle's tweak objects (`NTweak::CBase`; the value is
/// at +0xc), and the state signal's offset from the signal manager.
const HINGE_SOLVED: u32 = 0x624;
/// The hinged body's second walk output (`SimE1D0`'s settle hit), at +0x610.
const HINGE_SETTLE_HIT: u32 = 0x610;
const RATE_TWEAK: u32 = 0x400;
const ANGLE_TWEAK: u32 = 0x3F0;
const STATE_SIGNAL: u32 = 0xE438;

fn store_vec4(mem: &mut Mem, at: u32, v: [f32; 4]) {
    for (i, value) in v.into_iter().enumerate() {
        mem.set_f32(at + 4 * i as u32, value);
    }
}

/// `CHingedRigidBody::SimF950` (0x51f950) on `this` with the step `dt`. `sig` is the state payload's slot in the
/// original's frame (its stack); the payload's padding words are not written, as the original does not write them.
/// Returns eax: the link's flag pointer for a settled body (the asm returns the pointer it loaded), the integrate's for a
/// body with no owner car, else the emit's.
pub fn sim_f950_at(mem: &mut Mem, this: u32, dt: f32, sig: u32, calls: &mut impl hinge_advance::Calls) -> u32 {
    let link = mem.u32(this + HINGE_LINK);
    if mem.u32(link) != 0 {
        return link;
    }
    if mem.u32(this + HINGE_OWNER) == 0 {
        return calls.integrate(this, dt);
    }
    let step =
        hinge_advance::advance(mem.f32(this + HINGE_SOLVED), dt, mem.f32(this + HINGE_RATE), mem.f32(this + HINGE_ANGLE));
    calls.set_dirty(this + RATE_TWEAK);
    mem.set_f32(this + HINGE_RATE, step.rate);
    // The asm tests the address (`this + 0x180`), so the scaled vector is absent only when `this` is -0x180.
    let scaled = if this.wrapping_add(0x180) != 0 { this + HINGE_SECOND } else { 0 };
    calls.scale(mem.u32(this + HINGE_DEF) + DEF_SCALE, step.rate, scaled);
    // E09Vec4::Clear (0x465d80) on the first vector: four zero lanes.
    store_vec4(mem, this + HINGE_FIRST, [0.0; 4]);
    calls.set_dirty(this + ANGLE_TWEAK);
    mem.set_f32(this + HINGE_ANGLE, step.angle);
    calls.apply_stops(this);
    calls.frame(this);
    // The payload: the key, the rotation block's vector (+0x100, or 0 when `this` is -0xf0) at +0x20, then the position (+0x140,
    // E01Vec4::CopyFrom) at +0x14, which overwrites the rotation's first lane (+0x20).
    let key = mem.u32(this + HINGE_KEY);
    let rotation_at = if this.wrapping_add(0xF0) != 0 { this + HINGE_BODY_ROTATION } else { 0 };
    let rotation = load_vec4(mem, rotation_at);
    let position = load_vec4(mem, this + HINGE_POSITION);
    mem.set_u32(sig, key);
    store_vec4(mem, sig + 0x20, rotation);
    store_vec4(mem, sig + 0x14, position);
    let signal = physics_core::layout::signal_manager(mem) + STATE_SIGNAL;
    calls.emit(signal, sig)
}

/// Stores the walk vector `SimE1D0` builds in the frame (`PivotCalls::store_vector`): three words.
pub fn store_walk_vector(mem: &mut Mem, at: u32, words: [u32; 3]) {
    for (i, word) in words.into_iter().enumerate() {
        mem.set_u32(at + 4 * i as u32, word);
    }
}

/// `SimCFF0`'s scratch in its frame (`base`, the frame's 16-aligned base): the copy of the car's vector and the two
/// walks' outputs.
pub fn slip_frame(base: u32) -> SlipFrame {
    SlipFrame { vector_copy: base - 0x20, hit: base - 0x50, velocity: base - 0x60 }
}

/// `SimE1D0`'s scratch in its frame: the walk vector, the first walk's output, the compose's scratch (the third walk's
/// output, with the compose's output 0x10 on).
pub fn pivot_frame(base: u32, this: u32) -> PivotFrame {
    PivotFrame { vector: base - 0x50, hit: base - 0x40, settle_hit: this + HINGE_SETTLE_HIT, scratch: base - 0x30 }
}

/// `SimEAD0`'s vote vectors in its frame (the two words the vote writes, 16 bytes each).
pub fn solve_frame(base: u32) -> SolveFrame {
    SolveFrame { v90: base - 0x20, v90b: base - 0x80 }
}

/// `CHingedRigidBody::SimCFF0` (0x51cff0) at `this`: the slip term, through `calls` (its walks and copy).
pub fn sim_cff0_at(mem: &Mem, this: u32, base: u32, calls: &mut impl SlipCalls) -> f32 {
    hinge_slip::slip_term(calls, &slip_input(mem, this), &slip_frame(base))
}

/// `CHingedRigidBody::SimE1D0` (0x51e1d0) at `this`: the pivot and the settle value, through `calls`, stored in the body.
/// Returns eax: `def + 0x2c0` when `def + 0x2b0` is set, else 0 (the asm's last load).
pub fn sim_e1d0_at(mem: &mut Mem, this: u32, base: u32, calls: &mut impl PivotCalls) -> u32 {
    let input = pivot_input(mem, this);
    let pivot = hinge_pivot::pivot_and_settle(calls, &input, &pivot_frame(base, this));
    store_pivot(mem, this, &pivot);
    let def = mem.u32(this + HINGE_DEF);
    if def.wrapping_add(0x2B0) == 0 {
        0
    } else {
        def.wrapping_add(DEF_SETTLE)
    }
}

/// `CHingedRigidBody::SimD160` (0x51d160) at `this`: the frame's products, through `calls`. Returns eax: `this + 0x140`,
/// which `E01Vec4::CopyFrom` returns (the asm's last call).
pub fn sim_d160_at(mem: &Mem, this: u32, calls: &mut impl PoseCalls) -> u32 {
    hinge_pose::update_pose(calls, &d160_frame(mem, this));
    this + HINGE_POSITION
}

/// `CHingedRigidBody::SimEAD0` (0x51ead0) at `this`: the hinge solve, through `calls`. A solved value is stored at +0x624.
pub fn sim_ead0_at(mem: &mut Mem, this: u32, base: u32, calls: &mut impl SolveCalls) -> Outcome {
    let outcome = hinge_solve::hinge_solve(calls, this, &solve_frame(base));
    if let Outcome::Solved(value) = outcome {
        mem.set_f32(this + HINGE_SOLVED, value);
    }
    outcome
}

/// The link pointer (+0x650) of the hinged body at `this`: the word `SimEAD0`'s early return leaves in eax.
pub fn hinge_link(mem: &Mem, this: u32) -> u32 {
    mem.u32(this + HINGE_LINK)
}

/// The definition pointer (+0x34) of the hinged body at `this`: the word `SimEAD0` leaves in eax after a vote hit.
pub fn hinge_definition(mem: &Mem, this: u32) -> u32 {
    mem.u32(this + HINGE_DEF)
}

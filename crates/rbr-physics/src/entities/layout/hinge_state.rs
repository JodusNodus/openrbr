//! `CHingedRigidBody`'s words: the limit, slip, pivot, signal, solve, `SimDB40`/`SimD860`/`SimEFE0`/`SimD160`
//! inputs and stores, the limit update, `SimF950` and the sequence functions over the frames.

use super::collide_init::IDENTITY_GLOBAL;
use crate::car_body;
use crate::entities::hinge_axes::{self, AxesHandles};
use crate::entities::hinge_frame_walks::FrameWalkInput;
use crate::entities::hinge_limits::{HingeState, LimitSpring, RotationBlock};
use crate::entities::hinge_offset::OffsetInput;
use crate::entities::hinge_pivot::{Pivot, PivotHandles, PivotInput};
use crate::entities::hinge_pose::{PoseBody, PoseDefinition, PoseHandles, PoseState};
use crate::entities::hinge_refresh::{RefreshHandles, RefreshState};
use crate::entities::hinge_signals::SignalInput;
use crate::entities::hinge_slip::{SlipHandles, SlipInput};
use crate::entities::hinge_solve::{ContactHandles, EntryTerms, SolveTerms};
use crate::entities::hinge_stops::Limits;
use crate::entities::store::{AdvanceInput, HingeStore, StatePayload};
use crate::math::vec::Vec4;
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

fn mat3(mem: &Mem, at: u32) -> [[f32; 3]; 3] {
    std::array::from_fn(|row| vec3(mem, at + 12 * row as u32))
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

pub(super) fn words<const N: usize>(mem: &Mem, at: u32) -> [u32; N] {
    std::array::from_fn(|i| mem.u32(at + 4 * i as u32))
}

fn set_words(mem: &mut Mem, at: u32, values: &[u32]) {
    for (i, value) in values.iter().enumerate() {
        mem.set_u32(at + 4 * i as u32, *value);
    }
}

/// Offset of `MB560`'s output in the body (`SimD160`'s second walk).
const HINGE_WALK_OUT: u32 = 0x5F0;
/// The definition's second matrix (+0x428, 3x3), read by `SimD160`.
const DEF_SECOND_MAT: u32 = 0x428;

fn set_mat3(mem: &mut Mem, at: u32, m: &Mat3) {
    for (i, value) in m.iter().flatten().enumerate() {
        mem.set_f32(at + 4 * i as u32, *value);
    }
}

/// `CHingedRigidBody::VfE070` (0x51e070) on the windows of `frame`. It stores `e`, then reads `f[9]` (the word after `f`: the
/// second call's is `e[0]`, which that store has just written), and stores `f` and `d`. The asm stores `f` before it reads
/// `f[9]` and reads `a` after that; `a` is read here before `f` is stored, which changes nothing: the windows don't overlap.
pub fn vf_e070(mem: &mut Mem, frame: &AxesHandles) {
    let axes = hinge_axes::axes(mem.f32s(frame.direction), vec3(mem, frame.reference));
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

/// Stores the walk vector `SimE1D0` builds in the frame (`PivotCalls::store_vector`): three words.
pub fn store_walk_vector(mem: &mut Mem, at: u32, words: [u32; 3]) {
    for (i, word) in words.into_iter().enumerate() {
        mem.set_u32(at + 4 * i as u32, word);
    }
}

/// `SimCFF0`'s scratch in its frame (`base`, the frame's 16-aligned base): the copy of the car's vector and the two
/// walks' outputs.
pub fn slip_frame(base: u32) -> SlipHandles {
    SlipHandles { vector_copy: base - 0x20, face_normal: base - 0x50, velocity: base - 0x60 }
}

/// `SimE1D0`'s scratch in its frame: the walk vector, the first walk's output, the compose's scratch (the third walk's
/// output, with the compose's output 0x10 on).
pub fn pivot_frame(base: u32, this: u32) -> PivotHandles {
    PivotHandles {
        vector: base - 0x50,
        acceleration: base - 0x40,
        angular_acceleration: this + HINGE_SETTLE_HIT,
        angular_velocity: base - 0x30,
        lever: base - 0x20,
    }
}

/// `SimEAD0`'s vote vectors in its frame (the two words the vote writes, 16 bytes each).
pub fn solve_frame(base: u32) -> ContactHandles {
    ContactHandles { force: base - 0x20, torque: base - 0x80 }
}

/// The tweak objects of the hinge rate and the angle (`NTweak::CBase`), which `SimF950` marks dirty.
pub fn rate_tweak(this: u32) -> u32 {
    this + RATE_TWEAK
}

pub fn angle_tweak(this: u32) -> u32 {
    this + ANGLE_TWEAK
}

/// The angular velocity vector `SimF950` scales into. The asm tests the address (`this + 0x180`), so the vector is absent
/// only when `this` is -0x180.
pub fn scaled_vector(this: u32) -> u32 {
    if this.wrapping_add(0x180) != 0 {
        this + HINGE_SECOND
    } else {
        0
    }
}

/// The hinged body's position (`this + 0x140`), which `E01Vec4::CopyFrom` returns.
pub fn position_at(this: u32) -> u32 {
    this + HINGE_POSITION
}

/// The definition's limit block `idx` (any index: the asm's `idx * 0x60` wraps).
fn limit_block(mem: &Mem, def: u32, idx: u32) -> LimitSpring {
    let block = def.wrapping_add(LIMIT_BLOCK).wrapping_add(LIMIT_STRIDE.wrapping_mul(idx));
    LimitSpring {
        kind: mem.u32(block + LIMIT_MODE),
        stiffness: mem.f32(block + LIMIT_STOP_COEF),
        damping: mem.f32(block + LIMIT_DAMP_COEF),
        max_displacement: mem.f32(block + LIMIT_MAX_STEP),
        span: mem.f32(block + LIMIT_REF_ANGLE),
    }
}

/// The body's limit state word `idx` (any index: the address the asm forms).
fn limit_state_at(this: u32, idx: u32) -> u32 {
    this.wrapping_add(HINGE_LIMIT_STATE).wrapping_add(idx.wrapping_mul(4))
}

impl HingeStore for Mem {
    fn hinge_link(&self, this: u32) -> u32 {
        self.u32(this + HINGE_LINK)
    }

    fn hinge_definition(&self, this: u32) -> u32 {
        self.u32(this + HINGE_DEF)
    }

    /// `*(this+0x650) != 0` makes `SimEAD0` return at once.
    fn hinge_linked(&self, this: u32) -> bool {
        self.u32(self.u32(this + HINGE_LINK)) != 0
    }

    /// `SimEAD0` solves only for a body with an owner car (`this+0x3d0`).
    fn hinge_owned(&self, this: u32) -> bool {
        self.u32(this + HINGE_OWNER) != 0
    }

    fn load_hinge_limit(&self, def: u32, idx: u32) -> LimitSpring {
        limit_block(self, def, idx)
    }

    fn load_hinge_limits(&self, def: u32) -> [LimitSpring; 2] {
        std::array::from_fn(|i| limit_block(self, def, i as u32))
    }

    /// The hinge state of the body at `this`: its limit states, angle and rate.
    fn load_hinge_state(&self, this: u32) -> HingeState {
        HingeState {
            spring_position: self.f32s(this + HINGE_LIMIT_STATE),
            angle: self.f32(this + HINGE_ANGLE),
            rate: self.f32(this + HINGE_RATE),
        }
    }

    fn body_limit(&self, this: u32, idx: u32) -> LimitSpring {
        limit_block(self, self.u32(this + HINGE_DEF), idx)
    }

    fn body_limits(&self, this: u32) -> [LimitSpring; 2] {
        self.load_hinge_limits(self.u32(this + HINGE_DEF))
    }

    fn limit_position(&self, this: u32, idx: u32) -> f32 {
        self.f32(limit_state_at(this, idx))
    }

    fn store_limit_position(&mut self, this: u32, idx: u32, value: f32) {
        self.set_f32(limit_state_at(this, idx), value);
    }

    fn store_limit_positions(&mut self, this: u32, values: [f32; 2]) {
        for (i, value) in values.into_iter().enumerate() {
            self.set_f32(this + HINGE_LIMIT_STATE + 4 * i as u32, value);
        }
    }

    fn angle_rate(&self, this: u32) -> (f32, f32) {
        (self.f32(this + HINGE_ANGLE), self.f32(this + HINGE_RATE))
    }

    fn hinge_angle(&self, this: u32) -> f32 {
        self.f32(this + HINGE_ANGLE)
    }

    /// The rotation block of the hinge angle, nine words row by row.
    fn store_rotation_block(&mut self, this: u32, block: &RotationBlock) {
        for (i, value) in block.rows.iter().flatten().enumerate() {
            self.set_f32(this + HINGE_ROTATION + 4 * i as u32, *value);
        }
    }

    /// The inputs of `CHingedRigidBody::SimCFF0` (0x51cff0) at `this`.
    fn slip_input(&self, this: u32) -> SlipInput {
        let def = self.u32(this + HINGE_DEF);
        let car = self.u32(this + HINGE_OWNER);
        let state = car_body::layout::state_address(car);
        SlipInput {
            body: this,
            face_normal_bf: def + DEF_AXIS,
            car,
            state,
            identity: self.u32(IDENTITY_GLOBAL),
            car_vector: state + STATE_VECTOR,
            position: vec3(self, this + HINGE_POSITION),
            composed_hinge_point: vec3(self, def + DEF_ANCHOR),
            face_area: self.f32(def + DEF_FRICTION),
            hinge_axis_rf: vec3(self, def + DEF_SCALE),
        }
    }

    fn car_state_c00(&self, state: u32) -> f32 {
        self.f32(state + CAR_STATE_C00)
    }

    /// The inputs `SimE1D0` reads (`PivotInput`). The probe and the compose's `a` are taken only when their gate word's
    /// address is nonzero, as the asm tests the address (`leal`/`testl`), not the memory.
    fn pivot_input(&self, this: u32) -> PivotInput {
        let def = self.u32(this + HINGE_DEF);
        let car = self.u32(this + HINGE_OWNER);
        PivotInput {
            identity: self.u32(IDENTITY_GLOBAL),
            car,
            car_acceleration_xy: [self.u32(car + CAR_REFERENCE), self.u32(car + CAR_REFERENCE + 4)],
            car_acceleration_z: self.f32(car + CAR_HEIGHT),
            car_angular_acceleration: car + CAR_OFFSET,
            car_angular_velocity: if car + CAR_PROBE_GATE != 0 { car + CAR_PROBE } else { 0 },
            hinge_point_rf: if def + DEF_COMPOSE_GATE != 0 { def + DEF_COMPOSE } else { 0 },
            hinge_axis_rf: vec3(self, def + DEF_SETTLE),
        }
    }

    /// Stores `SimE1D0`'s pivot (+0x600..+0x608) and settle value (+0x620) in the hinged body.
    fn store_pivot(&mut self, this: u32, pivot: &Pivot) {
        for (i, &v) in pivot.hinge_point_acceleration.iter().enumerate() {
            self.set_f32(this + HINGE_PIVOT + 4 * i as u32, v);
        }
        self.set_f32(this + HINGE_SETTLE, pivot.frame_angular_acceleration);
    }

    fn settle_axis(&self, this: u32) -> u32 {
        let def = self.u32(this + HINGE_DEF);
        if def.wrapping_add(0x2B0) == 0 {
            0
        } else {
            def.wrapping_add(DEF_SETTLE)
        }
    }

    /// The inputs `SimFB40` reads (`SignalInput`). Its branches test the key and the owner; the addresses it forms are not
    /// tested in memory, so every field is read here.
    fn signal_input(&self, this: u32) -> SignalInput {
        let definition = self.u32(this + HINGE_DEF);
        let link = self.u32(this + HINGE_LINK);
        SignalInput {
            body: this,
            orientation_matrix: this + HINGE_QUAT_MATRIX,
            identity: self.u32(IDENTITY_GLOBAL),
            hinge_point_bf: definition + DEF_CONTEXT,
            key: self.u32(this + HINGE_KEY),
            owner: self.u32(this + HINGE_OWNER),
            orientation: self.f32s(this + HINGE_BODY_ROTATION),
            linear_velocity: self.f32s(this + HINGE_FIRST),
            angular_velocity: self.f32s(this + HINGE_SECOND),
            hinge_point_rf: self.f32s(definition + DEF_POSE),
            body_id: self.u32(definition + DEF_VALUE),
            angle: self.f32(this + HINGE_ANGLE),
            default_angle: self.f32(definition + DEF_REFERENCE),
            link_flag: self.u32(link),
        }
    }

    /// The definition's `+0x4bc` (`b0`) and `+0x16c` (`mass`), read before `SimE1D0`.
    fn solve_entry_terms(&self, this: u32) -> EntryTerms {
        let def = self.u32(this + HINGE_DEF);
        EntryTerms { inertia: self.f32(def + DEF_B0), inverse_mass: self.f32(def + DEF_MASS) }
    }

    /// The solve's inputs, read after `SimE1D0`.
    fn solve_terms(&self, this: u32) -> SolveTerms {
        let def = self.u32(this + HINGE_DEF);
        SolveTerms {
            axes_rf: mat3(self, def + DEF_MATRIX),
            hinge_rotation: mat3(self, this + HINGE_ROTATION),
            hinge_point_acceleration: vec3(self, this + HINGE_PIVOT),
            inverse_origin_bf: vec3(self, def + DEF_COLUMN_WEIGHTS),
            hinge_axis_rf: vec3(self, def + DEF_SCALE),
            hinge_axis_bf: vec3(self, def + DEF_VOTE_WEIGHTS),
            hinge_point_bf: vec3(self, def + DEF_VOTE_LEVER),
            default_angle: self.f32(def + DEF_REF_ANGLE),
            spring_rate: self.f32(def + DEF_SPRING_GAIN),
            damping: self.f32(def + DEF_DAMPING),
            angle: self.f32(this + HINGE_ANGLE),
            rate: self.f32(this + HINGE_RATE),
            frame_angular_acceleration: self.f32(this + HINGE_SETTLE),
        }
    }

    /// The solve's value, which `SimEAD0` writes at +0x624.
    fn store_solved(&mut self, this: u32, value: f32) {
        self.set_f32(this + HINGE_SOLVED, value);
    }

    /// The inputs of `CHingedRigidBody::SimDB40` (0x51db40) at `this`, or `None` when its guard fails: the debug flag is
    /// clear, or there is no owner car. The asm tests the flag first, then the car.
    fn db40_input(&self, this: u32) -> Option<FrameWalkInput> {
        let physics = self.u32(car_body::layout::WORLD);
        let flags = self.u32(self.u32(physics + PHYSICS_DEF) + DBG_FLAGS);
        let car = self.u32(this + HINGE_OWNER);
        if flags & DBG_HINGE_FLAG == 0 || car == 0 {
            return None;
        }
        let def = self.u32(this + HINGE_DEF);
        Some(FrameWalkInput {
            identity: self.u32(IDENTITY_GLOBAL),
            body: this,
            car,
            hinge_rotation: mat3(self, this + HINGE_ROTATION),
            frame_matrix: mat3(self, this + HINGE_MATRIX),
            axes_rf: mat3(self, def + DEF_MATRIX),
            inverse_origin_bf: vec3(self, def + DEF_VECTOR),
            hinge_point_rf: vec3(self, def + DEF_POSE),
            face_normal_bf: vec3(self, def + DEF_AXIS),
        })
    }

    /// The inputs of `CHingedRigidBody::SimD860` (0x51d860) at `this`.
    fn d860_input(&self, this: u32) -> OffsetInput {
        let def = self.u32(this + HINGE_DEF);
        OffsetInput {
            identity: self.u32(IDENTITY_GLOBAL),
            car: self.u32(this + HINGE_OWNER),
            rotation_row: vec3(self, this + HINGE_ROTATION + 12),
            axes_rf: mat3(self, def + DEF_MATRIX),
            offset: vec3(self, this + HINGE_OFFSET),
        }
    }

    /// The body and definition words `SimEFE0` reads or writes (`RefreshState`), as the body is in memory now.
    fn efe0_state(&self, this: u32) -> RefreshState {
        let def = self.u32(this + HINGE_DEF);
        let link_address = self.u32(this + HINGE_LINK);
        RefreshState {
            unlocked: self.u32(def + DEF_GATE),
            definition_angle_block: words(self, def + DEF_ANGLE_BLOCK),
            max_angle: self.u32(def + DEF_47C),
            min_angle: self.u32(def + DEF_48C),
            definition_spring_position: [self.u32(def + DEF_20C), self.u32(def + DEF_26C)],
            hinge_point_bf: words(self, def + DEF_CONTEXT),
            link_address,
            link_flag: self.u32(link_address),
            grid: self.u32(this + HINGE_GRID),
            dirty: self.u32(this + HINGE_CBASE + 8),
            dirty_counter: self.u32(this + HINGE_CBASE_COUNTER),
            angle_block: words(self, this + HINGE_ANGLE_BLOCK),
            spring_position: words(self, this + HINGE_LIMIT_STATE),
            position: words(self, this + HINGE_POSITION),
            position_copy: words(self, this + HINGE_POSITION_COPY),
            orientation: words(self, this + HINGE_BODY_ROTATION),
            orientation_copy: words(self, this + HINGE_ROTATION_COPY),
            offset: words(self, this + HINGE_OFFSET),
            offset_copy: words(self, this + HINGE_OFFSET_COPY),
            offset_result_copy: words(self, this + HINGE_OUT_COPY),
        }
    }

    /// Writes the words `SimEFE0` changes: the flag, the CBase and its counter, the angle block, the limit words and the
    /// copies.
    fn store_efe0_state(&mut self, this: u32, state: &RefreshState) {
        self.set_u32(state.link_address, state.link_flag);
        set_words(self, this + HINGE_CBASE + 8, &[state.dirty]);
        set_words(self, this + HINGE_CBASE_COUNTER, &[state.dirty_counter]);
        set_words(self, this + HINGE_ANGLE_BLOCK, &state.angle_block);
        set_words(self, this + HINGE_LIMIT_STATE, &state.spring_position);
        set_words(self, this + HINGE_POSITION_COPY, &state.position_copy);
        set_words(self, this + HINGE_ROTATION_COPY, &state.orientation_copy);
        set_words(self, this + HINGE_OFFSET_COPY, &state.offset_copy);
        set_words(self, this + HINGE_OUT_COPY, &state.offset_result_copy);
    }

    /// The addresses `SimEFE0` passes to its calls: the definition's blocks (the `VfE070` pointers, `BodyCompose`'s `a` and
    /// output) and the body's car. `d860_out` is the caller's scratch vector for `SimD860`.
    fn efe0_frame(&self, this: u32, d860_out: u32) -> RefreshHandles {
        let def = self.u32(this + HINGE_DEF);
        RefreshHandles {
            body: this,
            car: self.u32(this + HINGE_OWNER),
            first_axes: [def + DEF_POSE, def + DEF_SETTLE, def + 0x2E0, def + 0x310, def + DEF_MATRIX, def + 0x404],
            second_axes: [def + DEF_CONTEXT, def + 0x380, def + 0x3A0, def + DEF_VECTOR, def + 0x44C, def + 0x428],
            compose_input: def + DEF_POSE,
            compose_output: def + DEF_ANCHOR,
            offset_out: d860_out,
        }
    }

    /// The addresses `SimD160` passes to its calls.
    fn d160_frame(&self, this: u32) -> PoseHandles {
        let def = self.u32(this + HINGE_DEF);
        PoseHandles {
            car: self.u32(this + HINGE_OWNER),
            hinge_point_rf: def + DEF_POSE,
            hinge_axis_rf: def + DEF_SETTLE,
            identity: self.u32(IDENTITY_GLOBAL),
            transformed_hinge_point: this + HINGE_OFFSET,
            carried_hinge_axis: this + HINGE_WALK_OUT,
            body: this,
            frame_matrix: this + HINGE_MATRIX,
            quaternion_out: this + HINGE_BODY_ROTATION,
        }
    }

    /// The definition's frame words, and the body's block and position, as they are in memory now (`SimD160`'s reads).
    fn d160_words(&self, this: u32) -> PoseState {
        let def = self.u32(this + HINGE_DEF);
        PoseState {
            definition: PoseDefinition {
                composed_hinge_point: vec3(self, def + DEF_ANCHOR),
                inverse_origin_bf: vec3(self, def + DEF_COLUMN_WEIGHTS),
                axes_rf: mat3(self, def + DEF_MATRIX),
                inverse_axes_bf: mat3(self, def + DEF_SECOND_MAT),
            },
            body: PoseBody {
                orientation_matrix: mat3(self, this + HINGE_QUAT_MATRIX),
                frame_matrix: mat3(self, this + HINGE_MATRIX),
                hinge_rotation: mat3(self, this + HINGE_ROTATION),
            },
            position: self.f32s(this + HINGE_POSITION),
        }
    }

    /// Writes the body's products and its position (`SimD160`'s stores). `hinge_rotation` is not written: the port does not
    /// change it.
    fn store_d160_words(&mut self, this: u32, state: &PoseState) {
        set_mat3(self, this + HINGE_QUAT_MATRIX, &state.body.orientation_matrix);
        set_mat3(self, this + HINGE_MATRIX, &state.body.frame_matrix);
        for (i, value) in state.position.iter().enumerate() {
            self.set_f32(this + HINGE_POSITION + 4 * i as u32, *value);
        }
    }

    fn limit_inputs(&self, this: u32) -> Limits {
        let tweak = self.u32(this + LIMIT_TWEAK);
        Limits {
            angle: self.f32(this + LIMIT_VALUE),
            rate: self.f32(this + LIMIT_RESIDUAL),
            restitution: self.f32(tweak + LIMIT_FACTOR),
            max_angle: load_window(self, tweak + LIMIT_UPPER),
            min_angle: load_window(self, tweak + LIMIT_LOWER),
        }
    }

    fn store_limit_window(&mut self, this: u32, window: [u32; 3]) {
        for (i, word) in window.into_iter().enumerate() {
            self.set_u32(this + LIMIT_WINDOW + 4 * i as u32, word);
        }
    }

    fn store_limit_rate(&mut self, this: u32, rate: f32) {
        self.set_f32(this + LIMIT_RESIDUAL, rate);
    }

    fn advance_input(&self, this: u32) -> AdvanceInput {
        AdvanceInput {
            solved: self.f32(this + HINGE_SOLVED),
            rate: self.f32(this + HINGE_RATE),
            angle: self.f32(this + HINGE_ANGLE),
        }
    }

    fn store_rate(&mut self, this: u32, rate: f32) {
        self.set_f32(this + HINGE_RATE, rate);
    }

    fn store_angle(&mut self, this: u32, angle: f32) {
        self.set_f32(this + HINGE_ANGLE, angle);
    }

    fn rate_axis(&self, this: u32) -> u32 {
        self.u32(this + HINGE_DEF) + DEF_SCALE
    }

    fn clear_linear_velocity(&mut self, this: u32) {
        Vec4::from([0.0; 4]).store(self, this + HINGE_FIRST);
    }

    /// The payload's reads: the key, the rotation block's vector (+0x100, or 0 when `this` is -0xf0) and the position
    /// (+0x140).
    fn state_payload(&self, this: u32) -> StatePayload {
        let orientation_at = if this.wrapping_add(0xF0) != 0 { this + HINGE_BODY_ROTATION } else { 0 };
        StatePayload {
            key: self.u32(this + HINGE_KEY),
            orientation: self.f32s(orientation_at),
            position: self.f32s(this + HINGE_POSITION),
        }
    }

    /// The key, the orientation at +0x20, then the position (`E01Vec4::CopyFrom`) at +0x14, which overwrites the
    /// orientation's first lane (+0x20). The padding words are not written, as the original does not write them.
    fn store_state_payload(&mut self, sig: u32, payload: &StatePayload) {
        self.set_u32(sig, payload.key);
        Vec4::from(payload.orientation).store(self, sig + 0x20);
        Vec4::from(payload.position).store(self, sig + 0x14);
    }

    fn state_signal(&self) -> u32 {
        physics_core::layout::signal_manager(self) + STATE_SIGNAL
    }
}

//! The Mem store of the rigid body: the layout's reads and writes behind `RigidStore`.

use super::contacts::{self, CONTACT_LIST, HALF, HOOK, RING_TAIL};
use super::scratch_state::load_quat;
use super::solid_ctor::SOLID_STATE;
use super::{
    Body, Def, ParentFrame, Vec4, ANGULAR_VELOCITY, BODY_FLAG_8, DEFINITION, DEF_INVERSE_MASS, DEF_VERY_SOFT,
    ENTITY_FLAGS, LINEAR_VELOCITY, POSITION, SCRATCH_ACCUM, SCRATCH_SCALE, START_ORIENTATION, START_POSITION,
};
use crate::math::quat::Quat;
use crate::math::vec::Vec3;
use crate::mem::Mem;
use crate::physics_core::layout as core_layout;
use crate::physics_core::math::Mat;
use crate::rigid_body::contact::Input;
use crate::rigid_body::rows::RowFrame;
use crate::rigid_body::store::{HookPair, Motion, RigidStore, RingHeader, StateRotation};

/// The physics global (`g_1659b70`), whose `+0x24` is the settings object.
const PHYSICS_GLOBAL: u32 = 0x1659b70;
/// The settings' `MaxTimeStep`, and the two limits the velocity clamp reads.
const MAX_TIME_STEP: u32 = 0x1608C;
const CLAMP_LIMIT: u32 = 0x1610c;
const CLAMP_SCALE: u32 = 0x1611c;

/// The body's matrix, ten words from `+0x110` (the state's matrix at `+0x30`).
const MATRIX: u32 = 0x110;

/// The state's orientation, scale diagonal and gate, and where its refreshed matrix goes.
const STATE_ORIENTATION: u32 = 0x20;
const STATE_DIAGONAL: u32 = 0x70;
const STATE_GATE: u32 = 0x7c;
const STATE_MATRIX: u32 = 0x30;

fn settings(mem: &Mem) -> u32 {
    mem.u32(mem.u32(PHYSICS_GLOBAL) + 0x24)
}

impl RigidStore for Mem {
    fn body(&self, this: u32) -> Body {
        Body::load(self, this)
    }

    fn store_body(&mut self, this: u32, body: &Body) {
        body.store(self, this);
    }

    fn definition(&self, this: u32) -> Def {
        Def::load(self, self.u32(this + DEFINITION))
    }

    fn parent_frame(&self, parent: u32) -> ParentFrame {
        ParentFrame::load(self, parent)
    }

    fn def_very_soft(&self, this: u32) -> bool {
        self.u32(self.u32(this + DEFINITION) + DEF_VERY_SOFT) != 0
    }

    fn inverse_mass(&self, this: u32) -> f32 {
        let definition = self.u32(this + DEFINITION);
        self.f32(definition + DEF_INVERSE_MASS)
    }

    fn row(&self, at: u32) -> Vec4 {
        Vec4::load(self, at)
    }

    fn store_row(&mut self, at: u32, row: Vec4) {
        row.store(self, at);
    }

    fn store_row3(&mut self, at: u32, row: Vec4) {
        self.set_f32(at, row.x);
        self.set_f32(at + 4, row.y);
        self.set_f32(at + 8, row.z);
    }

    fn vec3(&self, at: u32) -> Vec3 {
        Vec3::load(self, at)
    }

    fn store_vec3(&mut self, at: u32, value: Vec3) {
        value.store(self, at);
    }

    fn row_frame(&self, this: u32) -> RowFrame {
        RowFrame { m: self.f32s(this + MATRIX), origin: Vec3::load(self, this + POSITION).extend(0.0) }
    }

    fn matrix_words(&self, this: u32) -> [f32; 9] {
        self.f32s(this + MATRIX)
    }

    fn matrix(&self, this: u32) -> Mat {
        core_layout::mat(self, this + MATRIX)
    }

    fn position(&self, this: u32) -> Vec4 {
        Vec4::load(self, this + POSITION)
    }

    fn motion(&self, this: u32) -> Motion {
        Motion {
            position: Vec3::load(self, this + POSITION),
            angular_velocity: Vec3::load(self, this + ANGULAR_VELOCITY),
            linear_velocity: Vec3::load(self, this + LINEAR_VELOCITY),
        }
    }

    fn linear_velocity(&self, this: u32) -> Vec4 {
        Vec4::load(self, this + LINEAR_VELOCITY)
    }

    fn store_linear_velocity(&mut self, this: u32, value: Vec4) {
        value.store(self, this + LINEAR_VELOCITY);
    }

    fn start_orientation(&self, this: u32) -> Quat {
        load_quat(self, this + START_ORIENTATION)
    }

    fn start_offset(&self, this: u32) -> Vec4 {
        Vec3::load(self, this + START_POSITION).extend(0.0)
    }

    fn impulse_limits(&self) -> (f32, f32) {
        let settings = settings(self);
        (self.f32(settings + CLAMP_LIMIT), self.f32(settings + CLAMP_SCALE))
    }

    fn contact_input(&self, this: u32, contact: u32, range: f32) -> Input {
        Input {
            contact: contacts::load_contact(self, contact),
            inverse_mass: RigidStore::inverse_mass(self, this),
            max_step: self.f32(settings(self) + MAX_TIME_STEP),
            range,
            sample: self.u32(this + HOOK) != 0 && self.u8(this + ENTITY_FLAGS) & BODY_FLAG_8 != 0,
        }
    }

    fn contact_count(&self, this: u32) -> Option<u32> {
        let list = self.u32(this + CONTACT_LIST);
        (list != 0).then(|| self.u32(list + 4))
    }

    fn linear_velocity_at(&self, this: u32) -> u32 {
        this + LINEAR_VELOCITY
    }

    fn state_at(&self, this: u32) -> u32 {
        this + SOLID_STATE
    }

    fn hook_pair(&self, pair: u32) -> HookPair {
        HookPair { third: self.f32(pair + 8), half: self.f32(HALF), first: self.u32(pair), second: self.u32(pair + 4) }
    }

    fn hook(&self, this: u32) -> u32 {
        self.u32(this + HOOK)
    }

    fn ring_tail(&self, this: u32) -> u32 {
        self.u32(this + RING_TAIL)
    }

    fn ring_header(&self, ring: u32) -> RingHeader {
        contacts::ring_header(self, ring)
    }

    fn store_ring_entry(&mut self, ring: u32, index: u32, words: [u32; 3], src: u32) {
        contacts::store_ring_entry(self, ring, index, words, src);
    }

    fn store_ring_cursor(&mut self, ring: u32, index: u32, count: u32) {
        contacts::store_ring_cursor(self, ring, index, count);
    }

    fn scratch_scale(&self, this: u32) -> f32 {
        self.f32(this + SCRATCH_SCALE)
    }

    fn scratch_accum(&self, this: u32) -> Vec4 {
        Vec4::load(self, this + SCRATCH_ACCUM)
    }

    fn store_scratch_accum(&mut self, this: u32, value: Vec4) {
        value.store(self, this + SCRATCH_ACCUM);
    }

    fn state_rotation(&self, state: u32) -> StateRotation {
        StateRotation {
            orientation: load_quat(self, state + STATE_ORIENTATION),
            gate: self.f32(state + STATE_GATE),
            diagonal: [
                self.f32(state + STATE_DIAGONAL),
                self.f32(state + STATE_DIAGONAL + 4),
                self.f32(state + STATE_DIAGONAL + 8),
            ],
        }
    }

    fn store_state_matrix(&mut self, state: u32, matrix: [f32; 9]) {
        for (k, v) in matrix.into_iter().enumerate() {
            self.set_f32(state + STATE_MATRIX + 4 * k as u32, v);
        }
    }
}

//! The loads and stores of each tyre kernel call (`ComputeSlip`, `Sign`, `Accumulate`, the contact solve,
//! `Friction`, `HeatStep`, `TableStep`, `RelaxStep`, `ComputeOutputs`, `StepInner`, `Step`, `BlendTable`), and the
//! small tyre calls (the tail getters, the noise signal).

use crate::math::vec::Vec4;
use crate::mem::Mem;

use super::{
    SlipMatrix, BLEND_HI, BLEND_LO, DEF, DEF_CARCASS_RADIUS, DEF_CARCASS_STIFFNESS_LATERAL,
    DEF_CARCASS_STIFFNESS_TORSIONAL, DEF_CARCASS_WIDTH, DEF_DEBUG_ID, DEF_REFERENCE_FRICTION, DEF_TREAD_COVERAGE,
    DEF_TREAD_DEPTH, DEF_TREAD_WIDTH, LONG_CONTACT_VEL, SLIP_MATRIX, SLIP_RATIO_TERM, SLIP_X, SLIP_Y, SPEED, SPEED_SQ,
    SPIN, STATE, STATE_EFFECTIVE_RADIUS, STATE_LERP_WEIGHT, STATE_TYRE_TEMPERATURE,
};
use crate::physics_core::layout::SignalHost;
use crate::spring_damper::{AccumulationFactors, ForceTerms};
use crate::tyre::force::{AccumulateInput, Accumulated};
use crate::tyre::friction::FrictionInput;
use crate::tyre::heat::{HeatInput, HeatOutput};
use crate::tyre::material::{Material, WORDS};
use crate::tyre::store::TyreStore;

use super::frame::{store_floats, Table};
use super::frame_types::{Frame, Scratch, Segment, Setup, StateWords, TableEntry};
use crate::tyre::outputs::{BlendPair, ComputeInput, ComputeOutputs};
use crate::tyre::relax::{BlendInput, BlendOut, RelaxInput, RelaxOutput, TableInput, TableOutput};
use crate::tyre::solve::Contact;
use crate::tyre::step_inner::{StepInput, StepOutput};
use crate::{ntweak, physics_core};

/// Results of `ComputeSlip`, stored at `this+0x360..0x37c`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slip {
    /// The slip velocity components (`+0x368`, `+0x36c`, `+0x370`).
    pub v: [f32; 3],
    /// `v0² + v1²` (`+0x364`).
    pub speed_sq: f32,
    /// `sqrt(speed_sq)` (`+0x360`), m/s.
    pub speed: f32,
    /// `R·ω + v1`: the longitudinal velocity of the contact point (`+0x374`).
    pub long_contact_vel: f32,
    /// `-(1/R)·v1 - ω` (`+0x37c`).
    pub slip_ratio_term: f32,
}

/// The state values the tyre functions read (`CState`, `m_state`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    /// `R` (`m_state+0x4c`), the effective radius, m.
    pub effective_radius: f32,
    /// The lerp weight `w` of `Accumulate` (`m_state+0x58`).
    pub lerp_weight: f32,
}

/// The `Sign` inputs (`CScratch::Sign`, see `force::signed_term`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignInput {
    /// `this+0x30c`: a tyre factor (rbr-recomp `notes/physics/tyre.md` 2a, `F[0xC3]`).
    pub tyre_factor: f32,
    /// `this+0x298`: unknown meaning.
    pub factor_298: f32,
    /// `this+0x314`: the second tyre factor (`F[0xC5]`).
    pub tyre_factor_2: f32,
    /// `this+0x3dc`: unknown meaning.
    pub factor_3dc: f32,
    /// `this+0x378`: the wheel's spin rate ω, rad/s (stored as float bits by the original).
    pub spin: f32,
}

/// Loads the two factors of a `DamperAccumA` block at `at`.
fn load_factors(mem: &Mem, at: u32) -> AccumulationFactors {
    AccumulationFactors { first: mem.f32(at), second: mem.f32(at + 4) }
}

/// Loads `DamperForce`'s four words at `at`: the multiplier, the accumulated force, the response and the divisor.
fn load_force_terms(mem: &Mem, at: u32) -> ForceTerms {
    ForceTerms {
        multiplier: mem.f32(at),
        accumulated_force: mem.f32(at + 4),
        response: mem.f32(at + 8),
        divisor: mem.f32(at + 12),
    }
}

/// Stores `DamperForce`'s four words at `at` (see `load_force_terms`).
fn store_force_terms(mem: &mut Mem, at: u32, t: ForceTerms) {
    mem.set_f32(at, t.multiplier);
    mem.set_f32(at + 4, t.accumulated_force);
    mem.set_f32(at + 8, t.response);
    mem.set_f32(at + 12, t.divisor);
}

/// `v * s + base` over the four lanes (the asm's `mulps` by `s`, then `addps` to `base`).
pub fn scaled_sum(v: Vec4, s: f32, base: Vec4) -> Vec4 {
    Vec4 { x: v.x * s + base.x, y: v.y * s + base.y, z: v.z * s + base.z, w: v.w * s + base.w }
}

/// The address of word `index` of the table at `base`. The indices are Locate's `lo`, which is -1 (0xffffffff) when a NaN
/// input walks below the table, so the address wraps as the original's 32-bit one does.
pub fn word_at(base: u32, index: u32) -> u32 {
    base.wrapping_add(index.wrapping_mul(4))
}

/// The tyre index of `RelaxStep` and `TableStep`: `m_state+0x1c & 7`.
fn relax_index(mem: &Mem, state: u32) -> u32 {
    mem.u32(state + 0x1C) & 7
}

/// The blend that `ComputeOutputs` and `StepInner` share: the weight at `m_state+0xa4`, and the four `CDef` tables
/// (`+0x244`, `+0x264`, `+0x284`, `+0x2a4`) at the state's indices `m_state+0x9c` and `+0xa0`.
fn load_blend(mem: &Mem, this: u32) -> (f32, [BlendPair; 4]) {
    let state = mem.u32(this + STATE);
    let def = mem.u32(this + DEF);
    let (a, b) = (mem.u32(state + 0x9C), mem.u32(state + 0xA0));
    let pair = |base: u32| BlendPair { at_a: mem.f32(word_at(def + base, a)), at_b: mem.f32(word_at(def + base, b)) };
    (mem.f32(state + 0xA4), [pair(0x244), pair(0x264), pair(0x284), pair(0x2A4)])
}

/// The output block that `WriteOutputs` fills (the caller's struct, one stack word): `+0x10`, `+0x20` and `+0x50`
/// vectors, `+0x80` and `+0x84` the flags, `+0x88`, `+0x90`, `+0x94` and `+0x98` the scalars, `+0xac` set to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputBlock {
    pub v10: Vec4,
    pub v20: Vec4,
    pub v50: Vec4,
    pub f80: f32,
    pub f84: f32,
    pub f88: f32,
    pub u90: u32,
    pub u94: u32,
    pub f98: f32,
    pub flag_ac: u32,
}

/// The stack words of `Step` as typed values, loaded once at entry. `spin`, `dt`, `c` and `d` are the float words
/// `a` to `d`; `pair` is the three dwords at `f`, and `out_m` is the caller's `m` vector, which `SolveForces` adds to.
#[derive(Clone, Copy, Debug)]
pub struct StepArgs {
    pub spin: f32,
    pub dt: f32,
    pub c: f32,
    pub d: f32,
    pub material: Material,
    pub pair: [u32; 3],
    /// `m_step_flag`: the game flag that copies `pair` to `+0x4c8`.
    pub pair_copy: bool,
    pub align_a: Vec4,
    pub dir: Vec4,
    pub out_a: Vec4,
    pub vec_k: Vec4,
    pub out_m: Vec4,
}

/// Where `Step` stores its outputs (the stack words `l`, `m`, `n`, `o`, `p`).
#[derive(Clone, Copy, Debug)]
pub struct StepPtrs {
    pub out_l: u32,
    pub out_m: u32,
    pub out_n: u32,
    pub out_o: u32,
    pub out_p: u32,
}

/// The outputs of `Step`. The no-contact path sets `l`, `m` and `n` to zero and leaves `o` and the output block as
/// they were, so those are `None` there.
#[derive(Clone, Copy, Debug)]
pub struct StepOut {
    pub l: Vec4,
    pub m: Vec4,
    pub n: f32,
    pub o: Option<f32>,
    pub block: Option<OutputBlock>,
}

/// `CTyre::CScratch::EmitNoise` (0x535890, thiscall, no arguments): emits the tyre-noise signal with the scratch's signal
/// block (`m_sig`, +0x420), then clears the block's second word (+0x424).
pub fn emit_noise<S: TyreStore>(mem: &mut S, host: &mut impl SignalHost, this: u32) {
    let signal = mem.noise_signal();
    let sig = this + 0x420;
    physics_core::layout::emit(mem.mem_mut(), host, signal, sig);
    mem.set_u32(sig + 4, 0);
}

/// `CU32Value8::Clear` (0x5317f0, thiscall): the eight floats at `this` become 0.0.
pub fn value8_clear<S: TyreStore>(mem: &mut S, this: u32) {
    for k in 0..8 {
        mem.set_f32(this + 4 * k, 0.0);
    }
}

/// The Mem store: the bodies are the layout's reads and writes.
impl TyreStore for Mem {
    /// Loads a [`Segment`] at `at`: `lo`, `hi` and `frac` in three words.
    fn load_segment(&self, at: u32) -> Segment {
        let mem = self;
        Segment { lo: mem.u32(at), hi: mem.u32(at + 4), frac: mem.f32(at + 8) }
    }

    /// Stores a [`Segment`] at `at`.
    fn store_segment(&mut self, at: u32, s: &Segment) {
        let mem = self;
        mem.set_u32(at, s.lo);
        mem.set_u32(at + 4, s.hi);
        mem.set_f32(at + 8, s.frac);
    }

    /// The table of `len` entries at `at`, with the word below it.
    fn read_table(&self, at: u32, len: usize) -> Table {
        let mem = self;
        Table {
            below: mem.f32(at - 4),
            len,
            entries: std::array::from_fn(|k| if k < len { mem.f32(at + 4 * k as u32) } else { 0.0 }),
        }
    }

    /// Loads the whole tyre step footprint of the scratch at `this`: its state, its setup, its wear table and the
    /// contact record's angle. The pointers are read from `this` (`+0x10` setup, `+0x14` state, `+0x410` record,
    /// `+0x414` wear table).
    fn load_frame(&self, this: u32) -> Frame {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let record = mem.u32(this + 0x410);
        let wear = mem.u32(this + 0x414);
        let f = |off: u32| mem.f32(this + off);
        let scratch = Scratch {
            k_vec: Vec4::load(mem, this + 0x160),
            contact_frame: std::array::from_fn(|r| Vec4::load(mem, this + SLIP_MATRIX + 0x10 * r as u32)),
            material: mem.load_material(this + 0x1A0),
            dirty_1e4: mem.u32(this + 0x1E4),
            f_1e8: f(0x1E8),
            flag_1f4: mem.u32(this + 0x1F4),
            factor_1f8: f(0x1F8),
            f_208: f(0x208),
            f_218: f(0x218),
            g_228: f(0x228),
            f96: f(0x258),
            heat_source: f(0x288),
            factor_298: f(0x298),
            scale_300: f(0x300),
            r304: f(0x304),
            f308: f(0x308),
            tyre_factor: f(0x30C),
            f310: f(0x310),
            tyre_factor_2: f(0x314),
            f318: f(0x318),
            f31c: f(0x31C),
            ratio_324: f(0x324),
            ratio_328: f(0x328),
            contact_p: mem.f32s(this + 0x32C),
            force_sum: f(0x348),
            nh: f(0x34C),
            zero_350: f(0x350),
            force_t1: f(0x354),
            force_b: f(0x358),
            force_a: f(0x35C),
            speed: f(SPEED),
            speed_sq: f(SPEED_SQ),
            slip_v: mem.f32s(this + SLIP_X),
            long_contact_vel: f(LONG_CONTACT_VEL),
            spin: f(SPIN),
            slip_ratio_term: f(SLIP_RATIO_TERM),
            f380: f(0x380),
            f384: f(0x384),
            f_e2: f(0x388),
            heat_flow_e3: f(0x38C),
            heat_flow_e4: f(0x390),
            f394: f(0x394),
            f398: f(0x398),
            f39c: f(0x39C),
            f3a0: f(0x3A0),
            f3a4: f(0x3A4),
            arg_c: f(0x3A8),
            f3ac: f(0x3AC),
            scale_3b0: f(0x3B0),
            blend: f(0x3B4),
            f3b8: f(0x3B8),
            p3bc: f(0x3BC),
            tyre_index: mem.u32(this + 0x3C0),
            heat_t: f(0x3C4),
            heat_f2: f(0x3C8),
            f3cc: f(0x3CC),
            f3d0: f(0x3D0),
            f3d4: f(0x3D4),
            arg_d: f(0x3D8),
            f3dc: f(0x3DC),
            f3e0: f(0x3E0),
            f3e4: f(0x3E4),
            f3e8: f(0x3E8),
            f3ec: f(0x3EC),
            gate: f(0x3F0),
            f3f4: f(0x3F4),
            f3f8: f(0x3F8),
            f3fc: f(0x3FC),
            f400: f(0x400),
            flow: f(0x404),
            contact_angle: mem.f32(record + 0x5AC),
            wear: mem.f32s(wear),
            signal: std::array::from_fn(|k| mem.u32(this + 0x420 + 4 * k as u32)),
            contact_flag: mem.u32(this + 0x480),
            tyre_index_copy: mem.u32(this + 0x484),
            copy_490: Vec4::load(mem, this + 0x490),
            copy_4a0: Vec4::load(mem, this + 0x4A0),
            copy_4b0: mem.u32(this + 0x4B0),
            copy_4b4: mem.u32(this + 0x4B4),
            copy_4b8: mem.u32(this + 0x4B8),
            e3: f(0x4BC),
            x4c0: f(0x4C0),
            y4c4: f(0x4C4),
            pair: std::array::from_fn(|k| mem.u32(this + 0x4C8 + 4 * k as u32)),
            heat_r: f(0x4E0),
            heat_sum_ab: f(0x4E4),
            heat_t_inv: f(0x4EC),
            ngp_4e8: f(0x4E8),
            acc_504: f(0x504),
            acc_508: f(0x508),
            acc_50c: f(0x50C),
        };
        let st = |off: u32| mem.f32(state + off);
        let table = std::array::from_fn(|k| {
            let at = state + 0xD0 + 12 * k as u32;
            TableEntry { value: mem.f32(at), a: mem.f32(at + 4), b: mem.f32(at + 8) }
        });
        let state_words = StateWords {
            flag_0c: mem.u32(state + 0x0C),
            dirty_28: mem.u32(state + 0x28),
            flag_2c: mem.u32(state + 0x2C),
            vec_30: Vec4::load(mem, state + 0x30),
            k40: st(0x40),
            k44: st(0x44),
            tread_48: st(0x48),
            radius: st(STATE_EFFECTIVE_RADIUS),
            s50: st(0x50),
            s54: st(0x54),
            lerp_weight: st(STATE_LERP_WEIGHT),
            grip_flag: mem.u32(state + 0x5C),
            s68: st(0x68),
            contact_70: mem.u32(state + 0x70),
            contact_74: mem.u32(state + 0x74),
            sv: Vec4::load(mem, state + 0x80),
            seg_blend: mem.load_segment(state + 0x90),
            seg_heat: mem.load_segment(state + 0x9C),
            seg_shape: mem.load_segment(state + 0xA8),
            seg_shape2: mem.load_segment(state + 0xB4),
            seg_mix: mem.load_segment(state + 0xC0),
            table,
            state_130: st(0x130),
            tyre_index: mem.u32(state + 0x134),
        };
        let setup = Setup {
            debug_id: mem.u32(def + DEF_DEBUG_ID),
            f2c: mem.f32(def + 0x2C),
            f3c: mem.f32(def + DEF_REFERENCE_FRICTION),
            f4c: mem.f32(def + DEF_CARCASS_STIFFNESS_LATERAL),
            f5c: mem.f32(def + DEF_CARCASS_STIFFNESS_TORSIONAL),
            f7c: mem.f32(def + DEF_CARCASS_RADIUS),
            f8c: mem.f32(def + DEF_TREAD_DEPTH),
            f9c: mem.f32(def + 0x9C),
            fac: mem.f32(def + DEF_TREAD_WIDTH),
            fbc: mem.f32(def + DEF_TREAD_COVERAGE),
            fcc: mem.f32(def + DEF_CARCASS_WIDTH),
            fec: mem.f32(def + 0xEC),
            f310: mem.f32(def + 0x310),
            f10c: mem.f32(def + 0x10C),
            damper_110: mem.f32s(def + 0x110),
            damper_11c: mem.f32s(def + 0x11C),
            damper_128: mem.f32s(def + 0x128),
            def_118: mem.f32(def + 0x118),
            t134: mem.read_table(def + 0x134, 4),
            t144: mem.read_table(def + 0x144, 4),
            t154: mem.read_table(def + 0x154, 6),
            t16c: mem.read_table(def + 0x16C, 6),
            t184: mem.read_table(def + 0x184, 8),
            t1a4: mem.read_table(def + 0x1A4, 8),
            t1c4: mem.read_table(def + 0x1C4, 8),
            t1e4: mem.read_table(def + 0x1E4, 8),
            t204: mem.read_table(def + 0x204, 8),
            t224: mem.read_table(def + 0x224, 8),
            t244: mem.read_table(def + 0x244, 8),
            t264: mem.read_table(def + 0x264, 8),
            t284: mem.read_table(def + 0x284, 8),
            t2a4: mem.read_table(def + 0x2A4, 8),
        };
        Frame { scratch, state: state_words, setup }
    }

    /// Stores the tyre step footprint back to the scratch at `this` (its state and scratch; the setup and the contact
    /// angle are read only). Every word is written, with the value it was loaded with unless the port changed it.
    fn store_frame(&mut self, this: u32, frame: &Frame) {
        let mem = self;
        let state = mem.u32(this + STATE);
        let wear = mem.u32(this + 0x414);
        let s = &frame.scratch;
        // The material block first: ten of its words are scalars below, which hold the frame's latest values.
        mem.store_material(this + 0x1A0, &s.material);
        mem.set_f32(this + 0x1E8, s.f_1e8);
        mem.set_f32(this + 0x1F8, s.factor_1f8);
        mem.set_f32(this + 0x208, s.f_208);
        mem.set_f32(this + 0x218, s.f_218);
        mem.set_f32(this + 0x228, s.g_228);
        mem.set_f32(this + 0x258, s.f96);
        mem.set_f32(this + 0x288, s.heat_source);
        mem.set_f32(this + 0x298, s.factor_298);
        mem.set_f32(this + 0x300, s.scale_300);
        mem.set_f32(this + 0x304, s.r304);
        mem.set_f32(this + 0x308, s.f308);
        mem.set_f32(this + 0x30C, s.tyre_factor);
        mem.set_f32(this + 0x310, s.f310);
        mem.set_f32(this + 0x314, s.tyre_factor_2);
        mem.set_f32(this + 0x318, s.f318);
        mem.set_f32(this + 0x31C, s.f31c);
        mem.set_f32(this + 0x324, s.ratio_324);
        mem.set_f32(this + 0x328, s.ratio_328);
        store_floats(mem, this + 0x32C, &s.contact_p);
        mem.set_f32(this + 0x348, s.force_sum);
        mem.set_f32(this + 0x34C, s.nh);
        mem.set_f32(this + 0x350, s.zero_350);
        mem.set_f32(this + 0x354, s.force_t1);
        mem.set_f32(this + 0x358, s.force_b);
        mem.set_f32(this + 0x35C, s.force_a);
        mem.set_f32(this + SPEED, s.speed);
        mem.set_f32(this + SPEED_SQ, s.speed_sq);
        store_floats(mem, this + SLIP_X, &s.slip_v);
        mem.set_f32(this + LONG_CONTACT_VEL, s.long_contact_vel);
        mem.set_f32(this + SPIN, s.spin);
        mem.set_f32(this + SLIP_RATIO_TERM, s.slip_ratio_term);
        mem.set_f32(this + 0x380, s.f380);
        mem.set_f32(this + 0x384, s.f384);
        mem.set_f32(this + 0x388, s.f_e2);
        mem.set_f32(this + 0x38C, s.heat_flow_e3);
        mem.set_f32(this + 0x390, s.heat_flow_e4);
        mem.set_f32(this + 0x394, s.f394);
        mem.set_f32(this + 0x398, s.f398);
        mem.set_f32(this + 0x39C, s.f39c);
        mem.set_f32(this + 0x3A0, s.f3a0);
        mem.set_f32(this + 0x3A4, s.f3a4);
        mem.set_f32(this + 0x3A8, s.arg_c);
        mem.set_f32(this + 0x3AC, s.f3ac);
        mem.set_f32(this + 0x3B0, s.scale_3b0);
        mem.set_f32(this + 0x3B4, s.blend);
        mem.set_f32(this + 0x3B8, s.f3b8);
        mem.set_f32(this + 0x3BC, s.p3bc);
        mem.set_f32(this + 0x3C4, s.heat_t);
        mem.set_f32(this + 0x3C8, s.heat_f2);
        mem.set_f32(this + 0x3CC, s.f3cc);
        mem.set_f32(this + 0x3D0, s.f3d0);
        mem.set_f32(this + 0x3D4, s.f3d4);
        mem.set_f32(this + 0x3D8, s.arg_d);
        mem.set_f32(this + 0x3DC, s.f3dc);
        mem.set_f32(this + 0x3E0, s.f3e0);
        mem.set_f32(this + 0x3E4, s.f3e4);
        mem.set_f32(this + 0x3E8, s.f3e8);
        mem.set_f32(this + 0x3EC, s.f3ec);
        mem.set_f32(this + 0x3F0, s.gate);
        mem.set_f32(this + 0x3F4, s.f3f4);
        mem.set_f32(this + 0x3F8, s.f3f8);
        mem.set_f32(this + 0x3FC, s.f3fc);
        mem.set_f32(this + 0x400, s.f400);
        mem.set_f32(this + 0x404, s.flow);
        store_floats(mem, wear, &s.wear);
        for (k, word) in s.signal.iter().enumerate() {
            mem.set_u32(this + 0x420 + 4 * k as u32, *word);
        }
        mem.set_u32(this + 0x480, s.contact_flag);
        mem.set_u32(this + 0x484, s.tyre_index_copy);
        s.copy_490.store(mem, this + 0x490);
        s.copy_4a0.store(mem, this + 0x4A0);
        mem.set_u32(this + 0x4B0, s.copy_4b0);
        mem.set_u32(this + 0x4B4, s.copy_4b4);
        mem.set_u32(this + 0x4B8, s.copy_4b8);
        mem.set_f32(this + 0x4BC, s.e3);
        mem.set_f32(this + 0x4C0, s.x4c0);
        mem.set_f32(this + 0x4C4, s.y4c4);
        for (k, word) in s.pair.iter().enumerate() {
            mem.set_u32(this + 0x4C8 + 4 * k as u32, *word);
        }
        mem.set_f32(this + 0x4E0, s.heat_r);
        mem.set_f32(this + 0x4E4, s.heat_sum_ab);
        mem.set_f32(this + 0x4EC, s.heat_t_inv);
        mem.set_f32(this + 0x4E8, s.ngp_4e8);
        mem.set_f32(this + 0x504, s.acc_504);
        mem.set_f32(this + 0x508, s.acc_508);
        mem.set_f32(this + 0x50C, s.acc_50c);
        s.k_vec.store(mem, this + 0x160);
        for (r, row) in s.contact_frame.iter().enumerate() {
            (*row).store(mem, this + SLIP_MATRIX + 0x10 * r as u32);
        }
        mem.set_u32(this + 0x1E4, s.dirty_1e4);
        mem.set_u32(this + 0x1F4, s.flag_1f4);
        mem.set_u32(this + 0x3C0, s.tyre_index);

        let st = &frame.state;
        mem.set_u32(state + 0x0C, st.flag_0c);
        mem.set_u32(state + 0x28, st.dirty_28);
        mem.set_u32(state + 0x2C, st.flag_2c);
        st.vec_30.store(mem, state + 0x30);
        store_floats(mem, state + 0x40, &[st.k40, st.k44, st.tread_48, st.radius, st.s50, st.s54]);
        mem.set_f32(state + STATE_LERP_WEIGHT, st.lerp_weight);
        mem.set_u32(state + 0x5C, st.grip_flag);
        mem.set_f32(state + 0x68, st.s68);
        mem.set_u32(state + 0x70, st.contact_70);
        mem.set_u32(state + 0x74, st.contact_74);
        st.sv.store(mem, state + 0x80);
        mem.store_segment(state + 0x90, &st.seg_blend);
        mem.store_segment(state + 0x9C, &st.seg_heat);
        mem.store_segment(state + 0xA8, &st.seg_shape);
        mem.store_segment(state + 0xB4, &st.seg_shape2);
        mem.store_segment(state + 0xC0, &st.seg_mix);
        for (k, entry) in st.table.iter().enumerate() {
            let at = state + 0xD0 + 12 * k as u32;
            mem.set_f32(at, entry.value);
            mem.set_f32(at + 4, entry.a);
            mem.set_f32(at + 8, entry.b);
        }
        mem.set_f32(state + 0x130, st.state_130);
        mem.set_u32(state + 0x134, st.tyre_index);
    }

    /// Loads the copied words of the block at `at`.
    fn load_material(&self, at: u32) -> Material {
        let mem = self;
        std::array::from_fn(|i| mem.u32(at + 4 * WORDS[i]))
    }

    /// Stores the copied words into the block at `at`; the other words are left as they are.
    fn store_material(&mut self, at: u32, material: &Material) {
        let mem = self;
        for (i, word) in material.iter().enumerate() {
            mem.set_u32(at + 4 * WORDS[i], *word);
        }
    }

    /// Loads the state the tyre functions read. The state pointer is at `this+0x14`.
    fn load_state(&self, this: u32) -> State {
        let mem = self;
        let state = mem.u32(this + STATE);
        State {
            effective_radius: mem.f32(state + STATE_EFFECTIVE_RADIUS),
            lerp_weight: mem.f32(state + STATE_LERP_WEIGHT),
        }
    }

    /// Loads the slip matrix (`this+0x170`, rows 0x10 bytes apart).
    fn load_slip_matrix(&self, this: u32) -> SlipMatrix {
        let mem = self;
        std::array::from_fn(|r| mem.f32s(this + SLIP_MATRIX + 0x10 * r as u32))
    }

    /// Stores the `ComputeSlip` results.
    fn store_slip(&mut self, this: u32, slip: &Slip) {
        let mem = self;
        for (i, v) in slip.v.iter().enumerate() {
            mem.set_f32(this + SLIP_X + 4 * i as u32, *v);
        }
        mem.set_f32(this + SPEED_SQ, slip.speed_sq);
        mem.set_f32(this + SPEED, slip.speed);
        mem.set_f32(this + LONG_CONTACT_VEL, slip.long_contact_vel);
        mem.set_f32(this + SLIP_RATIO_TERM, slip.slip_ratio_term);
    }

    /// Loads the `Sign` inputs.
    fn load_sign_input(&self, this: u32) -> SignInput {
        let mem = self;
        SignInput {
            tyre_factor: mem.f32(this + 0x30c),
            factor_298: mem.f32(this + 0x298),
            tyre_factor_2: mem.f32(this + 0x314),
            factor_3dc: mem.f32(this + 0x3dc),
            spin: mem.f32(this + SPIN),
        }
    }

    /// Loads the `Accumulate` inputs: the scratch words, the state and the tyre setup (`this+0x10`, `m_def`).
    fn load_accumulate(&self, this: u32) -> AccumulateInput {
        let mem = self;
        let state = mem.load_state(this);
        let def = mem.u32(this + DEF);
        let f = |off: u32| mem.f32(this + off);
        AccumulateInput {
            blend_hi: f(BLEND_HI),
            blend_lo: f(BLEND_LO),
            tyre_factor: f(0x30c),
            tyre_factor_2: f(0x314),
            scale_238: f(0x238),
            slip_x: f(SLIP_X),
            slip_y: f(SLIP_Y),
            long_contact_vel: f(LONG_CONTACT_VEL),
            scale_300: f(0x300),
            scale_3b0: f(0x3b0),
            acc_504: f(0x504),
            acc_508: f(0x508),
            acc_50c: f(0x50c),
            effective_radius: state.effective_radius,
            lerp_weight: state.lerp_weight,
            setup_cc: mem.f32(def + DEF_CARCASS_WIDTH),
        }
    }

    /// Stores the `Accumulate` results.
    fn store_accumulate(&mut self, this: u32, out: &Accumulated) {
        let mem = self;
        let mut set = |off: u32, value: f32| mem.set_f32(this + off, value);
        set(0x35c, out.force_a);
        set(0x358, out.force_b);
        set(0x350, out.zero_350);
        set(0x314, out.factor_314);
        set(0x348, out.force_sum);
        set(0x354, out.force_t1);
        set(0x34c, out.nh);
        set(0x504, out.acc_504);
        set(0x508, out.acc_508);
        set(0x50c, out.acc_50c);
    }

    /// Stores the `Set414` value (`CScratch::Set414`, 0x532490).
    fn store_414(&mut self, this: u32, value: u32) {
        let mem = self;
        mem.set_u32(this + 0x414, value);
    }

    /// Loads the values a contact solve reads: `p` (7 words), `qb` (3 words) and `r` (8 words) at the given
    /// addresses, the contact offset at `this+0x258` and the state words at `+0x70` and `+0x74`.
    fn load_contact(&self, this: u32, p: u32, qb: u32, r: u32) -> Contact {
        let mem = self;
        let r = mem.f32s(r);
        mem.load_contact_with(this, p, qb, r)
    }

    /// `load_contact` with the `r` words given, for a caller whose `r` is in its frame (`ContactPatch`).
    fn load_contact_with(&self, this: u32, p: u32, qb: u32, r: [f32; 8]) -> Contact {
        let mem = self;
        let state = mem.u32(this + STATE);
        Contact {
            f96: mem.f32(this + 0x258),
            p: mem.f32s(p),
            qb: mem.f32s(qb),
            r,
            state_70: mem.u32(state + 0x70),
            state_74: mem.u32(state + 0x74),
        }
    }

    /// Stores the values a contact solve writes: `p`, `qb` and the state words (`this+0x14`).
    fn store_contact(&mut self, this: u32, p: u32, qb: u32, c: &Contact) {
        let mem = self;
        for (k, value) in c.p.iter().enumerate() {
            mem.set_f32(p + 4 * k as u32, *value);
        }
        for (k, value) in c.qb.iter().enumerate() {
            mem.set_f32(qb + 4 * k as u32, *value);
        }
        let state = mem.u32(this + STATE);
        mem.set_u32(state + 0x70, c.state_70);
        mem.set_u32(state + 0x74, c.state_74);
    }

    /// Loads the inputs of `Friction`: the gate, speed and factors of `this`, the vectors at `this+0x170`,
    /// `+0x180`, `+0x190`, the state's flag (`m_state+0xc`) and `sv` (`m_state+0x80`). `k` is its stack argument.
    fn load_friction(&self, this: u32, k: f32) -> FrictionInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        FrictionInput {
            gate: mem.f32(this + 0x3F0),
            state_flag: mem.u32(state + 0xC),
            speed: mem.f32(this + SPEED),
            factor_314: mem.f32(this + 0x314),
            factor_1f8: mem.f32(this + 0x1F8),
            dir: Vec4::load(mem, this + SLIP_MATRIX),
            rest: Vec4::load(mem, this + 0x180),
            axis: Vec4::load(mem, this + 0x190),
            sv: Vec4::load(mem, state + 0x80),
            k,
        }
    }

    /// Stores the state vector `Friction` leaves (`m_state+0x80`).
    fn store_friction(&mut self, this: u32, sv: Vec4) {
        let mem = self;
        let state = mem.u32(this + STATE);
        sv.store(mem, state + 0x80);
    }

    /// Loads the inputs of `HeatStep`. The state's indices (`m_state+0xa8`, `+0xac`, `+0xc0`, `+0xc4`) select the
    /// def entries they name, and `this+0x3c0` selects the table entry and the wear word.
    fn load_heat(&self, this: u32, a: f32, b: f32, c: f32) -> HeatInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let idx = mem.u32(this + 0x3C0);
        let wear = mem.u32(this + 0x414) + 4 * idx;
        HeatInput {
            a,
            b,
            c,
            heat_source: mem.f32(this + 0x288),
            f_da: mem.f32(this + SLIP_X),
            f_dd: mem.f32(this + LONG_CONTACT_VEL),
            f_de: mem.f32(this + SPIN),
            f_f2: mem.f32(this + 0x3C8),
            f_c4: mem.f32(this + 0x310),
            speed: mem.f32(this + SPEED),
            f_e2: mem.f32(this + 0x388),
            f_c2: mem.f32(this + 0x308),
            tab: mem.f32(state + 0xD0 + 12 * idx),
            state_12c: mem.f32(state + 0x12C),
            state_130: mem.f32(state + 0x130),
            state_b0: mem.f32(state + 0xB0),
            state_c8: mem.f32(state + 0xC8),
            mix_a: mem.f32(word_at(def + 0x16C, mem.u32(state + 0xC0))),
            mix_b: mem.f32(word_at(def + 0x16C, mem.u32(state + 0xC4))),
            shape_a: mem.f32(word_at(def + 0x1C4, mem.u32(state + 0xA8))),
            shape_b: mem.f32(word_at(def + 0x1C4, mem.u32(state + 0xAC))),
            def_118: mem.f32(def + 0x118),
            damper: [0x110, 0x11C, 0x128].map(|at| load_factors(mem, def + at)),
            wear: mem.f32(wear),
        }
    }

    /// Stores the `HeatStep` results: the scratch words, the table entry and the two damper states, and the wear.
    fn store_heat(&mut self, this: u32, out: &HeatOutput) {
        let mem = self;
        let state = mem.u32(this + STATE);
        let idx = mem.u32(this + 0x3C0);
        let wear = mem.u32(this + 0x414) + 4 * idx;
        mem.set_f32(this + 0x3C4, out.heat_t);
        mem.set_f32(this + 0x4E4, out.sum_ab);
        mem.set_f32(this + 0x4EC, out.t_inv);
        mem.set_f32(this + 0x4E0, out.r);
        mem.set_f32(this + 0x404, out.flow);
        mem.set_f32(state + 0xD0 + 12 * idx, out.tab);
        mem.set_f32(state + 0x12C, out.state_12c);
        mem.set_f32(state + 0x130, out.state_130);
        mem.set_f32(wear, out.wear);
    }

    /// Loads the `TableStep` inputs for table index `idx`: the entry at `m_state+0xd0+12·idx`, the speed, the
    /// state's `m_state+0x12c`, the time constant (`m_def+0x118`) and the two damper blocks (`m_def+0x110`, `+0x11c`).
    fn load_table(&self, this: u32, idx: u32) -> TableInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        TableInput {
            v: mem.f32(state + 0xD0 + 12 * idx),
            speed: mem.f32(this + SPEED),
            s4b: mem.f32(state + 0x12C),
            def_118: mem.f32(def + 0x118),
            s110: load_factors(mem, def + 0x110),
            s11c: load_factors(mem, def + 0x11C),
        }
    }

    /// Stores the `TableStep` results: `this+0x4f8`, the table entry at `idx`, and `m_state+0x12c`.
    fn store_table(&mut self, this: u32, idx: u32, out: &TableOutput) {
        let mem = self;
        let state = mem.u32(this + STATE);
        mem.set_f32(this + 0x4F8, out.f13e);
        mem.set_f32(state + 0xD0 + 12 * idx, out.v);
        mem.set_f32(state + 0x12C, out.s4b);
    }

    /// Loads the `RelaxStep(dt)` inputs: the carcass and tread damper states, the factors of the spring term
    /// (`m_def+0x7c`, `+0xac`, `+0x124`), the damper blocks, `DamperForce`'s state (`m_state+0x60..0x6c`) and the
    /// `TableStep` input at the tyre index (`m_state+0x1c & 7`).
    fn load_relax(&self, this: u32, dt: f32) -> RelaxInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        RelaxInput {
            dt,
            s130: mem.f32(state + 0x130),
            s19: mem.f32(state + STATE_TYRE_TEMPERATURE),
            dff_1f: mem.f32(def + DEF_CARCASS_RADIUS),
            dff_2b: mem.f32(def + DEF_TREAD_WIDTH),
            dff_49: mem.f32(def + 0x124),
            s11c: load_factors(mem, def + 0x11C),
            s128: load_factors(mem, def + 0x128),
            force: load_force_terms(mem, state + 0x60),
            table: mem.load_table(this, relax_index(mem, state)),
        }
    }

    /// Stores the `RelaxStep` results: `m_state+0x130`, `this+0x4fc`, `DamperForce`'s state, and the table step.
    fn store_relax(&mut self, this: u32, out: &RelaxOutput) {
        let mem = self;
        let state = mem.u32(this + STATE);
        mem.set_f32(state + 0x130, out.s130);
        mem.set_f32(this + 0x4FC, out.r3);
        store_force_terms(mem, state + 0x60, out.force);
        mem.store_table(this, relax_index(mem, state), &out.table);
    }

    /// Loads the `ComputeOutputs` inputs. `src` is stack word 6, and its words `+0x36c` and `+0x370` are copied through.
    /// `p1`, `p2`, `p4` and `p5` are stack words 1, 2, 4 and 5.
    fn load_compute_outputs(&self, this: u32, src: u32, p1: f32, p2: f32, p4: f32, p5: f32) -> ComputeInput {
        let mem = self;
        let (weight, [table_244, table_264, table_284, table_2a4]) = load_blend(mem, this);
        ComputeInput {
            weight,
            table_244,
            table_264,
            table_284,
            table_2a4,
            scale_1: p1,
            scale_2: p2,
            scale_4: p4,
            scale_5: p5,
            raw: [mem.u32(src + 0x36C), mem.u32(src + 0x370)],
        }
    }

    /// Loads the `StepInner` inputs: the record (`args[0]`), and the CScratch, state and def words the step reads.
    fn load_step_inner(&self, this: u32, rec: u32) -> StepInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let (weight, blend) = load_blend(mem, this);
        StepInput {
            rec_340: mem.f32(rec + 0x340),
            rec_348: mem.f32(rec + 0x348),
            rec_34c: mem.f32(rec + 0x34C),
            rec_358: mem.f32(rec + 0x358),
            rec_1c8: mem.f32(rec + 0x1C8),
            rec_1d8: mem.f32(rec + 0x1D8),
            rec_2f4: mem.f32(rec + 0x2F4),
            rec_36c: mem.f32(rec + 0x36C),
            rec_370: mem.f32(rec + 0x370),
            rec_38c: mem.f32(rec + 0x38C),
            factor_30c: mem.f32(this + 0x30C),
            this_340: mem.f32(this + BLEND_HI),
            this_368: mem.f32(this + SLIP_X),
            this_36c: mem.f32(this + SLIP_Y),
            radius: mem.f32(state + STATE_EFFECTIVE_RADIUS),
            def_3c: mem.f32(def + DEF_REFERENCE_FRICTION),
            weight,
            blend,
        }
    }

    /// Stores the `StepInner` results: `args[1]` (three floats), `args[2]` (zeroed, as `E09Vec4::Clear` does), and
    /// `args[3]` to `args[5]`. `ptrs` holds `args[1]` to `args[5]`.
    fn store_step_inner(&mut self, ptrs: [u32; 5], out: &StepOutput) {
        let mem = self;
        for (i, value) in out.vec3.iter().enumerate() {
            mem.set_f32(ptrs[0] + 4 * i as u32, *value);
        }
        Vec4::default().store(mem, ptrs[1]);
        mem.set_f32(ptrs[2], out.arg3);
        mem.set_f32(ptrs[3], out.arg4);
        mem.set_f32(ptrs[4], out.arg5);
    }

    /// Stores the `ComputeOutputs` results through the ten output pointers, stack words 7 to 16 (`ptrs[0]` is word 7).
    fn store_compute_outputs(&mut self, ptrs: [u32; 10], out: &ComputeOutputs) {
        let mem = self;
        mem.set_u32(ptrs[0], out.raw[0]);
        mem.set_u32(ptrs[1], out.raw[1]);
        mem.set_f32(ptrs[2], out.out9);
        mem.set_f32(ptrs[3], out.out10);
        mem.set_f32(ptrs[4], out.out11);
        mem.set_f32(ptrs[5], out.out12);
        mem.set_f32(ptrs[6], out.out13);
        mem.set_f32(ptrs[7], out.out14);
        mem.set_f32(ptrs[8], out.out15);
        mem.set_f32(ptrs[9], out.out16);
    }

    /// The game's debug flags for `Step`: the word at `g_1659b70 -> +0x24 -> +0x160e0` (`& 0x20`). When it is set, `Step`
    /// copies its `f` pair to `this+0x4c8`. The three words are read here, and the test maps them as node windows.
    fn load_step_flag(&self) -> bool {
        let mem = self;
        let physics = mem.u32(0x0165_9B70);
        let def = mem.u32(physics + 0x24);
        mem.u32(def + 0x160E0) & 0x20 != 0
    }

    /// Stores the output block at `out`.
    fn store_output_block(&mut self, out: u32, b: &OutputBlock) {
        let mem = self;
        b.v10.store(mem, out + 0x10);
        b.v20.store(mem, out + 0x20);
        b.v50.store(mem, out + 0x50);
        mem.set_f32(out + 0x80, b.f80);
        mem.set_f32(out + 0x84, b.f84);
        mem.set_f32(out + 0x88, b.f88);
        mem.set_u32(out + 0x90, b.u90);
        mem.set_u32(out + 0x94, b.u94);
        mem.set_f32(out + 0x98, b.f98);
        mem.set_u32(out + 0xAC, b.flag_ac);
    }

    /// Loads the sixteen stack words of `Step` (`a` to `p`).
    fn load_step(&self, w: &[u32]) -> (StepArgs, StepPtrs) {
        let mem = self;
        let args = StepArgs {
            spin: f32::from_bits(w[0]),
            dt: f32::from_bits(w[1]),
            c: f32::from_bits(w[2]),
            d: f32::from_bits(w[3]),
            material: mem.load_material(w[4]),
            pair: std::array::from_fn(|k| mem.u32(w[5] + 4 * k as u32)),
            pair_copy: mem.load_step_flag(),
            align_a: Vec4::load(mem, w[6]),
            dir: Vec4::load(mem, w[8]),
            out_a: Vec4::load(mem, w[9]),
            vec_k: Vec4::load(mem, w[10]),
            out_m: Vec4::load(mem, w[12]),
        };
        let ptrs = StepPtrs { out_l: w[11], out_m: w[12], out_n: w[13], out_o: w[14], out_p: w[15] };
        (args, ptrs)
    }

    /// Stores the outputs of `Step` (the `None` fields are not written).
    fn store_step(&mut self, p: &StepPtrs, out: &StepOut) {
        let mem = self;
        out.l.store(mem, p.out_l);
        out.m.store(mem, p.out_m);
        mem.set_f32(p.out_n, out.n);
        if let Some(o) = out.o {
            mem.set_f32(p.out_o, o);
        }
        if let Some(block) = &out.block {
            mem.store_output_block(p.out_p, block);
        }
    }

    /// Loads the inputs of `BlendTable` for the indices `i` and `j` and the fractions `a` and `b`: the def's bases (`+0x2c4`),
    /// slopes (`+0x2e0`) and wear factors (`+0x2f8`) at those indices, its `+0x2dc`, and the wear factor at the tyre index
    /// `m_state+0x134`. The indices are words, so the address is wrapping, as the asm's is.
    fn load_blend_table(&self, this: u32, i: u32, j: u32, a: f32, b: f32) -> BlendInput {
        let mem = self;
        let def = mem.u32(this + DEF);
        let state = mem.u32(this + STATE);
        let wear = mem.u32(this + 0x414);
        BlendInput {
            base_i: mem.f32(word_at(def + 0x2C4, i)),
            base_j: mem.f32(word_at(def + 0x2C4, j)),
            slope_i: mem.f32(word_at(def + 0x2E0, i)),
            slope_j: mem.f32(word_at(def + 0x2E0, j)),
            wear_i: mem.f32(word_at(def + 0x2F8, i)),
            wear_j: mem.f32(word_at(def + 0x2F8, j)),
            dff_2dc: mem.f32(def + 0x2DC),
            a,
            b,
            u: mem.f32(word_at(wear, mem.u32(state + 0x134))),
        }
    }

    /// Stores the two outputs of `BlendTable` through the caller's pointers `o1` and `o2`, in that order.
    fn store_blend(&mut self, o1: u32, o2: u32, out: BlendOut) {
        let mem = self;
        mem.set_f32(o1, out.o1);
        mem.set_f32(o2, out.o2);
    }

    /// Stores the `RelaxStep` results, then `RelaxCount`'s counter: `m_state+0x1c` goes up by one and its dirty flag
    /// (`+0x18`) is set.
    fn store_relax_count(&mut self, this: u32, out: &RelaxOutput) {
        let mem = self;
        mem.store_relax(this, out);
        let state = mem.u32(this + STATE);
        let count = mem.u32(state + 0x1C);
        ntweak::set_dirty(mem, state + 0x10);
        mem.set_u32(state + 0x1C, count.wrapping_add(1));
    }

    /// `TyreTail_Get10` (0x5326b0, thiscall): the word at `container+0x10`, the tyre tail of a wheel's container.
    fn load_tyre_tail10(&self, container: u32) -> u32 {
        let mem = self;
        mem.u32(container + 0x10)
    }

    /// `TyreTail_Get14` (0x5326c0, thiscall): the word at `container+0x14`.
    fn load_tyre_tail14(&self, container: u32) -> u32 {
        let mem = self;
        mem.u32(container + 0x14)
    }

    /// `NTweak::CBase::GetName` (0x52db10, thiscall): the name object at `this+4`.
    fn load_name_object(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + 4)
    }

    /// The tyre-noise signal, at `g_signalManager + 0x6268` (`CSignalManager::CMultiSignal1<CSigTyreNoise>`).
    fn noise_signal(&self) -> u32 {
        let mem = self;
        physics_core::layout::signal_manager(mem) + 0x6268
    }
}

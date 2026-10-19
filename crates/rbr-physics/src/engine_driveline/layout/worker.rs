//! The drive state, definition and worker object the wheel and gear workers read and write (`E39`).

use super::drive_objects::{DriveState, IntLane, Lane};
use super::step::{
    BLOCK_LATCH_BOUND, OBJ_TORQUE_GAIN, STATE_CAR, STATE_STEP_COUNT, THIS_ABS_DUMMY, THIS_RING_RATIO, THIS_SPEED_COPY,
    THIS_WHEEL_BIAS,
};
use crate::engine_driveline::store::EngineStore;
use crate::mem::Mem;

/// One axle record of the definition (`CDef`, 0x60 bytes each, four from `+0x0`): the direction word (`+0x18`), the two
/// block indices (`+0x28`, `+0x38`), the scale (`+0x48`) and the active index (`+0x68`). The names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Axle {
    pub direction: i32,
    pub block_pair: [u32; 2],
    pub scale: f32,
    pub active_index: u32,
}

/// One wheel record of the definition (0xf0 bytes each, four from `+0x148`): the weight (`+0x148`) and the pack kind
/// (`+0x158`, an index into the axles).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    pub weight: f32,
    pub pack: u32,
}

/// The gear band of one gear (`CDef`, 0x50 bytes each from `+0x688`): the ratio (`+0x694`), and the lower and upper
/// edges (`+0x6c4`, `+0x6d4`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GearBand {
    pub ratio: f32,
    pub lower_edge: f32,
    pub upper_edge: f32,
}

impl GearBand {
    /// The band of `gear` (any value, read at the band's offset as the original does).
    pub fn load<S: EngineStore>(mem: &S, def: u32, gear: u32) -> Self {
        mem.load_gear_band(def, gear)
    }
}

/// The definition's fields the group 6 workers read (`CDrive::CDef`, `this+4`). Only the names of the axle and wheel
/// records are ours; `torque_scale` is the axle 0 scale (`+0x48`) and is kept apart where the original reads the word
/// as a scalar.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorkerDef {
    /// The mode (`+0x640`, the tweak `DriveType`): `Step1E0` runs `M96C0` when it is 3, else the wheel worker.
    pub mode: u32,
    /// The pack term (`+0x684`; names ours), the base of the pack gain.
    pub pack_term: f32,
    pub axles: [Axle; 4],
    pub wheels: [Wheel; 4],
    /// Set when the CDef flags have `0x80000`: the gear guard runs.
    pub gear_guard: bool,
    /// The scalar at `+0x48` (the axle 0 scale).
    pub torque_scale: f32,
    /// `+0x664` and `+0x674`: the two terms of the clutch torque's total (`ClutchTorque`).
    pub slip_gain_a: f32,
    pub slip_gain_b: f32,
    /// `+0xa54`: the scale of the drive speed in `GearSpeedError`.
    pub ratio_gain: f32,
    /// `+0xa74` and `+0xa84`: the slope and the reach of the gear trim's limit.
    pub slope_limit: f32,
    pub reach_limit: f32,
    /// `+0x630`: the gain `M5970` scales the lane by (names ours).
    pub lane_scale_gain: f32,
}

impl WorkerDef {
    /// The definition at `def`.
    pub fn load<S: EngineStore>(mem: &S, def: u32) -> Self {
        mem.load_worker_def(def)
    }
}

/// One wheel term of the state (`0xc + 0x14 i`, `i < 4`): the value (`+0`), the term (`+4`), the sign (`+8`), the scale
/// (`+0xc`) and the speed (`+0x10`). Term 3's words are the first words of block 0's slip lane (`+0x48` to `+0x58`):
/// the workers read its value and term only, and `WorkerState::store` writes terms 0 to 2 only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelTerm {
    pub value: f32,
    pub term: f32,
    pub sign: i32,
    pub scale: f32,
    pub speed: f32,
}

impl WheelTerm {
    pub(super) fn load(mem: &Mem, at: u32) -> Self {
        WheelTerm {
            value: mem.f32(at),
            term: mem.f32(at + 4),
            sign: mem.u32(at + 8) as i32,
            scale: mem.f32(at + 0xC),
            speed: mem.f32(at + 0x10),
        }
    }

    pub(super) fn store(&self, mem: &mut Mem, at: u32) {
        mem.set_f32(at, self.value);
        mem.set_f32(at + 4, self.term);
        mem.set_u32(at + 8, self.sign as u32);
        mem.set_f32(at + 0xC, self.scale);
        mem.set_f32(at + 0x10, self.speed);
    }
}

/// One wheel block of the state (`200 i`, `i < 4`). `force` (`+0x88`) and `gate` (`+0x98`) are the lanes the wheels
/// work on; `slip` (`+0x48`) and the two `prev_` lanes hold the previous values. The `_aux` words are copied with their
/// lanes but are not read by the ported code. Block 0's `slip` shares its words with term 3 (see `WheelTerm`). The
/// names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelBlock {
    pub slip: Lane,
    pub prev_force: Lane,
    pub prev_force_aux: u32,
    pub prev_gate: IntLane,
    pub prev_gate_aux: u32,
    /// The force of the wheel (`+0x88`).
    pub force: Lane,
    pub force_aux: u32,
    /// The latch of the wheel (`+0x98`): 1 or -1 when latched, 0 otherwise.
    pub gate: IntLane,
    pub gate_aux: u32,
    /// The free wheel's inertia (`+0x84`; names ours): `E39_Step3870` steps `prev_force` by the torque over it.
    pub inertia: f32,
    /// The clutch load (`+0xb4`), read by `ClutchValues`.
    pub clutch_load: f32,
    /// The latch bound (`+0xf8`), read by `WheelSlipValue` and `WheelLatch`.
    pub latch_bound: f32,
}

impl WheelBlock {
    pub(super) fn load(mem: &Mem, at: u32) -> Self {
        WheelBlock {
            slip: Lane::load(mem, at + 0x48),
            prev_force: Lane::load(mem, at + 0x58),
            prev_force_aux: mem.u32(at + 0x5C),
            prev_gate: IntLane::load(mem, at + 0x68),
            prev_gate_aux: mem.u32(at + 0x6C),
            force: Lane::load(mem, at + 0x88),
            force_aux: mem.u32(at + 0x8C),
            gate: IntLane::load(mem, at + 0x98),
            gate_aux: mem.u32(at + 0x9C),
            inertia: mem.f32(at + 0x84),
            clutch_load: mem.f32(at + 0xB4),
            latch_bound: mem.f32(at + BLOCK_LATCH_BOUND),
        }
    }

    /// Writes the block's words except `slip`, which the state writes when it changed (see `WorkerState::store`).
    pub(super) fn store(&self, mem: &mut Mem, at: u32) {
        self.prev_force.store(mem, at + 0x58);
        mem.set_u32(at + 0x5C, self.prev_force_aux);
        self.prev_gate.store(mem, at + 0x68);
        mem.set_u32(at + 0x6C, self.prev_gate_aux);
        self.force.store(mem, at + 0x88);
        mem.set_u32(at + 0x8C, self.force_aux);
        self.gate.store(mem, at + 0x98);
        mem.set_u32(at + 0x9C, self.gate_aux);
        mem.set_f32(at + 0x84, self.inertia);
        mem.set_f32(at + 0xB4, self.clutch_load);
        mem.set_f32(at + BLOCK_LATCH_BOUND, self.latch_bound);
    }
}

/// The drive state the wheel workers read and write (`CDrive::CState`, `this+8`). `drive` is the shared part (the gear
/// and the engine speed). The `Vec` of values stays a fixed array: the state's offsets below cover the whole range the
/// workers touch. Names are ours where the meaning is not known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkerState {
    pub drive: DriveState,
    /// The sign word of the clutch torque (`+0x368`).
    pub sign_word: i32,
    /// `+0x37c`, the drive gain of `WheelLatch` and `WheelSlipValue`.
    pub drive_gain: f32,
    /// `+0x380`, the drive gain negated: `M96C0` sets it, its pair leaves read it (names ours).
    pub drive_gain_neg: f32,
    /// `+0x460`, the pack gain of `ClutchValues` and `WheelLatch`.
    pub pack_gain: f32,
    /// The clutch matrix (`+0x470`, 16 floats, row-major). Its input to `E39_Step4990`.
    pub clutch_matrix: [f32; 16],
    /// The output of `E39_Step4990` (`+0x4b0`).
    pub step_matrix: [f32; 16],
    /// The vector of `E39_MatVec` (`+0x530`), one word per wheel from `WheelSlipValue`.
    pub step_vec: [f32; 4],
    /// The active flags, one per axle index (`+0x4f0`).
    pub lane_active: [u32; 4],
    /// The pair worker's 2x2 clutch matrix (`+0x500`, row-major, a row per wheel parity) and its inverse (`+0x510`),
    /// both set by `M7AA0` (names ours).
    pub pair_matrix: [f32; 4],
    pub pair_inverse: [f32; 4],
    /// The pair worker's active flags (`+0x520`, `+0x524`) and the two words `M7AA0` clears (`+0x528`, `+0x52c`).
    pub pair_active: [u32; 2],
    pub pair_words: [u32; 2],
    /// The gear history (`+0x544`, `+0x548`), and the word at `+0x568` that the shift copies.
    pub shift_from: [u32; 2],
    pub commanded_aux: u32,
    /// The gear-change flag (`+0x550`, the value at `+0x55c`).
    pub gear_flag: IntLane,
    /// The trim (`+0x560`) and the accumulator (`+0x574`), both f32.
    pub trim: f32,
    pub trim_accum: f32,
    /// The trigger (`+0x578`), set by each shift and gear change.
    pub trigger: u32,
    /// The random state of the gear logic (`+0x57c`).
    pub rng: u32,
    /// The car the gear change is for (`+0x580`).
    pub car: u32,
    /// The step counter (`+0x584`, the value at `+0x590`; names ours): `Step1E0` adds one each step.
    pub step_count: Lane,
    /// The reset counter (`+0x594`, the value at `+0x5a0`).
    pub reset_count: Lane,
    /// The count of the latches (`+0x5a4`).
    pub latch_count: u32,
    /// The count of the clutch passes (`+0x5a8`).
    pub count_a: u32,
    /// The count of the wheel worker's repeats (`+0x5ac`).
    pub repeat_count: u32,
    /// The count of the dirty bumps (`+0x5b0`).
    pub dirty_count: u32,
    /// The step count (`+0x5b4`; names ours): `Step1E0` adds one when the pending word is set.
    pub step_iter: u32,
    /// The branch word of the flags (`+0x5b8`).
    pub branch: u32,
    /// The pending word (`+0x5bc`), set by a latch and cleared by the reset.
    pub pending: u32,
    pub terms: [WheelTerm; 4],
    pub blocks: [WheelBlock; 4],
}

impl WorkerState {
    pub(super) fn load(mem: &Mem, at: u32) -> Self {
        let f = |off: u32| mem.f32(at + off);
        let u = |off: u32| mem.u32(at + off);
        WorkerState {
            drive: DriveState::load(mem, at),
            sign_word: u(0x368) as i32,
            drive_gain: f(0x37C),
            drive_gain_neg: f(0x380),
            pack_gain: f(0x460),
            clutch_matrix: mem.f32s(at + 0x470),
            step_matrix: mem.f32s(at + 0x4B0),
            step_vec: std::array::from_fn(|k| f(0x530 + 4 * k as u32)),
            lane_active: std::array::from_fn(|k| u(0x4F0 + 4 * k as u32)),
            pair_matrix: mem.f32s(at + 0x500),
            pair_inverse: mem.f32s(at + 0x510),
            pair_active: [u(0x520), u(0x524)],
            pair_words: [u(0x528), u(0x52C)],
            shift_from: [u(0x544), u(0x548)],
            commanded_aux: u(0x568),
            gear_flag: IntLane::load(mem, at + 0x550),
            trim: f(0x560),
            trim_accum: f(0x574),
            trigger: u(0x578),
            rng: u(0x57C),
            car: u(STATE_CAR),
            step_count: Lane::load(mem, at + STATE_STEP_COUNT),
            reset_count: Lane::load(mem, at + 0x594),
            latch_count: u(0x5A4),
            count_a: u(0x5A8),
            repeat_count: u(0x5AC),
            dirty_count: u(0x5B0),
            step_iter: u(0x5B4),
            branch: u(0x5B8),
            pending: u(0x5BC),
            terms: std::array::from_fn(|k| WheelTerm::load(mem, at + 0xC + 0x14 * k as u32)),
            blocks: std::array::from_fn(|k| WheelBlock::load(mem, at + 200 * k as u32)),
        }
    }

    /// Writes the state back. `before` is the state as loaded (the memory still holds it). Term 3's sign and scale are
    /// block 0's slip lane (`+0x50`, `+0x54`), so each of the two is written only when the port changed it, and the
    /// slip lane likewise: a stale copy of the other must not overwrite the change.
    pub(super) fn store(&self, mem: &mut Mem, at: u32, before: &WorkerState) {
        self.drive.store(mem, at);
        mem.set_u32(at + 0x368, self.sign_word as u32);
        mem.set_f32(at + 0x37C, self.drive_gain);
        mem.set_f32(at + 0x380, self.drive_gain_neg);
        mem.set_f32(at + 0x460, self.pack_gain);
        for (k, v) in self.clutch_matrix.iter().enumerate() {
            mem.set_f32(at + 0x470 + 4 * k as u32, *v);
        }
        for (k, v) in self.step_matrix.iter().enumerate() {
            mem.set_f32(at + 0x4B0 + 4 * k as u32, *v);
        }
        for (k, v) in self.step_vec.iter().enumerate() {
            mem.set_f32(at + 0x530 + 4 * k as u32, *v);
        }
        for (k, v) in self.lane_active.iter().enumerate() {
            mem.set_u32(at + 0x4F0 + 4 * k as u32, *v);
        }
        for (k, v) in self.pair_matrix.iter().enumerate() {
            mem.set_f32(at + 0x500 + 4 * k as u32, *v);
        }
        for (k, v) in self.pair_inverse.iter().enumerate() {
            mem.set_f32(at + 0x510 + 4 * k as u32, *v);
        }
        for (k, v) in self.pair_active.iter().enumerate() {
            mem.set_u32(at + 0x520 + 4 * k as u32, *v);
        }
        for (k, v) in self.pair_words.iter().enumerate() {
            mem.set_u32(at + 0x528 + 4 * k as u32, *v);
        }
        mem.set_u32(at + 0x544, self.shift_from[0]);
        mem.set_u32(at + 0x548, self.shift_from[1]);
        mem.set_u32(at + 0x568, self.commanded_aux);
        self.gear_flag.store(mem, at + 0x550);
        mem.set_f32(at + 0x560, self.trim);
        mem.set_f32(at + 0x574, self.trim_accum);
        mem.set_u32(at + 0x578, self.trigger);
        mem.set_u32(at + 0x57C, self.rng);
        mem.set_u32(at + STATE_CAR, self.car);
        self.step_count.store(mem, at + STATE_STEP_COUNT);
        self.reset_count.store(mem, at + 0x594);
        mem.set_u32(at + 0x5A4, self.latch_count);
        mem.set_u32(at + 0x5A8, self.count_a);
        mem.set_u32(at + 0x5AC, self.repeat_count);
        mem.set_u32(at + 0x5B0, self.dirty_count);
        mem.set_u32(at + 0x5B4, self.step_iter);
        mem.set_u32(at + 0x5B8, self.branch);
        mem.set_u32(at + 0x5BC, self.pending);
        for (k, term) in self.terms.iter().enumerate().take(3) {
            term.store(mem, at + 0xC + 0x14 * k as u32);
        }
        let shared = self.terms[3];
        let old = before.terms[3];
        let t3 = at + 0xC + 0x14 * 3;
        mem.set_f32(t3, shared.value);
        mem.set_f32(t3 + 4, shared.term);
        mem.set_f32(t3 + 0x10, shared.speed);
        if shared.sign != old.sign {
            mem.set_u32(t3 + 8, shared.sign as u32);
        }
        if shared.scale.to_bits() != old.scale.to_bits() {
            mem.set_f32(t3 + 0xC, shared.scale);
        }
        for (k, block) in self.blocks.iter().enumerate() {
            let base = at + 200 * k as u32;
            block.store(mem, base);
            if block.slip != before.blocks[k].slip {
                block.slip.store(mem, base + 0x48);
            }
        }
    }
}

impl WorkerState {
    /// Sets block `i`'s slip. Block 0's slip is the words term 3 reads as its sign and scale (see `WheelTerm`), so a
    /// write to block 0 is also term 3's: its sign takes the slip's dirty flag, its scale the value.
    pub fn set_slip(&mut self, i: usize, value: f32) {
        self.blocks[i].slip.set(value);
        if i == 0 {
            self.terms[3].sign = self.blocks[0].slip.dirty as i32;
            self.terms[3].scale = value;
        }
    }
}

/// The wheel worker's words the original's callees read and write (`E39_Step4990`, `E39_MatVec`): the clutch matrix,
/// the step matrix and the step vector (state offsets), and the MatVec output (`this` offset). The host calls use them.
pub const CLUTCH_MATRIX_AT: u32 = 0x470;
pub const STEP_MATRIX_AT: u32 = 0x4B0;
pub const STEP_VEC_AT: u32 = 0x530;
pub const LOC_AT: u32 = 0xA0;

/// The CDrive's own words the workers use (`this`): the `loc0` lane (`+0x84`), the MatVec output `loc` (`+0xa0`), the
/// pack outputs (`+0xc0`), the wheel
/// offsets (`+0xf4`, 0x10 apart), the shift's speed and trim words (`+0x1b4`, `+0x1b8`) and the gear level (`+0x1c4`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorkerThis {
    pub loc0: Lane,
    /// The output of `E39_MatVec` (`+0xa0`, the original's `loc20`).
    pub loc: [f32; 4],
    /// The lane at `+0x40` (`M5970` sets its scale).
    pub lane_scale: Lane,
    /// The lane at `+0x74` (`M5970` sets the scale times the definition's gain).
    pub lane_scaled: Lane,
    /// The gate word (`+0x50`): `M5970` copies it, `M6C90` reads it.
    pub lane_gate: u32,
    /// `+0x2c`, the gate level `M6C90` compares with 0.5 (names ours).
    pub gate_level: f32,
    /// `+0x1a0`, the scaled speed term `M6C90` returns (names ours).
    pub scaled_term: f32,
    /// `+0x1c8`, the speed factor `M6C90` writes (names ours).
    pub speed_factor: f32,
    pub pack_out: [f32; 4],
    /// The pair worker's two pack outputs (`+0xd0`), its solved pair (`+0xd8`) and its slip values (`+0xe0`), one per
    /// wheel parity (names ours).
    pub pair_pack: [f32; 2],
    pub pair_loc: [f32; 2],
    pub pair_slip: [f32; 2],
    pub wheel_offset: [f32; 4],
    pub shift_speed: f32,
    pub shift_trim: u32,
    pub gear_level: f32,
    /// The ring's mean, the result of `E39_RatioCA0` (`+0x19c`; names ours).
    pub ring_ratio: f32,
    /// The per-wheel bias added to each curve term (`+0x134`, every `0x10` bytes; names ours).
    pub wheel_bias: [f32; 4],
    /// The speed copy: the engine speed or the drive speed key, as bits (`+0x1a4`; names ours).
    pub speed_copy: u32,
    /// The magnitude of the clutch torque (`+0x1bc`; names ours).
    pub abs_dummy: f32,
}

impl WorkerThis {
    pub(super) fn load(mem: &Mem, this: u32) -> Self {
        WorkerThis {
            loc0: Lane::load(mem, this + 0x84),
            loc: mem.f32s(this + 0xA0),
            lane_scale: Lane::load(mem, this + 0x40),
            lane_scaled: Lane::load(mem, this + 0x74),
            lane_gate: mem.u32(this + 0x50),
            gate_level: mem.f32(this + 0x2C),
            scaled_term: mem.f32(this + 0x1A0),
            speed_factor: mem.f32(this + 0x1C8),
            pack_out: mem.f32s(this + 0xC0),
            pair_pack: mem.f32s(this + 0xD0),
            pair_loc: mem.f32s(this + 0xD8),
            pair_slip: mem.f32s(this + 0xE0),
            wheel_offset: std::array::from_fn(|k| mem.f32(this + 0xF4 + 0x10 * k as u32)),
            shift_speed: mem.f32(this + 0x1B4),
            shift_trim: mem.u32(this + 0x1B8),
            gear_level: mem.f32(this + 0x1C4),
            ring_ratio: mem.f32(this + THIS_RING_RATIO),
            wheel_bias: std::array::from_fn(|k| mem.f32(this + THIS_WHEEL_BIAS + 0x10 * k as u32)),
            speed_copy: mem.u32(this + THIS_SPEED_COPY),
            abs_dummy: mem.f32(this + THIS_ABS_DUMMY),
        }
    }

    pub(super) fn store(&self, mem: &mut Mem, this: u32) {
        self.loc0.store(mem, this + 0x84);
        for (k, v) in self.loc.iter().enumerate() {
            mem.set_f32(this + 0xA0 + 4 * k as u32, *v);
        }
        self.lane_scale.store(mem, this + 0x40);
        self.lane_scaled.store(mem, this + 0x74);
        mem.set_u32(this + 0x50, self.lane_gate);
        mem.set_f32(this + 0x2C, self.gate_level);
        mem.set_f32(this + 0x1A0, self.scaled_term);
        mem.set_f32(this + 0x1C8, self.speed_factor);
        for (k, v) in self.pack_out.iter().enumerate() {
            mem.set_f32(this + 0xC0 + 4 * k as u32, *v);
        }
        for (k, v) in self.pair_pack.iter().chain(&self.pair_loc).chain(&self.pair_slip).enumerate() {
            mem.set_f32(this + 0xD0 + 4 * k as u32, *v);
        }
        for (k, v) in self.wheel_offset.iter().enumerate() {
            mem.set_f32(this + 0xF4 + 0x10 * k as u32, *v);
        }
        mem.set_f32(this + 0x1B4, self.shift_speed);
        mem.set_u32(this + 0x1B8, self.shift_trim);
        mem.set_f32(this + 0x1C4, self.gear_level);
        mem.set_f32(this + THIS_RING_RATIO, self.ring_ratio);
        for (k, v) in self.wheel_bias.iter().enumerate() {
            mem.set_f32(this + THIS_WHEEL_BIAS + 0x10 * k as u32, *v);
        }
        mem.set_u32(this + THIS_SPEED_COPY, self.speed_copy);
        mem.set_f32(this + THIS_ABS_DUMMY, self.abs_dummy);
    }
}

/// The drive's worker object (`this+0x170`): the enable word per wheel (`+0x18`, 0xc apart), the rate (`+0x74`), the
/// enable (`+0x78`), the scale (`+0x88`) and the gear's scale (`+0x48`, four bytes per gear). The names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorkerObj {
    /// The gate trip (`+0x70`; names ours): `Step1E0` clears it when the trip is taken.
    pub gate_trip: u32,
    /// The torque gain (`+0x84`; names ours): `Step1E0` scales the clutch torque by it.
    pub torque_gain: f32,
    pub wheel_enabled: [u32; 4],
    pub rate: f32,
    pub enable: u32,
    /// `+0x7c`, the ready word `M6C90` needs nonzero (names ours).
    pub ready: u32,
    /// `+0x8c`, the speed term `M6C90` updates (names ours).
    pub speed_term: f32,
    pub scale: f32,
}

impl WorkerObj {
    pub(super) fn load(mem: &Mem, obj: u32) -> Self {
        WorkerObj {
            wheel_enabled: std::array::from_fn(|k| mem.u32(obj + 0x18 + 0xC * k as u32)),
            rate: mem.f32(obj + 0x74),
            enable: mem.u32(obj + 0x78),
            ready: mem.u32(obj + 0x7C),
            speed_term: mem.f32(obj + 0x8C),
            scale: mem.f32(obj + 0x88),
            gate_trip: mem.u32(obj + 0x70),
            torque_gain: mem.f32(obj + OBJ_TORQUE_GAIN),
        }
    }

    pub(super) fn store(&self, mem: &mut Mem, obj: u32) {
        for (k, v) in self.wheel_enabled.iter().enumerate() {
            mem.set_u32(obj + 0x18 + 0xC * k as u32, *v);
        }
        mem.set_f32(obj + 0x74, self.rate);
        mem.set_u32(obj + 0x78, self.enable);
        mem.set_u32(obj + 0x7C, self.ready);
        mem.set_f32(obj + 0x8C, self.speed_term);
        mem.set_f32(obj + 0x88, self.scale);
        mem.set_u32(obj + 0x70, self.gate_trip);
        mem.set_f32(obj + OBJ_TORQUE_GAIN, self.torque_gain);
    }
}

/// The objects one drive worker call reads and writes: the definition (`this+4`), the state (`this+8`), the drive's own
/// words (`this`) and the worker object (`this+0x170`). The gear-indexed words are in `GearWork`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DriveWorker {
    pub def: WorkerDef,
    pub state: WorkerState,
    pub this: WorkerThis,
    pub obj: WorkerObj,
}

impl DriveWorker {
    pub fn load<S: EngineStore>(mem: &S, this: u32) -> Self {
        mem.load_drive_worker(this)
    }

    pub fn store<S: EngineStore>(&self, mem: &mut S, this: u32) {
        mem.store_drive_worker(this, self)
    }
}

/// The words the gear workers read at the state's gear: the gear's band (`CDef`, `+0x688`), the gear's scale (the object's
/// `+0x48` table) and the gear's time step (`this+0x174`). Only the gear workers load them, since the gear is an index
/// the other workers never read at. `store` writes the scale and the time step last: the scale can share a word with
/// the object's rate (at gear 11), and the rate is written first.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GearWork {
    pub gear: u32,
    pub band: GearBand,
    pub scale: f32,
    pub dt: f32,
}

impl GearWork {
    pub fn load<S: EngineStore>(mem: &S, this: u32) -> Self {
        mem.load_gear_work(this)
    }

    /// Writes the gear's scale and time step (the state's gear is the one `load` read).
    pub fn store<S: EngineStore>(&self, mem: &mut S, this: u32) {
        mem.store_gear_work(this, self)
    }
}

/// The ring of `E39_RatioCA0` (0x503ca0): the count (`+0`, at most 8), the index of the last value written (`+4`) and the
/// eight values (`+8`). `Step1E0` keeps it at the state's `+0x388`. Names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RatioRing {
    pub count: u32,
    pub index: u32,
    pub values: [f32; 8],
}

impl RatioRing {
    /// The ring at `at`.
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_ratio_ring(at)
    }

    pub fn store<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_ratio_ring(at, self)
    }
}

/// The lane scale `M5970` takes (`x`, a struct on the stack): the scale (`+8`) and the gate word (`+0x1c`). Names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaneScale {
    pub scale: f32,
    pub gate: u32,
}

impl LaneScale {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_lane_scale(at)
    }
}

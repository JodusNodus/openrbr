//! The Mem store of the engine and the drive (`super::super::store::EngineStore`): the bodies are the layout's reads
//! and writes, moved from the records' `load`/`store` and the readers next to them.

use super::curve::{
    CurveInput, CurveRow, CurveState, CurveTables, HookBlock, CURVE_TABLE_A_AT, CURVE_TABLE_B_AT, CURVE_TABLE_C_AT,
};
use super::drive_objects::{
    Drive, DriveDef, DriveObjects, DriveState, IntLane, Lane, DRIVE_DEF_GEAR_COUNT, DRIVE_DEF_MAX_FOOTBRAKE_PRESSURE,
    DRIVE_DEF_PTR, DRIVE_STATE_COMMANDED_GEAR_LANE, DRIVE_STATE_ENGINE_SPEED, DRIVE_STATE_GEAR, DRIVE_STATE_PTR,
};
use super::engine_objects::{
    EngineDef, EngineObjects, EngineOptions, EngineState, EngineThis, LoadDef, ENGINE_DEF_ABS_SCALE, ENGINE_DEF_PTR,
    ENGINE_OPTIONS_PTR, ENGINE_PEDAL, ENGINE_STATE_ABS_VALUE, ENGINE_STATE_ABS_VALUE_DIRTY,
    ENGINE_STATE_COMPLIANCE_DIRTY, ENGINE_STATE_COMPLIANCE_INVERSE, ENGINE_STATE_CRANK_ANGLE, ENGINE_STATE_CRANK_COUNT,
    ENGINE_STATE_CRANK_COUNT_PREV, ENGINE_STATE_ENGINE_SPEED, ENGINE_STATE_PTR, ENGINE_STATE_SCALED_VALUE,
    ENGINE_STATE_SCALED_VALUE_DIRTY, ENGINE_STATE_SMOOTHED, ENGINE_STATE_TORQUE_ACCUM,
};
use super::step::{DEF_MODE, DEF_RATIO_GAIN};
use super::tables::{BlendKnot, CurveKnot, UnitSplit};
use super::worker::{
    Axle, DriveWorker, GearBand, GearWork, LaneScale, RatioRing, Wheel, WorkerDef, WorkerObj, WorkerState, WorkerThis,
};
use crate::engine_driveline::drive_line;
use crate::engine_driveline::drive_line::Entry;
use crate::engine_driveline::sim::{BlendRec, Bracket};
use crate::engine_driveline::store::EngineStore;
use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::physics_core::layout::cdef_flags;
use crate::spring_damper::{BlendTerms, Relaxation};

impl EngineStore for Mem {
    fn load_drive(&self, at: u32) -> Drive {
        let mem = self;
        Drive {
            throttle_in: Lane::load(mem, at + 0x20),
            brake_in: Lane::load(mem, at + 0x30),
            brake_torque: Lane::load(mem, at + 0x64),
            clutch_slip: Lane::load(mem, at + 0x54),
        }
    }

    fn store_drive(&mut self, at: u32, value: &Drive) {
        let mem = self;
        value.throttle_in.store(mem, at + 0x20);
        value.brake_in.store(mem, at + 0x30);
        value.brake_torque.store(mem, at + 0x64);
        value.clutch_slip.store(mem, at + 0x54);
    }

    fn load_drive_def(&self, at: u32) -> DriveDef {
        let mem = self;
        DriveDef {
            max_footbrake_pressure: mem.f32(at + DRIVE_DEF_MAX_FOOTBRAKE_PRESSURE),
            gear_count: mem.u32(at + DRIVE_DEF_GEAR_COUNT),
        }
    }

    fn load_drive_state(&self, at: u32) -> DriveState {
        let mem = self;
        DriveState {
            drive_speed_key: mem.f32(at + 0x384),
            engine_speed: mem.f32(at + DRIVE_STATE_ENGINE_SPEED),
            gear: mem.u32(at + DRIVE_STATE_GEAR) as i32,
            commanded_gear: IntLane::load(mem, at + DRIVE_STATE_COMMANDED_GEAR_LANE),
        }
    }

    fn store_drive_state(&mut self, at: u32, value: &DriveState) {
        let mem = self;
        mem.set_f32(at + 0x384, value.drive_speed_key);
        mem.set_f32(at + DRIVE_STATE_ENGINE_SPEED, value.engine_speed);
        mem.set_u32(at + DRIVE_STATE_GEAR, value.gear as u32);
        value.commanded_gear.store(mem, at + DRIVE_STATE_COMMANDED_GEAR_LANE);
    }

    fn store_commanded_gear(&mut self, at: u32, value: &DriveState) {
        let mem = self;
        value.commanded_gear.store(mem, at + DRIVE_STATE_COMMANDED_GEAR_LANE);
    }

    fn drive_objects(&self, drive: u32) -> DriveObjects {
        let mem = self;
        DriveObjects { def: mem.u32(drive + DRIVE_DEF_PTR), state: mem.u32(drive + DRIVE_STATE_PTR) }
    }

    /// The row at `at` (see [`curve_row_at`]).
    fn load_curve_row(&self, at: u32) -> CurveRow {
        let mem = self;
        CurveRow {
            breaks: std::array::from_fn(|k| mem.u32(at + 0xC + 0x10 * k as u32)),
            values: std::array::from_fn(|k| mem.f32(at + 0x6C + 0x10 * k as u32)),
            gain: mem.f32(at + 0x4C),
            rate: mem.f32(at + 0x5C),
            limit: mem.u32(at + 0xAC),
        }
    }

    fn load_curve_tables(&self, def: u32) -> CurveTables {
        let mem = self;
        CurveTables {
            a: mem.f32s(def + CURVE_TABLE_A_AT),
            b: mem.f32s(def + CURVE_TABLE_B_AT),
            c: mem.f32s(def + CURVE_TABLE_C_AT),
        }
    }

    fn load_curve_state(&self, at: u32) -> CurveState {
        let mem = self;
        CurveState {
            pos: IntLane::load(mem, at + 0x70),
            slope: mem.f32(at + 0x1C),
            term_gain: mem.f32(at + 0xAC),
            lane: IntLane::load(mem, at + 0x80),
            counter: IntLane::load(mem, at + 0xB8),
        }
    }

    fn store_curve_state(&mut self, at: u32, value: &CurveState) {
        let mem = self;
        value.pos.store(mem, at + 0x70);
        value.lane.store(mem, at + 0x80);
        value.counter.store(mem, at + 0xB8);
    }

    fn load_curve_input(&self, at: u32) -> CurveInput {
        let mem = self;
        CurveInput { w60: mem.f32(at + 0x60), w70: mem.f32(at + 0x70) }
    }

    fn load_hook_block(&self, at: u32) -> HookBlock {
        let mem = self;
        HookBlock {
            index: mem.u32(at + 0xDC),
            gain: mem.f32(at + 0x6C),
            e_scale: mem.f32(at + 0xEC),
            dt_scale: mem.f32(at + 0xCC),
            p_scale: mem.f32(at + 0x9C),
            x1_scale: mem.f32(at + 0xBC),
            w_scale: mem.f32(at + 0x8C),
            den_a: mem.f32(at + 0x7C),
            den_b: mem.f32(at + 0x3C),
        }
    }

    fn engine_objects(&self, engine: u32) -> EngineObjects {
        let mem = self;
        EngineObjects {
            def: mem.u32(engine + ENGINE_DEF_PTR),
            state: mem.u32(engine + ENGINE_STATE_PTR),
            options: mem.u32(engine + ENGINE_OPTIONS_PTR),
        }
    }

    fn load_load_def(&self, at: u32) -> LoadDef {
        let mem = self;
        let f = |off: u32| mem.f32(at + off);
        LoadDef {
            load_den: [f(0x138), f(0x178), f(0x1b8), f(0x1f8)],
            load_span: f(0x284c),
            threshold_slope: f(0x285c),
            threshold: f(0x286c),
        }
    }

    fn load_engine_def(&self, at: u32) -> EngineDef {
        let mem = self;
        let f = |off: u32| mem.f32(at + off);
        EngineDef {
            abs_scale: f(ENGINE_DEF_ABS_SCALE),
            stage_low: [f(0x118), f(0x158), f(0x198), f(0x1d8)],
            stage_high: [f(0x128), f(0x168), f(0x1a8), f(0x1e8)],
            extra_stage_low: f(0xd8),
            extra_stage_high: f(0xe8),
            enable: mem.u32(at + 0x26f8),
            torque_a: [f(0x2214), f(0x2218), f(0x221c)],
            torque_a_scale: f(0x21e0),
            torque_b: [f(0x21fc), f(0x2200), f(0x2204)],
            torque_b_scale: f(0x21dc),
            torque_c: [f(0x222c), f(0x2230), f(0x2234)],
            torque_c_scale: f(0x21e4),
        }
    }

    fn load_engine_this(&self, engine: u32) -> EngineThis {
        let mem = self;
        EngineThis { pedal: Lane::load(mem, engine + ENGINE_PEDAL), misfire_level: mem.f32(engine + 0x194) }
    }

    fn store_engine_this(&mut self, engine: u32, value: &EngineThis) {
        let mem = self;
        value.pedal.store(mem, engine + ENGINE_PEDAL);
        mem.set_f32(engine + 0x194, value.misfire_level);
    }

    fn load_engine_state(&self, at: u32) -> EngineState {
        let mem = self;
        let f = |off: u32| mem.f32(at + off);
        EngineState {
            crank_angle: f(ENGINE_STATE_CRANK_ANGLE),
            crank_count: mem.u32(at + ENGINE_STATE_CRANK_COUNT),
            crank_count_prev: mem.u32(at + ENGINE_STATE_CRANK_COUNT_PREV),
            misfire_rate: f(0x10),
            misfire_time: f(0x14),
            misfire_count: f(0x18),
            misfire_out: f(0x3c),
            stage: [f(0xb8), f(0xf0), f(0x128), f(0x160)],
            extra_stage: f(0x80),
            compliance_dirty: mem.u32(at + ENGINE_STATE_COMPLIANCE_DIRTY),
            compliance_inverse: f(ENGINE_STATE_COMPLIANCE_INVERSE),
            speed: f(ENGINE_STATE_ENGINE_SPEED),
            abs_value_dirty: mem.u32(at + ENGINE_STATE_ABS_VALUE_DIRTY),
            abs_value: f(ENGINE_STATE_ABS_VALUE),
            scaled_value_dirty: mem.u32(at + ENGINE_STATE_SCALED_VALUE_DIRTY),
            scaled_value: f(ENGINE_STATE_SCALED_VALUE),
            signed_rate: f(0x184),
            load_bc: f(0xbc),
            load_f4: f(0xf4),
            load_12c: f(0x12c),
            load_164: f(0x164),
            threshold_value: f(0x23c),
            torque_accum: f(ENGINE_STATE_TORQUE_ACCUM),
            torque: f(0x270),
            slot_a: std::array::from_fn(|i| f(0x94 + 0x38 * i as u32)),
            slot_b: std::array::from_fn(|i| f(0xac + 0x38 * i as u32)),
            smoothed: Lane::load(mem, at + ENGINE_STATE_SMOOTHED),
            solved: Lane::load(mem, at + 0x1f4),
        }
    }

    /// Writes every field back (the ones a kernel did not change keep their bytes).
    fn store_engine_state(&mut self, at: u32, value: &EngineState) {
        let mem = self;
        mem.set_f32(at + ENGINE_STATE_CRANK_ANGLE, value.crank_angle);
        mem.set_u32(at + ENGINE_STATE_CRANK_COUNT, value.crank_count);
        mem.set_u32(at + ENGINE_STATE_CRANK_COUNT_PREV, value.crank_count_prev);
        mem.set_f32(at + 0x10, value.misfire_rate);
        mem.set_f32(at + 0x14, value.misfire_time);
        mem.set_f32(at + 0x18, value.misfire_count);
        mem.set_f32(at + 0x3c, value.misfire_out);
        for (off, v) in [0xb8, 0xf0, 0x128, 0x160].into_iter().zip(value.stage) {
            mem.set_f32(at + off, v);
        }
        mem.set_f32(at + 0x80, value.extra_stage);
        mem.set_u32(at + ENGINE_STATE_COMPLIANCE_DIRTY, value.compliance_dirty);
        mem.set_f32(at + ENGINE_STATE_COMPLIANCE_INVERSE, value.compliance_inverse);
        mem.set_f32(at + ENGINE_STATE_ENGINE_SPEED, value.speed);
        mem.set_u32(at + ENGINE_STATE_ABS_VALUE_DIRTY, value.abs_value_dirty);
        mem.set_f32(at + ENGINE_STATE_ABS_VALUE, value.abs_value);
        mem.set_u32(at + ENGINE_STATE_SCALED_VALUE_DIRTY, value.scaled_value_dirty);
        mem.set_f32(at + ENGINE_STATE_SCALED_VALUE, value.scaled_value);
        mem.set_f32(at + 0x184, value.signed_rate);
        mem.set_f32(at + 0xbc, value.load_bc);
        mem.set_f32(at + 0xf4, value.load_f4);
        mem.set_f32(at + 0x12c, value.load_12c);
        mem.set_f32(at + 0x164, value.load_164);
        mem.set_f32(at + 0x23c, value.threshold_value);
        mem.set_f32(at + ENGINE_STATE_TORQUE_ACCUM, value.torque_accum);
        mem.set_f32(at + 0x270, value.torque);
        for i in 0..4 {
            mem.set_f32(at + 0x94 + 0x38 * i as u32, value.slot_a[i]);
            mem.set_f32(at + 0xac + 0x38 * i as u32, value.slot_b[i]);
        }
        value.smoothed.store(mem, at + ENGINE_STATE_SMOOTHED);
        value.solved.store(mem, at + 0x1f4);
    }

    fn load_engine_options(&self, at: u32) -> EngineOptions {
        let mem = self;
        let f = |off: u32| mem.f32(at + off);
        EngineOptions {
            gate_58: f(0x58),
            gate_5c: f(0x5c),
            stall_terms: [f(0x8), f(0x18), f(0x28), f(0x38)],
            stage_t: [f(0xc), f(0x1c), f(0x2c), f(0x3c)],
            fifth_t: f(0x70),
            rev_gate: mem.u32(at + 0x74),
            load_terms: [f(0x14), f(0x24), f(0x34), f(0x44)],
            roll_gate: mem.u32(at + 0x4),
        }
    }

    /// Writes the words `SimD870` changes (the stall terms); the rest of the block keeps its bytes.
    fn store_stall_terms(&mut self, at: u32, value: &EngineOptions) {
        let mem = self;
        for (i, v) in value.stall_terms.iter().enumerate() {
            mem.set_f32(at + 0x8 + 0x10 * i as u32, *v);
        }
    }

    /// The band of `gear` (any value, read at the band's offset as the original does).
    fn load_gear_band(&self, def: u32, gear: u32) -> GearBand {
        let mem = self;
        let band = def.wrapping_add(0x688).wrapping_add(gear.wrapping_mul(0x50));
        GearBand { ratio: mem.f32(band + 0xC), lower_edge: mem.f32(band + 0x3C), upper_edge: mem.f32(band + 0x4C) }
    }

    /// The definition at `def`.
    fn load_worker_def(&self, def: u32) -> WorkerDef {
        let mem = self;
        let f = |off: u32| mem.f32(def + off);
        WorkerDef {
            axles: std::array::from_fn(|k| {
                let at = def + 0x60 * k as u32;
                Axle {
                    direction: mem.u32(at + 0x18) as i32,
                    block_pair: [mem.u32(at + 0x28), mem.u32(at + 0x38)],
                    scale: mem.f32(at + 0x48),
                    active_index: mem.u32(at + 0x68),
                }
            }),
            wheels: std::array::from_fn(|k| {
                let at = def + 0xF0 * k as u32;
                Wheel { weight: mem.f32(at + 0x148), pack: mem.u32(at + 0x158) }
            }),
            gear_guard: cdef_flags(mem) & 0x80000 != 0,
            torque_scale: f(0x48),
            slip_gain_a: f(0x664),
            slip_gain_b: f(0x674),
            ratio_gain: f(DEF_RATIO_GAIN),
            slope_limit: f(0xA74),
            reach_limit: f(0xA84),
            lane_scale_gain: f(0x630),
            mode: mem.u32(def + DEF_MODE),
            pack_term: f(0x684),
        }
    }

    fn load_drive_worker(&self, this: u32) -> DriveWorker {
        let mem = self;
        DriveWorker {
            def: WorkerDef::load(mem, mem.u32(this + 4)),
            state: WorkerState::load(mem, mem.u32(this + 8)),
            this: WorkerThis::load(mem, this),
            obj: WorkerObj::load(mem, mem.u32(this + 0x170)),
        }
    }

    fn store_drive_worker(&mut self, this: u32, value: &DriveWorker) {
        let mem = self;
        let state_at = mem.u32(this + 8);
        let before = WorkerState::load(mem, state_at);
        value.state.store(mem, state_at, &before);
        value.this.store(mem, this);
        value.obj.store(mem, mem.u32(this + 0x170));
    }

    fn load_gear_work(&self, this: u32) -> GearWork {
        let mem = self;
        let gear = mem.u32(mem.u32(this + 8) + 0x54C);
        let shift = gear.wrapping_mul(4);
        GearWork {
            gear,
            band: GearBand::load(mem, mem.u32(this + 4), gear),
            scale: mem.f32(mem.u32(this + 0x170).wrapping_add(0x48).wrapping_add(shift)),
            dt: mem.f32(this.wrapping_add(0x174).wrapping_add(shift)),
        }
    }

    /// Writes the gear's scale and time step (the state's gear is the one `load` read).
    fn store_gear_work(&mut self, this: u32, value: &GearWork) {
        let mem = self;
        let shift = value.gear.wrapping_mul(4);
        mem.set_f32(mem.u32(this + 0x170).wrapping_add(0x48).wrapping_add(shift), value.scale);
        mem.set_f32(this.wrapping_add(0x174).wrapping_add(shift), value.dt);
    }

    /// The ring at `at`.
    fn load_ratio_ring(&self, at: u32) -> RatioRing {
        let mem = self;
        RatioRing { count: mem.u32(at), index: mem.u32(at + 4), values: mem.f32s(at + 8) }
    }

    fn store_ratio_ring(&mut self, at: u32, value: &RatioRing) {
        let mem = self;
        mem.set_u32(at, value.count);
        mem.set_u32(at + 4, value.index);
        for (k, v) in value.values.iter().enumerate() {
            mem.set_f32(at + 8 + 4 * k as u32, *v);
        }
    }

    fn load_lane_scale(&self, at: u32) -> LaneScale {
        let mem = self;
        LaneScale { scale: mem.f32(at + 8), gate: mem.u32(at + 0x1C) }
    }

    /// Writes the record at `at` (the four words; the rest of the object is left alone).
    fn store_unit_split(&mut self, at: u32, value: &UnitSplit) {
        let mem = self;
        mem.set_f32(at + 0x08, value.at_zero);
        mem.set_f32(at + 0x0c, value.w_low);
        mem.set_f32(at + 0x10, value.w_mid);
        mem.set_f32(at + 0x14, value.w_high);
    }

    /// The block's gain (`+0x6c`) that the curve workers scale by.
    fn curve_gain(&self, block: u32) -> f32 {
        let mem = self;
        mem.f32(block + 0x6C)
    }

    /// The block's row index (`+0xdc`) that `Worker4060` picks the row by.
    fn curve_index(&self, block: u32) -> u32 {
        let mem = self;
        mem.u32(block + 0xDC)
    }

    /// The gate word of the drive object, which `Hook4160` reads: `this+0x170` points at it, its `+0x80` is the gate.
    fn obj_gate(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(mem.u32(this + 0x170) + 0x80)
    }

    fn drive_record_f32(&self, this: u32, a: u32, at: u32) -> f32 {
        let mem = self;
        mem.f32(mem.u32(this + 0x08).wrapping_add(a.wrapping_mul(0xc8)).wrapping_add(at))
    }

    /// `E39_MgrB` (0x501fa0, name ours): the word at `this + 8`.
    fn mgr_b_word8(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + 8)
    }

    /// `E39_MgrB` (0x501fb0, name ours): entry `idx` (20 bytes each) of the table the word at `this + 0xc` points to.
    fn mgr_b_entry(&self, this: u32, idx: u32) -> u32 {
        let mem = self;
        mem.u32(this + 0xc).wrapping_add(idx.wrapping_mul(20))
    }

    /// `SetDriveLine`'s stores: the entries, the count, and entries `n..10000` with zero `q` and `v` and `s = -1`. The
    /// tags of the filled entries are left as they were. The log lines are skipped; a count over 10000 traps in the
    /// original.
    fn store_drive_line(&mut self, this: u32, entries: &[drive_line::Entry]) {
        let mem = self;
        let n = entries.len() as u32;
        assert!(n <= drive_line::MAX_ENTRIES, "more than 10000 drive-line entries");
        for (k, e) in (0..).zip(entries) {
            mem.store_entry(drive_line::entry_at(this, k), *e);
        }
        mem.set_u32(this + drive_line::COUNT_AT, n);
        for k in n..drive_line::MAX_ENTRIES {
            let at = drive_line::entry_at(this, k);
            Vec4::default().store(mem, at);
            Vec4::default().store(mem, at + 0x10);
            mem.set_f32(at + 0x20, -1.0);
        }
    }

    /// `CDriveLine::LerpAt` (0x50a920): the lerp of entry `idx`'s tangent `v` and entry `idx + 1`'s, at `t`, as the jump
    /// into `E11_VecLerp` takes them (self at +0x20 of the entry, the other at the next entry's +0x20). The count check is
    /// the caller's (the original traps past it).
    fn lerp_at(&self, this: u32, idx: i32, t: f32) -> Vec4 {
        let mem = self;
        let at = drive_line::entry_at(this, idx as u32);
        let next = at.wrapping_add(drive_line::STRIDE);
        Vec4::load(mem, at + 0x10).lerp(Vec4::load(mem, next + 0x10), t)
    }

    /// One row of the exhaust table (`SimEC40`): eight floats at `+0x1fc + 0x20 * i` of the definition.
    fn exhaust_row(&self, def: u32, i: u32) -> [f32; 8] {
        let mem = self;
        mem.f32s(def + 0x1fc + 0x20 * i)
    }

    /// The `n` knots of a curve at `at`.
    fn curve_knots(&self, at: u32, n: usize) -> Vec<CurveKnot> {
        let mem = self;
        (0..n as u32)
            .map(|k| CurveKnot { key: mem.f32(at + 0x20 * k + 0x0c), value: mem.f32(at + 0x20 * k + 0x1c) })
            .collect()
    }

    /// The knots of a bracket table (`E41_BlendW`'s argument): the count at `at`, the records' pointer at `at + 4`, and
    /// each record's key at +0 (0xc bytes apart).
    fn bracket_keys(&self, at: u32) -> Vec<f32> {
        let mem = self;
        let count = mem.u32(at) as usize;
        let records = mem.u32(at + 4);
        (0..count as u32).map(|k| mem.f32(records + 0xc * k)).collect()
    }

    /// The knots of a two-level blend table at `at`: the outer count at +0 and the records' pointer at +4, each record 0xc
    /// bytes (key, sub-table count, sub-table pointer). Each sub-table is read with its own count.
    fn blend_table(&self, at: u32) -> Vec<BlendKnot> {
        let mem = self;
        let count = mem.u32(at) as usize;
        let records = mem.u32(at + 4);
        (0..count as u32)
            .map(|k| {
                let row = records + 0xC * k;
                BlendKnot { key: mem.f32(row), records: mem.blend_records(row, mem.u32(row + 4) as usize) }
            })
            .collect()
    }

    /// The four words of a `BlendTerms` block at `at` (weight +0, value +4, response +8, scale +0xc).
    fn load_blend_terms(&self, at: u32) -> BlendTerms {
        let mem = self;
        BlendTerms { weight: mem.f32(at), value: mem.f32(at + 4), response: mem.f32(at + 8), scale: mem.f32(at + 0xc) }
    }

    fn store_blend_terms(&mut self, at: u32, block: BlendTerms) {
        let mem = self;
        mem.set_f32(at, block.weight);
        mem.set_f32(at + 4, block.value);
        mem.set_f32(at + 8, block.response);
        mem.set_f32(at + 0xc, block.scale);
    }

    /// The four words of a `Relaxation` block at `at` (remaining +0, accumulated +4, response +8, scale +0xc).
    fn load_relaxation(&self, at: u32) -> Relaxation {
        let mem = self;
        Relaxation {
            remaining: mem.f32(at),
            accumulated: mem.f32(at + 4),
            response: mem.f32(at + 8),
            scale: mem.f32(at + 0xc),
        }
    }

    fn store_relaxation(&mut self, at: u32, block: Relaxation) {
        let mem = self;
        mem.set_f32(at, block.remaining);
        mem.set_f32(at + 4, block.accumulated);
        mem.set_f32(at + 8, block.response);
        mem.set_f32(at + 0xc, block.scale);
    }

    /// The four outputs of `E41_BlendW`, written through the pointers the caller passed (`hi`, `lo` as ints).
    fn store_bracket(&mut self, b: &Bracket, out: [u32; 4]) {
        let mem = self;
        mem.set_u32(out[0], b.hi as u32);
        mem.set_u32(out[1], b.lo as u32);
        mem.set_f32(out[2], b.w_upper);
        mem.set_f32(out[3], b.w_lower);
    }

    /// `n` consecutive floats at `at`.
    fn f32_table(&self, at: u32, n: usize) -> Vec<f32> {
        let mem = self;
        (0..n as u32).map(|k| mem.f32(at + 4 * k)).collect()
    }

    /// The `count` records of a blend table (`E41_BlendX`/`BlendY`'s argument `desc`): the table's pointer is at
    /// `desc + 8`.
    /// The count is the first table's (at its `+4`), and the second table is read with the same count.
    fn blend_records(&self, desc: u32, count: usize) -> Vec<BlendRec> {
        let mem = self;
        let base = mem.u32(desc + 8);
        (0..count as u32)
            .map(|k| BlendRec {
                key: mem.f32(base + 0xc * k),
                y1: mem.f32(base + 0xc * k + 4),
                y2: mem.f32(base + 0xc * k + 8),
            })
            .collect()
    }

    /// The drive's definition (`+4`), which `CDrive::M8770` fills.
    fn drive_def(&self, drive: u32) -> u32 {
        let mem = self;
        mem.u32(drive + 4)
    }

    fn load_entry(&self, at: u32) -> Entry {
        let mem = self;
        Entry { q: Vec4::load(mem, at), v: Vec4::load(mem, at + 0x10), s: mem.f32(at + 0x20), tag: mem.u32(at + 0x24) }
    }

    fn store_entry(&mut self, at: u32, e: Entry) {
        let mem = self;
        e.q.store(mem, at);
        e.v.store(mem, at + 0x10);
        mem.set_f32(at + 0x20, e.s);
        mem.set_u32(at + 0x24, e.tag);
    }
}

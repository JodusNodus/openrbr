//! Replays the original's engine and driveline calls (`harness/vcu_vectors.py --set engine-driveline`, with the
//! fuzzed direct calls of `harness/vector_specs/engine-driveline.py`) through the ports.
//!
//! `RBR_ENGINE_DRIVELINE_VECTORS=<out>[:<out>...]` names the record files; without it the tests are skipped.
//! Each record is one call of the original with its arguments, its windows before and after, and the x87 return.

mod common;

use common::Inner;
use common::{check_windows, load, map_windows, shared_windows, Call};
use rbr_physics::engine_driveline::store::EngineStore;
use rbr_physics::engine_driveline::{drive, drive_curve, drive_leaf, drive_line, drive_worker, engine, layout, sim};
use rbr_physics::fpu::Precision;
use rbr_physics::math::vec::Vec4;

const PHYSICS_GLOBAL: u32 = 0x0165_9B70;
const VARIABLE: &str = "RBR_ENGINE_DRIVELINE_VECTORS";
const TABLE_LERP: u32 = 0x50d580;
const UNK_D6F0: u32 = 0x50d6f0;
const CURVE_ETA: u32 = 0x50d7a0;
const BLEND_W: u32 = 0x50d220;
const SIM_410: u32 = 0x50d410;
const SET_30: u32 = 0x504450;
const SET_44A0: u32 = 0x5044a0;
const SET_47B0: u32 = 0x5047b0;
const SET_4820: u32 = 0x504820;
const SET_TWO: u32 = 0x5043c0;
const SET_4570: u32 = 0x504570;
const TEST_4840: u32 = 0x504840;
const SIM_8CF0: u32 = 0x518cf0;
const GET_4520: u32 = 0x504520;
const GET_4530: u32 = 0x504530;
const GET_4540: u32 = 0x504540;

/// Checks the port's st0 against the recorded one. Any NaN matches any NaN: the sign of the default NaN differs
/// between x86 and arm64 (rbr-recomp `notes/rust-physics.md`, "Platform: default NaN").
fn check_st0(call: &Call, st0: impl Into<f64>, name: &str) {
    let want = f64::from_bits(call.st0_bits);
    let got: f64 = st0.into();
    assert!(
        got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
        "{name} call {}: st0 {got} vs {want} ({})",
        call.index,
        call.branch
    );
}

#[test]
fn table_lerp_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, TABLE_LERP) else {
        eprintln!("skipping table_lerp: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let table = mem.f32_table(call.args[0], call.args[4] as i32 as usize);
        let st0 = sim::table_lerp(
            &table,
            f32::from_bits(call.args[1]),
            f32::from_bits(call.args[2]),
            f32::from_bits(call.args[3]),
        );
        check_st0(call, st0, "E41_TableLerp");
        check_windows(call, &mem, "E41_TableLerp");
    }
    eprintln!("E41_TableLerp: {} calls match", calls.len());
}

#[test]
fn unk_d6f0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, UNK_D6F0) else {
        eprintln!("skipping unk_d6f0: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let split = sim::unk_d6f0(f32::from_bits(call.args[0]));
        split.store(&mut mem, call.args[1]);
        check_windows(call, &mem, "E41_UnkD6F0");
    }
    eprintln!("E41_UnkD6F0: {} calls match", calls.len());
}

#[test]
fn curve_eta_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, CURVE_ETA) else {
        eprintln!("skipping curve_eta: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let curve = mem.curve_knots(call.args[0], call.args[2] as i32 as usize);
        let st0 = sim::curve_eta(&curve, f32::from_bits(call.args[1]));
        check_st0(call, st0, "E41_CurveEta");
        check_windows(call, &mem, "E41_CurveEta");
    }
    eprintln!("E41_CurveEta: {} calls match", calls.len());
}

#[test]
fn blend_w_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, BLEND_W) else {
        eprintln!("skipping blend_w: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let keys = mem.bracket_keys(call.args[0]);
        let b = sim::blend_w(&keys, f32::from_bits(call.args[1]));
        mem.store_bracket(&b, [call.args[2], call.args[3], call.args[4], call.args[5]]);
        check_windows(call, &mem, "E41_BlendW");
    }
    eprintln!("E41_BlendW: {} calls match", calls.len());
}

#[test]
fn sim410_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_410) else {
        eprintln!("skipping abs_rpm_term: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let engine = call.args[0];
        let objects = layout::EngineObjects::at(&mem, engine);
        let def = layout::EngineDef::load(&mem, objects.def);
        let mut state = layout::EngineState::load(&mem, objects.state);
        let options = layout::EngineOptions::load(&mem, objects.options);
        let st0 = sim::abs_rpm_term(&def, &mut state, &options);
        state.store(&mut mem, objects.state);
        check_st0(call, st0, "CEngine::Sim410");
        check_windows(call, &mem, "CEngine::Sim410");
    }
    eprintln!("CEngine::Sim410: {} calls match", calls.len());
}

/// `check_windows`, with one allowance: a NaN word matches any NaN. The sign of the default NaN differs between x86 and
/// arm64 (rbr-recomp `notes/rust-physics.md`, "Platform: default NaN"); the damper steps write one from a 0/0 in `DamperCombine`.
fn check_windows_nan(call: &Call, mem: &rbr_physics::mem::Mem, name: &str) {
    for (k, at, len, _, expected) in &call.windows {
        let actual = mem.read(*at, *len);
        assert_eq!(actual.len(), expected.len(), "{name} call {} window {k}", call.index);
        for (word, (got, want)) in actual.chunks_exact(4).zip(expected.chunks_exact(4)).enumerate() {
            if got == want {
                continue;
            }
            let got = f32::from_le_bytes(got.try_into().unwrap());
            let want = f32::from_le_bytes(want.try_into().unwrap());
            assert!(
                got.is_nan() && want.is_nan(),
                "{name} call {} window {k} dword {word}: {got} vs {want}",
                call.index
            );
        }
    }
}

/// Runs `port` on each recorded call of `addr`: the port gets the call's windows mapped (its `this` is `args[0]`),
/// and writes the memory back through the layout's `store`. The windows are then compared by `check`.
fn each_call_with(
    addr: u32,
    name: &str,
    check: fn(&Call, &rbr_physics::mem::Mem, &str),
    port: impl Fn(&mut rbr_physics::mem::Mem, &Call),
) {
    let Some(calls) = load(VARIABLE, addr) else {
        eprintln!("skipping {name}: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        port(&mut mem, call);
        check(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

fn each_call(addr: u32, name: &str, port: impl Fn(&mut rbr_physics::mem::Mem, &Call)) {
    each_call_with(addr, name, check_windows, port);
}

#[test]
fn set30_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_30, "CDrive::Set30", |mem, call| {
        let drive_at = call.args[0];
        let def = layout::DriveDef::load(mem, layout::DriveObjects::at(mem, drive_at).def);
        let mut d = layout::Drive::load(mem, drive_at);
        drive::set_brake_in(&mut d, &def, f32::from_bits(call.args[1]));
        d.store(mem, drive_at);
    });
}

#[test]
fn set44a0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_44A0, "CDrive::Set44A0", |mem, call| {
        let drive_at = call.args[0];
        let mut d = layout::Drive::load(mem, drive_at);
        drive::set_throttle_in(&mut d, f32::from_bits(call.args[1]));
        d.store(mem, drive_at);
    });
}

#[test]
fn set4820_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_4820, "CDrive::Set4820", |mem, call| {
        let drive_at = call.args[0];
        let mut d = layout::Drive::load(mem, drive_at);
        drive::set_clutch_slip(&mut d, f32::from_bits(call.args[1]));
        d.store(mem, drive_at);
    });
}

#[test]
fn set47b0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_47B0, "CDrive::Set47B0", |mem, call| {
        let drive_at = call.args[0];
        let objects = layout::DriveObjects::at(mem, drive_at);
        let def = layout::DriveDef::load(mem, objects.def);
        let mut state = layout::DriveState::load(mem, objects.state);
        drive::request_gear(&mut state, def.gear_count, call.args[1] as i32);
        state.store_commanded_gear(mem, objects.state);
    });
}

#[test]
fn drive_getters_match_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    for (addr, name) in [(GET_4520, "CDrive::Get4520"), (GET_4540, "CDrive::Get4540")] {
        let Some(calls) = load(VARIABLE, addr) else {
            eprintln!("skipping {name}: {VARIABLE} not set");
            continue;
        };
        for call in &calls {
            let mem = map_windows(call);
            let state = layout::DriveState::load(&mem, layout::DriveObjects::at(&mem, call.args[0]).state);
            let st0 = if addr == GET_4520 { drive::engine_speed(&state) } else { drive::drive_speed_key(&state) };
            check_st0(call, st0, name);
            check_windows(call, &mem, name);
        }
        eprintln!("{name}: {} calls match", calls.len());
    }
    let Some(calls) = load(VARIABLE, GET_4530) else {
        eprintln!("skipping CDrive::Get4530: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let state = layout::DriveState::load(&mem, layout::DriveObjects::at(&mem, call.args[0]).state);
        assert_eq!(drive::gear(&state) as u32, call.eax, "CDrive::Get4530 call {}", call.index);
        check_windows(call, &mem, "CDrive::Get4530");
    }
    eprintln!("CDrive::Get4530: {} calls match", calls.len());
}

const SET_SCRATCH08: u32 = 0x50eb90;
const SIM_E370: u32 = 0x50e370;
const SIM_E470: u32 = 0x50e470;
const SIM_E9F0: u32 = 0x50e9f0;
const SIM_EC40: u32 = 0x50ec40;
const SIM_ECF0: u32 = 0x50ecf0;
const SIM_ED70: u32 = 0x50ed70;
const SIM_E520: u32 = 0x50e520;
const SIM_E660: u32 = 0x50e660;
const SIM_D870: u32 = 0x50d870;
const BLEND_X: u32 = 0x50ced0;
const BLEND_Y: u32 = 0x50d090;
/// The table `SimD870` rolls into (four words, one per slot).
const ROLL_TABLE: u32 = 0x007c_a668;

#[test]
fn set_scratch08_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_SCRATCH08, "CEngine::SetScratch08", |mem, call| {
        let engine_at = call.args[0];
        let mut this = layout::EngineThis::load(mem, engine_at);
        engine::set_pedal(&mut this.pedal, call.args[1]);
        this.store(mem, engine_at);
    });
}

#[test]
fn sim_e370_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_E370, "CEngine::SimE370", |mem, call| {
        let objects = layout::EngineObjects::at(mem, call.args[0]);
        let mut state = layout::EngineState::load(mem, objects.state);
        let (b, c) = engine::crank_tick(&mut state, f32::from_bits(call.args[1]));
        state.store(mem, objects.state);
        mem.set_u32(call.args[2], b as u32);
        mem.set_f32(call.args[3], c);
    });
}

#[test]
fn sim_e470_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_E470) else {
        eprintln!("skipping rev_gate: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let objects = layout::EngineObjects::at(&mem, call.args[0]);
        let this = layout::EngineThis::load(&mem, call.args[0]);
        let state = layout::EngineState::load(&mem, objects.state);
        let options = layout::EngineOptions::load(&mem, objects.options);
        let st0 =
            engine::rev_gate(state.speed, this.pedal.value, options.rev_gate, layout::EngineTuning::STOCK.rev_gate);
        check_st0(call, st0, "CEngine::SimE470");
        check_windows(call, &mem, "CEngine::SimE470");
    }
    eprintln!("CEngine::SimE470: {} calls match", calls.len());
}

#[test]
fn sim_e9f0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_E9F0, "CEngine::SimE9F0", |mem, call| {
        let objects = layout::EngineObjects::at(mem, call.args[0]);
        let def = layout::EngineDef::load(mem, objects.def);
        let options = layout::EngineOptions::load(mem, objects.options);
        let mut state = layout::EngineState::load(mem, objects.state);
        engine::compliance(&def, &options, &mut state);
        state.store(mem, objects.state);
    });
}

#[test]
fn sim_ec40_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_EC40) else {
        eprintln!("skipping exhaust_blend: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let objects = layout::EngineObjects::at(&mem, call.args[0]);
        let row = mem.exhaust_row(objects.def, call.args[1]);
        let q = call.args[2];
        let s: [f32; 12] = std::array::from_fn(|k| mem.f32(q + 4 * k as u32));
        let st0 = engine::exhaust_blend(&row, &s);
        check_st0(call, st0, "CEngine::SimEC40");
        check_windows(call, &mem, "CEngine::SimEC40");
    }
    eprintln!("CEngine::SimEC40: {} calls match", calls.len());
}

#[test]
fn sim_ecf0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_ECF0) else {
        eprintln!("skipping stall_guard: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let objects = layout::EngineObjects::at(&mem, call.args[0]);
        let state = layout::EngineState::load(&mem, objects.state);
        let options = layout::EngineOptions::load(&mem, objects.options);
        let answer = engine::stall_guard(state.speed, &options, layout::EngineTuning::STOCK.stall_speed) as u32;
        assert_eq!(answer, call.eax, "CEngine::SimECF0 call {}", call.index);
        check_windows(call, &mem, "CEngine::SimECF0");
    }
    eprintln!("CEngine::SimECF0: {} calls match", calls.len());
}

#[test]
fn sim_ed70_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_ED70, "CEngine::SimED70", |mem, call| {
        let objects = layout::EngineObjects::at(mem, call.args[0]);
        let def = layout::EngineDef::load(mem, objects.def);
        let q_at = call.args[2];
        let mut q: [f32; 8] = std::array::from_fn(|k| mem.f32(q_at + 4 * k as u32));
        engine::torque_curve(&def, f32::from_bits(call.args[1]), &mut q);
        for (k, v) in q.iter().enumerate() {
            mem.set_f32(q_at + 4 * k as u32, *v);
        }
    });
}

#[test]
fn sim_e520_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_E520, "CEngine::SimE520", |mem, call| {
        let engine_at = call.args[0];
        let objects = layout::EngineObjects::at(mem, engine_at);
        let seed_at = mem.u32(objects.options);
        let mut seed = mem.u32(seed_at);
        let mut state = layout::EngineState::load(mem, objects.state);
        let level = engine::misfire_roll(&mut state, &mut seed, f32::from_bits(call.args[1]));
        mem.set_u32(seed_at, seed);
        state.store(mem, objects.state);
        if let Some(level) = level {
            let mut this = layout::EngineThis::load(mem, engine_at);
            this.misfire_level = level;
            this.store(mem, engine_at);
        }
    });
}

#[test]
fn sim_e660_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_E660) else {
        eprintln!("skipping engine_torque: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let objects = layout::EngineObjects::at(&mem, call.args[0]);
        let def = layout::LoadDef::load(&mem, objects.def);
        let options = layout::EngineOptions::load(&mem, objects.options);
        let mut state = layout::EngineState::load(&mem, objects.state);
        let st0 = engine::torque_sum(&def, &options, &mut state, f32::from_bits(call.args[1]));
        state.store(&mut mem, objects.state);
        check_st0(call, st0, "CEngine::SimE660");
        check_windows(call, &mem, "CEngine::SimE660");
    }
    eprintln!("CEngine::SimE660: {} calls match", calls.len());
}

#[test]
fn sim_d870_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_D870) else {
        eprintln!("skipping slot_roll: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let objects = layout::EngineObjects::at(&mem, call.args[0]);
        let i = call.args[1] as usize;
        let state = layout::EngineState::load(&mem, objects.state);
        let mut options = layout::EngineOptions::load(&mem, objects.options);
        let seed_at = mem.u32(objects.options);
        let mut seed = mem.u32(seed_at);
        let mut option = options.stall_terms[i];
        let mut global = mem.f32(ROLL_TABLE + 4 * i as u32);
        let ret = engine::slot_roll(
            options.roll_gate,
            state.threshold_value,
            state.slot_a[i],
            state.slot_b[i],
            &mut option,
            &mut global,
            &mut seed,
            None,
        );
        mem.set_u32(seed_at, seed);
        mem.set_f32(ROLL_TABLE + 4 * i as u32, global);
        options.stall_terms[i] = option;
        options.store_stall_terms(&mut mem, objects.options);
        let eax = match ret {
            engine::RollRet::Null => 0,
            engine::RollRet::Quotient(q) => q,
            engine::RollRet::Slot => objects.options + 8 + 0x10 * i as u32,
            engine::RollRet::Options => objects.options,
        };
        assert_eq!(eax, call.eax, "CEngine::SimD870 call {}: eax", call.index);
        check_windows(call, &mem, "CEngine::SimD870");
    }
    eprintln!("CEngine::SimD870: {} calls match", calls.len());
}

#[test]
fn blend_x_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(BLEND_X, "E41_BlendX", |mem, call| {
        let (b_desc, c_desc) = (call.args[1], call.args[2]);
        let count = mem.u32(b_desc + 4) as usize;
        let b = mem.blend_records(b_desc, count);
        let c = mem.blend_records(c_desc, count);
        let (f, g, h) = sim::blend_x(
            f32::from_bits(call.args[0]),
            f32::from_bits(call.args[3]),
            f32::from_bits(call.args[4]),
            &b,
            &c,
        );
        mem.set_f32(call.args[5], f);
        mem.set_f32(call.args[6], g);
        mem.set_u32(call.args[7], h);
    });
}

#[test]
fn blend_y_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(BLEND_Y, "E41_BlendY", |mem, call| {
        let (b_desc, c_desc) = (call.args[1], call.args[2]);
        let count = mem.u32(b_desc + 4) as usize;
        let b = mem.blend_records(b_desc, count);
        let c = mem.blend_records(c_desc, count);
        let (f, g) = sim::blend_y(
            f32::from_bits(call.args[0]),
            f32::from_bits(call.args[3]),
            f32::from_bits(call.args[4]),
            &b,
            &c,
        );
        mem.set_f32(call.args[5], f);
        mem.set_f32(call.args[6], g);
    });
}

const SIM_E060: u32 = 0x50e060;
const SIM_E1F0: u32 = 0x50e1f0;
const BLEND_B: u32 = 0x50a650;
const LAST: u32 = 0x50ad40;
const THUNK_4F0: u32 = 0x50a4f0;
const CLAMP_MIX: u32 = 0x50b0f0;
const SET_DRIVE_LINE: u32 = 0x50aec0;
const M5970: u32 = 0x505970;
const M5A10: u32 = 0x505a10;
const M5D50: u32 = 0x505d50;
const M5BE0: u32 = 0x505be0;
const M6630: u32 = 0x506630;
const M6A00: u32 = 0x506a00;
const M6B20: u32 = 0x506b20;
const M6DB0: u32 = 0x506db0;
const M6DD0: u32 = 0x506dd0;
const M79B0: u32 = 0x5079b0;
const M9120: u32 = 0x509120;
const M9210: u32 = 0x509210;
const M8570: u32 = 0x508570;
const M7C10: u32 = 0x507c10;
const STEP4990: u32 = 0x504990;
const RATIO_CA0: u32 = 0x503ca0;
const MATVEC: u32 = 0x502400;
const STEP3870: u32 = 0x503870;
const TAB_E00: u32 = 0x503e00;
const WORKER_F60: u32 = 0x503f60;
const WORKER_4060: u32 = 0x504060;

/// Runs `port` on each recorded call of `addr`, whose return is st0 (checked by `check_st0`); the windows are compared
/// with `check_windows`.
fn each_st0<R: Into<f64>>(addr: u32, name: &str, port: impl Fn(&mut rbr_physics::mem::Mem, &Call) -> R) {
    let Some(calls) = load(VARIABLE, addr) else {
        eprintln!("skipping {name}: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let st0 = port(&mut mem, call);
        check_st0(call, st0, name);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn sim_e060_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call_with(SIM_E060, "CEngine::SimE060", check_windows_nan, |mem, call| {
        let objects = layout::EngineObjects::at(mem, call.args[0]);
        let table = mem.blend_table(call.args[4]);
        let mut block = mem.load_blend_terms(call.args[6]);
        let mut state = layout::EngineState::load(mem, objects.state);
        let g = engine::smooth_damper(
            &table,
            f32::from_bits(call.args[1]),
            f32::from_bits(call.args[2]),
            f32::from_bits(call.args[3]),
            f32::from_bits(call.args[5]),
            &mut block,
            &mut state.smoothed,
        );
        mem.store_blend_terms(call.args[6], block);
        state.store(mem, objects.state);
        mem.set_f32(call.args[7], g);
    });
}

#[test]
fn sim_e1f0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call_with(SIM_E1F0, "CEngine::SimE1F0", check_windows_nan, |mem, call| {
        let objects = layout::EngineObjects::at(mem, call.args[0]);
        let table = mem.blend_table(call.args[3]);
        let mut block = mem.load_relaxation(call.args[5]);
        let mut state = layout::EngineState::load(mem, objects.state);
        let f = engine::solve_damper(
            &table,
            f32::from_bits(call.args[1]),
            f32::from_bits(call.args[2]),
            f32::from_bits(call.args[4]),
            &mut block,
            &mut state.solved,
            engine::STANDARD_PRESSURE,
        );
        mem.store_relaxation(call.args[5], block);
        state.store(mem, objects.state);
        mem.set_f32(call.args[6], f);
    });
}

#[test]
fn blend_b_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(BLEND_B, "CDriveLine::BlendB", |mem, call| {
        let out = drive_line::blend_b(mem, call.args[0], call.args[1] as i32, f32::from_bits(call.args[2]));
        out.store(mem, call.args[3]);
    });
}

#[test]
fn last_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(LAST, "CDriveLine::Last", |mem, call| drive_line::last(mem, call.args[0]));
}

#[test]
fn thunk_4f0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // E12_SampleX is not ported: its result is the record's st0, and the host word is the window at 0x7eaba8.
    each_st0(THUNK_4F0, "CDriveLine::Thunk4F0", |mem, call| {
        drive_line::thunk_4f0(mem, call.args[1], |_host, _x| f64::from_bits(call.st0_bits) as f32)
    });
}

#[test]
fn sample_x_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // E12_SampleX is the thunk's callee: the record's st0 is the port's answer at the thunk's x.
    each_st0(THUNK_4F0, "E12_SampleX", |mem, call| {
        layout::sample_x(mem, f32::from_bits(call.args[1])).expect("the original does not assert")
    });
}

#[test]
fn clamp_mix_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(CLAMP_MIX, "CDriveLine::ClampMix", |mem, call| {
        let out = drive_line::clamp_mix(mem, call.args[0], f32::from_bits(call.args[1]));
        out.store(mem, call.args[2]);
    });
}

#[test]
fn set_drive_line_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_DRIVE_LINE, "CDriveLine::SetDriveLine", |mem, call| {
        drive_line::set_drive_line(mem, call.args[0], call.args[1]);
    });
}

#[test]
fn m5970_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M5970, "CDrive::M5970", |mem, call| {
        let x = layout::LaneScale::load(mem, call.args[1]);
        with_worker(mem, call.args[0], |w| drive_leaf::set_lane_scale(w, x));
    });
}

/// The word at `at` in the windows' pre bytes, if a window maps the whole word.
fn window_word(call: &Call, at: u32) -> Option<u32> {
    call.windows.iter().find_map(|(_, base, len, pre, _)| {
        let off = at.checked_sub(*base)? as usize;
        (off + 4 <= *len).then(|| u32::from_le_bytes(pre[off..off + 4].try_into().unwrap()))
    })
}

/// The memory a worker call runs on. The worker loads the whole objects its `this` points at (the definition, the state,
/// the object) and the physics CDef's flags, but the records map only the windows the original touched. So the objects
/// are zeroed first, at the pointers the windows give (zero when a window does not map the pointer), and the windows
/// are mapped over them. A port reads a zero only where the original reads nothing, and the windows check the rest.
fn worker_mem(call: &Call) -> rbr_physics::mem::Mem {
    let mut mem = rbr_physics::mem::Mem::new();
    let this = call.args[0];
    let ptr = |at: u32| window_word(call, at).unwrap_or(0);
    let zeroed = [
        (this, 0x200),
        (ptr(this + 4), 0x2840),
        (ptr(this + 8), 0x600),
        (ptr(this + 0x170), 0x100),
        (0, 0x30),
        (PHYSICS_GLOBAL, 4),
        (0x160CC, 4),
    ];
    for (at, len) in zeroed {
        mem.map(at, &vec![0; len]);
    }
    for (_, at, _, pre, _) in &call.windows {
        mem.map(*at, pre);
    }
    mem
}

/// Runs `port` on each recorded call of `addr` over `worker_mem`, and compares the windows.
fn each_worker_call(addr: u32, name: &str, port: impl Fn(&mut rbr_physics::mem::Mem, &Call)) {
    each_worker_call_with(addr, name, check_windows, port);
}

/// `each_worker_call` with the windows compared by `check`.
fn each_worker_call_with(
    addr: u32,
    name: &str,
    check: fn(&Call, &rbr_physics::mem::Mem, &str),
    port: impl Fn(&mut rbr_physics::mem::Mem, &Call),
) {
    let Some(calls) = load(VARIABLE, addr) else {
        eprintln!("skipping {name}: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = worker_mem(call);
        port(&mut mem, call);
        check(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// `each_st0` over `worker_mem`.
fn each_worker_st0<R: Into<f64>>(addr: u32, name: &str, port: impl Fn(&mut rbr_physics::mem::Mem, &Call) -> R) {
    let Some(calls) = load(VARIABLE, addr) else {
        eprintln!("skipping {name}: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = worker_mem(call);
        let st0 = port(&mut mem, call);
        check_st0(call, st0, name);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// Loads the worker of `this` from `mem`, runs `port` on it and writes it back.
fn with_worker<T>(mem: &mut rbr_physics::mem::Mem, this: u32, port: impl FnOnce(&mut layout::DriveWorker) -> T) -> T {
    let mut w = layout::DriveWorker::load(mem, this);
    let out = port(&mut w);
    w.store(mem, this);
    out
}

/// `with_worker` for the gear workers, which also load the gear's words (written last).
fn with_gear<T>(
    mem: &mut rbr_physics::mem::Mem,
    this: u32,
    port: impl FnOnce(&mut layout::DriveWorker, &mut layout::GearWork) -> T,
) -> T {
    let mut w = layout::DriveWorker::load(mem, this);
    let mut g = layout::GearWork::load(mem, this);
    let out = port(&mut w, &mut g);
    w.store(mem, this);
    g.store(mem, this);
    out
}

#[test]
fn m5a10_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_st0(M5A10, "CDrive::M5A10", |mem, call| {
        let w = layout::DriveWorker::load(mem, call.args[0]);
        drive_leaf::gear_speed_error(&w, &layout::GearWork::load(mem, call.args[0]))
    });
}

#[test]
fn m5d50_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M5D50, "CDrive::M5D50", |mem, call| {
        with_worker(mem, call.args[0], drive_leaf::clutch_flags);
    });
}

#[test]
fn m5be0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M5BE0, "CDrive::M5BE0", |mem, call| {
        let x = call.args[1] as usize;
        with_worker(mem, call.args[0], |w| drive_leaf::clutch_values(w, x));
    });
}

#[test]
fn m6630_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M6630, "CDrive::M6630", |mem, call| {
        with_worker(mem, call.args[0], drive_leaf::axle_pair_stages);
    });
}

#[test]
fn m6a00_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M6A00, "CDrive::M6A00", |mem, call| {
        let x = call.args[1] as usize;
        with_worker(mem, call.args[0], |w| drive_leaf::block_lane_step(w, x));
    });
}

#[test]
fn m6b20_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M6B20, "CDrive::M6B20", |mem, call| {
        let x = call.args[1] as usize;
        let eax = with_worker(mem, call.args[0], |w| drive_leaf::block_pair_gate(w, x));
        assert_eq!(u32::from(eax), call.eax, "CDrive::M6B20 call {}: eax", call.index);
    });
}

/// Answers one M79B0 call's external calls from its record, in order: `Get30` returns the recorded eax, and the
/// emit's payload must equal the bytes of its recorded window (the payload the original passed by address).
struct GearCalls<'a> {
    inner: &'a [Inner],
    next: usize,
}

impl GearCalls<'_> {
    fn take(&mut self, addr: u32) -> &Inner {
        let call = self.inner.get(self.next).unwrap_or_else(|| {
            panic!("the port made external call {} ({addr:#x}), the original made {}", self.next, self.inner.len())
        });
        assert_eq!(call.addr, addr, "external call {}: callee", self.next);
        self.next += 1;
        call
    }
}

impl drive_worker::GearHost for GearCalls<'_> {
    fn car_get30(&mut self, car: u32) -> u32 {
        let call = self.take(0x4E67F0);
        assert_eq!(call.args, [car], "Get30 call: the car");
        call.eax
    }

    fn emit_gear_change(&mut self, payload: [u32; 4]) {
        let call = self.take(0x506F20);
        let bytes: Vec<u8> = payload.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(call.windows[0].3, bytes, "gear change payload");
    }
}

#[test]
fn m79b0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M79B0, "CDrive::M79B0", |mem, call| {
        let mut host = GearCalls { inner: &call.inner, next: 0 };
        let w = layout::DriveWorker::load(mem, call.args[0]);
        drive_worker::signal_gear_change(&w, &mut host, call.args[1]);
        assert_eq!(host.next, call.inner.len(), "CDrive::M79B0 call {}: external calls", call.index);
    });
}

#[test]
fn m9120_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M9120, "CDrive::M9120", |mem, call| {
        let mut host = GearCalls { inner: &call.inner, next: 0 };
        let dt = f32::from_bits(call.args[1]);
        let changed = with_gear(mem, call.args[0], |w, g| drive_worker::gear_band_guard(w, g, &mut host, dt));
        assert_eq!(u32::from(changed), call.eax, "CDrive::M9120 call {}: eax", call.index);
        assert_eq!(host.next, call.inner.len(), "CDrive::M9120 call {}: external calls", call.index);
    });
}

#[test]
fn m9210_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M9210, "CDrive::M9210", |mem, call| {
        let mut host = GearCalls { inner: &call.inner, next: 0 };
        let dt = f32::from_bits(call.args[1]);
        with_gear(mem, call.args[0], |w, g| drive_worker::gear_logic(w, g, &mut host, dt));
        assert_eq!(host.next, call.inner.len(), "CDrive::M9210 call {}: external calls", call.index);
    });
}

#[test]
fn m8570_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M8570, "CDrive::M8570", |mem, call| {
        let mut host = GearCalls { inner: &call.inner, next: 0 };
        let dt = f32::from_bits(call.args[1]);
        with_gear(mem, call.args[0], |w, g| drive_worker::gear_trim(w, g, &mut host, dt));
        assert_eq!(host.next, call.inner.len(), "CDrive::M8570 call {}: external calls", call.index);
    });
}

/// Answers the external calls of one record in order (`E39_Step4990`, `E39_MatVec`, `E39_Step3870`). Each callee is
/// checked in order. The typed answer is the output window's post bytes; the input windows must hold what the port
/// passed. `base` is the call's memory before it ran, and `this` the worker, for the steps that edit the worker.
struct Externals<'a> {
    inner: Vec<&'a Inner>,
    next: usize,
    base: rbr_physics::mem::Mem,
    this: u32,
}

/// The answers for the external calls of `call`.
/// The port's external calls of `call`. A listed `Sim8D90` under `Fwd18E20` also puts its `Sim80D0` on the open
/// `Step1E0` frame, and the port reaches that reset only through `Sim8D90`, so it is left out here.
fn externals(call: &Call) -> Externals<'_> {
    Externals {
        inner: call.inner.iter().filter(|c| c.addr != SIM_80D0).collect(),
        next: 0,
        base: worker_mem(call),
        this: call.args[0],
    }
}

/// `base` with the windows of an external call mapped, as they were before the call (`after` false) or after it.
fn window_mem(base: &rbr_physics::mem::Mem, call: &Inner, after: bool) -> rbr_physics::mem::Mem {
    let mut mem = base.clone();
    for (_, at, _, pre, post) in &call.windows {
        mem.map(*at, if after { post } else { pre });
    }
    mem
}

impl<'a> Externals<'a> {
    fn take(&mut self, addr: u32) -> &'a Inner {
        let call = *self.inner.get(self.next).unwrap_or_else(|| {
            panic!("the port called {} externals, the original {}", self.next + 1, self.inner.len())
        });
        assert_eq!(call.addr, addr, "external call {}: callee", self.next);
        self.next += 1;
        call
    }
}

/// The pre and post bytes of the window at `at` of an external call.
fn inner_window(call: &Inner, at: u32) -> (&[u8], &[u8]) {
    let (_, _, _, pre, post) =
        call.windows.iter().find(|w| w.1 == at).unwrap_or_else(|| panic!("no window at {at:#x} in the external call"));
    (pre, post)
}

fn float_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn floats<const N: usize>(bytes: &[u8]) -> [f32; N] {
    std::array::from_fn(|k| f32::from_le_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap()))
}

impl drive_worker::Step4990Host for Externals<'_> {
    fn mat4_inverse_transposed(&mut self, mat: [f32; 16]) -> [f32; 16] {
        let call = self.take(STEP4990);
        let (input, _) = inner_window(call, call.args[0]);
        assert_eq!(input, float_bytes(&mat), "E39_Step4990 call {}: the matrix", self.next - 1);
        floats(inner_window(call, call.args[1]).1)
    }
}

impl drive_worker::MatVecHost for Externals<'_> {
    fn mat_vec(&mut self, m: [f32; 16], v: [f32; 4]) -> [f32; 4] {
        let call = self.take(MATVEC);
        let n = self.next - 1;
        assert_eq!(inner_window(call, call.args[0]).0, float_bytes(&m), "E39_MatVec call {n}: the matrix");
        assert_eq!(inner_window(call, call.args[1]).0, float_bytes(&v), "E39_MatVec call {n}: the vector");
        floats(inner_window(call, call.args[2]).1)
    }
}

impl drive_worker::Step3870Host for Externals<'_> {
    fn free_wheel_step(&mut self, w: &mut layout::DriveWorker, i: usize, dt: f32) {
        let call = self.take(STEP3870);
        let n = self.next - 1;
        assert_eq!(call.args, [self.this, i as u32, dt.to_bits()], "E39_Step3870 call {n}: the arguments");
        let before = layout::DriveWorker::load(&window_mem(&self.base, call, false), self.this);
        assert_eq!(format!("{w:?}"), format!("{before:?}"), "E39_Step3870 call {n}: the worker it was given");
        *w = layout::DriveWorker::load(&window_mem(&self.base, call, true), self.this);
        // The port of the call (`free_wheel_step`) on the worker it was given gives the worker the call left.
        let mut ported = before;
        drive_worker::free_wheel_step(&mut ported, i, dt);
        assert_eq!(format!("{ported:?}"), format!("{w:?}"), "E39_Step3870 call {n}: the port's worker");
    }
}

#[test]
fn m7c10_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call(M7C10, "CDrive::M7C10", |mem, call| {
        let mut host = externals(call);
        with_worker(mem, call.args[0], |w| drive_worker::reset_wheels(w, &mut host));
        assert_eq!(host.next, host.inner.len(), "CDrive::M7C10 call {}: step calls", call.index);
    });
}

const M96C0: u32 = 0x5096c0;
const M7AA0: u32 = 0x507aa0;
const M5B00: u32 = 0x505b00;
const M5F10: u32 = 0x505f10;
const M60D0: u32 = 0x5060d0;
const M6540: u32 = 0x506540;

// The pair worker's records compare a NaN word with any NaN: a 0/0 or an infinite inverse writes the default NaN, whose
// sign differs between x86 and arm64 (`check_windows_nan`).

#[test]
fn m96c0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call_with(M96C0, "CDrive::M96C0", check_windows_nan, |mem, call| {
        let dt = f32::from_bits(call.args[1]);
        with_worker(mem, call.args[0], |w| drive_worker::pair_worker(w, dt));
    });
}

#[test]
fn step3870_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call_with(STEP3870, "E39_Step3870", check_windows_nan, |mem, call| {
        let (i, dt) = (call.args[1] as usize, f32::from_bits(call.args[2]));
        with_worker(mem, call.args[0], |w| drive_worker::free_wheel_step(w, i, dt));
    });
}

#[test]
fn m7aa0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call_with(M7AA0, "CDrive::M7AA0", check_windows_nan, |mem, call| {
        with_worker(mem, call.args[0], drive_worker::reset_pair);
    });
}

#[test]
fn m6540_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_worker_call_with(M6540, "CDrive::M6540", check_windows_nan, |mem, call| {
        with_worker(mem, call.args[0], drive_leaf::pair_axle_stage);
    });
}

/// A pair leaf on wheel `x`.
type PairLeaf = fn(&mut layout::DriveWorker, usize);

#[test]
fn pair_leaves_match_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let leaves: [(u32, &str, PairLeaf); 3] = [
        (M5B00, "CDrive::M5B00", drive_leaf::pair_clutch_values),
        (M5F10, "CDrive::M5F10", drive_leaf::pair_slip_value),
        (M60D0, "CDrive::M60D0", drive_leaf::pair_latch),
    ];
    for (addr, name, leaf) in leaves {
        each_worker_call_with(addr, name, check_windows_nan, |mem, call| {
            let x = call.args[1] as usize;
            with_worker(mem, call.args[0], |w| leaf(w, x));
        });
    }
}

#[test]
fn ratio_ca0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(RATIO_CA0, "E39_RatioCA0", |mem, call| {
        let mut ring = layout::RatioRing::load(mem, call.args[1]);
        let mean = drive_worker::ring_average(&mut ring, f32::from_bits(call.args[0]));
        ring.store(mem, call.args[1]);
        mean
    });
}

#[test]
fn tab_e00_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(TAB_E00, "E39_TabE00", |mem, call| {
        let table = mem.f32_table(call.args[0], 10);
        drive_curve::unsigned_table_lerp(
            &table,
            call.args[1] as i32,
            call.args[2] as i32,
            call.args[3] as i32,
            call.args[4] as i32,
        )
    });
}

#[test]
fn worker_f60_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, WORKER_F60) else {
        eprintln!("skipping E39_WorkerF60: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let row = layout::CurveRow::load(&mem, call.args[1]);
        let tables = layout::CurveTables::load(&mem, call.args[4]);
        let block_gain = mem.curve_gain(call.args[3]);
        let mut state = layout::CurveState::load(&mem, call.args[2]);
        let ok = drive_curve::count_down(
            f32::from_bits(call.args[0]),
            &row,
            &mut state,
            &tables,
            block_gain,
            f32::from_bits(call.args[5]),
        );
        state.store(&mut mem, call.args[2]);
        assert_eq!(u32::from(ok), call.eax, "E39_WorkerF60 call {}: eax", call.index);
        check_windows(call, &mem, "E39_WorkerF60");
    }
    eprintln!("E39_WorkerF60: {} calls match", calls.len());
}

#[test]
fn worker_4060_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(WORKER_4060, "E39_Worker4060", |mem, call| {
        let row = layout::CurveRow::load(mem, layout::curve_row_at(call.args[3], mem.curve_index(call.args[2])));
        let tables = layout::CurveTables::load(mem, call.args[3]);
        let block_gain = mem.curve_gain(call.args[2]);
        let mut state = layout::CurveState::load(mem, call.args[1]);
        let v = drive_curve::row_value(
            f32::from_bits(call.args[0]),
            &row,
            &mut state,
            &tables,
            block_gain,
            f32::from_bits(call.args[4]),
        );
        state.store(mem, call.args[1]);
        v
    });
}

impl drive_worker::GearHost for Externals<'_> {
    fn car_get30(&mut self, car: u32) -> u32 {
        let call = self.take(0x4E67F0);
        assert_eq!(call.args, [car], "Get30 call: the car");
        call.eax
    }

    fn emit_gear_change(&mut self, payload: [u32; 4]) {
        let call = self.take(0x506F20);
        let bytes: Vec<u8> = payload.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(call.windows[0].3, bytes, "gear change payload");
    }
}

const CLEAR_LANES: u32 = 0x502c00;

#[test]
fn clear_lanes_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(CLEAR_LANES, "E39_ClearLanes", |mem, call| {
        layout::clear_lanes(mem, call.args[0]);
    });
}

const SIM_8D90: u32 = 0x518d90;
const SIM_80D0: u32 = 0x5180d0;

/// The host's `Sim80D0`: its record's post state goes into the memory the port reads (shared with it), and its
/// arguments are checked against the record.
struct Sim43Externals<'a> {
    inner: &'a [Inner],
    next: usize,
    mem: rbr_physics::mem::Mem,
}

impl engine::E43Host for Sim43Externals<'_> {
    fn intake_state_reset(&mut self, this: u32, x: u32) {
        let call = self.inner.get(self.next).expect("the port reset an entry the original did not");
        assert_eq!(call.addr, SIM_80D0, "external call {}: callee", self.next);
        assert_eq!(call.args, [this, x], "external call {}: arguments", self.next);
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        self.next += 1;
    }
}

impl engine::Sim8890Host for Sim43Externals<'_> {
    fn s30(&mut self, _car: u32) -> u32 {
        panic!("the car's walk is off in these records")
    }

    fn mb560(&mut self, _a: u32, _b: u32) -> [f32; 4] {
        panic!("the car's walk is off in these records")
    }
}

impl engine::EngineStepHost for Sim43Externals<'_> {
    // The sound, graph and log sinks write nothing the records compare.
    fn emit(&mut self, _signal: u32) {}

    fn graph_push(&mut self, _graph: u32, _value: f32) {}

    fn floor_log(&mut self, _value: f32) {}
}

#[test]
fn sim8d90_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_8D90) else {
        eprintln!("skipping E43EngineSim::Sim8D90: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        let mut host = Sim43Externals { inner: &call.inner, next: 0, mem: mem.clone() };
        let st0 = layout::sim8d90(&mut host, &mut mem, call.args[0], call.args[1], call.args[2]);
        assert_eq!(host.next, call.inner.len(), "E43EngineSim::Sim8D90 call {}: externals", call.index);
        check_st0(call, st0, "E43EngineSim::Sim8D90");
        check_windows(call, &mem, "E43EngineSim::Sim8D90");
    }
    eprintln!("E43EngineSim::Sim8D90: {} calls match", calls.len());
}

/// `Sim80D0` on the original's direct calls with fuzzed engines (`fuzz_sim80d0`, the seed word is one of the windows,
/// so the draw is the original's). The port's eax (the roll's) and windows must match.
#[test]
fn sim80d0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_80D0) else {
        eprintln!("skipping E43EngineSim::Sim80D0: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::intake_state_reset(&mut mem, call.args[0], call.args[1], &layout::EngineTuning::STOCK);
        assert_eq!(eax, call.eax, "E43EngineSim::Sim80D0 call {}: eax", call.index);
        check_windows_nan(call, &mem, "E43EngineSim::Sim80D0");
    }
    eprintln!("E43EngineSim::Sim80D0: {} calls match", calls.len());
}

const SIM_8E20: u32 = 0x518e20;

/// `Sim8E20` on the original's direct calls with fuzzed engines (`fuzz_sim8e20`). Its inner `Sim80D0` calls replay from
/// the records; the sinks are no-ops, and the port's st0 is the sum the asm leaves (`g9 + b`).
#[test]
fn sim8e20_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_8E20) else {
        eprintln!("skipping E43EngineSim::Sim8E20: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        let mut host = Sim43Externals { inner: &call.inner, next: 0, mem: mem.clone() };
        let st0 = layout::engine_step(
            &mut host,
            &mut mem,
            call.args[0],
            f32::from_bits(call.args[1]),
            f32::from_bits(call.args[2]),
            &layout::EngineTuning::STOCK,
        );
        assert_eq!(host.next, call.inner.len(), "E43EngineSim::Sim8E20 call {}: externals", call.index);
        check_st0(call, st0, "E43EngineSim::Sim8E20");
        check_windows_nan(call, &mem, "E43EngineSim::Sim8E20");
    }
    eprintln!("E43EngineSim::Sim8E20: {} calls match", calls.len());
}

const SIM_7D30: u32 = 0x517d30;

/// `Sim7D30` on the original's direct calls with fuzzed engines (`fuzz_sim7d30`). The port's eax and windows must match.
#[test]
fn sim7d30_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_7D30) else {
        eprintln!("skipping E43EngineSim::Sim7D30: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::turbo_energy_step(
            &mut mem,
            call.args[0],
            f32::from_bits(call.args[1]),
            &layout::EngineTuning::STOCK,
        );
        assert_eq!(eax, call.eax, "E43EngineSim::Sim7D30 call {}: eax", call.index);
        check_windows_nan(call, &mem, "E43EngineSim::Sim7D30");
    }
    eprintln!("E43EngineSim::Sim7D30: {} calls match", calls.len());
}

const SIM_8610: u32 = 0x518610;

/// `Sim8610` on the original's direct calls with fuzzed engines (`fuzz_sim8610`). The port's eax and windows must match.
#[test]
fn sim8610_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_8610) else {
        eprintln!("skipping E43EngineSim::Sim8610: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::sim8610(&mut mem, call.args[0], f32::from_bits(call.args[1]));
        assert_eq!(eax, call.eax, "E43EngineSim::Sim8610 call {}: eax", call.index);
        check_windows_nan(call, &mem, "E43EngineSim::Sim8610");
    }
    eprintln!("E43EngineSim::Sim8610: {} calls match", calls.len());
}

const SIM_8B20: u32 = 0x518b20;

/// `Sim8B20` on the original's direct calls with fuzzed engines (`fuzz_sim8b20`). The port's eax and windows must match.
#[test]
fn sim8b20_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_8B20) else {
        eprintln!("skipping E43EngineSim::Sim8B20: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::fuel_air_terms(&mut mem, call.args[0], f32::from_bits(call.args[1]));
        assert_eq!(eax, call.eax, "E43EngineSim::Sim8B20 call {}: eax", call.index);
        check_windows_nan(call, &mem, "E43EngineSim::Sim8B20");
    }
    eprintln!("E43EngineSim::Sim8B20: {} calls match", calls.len());
}

const SIM_8890: u32 = 0x518890;

/// The car's walk is not in the records (`fuzz_sim8890` keeps `state+0x1c` at or below the walk's threshold), so the
/// port must not reach the host.
struct NoWalk;

impl engine::Sim8890Host for NoWalk {
    fn s30(&mut self, _car: u32) -> u32 {
        panic!("the port walked the car, which these records do not cover")
    }

    fn mb560(&mut self, _a: u32, _b: u32) -> [f32; 4] {
        panic!("the port walked the car, which these records do not cover")
    }
}

/// `Sim8890` on the original's direct calls with fuzzed engines (`fuzz_sim8890`), the car's walk off. The port's eax and
/// windows must match.
#[test]
fn sim8890_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SIM_8890) else {
        eprintln!("skipping E43EngineSim::Sim8890: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::sim8890(
            &mut NoWalk,
            &mut mem,
            call.args[0],
            f32::from_bits(call.args[1]),
            &layout::EngineTuning::STOCK,
        );
        assert_eq!(eax, call.eax, "E43EngineSim::Sim8890 call {}: eax", call.index);
        check_windows_nan(call, &mem, "E43EngineSim::Sim8890");
    }
    eprintln!("E43EngineSim::Sim8890: {} calls match", calls.len());
}

const VEC_LERP: u32 = 0x473c90;

#[test]
fn vec_lerp_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(VEC_LERP, "E11_VecLerp", |mem, call| {
        let a = Vec4::load(mem, call.args[0]);
        let b = Vec4::load(mem, call.args[1]);
        let v = a.lerp(b, f32::from_bits(call.args[2]));
        v.store(mem, call.args[3]);
    });
}

const SAMPLE_BIG: u32 = 0x50b300;

#[test]
fn sample_big_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VARIABLE, SAMPLE_BIG) else {
        eprintln!("skipping CDriveLine::SampleBig: {VARIABLE} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let got = layout::sample_big(&mem, call.args[0], call.args[1], f32::from_bits(call.args[2]));
        check_st0(call, got, "CDriveLine::SampleBig");
        check_windows(call, &mem, "CDriveLine::SampleBig");
    }
    eprintln!("CDriveLine::SampleBig: {} calls match", calls.len());
}

const SIM_EBB0: u32 = 0x50ebb0;

#[test]
fn sim_ebb0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_EBB0, "CEngine::SimEBB0", |mem, call| {
        layout::sim_ebb0(mem, call.args[0]);
    });
}

const SET_48: u32 = 0x50d310;
const SET_STATE4: u32 = 0x50d340;
const GET_48_58: u32 = 0x50d380;
const SIM_E970: u32 = 0x50e970;
const SIM_E980: u32 = 0x50e980;
const SIM_E9A0: u32 = 0x50e9a0;
const SIM_EE60: u32 = 0x50ee60;
const GET_STATE188: u32 = 0x50eb80;

#[test]
fn set48_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_48, "CEngine::Set48", |mem, call| {
        let eax = layout::set_options(mem, call.args[0], call.args[1]);
        assert_eq!(eax, call.eax, "CEngine::Set48 call {}: eax", call.index);
    });
}

#[test]
fn set_state4_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_STATE4, "CEngine::SetState4", |mem, call| {
        let words = [call.args[1], call.args[2], call.args[3], call.args[4]];
        let eax = layout::set_cut_flags(mem, call.args[0], words);
        assert_eq!(eax, call.eax, "CEngine::SetState4 call {}: eax", call.index);
    });
}

#[test]
fn get48_58_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(GET_48_58, "CEngine::Get48_58", |mem, call| layout::options_gate_58(mem, call.args[0]));
}

#[test]
fn sim_e970_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(SIM_E970, "CEngine::SimE970", |mem, call| layout::sim_e970(mem, call.args[0]));
}

#[test]
fn sim_e980_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_E980, "CEngine::SimE980", |mem, call| {
        let eax = layout::rpm_above_40(mem, call.args[0]);
        assert_eq!(eax, call.eax, "CEngine::SimE980 call {}: eax", call.index);
    });
}

#[test]
fn sim_e9a0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_E9A0, "CEngine::SimE9A0", |mem, call| {
        let eax = layout::sim_e9a0(mem, call.args[0]);
        assert_eq!(eax, call.eax, "CEngine::SimE9A0 call {}: eax", call.index);
    });
}

#[test]
fn sim_ee60_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(SIM_EE60, "CEngine::SimEE60", |mem, call| layout::sim_ee60(mem, call.args[0]));
}

#[test]
fn get_state188_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(GET_STATE188, "CEngine::GetState188", |mem, call| layout::engine_state_speed(mem, call.args[0]));
}

#[test]
fn set_two_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_TWO, "CDrive::SetTwo", |mem, call| {
        let eax = layout::set_two(
            mem,
            call.args[0],
            call.args[1],
            f32::from_bits(call.args[2]),
            f32::from_bits(call.args[3]),
        );
        assert_eq!(eax, call.eax, "CDrive::SetTwo call {}: eax", call.index);
    });
}

#[test]
fn set4570_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SET_4570, "CDrive::Set4570", |mem, call| {
        let eax = layout::set_diff_lock(mem, call.args[0], call.args[1], f32::from_bits(call.args[2]));
        assert_eq!(eax, call.eax, "CDrive::Set4570 call {}: eax", call.index);
    });
}

#[test]
fn m6db0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(M6DB0, "CDrive::M6DB0", |mem, call| layout::m6db0(mem, call.args[0], call.args[1]));
}

#[test]
fn m6dd0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_st0(M6DD0, "CDrive::M6DD0", |mem, call| layout::m6dd0(mem, call.args[0], call.args[1]));
}

#[test]
fn test4840_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(TEST_4840, "CDrive::Test4840", |mem, call| {
        let eax = layout::drive_gate(mem, call.args[0]);
        assert_eq!(eax, call.eax, "CDrive::Test4840 call {}: eax", call.index);
    });
}

/// The emit's sink for `Sim8CF0`: it writes nothing the records compare, and its eax is 0 on the records' path.
struct NoSignal;

impl engine::Sim8CF0Host for NoSignal {
    fn signal(&mut self, _obj: u32, _arg: u32) -> u32 {
        0
    }
}

#[test]
fn sim8cf0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(SIM_8CF0, "E43EngineSim::Sim8CF0", |mem, call| {
        // The signal table is not in the windows; the sink ignores the object it points at.
        mem.map(0x0089_3600, &0x0010_0000u32.to_le_bytes());
        let eax = layout::sim8cf0(&mut NoSignal, mem, call.args[0]);
        assert_eq!(eax, call.eax, "E43EngineSim::Sim8CF0 call {}: eax", call.index);
    });
}

#[test]
fn mgr_b_word8_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(0x501fa0, "E39_MgrB_Word8", |mem, call| {
        let eax = mem.mgr_b_word8(call.args[0]);
        assert_eq!(eax, call.eax, "E39_MgrB_Word8 call {}: eax", call.index);
    });
}

#[test]
fn mgr_b_entry_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(0x501fb0, "E39_MgrB_Entry", |mem, call| {
        let eax = mem.mgr_b_entry(call.args[0], call.args[1]);
        assert_eq!(eax, call.eax, "E39_MgrB_Entry call {}: eax", call.index);
    });
}

#[test]
fn mgr_b_field_at_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(0x502060, "E39_MgrB_FieldAt", |_mem, call| {
        let eax = layout::mgr_b_field_at(call.args[0], call.args[1]);
        assert_eq!(eax, call.eax, "E39_MgrB_FieldAt call {}: eax", call.index);
    });
}

#[test]
fn copy_row_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    each_call(0x502460, "E39_MgrD_CopyRow", |mem, call| {
        let eax = layout::copy_row(mem, call.args[0], call.args[1], call.args[2]);
        assert_eq!(eax, call.eax, "E39_MgrD_CopyRow call {}: eax", call.index);
    });
}

/// `mat4_mul_vec4` (reached by the drive, so checked by rust_step): each lane is `c0 * x + c1 * y + c2 * z + c3 * w`
/// added left to right, and the output may overlap the vector.
#[test]
fn mat4_mul_vec4_adds_the_columns_left_to_right() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = rbr_physics::mem::Mem::new();
    let (m, v) = (0x0100_0000, 0x0200_0000);
    for k in 0..4u32 {
        for i in 0..4u32 {
            mem.set_f32(m + 0x10 * k + 4 * i, (k * 4 + i) as f32 + 0.1);
        }
        mem.set_f32(v + 4 * k, 1.0 + k as f32 * 0.3);
    }
    let s: [f32; 4] = std::array::from_fn(|k| 1.0 + k as f32 * 0.3);
    let lane = |i: usize| {
        let c = |k: usize| (k * 4 + i) as f32 + 0.1;
        c(0) * s[0] + c(1) * s[1] + c(2) * s[2] + c(3) * s[3]
    };
    let want: Vec<f32> = (0..4).map(lane).collect();
    assert_eq!(layout::mat4_mul_vec4(&mut mem, m, v, v), v);
    for (i, w) in want.iter().enumerate() {
        assert_eq!(mem.f32(v + 4 * i as u32), *w, "lane {i}");
    }
}

/// `E39_RowsCopy` (name ours): the transpose, output word `4 k + j` is input word `4 j + k`.
#[test]
fn transpose_swaps_rows_and_columns() {
    let _pc24 = Precision::Pc24.scope();
    let m: [f32; 16] = std::array::from_fn(|i| i as f32);
    let t = rbr_physics::engine_driveline::mat4::transpose(m);
    assert_eq!(t[1], 4.0);
    assert_eq!(t[4], 1.0);
    assert_eq!(t[15], 15.0);
}

/// `E39_Step4990` (name ours): the inverse of a diagonal matrix is the reciprocal diagonal (to f32 rounding).
#[test]
fn inverse_of_a_diagonal_matrix_is_its_reciprocal() {
    let _pc24 = Precision::Pc24.scope();
    let mut m = [0.0f32; 16];
    for k in 0..4 {
        m[5 * k] = 2.0 + k as f32;
    }
    let inv = rbr_physics::engine_driveline::mat4::inverse(m);
    for k in 0..4 {
        assert!((inv[5 * k] - 1.0 / (2.0 + k as f32)).abs() < 1e-6);
    }
}

#[test]
fn init_real_lerp_builds_the_real() {
    let _pc24 = Precision::Pc24.scope();
    // The buffer is a CReal: the CBase(0) header (name 0, dirty 0) under the real vtable, and (b - a) * t + a at +0xc.
    let mut mem = rbr_physics::mem::Mem::new();
    mem.map(0x1000, &[0xff; 0x10]);
    let eax = layout::init_real_lerp(&mut mem, 0x1000, 2.0, 6.0, 0.25);
    assert_eq!(eax, 0x1000);
    assert_eq!(mem.u32(0x1000), 0x0073_b644);
    assert_eq!(mem.u32(0x1004), 0);
    assert_eq!(mem.u32(0x1008), 0);
    assert_eq!(mem.f32(0x100c), 3.0);
}

//! Replays the original's physics core math calls (`harness/vcu_vectors.py --set physics_core`) through the ports in
//! `rbr_physics::physics_core`. Needs record files: `RBR_PHYSICS_CORE_VECTORS=<file>[:<file>...]`. Without the
//! variable the tests are skipped.

mod common;

use std::collections::VecDeque;

use common::{
    check_window_list, check_windows, load, load_inner, map_window_list, map_windows, replay_eax, shared_windows,
    After, Call, Inner, Window,
};
use rbr_physics::fpu::Precision;
use rbr_physics::math::table;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::{
    self, construct, contact_walk, detect_calls, flags, material, mounted, mounted_body, mounted_ctor, mounted_pose,
    passenger_ctor, physics_object, setup, signal_emit,
};
use rbr_physics::physics_core::math::{self, Vec4};
use rbr_physics::physics_core::{debug, detect, pacing, passenger};

const VAR: &str = "RBR_PHYSICS_CORE_VECTORS";

const IDX_WRITE: u32 = 0x528980;
const VEC_ADD: u32 = 0x528b00;
const VEC_SUB: u32 = 0x528b90;
const MAT_MUL: u32 = 0x528c20;
const SCALE_ROWS: u32 = 0x528dd0;
const XFORM_ROWS: u32 = 0x528fa0;
const ACCUM_250: u32 = 0x529250;
const ACCUM_340: u32 = 0x529340;
const GET_1DC: u32 = 0x5293d0;
const GET_1EC: u32 = 0x5293e0;
const MB560: u32 = 0x51b560;
const CSTATE_CTOR: u32 = 0x5270c0;
const CSTATE_FACTORY: u32 = 0x5278d0;
const VF10: u32 = 0x524f80;
const CSCRATCH_CTOR: u32 = 0x524e80;
const CSCRATCH_VF04: u32 = 0x523c50;
const CTOR_SOLVER: u32 = 0x4fc930;
const SOLVER_S08: u32 = 0x4fc0b0;
const CDEF_VF54: u32 = 0x526760;
const CPHYSICS_CTOR: u32 = 0x527c60;
const CDEF_VF4C: u32 = 0x5284f0;
const OPERATOR_NEW: u32 = 0x70bec2;
const E37_PREFILL: u32 = 0x4f5340;
const GET_NAME_OR_NULL: u32 = 0x52f0b0;
const HOOK: u32 = 0x4a0150;
const CDEF_CTOR: u32 = 0x524c70;
const CREATE_DEF: u32 = 0x526660;
const STEP_FORWARD: u32 = 0x52f0c0;
const PHYS_UNK34: u32 = 0x523a20;
const INIT_PHYSICS: u32 = 0x5266d0;
const STRIP_COMMENTS: u32 = 0x52eba0;
const TOKENIZE: u32 = 0x52ec80;
const FREE_TOKENS: u32 = 0x52ee10;
const APP_INIT: u32 = 0x526fd0;
const CLEAR_WALL: u32 = 0x4e40d0;
const COLLISION_CLEAR: u32 = 0x4fad10;
const WATER_RESET: u32 = 0x4ffba0;
const VEC4_CLEAR: u32 = 0x465d80;
const INIT_TOKEN_SCRATCH: u32 = 0x52eb20;
const WALL_REL: u32 = 0x5276a0;
const UNLINK_A: u32 = 0x4e41a0;
const REG_WALL: u32 = 0x5263d0;
const E01_LOG: u32 = 0x43c8e0;
const EMIT_LOCAL_TIME: u32 = 0x526580;
/// CMultiSignal1<CSigCheckPointTime>::Emit: the checkpoint signal (g-emit records).
const EMIT_CHECKPOINT: u32 = 0x4A0390;
/// CMultiSignal1<CSigCarControl>::Emit: the car's control signal (g-emit records).
const EMIT_CONTROL: u32 = 0x48CD90;
const NOTIFY_PAD: u32 = 0x527b40;
const MATERIAL_CTOR: u32 = 0x520980;
const MATERIAL_TAG: u32 = 0x520940;
const NOTIFY_EXIT_GAME: u32 = 0x527b90;
const DEF_MANAGER_CTOR: u32 = 0x501d90;
const MATERIALS_CTOR: u32 = 0x520b30;
const SOLVER_DEF_CTOR: u32 = 0x4fb350;
const MOUNTED_DEF_CTOR: u32 = 0x521480;
const MOUNTED_NEW_DEF: u32 = 0x521b50;
const MOUNTED_STATE_CTOR: u32 = 0x5215a0;
const MOUNTED_VF4C: u32 = 0x521500;
const MOUNTED_BODY_CTOR: u32 = 0x521c30;
const MOUNTED_BODY_VF50: u32 = 0x521d20;
const SOLID_BASE_CTOR: u32 = 0x52d480;
const SCRATCH_BASE_INIT: u32 = 0x52b930;
const EXCLUDE_A000: u32 = 0x51a000;
const MOUNTED_POSE: u32 = 0x521980;
const CBASE_SET_DIRTY: u32 = 0x52f450;
const BODY_COMPOSE: u32 = 0x4e6a30;
const E01_VEC4_COPY_FROM: u32 = 0x43c5b0;
const PASSENGER_DEF_CTOR: u32 = 0x522450;
const PASSENGER_DEF_DEMOTE: u32 = 0x5224a0;
const PASSENGER_STATE_CTOR: u32 = 0x522550;
const PASSENGER_CTOR: u32 = 0x5230d0;
const CBASE_CTOR: u32 = 0x52f090;
const PASSENGER_SCRATCH_VF04: u32 = 0x5224e0;
const SOLID_DEF_CTOR_MOUNTED: u32 = 0x529840;
const NTWEAK_UNSIGNED: u32 = 0x52fec0;
const NTWEAK_REAL: u32 = 0x52fea0;
const NTWEAK_VECTOR: u32 = 0x52ff80;
const PASSENGER_FLAG: u32 = 0x5233F0;
const E37_GET_67C: u32 = 0x4F7E40;
const INCAR_SHAKE_EMIT: u32 = 0x523010;
const PASSENGER_SHAKE: u32 = 0x523210;
const CLEAR6: u32 = 0x49FEA0;
const PASSENGER_TIME: u32 = 0x523360;
const CRT_FTOL: u32 = 0x70C2C4;
const PASSENGER_SPRING: u32 = 0x5229E0;
const CELL_LOOKUP4: u32 = 0x5240C0;
const EDGE_SWEEP: u32 = 0x5246A0;
const LOOKUP_TWIN: u32 = 0x4E03B0;
const EDGE_EDGE: u32 = 0x4F8280;
const PENETRATION_CTOR: u32 = 0x5235B0;
const E38_RAY_SEEDS: u32 = 0x4F6550;
const GRID_BUILD: u32 = 0x523D60;
const QUERY_WALL: u32 = 0x4E4140;
const CELL_GRID_CTOR: u32 = 0x4E0550;
const WORK_5500: u32 = 0x525500;
/// The listener the incar shake's walk invokes besides the one the records list (its invokes are not recorded: it is not in
/// the set). The port's invokes to it are not compared.
const UNRECORDED_LISTENER: u32 = 0x7583CD0;
/// The host's lane-3 words (+0x2c and +0x7c, of the 0xa0-byte host window): the original writes them from stack words it
/// never sets (the asm, see `RBR_E45_PassengerB.cpp`'s M29E0), so they are not compared.
const LANE3_SKIP: [(usize, usize); 2] = [(0x2c, 4), (0x7c, 4)];

/// The spans of a passenger window that are not compared: the host's lane-3 words in its 0xa0-byte window.
fn host_skip(_: &Call, _: u32, len: usize) -> Vec<(usize, usize)> {
    if len == 0xa0 {
        LANE3_SKIP.to_vec()
    } else {
        Vec::new()
    }
}

#[test]
fn table_index_frac_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, IDX_WRITE, "E46_IdxWrite", false, |mem, a| {
        let table = layout::floats(mem, a[0], a[2] as usize);
        let hint = layout::table_index_hint(mem, a[3]);
        // the entries outside the table are NaN here (the original reads the words beside it; a NaN value does not
        // reach the result, see `physics_core/README.md` (git 9c28c0b8))
        let entry = |i: i32| usize::try_from(i).ok().and_then(|i| table.get(i)).copied().unwrap_or(f32::NAN);
        let out = table::table_index_frac(entry, table.len() as i32, f32::from_bits(a[1]), hint);
        layout::set_table_index(mem, a[3], out);
        0
    });
}

#[test]
fn vec4_add_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, VEC_ADD, "E46Vec::Add", true, |mem, a| {
        let this = Vec4::load(mem, a[0]);
        let mut dst = layout::vec4s(mem, a[1], a[2] as usize);
        for row in &mut dst {
            *row = *row + this;
        }
        layout::set_vec4s(mem, a[1], &dst);
        a[1]
    });
}

/// `E46_Offset540` (0x529540): direct records (`harness/vector_specs/g_v1stops.py`); out's fourth lane is not written.
#[test]
fn offset_540_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, 0x529540, "E46_Offset540", false, |mem, a| {
        layout::offset_540(mem, a[0], a[1], a[2]);
        0
    });
}

/// `CMountedRigidBody::Vf6c`'s emits, replayed from the record in order: the signal, the payload's address and the
/// payload words the port writes must be the original's (the spans in `written`: the rest is the stack's). The
/// listeners' writes to the payload are the record's, and so is each emit's eax.
struct Vf6cReplay<'a> {
    mem: Mem,
    index: usize,
    inner: &'a [Inner],
    next: usize,
}

impl Vf6cReplay<'_> {
    fn take(&mut self, addr: u32, signal: u32, sig: u32, written: &[(u32, usize)]) -> u32 {
        let call = &self.inner[self.next];
        let name = format!("Vf6c call {} inner {}", self.index, self.next);
        assert_eq!((call.addr, &call.args[..]), (addr, &[signal, sig][..]), "{name}");
        let Window { index: _, at, len: _, before: pre, after: post } = &call.windows[0];
        for &(from, len) in written {
            let span = from as usize..from as usize + len;
            assert_eq!(self.mem.read(at + from, len), pre[span], "{name}: payload +{from:#x}");
        }
        self.mem.map(*at, post);
        self.next += 1;
        call.eax
    }
}

impl layout::MountedVf6cHost for Vf6cReplay<'_> {
    /// `E43Vf6C`'s record is the last: the records before it are its own inner calls (they return first).
    fn solid_vf6c(&mut self, _: u32, body: u32, dt: f32) -> u32 {
        let call = self.inner.last().expect("Vf6c: no E43Vf6C record");
        assert_eq!((call.addr, &call.args[..]), (0x52cc30, &[body, dt.to_bits()][..]));
        self.mem.map(call.windows[0].at, &call.windows[0].after);
        self.next = self.inner.len();
        call.eax
    }

    fn emit_event(&mut self, _: u32, signal: u32, sig: u32) -> u32 {
        self.take(0x51f7c0, signal, sig, &[(0, 8), (0x10, 0x40)])
    }

    fn emit_state(&mut self, _: u32, signal: u32, sig: u32) -> u32 {
        self.take(0x51f700, signal, sig, &[(0, 4), (0x10, 0x20)])
    }
}

/// `CMountedRigidBody::Vf6c` (0x521d90): direct records on a mounted body of v1-tarmac-zr 6120 and on the unowned one of
/// v1-gravel 6120 (`harness/vector_specs/g_mounted.py`). The frame is the state payload's address less 0x20.
#[test]
fn mounted_vf6c_matches() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, 0x521d90) else {
        eprintln!("skipping CMountedRigidBody::Vf6c: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = shared_windows(call);
        let frame = call.inner.last().map_or(0, |state| state.args[1].wrapping_sub(0x20));
        let mut host = Vf6cReplay { mem: mem.clone(), index: call.index, inner: &call.inner, next: 0 };
        let dt = f32::from_bits(call.args[1]);
        let eax = layout::mounted_vf6c(&mut mem, &mut host, call.args[0], dt, frame);
        assert_eq!(host.next, call.inner.len(), "Vf6c call {}: emits", call.index);
        assert_eq!(eax, call.eax, "Vf6c call {}: eax", call.index);
        check_windows(call, &mem, "CMountedRigidBody::Vf6c");
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("CMountedRigidBody::Vf6c: {} calls match, by branch: {branches:?}", calls.len());
}

#[test]
fn vec4_sub_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, VEC_SUB, "E46Vec::Sub", true, |mem, a| {
        let this = Vec4::load(mem, a[0]);
        let mut dst = layout::vec4s(mem, a[1], a[2] as usize);
        for row in &mut dst {
            *row = *row - this;
        }
        layout::set_vec4s(mem, a[1], &dst);
        a[1]
    });
}

/// `E46Mat::Mul` is called only from `E47SolidState::Vf17x`, so its records are the inner calls of that function.
#[test]
fn mat_mul_matches() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load_inner(VAR, MAT_MUL) else {
        eprintln!("skipping E46Mat::Mul: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "E46Mat::Mul: no inner calls in the vector files");
    for call in &calls {
        let mut mem = map_window_list(&call.windows);
        let a = &call.args;
        layout::mat_mul(&mut mem, a[0], a[1], a[2]);
        assert_eq!(a[1], call.eax, "E46Mat::Mul: eax");
        check_window_list(&call.windows, &mem, "E46Mat::Mul");
    }
    eprintln!("E46Mat::Mul: {} inner calls match", calls.len());
}

#[test]
fn mat_scale_rows_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, SCALE_ROWS, "E46Mat::ScaleRows", true, |mem, a| {
        let m = layout::mat(mem, a[0]);
        let mut rows = layout::vec4s(mem, a[1], a[2] as usize);
        m.scale_rows(&mut rows);
        layout::set_vec4s(mem, a[1], &rows);
        a[1]
    });
}

#[test]
fn mat_xform_rows_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, XFORM_ROWS, "E46Mat::XformRows", false, |mem, a| {
        let m = layout::mat(mem, a[0]);
        let mut rows = layout::vec4s(mem, a[1], a[2] as usize);
        m.transform_xyz(&mut rows);
        layout::set_vec4s(mem, a[1], &rows);
        0
    });
}

/// A window's bytes as a memory of their own (the post state of an inner call's window, e.g. a walk's output).
fn post_mem(window: &common::Window) -> Mem {
    let Window { index: _, at, len: _, before: _, after: post } = window;
    let mut mem = Mem::new();
    mem.map(*at, post);
    mem
}

#[test]
fn accumulate_force_matches() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load_inner(VAR, ACCUM_340) else {
        eprintln!("skipping E46_Accum340: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "E46_Accum340: no inner calls in the vector files");
    for call in &calls {
        let mut mem = map_window_list(&call.windows);
        let body = call.args[0];
        let v = Vec4::load(&mem, call.args[1]);
        let force = layout::force_accum(&mem, body);
        let mut force = force;
        math::accumulate_force(&mut force, v, layout::force_scale(&mem, body));
        layout::set_force_accum(&mut mem, body, force);
        check_window_list(&call.windows, &mem, "E46_Accum340");
    }
    eprintln!("E46_Accum340: {} inner calls match", calls.len());
}

/// `E46_Accum250` makes its walk (`MB560`) and its accumulate (`Vf70`, Accum340) inside it: the port's
/// `gravity_force`, on the walk's output, must give the vector the original passed to the accumulate, or no accumulate
/// when the inverse mass is 0.
#[test]
fn gravity_force_matches() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, ACCUM_250) else {
        eprintln!("skipping E46_Accum250: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "E46_Accum250: no calls in the vector files");
    let mut accumulated = 0;
    for call in &calls {
        let mem = map_windows(call);
        let body = call.args[0];
        let walk: Option<&Inner> = call.inner.iter().find(|i| i.addr == MB560);
        let accum: Option<&Inner> = call.inner.iter().find(|i| i.addr == ACCUM_340);
        let inverse_mass = layout::inverse_mass(&mem, body);
        let walked = walk.map(|w| {
            let out = w.windows.iter().find(|win| win.index == 1).expect("MB560 out window");
            Vec4::load(&post_mem(out), out.at)
        });
        // The walk and the accumulate come together or not at all.
        assert_eq!(walked.is_some(), accum.is_some(), "E46_Accum250 call {}: walk and accumulate", call.index);
        match (walked, accum) {
            (Some(walked), Some(accum)) => {
                let got = math::gravity_force(walked, inverse_mass).expect("inverse mass is not 0 when the walk ran");
                let v = accum.windows.iter().find(|win| win.index == 1).expect("Vf70 v window");
                let want = Vec4::load(&post_mem(v), v.at);
                assert_eq!(bits(got), bits(want), "E46_Accum250 call {}: force", call.index);
                accumulated += 1;
            }
            _ => assert!(
                math::gravity_force(Vec4::default(), inverse_mass).is_none(),
                "E46_Accum250 call {}: gate",
                call.index
            ),
        }
    }
    eprintln!("E46_Accum250: {} calls match ({accumulated} accumulate)", calls.len());
}

fn bits(v: Vec4) -> [u32; 4] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits(), v.w.to_bits()]
}

#[test]
fn solid_getters_match() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, GET_1DC, "CSolid::E43Vf80", true, |mem, a| layout::cache_1dc(mem, a[0]));
    replay_eax(VAR, GET_1EC, "CSolid::E43Vf8C", true, |mem, a| layout::cache_1ec(mem, a[0]));
}

/// `CPhysics::CState::CState` (load time: from the entry dump's record, `RBR_PHYSICS_CORE_VECTORS`).
#[test]
fn state_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_eax(VAR, CSTATE_CTOR, "CPhysics::CState::CState", false, |mem, a| {
        layout::store_state(mem, a[0], &construct::new_state(a[1]));
        0
    });
}

/// `CPhysics::CDef::Vf50` (0x5278d0): allocates the state (0x4b8 bytes) and constructs it. The allocation is the
/// caller's; the port builds the image at the returned address. The `after` record is the object's bytes after the
/// return, used as the start state (the entry dump has no bytes of the new object before the call).
/// `CPhysics::Vf10` (load time). The port is the idle path: a call with a debug tool flag set is counted, not checked.
#[test]
fn debug_hook_idle_matches() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VF10) else {
        eprintln!("skipping CPhysics::Vf10: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "CPhysics::Vf10: no calls in the vector files");
    let (mut idle, mut tools) = (0, 0);
    for call in &calls {
        // window 1 is the CDef's DbgFlags word (see the spec's nodes)
        let flags = u32::from_le_bytes(call.windows[1].before[..4].try_into().unwrap());
        if debug::is_idle(flags) {
            let mem = map_windows(call);
            check_windows(call, &mem, "CPhysics::Vf10");
            idle += 1;
        } else {
            tools += 1;
        }
    }
    eprintln!("CPhysics::Vf10: {idle} idle calls match, {tools} tool calls not ported");
}

/// The calls a port makes into the caller's code, in call order, as (address, args) like `Inner`. Allocations return
/// the recorded blocks (mapped as zeros); `E21Stub::Hook` returns its recorded result. The signal links are not calls,
/// so the mock does nothing for them.
#[derive(Default)]
struct Calls {
    blocks: VecDeque<u32>,
    hook: u32,
    phys_unk34: u32,
    /// The name `GetNameOrNull` returns in the recorded body (the mock's answer to the body's name query).
    name: u32,
    /// The answer of the passenger's external flag test (`E37Get67C`).
    flag: u32,
    calls: Vec<(u32, Vec<u32>)>,
}

impl Calls {
    /// The mock for a recorded call: its allocations return the recorded blocks, and Hook its recorded result.
    fn for_call(call: &Call) -> Self {
        let calls = port_calls(call);
        Calls {
            blocks: calls.iter().filter(|i| i.addr == OPERATOR_NEW).map(|i| i.eax).collect(),
            hook: calls.iter().find(|i| i.addr == HOOK).map_or(0, |i| i.eax),
            phys_unk34: calls.iter().find(|i| i.addr == PHYS_UNK34).map_or(0, |i| i.eax),
            name: calls.iter().find(|i| i.addr == GET_NAME_OR_NULL).map_or(0, |i| i.eax),
            flag: calls.iter().find(|i| i.addr == E37_GET_67C).map_or(0, |i| i.eax),
            calls: Vec::new(),
        }
    }
}

impl construct::Heap for Calls {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        self.calls.push((OPERATOR_NEW, vec![size]));
        let block = self.blocks.pop_front().expect("an allocation the original did not make");
        if block != 0 {
            mem.map(block, &vec![0; size as usize]);
        }
        block
    }
}

impl construct::ScratchHost for Calls {
    fn ctor_solver(&mut self, _: &mut Mem, solver: u32, def: u32) {
        self.calls.push((CTOR_SOLVER, vec![solver, def]));
    }

    fn solver_s08(&mut self, _: &mut Mem, solver: u32) {
        self.calls.push((SOLVER_S08, vec![solver]));
    }
}

impl setup::CreateHost for Calls {
    fn step_forward(&mut self, _: &mut Mem, def: u32, arg: u32) {
        self.calls.push((STEP_FORWARD, vec![def, arg]));
    }

    fn phys_unk34(&mut self, _: &mut Mem, def: u32) -> u32 {
        self.calls.push((PHYS_UNK34, vec![def]));
        self.phys_unk34
    }
}

impl setup::TokenHost for Calls {
    fn strip_comments(&mut self, _: &mut Mem, script: u32) {
        self.calls.push((STRIP_COMMENTS, vec![script]));
    }

    fn tokenize(&mut self, _: &mut Mem, cursor: u32, script: u32) {
        self.calls.push((TOKENIZE, vec![cursor, script, 0]));
    }

    fn free_tokens(&mut self, _: &mut Mem, cursor: u32) {
        self.calls.push((FREE_TOKENS, vec![cursor]));
    }
}

impl setup::AppHost for Calls {
    fn clear_walls(&mut self, _: &mut Mem, wall_manager: u32) {
        self.calls.push((CLEAR_WALL, vec![wall_manager]));
    }

    fn collision_clear(&mut self, _: &mut Mem, value: u32) {
        self.calls.push((COLLISION_CLEAR, vec![value]));
    }

    fn water_reset(&mut self, _: &mut Mem, table: u32) {
        self.calls.push((WATER_RESET, vec![table]));
    }

    fn vec4_clear(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((VEC4_CLEAR, vec![at]));
    }

    fn init_token_scratch(&mut self, _: &mut Mem) {
        self.calls.push((INIT_TOKEN_SCRATCH, Vec::new()));
    }

    /// E37SigInit800 is a tail call of AppInit: the recorder keeps it as the file's own top-level record, not as an
    /// inner call, so the mock does not record it here.
    fn sig_init(&mut self, _: &mut Mem) {}
}

impl setup::WallHost for Calls {
    fn unlink(&mut self, _: &mut Mem, link: u32) {
        self.calls.push((UNLINK_A, vec![link]));
    }

    fn reg_wall(&mut self, _: &mut Mem, state: u32, wall: u32) {
        self.calls.push((REG_WALL, vec![state, wall]));
    }
}

impl signal_emit::SignalHost for Calls {
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32) {
        let target = mem.u32(mem.u32(conn) + 4);
        self.calls.push((target, vec![conn, sig]));
    }

    fn log_recursive(&mut self) {
        self.calls.push((E01_LOG, vec![6, 0]));
    }

    fn log_range(&mut self, count: u32) {
        self.calls.push((E01_LOG, vec![7, 0, count]));
    }
}

impl signal_emit::OwnerHost for Calls {
    fn call_owner(&mut self, _: &mut Mem, owner: u32, sig: u32) {
        self.calls.push((owner, vec![sig]));
    }
}

impl physics_object::PhysicsHost for Calls {
    fn prefill_pool(&mut self) {
        self.calls.push((E37_PREFILL, Vec::new()));
    }

    fn name_or_null(&mut self, def: u32) {
        self.calls.push((GET_NAME_OR_NULL, vec![def]));
    }

    fn hook(&mut self, _: &mut Mem, object: u32) -> u32 {
        self.calls.push((HOOK, vec![object]));
        self.hook
    }

    fn link(&mut self, _: &mut Mem, _: u32, _: u32) {}
}

impl passenger_ctor::ShakeHost for Calls {
    fn flag_67c(&mut self, _: &mut Mem, obj: u32) -> u32 {
        self.calls.push((E37_GET_67C, vec![obj]));
        self.flag
    }

    /// `Clear6` clears its six vectors (96 bytes).
    /// `Clear6` clears the six vectors at +0x10 (`E09Vec4::Clear`, four words each; the record lists them before Clear6).
    fn clear6(&mut self, mem: &mut Mem, at: u32) {
        for k in 0..6 {
            let vector = at + 0x10 + 16 * k;
            self.calls.push((VEC4_CLEAR, vec![vector]));
            for word in 0..4 {
                mem.set_u32(vector + 4 * word, 0);
            }
        }
        self.calls.push((CLEAR6, vec![at]));
    }
}

impl passenger_ctor::PassengerHost for Calls {
    fn base(&mut self, _: &mut Mem, at: u32, name: u32) {
        self.calls.push((CBASE_CTOR, vec![at, name]));
    }

    fn unsigned(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((NTWEAK_UNSIGNED, vec![at]));
    }

    fn real(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((NTWEAK_REAL, vec![at]));
    }

    fn clear(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((VEC4_CLEAR, vec![at]));
    }

    fn scratch_vf04(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((PASSENGER_SCRATCH_VF04, vec![at]));
    }
}

impl mounted_pose::PoseHost for Calls {
    fn set_dirty(&mut self, _: &mut Mem, base: u32) {
        self.calls.push((CBASE_SET_DIRTY, vec![base]));
    }

    fn body_compose(&mut self, _: &mut Mem, owner: u32, a: u32, b: u32) {
        self.calls.push((BODY_COMPOSE, vec![owner, a, b]));
    }

    fn copy_from(&mut self, _: &mut Mem, to: u32, src: u32) {
        self.calls.push((E01_VEC4_COPY_FROM, vec![to, src]));
    }
}

impl mounted_body::BodyHost for Calls {
    fn solid_base(&mut self, _: &mut Mem, at: u32, def: u32, a: u32, b: u32, flags: u32) {
        self.calls.push((SOLID_BASE_CTOR, vec![at, def, a, b, flags]));
    }

    fn base_name(&mut self, _: &mut Mem, base: u32) -> u32 {
        self.calls.push((GET_NAME_OR_NULL, vec![base]));
        self.name
    }

    fn state_ctor(&mut self, _: &mut Mem, at: u32, this: u32, name: u32, c: u32) {
        self.calls.push((MOUNTED_STATE_CTOR, vec![at, this, name, c]));
    }

    fn scratch_base(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((SCRATCH_BASE_INIT, vec![at]));
    }

    fn exclusion(&mut self, _: &mut Mem, at: u32, n: u32) {
        self.calls.push((EXCLUDE_A000, vec![at, n]));
    }
}

impl construct::CDefHost for Calls {
    fn def_manager(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((DEF_MANAGER_CTOR, vec![at]));
    }

    fn materials(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((MATERIALS_CTOR, vec![at]));
    }

    fn solver_def(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((SOLVER_DEF_CTOR, vec![at]));
    }
}

impl mounted_ctor::MountedHost for Calls {
    fn solid_def(&mut self, _: &mut Mem, at: u32, name: u32) {
        self.calls.push((SOLID_DEF_CTOR_MOUNTED, vec![at, name]));
    }

    fn unsigned(&mut self, _: &mut Mem, at: u32) {
        self.calls.push((NTWEAK_UNSIGNED, vec![at]));
    }
}

/// The calls the port makes, in order: the recorded inner calls without the allocation `E21Stub::Hook` makes inside
/// itself (the record just before the hook's own: inner records are kept as the callee returns).
fn port_calls(call: &Call) -> Vec<&Inner> {
    let mut calls: Vec<&Inner> = call.inner.iter().collect();
    if let Some(hook) = calls.iter().position(|i| i.addr == HOOK) {
        assert_eq!(calls[hook - 1].addr, OPERATOR_NEW, "an allocation inside the hook");
        calls.remove(hook - 1);
    }
    // The allocations right before a Tokenize or InitTokenScratch record are that callee's own buffers.
    let mut buffers = Vec::new();
    for k in 0..calls.len() {
        if calls[k].addr == TOKENIZE || calls[k].addr == INIT_TOKEN_SCRATCH {
            buffers.extend((0..k).rev().take_while(|&j| calls[j].addr == OPERATOR_NEW));
        }
    }
    let mut k = 0;
    calls.retain(|_| {
        k += 1;
        !buffers.contains(&(k - 1))
    });
    // The body's constructor (and Vf50, which runs it on its block) makes the calls of its solid base before its record,
    // and the calls of the state, scratch and exclusion constructors flattened into their records: those are not the
    // body's. The name query stays (its record is the body's own, after the solid base).
    if call.addr == MOUNTED_BODY_CTOR || call.addr == MOUNTED_BODY_VF50 {
        let solid = calls.iter().position(|i| i.addr == SOLID_BASE_CTOR).expect("the solid base");
        let start = calls[..solid].iter().rposition(|i| i.addr == OPERATOR_NEW).map_or(0, |alloc| alloc + 1);
        calls.drain(start..solid);
        let mut k = 0;
        calls.retain(|i| {
            k += 1;
            k <= start || matches!(i.addr, SOLID_BASE_CTOR | GET_NAME_OR_NULL | SCRATCH_BASE_INIT | EXCLUDE_A000)
        });
    }
    // E45_M1980's owner call (CCar::BodyCompose) makes a vector copy of its own before its record: flattened into this
    // record, it is the owner's, not the pose update's.
    if call.addr == MOUNTED_POSE {
        let dirty = calls.iter().position(|i| i.addr == CBASE_SET_DIRTY).expect("SetDirty");
        let compose = calls.iter().position(|i| i.addr == BODY_COMPOSE).expect("BodyCompose");
        calls.drain(dirty + 1..compose);
    }
    // The def's StepForward (its last record) makes calls on its sub-objects after the port's last direct call before
    // it (the solver's constructor, or the script's Tokenize on a later step): those records are the caller's.
    if let Some(step) = calls.iter().rposition(|i| i.addr == STEP_FORWARD) {
        let anchor = calls[..step]
            .iter()
            .rposition(|i| i.addr == SOLVER_DEF_CTOR)
            .unwrap_or_else(|| calls.iter().position(|i| i.addr == TOKENIZE).expect("Tokenize"));
        calls.drain(anchor + 1..step);
    }
    // The CDef's constructor ends with the solver's CDef constructor (the caller's). The calls that constructor makes
    // (E09Vec4::Clear, in some runs) are recorded flattened after its record: they are the caller's too.
    // The solid base's constructor makes calls of its own before its record (flattened into the def's record, after the
    // allocation of `NewDef`), and the vector's constructor clears its vector (E09Vec4::Clear) before its record: the
    // caller's too.
    if call.addr == MOUNTED_DEF_CTOR || call.addr == MOUNTED_NEW_DEF {
        let solid = calls.iter().position(|i| i.addr == SOLID_DEF_CTOR_MOUNTED).expect("the solid base");
        let start = calls[..solid].iter().rposition(|i| i.addr == OPERATOR_NEW).map_or(0, |alloc| alloc + 1);
        calls.drain(start..solid);
        calls.retain(|i| i.addr != VEC4_CLEAR);
    }
    if call.addr == CDEF_CTOR {
        if let Some(solver) = calls.iter().position(|i| i.addr == SOLVER_DEF_CTOR) {
            calls.truncate(solver + 1);
        }
    }
    calls
}

/// Checks the bytes at `at` (`post.len()` of them) against the original's, except the `skip` spans (offset, length)
/// that the caller's code writes.
fn check_except(mem: &Mem, at: u32, post: &[u8], skip: &[(usize, usize)], label: &str) {
    let got = mem.read(at, post.len());
    let mut want = post.to_vec();
    for &(off, len) in skip {
        want[off..off + len].copy_from_slice(&got[off..off + len]);
    }
    assert_eq!(got, want, "{label}");
}

/// The solver's bytes: written by the solver's constructor and `S08` (the caller's).
const SOLVER_SKIP: [(usize, usize); 1] = [(construct::SOLVER_AT as usize, construct::SOLVER_SIZE as usize)];
/// The `CDef`'s externals: the def manager (+0x0c, 0xc bytes), the material manager (+0x18, 0x1600c bytes) and the
/// solver's def (+0x16024, 0x4c bytes).
const CDEF_SKIP: [(usize, usize); 3] = [(0x0c, 0x0c), (0x18, 0x1600c), (0x16024, 0x4c)];
/// The link words of the five linked connections (next, prev, list at +4..+0x10 of each).
const LINK_SKIP: [(usize, usize); 5] = [(0x68, 12), (0x80, 12), (0x98, 12), (0xb0, 12), (0xc8, 12)];

/// Replays the recorded calls of `addr`: `port` runs with the mock of the call (`Calls::for_call`), the calls the port
/// makes must be the original's in order, and the windows and the `after` runs must match except the `skip` spans.
fn replay_calls(addr: u32, name: &str, skip: &[(usize, usize)], port: impl Fn(&mut Mem, &Call, &mut Calls)) {
    replay_calls_with(addr, name, |_, _, _| skip.to_vec(), port);
}

/// `replay_calls` with the skipped spans chosen per window: `skip(call, window address, window length)`.
fn replay_calls_with(
    addr: u32,
    name: &str,
    skip: impl Fn(&Call, u32, usize) -> Vec<(usize, usize)>,
    port: impl Fn(&mut Mem, &Call, &mut Calls),
) {
    replay_calls_expect(addr, name, skip, port_pairs, port);
}

/// The calls `port_calls` keeps, as (address, args).
fn port_pairs(call: &Call) -> Vec<(u32, Vec<u32>)> {
    port_calls(call).iter().map(|i| (i.addr, i.args.clone())).collect()
}

/// The calls `port_calls` keeps, without the `CVector` and `CReal` constructors: the mounted constructors make those
/// inline (`ntweak`), so their bytes are compared in the windows, not as calls.
fn port_pairs_inline(call: &Call) -> Vec<(u32, Vec<u32>)> {
    port_pairs(call).into_iter().filter(|(addr, _)| *addr != NTWEAK_VECTOR && *addr != NTWEAK_REAL).collect()
}

/// `replay_calls` for the mounted constructors, whose tweak constructors are inline (see `port_pairs_inline`).
fn replay_mounted(addr: u32, name: &str, skip: &[(usize, usize)], port: impl Fn(&mut Mem, &Call, &mut Calls)) {
    replay_calls_expect(addr, name, |_, _, _| skip.to_vec(), port_pairs_inline, port);
}

/// `replay_calls_with` with the expected calls chosen by `expected(call)` (for a listed function whose records hide some
/// of its calls: see `body_expected`).
fn replay_calls_expect(
    addr: u32,
    name: &str,
    skip: impl Fn(&Call, u32, usize) -> Vec<(usize, usize)>,
    expected: impl Fn(&Call) -> Vec<(u32, Vec<u32>)>,
    port: impl Fn(&mut Mem, &Call, &mut Calls),
) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    for call in &calls {
        let mut mem = map_windows(call);
        let mut host = Calls::for_call(call);
        port(&mut mem, call, &mut host);
        assert_eq!(host.calls, expected(call), "{name} call {}: calls", call.index);
        for Window { index: k, at, len, before: _, after: post } in &call.windows {
            let label = format!("{name} call {} window {k}", call.index);
            check_except(&mem, *at, post, &skip(call, *at, *len), &label);
        }
        for After { at, len: _, bytes } in &call.after {
            let label = format!("{name} call {} after {at:#x}", call.index);
            check_except(&mem, *at, bytes, &skip(call, *at, bytes.len()), &label);
        }
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn scratch_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CSCRATCH_CTOR, "CPhysics::CScratch::CScratch", &SOLVER_SKIP, |mem, call, host| {
        construct::construct_scratch(mem, host, call.args[0], call.args[1]);
    });
}

#[test]
fn scratch_vf04_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CSCRATCH_VF04, "CPhysics::CScratch::Vf04", &SOLVER_SKIP, |mem, call, host| {
        construct::scratch_vf04(mem, host, call.args[0]);
    });
}

#[test]
fn state_factory_matches() {
    let _pc24 = Precision::Pc24.scope();
    // Vf50(owner, name): the state's argument is the owner (args[1]), the name is not used.
    replay_calls(CSTATE_FACTORY, "CPhysics::CDef::Vf50", &[], |mem, call, host| {
        assert_eq!(construct::cdef_new_state(mem, host, call.args[1]), call.eax);
    });
}

#[test]
fn cdef_new_scratch_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CDEF_VF54, "CPhysics::CDef::Vf54", &SOLVER_SKIP, |mem, call, host| {
        assert_eq!(construct::cdef_new_scratch(mem, host, call.args[0]), call.eax);
    });
}

#[test]
fn physics_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CPHYSICS_CTOR, "CPhysics::CPhysics", &LINK_SKIP, |mem, call, host| {
        physics_object::construct_physics(mem, host, call.args[0], call.args[1]);
    });
}

#[test]
fn cdef_new_physics_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CDEF_VF4C, "CPhysics::CDef::Vf4c", &LINK_SKIP, |mem, call, host| {
        assert_eq!(physics_object::cdef_new_physics(mem, host, call.args[0]), call.eax);
    });
}

#[test]
fn cdef_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CDEF_CTOR, "CPhysics::CDef::CDef", &CDEF_SKIP, |mem, call, host| {
        construct::construct_cdef(mem, host, call.args[0]);
    });
}

#[test]
fn create_def_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(CREATE_DEF, "E46_CreateDef", &[], |mem, call, host| {
        assert_eq!(setup::create_def(mem, host, call.args[0]), call.eax);
    });
}

#[test]
fn init_physics_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(INIT_PHYSICS, "E46_InitPhysics", &[], |mem, call, host| {
        // The token cursor is the stack buffer the original passes to Tokenize (its first argument).
        let cursor = call.inner.iter().find(|i| i.addr == TOKENIZE).expect("Tokenize").args[0];
        setup::init_physics(mem, host, call.args[0], cursor);
    });
}

#[test]
fn app_init_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(APP_INIT, "E46_AppInit", &[], |mem, _, host| {
        setup::app_init(mem, host);
    });
}

#[test]
fn wall_rel_matches() {
    let _pc24 = Precision::Pc24.scope();
    // With a target, the target's count (window 2) is lowered by the port, then RegWall (the caller's) adds the relay
    // back to the same list: the port's own result is checked here, the count one less than the original's before it.
    // Without a target (the fuzzed early return) the record has no count window and no calls.
    replay_calls_with(
        WALL_REL,
        "E43HostRel::WallRel",
        |call, at, len| {
            if call.windows.get(2).is_some_and(|w| w.at == at) {
                vec![(0, len)]
            } else {
                Vec::new()
            }
        },
        |mem, call, host| {
            setup::wall_rel(mem, host, call.args[0], call.args[1]);
            if let Some(Window { at: count_at, before: pre, .. }) = call.windows.get(2) {
                let before = u32::from_le_bytes(pre[..4].try_into().unwrap());
                assert_eq!(mem.u32(*count_at), before - 1, "E43HostRel::WallRel: the count");
            }
        },
    );
}

#[test]
fn emit_local_time_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(EMIT_LOCAL_TIME, "CMultiSignal1<CSigLocalTime>::Emit", &[], |mem, call, host| {
        signal_emit::emit(mem, host, call.args[0], call.args[1]);
    });
}

/// `E45_P33F0` (0x5233f0): the flag test, with its `E37Get67C` external.
#[test]
fn passenger_flag_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(PASSENGER_FLAG, "E45_P33F0", &[], |mem, call, host| {
        let set = passenger_ctor::flag_67c_set(mem, host, call.args[0]);
        assert_eq!(set as u32, call.eax, "E45_P33F0 call {}: eax", call.index);
    });
}

/// CMultiSignal1<CSigCheckPointTime>::Emit (0x4a0390, direct records from g-emit): the same walk as the local time's emit,
/// over the table's slot for the count, the secondary list and the receivers (`ret 4`), in the original's order.
#[test]
fn emit_checkpoint_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(EMIT_CHECKPOINT, "CMultiSignal1<CSigCheckPointTime>::Emit", &[], |mem, call, host| {
        signal_emit::emit(mem, host, call.args[0], call.args[1]);
    });
}

/// CMultiSignal1<CSigCarControl>::Emit (0x48cd90, direct records from g-emit): the same template, with the guard set on
/// some calls (the recursion's log, then the return). Skipped for a store without them.
#[test]
fn emit_control_matches() {
    let _pc24 = Precision::Pc24.scope();
    if load(VAR, EMIT_CONTROL).is_some_and(|c| c.is_empty()) {
        eprintln!("skipping CMultiSignal1<CSigCarControl>::Emit: no calls ({VAR})");
        return;
    }
    replay_calls(EMIT_CONTROL, "CMultiSignal1<CSigCarControl>::Emit", &[], |mem, call, host| {
        signal_emit::emit(mem, host, call.args[0], call.args[1]);
    });
}

/// `CIncarShake::Emit` (0x523010): the same walk as `CSigLocalTime`'s Emit, on the incar shake signal.
#[test]
fn incar_shake_emit_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(INCAR_SHAKE_EMIT, "CIncarShake::Emit", &[], |mem, call, host| {
        signal_emit::emit(mem, host, call.args[0], call.args[1]);
        host.calls.retain(|(addr, _)| *addr != UNRECORDED_LISTENER);
    });
}

/// `E45_P3210` (0x523210): its calls (`Clear6` with the six vectors it clears, then the Emit's invokes, which the record
/// lists under the shake) and the payload it fills, which is the Emit's window 2 (each shake emits once: the two lists
/// have the same length).
#[test]
fn passenger_shake_matches() {
    let _pc24 = Precision::Pc24.scope();
    let (Some(shakes), Some(emits)) = (load(VAR, PASSENGER_SHAKE), load(VAR, INCAR_SHAKE_EMIT)) else {
        eprintln!("skipping E45_P3210: {VAR} not set");
        return;
    };
    assert_eq!(shakes.len(), emits.len(), "one Emit per shake");
    for (call, emit) in shakes.iter().zip(&emits) {
        let Window { index: _, at: pay, len, before: payload, after: _ } =
            emit.windows.get(2).expect("the payload window");
        assert_eq!(*len, 0x64);
        let mut mem = map_windows(emit);
        // `Clear6` also zeroes the words past the payload (up to +0x70 of the payload's address).
        mem.map(*pay + *len as u32, &[0; 0x20]);
        for Window { index: _, at, len: _, before: pre, after: _ } in &call.windows {
            mem.map(*at, pre);
        }
        let mut host = Calls::for_call(call);
        passenger_ctor::incar_shake(&mut mem, &mut host, call.args[0], *pay);
        host.calls.retain(|(addr, _)| *addr != UNRECORDED_LISTENER);
        assert_eq!(host.calls, port_pairs(call), "E45_P3210 call {}: calls", call.index);
        assert_eq!(mem.read(*pay, *len), *payload, "E45_P3210 call {}: the payload", call.index);
        check_windows(call, &mem, "E45_P3210");
    }
    eprintln!("E45_P3210: {} calls match", shakes.len());
}

/// `E45PSmalls::M3360` (0x523360): the time decode of the double argument (its `_ftol2` is `math::ftol::ftol2`).
#[test]
fn decode_time_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls_expect(
        PASSENGER_TIME,
        "E45PSmalls::M3360",
        |_, _, _| Vec::new(),
        |call| {
            // The CRT's `_ftol2` is a pure call now (`math::ftol::ftol2`), so the record's call to it is not expected.
            port_pairs(call).into_iter().filter(|(addr, _)| *addr != CRT_FTOL).collect()
        },
        |mem, call, _| {
            let v = f64::from_bits(((call.args[2] as u64) << 32) | call.args[1] as u64);
            passenger_ctor::decode_time(mem, call.args[0], v);
        },
    );
}

/// `E45PSmalls::M29E0` (0x5229e0): the spring.
#[test]
fn passenger_spring_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls_with(PASSENGER_SPRING, "E45PSmalls::M29E0", host_skip, |mem, call, _| {
        passenger_ctor::spring(mem, call.args[0], f32::from_bits(call.args[1]));
    });
}

/// Replays the recorded calls of `addr`, checking the value `port` returns against the original's eax.
fn replay_value(addr: u32, name: &str, port: impl Fn(&Mem, &Call) -> u32) {
    replay_calls(addr, name, &[], |mem, call, _| {
        assert_eq!(port(mem, call), call.eax, "{name} call {}: eax", call.index);
    });
}

#[test]
fn flag_names_match() {
    let _pc24 = Precision::Pc24.scope();
    replay_value(0x523B90, "CDef::CFlags::GetBitName", |mem, c| flags::bit_name(mem, flags::CDEF_NAMES, c.args[1]));
    replay_value(0x523BC0, "CDef::CDbgFlags::GetBitName", |mem, c| flags::bit_name(mem, flags::DBG_NAMES, c.args[1]));
    replay_value(0x523BF0, "CDef::CLogFlags::GetBitName", |mem, c| flags::bit_name(mem, flags::LOG_NAMES, c.args[1]));
    replay_value(0x520840, "CMaterial::CFlags::GetBitName", |mem, c| {
        flags::bit_name(mem, flags::MATERIAL_NAMES, c.args[1])
    });
}

#[test]
fn flag_details_match() {
    let _pc24 = Precision::Pc24.scope();
    replay_value(0x523BA0, "CDef::CFlags::GetBitDetail", |mem, c| {
        flags::bit_detail(mem, flags::CDEF_DETAILS, c.args[1])
    });
    replay_value(0x523BD0, "CDef::CDbgFlags::GetBitDetail", |mem, c| {
        flags::bit_detail(mem, flags::DBG_DETAILS, c.args[1])
    });
    replay_value(0x523C00, "CDef::CLogFlags::GetBitDetail", |mem, c| {
        flags::bit_detail(mem, flags::LOG_DETAILS, c.args[1])
    });
}

#[test]
fn flag_counts_match() {
    let _pc24 = Precision::Pc24.scope();
    replay_value(0x523BB0, "CDef::CFlags::GetBitCount", |_, _| flags::CDEF_BITS);
    replay_value(0x523BE0, "CDef::CDbgFlags::GetBitCount", |_, _| flags::DBG_BITS);
    replay_value(0x523C10, "CDef::CLogFlags::GetBitCount", |_, _| flags::LOG_BITS);
    replay_value(0x520860, "CMaterial::CFlags::GetBitCount", |_, _| flags::MATERIAL_BITS);
}

#[test]
fn unk_globals_match() {
    let _pc24 = Precision::Pc24.scope();
    // The drive reaches Unk20 of each class and Unk1C of CDef (0x523b20); 0x521560 and 0x5208e0 are not reached.
    for addr in [0x523B10, 0x523B20] {
        replay_value(addr, "CDef::Unk", |_, _| flags::CDEF_UNK_165AA90);
    }
    replay_value(0x521550, "CMountedRigidBody::CDef::Unk", |_, _| flags::MOUNTED_DEF_UNK_1659B20);
    replay_value(0x5208D0, "CMaterialManager::Unk", |_, _| flags::MATERIAL_MANAGER_UNK_1659AD8);
}

#[test]
fn mounted_flags_match() {
    let _pc24 = Precision::Pc24.scope();
    replay_value(0x521920, "E45_M1920", |mem, c| mounted::flag_is_one(mem, c.args[0]) as u32);
    replay_value(0x521940, "E45_M1940", |mem, c| mounted::flag_is_two(mem, c.args[0]) as u32);
    replay_calls(0x521910, "E45Smalls::M1910", &[], |mem, call, _| {
        mounted::set_state(mem, call.args[0], call.args[1]);
    });
    replay_calls(0x521640, "E45_M1640", &[], |mem, call, _| {
        mounted::clear_state(mem, call.args[0]);
    });
}

/// The notify functions call the connection's owner, a listed function (the pad's owner is `E46_InitPhysics`), so the
/// recorder keeps the owner's call as a record of its own, not as an inner call of the notify. The port's call is the
/// owner the window names; the connection's bytes are unchanged.
#[test]
fn notify_owner_matches() {
    let _pc24 = Precision::Pc24.scope();
    for (addr, name) in [(NOTIFY_PAD, "E46FuncConn::NotifyPad"), (NOTIFY_EXIT_GAME, "E46FuncConn::NotifyExitGame")] {
        let Some(calls) = load(VAR, addr) else {
            eprintln!("skipping {name}: {VAR} not set");
            return;
        };
        assert!(!calls.is_empty(), "{name}: no calls in the vector files");
        for call in &calls {
            let mut mem = map_windows(call);
            let mut host = Calls::for_call(call);
            signal_emit::notify_owner(&mut mem, &mut host, call.args[0], call.args[1]);
            let owner = call.windows[0].before[0x10..0x14].try_into().map(u32::from_le_bytes);
            assert_eq!(host.calls, vec![(owner.unwrap(), vec![call.args[1]])], "{name} call {}: owner", call.index);
            for Window { index: k, at, len: _, before: _, after: post } in &call.windows {
                check_except(&mem, *at, post, &[], &format!("{name} call {} window {k}", call.index));
            }
        }
        eprintln!("{name}: {} calls match", calls.len());
    }
}

#[test]
fn material_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(MATERIAL_CTOR, "CMaterial::CMaterial", &[], |mem, call, _| {
        material::construct(mem, call.args[0]);
    });
}

/// `Tag520940` for tag 0 (the drive's first calls, the g4b records) and tags 1..255 and out of range (`tag520940.txt`: one
/// record per tag 0..287, the tag set by the spec's `fuzz_args`). The unit tests in `material.rs` cover the same branches.
#[test]
fn material_tag_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_value(MATERIAL_TAG, "CMaterialManager::Tag520940", |mem, c| material::tag_entry(mem, c.args[0], c.args[1]));
}

/// The spans of a mounted def the caller's code writes: the solid base (+4 to +0x1f0) and the two `unsigned` tweaks
/// (+0x1f0 and +0x230, host calls). The vptr at +0 and the `vector` tweak at +0x200 are the port's, compared here.
const MOUNTED_DEF_SKIP: [(usize, usize); 3] = [(0x4, 0x1ec), (0x1f0, 0x10), (0x230, 0x10)];

/// `CMountedRigidBody::CDef::CtorA` (0x521480).
#[test]
fn mounted_def_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_mounted(MOUNTED_DEF_CTOR, "CMountedRigidBody::CDef::CtorA", &MOUNTED_DEF_SKIP, |mem, call, host| {
        mounted_ctor::construct_def(mem, host, call.args[0], call.args[1]);
    });
}

/// `CMountedRigidBody::NewDef` (0x521b50): the allocation, then `CtorA` on the block (its record is the block's `after`).
#[test]
fn mounted_new_def_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_mounted(MOUNTED_NEW_DEF, "CMountedRigidBody::NewDef", &MOUNTED_DEF_SKIP, |mem, call, host| {
        assert_eq!(mounted_ctor::new_def(mem, host, call.args[0]), call.eax);
    });
}

/// `CMountedRigidBody::CState::Ctor` (0x5215a0).
#[test]
fn mounted_state_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_mounted(MOUNTED_STATE_CTOR, "CMountedRigidBody::CState::Ctor", &[], |mem, call, _host| {
        mounted_ctor::construct_state(mem, call.args[0]);
    });
}

/// `CMountedRigidBody::CDef::Vf4c` (0x521500): the constant string.
#[test]
fn mounted_vf4c_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_value(MOUNTED_VF4C, "CMountedRigidBody::CDef::Vf4c", |_, _| mounted_ctor::vf4c());
}

/// The spans of a body the caller's code writes: everything but the words the port writes (the vptr at +0, the owner at
/// +0x3d0, the scratch's vptr at +0x410, the word at +0x584). The body's `after` run is at its returned address.
const BODY_SKIP: [(usize, usize); 4] = [(0x4, 0x3cc), (0x3d4, 0x3c), (0x414, 0x170), (0x588, 0x8)];

/// The calls a body's constructor makes, as the recording shows them: `port_calls` (the body's direct calls) with the
/// state's constructor before the scratch base. `CState::Ctor` is a listed function, so its record is its own, not one
/// of the body's inner records; its arguments are the body's (`this + 0x3d4`, `this`, the name, `c`).
fn body_expected(call: &Call, c: u32) -> Vec<(u32, Vec<u32>)> {
    let this = call.eax;
    let calls = port_calls(call);
    let name = calls.iter().find(|i| i.addr == GET_NAME_OR_NULL).map_or(0, |i| i.eax);
    let mut expected = Vec::new();
    for i in calls {
        if i.addr == SCRATCH_BASE_INIT {
            expected.push((MOUNTED_STATE_CTOR, vec![this + 0x3d4, this, name, c]));
        }
        expected.push((i.addr, i.args.clone()));
    }
    expected
}

/// The body's skip: its `after`/window at the returned address (`call.eax`) gets `BODY_SKIP`; the owner's window (argument
/// `c`, read for its flags) is checked whole.
fn body_skip(call: &Call, at: u32, _len: usize) -> Vec<(usize, usize)> {
    if at == call.eax {
        BODY_SKIP.to_vec()
    } else {
        Vec::new()
    }
}

/// `CMountedRigidBody::CtorBig` (0x521c30): the solid base, the name, the state, the scratch base and the exclusion are
/// the caller's (the mock writes none of their bytes); the vptrs, the owner and the zero word are the port's.
#[test]
fn mounted_body_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls_expect(
        MOUNTED_BODY_CTOR,
        "CMountedRigidBody::CtorBig",
        body_skip,
        |call| body_expected(call, call.args[3]),
        |mem, call, host| {
            mounted_body::construct_body(
                mem,
                host,
                call.args[0],
                call.args[1],
                call.args[2],
                call.args[3],
                call.args[4],
            );
        },
    );
}

/// `CMountedRigidBody::CDef::Vf50` (0x521d20): the allocation, then `CtorBig` on the block (its calls are the body's).
#[test]
fn mounted_body_vf50_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls_expect(
        MOUNTED_BODY_VF50,
        "CMountedRigidBody::CDef::Vf50",
        body_skip,
        |call| body_expected(call, call.args[2]),
        |mem, call, host| {
            assert_eq!(mounted_body::new_body(mem, host, call.args[0], call.args[1], call.args[2]), call.eax);
        },
    );
}

/// The spans of the pose the caller's code writes: the vector at +0x140 (`CopyFrom`) and the copy at +0x330.
const POSE_SKIP: [(usize, usize); 2] = [(0x140, 0x10), (0x330, 0x10)];

/// `CMountedRigidBody::E45_M1980` (0x521980): the base's `SetDirty`, the owner's `BodyCompose` and the two `CopyFrom`s are
/// the caller's (the mock writes none of their bytes); the dirty flag and the copy to +0x2a0 are the port's.
#[test]
fn mounted_pose_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls_with(
        MOUNTED_POSE,
        "CMountedRigidBody::E45_M1980",
        |call, at, _len| {
            if at == call.args[0] {
                POSE_SKIP.to_vec()
            } else {
                Vec::new()
            }
        },
        |mem, call, host| {
            mounted_pose::pose_update(mem, host, call.args[0]);
        },
    );
}

/// The spans of a passenger the callees write (the base, the tweaks and the vectors): all but the vptr at +0.
const PASSENGER_DEF_SKIP: [(usize, usize); 1] = [(0x4, 0x3c)];
const PASSENGER_STATE_SKIP: [(usize, usize); 1] = [(0x4, 0xbc)];

/// `CPassenger::CDef::CtorA` (0x522450): the base and the tweak are the caller's; the vptr is the port's.
#[test]
fn passenger_def_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(PASSENGER_DEF_CTOR, "CPassenger::CDef::CtorA", &PASSENGER_DEF_SKIP, |mem, call, host| {
        passenger_ctor::construct_def(mem, host, call.args[0]);
    });
}

/// `CPassenger::CDef::CtorB` (0x5224a0): stores only.
#[test]
fn passenger_def_demote_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(PASSENGER_DEF_DEMOTE, "CPassenger::CDef::CtorB", &[], |mem, call, _| {
        passenger_ctor::demote_def(mem, call.args[0]);
    });
}

/// `CPassenger::CState::CtorB` (0x522550): the base, the tweak and the vectors are the caller's; the vptr is the port's.
#[test]
fn passenger_state_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(PASSENGER_STATE_CTOR, "CPassenger::CState::CtorB", &PASSENGER_STATE_SKIP, |mem, call, host| {
        passenger_ctor::construct_state(mem, host, call.args[0]);
    });
}

/// `CPassenger::Ctor` (0x5230d0): the vptrs, the three words and the scratch's `Vf04` (a no-op, so no bytes are skipped).
#[test]
fn passenger_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    replay_calls(PASSENGER_CTOR, "CPassenger::Ctor", &[], |mem, call, host| {
        passenger_ctor::construct_body(mem, host, call.args[0], call.args[1], call.args[2], call.args[3]);
    });
}

/// The grid of an `E45_M40C0` call: each `LookupTwin` answers with its recorded eax, in the recorded order.
struct GridAnswers {
    answers: VecDeque<u32>,
    calls: Vec<(u32, Vec<u32>)>,
}

impl detect_calls::GridHost for GridAnswers {
    fn lookup_twin(&mut self, grid: u32, x: f32, y: f32) -> u32 {
        self.calls.push((LOOKUP_TWIN, vec![grid, x.to_bits(), y.to_bits()]));
        self.answers.pop_front().expect("a recorded LookupTwin")
    }
}

#[test]
fn cell_lookup4_matches() {
    let _pc24 = Precision::Pc24.scope();
    let name = "E45_M40C0";
    let Some(calls) = load(VAR, CELL_LOOKUP4) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let inner: Vec<&Inner> = call.inner.iter().filter(|i| i.addr == LOOKUP_TWIN).collect();
        let mut host = GridAnswers { answers: inner.iter().map(|i| i.eax).collect(), calls: Vec::new() };
        detect_calls::box_cells(&mut mem, &mut host, call.args[0], call.args[1], call.args[2]);
        let want: Vec<(u32, Vec<u32>)> = inner.iter().map(|i| (i.addr, i.args.clone())).collect();
        assert_eq!(host.calls, want, "{name} call {}: lookups", call.index);
        check_windows(call, &mem, name);
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, by branch: {branches:?}", calls.len());
}

/// The edge tests of an `E45_M46A0` call, in the recorded order. A hit's point, distance, normal and hit word are the
/// record's output windows (its windows 0..4, after the call).
struct EdgeAnswers<'a> {
    records: VecDeque<&'a Inner>,
}

impl detect_calls::EdgeHost for EdgeAnswers<'_> {
    fn edge_edge(&mut self, pair: u32, a: u32, b: u32) -> Option<detect::EdgeHit> {
        let rec = self.records.pop_front().expect("a recorded EdgeEdge");
        assert_eq!(&rec.args[..3], &[pair, a, b], "EdgeEdge arguments");
        (rec.eax != 0).then(|| edge_hit(rec))
    }
}

fn edge_hit(rec: &Inner) -> detect::EdgeHit {
    let out = |k: usize| rec.windows[k].after.as_slice();
    let f32_at = |bytes: &[u8], k: usize| f32::from_le_bytes(bytes[k..k + 4].try_into().unwrap());
    let vec4 =
        |bytes: &[u8]| Vec4 { x: f32_at(bytes, 0), y: f32_at(bytes, 4), z: f32_at(bytes, 8), w: f32_at(bytes, 12) };
    detect::EdgeHit {
        point: vec4(out(0)),
        dist: f32_at(out(1), 0),
        normal: vec4(out(2)),
        hit_word: u32::from_le_bytes(out(3).try_into().unwrap()),
    }
}

#[test]
fn edge_sweep_matches() {
    let _pc24 = Precision::Pc24.scope();
    let name = "E45_M46A0";
    let Some(calls) = load(VAR, EDGE_SWEEP) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let records: VecDeque<&Inner> = call.inner.iter().filter(|i| i.addr == EDGE_EDGE).collect();
        let expected = records.len();
        let mut host = EdgeAnswers { records };
        let [_, a, b, list, out] = call.args[..] else { panic!("{name} call {}: five words", call.index) };
        let ret = detect_calls::e45_m46a0(&mut mem, &mut host, a, b, list, out);
        assert_eq!(ret, call.eax, "{name} call {}: eax", call.index);
        assert!(host.records.is_empty(), "{name} call {}: {expected} EdgeEdge records, not all tested", call.index);
        check_windows(call, &mem, name);
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, by branch: {branches:?}", calls.len());
}

/// Takes the next recorded call of a callee, checking its address and its leading arguments.
fn take<'a>(records: &mut VecDeque<&'a Inner>, addr: u32, args: &[u32]) -> &'a Inner {
    let rec = records.pop_front().expect("a recorded call");
    assert_eq!(rec.addr, addr, "recorded call");
    assert_eq!(&rec.args[..args.len()], args, "recorded arguments");
    rec
}

/// Writes the bytes a recorded callee left in its windows (its effect on the caller's memory).
fn write_windows(mem: &mut Mem, rec: &Inner) {
    for Window { index: _, at, len: _, before: _, after: post } in &rec.windows {
        mem.map(*at, post);
    }
}

/// The callees of a `CL2PenetrationDetector::Ctor` call, answered from its recorded inner calls in order: `E38RaySeeds`
/// and `E01Vec4::CopyFrom` replay the bytes they left. The `Vec4::Clear` calls are inside `E38RaySeeds`, so they are
/// not the port's.
struct PenetrationAnswers<'a> {
    records: VecDeque<&'a Inner>,
}

impl detect_calls::PenetrationHost for PenetrationAnswers<'_> {
    fn ray_seeds(&mut self, mem: &mut Mem, seeds: u32) {
        write_windows(mem, take(&mut self.records, E38_RAY_SEEDS, &[seeds]));
    }

    fn copy_vec4(&mut self, mem: &mut Mem, to: u32, from: u32) {
        take(&mut self.records, E01_VEC4_COPY_FROM, &[to, from]);
        let bytes = mem.read(from, 16);
        mem.map(to, &bytes);
    }
}

/// `CL2PenetrationDetector::Ctor` (0x5235b0): the object's stores, `E38RaySeeds` at +0x30 and the two `CopyFrom`s.
#[test]
fn penetration_ctor_matches() {
    let _pc24 = Precision::Pc24.scope();
    let name = "CL2PenetrationDetector::Ctor";
    let Some(calls) = load(VAR, PENETRATION_CTOR) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    for call in &calls {
        let mut mem = map_windows(call);
        let records: VecDeque<&Inner> =
            call.inner.iter().filter(|i| i.addr == E38_RAY_SEEDS || i.addr == E01_VEC4_COPY_FROM).collect();
        let mut host = PenetrationAnswers { records };
        let [this, owner, b, c, d, e] = call.args[..] else { panic!("{name} call {}: six words", call.index) };
        let ret = detect_calls::l2_penetration_ctor(&mut mem, &mut host, this, owner, b, c, f32::from_bits(d), e);
        assert_eq!(ret, call.eax, "{name} call {}: eax", call.index);
        assert!(host.records.is_empty(), "{name} call {}: recorded calls not made", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

/// The callees of an `E45_MD60` call, answered from its recorded inner calls in order: `QueryWall` answers its recorded
/// eax, with its two corners in the out windows (the record's first two); `operator new` and the `CCellGrid` constructor
/// answer their recorded results, and the constructor's object bytes are written back.
struct GridBuildAnswers<'a> {
    records: VecDeque<&'a Inner>,
}

impl detect_calls::GridBuildHost for GridBuildAnswers<'_> {
    fn query_wall(&mut self, _: &mut Mem, wall: u32) -> Option<[detect::Xy; 2]> {
        let rec = take(&mut self.records, QUERY_WALL, &[wall]);
        // the out windows are the record's first two, in argument order (min, then max)
        let corner = |k: usize| {
            let bytes = &rec.windows[k].after;
            let f32_at = |k: usize| f32::from_le_bytes(bytes[k..k + 4].try_into().unwrap());
            detect::Xy { x: f32_at(0), y: f32_at(4) }
        };
        (rec.eax != 0).then(|| [corner(0), corner(1)])
    }

    fn alloc_grid(&mut self, _: &mut Mem) -> u32 {
        take(&mut self.records, OPERATOR_NEW, &[0x14]).eax
    }

    fn construct_grid(&mut self, mem: &mut Mem, object: u32, min: detect::Xy, width: f32, height: f32) -> u32 {
        let rec = take(
            &mut self.records,
            CELL_GRID_CTOR,
            &[object, min.x.to_bits(), min.y.to_bits(), width.to_bits(), height.to_bits()],
        );
        write_windows(mem, rec);
        rec.eax
    }
}

/// `E45_MD60` (0x523d60): the grid build (the walls' bounds, or the defaults when QueryWall answers 0), or the early
/// return when the grid is set.
#[test]
fn grid_build_matches() {
    let _pc24 = Precision::Pc24.scope();
    let name = "E45_MD60";
    let Some(calls) = load(VAR, GRID_BUILD) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        // the CCellGrid constructor's own `operator new(0x484)` is inside the constructor, so it is not the port's
        let records: VecDeque<&Inner> = call
            .inner
            .iter()
            .filter(|i| {
                i.addr == QUERY_WALL || i.addr == CELL_GRID_CTOR || (i.addr == OPERATOR_NEW && i.args == [0x14])
            })
            .collect();
        let mut host = GridBuildAnswers { records };
        detect_calls::build_cell_grid(&mut mem, &mut host);
        assert!(host.records.is_empty(), "{name} call {}: recorded calls not made", call.index);
        check_windows(call, &mem, name);
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, by branch: {branches:?}", calls.len());
}

/// The slot calls a `Work5500` host records: (slot, node, the stack word for the slots that take it).
#[derive(Default)]
struct NodeCalls(Vec<(u32, u32, Option<u32>)>);

impl contact_walk::NodeHost for NodeCalls {
    fn force_input(&mut self, _: &mut Mem, node: u32) {
        self.0.push((0x5C, node, None));
    }

    fn force_to_accel(&mut self, _: &mut Mem, node: u32) {
        self.0.push((0x64, node, None));
    }

    fn integrate(&mut self, _: &mut Mem, node: u32, p: f32) {
        self.0.push((0x6C, node, Some(p.to_bits())));
    }

    fn child_walk(&mut self, _: &mut Mem, node: u32, p: f32) {
        self.0.push((0x58, node, Some(p.to_bits())));
    }
}

/// `Work5500` (0x525500): the three passes over the owner's contact list, in the asm's order. The expected nodes are read
/// from the record's memory before the port runs. The record also holds the nodes' own calls (flat, in call order), which
/// are not the host's: the windows are compared, and the host writes nothing.
#[test]
fn work_5500_matches() {
    let _pc24 = Precision::Pc24.scope();
    let name = "Work5500";
    let Some(calls) = load(VAR, WORK_5500) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    let mut walked = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let [this, p_bits] = call.args[..] else { panic!("{name} call {}: two words", call.index) };
        let owner = mem.u32(this + 0x20);
        let mut nodes = Vec::new();
        let mut node = mem.u32(owner + 0x450);
        while node != 0 {
            nodes.push(node);
            node = mem.u32(node + 0x10);
        }
        walked += nodes.len();
        let want: Vec<(u32, u32, Option<u32>)> = nodes
            .iter()
            .map(|&n| (0x5C, n, None))
            .chain(nodes.iter().map(|&n| (0x64, n, None)))
            .chain(nodes.iter().flat_map(|&n| [(0x6C, n, Some(p_bits)), (0x58, n, Some(p_bits))]))
            .collect();
        let mut host = NodeCalls::default();
        contact_walk::step_bodies(&mut mem, &mut host, this, f32::from_bits(p_bits));
        assert_eq!(host.0, want, "{name} call {}: node calls", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls match, {walked} nodes walked", calls.len());
}

#[test]
fn pull_to_rest_drops_a_pull_against_the_dead_band() {
    let _pc24 = Precision::Pc24.scope();
    // Lane 0: h40 = -10 pulls one way, the dead band (h90 = 0.1, past its zone) the other, so the pull is 0.
    // The work vector is K3*h90 + K4*h40 = -1000; rate = -0.001, so h40 = 1 - 10 = -9 and h90 = -9*0.03 + 0.1.
    let h40 = Vec4 { x: -10.0, ..Vec4::default() };
    let h90 = Vec4 { x: 0.1, ..Vec4::default() };
    let (h40, h90) = passenger::pull_to_rest(h40, h90, Vec4::default(), 0.03);
    assert!((h40.x + 9.0).abs() < 1e-3);
    assert!((h90.x + 0.17).abs() < 1e-4);
}

#[test]
fn walk_point_adds_the_host_offset() {
    let _pc24 = Precision::Pc24.scope();
    assert_eq!(passenger::walk_point([1.0, 2.0, 3.0], [10.0, 20.0, 30.0]), [11.0, 22.0, 33.0]);
}

#[test]
fn step_forward_pacing_and_clock_match_retail_rules() {
    let _pc24 = Precision::Pc24.scope();
    let elapsed = 1.234_567_89_f64;
    let dt = 0.007_f32;
    assert_eq!(pacing::advance_time(elapsed, dt), f64::from(elapsed as f32 + dt));

    assert_eq!(pacing::single_step_count(3, 0, true, 2), (3, 2));
    assert_eq!(pacing::single_step_count(3, 0x8000, true, 0), (0, 0));
    assert_eq!(pacing::single_step_count(3, 0x8000, true, 2), (1, 1));
    assert_eq!(pacing::single_step_count(3, 0x8000, false, 2), (3, 2));

    assert!(!pacing::contact_due(0x1000, 3));
    assert!(pacing::contact_due(0x1000, 4));
    assert!(pacing::contact_due(0, 3));
}

//! Replays the original's tyre calls (`harness/vcu_vectors.py --set tyre`) through the ports in
//! `rbr_physics::tyre`. Needs record files: `RBR_TYRE_VECTORS=<file>[:<file>...]`. Without the variable
//! the tests are skipped.

mod common;

use common::{load, map_windows, Call};
use rbr_physics::fpu::Precision;
use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::Mem;
use rbr_physics::tyre::store::TyreStore;
use rbr_physics::tyre::{
    align, contact_patch, curve, fill, force, forces, friction, grip, layout, locate, relax, slip, solve, state, write,
};

const VAR: &str = "RBR_TYRE_VECTORS";

/// Replays every recorded call of `addr` through `run`, on the memory `map` builds for the call. `run` returns st0
/// (for an x87 return), or `Outside` when the port declines the call: a declined call is counted and skipped, because
/// the binding runs the original for it. st0 is checked bit for bit, then every window must hold the original's bytes.
fn replay_run(
    addr: u32,
    name: &str,
    nan_tolerant: bool,
    map: fn(&Call) -> Mem,
    run: impl Fn(&mut Mem, &Call) -> Result<Option<f64>, layout::Outside>,
) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    let mut declined = 0;
    for call in &calls {
        let mut mem = map(call);
        let st0 = match run(&mut mem, call) {
            Ok(st0) => st0,
            Err(out) => {
                // A decline is an index outside the table's window: below 0, or past its entries. The word below a
                // table (-1) is in its window (locate and the blend tables), so no decline reports -1.
                assert!(
                    out.index < 0 || out.index >= out.len as i32,
                    "{name} call {} ({}): declined at index {} of {} entries, inside the window",
                    call.index,
                    call.branch,
                    out.index,
                    out.len
                );
                declined += 1;
                continue;
            }
        };
        if let Some(st0) = st0 {
            let recorded = f64::from_bits(call.st0_bits);
            // The fixture retains the original st0 NaN sign, but this comparison intentionally ignores it:
            // a host may choose a different default-NaN sign. The windows are compared bit for bit here.
            if recorded.is_nan() {
                assert!(st0.is_nan(), "{name} call {} ({}): st0 {st0} vs NaN", call.index, call.branch);
            } else {
                assert_eq!(
                    st0.to_bits(),
                    recorded.to_bits(),
                    "{name} call {} ({}): st0 {st0} vs {recorded}",
                    call.index,
                    call.branch
                );
            }
        }
        for (k, at, len, _, post) in &call.windows {
            let now = mem.read(*at, *len);
            let same = if nan_tolerant { same_words(&now, post) } else { now == *post };
            assert!(same, "{name} call {} window {k}: {now:02x?} vs {post:02x?}", call.index);
        }
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, {declined} declined, by branch: {branches:?}", calls.len() - declined);
}

/// Replays every recorded call of `addr` through `port`, which maps the call's arguments onto the port and
/// writes the results into `mem` and returns st0 (for an x87 return). st0 is checked bit for bit, then every
/// window must hold the original's bytes after the call.
fn replay(addr: u32, name: &str, port: impl Fn(&mut Mem, &Call) -> Option<f64>) {
    replay_inner(addr, name, false, port);
}

/// `replay`, with the windows compared NaN-tolerantly when `nan_tolerant` (see `same_words`).
fn replay_inner(addr: u32, name: &str, nan_tolerant: bool, port: impl Fn(&mut Mem, &Call) -> Option<f64>) {
    replay_run(addr, name, nan_tolerant, map_windows, |mem, c| Ok(port(mem, c)));
}

/// Replays `addr` through a frame port: the frame is loaded from `this` (`args[0]`) once, `port` runs on it, and the
/// frame is stored once after. `port` reads the call's other words through `mem`; a port with output pointers that may
/// sit in the frame writes them through `replay_run`, after the frame.
fn replay_frame(
    addr: u32,
    name: &str,
    nan_tolerant: bool,
    port: impl Fn(&mut layout::Frame, &mut Mem, &Call) -> Result<Option<f64>, layout::Outside>,
) {
    replay_run(addr, name, nan_tolerant, map_frame, |mem, c| {
        let this = c.args[0];
        let mut frame = mem.load_frame(this);
        let out = port(&mut frame, mem, c)?;
        mem.store_frame(this, &frame);
        Ok(out)
    });
}

/// Whether the call's windows hold the `len` bytes at `at`.
fn recorded(call: &Call, at: u32, len: u32) -> bool {
    call.windows.iter().any(|(_, w, wlen, _, _)| *w <= at && at + len <= *w + *wlen as u32)
}

/// The memory of a frame call: the recorded windows, then zeros over the frame's footprint (`load_frame` reads
/// `this`, the def, the state, the record's contact angle and the wear table) wherever no window holds the word. A
/// window keeps its own bytes, and a word no window holds reads as zero.
fn map_frame(call: &Call) -> Mem {
    let mut mem = map_windows(call);
    let zero = |mem: &mut Mem, base: u32, len: u32| {
        for at in base..base + len {
            if !recorded(call, at, 1) {
                mem.map(at, &[0]);
            }
        }
    };
    let this = call.args[0];
    zero(&mut mem, this, 0x520);
    let (def, state) = (mem.u32(this + 0x10), mem.u32(this + 0x14));
    let (record, wear) = (mem.u32(this + 0x410), mem.u32(this + 0x414));
    zero(&mut mem, def, 0x320);
    zero(&mut mem, state, 0x140);
    zero(&mut mem, record + 0x5AC, 4);
    zero(&mut mem, wear, 0x20);
    mem
}

/// The argument word `i` as an f32.
fn arg_f32(call: &Call, i: usize) -> f32 {
    f32::from_bits(call.args[i])
}

#[test]
fn sign_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x531ba0, "sign", |mem, c| Some(force::signed_term(mem.load_sign_input(c.args[0]))));
}

#[test]
fn tyre_clamp_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x5322e0, "tyre_clamp", |mem, c| {
        let out = force::tyre_clamp(arg_f32(c, 0), arg_f32(c, 1), arg_f32(c, 2));
        mem.set_f32(c.args[3], out);
        None
    });
}

#[test]
fn compute_slip_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x532a30, "compute_slip", |mem, c| {
        let this = c.args[0];
        let dir = c.args[1];
        let dir = [mem.f32(dir), mem.f32(dir + 4), mem.f32(dir + 8)];
        let state = mem.load_state(this);
        let matrix = mem.load_slip_matrix(this);
        let out = slip::compute_slip(&matrix, state.effective_radius, dir, arg_f32(c, 2));
        mem.store_slip(this, &out);
        None
    });
}

#[test]
fn solve_dirs_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x533640, "solve_dirs", |mem, c| {
        let state = mem.load_state(c.args[0]);
        let out = slip::solve_dirs(arg_f32(c, 1), arg_f32(c, 2), arg_f32(c, 3), state.effective_radius);
        for (i, value) in out.iter().enumerate() {
            mem.set_f32(c.args[4 + i], *value);
        }
        None
    });
}

#[test]
fn accumulate_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_inner(0x5338a0, "accumulate", true, |mem, c| {
        let this = c.args[0];
        let out = force::accumulate(mem.load_accumulate(this));
        mem.store_accumulate(this, &out);
        None
    });
}

#[test]
fn copy_from_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x535040, "copy_from", |mem, c| {
        let src = mem.load_material(c.args[1]);
        mem.store_material(c.args[0], &src);
        None
    });
}

#[test]
fn set_414_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x532490, "set_414", |mem, c| {
        mem.store_414(c.args[0], c.args[1]);
        None
    });
}

/// The solve tests: the contact values are loaded from the record's `p`, `qb` and `r` and stored back after.
/// The windows are compared with `same_words`: a NaN the solve makes from finite operands (`inf - inf`, a
/// `sqrt` of a negative) may take a different default-NaN sign on the host. Every other word must match bit for bit.
fn replay_solve(addr: u32, name: &str, run: impl Fn(&mut rbr_physics::tyre::solve::Contact, &Call)) {
    replay_inner(addr, name, true, |mem, c| {
        let (this, p, qb, r) = (c.args[0], c.args[1], c.args[2], c.args[3]);
        let mut contact = mem.load_contact(this, p, qb, r);
        run(&mut contact, c);
        mem.store_contact(this, p, qb, &contact);
        None
    });
}

/// Equal byte strings, with NaN equal to NaN word by word (`f32`, 4 bytes at a time; the tail is compared as bytes).
fn same_words(ours: &[u8], orig: &[u8]) -> bool {
    let word_ok = |a: &[u8], b: &[u8]| {
        a == b
            || (f32::from_le_bytes(a.try_into().unwrap()).is_nan()
                && f32::from_le_bytes(b.try_into().unwrap()).is_nan())
    };
    let whole = ours.len() / 4 * 4;
    ours.len() == orig.len()
        && (0..whole).step_by(4).all(|i| word_ok(&ours[i..i + 4], &orig[i..i + 4]))
        && ours[whole..] == orig[whole..]
}

#[test]
fn solve_a_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_solve(0x533ae0, "solve_a", |contact, c| solve::solve_a(contact, c.args[4] as i32));
}

#[test]
fn solve_b_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_solve(0x533bf0, "solve_b", |contact, c| solve::solve_b(contact, c.args[4] as i32));
}

#[test]
fn solve_c_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_solve(0x533d40, "solve_c", |contact, c| solve::solve_c(contact, c.args[4] as i32));
}

#[test]
fn solve_d_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_solve(0x533e80, "solve_d", |contact, c| solve::solve_d(contact, c.args[4] as i32));
}

#[test]
fn solve_dispatch_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_solve(0x5351a0, "solve_dispatch", |contact, c| {
        solve::solve_dispatch(contact, c.args[4] != 0, c.args[5] != 0, c.args[6] as i32)
    });
}

/// `friction` with NaN-tolerant windows: its `sqrt` of a negative takes the host's NaN sign (see `same_words`).
#[test]
fn friction_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_inner(0x531c70, "friction", true, |mem, c| {
        let this = c.args[0];
        let out = friction::friction(mem.load_friction(this, arg_f32(c, 1)));
        mem.store_friction(this, out.sv);
        Some(out.value)
    });
}

/// `floor_s32` (FloorS32, stdcall: x, the table, its count). The table is a window read through `mem`.
#[test]
fn floor_s32_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x5325f0, "floor_s32", |mem, c| {
        let (table, count) = (c.args[1], c.args[2] as i32);
        Some(curve::floor_s32(arg_f32(c, 0), count, |i| mem.f32(table + 4 * i)))
    });
}

/// `friction_curve` (TyreFrictionCurve, stdcall with 13 words): args 0-8 are values, 9 and 10 the outputs,
/// 11 the table and 12 its count. NaN-tolerant windows: a NaN output takes the host's NaN sign (see `same_words`).
#[test]
fn friction_curve_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_inner(0x5334d0, "friction_curve", true, |mem, c| {
        let (table, count) = (c.args[11], c.args[12] as i32);
        let input = curve::CurveInput { args: std::array::from_fn(|k| arg_f32(c, k)), count };
        let out = curve::friction_curve(&input, |i| mem.f32(table + 4 * i));
        mem.set_f32(c.args[9], out.out9);
        mem.set_f32(c.args[10], out.out10);
        Some(out.value)
    });
}

/// `split_slip` (SplitSlip, thiscall, no stack words): the slip speed at +0x360 splits into the weights at +0x3dc
/// and +0x3f0.
#[test]
fn split_slip_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_frame(0x5346f0, "split_slip", false, |frame, _mem, _c| {
        slip::split_slip(frame);
        Ok(None)
    });
}

/// `align` (Align, thiscall: the two vector pointers as stack words). Its eax is 1 when it aligned.
#[test]
fn align_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_frame(0x531fc0, "align", true, |frame, mem, c| {
        let a = Vec4::load(mem, c.args[1]);
        let b = Vec4::load(mem, c.args[2]);
        let aligned = align::align(frame, a, b);
        assert_eq!(u32::from(aligned), c.eax, "align call {}: eax", c.index);
        Ok(None)
    });
}

/// `clamp_grip` (ClampGrip, thiscall, seven stack words: five floats, then the out pointers).
#[test]
fn clamp_grip_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // The outputs are pointers that may sit in the frame: they are written after the frame, as the binding does.
    replay_run(0x533760, "clamp_grip", false, map_frame, |mem, c| {
        let this = c.args[0];
        let g = grip::GripInput {
            s: f32::from_bits(c.args[1]),
            a: f32::from_bits(c.args[2]),
            b: f32::from_bits(c.args[3]),
            c: f32::from_bits(c.args[4]),
            d: f32::from_bits(c.args[5]),
        };
        let mut frame = mem.load_frame(this);
        let out = grip::clamp_grip(&frame.state, g);
        frame.state.grip_flag = out.flag;
        frame.scratch.f3f4 = out.f3f4;
        frame.scratch.f3f8 = out.f3f8;
        mem.store_frame(this, &frame);
        mem.set_u32(c.args[6], out.out_a);
        mem.set_u32(c.args[7], out.out_b);
        Ok(None)
    });
}

/// `solve_pair` (SolvePair, thiscall, no stack words).
#[test]
fn solve_pair_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_frame(0x5352a0, "solve_pair", false, |frame, _mem, _c| {
        grip::solve_pair(frame);
        Ok(None)
    });
}

/// `solve_forces` (SolveForces, thiscall: the friction's `k` and the two output vectors as stack words).
/// NaN-tolerant: a NaN lane of an output takes the other default NaN sign (the x87 default is negative), see notes.
#[test]
fn solve_forces_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // The outputs are pointers that may sit in the frame: they are written after the frame, as the binding does.
    replay_run(0x532b60, "solve_forces", true, map_frame, |mem, c| {
        let this = c.args[0];
        let out2 = Vec4::load(mem, c.args[3]);
        let mut frame = mem.load_frame(this);
        let (out1, out2) = forces::solve_forces(&mut frame, arg_f32(c, 1), out2);
        mem.store_frame(this, &frame);
        out1.store(mem, c.args[2]);
        out2.store(mem, c.args[3]);
        Ok(None)
    });
}

/// `fill_outputs` (FillOutputs, thiscall: the `src` vector pointer, the `pair` pointer and the divisor as stack words).
#[test]
fn fill_outputs_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_frame(0x532f10, "fill_outputs", false, |frame, mem, c| {
        let src = Vec4::load(mem, c.args[1]);
        let pair: [u32; 3] = std::array::from_fn(|k| mem.u32(c.args[2] + 4 * k as u32));
        fill::fill_outputs(frame, src, pair, arg_f32(c, 3));
        Ok(None)
    });
}

/// `write_outputs` (WriteOutputs, thiscall: the output struct pointer as its stack word).
#[test]
fn write_outputs_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // Fuzz call 3858 differs only in the sign of six default NaNs (see rbr-recomp notes/rust-physics.md section 8).
    // The outputs are pointers that may sit in the frame: they are written after the frame, as the binding does.
    replay_run(0x533080, "write_outputs", true, map_frame, |mem, c| {
        let this = c.args[0];
        let mut frame = mem.load_frame(this);
        let block = write::write_outputs(&mut frame);
        mem.store_frame(this, &frame);
        mem.store_output_block(c.args[1], &block);
        Ok(None)
    });
}

/// `contact_patch` (ContactPatch, thiscall with one stack word, the `out` float). Its `eax` is 1 when solved. The
/// windows are NaN-tolerant: a `sqrt` of a negative `s8` takes the other default NaN sign (fuzz call 233).
#[test]
fn contact_patch_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // The outputs are pointers that may sit in the frame: they are written after the frame, as the binding does.
    replay_run(0x5358f0, "contact_patch", true, map_frame, |mem, c| {
        let this = c.args[0];
        let mut frame = mem.load_frame(this);
        let out = contact_patch::contact_patch(&mut frame);
        mem.store_frame(this, &frame);
        assert_eq!(u32::from(out.is_some()), c.eax, "contact_patch call {}: eax", c.index);
        if let Some(value) = out {
            mem.set_f32(c.args[1], value);
        }
        Ok(None)
    });
}

/// `locate` (0x528980, cdecl): the table pointer, `x`, the count and the out struct as stack words 0 to 3.
#[test]
fn locate_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // NaN-tolerant: a NaN fraction's sign is the host's default, not the original's x87 one.
    replay_run(0x528980, "locate", true, map_locate, |mem, c| {
        let (table, n, out) = (c.args[0], c.args[2], c.args[3]);
        let table = mem.read_table(table, n as usize);
        let seg = locate::locate(mem.load_segment(out), &table, arg_f32(c, 1))?;
        mem.store_segment(out, &seg);
        Ok(None)
    });
}

/// The memory of a `locate` call: the recorded windows, then zero on the word below the table where no window holds it
/// (the drive records `d-*` predate that window). The search reads that word only from a hint of -1, or from a NaN `x`
/// with a hint of 0, which gives a NaN fraction whatever the word. No store record has a hint of -1, so the zero is
/// never read into a result; a record that does would show it as a mismatch.
fn map_locate(call: &Call) -> Mem {
    let mut mem = map_windows(call);
    let below = call.args[0] - 4;
    if !recorded(call, below, 4) {
        mem.map(below, &[0; 4]);
    }
    mem
}

/// `set_state_field` (SetStateField, thiscall with the flag as its stack word).
#[test]
fn set_state_field_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_frame(0x532470, "set_state_field", false, |frame, _mem, c| {
        state::set_state_field(frame, c.args[1]);
        Ok(None)
    });
}

/// `set_state48` (SetState48, thiscall with the float as its stack word). The windows include the def (`+0x7c`, `+0x8c`).
#[test]
fn set_state48_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // The original reads the def's `+0x7c` and `+0x8c`. A record without a window over them (`d-s*`) cannot show the
    // result, so the call is counted as declined.
    replay_run(0x533490, "set_state48", false, map_frame, |mem, c| {
        let this = c.args[0];
        let def = mem.u32(this + 0x10);
        if !recorded(c, def + 0x7C, 0x14) {
            // Not an index: no window holds the def. The empty window (index 0 of 0 entries) is outside it.
            return Err(layout::Outside { index: 0, len: 0 });
        }
        let mut frame = mem.load_frame(this);
        state::set_state48(&mut frame, arg_f32(c, 1));
        mem.store_frame(this, &frame);
        Ok(None)
    });
}

/// `clear_bias` (ClearBias, thiscall, no stack words). Its `DamperBiasSub` calls write nothing, so the windows are
/// unchanged.
#[test]
fn clear_bias_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x534000, "clear_bias", |_mem, _c| {
        state::clear_bias();
        None
    });
}

/// `blend_table` (BlendTable, thiscall with six stack words; eax is 1). The outputs go through the out pointers in
/// the asm's order.
#[test]
fn blend_table_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x534e70, "blend_table", |mem, c| {
        let input = mem.load_blend_table(c.args[0], c.args[1], c.args[2], arg_f32(c, 3), arg_f32(c, 4));
        mem.store_blend(c.args[5], c.args[6], relax::blend_table(&input));
        assert_eq!(c.eax, 1, "blend_table call {}: eax", c.index);
        None
    });
}

/// `TyreTail_Get10` (thiscall, no stack words): the word at the container's `+0x10`, returned in eax.
#[test]
fn tyre_tail10_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x5326b0, "tyre_tail10", |mem, c| {
        assert_eq!(mem.load_tyre_tail10(c.args[0]), c.eax, "tyre_tail10 call {}: eax", c.index);
        None
    });
}

/// `TyreTail_Get14` (thiscall, no stack words): the word at the container's `+0x14`, returned in eax.
#[test]
fn tyre_tail14_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x5326c0, "tyre_tail14", |mem, c| {
        assert_eq!(mem.load_tyre_tail14(c.args[0]), c.eax, "tyre_tail14 call {}: eax", c.index);
        None
    });
}

/// `NTweak::CBase::GetName` (thiscall, no stack words): the name object at `this+4`, returned in eax.
#[test]
fn name_object_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x52db10, "name_object", |mem, c| {
        assert_eq!(mem.load_name_object(c.args[0]), c.eax, "name_object call {}: eax", c.index);
        None
    });
}

/// `CTyre::CScratch::CopyDefFrom` (thiscall, one stack word, the setup): the setup copied into the def, the state's
/// copies, and `DamperUpdateB` on the state's terms. The copied words are bit copies; the update may make NaNs.
#[test]
fn copy_def_from_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x534760, "copy_def_from", true, map_windows, |mem, c| {
        layout::copy_def_from(mem, c.args[0], c.args[1]);
        Ok(None)
    });
}

/// `CTyre::CState::CState` (thiscall, three stack words the constructor does not store, `ret 0xC`): the defaults.
/// It returns `this`.
#[test]
fn cstate_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x531500, "cstate_ctor", false, map_windows, |mem, c| {
        layout::construct_state(mem, c.args[0]);
        assert_eq!(c.eax, c.args[0], "cstate_ctor call {}: eax", c.index);
        Ok(None)
    });
}

/// `CTyre::CScratch::Init` (thiscall, no stack words): the scratch's defaults, on the scratch the call is made on.
#[test]
fn scratch_init_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x5327d0, "scratch_init", false, map_windows, |mem, c| {
        layout::init_scratch(mem, c.args[0]);
        Ok(None)
    });
}

/// `CTyre::CScratch::CScratch(def, state, record)` (thiscall, three stack words, `ret 0xC`): `Init` on the sub-block,
/// the scratch's defaults, and the three pointers. It returns `this`.
#[test]
fn scratch_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x5328d0, "scratch_ctor", false, map_windows, |mem, c| {
        layout::construct_scratch(mem, c.args[0], c.args[1], c.args[2], c.args[3]);
        assert_eq!(c.eax, c.args[0], "scratch_ctor call {}: eax", c.index);
        Ok(None)
    });
}

/// The memory of `CreateScratch`: the recorded windows, then the block it allocated, zeroed as a fresh allocation.
fn map_scratch_block(call: &Call) -> Mem {
    let mut mem = map_windows(call);
    for (at, len, _) in &call.after {
        mem.map(*at, &vec![0; *len]);
    }
    mem
}

/// `CTyre::CDef::CreateScratch` (thiscall): the block `operator new` returned (`eax`) holds the scratch after `Init`.
#[test]
fn create_scratch_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x535220, "create_scratch", false, map_scratch_block, |mem, c| {
        assert_eq!(layout::create_scratch(mem, c.eax), c.eax, "create_scratch call {}: eax", c.index);
        for (at, len, bytes) in &c.after {
            assert_eq!(&mem.read(*at, *len), bytes, "create_scratch call {}: block", c.index);
        }
        Ok(None)
    });
}

/// `FUN_005324a0` (fastcall, no stack words): 17 `NTweak::CBase` headers at every 0x10 bytes of the block.
#[test]
fn cbase_headers_17_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x5324a0, "cbase_headers_17", false, map_windows, |mem, c| {
        layout::cbase_headers_17(mem, c.args[0]);
        Ok(None)
    });
}

/// `FUN_00532550` (fastcall, no stack words): 3 `NTweak::CBase` headers at every 0x10 bytes of the block.
#[test]
fn cbase_headers_3_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x532550, "cbase_headers_3", false, map_windows, |mem, c| {
        layout::cbase_headers_3(mem, c.args[0]);
        Ok(None)
    });
}

/// `CTyre::CScratch::CScratch()` (0x532570, the destructor body, thiscall): the defaults on the scratch.
#[test]
fn scratch_body_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x532570, "scratch_body", false, map_windows, |mem, c| {
        layout::destroy_scratch_body(mem, c.args[0]);
        Ok(None)
    });
}

/// `CTyre::CScratch::~CScratch` (0x5329a0, thiscall): the scratch's words, its def and state cleared, and the body on
/// the sub-block. The release of the def is a no-op in the original.
#[test]
fn scratch_dtor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x5329a0, "scratch_dtor", false, map_windows, |mem, c| {
        layout::destroy_scratch(mem, c.args[0]);
        Ok(None)
    });
}

/// `CTyre::CScratch::DeleteScratch(flags)` (0x5328b0, thiscall, one stack word): the body on the scratch, returning
/// `this`. The block is freed when `flags & 1`, by the caller; the original's call to free is not compared.
#[test]
fn delete_scratch_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x5328b0, "delete_scratch", false, map_windows, |mem, c| {
        let free = layout::delete_scratch(mem, c.args[0], c.args[1]);
        assert_eq!(free, c.args[1] & 1 != 0, "delete_scratch call {}: flags", c.index);
        Ok(None)
    });
}

/// `CTyre::CDef::DeleteContainer(flags)` (0x535280, thiscall, one stack word): `~CScratch` on the scratch, then the
/// free when `flags & 1`, as `DeleteScratch`.
#[test]
fn delete_container_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_run(0x535280, "delete_container", false, map_windows, |mem, c| {
        let free = layout::delete_container(mem, c.args[0], c.args[1]);
        assert_eq!(free, c.args[1] & 1 != 0, "delete_container call {}: flags", c.index);
        Ok(None)
    });
}

/// `CTyre::CScratch::EmitNoise` (0x535890) with no connections on the tyre-noise signal: nothing is invoked, and the
/// scratch's signal block keeps its first word and loses its second.
#[test]
fn emit_noise_clears_the_signal_block() {
    let _pc24 = Precision::Pc24.scope();
    struct NoConnections;
    impl rbr_physics::physics_core::layout::SignalHost for NoConnections {
        fn invoke(&mut self, _mem: &mut Mem, _conn: u32, _sig: u32) {
            unreachable!("the signal has no connections")
        }
        fn log_recursive(&mut self) {
            unreachable!("the signal is not emitting")
        }
        fn log_range(&mut self, _count: u32) {
            unreachable!("the count is in range")
        }
    }
    let (manager, scratch) = (0x8000, 0x9000);
    let mut mem = Mem::new();
    mem.set_u32(0x0089_3600, manager); // g_signalManager
    let signal = manager + 0x6268;
    mem.set_u32(signal, 0); // the guard, clear
    mem.set_u32(signal + 4, 0x7000); // the count pointer, count 0
    mem.set_u32(0x7000, 0);
    mem.set_u32(signal + 8, 0); // slot 0 has no connections
    mem.set_u32(signal + 0x208, 0); // nor the overflow list
    mem.set_u32(scratch + 0x420, 0x1111);
    mem.set_u32(scratch + 0x424, 0x2222);
    rbr_physics::tyre::layout::emit_noise(&mut mem, &mut NoConnections, scratch);
    assert_eq!(mem.u32(scratch + 0x420), 0x1111);
    assert_eq!(mem.u32(scratch + 0x424), 0);
}

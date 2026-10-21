//! Replays every committed spring-damper call captured from the original 1.02 executable.

mod common;

use common::Call;
use rbr_physics::mem::Mem;
use rbr_physics::spring_damper::{self, layout};
use rbr_physics::Variant;
use std::sync::OnceLock;

const VECTORS: &str = include_str!("vectors/spring-damper.txt");

static CALLS: OnceLock<Vec<Call>> = OnceLock::new();

#[derive(Debug)]
enum ReturnValue {
    Ignore,
    Eax(u32),
    St0(f64),
}

fn calls() -> &'static [Call] {
    CALLS.get_or_init(|| common::parse(VECTORS))
}

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

fn check_spring_windows(call: &Call, mem: &Mem, name: &str) {
    for (k, at, len, _, expected) in &call.windows {
        let actual = mem.read(*at, *len);
        assert_eq!(actual.len(), expected.len());
        for (word, (got, want)) in actual.chunks_exact(4).zip(expected.chunks_exact(4)).enumerate() {
            if got == want {
                continue;
            }
            let got = u32::from_le_bytes(got.try_into().unwrap());
            let want = u32::from_le_bytes(want.try_into().unwrap());
            let both_nan = f32::from_bits(got).is_nan() && f32::from_bits(want).is_nan();
            assert!(both_nan, "{name} call {} window {k} dword {word}: {got:#010x} vs {want:#010x}", call.index);
        }
        let remainder = actual.len() % 4;
        let tail = actual.len() - remainder;
        assert_eq!(&actual[tail..], &expected[tail..], "{name} call {} window {k} tail", call.index);
    }
}

fn replay(addr: u32, mut run: impl FnMut(&Call, &mut Mem) -> ReturnValue) {
    let selected: Vec<_> = calls().iter().filter(|call| call.addr == addr).collect();
    assert!(!selected.is_empty(), "{addr:#x}: no original calls recorded");

    for call in &selected {
        let mut mem = common::map_windows(call);
        match run(call, &mut mem) {
            ReturnValue::Ignore => {}
            ReturnValue::Eax(value) => assert_eq!(
                value, call.eax,
                "{addr:#x} call {} args={:08x?}: eax {value:#010x} vs {:#010x}",
                call.index, call.args, call.eax
            ),
            ReturnValue::St0(value) => {
                let expected = f64::from_bits(call.st0_bits);
                if expected.is_nan() {
                    assert!(
                        value.is_nan(),
                        "{addr:#x} call {} args={:08x?}: st0 {value:?} is not NaN",
                        call.index,
                        call.args
                    );
                } else {
                    assert_eq!(
                        value.to_bits(),
                        call.st0_bits,
                        "{addr:#x} call {} args={:08x?}: st0 {value:?} ({:#018x}) vs {:#018x}",
                        call.index,
                        call.args,
                        value.to_bits(),
                        call.st0_bits
                    );
                }
            }
        }
        check_spring_windows(call, &mem, &format!("{addr:#x} args={:08x?}", call.args));
    }
    eprintln!("{addr:#x}: {} calls match", selected.len());
}

#[test]
fn setup_options_dictionary_matches() {
    replay(0x52d710, |_, _| ReturnValue::Eax(layout::setup_options_dictionary()));
}

#[test]
fn definition_dictionary_matches() {
    replay(0x52d730, |_, _| ReturnValue::Eax(layout::definition_dictionary()));
}

#[test]
fn definition_constructor_matches() {
    replay(0x52d750, |call, mem| ReturnValue::Eax(layout::construct_definition(mem, call.args[0])));
}

#[test]
fn clear_unsigned_value_matches() {
    replay(0x52d960, |call, mem| {
        layout::clear_unsigned_value(mem, call.args[0]);
        ReturnValue::Ignore
    });
}

#[test]
fn setup_dictionary_matches() {
    replay(0x52d9e0, |_, _| ReturnValue::Eax(layout::setup_dictionary()));
}

#[test]
fn set_rebound_damping_matches() {
    replay(0x52da40, |call, mem| {
        layout::set_rebound_damping(mem, call.args[0], call.args[1]);
        ReturnValue::Ignore
    });
}

#[test]
fn state_constructor_matches() {
    replay(0x52da50, |call, mem| ReturnValue::Eax(layout::construct_state(mem, call.args[0], call.args[3])));
}

#[test]
fn copy_setup_to_definition_matches() {
    replay(0x52dbd0, |call, mem| {
        layout::apply_setup(mem, call.args[0], call.args[1]);
        ReturnValue::Ignore
    });
}

#[test]
fn update_forces_matches() {
    replay(0x52dd70, |call, mem| {
        layout::update_forces(mem, call.args[0]);
        ReturnValue::Ignore
    });
}

#[test]
fn signal_emit_boundary_matches() {
    replay(0x52de60, |_, _| {
        layout::emit_signal();
        ReturnValue::Ignore
    });
}

#[test]
fn controller_constructor_matches() {
    replay(0x52df20, |call, mem| {
        ReturnValue::Eax(layout::construct_controller(mem, call.args[0], call.args[1], call.args[2]))
    });
}

#[test]
fn reset_outputs_matches() {
    replay(0x52e010, |call, mem| {
        layout::reset_outputs(mem, call.args[0]);
        ReturnValue::Ignore
    });
}

#[test]
fn damper_step1_matches() {
    replay(0x52e350, |call, _| {
        ReturnValue::St0(spring_damper::step1(
            f(call.args[0]),
            f(call.args[1]),
            f(call.args[2]),
            f(call.args[3]),
            Variant::Stock102,
        ))
    });
}

#[test]
fn damper_step2_matches() {
    replay(0x52e370, |call, _| {
        ReturnValue::St0(spring_damper::step2(f(call.args[0]), f(call.args[1]), f(call.args[2]), Variant::Stock102))
    });
}

#[test]
fn damper_bias_sub_matches() {
    replay(0x52e390, |call, _| ReturnValue::St0(spring_damper::bias_sub(f(call.args[0]), Variant::Stock102)));
}

#[test]
fn damper_accumulate_a_matches() {
    replay(0x52e3a0, |call, mem| {
        layout::accumulate_a(mem, call.args[0], f(call.args[1]), call.args[2]);
        ReturnValue::Ignore
    });
}

#[test]
fn damper_accumulate_b_matches() {
    replay(0x52e3d0, |call, mem| {
        layout::accumulate_b(mem, call.args[0], f(call.args[1]), call.args[2]);
        ReturnValue::Ignore
    });
}

#[test]
fn damper_clamp_matches() {
    replay(0x52e400, |call, mem| ReturnValue::St0(layout::clamp_value(mem, call.args[0], call.args[1]) as f64));
}

#[test]
fn damper_merge_matches() {
    replay(0x52e460, |call, mem| {
        layout::merge_value(mem, call.args[0], call.args[1]);
        ReturnValue::Ignore
    });
}

#[test]
fn damper_update_a_matches() {
    replay(0x52e4c0, |call, mem| {
        layout::update_a(mem, call.args[0], f(call.args[1]));
        ReturnValue::Ignore
    });
}

#[test]
fn damper_update_b_matches() {
    replay(0x52e4f0, |call, mem| {
        layout::update_b(mem, call.args[0], f(call.args[1]));
        ReturnValue::Ignore
    });
}

#[test]
fn damper_update_c_matches() {
    replay(0x52e530, |call, mem| {
        layout::update_c(mem, call.args[0], f(call.args[1]));
        ReturnValue::Ignore
    });
}

#[test]
fn damper_scale_matches() {
    replay(0x52e590, |call, _| ReturnValue::St0(spring_damper::scale(f(call.args[0]), Variant::Stock102)));
}

#[test]
fn damper_sqrt4_matches() {
    replay(0x52e5a0, |call, _| ReturnValue::St0(spring_damper::sqrt4(f(call.args[0]), Variant::Stock102)));
}

#[test]
fn damper_curve_matches() {
    replay(0x52e5e0, |call, _| ReturnValue::St0(spring_damper::curve(f(call.args[0]), Variant::Stock102)));
}

#[test]
fn damper_combine_matches() {
    replay(0x52e630, |call, mem| {
        layout::combine(mem, call.args[0], call.args[1], f(call.args[2]), Variant::Stock102);
        ReturnValue::Ignore
    });
}

#[test]
fn damper_relax_matches() {
    replay(0x52e6d0, |call, mem| {
        layout::relax(mem, call.args[0], f(call.args[1]), f(call.args[2]), Variant::Stock102);
        ReturnValue::Ignore
    });
}

#[test]
fn damper_solve_matches() {
    replay(0x52e760, |call, mem| {
        ReturnValue::St0(layout::solve(
            mem,
            call.args[0],
            f(call.args[1]),
            f(call.args[2]),
            f(call.args[3]),
            Variant::Stock102,
        ) as f64)
    });
}

#[test]
fn damper_force_matches() {
    replay(0x52e8a0, |call, mem| {
        layout::force(mem, call.args[0], f(call.args[1]), f(call.args[2]), f(call.args[3]));
        ReturnValue::Ignore
    });
}

#[test]
fn recorded_helper_branches_are_complete() {
    let cases: &[(u32, &[&str])] = &[
        (
            0x52e090,
            &[
                "direct_positive_within_stops",
                "direct_negative_within_stops",
                "compound_negative_bump_nonpositive",
                "direct_negative_droop_negative",
                "direct_positive_droop_positive",
                "compound_negative_fast_bump_nonpositive",
                "direct_negative_fast_droop_negative",
                "compound_positive_bump_nonpositive",
                "direct_positive_bump_nonpositive",
                "compound_positive_droop_positive",
                "direct_negative_fast_bump_nonpositive",
                "compound_positive_within_stops",
                "compound_negative_droop_negative",
                "compound_negative_within_stops",
                "direct_negative_bump_nonpositive",
                "direct_positive_bump_positive",
                "compound_negative_fast_droop_negative",
                "direct_negative_fast_within_stops",
                "compound_negative_fast_within_stops",
            ],
        ),
        (0x52e400, &["clamp", "pass"]),
        (0x52e460, &["sum", "take_a", "weighted"]),
        (0x52e4c0, &["update", "zero"]),
        (0x52e4f0, &["update", "zero"]),
        (0x52e530, &["update", "zero"]),
        (0x52e5a0, &["nonpositive", "positive"]),
        (0x52e5e0, &["at_or_above_one", "below_one"]),
        (0x52e6d0, &["relax", "reset"]),
        (0x52e760, &["clear", "idle", "relax", "release"]),
    ];
    for &(addr, expected) in cases {
        let actual: std::collections::BTreeSet<_> =
            calls().iter().filter(|call| call.addr == addr).map(|call| call.branch.as_str()).collect();
        for branch in expected {
            assert!(actual.contains(branch), "{addr:#x}: fixture has no `{branch}` branch");
        }
    }
}

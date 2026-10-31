//! Replays the original's `CCar::Sampler54A0` (0x4e54a0) and `CDrive::M6840` (0x506840) calls (`harness/vector_specs/
//! g_vf98.py`, recorded with `harness/direct_calls.py --set g-vf98`) through the ports. Needs record files:
//! `RBR_G_VF98_VECTORS=<file>[:<file>...]`. Without the variable the tests are skipped.
//!
//! The sampler's vote (0x4fe810) is answered from its recorded eax, and its raw block takes the recorded post bytes (the
//! vote writes the hit point there, which the port then copies). M6840 checks the commanded gear it stores.

mod common;

use common::{load, map_windows, Call, Inner};
use rbr_physics::car_body::layout::{self, Sampler54Calls};
use rbr_physics::mem::Mem;
use rbr_physics::vcu::gear;
use rbr_physics::vcu::store::VcuStore;

const VAR: &str = "RBR_G_VF98_VECTORS";
const VOTE: u32 = 0x4fe810;
const M6840: u32 = 0x506840;
/// The scratch frame the port writes below its entry: mapped, zero.
const ENTRY: u32 = 0x0070_0400;

/// Answers the sampler's vote from its one recorded inner call. The raw block (window 2) takes its recorded post bytes.
struct RecordedVote<'a> {
    call: &'a Call,
    votes: usize,
}

impl Sampler54Calls for RecordedVote<'_> {
    fn vote(&mut self, mem: &mut Mem, _sp: u32, _probe_a: u32, _probe_b: u32, raw: u32) -> u32 {
        let inner: &Inner = self
            .call
            .inner
            .iter()
            .filter(|c| c.addr == VOTE)
            .nth(self.votes)
            .unwrap_or_else(|| panic!("Sampler54A0 call {}: the port voted more often", self.call.index));
        self.votes += 1;
        for k in 0..20 {
            mem.set_u32(raw + 4 * k, post_word(self.call, 2, 4 * k as usize));
        }
        inner.eax
    }
}

/// The window's recorded post bytes at `offset` (window `k` of the root call), as a little-endian word.
fn post_word(call: &Call, k: usize, offset: usize) -> u32 {
    let post = &call.windows[k].after;
    u32::from_le_bytes(post[offset..offset + 4].try_into().unwrap())
}

#[test]
fn sampler_54a0_matches_original_calls() {
    let Some(calls) = load(VAR, 0x4e54a0) else {
        eprintln!("skipping Sampler54A0: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "Sampler54A0: no calls in the vector files");
    for call in &calls {
        let mut mem = map_windows(call);
        mem.map(ENTRY - 0x400, &[0; 0x400]);
        let (a, b, c) = (call.args[1], call.args[2], call.args[3]);
        let mut votes = RecordedVote { call, votes: 0 };
        let eax = layout::vote_probe(&mut mem, &mut votes, ENTRY, a, b, c);
        assert_eq!(eax, call.eax, "Sampler54A0 call {}: eax", call.index);
        assert_eq!(
            votes.votes,
            call.inner.iter().filter(|i| i.addr == VOTE).count(),
            "Sampler54A0 call {}: votes",
            call.index
        );
        for k in 0..4 {
            assert_eq!(
                mem.u32(b + 4 * k),
                post_word(call, 1, 4 * k as usize),
                "Sampler54A0 call {}: answer",
                call.index
            );
        }
    }
}

#[test]
fn m6840_stores_the_commanded_gear() {
    let Some(calls) = load(VAR, M6840) else {
        eprintln!("skipping M6840: {VAR} not set");
        return;
    };
    assert!(!calls.is_empty(), "M6840: no calls in the vector files");
    for call in &calls {
        let mem = map_windows(call);
        let (drive, value) = (call.args[0], call.args[1]);
        let count = mem.gear_count(drive);
        assert_eq!(
            gear::commanded_gear(count, value),
            post_word(call, 1, 0x570),
            "M6840 call {}: commanded gear",
            call.index
        );
    }
}

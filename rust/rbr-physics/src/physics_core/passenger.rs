//! The passenger (`CPassenger`, `src/rbr/RBR_E45_PassengerA.cpp`, `RBR_E45_PassengerB.cpp`): the constructors of its
//! def (`CtorA` 0x522450, `CtorB` 0x5224a0), its state (`CState::CtorB`, 0x522550) and its body (`Ctor`, 0x5230d0).
//! The bases, tweaks and vectors the constructors build are the caller's (`PassengerHost`). Its flag test
//! (`flag_67c_set`, 0x5233f0), its incar shake (`incar_shake`, 0x523210, which emits through the Emit at 0x523010,
//! `signal::emit`), its time decode (`decode_time`, 0x523360) and its spring (`spring`, 0x5229e0) are here; the spring
//! writes lane 3 from stack garbage, which the port does not reproduce.

use super::math::Vec4;
use super::{layout, signal};
use crate::math::ftol::ftol2;
use crate::mem::Mem;
use crate::ntweak::layout::BASE_VTABLE;

/// The `CBase` name the constructors pass (0x73e824).
const BASE_NAME: u32 = 0x0073_e824;
/// The vtables the constructors store: the def's (0x73e7d4), the state's (0x73e834), the body's (0x73e890) and its
/// scratch's (0x73e884, at +0x20 of the body).
const DEF_VTABLE: u32 = 0x0073_e7d4;
const STATE_VTABLE: u32 = 0x0073_e834;
const BODY_VTABLE: u32 = 0x0073_e890;
const SCRATCH_VTABLE: u32 = 0x0073_e884;
/// The state's vectors (`E09Vec4`), cleared in this order by `CState::CtorB`.
const STATE_VECTORS: [u32; 10] = [0x70, 0x80, 0x90, 0xa0, 0xb0, 0x20, 0x30, 0x40, 0x50, 0x60];

/// The calls the passenger constructors make into code outside this port, in the original's order. Each gets the
/// address of the object it builds.
pub trait PassengerHost {
    /// `NTweak::CBase::CBase(name)` (0x52f090) on the base at `at`.
    fn base(&mut self, mem: &mut Mem, at: u32, name: u32);
    /// `NTweak::CUnsigned::CUnsigned` (0x52fec0) on the tweak at `at`.
    fn unsigned(&mut self, mem: &mut Mem, at: u32);
    /// `NTweak::CReal::CReal` (0x52fea0) on the tweak at `at`.
    fn real(&mut self, mem: &mut Mem, at: u32);
    /// `E09Vec4::Clear` (0x465d80) on the vector at `at`.
    fn clear(&mut self, mem: &mut Mem, at: u32);
    /// The scratch's `Vf04` (vtable slot 1, 0x5224e0) on the scratch at `at`.
    fn scratch_vf04(&mut self, mem: &mut Mem, at: u32);
}

/// `CPassenger::CDef::CtorA` (0x522450) on the def at `at`: its base, its vptr, then its unsigned tweak at +0xc.
pub fn construct_def(mem: &mut Mem, host: &mut impl PassengerHost, at: u32) {
    host.base(mem, at, BASE_NAME);
    layout::set_word(mem, at, DEF_VTABLE);
    host.unsigned(mem, at + 0xc);
}

/// `CPassenger::CDef::CtorB` (0x5224a0) on the def at `at`: its base's and tweak's words are demoted (stores only).
pub fn demote_def(mem: &mut Mem, at: u32) {
    layout::set_word(mem, at + 0xc, BASE_VTABLE);
    layout::set_word(mem, at + 0x10, 0);
    layout::set_word(mem, at, BASE_VTABLE);
    layout::set_word(mem, at + 4, 0);
}

/// `CPassenger::CState::CtorB` (0x522550) on the state at `at`: its base, its vptr, its real tweak at +0xc, and ten
/// cleared vectors. The three arguments are not read.
pub fn construct_state(mem: &mut Mem, host: &mut impl PassengerHost, at: u32) {
    host.base(mem, at, BASE_NAME);
    layout::set_word(mem, at, STATE_VTABLE);
    host.real(mem, at + 0xc);
    for off in STATE_VECTORS {
        host.clear(mem, at + off);
    }
}

/// `CPassenger::Ctor(a, b, c)` (0x5230d0) on the body at `this`: its vptr and its scratch's vptr (+0x20), then `a`
/// and `b` (at +0x10 and +0x14) and `c` (+0x40). The scratch's `Vf04` runs when `a` is set.
pub fn construct_body(mem: &mut Mem, host: &mut impl PassengerHost, this: u32, a: u32, b: u32, c: u32) {
    layout::set_word(mem, this, BODY_VTABLE);
    layout::set_word(mem, this + 0x20, SCRATCH_VTABLE);
    layout::set_word(mem, this + 0x14, b);
    layout::set_word(mem, this + 0x10, a);
    layout::set_word(mem, this + 0x40, c);
    if a != 0 {
        host.scratch_vf04(mem, this + 0x20);
    }
}

/// The body's host pointer (`this + 0x14`).
const HOST_AT: u32 = 0x14;
/// The velocity vector of the spring (`this + 0x30`, three floats).
const VELOCITY_AT: u32 = 0x30;
/// The incar shake signal's offset in the signal manager (`g_signalManager + 0xc6e8`).
const INCAR_SHAKE_AT: u32 = 0xc6e8;
/// The payload's words copied from the host: (payload word, host offset, count), in the original's order. `Clear6` zeroes
/// the vectors at +0x10..+0x70 first, so word 24 stays zero; the host's +0xbc goes to word 23.
const PAYLOAD_FROM_HOST: [(u32, u32, u32); 5] =
    [(4, 0x90, 4), (8, 0x80, 4), (12, 0x70, 4), (16, 0xa0, 7), (23, 0xbc, 1)];
/// `CTimeDecode` constants: the seconds of a day, of an hour and of a minute.
const SECS_PER_DAY: u32 = 0x15180;
const SECS_PER_HOUR: u32 = 0xe10;
const SECS_PER_MINUTE: u32 = 0x3c;
/// The time decode's fraction (`this + 0x10`) and the sign (`this`).
const FRACTION_AT: u32 = 0x10;

/// The calls the passenger's flag test, incar shake and time decode make outside this port, in the original's order.
pub trait ShakeHost: signal::SignalHost {
    /// `E37Get67C` (0x4f7e40, ecx): the word at +0x67c of the object `obj`.
    fn flag_67c(&mut self, mem: &mut Mem, obj: u32) -> u32;
    /// `E20Clear6::Clear6` (0x49fea0) on the payload at `at`: six vectors cleared.
    fn clear6(&mut self, mem: &mut Mem, at: u32);
}

/// `E45_P33F0` (0x5233f0, ecx): whether the object `obj` is set, that is, it is not null and its `E37Get67C` flag is.
pub fn flag_67c_set(mem: &mut Mem, host: &mut impl ShakeHost, obj: u32) -> bool {
    obj != 0 && host.flag_67c(mem, obj) != 0
}

/// `E45_P3210` (0x523210, ecx = the body `this`): clears the payload at `pay` (`Clear6`), fills it from the body's host
/// and emits it on the incar shake signal (`signal::emit`, the manager's `+0xc6e8`).
pub fn incar_shake(mem: &mut Mem, host: &mut impl ShakeHost, this: u32, pay: u32) {
    host.clear6(mem, pay);
    let body_host = layout::word(mem, this + HOST_AT);
    layout::set_word(mem, pay, 0);
    for (word, from, count) in PAYLOAD_FROM_HOST {
        for k in 0..count {
            let value = layout::word(mem, body_host + from + 4 * k);
            layout::set_word(mem, pay + 4 * (word + k), value);
        }
    }
    let signal = layout::signal_manager(mem) + INCAR_SHAKE_AT;
    signal::emit(mem, host, signal, pay);
}

/// `E45PSmalls::M3360` (0x523360, thiscall, `v` the double argument): the sign of `v` (+0), then its magnitude split into
/// days (+4), hours (+8), minutes (+0xc) and, with the fraction, the seconds (float at +0x10).
pub fn decode_time(mem: &mut Mem, this: u32, v: f64) {
    let (sign, x) = if 0.0 <= v { (1, v) } else { (-1i32, -v) };
    layout::set_word(mem, this, sign as u32);
    let t = ftol2(x) as u32;
    layout::set_f32(mem, this + FRACTION_AT, (x - t as f64) as f32);
    layout::set_word(mem, this + 4, t / SECS_PER_DAY);
    let r1 = t % SECS_PER_DAY;
    layout::set_word(mem, this + 8, r1 / SECS_PER_HOUR);
    let r2 = r1 % SECS_PER_HOUR;
    layout::set_word(mem, this + 0xc, r2 / SECS_PER_MINUTE);
    let seconds = (r2 % SECS_PER_MINUTE) as f32;
    let fraction = layout::f32(mem, this + FRACTION_AT) + seconds;
    layout::set_f32(mem, this + FRACTION_AT, fraction);
}

/// `E45PSmalls::M29E0` (0x5229e0, thiscall, `dt`): the spring of the three lanes. With `k = dt * -0.1`, each lane is
/// `f = 900*(h8 - h16) + 30000*(h28 - h36) + 10*v`, `h8 += f*k`, `h28 += h8*dt` (the host's words `h[i]` at +4i).
pub fn spring(mem: &mut Mem, this: u32, dt: f32) {
    let host = layout::word(mem, this + HOST_AT);
    let k = dt * -0.100000001;
    for lane in 0..3 {
        let h8 = host + 4 * (8 + lane);
        let h16 = host + 4 * (16 + lane);
        let h28 = host + 4 * (28 + lane);
        let h36 = host + 4 * (36 + lane);
        let v = this + VELOCITY_AT + 4 * lane;
        let f = 900.0 * (layout::f32(mem, h8) - layout::f32(mem, h16))
            + 30000.0 * (layout::f32(mem, h28) - layout::f32(mem, h36));
        let f = 10.0 * layout::f32(mem, v) + f;
        let h8_next = f * k + layout::f32(mem, h8);
        layout::set_f32(mem, h8, h8_next);
        let h28_next = h8_next * dt + layout::f32(mem, h28);
        layout::set_f32(mem, h28, h28_next);
    }
}

/// The dead band of `M2BC0`, lanes 0-2: `h90` is pulled towards zero by `DEAD_POS` above it and `DEAD_NEG` below.
const DEAD_POS: [f32; 3] = [0.005, 0.01, 0.01];
const DEAD_NEG: [f32; 3] = [-0.005, -0.02, -0.01];
/// The gains of `M2BC0`: `K1` on `h40` and `K2` on the dead-banded `h90` (the pull), `K3` on `h90` and `K4` on `h40`
/// (the spring).
const K1: [f32; 3] = [800.0, 900.0, 900.0];
const K2: [f32; 3] = [20000.0, 60000.0, 50000.0];
const K3: [f32; 3] = [20000.0, 35000.0, 35000.0];
const K4: [f32; 3] = [300.0, 500.0, 400.0];
/// The passenger's velocity gain, and the rate `dt * RATE` that moves `h40` by the work vector.
const VELOCITY_GAIN: f32 = 30.0;
const RATE: f32 = -1.0 / 30.0;

/// The dead band of one lane: the part of `h` past its zone on its own side of zero, 0 inside the zone.
fn dead_band(h: f32, pos: f32, neg: f32) -> f32 {
    if h >= 0.0 {
        let t = h - pos;
        if t > 0.0 {
            t
        } else {
            0.0
        }
    } else {
        let t = h - neg;
        if t < 0.0 {
            t
        } else {
            0.0
        }
    }
}

/// -1, 0 or 1: the sign of `x`.
fn sign(x: f32) -> i32 {
    i32::from(x > 0.0) - i32::from(x < 0.0)
}

fn lanes(v: Vec4) -> [f32; 4] {
    [v.x, v.y, v.z, v.w]
}

fn vec4(l: [f32; 4]) -> Vec4 {
    Vec4 { x: l[0], y: l[1], z: l[2], w: l[3] }
}

/// `E45PSmalls::M2BC0` (0x522bc0): the pull on the host's `h40` and `h90` vectors, given the passenger's velocity `v`
/// (its +0x30) and `dt`. Lanes 0-2 as the original. The work vector's lane 3 is 0: the original's is stack garbage
/// (`harness/diverge.py` masks the lane-3 words it reaches). Returns the new `h40` and `h90`.
pub fn pull_to_rest(h40: Vec4, h90: Vec4, v: Vec4, dt: f32) -> (Vec4, Vec4) {
    let (h40, h90, v) = (lanes(h40), lanes(h90), lanes(v));
    let mut work = [0.0f32; 4];
    for i in 0..3 {
        let a = dead_band(h90[i], DEAD_POS[i], DEAD_NEG[i]);
        let pull = K1[i] * h40[i] + K2[i] * a;
        let pull = if sign(pull) == sign(a) { pull } else { 0.0 };
        let spring = (pull + K3[i] * h90[i]) + K4[i] * h40[i];
        work[i] = v[i] * VELOCITY_GAIN + spring;
    }
    let rate = dt * RATE;
    let h40: [f32; 4] = std::array::from_fn(|i| work[i] * rate + h40[i]);
    let h90: [f32; 4] = std::array::from_fn(|i| h40[i] * dt + h90[i]);
    (vec4(h40), vec4(h90))
}

/// The point `CPassenger::Step` walks: the gravity of the physics object plus the host's offset, lane by lane.
pub fn walk_point(gravity: [f32; 3], offset: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| gravity[i] + offset[i])
}

/// The calls `CPassenger::Step` makes into code outside this port, in the original's order.
pub trait StepHost {
    /// `E43PairWalk::MB560` (0x51b560) on the walker: walks `point` (kept in memory for the call) against the host
    /// `host` and writes `out`.
    fn walk(&mut self, point: [f32; 3], host: u32, out: u32);
    /// `E45PSmalls::M29E0` (0x5229e0) on the passenger at `this`.
    fn spring(&mut self, this: u32, dt: f32);
    /// `E45PSmalls::M2BC0` (0x522bc0) on the passenger at `this`; returns the eax the original leaves.
    fn pull(&mut self, this: u32, dt: f32) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host that records the scratch's `Vf04` calls and makes no other writes.
    #[derive(Default)]
    struct Vf04Calls(Vec<u32>);

    impl PassengerHost for Vf04Calls {
        fn base(&mut self, _: &mut Mem, _: u32, _: u32) {}

        fn unsigned(&mut self, _: &mut Mem, _: u32) {}

        fn real(&mut self, _: &mut Mem, _: u32) {}

        fn clear(&mut self, _: &mut Mem, _: u32) {}

        fn scratch_vf04(&mut self, _: &mut Mem, at: u32) {
            self.0.push(at);
        }
    }

    #[test]
    fn scratch_runs_only_when_a_is_set() {
        let mut mem = Mem::new();
        mem.map(0x1000, &[0; 0x44]);
        let mut host = Vf04Calls::default();
        construct_body(&mut mem, &mut host, 0x1000, 0, 0x55, 0x66);
        assert!(host.0.is_empty());
        construct_body(&mut mem, &mut host, 0x1000, 0x77, 0x55, 0x66);
        assert_eq!(host.0, [0x1020]);
    }

    #[test]
    fn dead_band_keeps_only_the_part_past_its_zone() {
        assert_eq!(dead_band(0.003, 0.005, -0.005), 0.0);
        assert_eq!(dead_band(-0.003, 0.005, -0.005), 0.0);
        assert!((dead_band(0.1, 0.005, -0.005) - 0.095).abs() < 1e-6);
        assert!((dead_band(-0.1, 0.005, -0.005) + 0.095).abs() < 1e-6);
    }
}

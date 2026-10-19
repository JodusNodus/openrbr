//! NGP's rewrite of `CDrive::M93A0` (`0x10047300`, `clutch_torque` under `Variant::Ngp778`) against direct calls of
//! the original in the emulator (`scripts/ngp-clutch-torque-vectors.py` on `cov-rpl11.snap-9520`): 3,000 fuzzed calls
//! on the paths without callees.

use rbr_physics::engine_driveline::drive_worker::{clutch_torque, GearHost};
use rbr_physics::engine_driveline::layout::with_gear;
use rbr_physics::mem::Mem;
use rbr_physics::Variant;

const THIS: u32 = 0x1000_0000;
const DEF: u32 = 0x2000_0000;
const STATE: u32 = 0x3000_0000;
const OBJ: u32 = 0x4000_0000;
/// The world (`0x1659b70`) and its options (`+0x24`), whose flags the load reads: all clear.
const WORLD: u32 = 0x5000_0000;
const OPTIONS: u32 = 0x6000_0000;
const UNWRITTEN: u32 = 0xdead_beef;

struct NoCalls;

impl GearHost for NoCalls {
    fn car_get30(&mut self, _: u32) -> u32 {
        panic!("a fuzzed path makes no calls")
    }
    fn emit_gear_change(&mut self, _: [u32; 4]) {
        panic!("a fuzzed path makes no calls")
    }
}

#[test]
fn clutch_torque_matches_ngp_calls() {
    let mut n = 0;
    for line in include_str!("vectors/ngp-clutch-torque.txt").lines() {
        let w: Vec<u32> = line.split(' ').map(|x| u32::from_str_radix(x, 16).unwrap()).collect();
        let mut mem = Mem::new();
        for (at, len) in
            [(THIS, 0x400), (DEF, 0x2000), (STATE, 0x2000), (OBJ, 0x400), (WORLD, 0x100), (OPTIONS, 0x16100)]
        {
            mem.map(at, &vec![0; len]);
        }
        mem.map(0x0165_9b70, &WORLD.to_le_bytes());
        mem.set_u32(WORLD + 0x24, OPTIONS);
        mem.set_u32(THIS + 4, DEF);
        mem.set_u32(THIS + 8, STATE);
        mem.set_u32(THIS + 0x170, OBJ);
        mem.set_u32(DEF + 0x694, w[0]);
        mem.set_u32(STATE + 0x55c, w[1]);
        mem.set_u32(STATE + 0x368, w[2]);
        mem.set_u32(STATE + 0x578, w[3]);
        mem.set_u32(STATE + 0x45c, w[4]);
        mem.set_u32(STATE + 0x384, w[5]);
        mem.set_u32(DEF + 0x664, w[6]);
        mem.set_u32(DEF + 0x674, w[7]);
        let f = |k: usize| f32::from_bits(w[k]);
        let out = with_gear(&mut mem, THIS, |wk, g| {
            clutch_torque(wk, g, &mut NoCalls, f(8), [f(9), f(10), f(11)], Variant::Ngp778)
        });
        let got = [
            out.map_or(UNWRITTEN, f32::to_bits),
            mem.u32(STATE + 0x45c),
            mem.u32(STATE + 0x368),
            mem.u32(STATE + 0x578),
        ];
        assert_eq!(got, w[12..16], "{line}");
        n += 1;
    }
    assert_eq!(n, 3000);
}

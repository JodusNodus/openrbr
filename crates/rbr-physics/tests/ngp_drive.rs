//! NGP's `CDrive::M5FE0` and `M6260` rewrites (`0x100458c0`, `0x10045b80`; `ngp::drive`) against direct runs of the
//! original in the emulator (`scripts/ngp-drive-vectors.py` on `cov-rpl11.snap-9520`): 1,500 fuzzed runs each.

use rbr_physics::engine_driveline::layout::with_worker;
use rbr_physics::mem::Mem;
use rbr_physics::ngp::drive::{wheel_latch, wheel_slip_value};

const THIS: u32 = 0x1000_0000;
const DEF: u32 = 0x2000_0000;
const STATE: u32 = 0x3000_0000;
const OBJ: u32 = 0x4000_0000;
/// The world (`0x1659b70`) and its options (`+0x24`), whose flags the load reads: all clear.
const WORLD: u32 = 0x5000_0000;
const OPTIONS: u32 = 0x6000_0000;

#[test]
fn drive_workers_match_original_runs() {
    let mut n = 0;
    for line in include_str!("vectors/ngp-drive.txt").lines() {
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
        let (x, pack) = (w[1], w[3]);
        let (wheel, axle, blk) = (DEF + 0xf0 * x, DEF + 0x60 * pack, STATE + 0xc8 * x);
        mem.set_u32(wheel + 0x148, w[4]);
        mem.set_u32(wheel + 0x158, pack);
        mem.set_u32(axle + 0x18, w[5]);
        mem.set_u32(axle + 0x48, w[6]);
        mem.set_u32(DEF + 0x48, w[7]);
        mem.set_u32(blk + 0xa4, w[8]);
        mem.set_u32(blk + 0xf8, w[9]);
        mem.set_u32(OBJ + 0x18 + 12 * x, w[10]);
        mem.set_u32(THIS + 0xf4 + 16 * x, w[11]);
        mem.set_u32(STATE + 0x1c, w[12]);
        mem.set_u32(STATE + 0x1c + 20 * pack, w[13]);
        mem.set_u32(STATE + 0x37c, w[14]);
        mem.set_u32(STATE + 0x460, w[15]);
        mem.set_u32(THIS + 0x90, w[16]);
        mem.set_u32(STATE + 0x5bc, w[17]);
        mem.set_u32(STATE + 0x5a4, w[18]);
        mem.set_u32(STATE + 0x530 + 4 * x, 0xdead_beef);
        mem.set_u32(THIS + 0xc0 + 4 * x, 0xdead_beef);
        let split = f32::from_bits(w[2]);
        let x = x as usize;
        match w[0] {
            0x1004_58c0 => with_worker(&mut mem, THIS, |wk| wheel_slip_value(wk, x, split)),
            0x1004_5b80 => with_worker(&mut mem, THIS, |wk| wheel_latch(wk, x, split)),
            other => panic!("{other:#x}"),
        }
        let got = [
            mem.u32(STATE + 0x530 + 4 * x as u32),
            mem.u32(THIS + 0xc0 + 4 * x as u32),
            mem.u32(blk + 0xa4),
            mem.u32(blk + 0xa0),
            mem.u32(STATE + 0x5bc),
            mem.u32(STATE + 0x5a4),
        ];
        // a NaN's sign is not compared (as rust_step's classify: the emulator and arm64 differ from x86 there)
        let nan = |b: u32| f32::from_bits(b).is_nan();
        let same = got[0] == w[19] || (nan(got[0]) && nan(w[19]));
        assert!(same && got[1..] == w[20..25], "{line}: {got:08x?}");
        n += 1;
    }
    assert_eq!(n, 3000);
}

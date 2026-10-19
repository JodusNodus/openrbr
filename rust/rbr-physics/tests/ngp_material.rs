//! NGP's material sheet lookup (`0x1004df40`, `ngp::material::sample_slot`) against direct calls of the original in the
//! emulator (`scripts/ngp-sheet-vectors.py` on `cov-rpl11.snap-9520`): 3,000 fuzzed calls on a sheet whose byte `i`
//! is `i`.

use rbr_physics::mem::Mem;
use rbr_physics::ngp::material::sample_slot;

const SLOT: u32 = 0x1000;
const DATA: u32 = 0x2000;

#[test]
fn sheet_lookup_matches_original_calls() {
    let mut mem = Mem::new();
    mem.map(DATA, &(0..=255).collect::<Vec<u8>>());
    let mut n = 0;
    for line in include_str!("vectors/ngp-sheet.txt").lines() {
        let w: Vec<u32> = line.split(' ').map(|x| u32::from_str_radix(x, 16).unwrap()).collect();
        mem.set_u32(SLOT + 4, if w[0] != 0 { DATA } else { 0 });
        let got = sample_slot(&mem, SLOT, f32::from_bits(w[1]), f32::from_bits(w[2]));
        assert_eq!(got, w[3], "{line}");
        n += 1;
    }
    assert_eq!(n, 3000);
}

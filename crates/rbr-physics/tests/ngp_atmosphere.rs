//! NGP's atmosphere against direct calls of the original in the emulator (`scripts/ngp-atmosphere-vectors.py` on
//! `snap1.snap-10880`): 2,007 heights from -500 to 4000 m.

use rbr_physics::ngp::atmosphere::atmosphere;

#[test]
fn atmosphere_matches_original_calls() {
    let Some(tables) = rbr_formats::ngp::current() else {
        eprintln!("skipping: NGP.dll not configured (RBR_NGP_DLL or RBR_NGP)");
        return;
    };
    let mut n = 0;
    for line in include_str!("vectors/ngp-atmosphere.txt").lines() {
        let w: Vec<u32> = line.split(' ').map(|x| u32::from_str_radix(x, 16).unwrap()).collect();
        let a = atmosphere(f32::from_bits(w[0]), &tables.atmosphere);
        let got =
            [a.temperature, a.pressure, a.density, a.inv_pressure, a.pressure_115, a.pressure_0987].map(f32::to_bits);
        assert_eq!(got, w[1..7], "h = {}", f32::from_bits(w[0]));
        n += 1;
    }
    assert_eq!(n, 2007);
}

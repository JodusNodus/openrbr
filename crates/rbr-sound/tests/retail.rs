//! The retail sound data every model plays (`RBR_DATA`, and `RBR_PATCH` for the 1.02 `audio.ini`): each car's bank
//! loads, with the tyre, rumble, wetness, gravel-hit, off-road, impact, suspension, exhaust, fire and countdown samples and the co-driver's English words, numbers and damage
//! calls decoded; the scripted events' samples; the service park's banks.

use rbr_formats::data::Data;
use rbr_sound::data::CarBank;
use rbr_sound::menu::{MenuBank, MenuMixer};

#[test]
fn every_car_bank_loads_with_the_codriver() {
    let Some(data) = Data::from_env() else {
        eprintln!("RBR_DATA is not set; skipped");
        return;
    };
    for car in 0..8 {
        let bank = CarBank::load(&data, car).unwrap_or_else(|e| panic!("car {car}: {e}"));
        assert!(bank.surfaces.iter().flatten().all(|w| !w.samples.is_empty()), "car {car}: a surface loop is empty");
        assert_eq!((bank.offroad.len(), bank.ranged.len(), bank.screams.len()), (7, 20, 2));
        assert!(!bank.go.samples.is_empty());
        let counts =
            [&bank.grus, &bank.squeaks, &bank.bottomings, &bank.exhaust_pops, &bank.pops, &bank.bangs].map(Vec::len);
        assert_eq!(counts, [20, 18, 4, 11, 11, 6], "car {car}: effect banks");
        assert!([&bank.rumble, &bank.wetness, &bank.fire].iter().all(|w| !w.samples.is_empty()));
        let words = &bank.codriver.words;
        // Every word id but PLUS and MINUS (no takes in the archive) has at least one take.
        let silent: Vec<u32> = words.iter().filter(|(_, takes)| takes.is_empty()).map(|(&id, _)| id).collect();
        assert_eq!(silent.len(), 2, "car {car}: silent words {silent:x?}");
        assert!(words.len() >= 54);
        assert_eq!(bank.codriver.numbers.len(), 15);
        assert!(bank.codriver.damage[1..].iter().all(|w| w.rate > 0 && !w.samples.is_empty()));
        assert!(!bank.helicopter.samples.is_empty() && !bank.bird_hit.samples.is_empty());
    }
}

#[test]
fn the_service_park_plays() {
    let Some(data) = Data::from_env() else {
        eprintln!("RBR_DATA is not set; skipped");
        return;
    };
    let bank = MenuBank::load(&data).expect("menu bank");
    assert_eq!(bank.park.iter().map(Vec::len).collect::<Vec<_>>(), [1; 6]);
    let mut mixer = MenuMixer::new(bank, 44100, 1.0);
    mixer.set_park(true);
    let mut out = vec![[0.0; 2]; 44100 * 2];
    mixer.render(&mut out);
    let rms = (out[44100..].iter().map(|f| f[0] * f[0]).sum::<f32>() / 44100.0).sqrt();
    assert!(rms > 0.01, "rms {rms}");
}

#[test]
fn one_shots_are_refused_at_sixteen_sounds_and_ranged_impacts_steal() {
    use rbr_sound::mixer::{Mixer, SOUND_LIMIT};
    use rbr_sound::voice::{Command, Shot};
    let Some(data) = Data::from_env() else {
        eprintln!("RBR_DATA is not set; skipped");
        return;
    };
    let bank = CarBank::load(&data, 0).expect("car 0");
    let mut mixer = Mixer::new(&bank, 44100, 1.0);
    let squeak = |i| Command::Shot { shot: Shot::Squeak(i), gain: 1.0 };
    // The requests until a frame all see its count: 9 sounds always count, so all 9 squeaks are made.
    mixer.apply(&(0..9).map(squeak).collect::<Vec<_>>());
    mixer.render(&mut [[0.0; 2]; 64]);
    mixer.apply(&[Command::Frame]);
    mixer.apply(&[squeak(0), Command::Shot { shot: Shot::Ranged(0), gain: 1.0 }]);
    assert_eq!(mixer.tally()["Squeak"], [9, 1]);
    assert_eq!(mixer.tally()["Ranged"], [1, 0]);
    assert_eq!(SOUND_LIMIT, 16);
}

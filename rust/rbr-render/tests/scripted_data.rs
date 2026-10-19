//! The scripted events on retail data (`RBR_DATA`) and a recording (`RBR_RECORDINGS`): the car-flip spectators at
//! `drive-rcv-help-corolla-34`'s recovery, and on `drive-v6-gravel-206-64` (skipped when that recording is missing) the
//! rescue helicopter and forced bird strikes. The helicopter needs the longer drive: it lands 19.6 s after the call.

use glam::Vec3;
use rbr_formats::data::Data;
use rbr_render::characters::load::Options;
use rbr_render::characters::scripted::Event;
use rbr_render::characters::Characters;
use rbr_render::playback::Drive;
use rbr_render::stage;
use std::path::PathBuf;

fn setup(name: &str, drive: &str, options: Options) -> Option<(Characters, Drive)> {
    let data = Data::from_env()?;
    let recordings = PathBuf::from(std::env::var_os("RBR_RECORDINGS")?);
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, name, Default::default()).expect("stage");
    let characters = Characters::load_with(&data, name, &mut stage, options).expect("characters");
    let bytes = std::fs::read(recordings.join(format!("{drive}.bin"))).ok()?;
    Some((characters, Drive::parse(&bytes).expect("drive")))
}

/// The events of a drive's frames, with the rescue called on step `call`; the helicopter's distance to its landing point
/// is checked on step `landed`.
fn events(characters: &mut Characters, drive: &Drive, call: u32, landed: u32) -> Vec<(u32, Event)> {
    let mut events = Vec::new();
    for (end, sample) in drive.frames() {
        if sample.step == call {
            characters.call_rescue();
        }
        let car = characters.on_line(sample);
        characters.step(end, car);
        events.extend(characters.take_events().into_iter().map(|e| (sample.step, e)));
        if sample.step == landed {
            // Landed by now: its body is at the landing point.
            let heli = characters.rescue_position().expect("the helicopter runs");
            let landing = events.iter().find_map(|(_, e)| if let Event::RescueCalled(p) = e { Some(*p) } else { None });
            assert!(heli.distance(landing.expect("called")) < 1.0, "{heli} {landing:?}");
        }
    }
    events
}

#[test]
fn the_flip_spectators_follow_the_recovery() {
    let Some((mut characters, drive)) = setup("track-34_E", "drive-rcv-help-corolla-34", Options::default()) else {
        return;
    };
    let events = events(&mut characters, &drive, 700, u32::MAX);
    let flips: Vec<_> =
        events.iter().filter(|(_, e)| matches!(e, Event::FlipStart(_) | Event::FlipEnd)).copied().collect();
    // The emit is on the frame that ends at post 1957; the HUD time is the drive's (7 ms a step): 1 s later, 3.5 s on.
    assert_eq!(flips, [(2099, Event::FlipStart(1)), (2599, Event::FlipEnd)], "{events:?}");
    assert!(events.iter().any(|(step, e)| *step == 700 && matches!(e, Event::RescueCalled(_))));
}

#[test]
fn the_helicopter_lands_and_the_bystanders_follow_it() {
    let Some((mut characters, drive)) = setup("track-64_E", "drive-v6-gravel-206-64", Options::default()) else {
        return;
    };
    // Landed 19.6 s after the call (2795 steps), and the bystanders 24 s after it (f32 sums of 7 ms cross 24 a step
    // early).
    let events = events(&mut characters, &drive, 700, 3500);
    assert!(events.iter().any(|(step, e)| *step == 700 && matches!(e, Event::RescueCalled(_))));
    assert!(events.contains(&(4128, Event::RetireBystand)), "{events:?}");
    assert!(characters.rescue().is_none());
}

#[test]
fn every_bird_strike_of_track_64_strikes_once_at_the_car() {
    let options = Options { birdstrike: true, ..Options::default() };
    let Some((mut characters, drive)) = setup("track-64_E", "drive-v6-gravel-206-64", options) else { return };
    let mut strikes = Vec::new();
    for (end, sample) in drive.frames() {
        let car = characters.on_line(sample);
        let [x, y, z] = sample.car.position;
        let at = Vec3::new(x, z, y);
        // A camera behind the car, looking along its way: the visibility cells in view run their scripts.
        let ahead = car.map_or(Vec3::Z, |c| c.direction);
        let eye = at - ahead * 6.0 + Vec3::Y * 2.0;
        characters.look(rbr_render::coords::projection(1.0, 16.0 / 9.0) * rbr_render::coords::view_matrix(eye, ahead));
        characters.step(end, car);
        for event in characters.take_events() {
            if let Event::Birdstrike(p) = event {
                strikes.push((sample.step, p.distance(at)));
            }
        }
    }
    // The six items' plain trigger keys (no animation channel names them); each strikes once, next to the car. The
    // step within a key's few hundred depends on where the bird's idle clip is when it fires (the clips run while
    // the bird's cell is in view).
    let steps: Vec<u32> = strikes.iter().map(|s| s.0).collect();
    assert_eq!(steps, [4492, 9479, 21620, 22931, 29782, 44061], "{strikes:?}");
    assert!(strikes.iter().all(|s| s.1 < 5.0), "{strikes:?}");
}

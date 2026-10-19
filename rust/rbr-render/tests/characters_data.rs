//! `RBR_DATA` and `RBR_RECORDINGS`: the stage characters of track-54_E load and react to the
//! tanner drive the way the original's run scripts and Driveline triggers make them.

use rbr_formats::data::Data;
use rbr_render::characters::{load::Options, Characters};
use rbr_render::playback::Drive;
use rbr_render::stage::{self, Selection};

#[test]
fn tanner_drive_triggers_deer_and_media() {
    let (Some(data), Ok(recordings)) = (Data::from_env(), std::env::var("RBR_RECORDINGS")) else { return };
    let name = "track-54_E";
    let mut stage = stage::load(&data.install().join("Maps"), name, Selection::default()).expect("stage");
    // A race-start seed whose `PickRand` draws (`rand()`) pick the groups holding both watched characters.
    let options = Options { seed: 2, ..Options::default() };
    let mut characters = Characters::load_with(&data, name, &mut stage, options).expect("characters");
    characters.locate(&stage);
    assert_eq!(characters.count() - characters.report.scripted, 26, "placed items of the picked group variants");
    let drive = Drive::parse(&std::fs::read(format!("{recordings}/drive-tanner-corolla.bin")).unwrap()).unwrap();

    // First step each watched character leaves state 0, and first step it starts its second clip.
    let watched = [51378u32, 52066]; // the deer, a media person
    let mut triggered = [None; 2];
    let mut reacting = [None; 2];
    for (end, sample) in drive.frames() {
        characters.step(end, characters.on_line(sample));
        for s in characters.states() {
            let Some(w) = watched.iter().position(|&id| id == s.id) else { continue };
            if s.state != 0.0 {
                triggered[w].get_or_insert(sample.step);
            }
            if s.clip == Some(1) {
                reacting[w].get_or_insert(sample.step);
            }
        }
    }
    for w in 0..2 {
        let (Some(t), Some(r)) = (triggered[w], reacting[w]) else {
            panic!("{} never reacted: triggered {:?}, clip 1 {:?}", watched[w], triggered[w], reacting[w])
        };
        assert!(t <= r, "{} reacted before its trigger", watched[w]);
    }
    // The deer bolts well before the car arrives (nearest at step ~2766), the media person
    // just before (nearest at ~23459).
    assert!((2000..2766).contains(&triggered[0].unwrap()), "deer trigger at {:?}", triggered[0]);
    assert!((22000..23459).contains(&triggered[1].unwrap()), "media trigger at {:?}", triggered[1]);
}

/// The start and finish set pieces of track-54_E on the tanner drive: the countdown marshal (`load_start_race01c`,
/// object 0xc9cd, the `.dls` trigger `start_animation2`) starts its count when the HUD's countdown falls below 6 s,
/// and the finish walkers (`load_end_race01a/b`, 0xc7c1 and 0xc7bf, the `Driveline` plain keys at 4207 m, 39 m past
/// the finish line) when the car passes them. The time card writer (0xc9cc, `start_animation1`) is fired only by the
/// replay's timeline, so it holds its first clip.
#[test]
fn tanner_drive_runs_the_start_and_finish_set_pieces() {
    let (Some(data), Ok(recordings)) = (Data::from_env(), std::env::var("RBR_RECORDINGS")) else { return };
    let name = "track-54_E";
    let mut stage = stage::load(&data.install().join("Maps"), name, Selection::default()).expect("stage");
    let mut characters = Characters::load_with(&data, name, &mut stage, Options::default()).expect("characters");
    let drive = Drive::parse(&std::fs::read(format!("{recordings}/drive-tanner-corolla.bin")).unwrap()).unwrap();
    let watched = [0xc9cdu32, 0xc7c1, 0xc7bf, 0xc9cc];
    let mut triggered = [None; 4];
    for (end, sample) in drive.frames() {
        let time = rbr_render::ghost::race_time(sample, Default::default());
        characters.countdown(rbr_gameplay::race_clock::COUNTDOWN - time, false);
        characters.step(end, characters.on_line(sample));
        for s in characters.states() {
            if let Some(w) = watched.iter().position(|&id| id == s.id).filter(|_| s.state != 0.0) {
                triggered[w].get_or_insert((sample.step, time));
            }
        }
    }
    let (step, time) = triggered[0].expect("the countdown marshal never started");
    assert!((1.0..1.01).contains(&time), "the countdown marshal started at step {step}, race time {time}");
    for w in [1, 2] {
        let (step, _) = triggered[w].unwrap_or_else(|| panic!("{:#x} never walked", watched[w]));
        // Past the finish line, before the race's end at step 35403 (the end key, 4270 m).
        assert!((34000..35403).contains(&step), "{:#x} walked at step {step}", watched[w]);
    }
    assert_eq!(triggered[3], None, "the time card writer has no live trigger");
}

/// The RSF sample stages that name `start_animation2` (101, 138, 140; 294 does too, but its `.lbs` does not parse;
/// 129 and 450 have no named triggers): the countdown below 6 s starts their countdown marshal (`load_start_race01c`),
/// as on the retail stages.
#[test]
fn rsf_stages_start_their_countdown_marshal() {
    let (Some(data), Some(root)) = (Data::from_env(), std::env::var_os("RBR_RSF")) else { return };
    for id in ["101", "138", "140"] {
        let maps = std::path::Path::new(&root).join("Maps");
        let dir = std::fs::read_dir(&maps).unwrap().flatten().find(|e| e.file_name().to_string_lossy().starts_with(id));
        let dir = dir.unwrap_or_else(|| panic!("RSF {id} not in {}", maps.display())).path();
        let rsf = rbr_formats::rsf::open(&dir).unwrap();
        let name = rsf.stage(rsf.default_variant().unwrap());
        let mut stage = stage::load(&data.install().join("Maps"), &name, Selection::default())
            .unwrap_or_else(|e| panic!("RSF {id}: {e:?}"));
        let mut characters = Characters::load_with(&data, &name, &mut stage, Options::default()).expect("characters");
        let state = |c: &Characters| {
            let states = c.states();
            let mut marshals = states.iter().filter(|s| s.load.ends_with("load_start_race01c.lua"));
            let s = marshals.next().unwrap_or_else(|| panic!("RSF {id}: no countdown marshal"));
            assert!(marshals.next().is_none());
            s.state
        };
        characters.countdown(6.5, false);
        characters.step(0.1, None);
        assert_eq!(state(&characters), 0.0, "RSF {id}");
        characters.countdown(5.99, false);
        characters.step(0.2, None);
        assert_ne!(state(&characters), 0.0, "RSF {id}: the marshal did not start");
    }
}

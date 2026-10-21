use std::path::{Path, PathBuf};

use rbr_formats::rsf;
use rbr_gameplay::weather::{self, Draws, Preset, Season};
use rbr_hybrid::pure::Compare;
use rbr_recording::damage::{Damage, DMG1_BYTES};
use rbr_recording::detach;
use rbr_recording::hud::{Hud, HUD2_BYTES};
use rbr_recording::sound::{self, Sound};
use rbr_recording::{Frame, Frames, CONTROL_BYTES, HEADER_BYTES, WHEELS, WHEEL_BYTES};

use super::*;
use crate::headless::{run, Hooks, Source};
use crate::physics::{boot_playback, on_big_stack, payload};
use crate::stage::stage_data;

/// The snapshot the test boots from, in `RBR_SNAPS` (default `/tmp/rbr-snaps`); its drive is `drive-tanner-corolla.bin`.
fn snapshot() -> String {
    let dir = std::env::var("RBR_SNAPS").unwrap_or_else(|_| "/tmp/rbr-snaps".into());
    format!("{dir}/vcu-cov.snap-2000.bin")
}
use rbr_recording::particles::PAYLOAD_BYTES;

const STEPS: usize = 30;
/// One answer without its HUD and damage records: the header, the payload and the car and wheels.
const FRAME_BYTES: usize = HEADER_BYTES + CONTROL_BYTES + CAR_BYTES + WHEELS * WHEEL_BYTES;

/// Splits the protocol's output into (frame, HUD record, damage record) triples, checking the tagged records.
fn split(mut out: &[u8]) -> Vec<(&[u8], &[u8], &[u8])> {
    let mut frames = Vec::new();
    while !out.is_empty() {
        let (frame, rest) = out.split_at(FRAME_BYTES);
        let (hud, rest) = rest.split_at(HUD2_BYTES);
        let (damage, mut rest) = rest.split_at(DMG1_BYTES);
        let records = records::read_records(&mut rest).unwrap();
        let tags: Vec<[u8; 4]> = records.iter().map(|(tag, _)| *tag).collect();
        assert_eq!(
            tags,
            [
                particles::PTC1,
                particles::PEV1,
                detach::DET1,
                SND1,
                PCN1,
                rbr_recording::splits::SPL1,
                DSH1,
                rbr_recording::camera::CAM1,
                rbr_recording::clock::CLK1,
                rbr_recording::flow::FLW1,
                rbr_recording::ffb::FFB1,
                rbr_recording::gearbox::GBX1,
                rbr_recording::shift::SHF1,
                rbr_recording::menu::MNU1,
                rbr_recording::sound::FMB1,
                records::CTL1
            ]
        );
        particles::read_events(&records[1].1).unwrap();
        // One payload per wheel the game signals that step (not every wheel every step).
        let payloads = &records[0].1;
        let tyres = payloads.len();
        assert!(tyres.is_multiple_of(PAYLOAD_BYTES) && tyres <= 4 * PAYLOAD_BYTES, "{tyres} bytes of payloads");
        let wheels: Vec<u32> =
            payloads.chunks(PAYLOAD_BYTES).map(|p| u32::from_le_bytes(p[..4].try_into().unwrap())).collect();
        assert!(wheels.iter().all(|&w| w < 4), "wheels {wheels:?}");
        assert_eq!(records[7].1.len(), rbr_recording::camera::BODY_BYTES);
        let count = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        out = &rest[4 + count * 32..];
        frames.push((frame, hud, damage));
    }
    frames
}

#[test]
fn live_steps_are_the_headless_steps_and_the_recording() {
    let (Some(data), Some(recordings)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_RECORDINGS").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_RECORDINGS unset; skipped");
        return;
    };
    let recording = recordings.join("drive-tanner-corolla.bin");
    let snapshot = snapshot();
    if !Path::new(&snapshot).is_file() {
        eprintln!("{snapshot} is missing; skipped");
        return;
    }
    on_big_stack(move || {
        let bytes = std::fs::read(&recording).unwrap();
        let frames: Vec<Frame> = Frames::new(&bytes).map(Result::unwrap).collect();
        let stage = stage_data(&Data::new(data, None), "track-54_E", 6).unwrap();
        let snap = Snapshot::open(&snapshot).unwrap();
        let mask = pure::static_mask(&frames);
        let step = snap.metadata().step;
        let first = frames.iter().position(|f| f.pre == step).unwrap();
        let headless = run(
            Source::Snapshot(&snapshot),
            &frames,
            &mask,
            &stage,
            STEPS,
            None,
            MarkerSettings::default(),
            None,
            Hooks::default(),
        )
        .unwrap();
        let mut compare = Compare::new(&snap, mask.clone(), &frames[first - 1]);
        let payloads: Vec<[u8; CONTROL_BYTES]> = frames[first..first + STEPS].iter().map(|f| f.control).collect();

        let mut live = Live::new(&snap, &stage, MarkerSettings::default());
        let mut out = Vec::new();
        let mut good = None;
        let mut live_steps = 0;
        for (k, payload) in payloads.iter().enumerate() {
            let answer = live.answer(payload).unwrap();
            let (frame, hud, damage) = split(&answer).pop().unwrap();
            let recorded = &frames[first + k];
            assert_eq!(frame[HEADER_BYTES..HEADER_BYTES + CONTROL_BYTES], payload[..]);
            let record = Hud::read(&mut &hud[..]).unwrap();
            assert_eq!(record.race.map(|r| r.time), Some(live.game.race_clock().time));
            Damage::read(&mut &damage[..]).unwrap();
            if live.stopped {
                // Frozen: the last good frame again, under the step's own counter.
                let good = good.get_or_insert_with(|| live.last.clone());
                assert_eq!(frame[HEADER_BYTES + CONTROL_BYTES..], good.body[..]);
                assert_eq!(hud[..], good.hud[..]);
                assert_eq!(damage[..], good.damage[..]);
                assert_eq!(&frame[4..8], &recorded.pre.to_le_bytes());
            } else {
                live_steps += 1;
                assert!(
                    compare.diff(&live.playback.pure.mem, recorded).words.is_empty(),
                    "step {}: live differs from the recording",
                    recorded.pre
                );
                assert_eq!(frame[HEADER_BYTES + CONTROL_BYTES..], pure::frame_bytes(&live.playback.pure.mem)[..]);
            }
            out.extend(answer);
        }
        assert_eq!(live_steps, headless.steps, "steps run pure, live and headless");
        assert_eq!(headless.equal, headless.steps, "the headless steps equal the recording");

        // The same payloads through the protocol give the same bytes.
        let mut served = Vec::new();
        let input = payloads.concat();
        serve(Live::new(&snap, &stage, MarkerSettings::default()), &input[..], &mut served, None).unwrap();
        assert!(served == out, "serve writes the answers");
    })
    .unwrap();
}

#[test]
fn a_frame_of_several_steps_sends_each_wheel_s_last_tyre_payload() {
    let payload = |wheel: u8, tag: u8| {
        let mut p = [0; PAYLOAD_BYTES];
        (p[0], p[8]) = (wheel, tag);
        p
    };
    let tags = |ps: Vec<[u8; PAYLOAD_BYTES]>| ps.iter().map(|p| (p[0], p[8])).collect::<Vec<_>>();
    let steps = vec![payload(0, 1), payload(1, 1), payload(0, 2), payload(3, 2), payload(1, 3)];
    assert_eq!(tags(last_per_wheel(steps)), [(0, 2), (3, 2), (1, 3)]);
    assert_eq!(tags(last_per_wheel(vec![payload(2, 1)])), [(2, 1)]);
}

/// `--live --boot --ngp-physics` paced as the viewer paces it (`RBR_NGP_PHYSICS`: NGP 7.4.778's `physics.rbz`):
/// the go after the same 376 countdown frames as 1.02's, then frames of 5 or 6 steps of 1/720 s whose clocks tick
/// 7 ms of race time a frame, and the CLK1 record saying so.
#[test]
fn a_paced_ngp_live_boot_runs_frames_of_several_steps_at_real_speed() {
    let (Some(install), Some(patch), Some(ngp)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_NGP_PHYSICS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-11_O", 5).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 11, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race { ngp_physics: Some(ngp), ..boot::Race::quick_race(11, 5, 3, weather) };
        let mut live = Live::boot(&data, "track-11_O", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        let ngp = StepClock { step: f32::from_bits(0x3ab6_0b61), settle: 1520 };
        assert_eq!(live.clock, ngp);
        let substeps = |answer: &[u8]| u32::from_le_bytes(answer[12..16].try_into().unwrap());
        for _ in 0..376 {
            assert_eq!(substeps(&live.answer(&[0; CONTROL_BYTES]).unwrap()), 0, "a countdown frame");
        }
        let (frames, mut steps) = (60, 0);
        for _ in 0..frames {
            let answer = live.answer(&[0; CONTROL_BYTES]).unwrap();
            assert!(!live.stopped, "the NGP live boot stopped");
            let n = substeps(&answer);
            assert!(n == 5 || n == 6, "{n} steps in a frame");
            steps += n;
            let (_, rest) = answer.split_at(FRAME_BYTES + HUD2_BYTES + DMG1_BYTES);
            let records = records::read_records(&mut &rest[..]).unwrap();
            let clock = records.iter().find(|(tag, _)| *tag == rbr_recording::clock::CLK1).unwrap();
            assert_eq!(StepClock::read_body(&clock.1).unwrap(), ngp);
        }
        assert_eq!(live.next_step(), 1520 + steps);
        // The race clock's 7 s start countdown.
        let ticked = 7.0 - live.game.race_clock().countdown;
        let real = frames as f32 * rbr_recording::STEP_SECONDS;
        assert!((steps as f32 * ngp.step - real).abs() <= ngp.step, "{steps} steps in {real} s of frames");
        // The clock also ticks on the go's frame, before the first step, as under 1.02.
        assert!((ticked - real - rbr_recording::STEP_SECONDS).abs() <= ngp.step, "{ticked} s in {real} s");
        // `--pilot autopilot` holds the car until the race clock's go, then launches it (it revs to a share of
        // the car's rev limit in neutral first, so it needs the limit) and drives off within 5 s.
        let mut pilot = crate::autopilot::Pilot::new("autopilot", &stage, &live).unwrap();
        let mut last = None;
        let mut driving = 0;
        while driving < 700 {
            let payload = pilot.payload(&live, last.as_deref()).unwrap();
            last = Some(live.answer(&payload).unwrap());
            driving += u32::from(!live.game.race_clock().counting_down());
        }
        let sense = crate::autopilot::sense(&last.unwrap(), &live);
        let speed = sense.pose.speed();
        assert!(speed > 3.0, "the pilot did not drive off: {speed:.1} m/s in gear {}", sense.gear);
    })
    .unwrap();
}

/// NGP's force-feedback sensitivity (`Race::ffb_sensitivity`, `--ffb-sensitivity`) at 50 % halves the car's `S`:
/// the same drive, frame for frame, with each steering force `1 - (1 - f)^2` of the force `f` at 100 %.
#[test]
fn ngp_ffb_sensitivity_reshapes_the_force_and_nothing_else() {
    let (Some(install), Some(patch), Some(ngp)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_NGP_PHYSICS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-11_O", 5).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 11, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race { ngp_physics: Some(ngp), ..boot::Race::quick_race(11, 5, 3, weather) };
        let drive = |race: &boot::Race| {
            let mut live = Live::boot(&data, "track-11_O", race, &stage, MarkerSettings::default(), true).unwrap();
            live.pace(f64::from(rbr_recording::STEP_SECONDS));
            let mut pilot = crate::autopilot::Pilot::new("autopilot", &stage, &live).unwrap();
            let mut last: Option<Vec<u8>> = None;
            let mut out = vec![];
            // 3000 frames of 7 ms, the countdown included.
            while out.len() < 3000 || live.game.race_clock().counting_down() {
                let payload = pilot.payload(&live, last.as_deref()).unwrap();
                let answer = live.answer(&payload).unwrap();
                let (_, rest) = answer.split_at(FRAME_BYTES + HUD2_BYTES + DMG1_BYTES);
                let records = records::read_records(&mut &rest[..]).unwrap();
                let (_, force) = records.iter().find(|(tag, _)| *tag == rbr_recording::ffb::FFB1).unwrap();
                let force = rbr_recording::ffb::Steering::from_payload(force).unwrap().force;
                out.push((answer[..FRAME_BYTES].to_vec(), force));
                last = Some(answer);
            }
            out
        };
        let base = drive(&race);
        let half = drive(&boot::Race { ffb_sensitivity: [50; 3], ..race.clone() });
        let mut felt = 0;
        for (k, ((frame, f), (half_frame, h))) in base.iter().zip(&half).enumerate() {
            assert!(frame == half_frame, "frame {k}: the physics differs");
            let expected = f.signum() * (1.0 - (1.0 - f.abs()).powi(2));
            assert!((h - expected).abs() < 1e-4, "frame {k}: {h} at 50 %, {f} at 100 %");
            felt += usize::from(f.abs() > 0.05);
        }
        let peak = base.iter().map(|(_, f)| f.abs()).fold(0.0, f32::max);
        assert!(felt > 100, "only {felt} frames with a force to compare (peak {peak})");
    })
    .unwrap();
}

/// The viewer's Ignition action (its payload's aux bit, `CDrive+0x50`): an NGP engine stalled in first gear at idle
/// stays stalled with the clutch in, and NGP's starter (`rbr_physics::ngp::starter`) restarts it while the ignition
/// is held with the clutch in.
#[test]
fn an_ngp_engine_stalled_in_gear_restarts_on_the_held_ignition() {
    let (Some(install), Some(patch), Some(ngp)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_NGP_PHYSICS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-11_O", 5).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 11, Preset::Good, Draws::default()).unwrap();
        let mut race = boot::Race { ngp_physics: Some(ngp), ..boot::Race::quick_race(11, 5, 3, weather) };
        // Without the clutch help and the automatic gears, as a player who stalls drives.
        (race.options.aids.clutch_help, race.options.aids.auto_gears) = (false, false);
        let mut live = Live::boot(&data, "track-11_O", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        let mut engine = |input: Input| {
            let answer = live.answer(&input.to_bytes()).unwrap();
            assert!(!live.stopped, "the NGP live boot stopped");
            Hud::read(&mut &answer[FRAME_BYTES..]).unwrap().engine_speed
        };
        let idle = Input::default();
        for _ in 0..376 + 300 {
            engine(idle);
        }
        // A shift up into first at idle with the clutch out stalls it.
        engine(Input { flags: 0x01, ..idle });
        let mut stalled = 0.0;
        for _ in 0..400 {
            stalled = engine(idle);
        }
        assert!(stalled.abs() < 1.0, "not stalled: {stalled} rad/s");
        let clutch = Input { clutch: 1.0, ..idle };
        for _ in 0..300 {
            let speed = engine(clutch);
            assert!(speed.abs() < 1.0, "it started without the ignition: {speed} rad/s");
        }
        let (mut started, mut at) = (0.0f32, None);
        for k in 0..700 {
            let speed = engine(Input { aux: true, ..clutch });
            if speed > 50.0 && at.is_none() {
                at = Some(k);
            }
            started = started.max(speed);
        }
        let at = at.unwrap_or_else(|| panic!("no start in 4.9 s of ignition: at most {started} rad/s"));
        eprintln!("started after {:.2} s of ignition, {started:.0} rad/s at most", at as f32 * 0.007);
    })
    .unwrap();
}

/// The pause menu's record and retire (`rbr_gameplay::pause_menu`): a quick race offers the call for help from 2 s
/// of race time, and a payload with `RETIRE` ends the race retired on its frame, with no step, on page 0x12.
#[test]
fn the_pause_menu_s_retire_ends_the_race_without_a_step() {
    let (Some(install), Some(patch)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_PATCH").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather);
        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        let menu = |answer: &[u8]| {
            let (_, rest) = answer.split_at(FRAME_BYTES + HUD2_BYTES + DMG1_BYTES);
            let records = records::read_records(&mut &rest[..]).unwrap();
            let (_, body) = records.iter().find(|(tag, _)| *tag == rbr_recording::menu::MNU1).unwrap();
            Menu::read_body(body).unwrap()
        };
        let input = |flags| Input { flags, ..Input::default() }.to_bytes();
        let first = menu(&live.answer(&input(0)).unwrap());
        assert_eq!(first, Menu { mode: 1, driving: false, help: false, over: false }, "the start orbit");
        let mut answer = live.answer(&input(0)).unwrap();
        while live.game.race_clock().time < 2.0 {
            assert!(!menu(&answer).help, "no call for help before 2 s");
            answer = live.answer(&input(0)).unwrap();
        }
        assert_eq!(menu(&answer), Menu { mode: 1, driving: true, help: true, over: false });
        let (step, time) = (live.next_step(), live.game.race_clock().time);
        let answer = live.answer(&input(RETIRE)).unwrap();
        assert_eq!(live.next_step(), step, "the retire's frame steps nothing");
        assert_eq!(menu(&answer), Menu { mode: 1, driving: false, help: false, over: true });
        let end = live.game.flow().end().expect("the race ended");
        assert_eq!(
            (end.cause, end.finished(), end.end_race.time),
            (rbr_gameplay::flow::EndCause::Retired, false, time)
        );
        assert_eq!(live.end_page(&end), EndPage::QuickRetired);
    })
    .unwrap();
}

/// A championship stage's start (`rbr_gameplay::flow::Mode::confirm_arms`): the start orbit gives no go, and the
/// countdown waits for the player's confirm, an input payload's `ACCEPT_MESSAGE`; the driving starts 2 s after it.
#[test]
fn a_championship_start_waits_for_the_confirm() {
    let (Some(install), Some(patch)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_PATCH").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather).in_championship(Default::default());
        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        let input = |flags| Input { flags, ..Input::default() }.to_bytes();
        let start = live.next_step();
        for _ in 0..1000 {
            live.answer(&input(0)).unwrap();
        }
        assert_eq!((live.next_step(), live.game.stage().state()), (start, State::Countdown), "no go without it");
        live.answer(&input(ACCEPT_MESSAGE)).unwrap();
        let mut frames = 1;
        while live.next_step() == start {
            live.answer(&input(0)).unwrap();
            frames += 1;
        }
        assert_eq!(frames, 287, "the confirm's frame and 285 more of the 2 s countdown, then the first step");
    })
    .unwrap();
}

/// A payload of several periods (`Input::frames`, a viewer behind real time) is one game frame of their wall
/// time: the same answers as a process paced at that time a payload, frames of 3 steps once driving.
#[test]
fn a_payload_of_three_periods_is_one_frame_of_three_steps() {
    let (Some(install), Some(patch)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_PATCH").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather);
        let step = f64::from(rbr_recording::STEP_SECONDS);
        let boot = || Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        let (mut periods, mut frame) = (boot(), boot());
        periods.pace(step);
        frame.pace(3.0 * step);
        let throttle = |frames| Input { throttle: 1.0, frames, ..Input::default() }.to_bytes();
        let substeps = |answer: &[u8]| u32::from_le_bytes(answer[12..16].try_into().unwrap());
        let mut threes = 0;
        for k in 0..200 {
            let (a, b) = (periods.answer(&throttle(3)).unwrap(), frame.answer(&throttle(1)).unwrap());
            assert!(!periods.stopped && !frame.stopped, "frame {k}: stopped");
            assert_eq!(a[..HEADER_BYTES], b[..HEADER_BYTES], "frame {k}: the header");
            assert!(a[HEADER_BYTES + CONTROL_BYTES..] == b[HEADER_BYTES + CONTROL_BYTES..], "frame {k}");
            threes += usize::from(substeps(&a) == 3);
        }
        assert_eq!(periods.game.race_clock(), frame.game.race_clock());
        assert!(threes > 50, "{threes} frames of 3 steps");
        // The autopilot's recorded control in such a frame (`--pilot autopilot`, `Live::answer_over`) too.
        let (mut periods, mut frame) = (boot(), boot());
        periods.pace(step);
        frame.pace(3.0 * step);
        let control = QuantizedControl { throttle: 255, ..QuantizedControl::default() };
        let recorded = payload(&control.car_control(), 0);
        for k in 0..200 {
            let (a, b) = (periods.answer_over(&recorded, 3).unwrap(), frame.answer(&recorded).unwrap());
            assert!(a == b, "frame {k}: the recorded control's answers");
        }
    })
    .unwrap();
}

/// The SND1 tyre word 1 (`TyreMax`) against the memory's: over one-step frames the game's frame value is the
/// largest `FillOutputs` term of the step (from the finish's 0), and the memory keeps the larger of it and its last
/// value (the standalone runs no finish; both are cleared off the ground), so ours is never above the memory's,
/// and wherever either is above the memory's last value both are equal. Tanner to the finish.
#[test]
fn the_tyre_word_1_is_the_step_s_fill_outputs_term() {
    let (Some(install), Some(patch), Some(recordings)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_RECORDINGS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_RECORDINGS unset; skipped");
        return;
    };
    let recording = recordings.join("drive-tanner-corolla.bin");
    on_big_stack(move || {
        let data = Data::new(install, Some(patch));
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather);
        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        let mut pilot = Pilot::new(recording.to_str().unwrap(), &stage, &live).unwrap();
        let word = |live: &Live, k: usize| {
            let mem = live.mem();
            let wheel = mem.u32(mem.u32(CAR_PTR) + sound::CAR_WHEELS + 4 * k as u32);
            mem.f32(wheel + sound::WHEEL_TYRE_NOISE + 4 * sound::TYRE_MAX as u32)
        };
        let (mut before, mut steps, mut read_back, mut positive) = ([0.0f32; 4], 0, 0, 0);
        while live.game.stage().state() != State::Ended {
            let pre = live.next_step();
            live.answer(&pilot.payload(&live, None).unwrap()).unwrap();
            if live.next_step() == pre {
                before = std::array::from_fn(|k| word(&live, k));
                continue;
            }
            steps += 1;
            let sent = Sound::from_bytes(&live.last.sound).unwrap();
            for (k, last) in before.iter_mut().enumerate() {
                let (ours, now) = (f32::from_bits(sent.tyres[k][sound::TYRE_MAX]), word(&live, k));
                assert!(ours <= now, "step {pre} wheel {k}: {ours} above the memory's {now}");
                if ours > *last || now > *last {
                    assert_eq!(ours.to_bits(), now.to_bits(), "step {pre} wheel {k}: {ours} vs {now}");
                    read_back += 1;
                }
                positive += usize::from(ours > 0.0);
                *last = now;
            }
        }
        eprintln!("{steps} steps: {read_back} terms read back, {positive} positive wheel frames");
        assert!(read_back > 50 && positive > 10000, "{read_back} read back, {positive} positive");
    })
    .unwrap();
}

/// `--live --boot` as the viewer drives it with `--at-drive`: paced at 7 ms a payload, the camera's go, and each
/// payload the recording's control of the step it feeds (`viewer::recorded_controls`), so the first one goes
/// through the countdown's frames, which step nothing. Every step to the recording's end equals the recording, as
/// the headless boot does.
#[test]
fn a_paced_live_boot_with_the_recorded_controls_is_the_recording() {
    let (Some(install), Some(patch), Some(recordings)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_RECORDINGS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_RECORDINGS unset; skipped");
        return;
    };
    let recording = recordings.join("drive-tanner-corolla.bin");
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let bytes = std::fs::read(&recording).unwrap();
        let frames: Vec<Frame> = Frames::new(&bytes).map(Result::unwrap).collect();
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather);
        let (image, built) = boot_playback(&data, &stage, "track-54_E", &race, None, true).unwrap();
        let mut compare = Compare::at_start_of(&image.pure.mem, built.regions, pure::static_mask(&frames), &frames);
        let driving = &frames[SETTLE_SUBSTEPS.len()..];

        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        // `--pilot RECORDING` feeds the same controls.
        let mut pilot = Pilot::new(recording.to_str().unwrap(), &stage, &live).unwrap();
        let (mut k, mut waited) = (0, 0);
        // The race ends at the stage's end marker; the recording's last frame is the original's step after it,
        // which the headless boot does not run either.
        while live.game.stage().state() != State::Ended {
            let recorded = &driving[k];
            let pre = live.next_step();
            assert_eq!(pre, recorded.pre, "the step the payload feeds");
            assert_eq!(pilot.payload(&live, None), Some(recorded.control), "step {pre}: the pilot's control");
            live.answer(&recorded.control).unwrap();
            assert!(!live.stopped, "step {pre}: the live boot stopped");
            match live.next_step() - pre {
                0 => {
                    waited += 1;
                    assert!(waited <= 376 && k == 0, "step {pre}: a frame without a step while driving");
                }
                1 => {
                    assert!(
                        compare.diff(&live.playback.pure.mem, recorded).words.is_empty(),
                        "step {pre}: the paced live boot differs from the recording"
                    );
                    k += 1;
                }
                n => panic!("step {pre}: a frame of {n} steps"),
            }
        }
        assert_eq!(waited, 376, "the countdown's frames");
        assert_eq!(k + 1, driving.len(), "the driving steps run");
        // The race time and the HUD's splits (the race clock at each split key), as the headless boot has them
        // (`tests::the_headless_boot_times_tanner_as_live_play_does` in main.rs) and the end page shows them.
        let result = live.game.result();
        assert_eq!((result.time, result.penalties), (Some(TANNER_TIME), 0.0));
        assert_eq!(result.splits, TANNER_SPLITS.map(Some));
        let end = live.game.flow().end().expect("the race ended");
        assert_eq!((end.end_race.time, end.end_race.splits), (TANNER_TIME, TANNER_SPLITS));
    })
    .unwrap();
}

/// An `--autopilot --boot` playback (no `--saves`) waits for the race clock's go, and `--live --boot --pilot` with it
/// (paced, the camera's go) starts on the green too: neither takes the jump start, and the car drives off.
#[test]
fn an_autopilot_playback_starts_on_the_green_under_the_live_pilot() {
    let (Some(install), Some(patch)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_PATCH").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_PATCH unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install, Some(patch));
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather);
        let live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), false).unwrap();
        let out = std::env::temp_dir().join(format!("rbr-autopilot-green-{}.bin", std::process::id()));
        let neutral = payload(&QuantizedControl::default().car_control(), 0);
        let drove = crate::autopilot::drive(live, &[], neutral, &stage, 1600, None, None, &out).unwrap();
        assert!(!drove.out.unwrap().false_start, "the autopilot jumped the start");

        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        live.pace(f64::from(rbr_recording::STEP_SECONDS));
        let mut pilot = crate::autopilot::Pilot::new(out.to_str().unwrap(), &stage, &live).unwrap();
        let mut last = None;
        while let Some(control) = pilot.payload(&live, None) {
            last = Some(live.answer(&control).unwrap());
            assert!(!live.stopped, "step {}: the live boot stopped", live.next_step());
        }
        std::fs::remove_file(&out).unwrap();
        assert_eq!(live.next_step(), drove.start + drove.steps, "the playback's steps run");
        assert!(!live.game.race_clock().jumped, "the live pilot jumped the start");
        let speed = crate::autopilot::sense(&last.unwrap(), &live).pose.speed();
        assert!(speed > 3.0, "the pilot did not drive off: {speed:.1} m/s");
    })
    .unwrap();
}

/// `--live --boot`: after the boot's settle and go, each payload is the headless boot run's step, and equals the
/// recording.
#[test]
fn live_boot_steps_are_the_headless_boot_steps_and_the_recording() {
    let (Some(install), Some(patch), Some(recordings)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_RECORDINGS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_RECORDINGS unset; skipped");
        return;
    };
    let recording = recordings.join("drive-tanner-corolla.bin");
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let bytes = std::fs::read(&recording).unwrap();
        let frames: Vec<Frame> = Frames::new(&bytes).map(Result::unwrap).collect();
        let stage = stage_data(&data, "track-54_E", 6).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race::quick_race(54, 6, 4, weather);
        let mask = pure::static_mask(&frames);
        let settle = SETTLE_SUBSTEPS.len();
        let headless = run(
            Source::Boot(&data, "track-54_E", &race, None),
            &frames,
            &mask,
            &stage,
            settle + STEPS,
            None,
            MarkerSettings::default(),
            None,
            Hooks::default(),
        )
        .unwrap();
        assert_eq!(headless.steps, settle + STEPS, "the headless boot runs");
        assert_eq!(headless.equal, headless.steps, "the headless boot equals the recording");
        let (image, built) = boot_playback(&data, &stage, "track-54_E", &race, None, true).unwrap();
        let mut compare = Compare::at_start_of(&image.pure.mem, built.regions, mask, &frames);
        let payloads: Vec<[u8; CONTROL_BYTES]> = frames[settle..settle + STEPS].iter().map(|f| f.control).collect();

        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), false).unwrap();
        assert_eq!(live.start, frames[settle].pre, "the first payload's step");
        let mut out = Vec::new();
        for (k, payload) in payloads.iter().enumerate() {
            let answer = live.answer(payload).unwrap();
            assert!(!live.stopped, "step {}: the live boot stopped", frames[settle + k].pre);
            let (frame, _, _) = split(&answer).pop().unwrap();
            let recorded = &frames[settle + k];
            assert_eq!(&frame[4..8], &recorded.pre.to_le_bytes());
            assert!(
                compare.diff(&live.playback.pure.mem, recorded).words.is_empty(),
                "step {}: the live boot differs from the recording",
                recorded.pre
            );
            out.extend(answer);
        }

        // With the camera's go timer: 376 countdown frames (375 of the orbit, then `Tic`'s), answered at the
        // settle's last step with no substeps, then the same steps.
        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), true).unwrap();
        let mut angles = Vec::new();
        for _ in 0..376 {
            let answer = live.answer(&[0; CONTROL_BYTES]).unwrap();
            let (frame, ..) = split(&answer).pop().unwrap();
            let word = |i: usize| u32::from_le_bytes(frame[4 * i..4 * i + 4].try_into().unwrap());
            assert_eq!([word(1), word(2), word(3)], [frames[settle].pre, frames[settle].pre, 0]);
            angles.push(live.game.start_camera().and_then(|c| c.angle()));
        }
        assert!(angles[..374].iter().all(Option::is_some) && angles[374..].iter().all(Option::is_none));
        for (k, payload) in payloads.iter().enumerate() {
            live.answer(payload).unwrap();
            assert!(
                compare.diff(&live.playback.pure.mem, &frames[settle + k]).words.is_empty(),
                "step {}: the live boot with the go timer differs from the recording",
                frames[settle + k].pre
            );
        }

        let mut served = Vec::new();
        let input = payloads.concat();
        let live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), false).unwrap();
        serve(live, &input[..], &mut served, None).unwrap();
        assert!(served == out, "serve writes the answers");
    })
    .unwrap();
}

/// An RSF car (`RBR_RSF`: the Evo IX if it is there, else the first car with a pack) boots under NGP
/// (`RBR_NGP_PHYSICS`, a `physics.rbz`) on track-54_E from its pack and model, and its body stays finite through the
/// start hold and 2,000 steps at full throttle in first, in which it moves.
#[test]
fn an_rsf_car_boots_and_drives_under_ngp() {
    let (Some(data), Some(root), Some(ngp)) = (
        Data::from_env().filter(|d| d.patch().is_some()),
        std::env::var_os("RBR_RSF").map(PathBuf::from),
        std::env::var_os("RBR_NGP_PHYSICS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH, RBR_RSF or RBR_NGP_PHYSICS unset; skipped");
        return;
    };
    let install = rsf::Install::open(&root).unwrap();
    let key = match install.find("24") {
        Some(car) if install.pack(car).is_ok() => "24".to_owned(),
        _ => install.cars.iter().find(|c| install.pack(c).is_ok()).expect("a car with a pack").id.to_string(),
    };
    on_big_stack(move || {
        let selected = rsf::select(data, &key, rsf::SLOT, Some(&root)).unwrap();
        let data = selected.data;
        let stage = stage_data(&data, "track-54_E", rsf::SLOT).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 54, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race {
            ngp_physics: Some(ngp),
            car_pack: Some(selected.pack),
            ..boot::Race::quick_race(54, rsf::SLOT, 4, weather)
        };
        let mut live = Live::boot(&data, "track-54_E", &race, &stage, MarkerSettings::default(), false).unwrap();
        // The autopilot's way off the line: full throttle in neutral, a shift-up pulse into first, then full
        // throttle (shifting at idle stalls NGP's engines).
        let go = QuantizedControl { throttle: 255, ..QuantizedControl::default() };
        let first = QuantizedControl { flags: 0x01, ..go };
        let mut held = [0.0; 3];
        for k in 0..4500 {
            let control = match k {
                0..2000 => [0; CONTROL_BYTES],
                2500 => payload(&first.car_control(), 0),
                _ => payload(&go.car_control(), 0),
            };
            let answer = live.answer(&control).unwrap();
            let pose = pose::car_pose(&answer[HEADER_BYTES + CONTROL_BYTES..]).unwrap();
            let words = pose.position.iter().chain(&pose.orientation).chain(&pose.velocity);
            assert!(words.clone().all(|w| w.is_finite()), "step {k}: {pose:?}");
            match k {
                1999 => held = pose.position,
                4499 => {
                    let moved = (0..3).map(|i| (pose.position[i] - held[i]).powi(2)).sum::<f32>().sqrt();
                    assert!(moved > 3.0, "the car moved {moved} m under throttle");
                }
                _ => {}
            }
        }
    })
    .unwrap();
}

#[test]
fn ngp_control_input_emit_adds_the_direct_gears_with_the_automatic_gearbox_off() {
    // 0x100ed540: the car's VCU scratch +0x50 decides; the gear in use is the drive state's +0x54c.
    let (car, state) = (0x10_0000, 0x30_0000);
    let mut mem = Mem::new();
    mem.set_u32(CAR_PTR, car);
    mem.set_u32(car + 0x4b0 + 8, state);
    mem.set_u32(state + 0x54c, 2);
    mem.set_u32(car + 0x850 + 0x50, 0);
    // Third gear (gear word 4) held, all eight bound.
    let input = Input { gears_held: 1 << 4, gears_bound: 0xff, ..Input::default() };
    let raw = poll(&input, ControlPath::Ngp, &mem);
    assert_eq!((raw.flags, raw.gear), (0x04, 5));
    assert_eq!(poll(&input, ControlPath::Quantized, &mem).flags, 0);
    mem.set_u32(car + 0x850 + 0x50, 1);
    assert_eq!(poll(&input, ControlPath::Ngp, &mem).flags, 0);
}

/// `--live --realtime`: an NGP boot on the process's clock runs a frame per 1/720 s step with the newest input, sends a
/// step record after each and an answer per 7 ms, and its logged frames replay bit for bit through the paced path.
#[test]
fn a_realtime_ngp_drive_steps_once_a_frame_and_replays_from_its_frames() {
    use rbr_recording::realtime::{Message, Step, INPUT, RESUME, STEP_BYTES, STP1};
    let (Some(install), Some(patch), Some(ngp)) = (
        std::env::var_os("RBR_DATA").map(PathBuf::from),
        std::env::var_os("RBR_PATCH").map(PathBuf::from),
        std::env::var_os("RBR_NGP_PHYSICS").map(PathBuf::from),
    ) else {
        eprintln!("RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS unset; skipped");
        return;
    };
    on_big_stack(move || {
        let data = Data::new(install.clone(), Some(patch));
        let stage = stage_data(&data, "track-11_O", 5).unwrap();
        let season = Season::load(&data).unwrap();
        let weather = weather::quick_race(&season, 11, Preset::Good, Draws::default()).unwrap();
        let race = boot::Race { ngp_physics: Some(ngp), ..boot::Race::quick_race(11, 5, 3, weather) };
        let boot = || Live::boot(&data, "track-11_O", &race, &stage, MarkerSettings::default(), false).unwrap();
        let live = boot();
        let (reader, mut writer) = std::io::pipe().unwrap();
        // The first input, the start, then the steering swept for 0.6 s with a gear up and the throttle on.
        let feeder = std::thread::spawn(move || {
            let send = |w: &mut std::io::PipeWriter, tag, seq, input: Input| {
                w.write_all(&Message { tag, seq, payload: input.to_bytes() }.to_bytes()).unwrap()
            };
            send(&mut writer, INPUT, 0, Input::default());
            send(&mut writer, RESUME, 0, Input::default());
            for seq in 1..=60 {
                let input = Input {
                    throttle: 1.0,
                    steering: (seq as f32 / 10.0).sin(),
                    flags: if seq == 5 { 1 } else { 0 },
                    ..Input::default()
                };
                send(&mut writer, INPUT, seq, input);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        let log = std::env::temp_dir().join(format!("rbr-realtime-frames-{}.bin", std::process::id()));
        let file = Box::new(std::fs::File::create(&log).unwrap());
        let mut out = Vec::new();
        let live = serve_realtime(live, reader, &mut out, None, Some(file)).unwrap();
        feeder.join().unwrap();
        // The first answer, then step records and answers.
        let (mut rest, mut steps, mut answers, mut answered) = (&out[..], Vec::new(), 0, 0);
        while !rest.is_empty() {
            if rest[..4] == STP1 {
                steps.push(Step::from_body(rest[4..4 + STEP_BYTES].try_into().unwrap()));
                rest = &rest[4 + STEP_BYTES..];
                continue;
            }
            answered += u32::from_le_bytes(rest[12..16].try_into().unwrap());
            rest = &rest[FRAME_BYTES + HUD2_BYTES + DMG1_BYTES..];
            records::read_records(&mut rest).unwrap();
            let poses = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
            rest = &rest[4 + 32 * poses..];
            answers += 1;
        }
        // 432 frames in 0.6 s, fewer on a loaded machine (a frame that wakes late takes the steps it missed).
        assert!(steps.len() > 100, "{} step records in 0.6 s", steps.len());
        assert!(steps.windows(2).all(|w| w[1].step > w[0].step));
        let one = steps.windows(2).filter(|w| w[1].step == w[0].step + 1).count();
        assert!(one * 4 >= steps.len() * 3, "{one} of {} frames one step", steps.len());
        assert!(steps.iter().all(|s| s.ngp()) && steps.last().unwrap().seq == 60);
        // An answer per 7 ms of game time, about one per five steps, and they count every step.
        let ran = live.next_step() - live.start;
        assert!(answers * 6 >= ran && answers * 5 <= ran + 12, "{answers} answers of {ran} steps");
        assert!(answered <= ran && ran - answered < 6, "the answers count {answered} of {ran} steps");
        let line = replay_frames(boot(), &std::fs::read(&log).unwrap()).unwrap().1;
        std::fs::remove_file(&log).unwrap();
        assert!(line.contains("frames equal"), "{line}");
        eprintln!("{line}");
    })
    .unwrap();
}

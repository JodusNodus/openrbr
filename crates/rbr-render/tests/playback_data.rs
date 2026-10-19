//! Plays a recorded drive onto the car at the track-54_E start. Runs only when `RBR_DATA` (the extracted
//! `original/retail-1.0/install` tree) and `RBR_RECORDINGS` (the directory with `drive-<name>.bin`) are set.

use glam::Mat4;
use rbr_formats::data::Data;
use rbr_recording::clock::StepClock;
use rbr_render::car;
use rbr_render::playback::{Drive, Playback};
use rbr_render::stage::{self, Selection};
use std::path::PathBuf;

fn paths() -> Option<(PathBuf, PathBuf)> {
    let install = PathBuf::from(std::env::var_os("RBR_DATA")?);
    let recordings = PathBuf::from(std::env::var_os("RBR_RECORDINGS")?);
    Some((install, recordings))
}

#[test]
fn tanner_corolla_starts_at_the_stage_start() {
    let Some((install, recordings)) = paths() else {
        eprintln!("skipping: RBR_DATA or RBR_RECORDINGS not set");
        return;
    };
    let mut stage = stage::load(&install.join("Maps"), "track-54_E", Selection::default()).expect("track-54_E");
    let start = stage.start.expect("the stage has a start").position;
    let bytes = std::fs::read(recordings.join("drive-tanner-corolla.bin")).expect("recording");
    let drive = Drive::parse(&bytes).expect("frames");

    let data = Data::from_env().expect("RBR_DATA");
    let car = car::load(&data, "COROLLA", Mat4::IDENTITY).expect("COROLLA");
    let (model, damage) = (car.meshes.clone(), car.damage.clone());
    car.add_to_stage(&mut stage);
    let playback = Playback::load(&data, "COROLLA", &stage, &model, &damage, drive).expect("playback");

    let gap = (playback.position() - start).length();
    eprintln!("first frame {gap:.2} m from the start; duration {:.1} s", playback.duration());
    assert!(gap < 3.0, "first frame {gap} m from the start");
}

#[test]
fn a_finished_live_drive_becomes_the_best_ghost_only_when_faster() {
    use rbr_gameplay::ghost::{end_time, Best};
    use rbr_render::ghost::best_ghost;
    let Some((_, recordings)) = paths() else {
        eprintln!("skipping: RBR_DATA or RBR_RECORDINGS not set");
        return;
    };
    let bytes = std::fs::read(recordings.join("drive-tanner-corolla.bin")).expect("recording");
    let drive = Drive::parse(&bytes).expect("frames");
    let samples = drive.samples();
    // As if the finish line were passed at the drive's 10000th step: the ghost ends there.
    let end = samples.iter().find(|s| s.step >= 10_000).expect("a long drive").step;
    let ghost = best_ghost(samples, StepClock::default(), glam::Vec3::ZERO, 54, Some((end, false)), None)
        .expect("no stored ghost");
    let time = end_time(&ghost);
    assert!((time - (end - 160) as f32 * rbr_recording::STEP_SECONDS).abs() < 0.05, "{time}");
    let stored = |time| Some(Best { map: 54, time });
    assert!(
        best_ghost(samples, StepClock::default(), glam::Vec3::ZERO, 54, Some((end, false)), stored(time + 1.0)).is_ok()
    );
    assert!(best_ghost(samples, StepClock::default(), glam::Vec3::ZERO, 54, Some((end, false)), stored(time)).is_err());
    assert!(best_ghost(samples, StepClock::default(), glam::Vec3::ZERO, 54, Some((end, true)), None).is_err());
    assert!(best_ghost(samples, StepClock::default(), glam::Vec3::ZERO, 54, None, None).is_err());
    assert!(best_ghost(
        samples,
        StepClock::default(),
        glam::Vec3::ZERO,
        54,
        Some((end, false)),
        Some(Best { map: 33, time: 1.0 })
    )
    .is_ok());
}

/// An NGP drive's best ghost runs on NGP's step clock (1/720 s steps after a settle of 1520): the drive of
/// `rbr-standalone --autopilot OUT --boot --ngp-physics ...` in `RBR_NGP_DRIVE`, its finish line's step in
/// `RBR_NGP_FINISH`. Skipped without them.
#[test]
fn an_ngp_drive_becomes_the_best_ghost_on_ngp_time() {
    use rbr_gameplay::ghost::end_time;
    use rbr_render::ghost::best_ghost;
    let (Some(path), Some(finish)) = (std::env::var_os("RBR_NGP_DRIVE"), std::env::var("RBR_NGP_FINISH").ok()) else {
        eprintln!("skipping: RBR_NGP_DRIVE or RBR_NGP_FINISH not set");
        return;
    };
    let finish: u32 = finish.parse().expect("RBR_NGP_FINISH: a step");
    let drive = Drive::parse(&std::fs::read(path).expect("the NGP drive")).expect("frames");
    let ngp = StepClock { step: f32::from_bits(0x3ab6_0b61), settle: 1520 };
    let ghost =
        best_ghost(drive.samples(), ngp, glam::Vec3::ZERO, 11, Some((finish, false)), None).expect("no stored ghost");
    let time = end_time(&ghost);
    let expected = f64::from(finish - 1520) / 720.0;
    eprintln!("NGP ghost: {} samples, {time:.3} s (steps after the settle: {expected:.3} s)", ghost.samples.len());
    assert!((f64::from(time) - expected).abs() < 0.05, "{time} against {expected}");
}

/// The original's own `.ghc`: `CGhostCarManager::Save` (0x492eb0) run in the emulator on the tanner drive's snapshot
/// at step 11000 (the live recording), kept as `golden-tanner-corolla-11000.ghc` beside
/// the recordings. It reads back byte for byte, names map 54 and the Corolla (6) with the run's weather words, and its
/// points are the car model's root: the drive's body position plus the rotated `-vecCenterOfGravity` of
/// `t_coroll/common.lsp`, (0, 1.37, -0.22), which `rbr_render::ghost` draws with no offset. The harness ran the game
/// faster than its frame clock, so the stream's times (about 9.5 s for 76 s of driving) are not compared; each point
/// is matched to the nearest frame of the drive.
#[test]
fn the_original_ghost_save_reads_back_and_holds_the_model_root() {
    use rbr_formats::ghost::{SavedGhost, Weather};
    use rbr_gameplay::ghost::Player;
    let Some((_, recordings)) = paths() else {
        eprintln!("skipping: RBR_DATA or RBR_RECORDINGS not set");
        return;
    };
    let Ok(bytes) = std::fs::read(recordings.join("golden-tanner-corolla-11000.ghc")) else {
        eprintln!("skipping: no golden-tanner-corolla-11000.ghc");
        return;
    };
    let ghost = SavedGhost::parse(&bytes).expect("the checksum and layout");
    assert_eq!(ghost.write(), bytes, "written back byte for byte");
    assert_eq!((ghost.map, ghost.car), (54, 6));
    let words = Weather {
        track_letter: 2,
        cloud: 0,
        weather: 0,
        cloud_letter: 2,
        wear: 0,
        wetness: 1,
        amount: 0.0,
        unk3c: 15.0,
    };
    assert_eq!(ghost.weather, words);

    let drive = Drive::parse(&std::fs::read(recordings.join("drive-tanner-corolla.bin")).unwrap()).unwrap();
    let samples: Vec<_> = drive.samples().iter().filter(|s| s.step <= 11_000).collect();
    let root = glam::Vec3::new(0.0, 1.37, -0.22);
    let at = |s: &rbr_render::playback::Sample| {
        glam::Vec3::from(s.car.position) + glam::Quat::from_array(s.car.orientation) * root
    };
    let mut player = Player::new(&ghost.stream);
    let (mut worst, mut last, mut n, mut close) = (0.0f32, 0usize, 0, 0);
    // At each sample's own time (1/1024 s units), where the player emits the sample itself. Most are within 2 cm of
    // the drive; the in-between samples the recorder adds over a long frame (the harness's frames span 12 to 25 steps
    // at times) lie on the chord, up to 0.56 m off the curve.
    for t in ghost.stream.samples.iter().map(|s| s.time() as f32 / 1024.0) {
        let Some(pose) = player.advance(t) else { break };
        let p = glam::Vec3::from(pose.position);
        let (k, gap) = (last.saturating_sub(50)..samples.len().min(last + 4000))
            .map(|k| (k, (at(samples[k]) - p).length()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        close += usize::from(gap < 0.05);
        (last, worst, n) = (k, worst.max(gap), n + 1);
    }
    eprintln!(
        "{n} points, {close} within 5 cm of the drive's model root, the worst {worst:.3} m; the last at step {}",
        samples[last].step
    );
    assert!(n > 5000 && samples[last].step > 10_900);
    assert!(close * 100 > n * 95 && worst < 0.6, "{close} of {n}, worst {worst}");
}

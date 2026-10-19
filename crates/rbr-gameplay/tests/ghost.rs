//! Plays every retail ghost (`Root/ghosts.rbz`) through `rbr_gameplay::ghost::Player`, and records the reference drives
//! with `Recorder`. Runs only when `RBR_DATA` (the install tree) or `RBR_RECORDINGS` is set.

use rbr_formats::ghost::{Ghost, Stream};
use rbr_formats::rbz;
use rbr_gameplay::ghost::{Player, Pose, Recorder};
use rbr_recording::{pose, Frames};
use std::io::Read;
use std::path::Path;

/// The frame the ghost tests play at, s.
const FRAME: f32 = 0.02;

fn retail_ghosts() -> Option<Vec<(String, Ghost)>> {
    let data = std::env::var_os("RBR_DATA")?;
    let mut archive = rbz::open(&Path::new(&data).join("Root/ghosts.rbz")).ok()?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().unwrap().into_owned();
        if name.ends_with(".gho") {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            out.push((name, Ghost::parse(&bytes).unwrap()));
        }
    }
    Some(out)
}

/// Plays `stream` at [`FRAME`] steps; returns the poses emitted.
fn play(stream: &Stream) -> Vec<(f32, Pose)> {
    let mut player = Player::new(stream);
    let mut out = Vec::new();
    let mut t = 0.0;
    loop {
        t += FRAME;
        match player.advance(t) {
            Some(pose) => out.push((t, pose)),
            None => return out,
        }
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

#[test]
fn every_retail_ghost_plays_smoothly_to_its_end() {
    let Some(ghosts) = retail_ghosts() else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    for (name, ghost) in ghosts {
        let poses = play(&ghost.stream);
        let end = poses.last().unwrap().0;
        let duration = Player::new(&ghost.stream).duration();
        assert!(end >= duration && end < duration + 2.0 * FRAME, "{name}: ends at {end}, last sample {duration}");
        // No stage ghost goes faster than 80 m/s, so no frame step is longer than 1.6 m. `rse0` is mostly keys, every
        // 0.1 s and 2 m apart, and the playback does not interpolate into a key: its ghost jumps.
        let fastest = poses.windows(2).map(|w| distance(w[0].1.position, w[1].1.position)).fold(0.0, f32::max);
        assert!(name.contains("rse0") || fastest < 80.0 * FRAME, "{name}: a frame step of {fastest} m");
        let norm = |q: [f32; 4]| q.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(poses.iter().all(|(_, p)| (norm(p.rotation) - 1.0).abs() < 1e-3), "{name}: unit rotations");
        eprintln!("{name}: {} frames, {end:.2} s, fastest step {fastest:.3} m", poses.len());
    }
}

/// Records a reference drive at one recording frame (one `StepForward`) per game frame, then plays it back: the ghost
/// is one frame ahead of the car (see the module docs) and within a few centimetres of the frame it shows.
#[test]
fn a_recorded_drive_plays_back_on_its_path() {
    let Some(dir) = std::env::var_os("RBR_RECORDINGS") else {
        eprintln!("skipping: RBR_RECORDINGS not set");
        return;
    };
    let bytes = std::fs::read(Path::new(&dir).join("drive-tanner-corolla.bin")).unwrap();
    let mut recorder = Recorder::new();
    let mut drive = Vec::new();
    let mut t = 0.0;
    for frame in Frames::new(&bytes) {
        let frame = frame.unwrap();
        let car = pose::car_pose(frame.car).unwrap();
        let pose = Pose { position: car.position, rotation: car.orientation };
        recorder.car_state(pose);
        let dt = frame.substeps as f32 * 0.01;
        recorder.frame(dt);
        t += dt;
        drive.push((t, pose));
    }
    let stream = recorder.finish();
    let back = Stream::parse(&stream.write());
    assert_eq!(back, stream);
    let mut player = Player::new(&stream);
    let mut worst = 0.0f32;
    for w in drive.windows(2) {
        let Some(ghost) = player.advance(w[0].0) else {
            break;
        };
        worst = worst.max(distance(ghost.position, w[1].1.position));
    }
    eprintln!("{} samples over {:.1} s; worst offset from the next frame {worst:.3} m", stream.samples.len(), t);
    assert!(worst < 0.1, "worst {worst}");
}

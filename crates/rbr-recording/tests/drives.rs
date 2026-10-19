//! Poses from the four reference drives. Runs only when `RBR_RECORDINGS` names the directory with
//! `drive-<name>.bin` and `drive-<name>.auto`; the start-location check also needs `RBR_DATA`, the retail install tree
//! (`original/retail-1.0/install`), and is skipped without it.

use rbr_formats::lbs::{self, Chunk};
use rbr_recording::{pose, Frames};
use std::path::{Path, PathBuf};

const DRIVES: [&str; 4] = ["tanner-corolla", "tanner-damage", "arctic-snow", "tarmac-lancer"];
/// Frames whose position moves more than this, m, between two frames are reported as jumps.
const MAX_STEP_METRES: f32 = 1.0;
/// Start location tolerance, m: the first frame sits where the stage's `(2, 11)` start is.
const START_METRES: f32 = 3.0;
/// Wheel anchors stay within this distance of the car's origin, m, in the car frame.
const ANCHOR_METRES: f32 = 3.0;
/// Speed bound, m/s, for a plausible frame (360 km/h).
const MAX_SPEED: f32 = 100.0;

fn recordings_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("RBR_RECORDINGS")?;
    Some(PathBuf::from(dir))
}

/// The `StartGame` map number, the first word of `drive-<name>.auto`.
fn map_number(dir: &Path, name: &str) -> u32 {
    let auto =
        std::fs::read_to_string(dir.join(format!("drive-{name}.auto"))).unwrap_or_else(|e| panic!("{name}: {e}"));
    auto.split_whitespace()
        .next()
        .and_then(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("{name}: no map number in {auto:?}"))
}

/// The start location of every `Maps/track-<map>_*.lbs` variant of the stage, in file-name order.
fn stage_starts(data: &Path, map: u32) -> Vec<(String, [f32; 3])> {
    let maps = data.join("Maps");
    let prefix = format!("track-{map}_");
    let mut files: Vec<_> = std::fs::read_dir(&maps)
        .unwrap_or_else(|e| panic!("{}: {e}", maps.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".lbs")
        })
        .collect();
    files.sort();
    files
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let file = lbs::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let start = file
                .chunks
                .iter()
                .find_map(|chunk| match chunk {
                    Chunk::StartLocation(s) => Some(s.position),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{}: no start location", path.display()));
            (path.file_name().unwrap().to_string_lossy().into_owned(), start)
        })
        .collect()
}

/// The `.lbs` start is y-up; the physics is z-up, so its y and z are the file's z and y (`rbr_formats::lbs`).
fn to_physics([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, z, y]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

#[test]
fn reference_drives_give_finite_plausible_poses() {
    let Some(dir) = recordings_dir() else {
        eprintln!("skipping: RBR_RECORDINGS not set");
        return;
    };
    for name in DRIVES {
        let path = dir.join(format!("drive-{name}.bin"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut first: Option<[f32; 3]> = None;
        let mut previous: Option<[f32; 3]> = None;
        let mut max_speed = 0.0f32;
        let mut max_anchor = 0.0f32;
        let mut jumps = Vec::new();
        let mut frames = 0usize;
        for frame in Frames::new(&bytes) {
            let frame = frame.unwrap_or_else(|e| panic!("{name}: {e}"));
            let car =
                pose::car_pose(frame.car).unwrap_or_else(|| panic!("{name}: short car block at {}", frame.offset));
            assert!(
                all_finite(&car.position) && all_finite(&car.orientation) && all_finite(&car.velocity),
                "{name}: non-finite car pose at offset {}",
                frame.offset
            );
            let norm = car.orientation.iter().map(|q| q * q).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3, "{name}: quaternion norm {norm} at {}", frame.offset);
            for i in 0..rbr_recording::WHEELS {
                let wheel =
                    pose::wheel_pose(frame.wheel(i).unwrap()).unwrap_or_else(|| panic!("{name}: short wheel block"));
                assert!(
                    all_finite(&wheel.anchor) && all_finite(&wheel.orientation),
                    "{name}: non-finite wheel {i} at offset {}",
                    frame.offset
                );
                max_anchor = max_anchor.max(wheel.anchor.iter().map(|v| v * v).sum::<f32>().sqrt());
            }
            max_speed = max_speed.max(car.speed());
            if let Some(prev) = previous {
                let moved = distance(prev, car.position);
                if moved > MAX_STEP_METRES {
                    jumps.push((frame.post, moved));
                }
            }
            first.get_or_insert(car.position);
            previous = Some(car.position);
            frames += 1;
        }
        eprintln!(
            "{name}: {frames} frames, max speed {max_speed:.1} m/s, max anchor {max_anchor:.2} m, {} jumps over {MAX_STEP_METRES} m",
            jumps.len()
        );
        for (post, moved) in jumps.iter().take(20) {
            eprintln!("  jump at step {post}: {moved:.2} m");
        }
        assert!(max_speed < MAX_SPEED, "{name}: speed {max_speed} m/s");
        assert!(max_anchor < ANCHOR_METRES, "{name}: wheel anchor {max_anchor} m from the car");
        if name != "tarmac-lancer" {
            assert!(jumps.is_empty(), "{name}: {} jumps", jumps.len());
        }

        let Some(data) = std::env::var_os("RBR_DATA").map(PathBuf::from) else {
            eprintln!("  start check skipped: RBR_DATA not set");
            continue;
        };
        let map = map_number(&dir, name);
        let starts = stage_starts(&data, map);
        assert!(!starts.is_empty(), "{name}: no track-{map} lbs in {}", data.display());
        let start = to_physics(starts[0].1);
        for (file, other) in &starts {
            let other = to_physics(*other);
            assert!(distance(start, other) < 0.5, "{name}: {file} starts at {other:?}, not {start:?}");
        }
        let first = first.expect("at least one frame");
        let gap = distance(first, start);
        eprintln!("  {}: first position {gap:.2} m from its start {start:?}", starts[0].0);
        assert!(gap < START_METRES, "{name}: first frame {first:?} is {gap} m from {start:?}");
    }
}

/// The body frame the module docs prove (`rbr_recording::pose`): body `+Z` is up, the car drives along body `-Y`, and
/// the wheel anchors give the track and wheelbase in world coordinates on every frame.
#[test]
fn drives_fit_the_body_frame() {
    let Some(dir) = recordings_dir() else {
        eprintln!("skipping: RBR_RECORDINGS not set");
        return;
    };
    for name in DRIVES {
        let path = dir.join(format!("drive-{name}.bin"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (mut alignment, mut aligned, mut upright_sum, mut frames) = (0.0f32, 0usize, 0.0f32, 0usize);
        let (mut track_range, mut base_range) = ((f32::MAX, 0.0f32), (f32::MAX, 0.0f32));
        let mut z_range = (f32::MAX, f32::MIN);
        for frame in Frames::new(&bytes) {
            let frame = frame.unwrap_or_else(|e| panic!("{name}: {e}"));
            let car = pose::car_pose(frame.car).unwrap();
            let up = car.rotate([0.0, 0.0, 1.0])[2];
            upright_sum += up;
            frames += 1;
            let speed = car.speed();
            if speed > 5.0 {
                let v = car.velocity.map(|c| c / speed);
                alignment += dot(car.forward(), v);
                aligned += 1;
            }
            let anchors: [[f32; 3]; 4] =
                std::array::from_fn(|i| pose::wheel_pose(frame.wheel(i).unwrap()).unwrap().anchor);
            assert!(anchors[0][1] < -0.5 && anchors[1][1] < -0.5, "{name}: front anchors {anchors:?}");
            assert!(anchors[2][1] > 0.5 && anchors[3][1] > 0.5, "{name}: rear anchors {anchors:?}");
            let world: [[f32; 3]; 4] = anchors.map(|a| car.to_world(a));
            let track = distance(world[0], world[1]);
            let front = mid(world[0], world[1]);
            let rear = mid(world[2], world[3]);
            let base = distance(front, rear);
            track_range = (track_range.0.min(track), track_range.1.max(track));
            base_range = (base_range.0.min(base), base_range.1.max(base));
            for a in anchors {
                z_range = (z_range.0.min(a[2]), z_range.1.max(a[2]));
            }
        }
        let mean_up = upright_sum / frames as f32;
        let mean_align = alignment / aligned as f32;
        eprintln!(
            "{name}: forward·v̂ {mean_align:.3} over {aligned} frames, body up {mean_up:.3}, track {:.3}..{:.3} m, \
             wheelbase {:.3}..{:.3} m, anchor z {:.3}..{:.3}",
            track_range.0, track_range.1, base_range.0, base_range.1, z_range.0, z_range.1
        );
        assert!(mean_align > 0.9, "{name}: forward·v̂ {mean_align}");
        assert!(mean_up > 0.9, "{name}: body up {mean_up}");
        assert!(track_range.0 > 1.4 && track_range.1 < 1.6, "{name}: track {track_range:?}");
        assert!(base_range.0 > 2.4 && base_range.1 < 2.7, "{name}: wheelbase {base_range:?}");
        assert!(z_range.0 > -0.25 && z_range.1 < 0.25, "{name}: anchor height {z_range:?}");
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mid(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
}

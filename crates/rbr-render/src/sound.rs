//! The live drive's sound (`rbr_sound::Player`): the car's `Cars.ini` index from its folder name, the player the
//! viewer feeds each step's SND1 record (`rbr_recording::sound`), and the camera and car it hears them from.

use glam::Vec3;
use rbr_formats::cars;
use rbr_formats::data::Data;
use rbr_recording::pose::CarPose;
pub use rbr_sound::custom_codriver::Choice;
use rbr_sound::custom_codriver::{self, CustomCoDriver};
pub use rbr_sound::data::Wave;
pub use rbr_sound::mixer::Volumes;
pub use rbr_sound::{Events, Player, Scene, Sky};

use crate::camera::Lens;
use crate::characters::scripted::Event;

/// The `Cars/Cars.ini` index of the car in folder `name` (`COROLLA`), or `None`.
pub fn car_index(data: &Data, name: &str) -> Option<u32> {
    let text = std::fs::read(data.resolve("Cars\\Cars.ini")?).ok()?;
    let entries = data.car_slots(cars::parse(&String::from_utf8_lossy(&text)).ok()?);
    let folder = format!("cars\\{}\\", name.to_ascii_lowercase());
    entries
        .iter()
        .find(|e| e.ini_file.to_ascii_lowercase().replace('/', "\\").contains(&folder))
        .map(|e| e.index as u32)
}

/// The car's sound at `volume` under `sky`, with the co-driver's pace notes or not and the player's group
/// volumes (`rbr_sound::mixer::Volumes`), or `None` (with the reason on
/// stderr) when the data or the audio device is missing.
pub fn start(data: &Data, car: &str, volume: f32, sky: Sky, codriver: bool, volumes: Volumes) -> Option<Player> {
    let index = car_index(data, car).unwrap_or_else(|| {
        eprintln!("viewer: sound: {car} is not in Cars.ini; car 0's sound");
        0
    });
    match Player::start(data, index, volume, sky, codriver, volumes) {
        Ok(player) => Some(player),
        Err(e) => {
            eprintln!("viewer: no sound: {e}");
            None
        }
    }
}

/// Plays the RSF car's FMOD bank at `path` (`rbr_sound::fmod`) instead of its slot's 1.02 engine, turbo and
/// transmission sounds; without it (the reason on stderr) those stay.
pub fn use_fmod(player: &mut Player, path: &std::path::Path) {
    match rbr_sound::fmod::FmodCar::load(path) {
        Ok(car) => {
            eprintln!("viewer: sound: {} ({} recordings)", path.display(), car.waves.len());
            player.use_fmod(car);
        }
        Err(e) => eprintln!("viewer: sound: no FMOD bank: {e}"),
    }
}

/// Calls the pace notes with the co-driver mod `choice` instead of 1.02's, for the stage `stage` (a name or path, its
/// `.dls` the notes without a `choice.notes` file) and the call-out distance `callout_time`; on an error (on stderr) the
/// 1.02 co-driver stays.
pub fn use_codriver(player: &mut Player, data: &Data, stage: &str, choice: &Choice, callout_time: f32) {
    let load = || {
        let dls = if choice.notes.is_none() { custom_codriver::stage_notes(data, stage)? } else { Vec::new() };
        CustomCoDriver::load(choice, dls, callout_time)
    };
    match load() {
        Ok(custom) => {
            eprintln!("viewer: sound: co-driver {} ({} samples)", choice.voice, custom.samples().len());
            player.use_codriver(custom);
        }
        Err(e) => eprintln!("viewer: co-driver mod: {e}"),
    }
}

/// The sound's scene for the camera `lens` (stage frame) and the car at `car` (physics frame): the stage's y up is
/// the physics' z up (`crate::coords`), so the eye and its right swap y and z.
pub fn scene(lens: &Lens, car: &CarPose, inside: bool) -> Scene {
    let swap = |v: Vec3| [v.x, v.z, v.y];
    // Left-handed, y up: right is up x forward.
    let right = lens.up.cross(lens.forward).normalize_or_zero();
    Scene {
        listener: swap(lens.eye),
        right: swap(right),
        car: car.position,
        velocity: car.velocity,
        inside,
        ..Scene::default()
    }
}

/// The sound's scripted events from the characters' `events` and the helicopter body at `helicopter` (stage frame,
/// `Characters::rescue_position`).
pub fn events(events: &[Event], helicopter: Option<Vec3>) -> Events {
    let physics = |p: Vec3| [p.x, p.z, p.y];
    Events {
        helicopter: helicopter.map(physics),
        birdstrikes: events
            .iter()
            .filter_map(|e| if let Event::Birdstrike(p) = e { Some(physics(*p)) } else { None })
            .collect(),
        rescue_called: events.iter().any(|e| matches!(e, Event::RescueCalled(_))),
    }
}

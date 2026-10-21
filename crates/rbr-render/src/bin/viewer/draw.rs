//! What a screenshot and a window frame share: the cameras, the characters and crew, and the HUD's readout.

use crate::args::HudOptions;
use glam::{Mat4, Vec3};
use rbr_render::bindings::{Action, Bindings};
use rbr_render::camera::{FreeFly, Lens};
use rbr_render::characters::incar::Crew;
use rbr_render::characters::scripted::Event;
use rbr_render::characters::Characters;
use rbr_render::dials;
use rbr_render::game_camera;
use rbr_render::ghost;
use rbr_render::hud::model::Lamp;
use rbr_render::hud::Readout;
use rbr_render::live::{self, Live};
use rbr_render::particles;
use rbr_render::playback::{Playback, CHASE_DISTANCE, CHASE_HEIGHT, CHASE_TARGET_HEIGHT};
use rbr_render::renderer::Renderer;
use rbr_render::stage::Stage;
use rbr_render::views::{CarView, Shown};

/// Eye height above the car start position, metres.
const EYE_HEIGHT: f32 = 1.5;

/// Gives the particles the rescue helicopter's dust the characters threw.
pub fn throw_smoke(characters: Option<&mut Characters>, drawn: &mut particles::Drawn) {
    if let Some((kind, smoke)) = characters.and_then(Characters::take_smoke) {
        drawn.particles.rescue_smoke(kind, smoke);
    }
}

/// The camera the scripted events force, and the lens for [`Shown::lens`]: the helicam over the flip camera. The
/// rescue helicopter's mode 8 sets no field of view and keeps the last mode's, the game's 75 in every mode the player
/// reaches (`game_camera::fov`); the car-flip camera's mode 13 sets the same 75.
pub fn scripted_camera(characters: Option<&Characters>) -> (bool, bool, Option<Lens>) {
    let fov = game_camera::fov(game_camera::FOV_DEGREES);
    let heli = characters.and_then(|c| c.helicam(fov));
    let flip = characters.and_then(|c| c.flipcam(fov));
    (heli.is_some(), flip.is_some(), heli.or(flip))
}

/// Steps the crew to the frame shown, turns the steering wheel (to `steer` when given, a step newer than the frame)
/// and poses them in the car.
pub fn show_crew(crew: &mut Crew, playback: &mut Playback, view: CarView, steer: Option<f32>, renderer: &mut Renderer) {
    crew.follow(playback.frames(), playback.index());
    if let Some(steer) = steer {
        crew.steer_to(steer);
    }
    playback.turn = crew.wheel_turn();
    crew.apply(renderer, playback.body(), view.shows_crew());
}

/// Steps the characters to the frame shown (the drive's frames, else `seconds` of viewer time), with the start
/// countdown (live play's HUD2, else the drive's race time from 7 s, ours); returns the scripted events.
/// `Characters::apply` poses them once the frame's camera is known.
pub fn step_characters(
    characters: &mut Characters,
    playback: Option<&Playback>,
    seconds: f64,
    race: Option<live::RaceHud>,
) -> Vec<Event> {
    match playback {
        Some(playback) => {
            // Live play's HUD is the shown frame's.
            let shown = playback.sample().step;
            let countdown = |sample: &rbr_render::playback::Sample| match race {
                Some(race) => (sample.step == shown).then_some((race.countdown, race.jump_start)),
                None => {
                    let time = ghost::race_time(sample, playback.step_clock());
                    Some((rbr_gameplay::race_clock::COUNTDOWN - time, false))
                }
            };
            characters.follow(playback.frames(), playback.index(), countdown)
        }
        None => characters.step(seconds, None),
    }
    let events = characters.take_events();
    for event in &events {
        eprintln!("scripted event: {event:?}");
    }
    events
}

/// The HUD's values for the frame shown: the live process's gear, engine speed and speed when there is one (the
/// recordings have the speed only), and the controls fed to the frame's step; `bindings` name the camera keys.
pub fn readout(
    playback: &Playback,
    live: Option<&Live>,
    view: Shown,
    bindings: &Bindings,
    hud: HudOptions,
    fps: Option<f32>,
    view_projection: Mat4,
) -> Readout {
    let label = |action| bindings.get(action).label();
    let sample = playback.sample();
    let [throttle, brake, handbrake, clutch, steering, ..] = sample.control;
    let options = hud;
    let hud = live.map(|l| l.hud);
    Readout {
        view: view.name(),
        view_keys: label(Action::Camera),
        time: playback.time(),
        speed: hud.map_or(sample.car.speed(), |h| h.speed),
        mph: options.mph,
        fps,
        gear: hud.map(|h| h.gear_label()),
        rpm: hud.map(|h| h.rpm()),
        throttle,
        brake,
        handbrake,
        clutch,
        steering,
        race: hud.and_then(|h| h.race),
        splits: live.and_then(|l| l.splits.clone()),
        state: live.map(|l| l.hud_model.view()).unwrap_or_default(),
        pace_3d: (options.pace_3d && live.is_some()).then_some(view_projection),
    }
}

/// The camera at the car start location, looking where the car faces. With `chase`, it is behind the
/// car and looks at it. Without a start location it looks along +z from the origin.
pub fn start_camera(stage: &Stage, chase: bool) -> FreeFly {
    match stage.start {
        Some(pose) if chase => {
            let eye = pose.position - pose.forward * CHASE_DISTANCE + Vec3::Y * CHASE_HEIGHT;
            let target = pose.position + Vec3::Y * CHASE_TARGET_HEIGHT;
            FreeFly::looking_along(eye, target - eye)
        }
        Some(pose) => FreeFly::looking_along(pose.position + Vec3::Y * EYE_HEIGHT, pose.forward),
        None => FreeFly::looking_along(Vec3::ZERO, Vec3::Z),
    }
}

/// The values the dials show for the frame shown: the live process's engine speed, gear, race clock and dashboard when
/// there is one (a recording has the speed only).
pub fn dial_values(playback: &Playback, live: Option<&Live>) -> dials::Values {
    let hud = live.map(|l| l.hud);
    let view = live.map(|l| l.hud_model.view()).unwrap_or_default();
    let lit = |i: usize| view.lamps.is_some_and(|lamps| lamps[i] != Lamp::Off);
    let [throttle, brake, handbrake, _, steering, ..] = playback.sample().control;
    dials::Values {
        water: view.water,
        boost: view.boost,
        odometer: view.odometer.map(|(m, _)| m),
        to_go: view.odometer.and_then(|(_, to_go)| to_go),
        hot: lit(0),
        engine: lit(2),
        battery: lit(3),
        stalled: lit(4),
        rpm: hud.map_or(0.0, |h| h.rpm()),
        kmh: hud.map_or(playback.sample().car.speed(), |h| h.speed) * 3.6,
        gear: hud.map_or(1, |h| h.gear),
        race_time: hud.and_then(|h| h.race).map_or(0.0, |r| r.time),
        throttle,
        brake,
        handbrake,
        steering,
        shift: hud.and_then(|h| h.shift),
        volts: hud.and_then(|h| h.volts),
        body: view.body,
    }
}

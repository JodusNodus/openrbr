//! Plays a recorded drive back on a car in a stage (`viewer --drive`). The body and the four tyres follow the poses
//! in an `rbr-recording` file, and the chase camera follows the body.
//!
//! Frames: the physics is z-up and the stage y-up, with `stage = (x, z, y)` of the physics (the start check in
//! `rbr-recording`'s drive test). The car model has its front at `-z` and its up at `+y`. [`model_to_body`] maps it to the
//! physics body frame, whose front is `-Y` and up `+Z` (`rbr_recording::pose`), and keeps `+X`. The stage swap is a
//! mirror and the pose a rotation, so `model_to_body` must mirror too: the model to the stage is then a proper
//! rotation, as [`crate::coords::car_to_stage`] is, and the outer faces stay the front faces. A proper `model_to_body`
//! turned the car inside out: back-face culling dropped the outside and showed the black interior (render-m10).
//! So the model's `+X`, its left (the `l` tyres), is body `+X`, where wheels 0 and 2 are mounted.
//!
//! Each wheel is placed by its anchor (`WheelPose::anchor`, the car-frame mount point). The car's rigid offset to the
//! model is the mean over the four wheels of `anchor - model_to_body(centre)`, so the body meshes sit where the
//! wheels are. The roll turns each wheel about the model's lateral axis, so that a positive roll rolls the top of the
//! tyre forward: the recorded roll grows at about 2.2 rad per metre of forward travel (`rbr-recording` notes).

use crate::camera::{FreeFly, Lens};
use crate::coords::physics_to_stage;
use crate::damage::{self, Cracks, MeshDamage, Zones};
use crate::damage_parts::Detach;
use crate::game_camera::Follow;
use crate::lights::{self, Flare, Inputs, Lamps};
use crate::renderer::{mesh_bounds, Renderer};
use crate::stage::{Mesh, Stage, Vertex};
use crate::stage_probe::StageProbe;
use crate::views::{CarView, Mount, Mounts};
use glam::{Mat4, Quat, Vec3};
use rbr_formats::data::Data;
use rbr_recording::clock::StepClock;
use rbr_recording::detach::{self, PartPose};
use rbr_recording::pose::CarPose;
use rbr_recording::WHEELS;
use std::cell::Cell;

mod drive;
mod frames;

pub use drive::{Drive, Sample, CAMERA_SHAKE};
pub(crate) use frames::model_path;
pub use frames::{car_to_stage, root_to_stage};
use frames::{model_to_body, physics_pose, tyre_nodes, WHEEL_ROLL_AXIS};

/// The fallback chase before the game's has followed a frame, and the viewer's chase view at the start: the eye is
/// this far behind and above the body, and it looks at the body's middle. Ours.
pub const CHASE_DISTANCE: f32 = 6.0;
pub const CHASE_HEIGHT: f32 = 2.5;
pub const CHASE_TARGET_HEIGHT: f32 = 0.5;
/// The chase camera's horizontal field of view, degrees. Ours, and fixed: the `--fov` setting (the in-car views')
/// does not change it.
pub const CHASE_FOV_DEGREES: f32 = 65.0;

/// Lowest and highest elevation of the chase eye above the target, radians.
const LOOK_PITCH_MIN: f32 = -0.1;
const LOOK_PITCH_MAX: f32 = 1.4;
/// Nearest and farthest chase eye, as a factor of the default distance.
const LOOK_ZOOM_MIN: f32 = 0.3;
const LOOK_ZOOM_MAX: f32 = 8.0;

/// How the viewer has turned the chase camera around the car: `yaw` about world up (positive towards +x, as
/// [`FreeFly`]), `pitch` up, `zoom` times the default distance. The default is the plain chase view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChaseLook {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

impl Default for ChaseLook {
    fn default() -> ChaseLook {
        ChaseLook { yaw: 0.0, pitch: 0.0, zoom: 1.0 }
    }
}

impl ChaseLook {
    /// Turns the view by mouse deltas, radians.
    pub fn turn(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch += pitch_delta;
    }

    /// Multiplies the distance by `factor`, within the limits.
    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(LOOK_ZOOM_MIN, LOOK_ZOOM_MAX);
    }

    /// `offset` (the eye from the target) turned by the yaw, raised by the pitch within the elevation limits, and
    /// scaled by the zoom.
    fn apply(&self, offset: Vec3) -> Vec3 {
        let distance = offset.length() * self.zoom;
        let heading = offset.x.atan2(offset.z) + self.yaw;
        let elevation = ((offset.y / offset.length()).asin() + self.pitch).clamp(LOOK_PITCH_MIN, LOOK_PITCH_MAX);
        let (sin_heading, cos_heading) = heading.sin_cos();
        let (sin_elevation, cos_elevation) = elevation.sin_cos();
        Vec3::new(sin_heading * cos_elevation, sin_elevation, cos_heading * cos_elevation) * distance
    }
}

/// A drawn car mesh and what moves it.
#[derive(Debug, Clone, PartialEq)]
struct Part {
    /// Its index in the stage's meshes.
    stage: usize,
    /// The model-space vertices as loaded.
    rest: Vec<Vertex>,
    /// The tyre this mesh is, with its hub: the tyre node's model-space position, the pivot of the roll. (The mesh's
    /// own box centre is 0.07 m off it, which would give a 0.2 m narrower track than the anchors.) `None` for the body.
    wheel: Option<(usize, Vec3)>,
    /// In the interior set ([`Playback::add_interior`]), drawn instead of the exterior in the in-car views.
    interior: bool,
    /// Its model-space centre (the steering wheel is found by it, [`Playback::turn`]).
    centre: Option<Vec3>,
    /// What damage does to it.
    effect: MeshDamage,
    /// The detachable part it is ([`detach::PARTS`]), placed by its body once detached ([`Playback::set_detached`]).
    detach: Option<usize>,
    /// Per vertex, its offset and scrape alpha under the current zones; empty when undamaged.
    damage: Vec<(Vec3, u8)>,
    /// A model-space motion before the pose (the wipers, [`Playback::set_motion`]).
    motion: Mat4,
    /// The renderer holds its model-space vertices with the current damage ([`Renderer::set_car_vertices`]; the
    /// playback is drawn by one renderer).
    in_car: Cell<bool>,
}

impl Part {
    fn new(stage: usize, mesh: &Mesh, effect: &MeshDamage, wheel: Option<(usize, Vec3)>, interior: bool) -> Part {
        let centre = mesh.bounds.map(|b| b.center);
        let detach = effect.detach.map(|k| k as usize);
        Part {
            stage,
            rest: mesh.vertices.clone(),
            wheel,
            interior,
            centre,
            effect: effect.clone(),
            detach,
            damage: Vec::new(),
            motion: Mat4::IDENTITY,
            in_car: Cell::new(false),
        }
    }

    /// Its vertices, dented and scraped, placed by `matrix`.
    fn placed(&self, matrix: Mat4) -> Vec<Vertex> {
        self.rest
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let (offset, scrape) = self.damage.get(i).copied().unwrap_or((Vec3::ZERO, v.color[3]));
                Vertex {
                    position: matrix.transform_point3(Vec3::from(v.position) + offset).to_array(),
                    normal: matrix.transform_vector3(Vec3::from(v.normal)).to_array(),
                    color: [v.color[0], v.color[1], v.color[2], scrape],
                    ..crate::car::cabin::turned(v, matrix)
                }
            })
            .collect()
    }
}

/// The playback clock, the drive and the car's parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    drive: Drive,
    parts: Vec<Part>,
    /// Model-to-body offset of the body meshes (see the module docs).
    offset: Vec3,
    /// Drive time at the viewer's start, seconds from the first frame.
    clock: f64,
    /// Whether the clock stands still.
    pub paused: bool,
    /// Drive seconds per real second.
    pub rate: f64,
    /// The interior set is drawn instead of the exterior (the cockpit view, `crate::views`).
    pub interior: bool,
    /// The damage zones the parts are dented for ([`Playback::set_damage`]).
    zones: Zones,
    /// The window cracks the car has taken ([`Playback::on_damage`]).
    cracks: Cracks,
    /// The lamps' flares, each with its stage mesh ([`Playback::add_flares`]).
    flares: Vec<FlareSlot>,
    /// The gear is reverse. The live mode sets it from the HUD; a recording has no gear, so it stays off there.
    pub reverse: bool,
    /// The game's lamps by the frame they changed at ([`Playback::set_lamps`]); empty for a recording.
    lamps: std::collections::BTreeMap<usize, Lamps>,
    /// The headlight key's setting (ours).
    pub headlights: lights::Headlights,
    /// Per detachable part ([`detach::PARTS`]), its placement once the physics has detached it.
    detached: [Option<Detach>; detach::PARTS.len()],
    /// The interior parts centred within the distance of the point turn by the matrix, in model space (the steering
    /// wheel, `characters::incar::Crew::wheel_turn`).
    pub turn: Option<(Vec3, f32, Mat4)>,
    /// The steering wheel's node in model space: the interior parts centred within
    /// `characters::incar::WHEEL_MATCH_METRES` of it are not drawn (ours, `viewer --hide-wheel`).
    pub hidden_wheel: Option<Vec3>,
    /// The game's chase camera, moved by [`Playback::follow_game_chase`].
    game_chase: Follow,
    /// The stage's collision mesh for the cameras' probes and the car's shadow ([`Playback::set_probe`]).
    probe: Option<std::sync::Arc<StageProbe>>,
    /// The car's shadow decal's texture ([`crate::car_shadow::TEXTURE`]), when it reads.
    shadow: Option<rbr_formats::tga::Image>,
    /// The cockpit's mirrors' glass ([`Playback::set_mirrors`]).
    mirrors: Vec<crate::mirrors::Glass>,
}

/// A lamp's flare and the stage mesh it is drawn as.
#[derive(Debug, Clone, PartialEq)]
struct FlareSlot {
    stage: usize,
    flare: Flare,
}

impl Playback {
    /// Matches the car's model meshes to the recording: the tyre nodes of the car's `.sgc` name the four wheels, and
    /// their drawn meshes are the ones centred on the tyre nodes. `model` must be the car's meshes at the identity
    /// placement with what damage does to them (`Car::meshes` and `Car::damage`), and each must be in `stage` (the viewer adds
    /// them with `Car::add_to_stage`).
    pub fn load(
        data: &Data,
        car: &str,
        stage: &Stage,
        model: &[Mesh],
        damage: &[MeshDamage],
        drive: Drive,
    ) -> Result<Playback, String> {
        let (drawn, rest) = tyre_nodes(data, car)?;
        let mut parts: Vec<Part> = Vec::new();
        let mut found = [false; WHEELS];
        for (mesh, effect) in model.iter().zip(damage) {
            let stage_index = stage.meshes.iter().position(|m| m == mesh).ok_or("a car mesh is not in the stage")?;
            let wheel = effect.wheel.map(|k| (k, drawn[k]));
            if let Some((k, _)) = wheel {
                found[k] = true;
            }
            parts.push(Part::new(stage_index, mesh, effect, wheel, false));
        }
        if found.contains(&false) {
            return Err(format!("the car {car} has no drawn mesh for every tyre"));
        }
        let offset: Vec3 =
            (0..WHEELS).map(|k| drive.mean_anchor(k) - model_to_body().transform_point3(rest[k])).sum::<Vec3>()
                / WHEELS as f32;
        let mut playback = Playback {
            drive,
            parts,
            offset,
            clock: 0.0,
            paused: false,
            rate: 1.0,
            interior: false,
            zones: [0.0; damage::ZONES],
            cracks: Cracks::default(),
            flares: Vec::new(),
            reverse: false,
            lamps: Default::default(),
            headlights: lights::Headlights::Main,
            detached: [None; detach::PARTS.len()],
            turn: None,
            hidden_wheel: None,
            game_chase: Follow::new(crate::game_camera::Settings::default()),
            probe: None,
            shadow: data
                .resolve(crate::car_shadow::TEXTURE)
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| rbr_formats::tga::parse(&bytes).ok()),
            mirrors: Vec::new(),
        };
        playback.cracks = Cracks::new(playback.windows());
        Ok(playback)
    }

    /// The interior set's mirrors (`crate::mirrors::find`, marked in its meshes), drawn in the cockpit view.
    pub fn set_mirrors(&mut self, mirrors: Vec<crate::mirrors::Glass>) {
        self.mirrors = mirrors;
    }

    /// The stage indices of the car's meshes, outside and inside ([`Renderer::set_car`]).
    pub fn car_meshes(&self) -> Vec<usize> {
        self.parts.iter().map(|part| part.stage).collect()
    }

    /// Adds the car's interior set (`car::load_interior` at the identity placement, each mesh in `stage`, with its
    /// deformation) as body parts shown when [`Playback::interior`] is set.
    pub fn add_interior(&mut self, stage: &Stage, interior: &[Mesh], damage: &[MeshDamage]) -> Result<(), String> {
        for (mesh, effect) in interior.iter().zip(damage) {
            let stage_index =
                stage.meshes.iter().position(|m| m == mesh).ok_or("an interior mesh is not in the stage")?;
            self.parts.push(Part::new(stage_index, mesh, effect, None, true));
        }
        self.cracks = Cracks::new(self.windows());
        Ok(())
    }

    /// Adds the car's flares (`car::Car::flares`, whose meshes are in `stage`), drawn as the lamps switch.
    pub fn add_flares(&mut self, stage: &Stage, flares: &[Flare]) -> Result<(), String> {
        for flare in flares {
            let stage_index =
                stage.meshes.iter().position(|m| *m == flare.mesh).ok_or("a flare is not in the stage")?;
            self.flares.push(FlareSlot { stage: stage_index, flare: flare.clone() });
        }
        Ok(())
    }

    /// The lamps' inputs in frame `index`: its brake pedal value, the gear, and the game's lamps from the live process.
    fn inputs(&self, index: usize) -> Inputs {
        let game = self.lamps.range(..=index).next_back().map(|(_, lamps)| *lamps);
        Inputs { brake: self.drive.samples[index].brake_lamp, reverse: self.reverse, game, headlights: self.headlights }
    }

    /// The game's lamps of a recording, by step (a replay's lights events, [`crate::particles::Particles::recorded_lamps`]):
    /// each from the first frame with its step on.
    pub fn set_recorded_lamps(&mut self, lamps: &std::collections::BTreeMap<u32, Lamps>) {
        for (index, sample) in self.drive.samples.iter().enumerate() {
            if let Some(&l) = lamps.range(..=sample.step).next_back().map(|(_, l)| l) {
                if self.lamps.range(..=index).next_back().map(|(_, l)| *l) != Some(l) {
                    self.lamps.insert(index, l);
                }
            }
        }
    }

    /// The game's lamps from the current frame on (the live process's lights event), until the next change.
    pub fn set_lamps(&mut self, lamps: Lamps) {
        let index = self.index();
        self.lamps.split_off(&(index + 1));
        if self.lamps.range(..=index).next_back().map(|(_, l)| *l) != Some(lamps) {
            self.lamps.insert(index, lamps);
        }
    }

    /// The parts of the interior set (or the exterior one) whose rest box centre is within 5 mm of `centre`, model
    /// space (the wipers, `crate::glass`).
    pub fn parts_at(&self, centre: Vec3, interior: bool) -> Vec<usize> {
        let box_centre = |part: &Part| {
            let (lo, hi) = part.rest.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
                let p = Vec3::from(v.position);
                (lo.min(p), hi.max(p))
            });
            (lo + hi) * 0.5
        };
        (0..self.parts.len())
            .filter(|&i| self.parts[i].interior == interior && box_centre(&self.parts[i]).distance(centre) < 0.005)
            .collect()
    }

    /// Moves part `part` ([`Playback::parts_at`]) by `motion`, model space, before its pose.
    pub fn set_motion(&mut self, part: usize, motion: Mat4) {
        if let Some(part) = self.parts.get_mut(part) {
            part.motion = motion;
        }
    }

    /// The windows that have crack overlays.
    fn windows(&self) -> [bool; damage::WINDOWS.len()] {
        let mut present = [false; damage::WINDOWS.len()];
        for crack in self.parts.iter().filter_map(|p| p.effect.crack) {
            present[crack.window] = true;
        }
        present
    }

    /// A `CSigCarDamage` event: its code and functor index crack a window ([`Cracks::on_damage`]).
    pub fn on_damage(&mut self, code: u32, index: u32) {
        self.cracks.on_damage(code, index);
    }

    /// Places the detached parts at their bodies' poses in the live step's record (`DET1`, [`detach`]). A part is
    /// detached from the first record that lists it, its mesh at the car's placement there, and then follows its
    /// body. A part the record leaves out (its body asleep) keeps its last placement.
    pub fn set_detached(&mut self, poses: &[PartPose]) {
        let attached = self.body();
        for pose in poses {
            let Some(slot) = self.detached.get_mut(pose.part as usize) else {
                continue;
            };
            let orientation = Quat::from_array(pose.orientation);
            let body = physics_to_stage() * Mat4::from_rotation_translation(orientation, Vec3::from(pose.position));
            slot.get_or_insert_with(|| Detach::start(body, attached)).follow(body);
        }
    }

    /// Dents the damageable parts for the damage handler's `zones`. As `UpdateDeformation`, nothing is redone while
    /// the zones are unchanged.
    pub fn set_damage(&mut self, zones: &Zones) {
        if *zones == self.zones {
            return;
        }
        self.zones = *zones;
        for part in &mut self.parts {
            if let Some(dents) = &part.effect.dents {
                part.damage = dents.under(zones).into_iter().map(|(o, s)| (o, damage::scrape_byte(s))).collect();
                part.in_car.set(false);
            }
        }
    }

    /// The frame showing at the clock: the first whose end is after it.
    pub fn index(&self) -> usize {
        let i = self.drive.ends.partition_point(|&end| end <= self.clock);
        i.min(self.drive.samples.len() - 1)
    }

    /// Every frame of the drive.
    pub fn samples(&self) -> &[Sample] {
        &self.drive.samples
    }

    /// The current frame.
    pub fn sample(&self) -> &Sample {
        &self.drive.samples[self.index()]
    }

    /// Every frame with the drive time at its end, in order (the characters step through them).
    pub fn frames(&self) -> impl Iterator<Item = (f64, &Sample)> + '_ {
        self.drive.frames()
    }

    /// What a step of the frames' counters is (the ghost's race time, the wipers' clock).
    pub fn step_clock(&self) -> StepClock {
        self.drive.clock
    }

    /// Drive time at the end of the current frame, seconds from the first frame's start.
    pub fn time(&self) -> f64 {
        self.drive.ends[self.index()]
    }

    /// The payload periods frame `index` stands for: its game frame's wall time in [`rbr_recording::STEP_SECONDS`].
    pub fn periods(&self, index: usize) -> u32 {
        self.drive.periods[index]
    }

    /// Drive time of the last frame's end, seconds.
    pub fn duration(&self) -> f64 {
        self.drive.ends.last().copied().unwrap_or(0.0)
    }

    /// Moves the clock on by `seconds` of real time, scaled by `rate`, unless paused.
    pub fn advance(&mut self, seconds: f64) {
        if !self.paused {
            self.clock = (self.clock + seconds * self.rate).clamp(0.0, self.duration());
        }
    }

    /// Pauses and moves `frames` frames on (negative: back).
    pub fn step(&mut self, frames: isize) {
        self.paused = true;
        let i = (self.index() as isize + frames).clamp(0, self.drive.samples.len() as isize - 1);
        self.clock = self.start_of(i as usize);
    }

    /// Pauses at the first frame whose step counter is at least `step`.
    pub fn seek_to_step(&mut self, step: u32) {
        self.paused = true;
        let i = self.drive.samples.partition_point(|s| s.step < step);
        self.clock = self.start_of(i.min(self.drive.samples.len() - 1));
    }

    /// Back to the first frame.
    pub fn restart(&mut self) {
        self.clock = 0.0;
    }

    /// Adds frames after the drive's last and shows the last (the live mode).
    pub fn push(&mut self, more: Drive) {
        self.drive.extend(more);
        self.clock = self.duration();
    }

    /// Starts over on `drive` (the live mode's restart) with the windows whole. The body's offset stays the one
    /// [`Playback::load`] found.
    pub fn reset(&mut self, drive: Drive) {
        self.drive = drive;
        self.clock = 0.0;
        self.cracks = Cracks::new(self.windows());
        self.detached = [None; detach::PARTS.len()];
    }

    /// The drive time when frame `i` starts.
    fn start_of(&self, i: usize) -> f64 {
        if i == 0 {
            0.0
        } else {
            self.drive.ends[i - 1]
        }
    }

    /// The body's position in stage space, m.
    pub fn position(&self) -> Vec3 {
        physics_to_stage().transform_point3(Vec3::from(self.sample().car.position))
    }

    /// The chase camera behind the body, looking at its middle, turned around it by `look`.
    pub fn camera(&self, look: ChaseLook) -> FreeFly {
        let position = self.position();
        let forward = physics_to_stage().transform_vector3(Vec3::from(self.sample().car.forward()));
        let target = position + Vec3::Y * CHASE_TARGET_HEIGHT;
        let offset = -forward * CHASE_DISTANCE + Vec3::Y * (CHASE_HEIGHT - CHASE_TARGET_HEIGHT);
        let eye = target + look.apply(offset);
        FreeFly::looking_along(eye, target - eye)
    }

    /// A camera fixed to the body: `mount` placed as the body meshes are, so it follows the pose with no lag and rolls
    /// with the car. `fov` is the horizontal field of view, radians.
    pub fn mounted(&self, mount: &Mount, fov: f32) -> Lens {
        let body = self.body();
        Lens {
            eye: body.transform_point3(mount.eye),
            forward: body.transform_vector3(mount.target - mount.eye).normalize(),
            up: body.transform_vector3(mount.up),
            fov,
            near: mount.near,
        }
    }

    /// The camera of `view`: the chase (the game's, [`crate::game_camera`]) turned about its look point by `look`, at
    /// [`CHASE_FOV_DEGREES`], or one of `mounts` at the horizontal field of view `fov`, radians, shaken in the view
    /// that takes the seat shake.
    pub fn lens(&self, mounts: &Mounts, view: CarView, look: ChaseLook, fov: f32) -> Lens {
        match mounts.of(view) {
            Some(mount) if view.shakes() => self.mounted(&mount.shaken(self.sample().shake[CAMERA_SHAKE]), fov),
            Some(mount) if view == CarView::Bonnet => {
                let mut lens = self.mounted(&mount, fov);
                if let Some(probe) = &self.probe {
                    let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
                    lens.eye = swap(crate::game_camera::bonnet_lift(probe, swap(lens.eye), mount.near));
                }
                lens
            }
            Some(mount) => self.mounted(&mount, fov),
            None => match self.game_chase.lens() {
                Some(lens) => {
                    let target = lens.eye + lens.forward;
                    let eye = target + look.apply(lens.eye - target);
                    Lens { eye, forward: (target - eye).normalize(), ..lens }
                }
                None => self.camera(look).lens(CHASE_FOV_DEGREES.to_radians()),
            },
        }
    }

    /// The chase's settings (`crate::game_camera::Settings`: ours, nudged per car).
    pub fn set_chase_settings(&mut self, settings: crate::game_camera::Settings) {
        self.game_chase.set_settings(settings);
    }

    /// Moves the game's chase camera (`crate::game_camera`) to the frame shown; `swing` is the start orbit's angle
    /// while it runs, and `looking_back` the look-back key. The viewer calls it once per frame drawn.
    pub fn follow_game_chase(&mut self, swing: Option<f32>, looking_back: bool) {
        let index = self.index();
        let samples = &self.drive.samples;
        let probe = self.probe.as_deref();
        self.game_chase.follow(&self.drive.ends, |i| samples[i].car, index, swing, looking_back, probe);
    }

    /// Gives the cameras the stage's collision mesh: the game chase's probes and the bonnet view's ground probe.
    pub fn set_probe(&mut self, probe: Option<std::sync::Arc<StageProbe>>) {
        self.probe = probe;
    }

    /// Model space to the physics frame for the body meshes at `sample`.
    pub fn model_to_physics(&self, sample: &Sample) -> Mat4 {
        physics_pose(&sample.car) * Mat4::from_translation(self.offset) * model_to_body()
    }

    /// Model space to the physics body's frame (z up), for things placed on the car model.
    pub fn model_to_car(&self) -> Mat4 {
        Mat4::from_translation(self.offset) * model_to_body()
    }

    /// Model space to stage space for the body meshes at the current frame.
    pub fn body(&self) -> Mat4 {
        self.model_to_stage(&self.sample().car)
    }

    /// Model space to stage space for the body meshes at `car`'s pose.
    pub fn model_to_stage(&self, car: &CarPose) -> Mat4 {
        physics_to_stage() * physics_pose(car) * Mat4::from_translation(self.offset) * model_to_body()
    }

    /// Moves every car mesh to the current frame's pose in `renderer`, and hides the set not shown
    /// ([`Playback::interior`]), the crack overlays not projected and the lamp parts not in their lamp's state. The
    /// hidden set moves too, so the car's bounds stay the car's whichever set is drawn. The flares are drawn as seen
    /// through `lens` (the camera of the frame).
    pub fn apply(&self, renderer: &mut Renderer, lens: &Lens) {
        let sample = self.sample();
        let body = self.body();
        let index = self.index();
        let inputs = self.inputs(index);
        for part in &self.parts {
            let cracked = part.effect.crack.is_none_or(|crack| self.cracks.shows(crack));
            let lamp = part.effect.lamp.is_none_or(|switch| switch.shown(inputs));
            let near = |at: Vec3, within| part.interior && part.centre.is_some_and(|c| c.distance(at) < within);
            let hidden = self.hidden_wheel.is_some_and(|at| near(at, crate::characters::incar::WHEEL_MATCH_METRES));
            renderer.set_visible(part.stage, part.interior == self.interior && cracked && lamp && !hidden);
            renderer.set_outside_visible(part.stage, !part.interior && cracked && lamp);
            let turned = self.turn.filter(|&(at, within, _)| near(at, within));
            let detached = part.detach.and_then(|k| self.detached[k]);
            // The parts that move only with the body are given once, in its model space, and placed on the GPU: a car
            // has some 200,000 vertices, which took milliseconds a frame to place and give.
            if part.wheel.is_none() && detached.is_none() && turned.is_none() && part.motion == Mat4::IDENTITY {
                if !part.in_car.replace(true) {
                    let vertices = part.placed(Mat4::IDENTITY);
                    renderer.set_car_vertices(part.stage, &vertices, mesh_bounds(&vertices));
                }
                continue;
            }
            part.in_car.set(false);
            let matrix = match part.wheel {
                None => match detached {
                    Some(detach) => detach.placement(),
                    None => turned.map_or(body, |(_, _, turn)| body * turn) * part.motion,
                },
                Some((k, centre)) => {
                    let wheel = sample.wheels[k];
                    physics_to_stage()
                        * physics_pose(&sample.car)
                        * Mat4::from_translation(Vec3::from(wheel.anchor))
                        * model_to_body()
                        * Mat4::from_axis_angle(WHEEL_ROLL_AXIS, wheel.roll)
                        * Mat4::from_translation(-centre)
                }
            };
            let vertices = part.placed(matrix);
            renderer.set_vertices(part.stage, &vertices, mesh_bounds(&vertices));
        }
        let mut lamps = Vec::new();
        for slot in &self.flares {
            let light = &slot.flare.light;
            let on_at = |i| light.fixed.unwrap_or_else(|| lights::lit(light.kind, light.lamp, self.inputs(i)));
            let on = on_at(index);
            // A second past the longest fade, so that a lamp steady for a while reads as settled (`since_switch`).
            let horizon = f64::from(light.fade_in.max(light.fade_out)) + 1.0;
            let since = lights::since_switch(self.time(), index, |i| self.start_of(i), on_at, horizon);
            let sprite = lights::fade(light, on, since).and_then(|fade| {
                let position = body.transform_point3(slot.flare.position);
                let normal = body.transform_vector3(slot.flare.normal);
                let (forward, up) = (body.transform_vector3(Vec3::NEG_Z), body.transform_vector3(Vec3::Y));
                let outward = normal.normalize_or_zero();
                let main = self.headlights == lights::Headlights::Main;
                lamps.push(lights::lamp(light, position, outward, forward.normalize(), up.normalize(), fade, main));
                lights::sprite(light, position, normal, lens, fade)
            });
            match sprite {
                Some(vertices) => {
                    renderer.set_vertices(slot.stage, &vertices, None);
                    renderer.set_visible(slot.stage, true);
                }
                None => renderer.set_visible(slot.stage, false),
            }
        }
        renderer.set_lamps(&lamps);
        renderer.set_car_body(body);
        let mirrors: Vec<_> = self.mirrors.iter().filter(|_| self.interior).map(|glass| glass.placed(body)).collect();
        renderer.set_mirrors(&mirrors);
        if let (Some(texture), Some(probe)) = (&self.shadow, &self.probe) {
            renderer.set_car_shadow(texture, probe, physics_pose(&sample.car));
        }
    }
}

#[cfg(test)]
mod tests;

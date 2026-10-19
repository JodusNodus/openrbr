//! The scripted events on the stage's actors (`notes/gameplay.md` "Scripted events"): the rescue helicopter and its cameras and
//! dust, the car-flip spectators and camera, and the bird strike's windscreen point.

use super::load::Role;
use super::scripted::{self, Burst, Event, Flip, HostCall, Rescue};
use super::{Characters, OnLine, Thrown};
use glam::{Mat4, Vec3};

impl Characters {
    /// Calls the rescue helicopter on the next frame with a car (the game flow's retire codes 1 and 2, `M5662c0`).
    pub fn call_rescue(&mut self) {
        self.rescue_called = true;
    }

    /// Calls the rescue helicopter when the drive reaches step counter `step` (ours, for checks and captures).
    pub fn set_rescue_step(&mut self, step: Option<u32>) {
        self.rescue_step = step;
    }

    /// The rescue helicopter while it runs.
    pub fn rescue(&self) -> Option<Rescue> {
        self.rescue.filter(|r| r.active)
    }

    /// The rescue helicopter's body while it runs: its model's `default` node in stage space (the node
    /// `CPC_RescueHelicopter::Vf20` reads), for the sound and the captures.
    pub fn rescue_position(&self) -> Option<Vec3> {
        self.rescue()?;
        let actor = self.actors.iter().find(|a| a.spawn.role == Role::Rescue)?;
        let node = self.models[actor.spawn.model].skeleton.find("default")?;
        let world = self.pose(actor);
        Some((actor.placement * world[node]).w_axis.truncate())
    }

    /// The scripted events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The mode-8 camera (`M5010`, 0x485010) while the helicopter runs: on the driveline 30 m short of its landing
    /// point, 1.8 m up, looking at its flight point (`UpdateHeli`).
    pub fn helicam(&self, fov: f32) -> Option<crate::camera::Lens> {
        let rescue = self.rescue()?;
        let eye = self.stage_point(rescue.camera())? + Vec3::Y * scripted::CAMERA_LIFT;
        let (along, height) = rescue.flight();
        let target = self.stage_point(along)? + Vec3::Y * height;
        Some(crate::camera::Lens {
            eye,
            forward: (target - eye).normalize_or(Vec3::Z),
            up: Vec3::Y,
            fov,
            near: crate::coords::NEAR,
        })
    }

    /// The driveline's point at stage distance `distance`, in stage axes.
    fn stage_point(&self, distance: f32) -> Option<Vec3> {
        let p = scripted::point_at(&self.driveline, &self.values, distance)?;
        Some(Vec3::new(p.x, p.z, p.y))
    }

    /// The flip spectators and the rescue helicopter for one frame (the HUD tick and the helicopter's `Tic` follow
    /// the frame's steps).
    pub(super) fn scripted(&mut self, car: OnLine, dt: f32) {
        let emitted = match (self.assist, car.assist) {
            (Some(before), Some(now)) => now.emitted(before),
            _ => None,
        };
        if car.assist.is_some() {
            self.assist = car.assist;
        }
        match self.flip_timer.tick(dt, emitted) {
            Some(Flip::Start(code)) => self.flip_start(car, code),
            Some(Flip::End) => {
                // The camera's handler goes back to the mode before (it does nothing while the helicopter runs).
                if self.rescue().is_none() {
                    self.flip_camera = None;
                }
                // flag 1: every active spectator is released.
                for actor in &mut self.actors {
                    if matches!(actor.spawn.role, Role::Flip(_)) {
                        actor.active = false;
                    }
                }
                self.events.push(Event::FlipEnd);
            }
            None => {}
        }
        if std::mem::take(&mut self.rescue_called) {
            self.rescue_start(car);
        }
        if let Some(rescue) = &mut self.rescue {
            if rescue.tic(dt) {
                self.events.push(Event::RetireBystand);
            }
        }
        self.burst(dt);
    }

    /// `Burst` while the helicopter runs: dust from the ground under its body. Ours: the ground there is the
    /// driveline's height at its flight point, not the probe's hit, and the throws stop when its run ends.
    fn burst(&mut self, dt: f32) {
        let (Some(rescue), Some(body)) = (self.rescue(), self.rescue_position()) else { return };
        let Some(ground) = self.stage_point(rescue.flight().0) else { return };
        let count = self.burst.tick(dt, Some(body.y - ground.y));
        let at = Vec3::new(body.x, body.z, ground.y + scripted::BURST_LIFT);
        for _ in 0..count {
            let velocity = scripted::burst_velocity(&mut self.crt);
            self.smoke.push((self.frames, at, velocity));
        }
    }

    /// The helicopter's dust thrown since the last call: the map's smoke and each particle's playback frame, point
    /// and velocity, physics axes (for [`crate::particles::Particles::rescue_smoke`]).
    pub fn take_smoke(&mut self) -> Option<(scripted::Smoke, Vec<Thrown>)> {
        let smoke = std::mem::take(&mut self.smoke);
        Some((self.smoke_kind?, smoke)).filter(|(_, s)| !s.is_empty())
    }

    /// Gives the flip camera the stage's collision mesh to probe; without it the camera stands where its offset puts it.
    pub fn set_probe(&mut self, probe: Option<std::sync::Arc<crate::stage_probe::StageProbe>>) {
        self.probe = probe;
    }

    /// The car-flip camera (mode 13, `FUN_00487980`) while the spectators run, with field of view `fov` (the mode
    /// sets the game's 75).
    pub fn flipcam(&self, fov: f32) -> Option<crate::camera::Lens> {
        let (eye, look) = self.flip_camera?;
        Some(crate::camera::Lens {
            eye,
            forward: (look - eye).normalize_or(Vec3::Z),
            up: Vec3::Y,
            fov,
            near: crate::coords::NEAR,
        })
    }

    /// The flip handler's start (0x43d550): the group's spectators at the car's driveline point, turned to the
    /// driveline's heading, their clip from its start.
    fn flip_start(&mut self, car: OnLine, code: u32) {
        let (group, turn) = scripted::flip_group(code);
        let Some(point) = self.track_point(car.track) else { return };
        let heading = scripted::flip_heading([car.direction.x, car.direction.z, car.direction.y], turn);
        // The camera's handler of the same signal; nothing while the helicopter runs.
        if self.rescue().is_none() {
            let physics = |v: Vec3| Vec3::new(v.x, v.z, v.y);
            let probe = self.probe.as_deref();
            let segment = |a, b| probe.and_then(|p| p.segment(a, b)).map(|hit| hit.point);
            self.flip_camera = scripted::flip_camera(physics(point), physics(car.direction), code, segment)
                .map(|(eye, look)| (physics(eye), physics(look)));
        }
        for actor in &mut self.actors {
            let Role::Flip(n) = actor.spawn.role else { continue };
            if !group.contains(&n) {
                continue;
            }
            actor.placement = Mat4::from_translation(point)
                * Mat4::from_rotation_y(heading)
                * Mat4::from_scale(Vec3::splat(actor.spawn.scale));
            actor.clip = Some((0, self.time));
            actor.state = 0.0;
            actor.active = true;
        }
        self.events.push(Event::FlipStart(code));
    }

    /// The driveline's point at the car's driveline position `track` (`Split17C`'s `BlendB`), in stage axes.
    fn track_point(&self, track: f32) -> Option<Vec3> {
        let i = (track.max(0.0) as usize).min(self.driveline.len().checked_sub(1)?);
        let a = &self.driveline[i].vectors[0];
        let b = &self.driveline.get(i + 1).unwrap_or(&self.driveline[i]).vectors[0];
        let t = track - i as f32;
        let p = Vec3::new(a.x, a.y, a.z).lerp(Vec3::new(b.x, b.y, b.z), t.clamp(0.0, 1.0));
        Some(Vec3::new(p.x, p.z, p.y))
    }

    /// `CPC_RescueHelicopter::Vf18` (0x452290): `Activate`, then the helicopter at the landing point
    /// (`FillMatrix`) with a random heading (`rand() * 2 pi / 32767`), playing its fly-in.
    fn rescue_start(&mut self, car: OnLine) {
        let length = self.values.last().copied().unwrap_or(0.0);
        let rescue = Rescue::call(car.distance, length, self.crt.draw() as u32);
        let Some(point) = self.stage_point(rescue.anchor) else { return };
        let heading = self.crt.draw() as f32 * 0.000_191_753_45;
        for actor in &mut self.actors {
            if actor.spawn.role != Role::Rescue {
                continue;
            }
            actor.placement = Mat4::from_translation(point)
                * Mat4::from_rotation_y(heading)
                * Mat4::from_scale(Vec3::splat(actor.spawn.scale));
            actor.clip = Some((0, self.time));
            actor.state = 0.0;
            actor.active = true;
        }
        self.rescue = Some(rescue);
        self.burst = Burst::default();
        // `Activate` sets mode 8 over the flip camera.
        self.flip_camera = None;
        self.events.push(Event::RescueCalled(point));
    }

    /// `E02AnimUpdate`'s pair loop for a character placed at the car. `c` (the bird strike's only flag) moves it to
    /// `g_e02167ea40`, which is the 2D effect system's `+0x190` (the system is the static at 0x167e8b0): the
    /// windscreen point in the world, which `SetupDx` (0x5f6820) puts through the car's matrix every frame. The
    /// character keeps its turn. Ours: `a` and `b` (no script sets them) put it at the car's position, turned to the
    /// driveline's heading; without a windscreen, `c` uses the car's position.
    pub(super) fn place_at_car(&mut self, i: usize) {
        let Some(car) = self.car else { return };
        let Some(HostCall::PlaceAtCar { translate, rotate, strike }) = self.actors[i].at_car else { return };
        let strike_point = self.strike_point();
        let heading = scripted::flip_heading([car.direction.x, car.direction.z, car.direction.y], false);
        let actor = &mut self.actors[i];
        let (scale, turn, mut at) = actor.placement.to_scale_rotation_translation();
        let mut turn = turn;
        if translate {
            at = car.position;
        }
        if rotate {
            turn = glam::Quat::from_rotation_y(heading);
        }
        if strike {
            at = strike_point.unwrap_or(car.position);
        }
        actor.placement = Mat4::from_scale_rotation_translation(scale, turn, at);
    }

    /// The car's windscreen chart and the model-to-body matrix its points go through ([`crate::glass::CarGlass`],
    /// [`crate::playback::Playback::model_to_car`]), for the bird strike's point.
    pub fn set_windscreen(&mut self, windscreen: crate::glass::Windscreen, model_to_car: Mat4) {
        self.windscreen = Some((windscreen, model_to_car));
    }

    pub(super) fn windscreen_ratio(&self) -> f32 {
        self.windscreen.as_ref().map_or(1.0, |(w, _)| w.ratio())
    }

    /// The windscreen point in stage space: `RandPair`'s point read as fractions of the chart, across from its low
    /// side and up (ours: `SetupBig` builds its frame from four corner points not traced to the car's glass).
    pub(super) fn strike_point(&self) -> Option<Vec3> {
        let car = self.car?;
        let (windscreen, model_to_car) = self.windscreen.as_ref()?;
        let (x, y) = self.strike_at;
        let model = windscreen.at_fraction(glam::Vec2::new(x / windscreen.ratio().max(1e-6), y));
        Some((car.body * *model_to_car).transform_point3(model))
    }
}

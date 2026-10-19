//! Stage characters: spectators, marshals, photographers and animals placed by the stage's `.lbs` `(2, 14)` items,
//! animated by their `.sga` clips and driven by their Lua scripts (`notes/stage-characters.md`).
//!
//! - [`load`] spawns them: the `(2, 15)` random groups, each item's load script (`LuaRandom` picks the model and
//!   clips), the models and clips, and the meshes added to the stage.
//! - [`Characters::step`] runs the game's per-frame logic (`E02AnimUpdate`, 0x443a10): a visible character whose clip
//!   has ended calls its run script's `Update(state)`, which names the next clip; clips never loop on their own. The
//!   car triggers characters through the stage's `Driveline` animation channels (`E12_AnimSet::TrigLoop`,
//!   0x477ff0): once `distance - lead * v < x`, the character's `Trigger(state, isStart)` runs
//!   (`E02TrigLoader::Fire`, 0x443d00), and its next `Update` plays the triggered clip (the run off, the photo, the
//!   deer bolting, the bird flying away). The `Driveline` plain trigger keys do the same as the car passes them
//!   (`E12_AnimSet::Load3b0`, 0x4783b0); the bird strikes have only those.
//! - [`Characters::apply`] poses and skins the near ones into their stage meshes ([`anim`]).
//!
//! - Visibility ([`cells`]): a character shows, and its scripts run, while the stage's visibility cell holding its
//!   spawn point is in the camera's view (the last frame's, as the game's flags are the last render's).
//! - Triggers whose object id is not a loaded character do nothing, as `Fire` finds no descriptor (14 of the 31
//!   animation channels on `track-54_*` name ids no `.lbs` item has).
//!
//! Ours, not traced: the seed and the stream position of `LuaRandom` and `rand()` ([`random`]; the game seeds both
//! with `time(NULL)`); the lighting of the faces; `isStart` is passed as 1. The scripted events' characters (car-flip
//! spectators, helicopter) are shown whenever their event places them.
//! - The scripted events ([`scripted`], `notes/scripted-events.md`) run on the same actors: the car-flip spectators
//!   and the rescue helicopter are hidden until their event places them, and the bird strike's run script moves its
//!   bird to the car. Their events ([`scripted::Event`]) are for the sound and particle systems
//!   ([`Characters::take_events`]).

pub mod anim;
pub mod cells;
pub mod crew;
mod events;
pub mod incar;
pub mod load;
pub mod random;
pub mod script;
pub mod scripted;

use crate::playback::Sample;
use crate::renderer::{mesh_bounds, Renderer};
use crate::stage::{Mesh, Stage, Vertex};
use glam::{Mat4, Vec3};
use load::Role;
use load::{Loaded, Report};
use rbr_formats::data::Data;
use rbr_formats::markers::{AnimationTrigger, NamedTrigger, PlainTrigger};
use rbr_formats::trk::DriveLineRecord;
use script::{Host, Lua, Value};
use scripted::{Assist, Burst, Event, FlipTimer, HostCall, Rescue};
use std::collections::HashMap;

/// `isStart` of a `CSigTrigger` from `TrigLoop` (ours: the scripts do not read it).
const IS_START: f64 = 1.0;

/// One character at run time.
#[derive(Debug, Clone)]
struct Actor {
    spawn: load::Spawn,
    placement: Mat4,
    /// The Lua state number (`desc + 0x18`).
    state: f64,
    /// The playing clip: its index in the spawn's clips and the time it started, or `None` before the first
    /// `Update`.
    clip: Option<(usize, f64)>,
    /// Stage mesh indices, one per model mesh.
    slots: Vec<usize>,
    /// The visibility cell holding the spawn point (`desc + 0x78`).
    cell: Option<usize>,
    shown: bool,
    /// The descriptor's active byte (+0x75) for the scripted ones: hidden until their event places them.
    active: bool,
    /// `PlaceCharacterAtCar(a, b, c)` with one set: moved to the car every frame by those flags.
    at_car: Option<HostCall>,
}

/// The car on the driveline for one frame: its driveline distance `x` and its speed along the driveline `v`, and for
/// the scripted events its position on the driveline (`CCar+0x17c`), its stage position, the driveline's direction
/// there (stage axes) and the outside assistance's car words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnLine {
    pub distance: f32,
    pub speed: f32,
    pub track: f32,
    pub position: Vec3,
    pub direction: Vec3,
    pub assist: Option<Assist>,
    /// The physics body's frame to stage space.
    pub body: Mat4,
}

/// One character's state ([`Characters::states`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct State<'a> {
    /// The `(2, 14)` object id.
    pub id: u32,
    pub position: Vec3,
    /// The load script.
    pub load: &'a str,
    /// The clip playing, by `AddAnimationToCharacter` order.
    pub clip: Option<usize>,
    /// The run script's state.
    pub state: f64,
}

/// The stage's characters.
pub struct Characters {
    models: Vec<load::Model>,
    clips: Vec<anim::Clip>,
    actors: Vec<Actor>,
    by_id: HashMap<u32, usize>,
    runs: HashMap<String, Lua>,
    triggers: Vec<(AnimationTrigger, bool)>,
    /// The plain trigger keys by distance, and how many the car has passed (`Load3b0`).
    plain: Vec<PlainTrigger>,
    plain_passed: usize,
    /// The `.dls` named triggers ([`Characters::fire_named`]) and whether the countdown fired its one
    /// ([`Characters::countdown`]; the HUD's `+0x8`).
    named: Vec<NamedTrigger>,
    start_fired: bool,
    driveline: Vec<DriveLineRecord>,
    values: Vec<f32>,
    time: f64,
    /// The playback frames already stepped ([`Characters::follow`]).
    frames: usize,
    /// The step the rescue helicopter is called at ([`Characters::set_rescue_step`]).
    rescue_step: Option<u32>,
    /// The stage's visibility cells and the clip planes of the last [`Characters::apply`] (none before the first:
    /// every cell shown).
    cells: Vec<cells::Cell>,
    planes: Option<[glam::Vec4; 6]>,
    /// The scripted events' state.
    flip_timer: FlipTimer,
    assist: Option<Assist>,
    rescue: Option<Rescue>,
    rescue_called: bool,
    /// `rand()` (the loader's, continued).
    crt: random::CrtRand,
    /// The car-flip camera (mode 13) while the spectators run: its eye and look point, stage axes.
    flip_camera: Option<(Vec3, Vec3)>,
    /// The stage's collision mesh, for the flip camera's probes ([`Characters::set_probe`]).
    probe: Option<std::sync::Arc<crate::stage_probe::StageProbe>>,
    /// The helicopter's dust: its emitter on this map, its timer and the particles thrown since the last
    /// [`Characters::take_smoke`] (physics axes: where and how fast).
    smoke_kind: Option<scripted::Smoke>,
    burst: Burst,
    smoke: Vec<Thrown>,
    /// The car's windscreen chart and its model-to-body matrix ([`Characters::set_windscreen`]), the 2D effect
    /// system's LCG and its windscreen point (`RandPair`, chart fractions).
    windscreen: Option<(crate::glass::Windscreen, Mat4)>,
    strike_lcg: random::Lcg,
    strike_at: (f32, f32),
    events: Vec<Event>,
    car: Option<OnLine>,
    pub report: Report,
}

/// A particle of the helicopter's dust: the playback frame it is thrown on, its point and velocity (physics axes).
pub type Thrown = (usize, Vec3, Vec3);

/// Host calls of the run scripts: the bird strike's ([`HostCall`]); the stage characters' run scripts make none.
#[derive(Default)]
struct RunHost(Vec<HostCall>);

impl Host for RunHost {
    fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, String> {
        let flag = |i: usize| args.get(i).and_then(Value::number).is_some_and(|v| v != 0.0);
        match function {
            "app:PlaceCharacterAtCar" => {
                self.0.push(HostCall::PlaceAtCar { translate: flag(0), rotate: flag(1), strike: flag(2) })
            }
            "app:Birdstrike" => self.0.push(HostCall::Birdstrike),
            _ => return Err(format!("{function} is not modelled")),
        }
        Ok(Vec::new())
    }
}

impl Characters {
    /// Loads stage `name`'s characters and adds their meshes and textures to `stage`. Call [`Characters::locate`]
    /// once every other mesh is in the stage.
    pub fn load(data: &Data, name: &str, stage: &mut Stage) -> Result<Characters, String> {
        Self::load_with(data, name, stage, load::Options::default())
    }

    /// [`Characters::load`] with load options.
    pub fn load_with(data: &Data, name: &str, stage: &mut Stage, options: load::Options) -> Result<Characters, String> {
        // Ours: the 2D effect system's seed (its `+0x44` is not traced).
        let options_seed = options.seed;
        let Loaded { models, clips, spawns, runs, report, crt, cells } = load::load(data, name, stage, options)?;
        let maps = data.install().join("Maps");
        let dls = std::fs::read(rbr_formats::stage_files::file_in(&maps, name, "dls"))
            .map_err(|e| format!("{name}.dls: {e}"))?;
        let dls = rbr_formats::dls::parse(&dls).map_err(|e| format!("{name}.dls: {e}"))?;
        let triggers = rbr_formats::markers::animation_triggers(&dls).unwrap_or_default();
        let plain = rbr_formats::markers::plain_triggers(&dls).unwrap_or_default();
        let named = rbr_formats::markers::named_triggers(&dls);
        let trk = std::fs::read(rbr_formats::stage_files::file_in(&maps, name, "trk"))
            .map_err(|e| format!("{name}.trk: {e}"))?;
        let trk = rbr_formats::trk::parse(&trk).map_err(|e| format!("{name}.trk: {e}"))?;
        let driveline = trk
            .chunks
            .into_iter()
            .find_map(|c| match c.payload {
                rbr_formats::trk::ChunkPayload::DriveLine(d) => Some(d.records),
                _ => None,
            })
            .unwrap_or_default();
        let values = driveline.iter().map(|r| r.value).collect();
        let mut lua_runs = HashMap::new();
        for (path, chunk) in &runs {
            let mut lua = Lua::new();
            // `startup.lua`'s `true` and `false` (`run_birdstrike.lua` passes them to `PlaceCharacterAtCar`).
            lua.set_global("true", Value::Number(1.0));
            lua.set_global("false", Value::Number(0.0));
            if lua.run(chunk, &mut RunHost::default()).is_ok() {
                lua_runs.insert(path.clone(), lua);
            }
        }
        let mut actors = Vec::new();
        let mut by_id = HashMap::new();
        for spawn in spawns {
            let placement = anim::placement(spawn.position, spawn.degrees, spawn.scale);
            if spawn.role == Role::Stage {
                by_id.insert(spawn.id, actors.len());
            }
            let active = spawn.role == Role::Stage;
            let cell = cells::cell_of(&cells, spawn.position);
            actors.push(Actor {
                spawn,
                placement,
                state: 0.0,
                clip: None,
                slots: Vec::new(),
                cell,
                shown: true,
                active,
                at_car: None,
            });
        }
        let characters = Characters {
            models,
            clips,
            actors,
            by_id,
            runs: lua_runs,
            triggers: triggers.into_iter().map(|t| (t, false)).collect(),
            plain,
            plain_passed: 0,
            named,
            start_fired: false,
            driveline,
            values,
            time: 0.0,
            frames: 0,
            rescue_step: None,
            cells,
            planes: None,
            flip_timer: FlipTimer::default(),
            assist: None,
            rescue: None,
            rescue_called: false,
            crt,
            flip_camera: None,
            probe: None,
            smoke_kind: crate::stage::map_number(name).and_then(scripted::smoke),
            burst: Burst::default(),
            smoke: Vec::new(),
            windscreen: None,
            strike_lcg: random::Lcg::new(options_seed),
            strike_at: (0.5, 0.5),
            events: Vec::new(),
            car: None,
            report,
        };
        let meshes: Vec<Mesh> = (0..characters.actors.len()).flat_map(|i| characters.meshes(i)).collect();
        stage.meshes.extend(meshes);
        stage.meshes.sort_by_key(|mesh| mesh.layer);
        Ok(characters)
    }

    /// Finds each character's meshes in `stage` (adding meshes sorts it), by their first-frame vertices.
    pub fn locate(&mut self, stage: &Stage) {
        let mut taken = vec![false; stage.meshes.len()];
        for i in 0..self.actors.len() {
            let mut slots = Vec::new();
            for mesh in self.meshes(i) {
                let found = stage.meshes.iter().enumerate().position(|(k, m)| !taken[k] && *m == mesh);
                if let Some(k) = found {
                    taken[k] = true;
                    slots.push(k);
                }
            }
            self.actors[i].slots = slots;
        }
    }

    pub fn count(&self) -> usize {
        self.actors.len()
    }

    /// Character `i`'s meshes posed now, in stage space.
    fn meshes(&self, i: usize) -> Vec<Mesh> {
        let actor = &self.actors[i];
        let model = &self.models[actor.spawn.model];
        let world = self.pose(actor);
        model
            .meshes
            .iter()
            .map(|mesh| {
                let mut placed = mesh.template.clone();
                skin_into(&mut placed.vertices, mesh, &world, actor.placement);
                placed.bounds = mesh_bounds(&placed.vertices);
                placed
            })
            .collect()
    }

    /// The node matrices of `actor` at the current time: its clip at the clip's time, else its first clip's start.
    fn pose(&self, actor: &Actor) -> Vec<Mat4> {
        let model = &self.models[actor.spawn.model];
        let (clip, t) = match actor.clip {
            Some((k, started)) => {
                let clip = &self.clips[actor.spawn.clips[k]];
                (clip, (self.time - started) as f32 + clip.start)
            }
            None => {
                let clip = &self.clips[actor.spawn.clips[0]];
                (clip, clip.start)
            }
        };
        clip.pose(&model.skeleton, t)
    }

    /// The car's driveline distance and speed along it from a frame: `E12_SampleX` of `CCar+0x17c` and
    /// `E12_Sampler::Dot150` of the velocity (`rbr_gameplay::driveline`).
    pub fn on_line(&self, sample: &Sample) -> Option<OnLine> {
        let distance = rbr_gameplay::driveline::sample_x(&self.values, sample.track)?;
        let speed = rbr_gameplay::driveline::dot150(&self.driveline, sample.track, sample.car.velocity)?;
        let [x, y, z] = rbr_gameplay::driveline::sample_direction(&self.driveline, sample.track)?;
        let [px, py, pz] = sample.car.position;
        Some(OnLine {
            distance,
            speed,
            track: sample.track,
            position: Vec3::new(px, pz, py),
            direction: Vec3::new(x, z, y),
            assist: sample.assist,
            body: crate::playback::car_to_stage(&sample.car),
        })
    }

    /// Starts over: every character back to state 0 with no clip, every trigger armed.
    pub fn reset(&mut self) {
        self.time = 0.0;
        self.frames = 0;
        for actor in &mut self.actors {
            actor.state = 0.0;
            actor.clip = None;
            actor.at_car = None;
            if actor.spawn.role != Role::Stage {
                actor.active = false;
            } else {
                actor.placement = anim::placement(actor.spawn.position, actor.spawn.degrees, actor.spawn.scale);
            }
        }
        for (_, fired) in &mut self.triggers {
            *fired = false;
        }
        self.plain_passed = 0;
        self.start_fired = false;
        self.flip_timer = FlipTimer::default();
        self.assist = None;
        self.rescue = None;
        self.rescue_called = false;
        self.flip_camera = None;
        self.burst = Burst::default();
        self.smoke.clear();
        self.events.clear();
        self.car = None;
    }

    /// Steps the characters through the playback frames up to the one showing, each at its end time with the car
    /// where that frame has it. A drive shown from an earlier frame (restart, step back) starts the characters over.
    /// `countdown` gives a frame's start countdown and whether a jump start ended it ([`Characters::countdown`]), when
    /// known.
    pub fn follow<'a>(
        &mut self,
        frames: impl Iterator<Item = (f64, &'a Sample)>,
        shown: usize,
        countdown: impl Fn(&Sample) -> Option<(f32, bool)>,
    ) {
        if shown + 1 < self.frames {
            self.reset();
        }
        for (end, sample) in frames.skip(self.frames).take((shown + 1).saturating_sub(self.frames)) {
            let car = self.on_line(sample);
            if self.rescue_step == Some(sample.step) {
                self.call_rescue();
            }
            if let Some((left, jumped)) = countdown(sample) {
                self.countdown(left, jumped);
            }
            self.step(end, car);
            self.frames += 1;
        }
    }

    /// One frame at `time` seconds, with the car on the driveline (`None`: no car).
    pub fn step(&mut self, time: f64, car: Option<OnLine>) {
        if time < self.time {
            self.reset();
        }
        let dt = (time - self.time) as f32;
        self.time = time;
        self.car = car;
        if let Some(car) = car {
            self.fire(car);
            self.scripted(car, dt);
        }
        for i in 0..self.actors.len() {
            if self.actors[i].at_car.is_some() {
                self.place_at_car(i);
            }
            if self.visible(i) {
                self.update(i);
            }
        }
    }

    /// `TrigLoop`: each armed channel fires once when `distance - lead * v < x`. Then `Load3b0`'s plain keys, passed
    /// forward (or back) by `x`.
    fn fire(&mut self, car: OnLine) {
        for k in 0..self.triggers.len() {
            let (trigger, fired) = self.triggers[k];
            if fired || trigger.distance - trigger.lead * car.speed >= car.distance {
                continue;
            }
            self.triggers[k].1 = true;
            self.trigger(trigger.object);
        }
        while let Some(&key) = self.plain.get(self.plain_passed).filter(|k| k.distance <= car.distance) {
            self.plain_passed += 1;
            if key.forward() {
                self.trigger(key.object);
            }
        }
        while let Some(&key) =
            self.plain_passed.checked_sub(1).and_then(|k| self.plain.get(k)).filter(|k| car.distance < k.distance)
        {
            self.plain_passed -= 1;
            if !key.forward() {
                self.trigger(key.object);
            }
        }
    }

    /// `E12_AnimSet::FireByIdx` (0x47a7d0): triggers the object of the stage's trigger named `name`, if any.
    pub fn fire_named(&mut self, name: &str) {
        if let Some(object) = rbr_formats::markers::find_named(&self.named, name) {
            self.trigger(object);
        }
    }

    /// The HUD's start countdown (`M573d90`, 0x573d90): the first time it is below 6 s, `start_animation2` starts the
    /// countdown marshal. A jump start ends the countdown with the trigger counted as fired (`M579430`), so `jumped`
    /// fires nothing.
    pub fn countdown(&mut self, countdown: f32, jumped: bool) {
        if !self.start_fired && countdown < rbr_gameplay::race_clock::START_ANIMATION {
            self.start_fired = true;
            if !jumped {
                self.fire_named(rbr_formats::markers::START_ANIMATION);
            }
        }
    }

    /// `E02TrigLoader::Fire` for object `object`: its run script's `Trigger(state, isStart)` sets its state.
    fn trigger(&mut self, object: u32) {
        let Some(&i) = self.by_id.get(&object) else { return };
        let actor = &self.actors[i];
        let Some(lua) = actor.spawn.run.as_ref().and_then(|r| self.runs.get_mut(r)) else { return };
        let args = [Value::Number(actor.state), Value::Number(IS_START)];
        if let Some(Ok(values)) = lua.call("Trigger", &args, &mut RunHost::default()) {
            if let Some(state) = values.first().and_then(Value::number) {
                self.actors[i].state = state;
            }
        }
    }

    /// `E02AnimUpdate` for one visible character: when its clip has no time left (or none plays yet), `Update(state)`
    /// returns the next clip and state; the clip starts now.
    fn update(&mut self, i: usize) {
        let actor = &self.actors[i];
        let left = match actor.clip {
            Some((k, started)) => {
                let clip = &self.clips[actor.spawn.clips[k]];
                clip.end - ((self.time - started) as f32 + clip.start)
            }
            None => 0.0,
        };
        if left > 0.0 {
            return;
        }
        let Some(lua) = actor.spawn.run.as_ref().and_then(|r| self.runs.get_mut(r)) else {
            // No run script: the first clip plays once and holds.
            if actor.clip.is_none() {
                self.actors[i].clip = Some((0, self.time));
            }
            return;
        };
        let mut host = RunHost::default();
        let Some(Ok(values)) = lua.call("Update", &[Value::Number(actor.state)], &mut host) else { return };
        let clip = values.first().and_then(Value::number);
        let state = values.get(1).and_then(Value::number);
        self.host_calls(i, &host.0);
        let actor = &mut self.actors[i];
        if let Some(state) = state {
            actor.state = state;
        }
        // `Unk19f90` ignores a clip index out of range ("Invalid animation data handle").
        if let Some(clip) = clip.filter(|&c| c >= 0.0 && (c as usize) < actor.spawn.clips.len()) {
            actor.clip = Some((clip as usize, self.time));
        }
    }

    /// Applies a run script's host calls for character `i` (`E03AnimImpl::Dispatch`, 0x4462c0, and
    /// `E88ParticleSystem::BirdstrikeCheck`, 0x5f7670).
    fn host_calls(&mut self, i: usize, calls: &[HostCall]) {
        for call in calls {
            match *call {
                HostCall::PlaceAtCar { translate, rotate, strike } => {
                    // `E03AnimImpl::Dispatch`: all false drops the pair; `c` draws a new windscreen point.
                    self.actors[i].at_car = (translate || rotate || strike).then_some(*call);
                    if strike {
                        self.strike_at =
                            random::windscreen_point(self.strike_lcg.windscreen(), self.windscreen_ratio());
                    }
                    if self.actors[i].at_car.is_some() {
                        self.place_at_car(i);
                    }
                }
                HostCall::Birdstrike => {
                    // `BirdstrikeCheck`: `CSigBirdstrike` at the windscreen point, then a new one (`RandPair`).
                    let at = self.strike_point().unwrap_or(self.actors[i].placement.w_axis.truncate());
                    self.events.push(Event::Birdstrike(at));
                    self.strike_at = random::windscreen_point(self.strike_lcg.windscreen(), self.windscreen_ratio());
                }
            }
        }
    }

    /// `E02AnimUpdate`'s test: the character's cell is in view (a stage item outside every cell, by its own point).
    /// The scripted events' characters show while active.
    fn visible(&self, i: usize) -> bool {
        let actor = &self.actors[i];
        let in_view = |centre: Vec3, half: Vec3| self.planes.as_ref().is_none_or(|p| cells::in_view(p, centre, half));
        match (actor.spawn.role, actor.cell) {
            _ if !actor.active => false,
            (Role::Stage, Some(cell)) => in_view(self.cells[cell].centre, self.cells[cell].half),
            (Role::Stage, None) => in_view(actor.spawn.position, Vec3::ZERO),
            _ => true,
        }
    }

    /// Takes the cells' flags for the frame seen through `view_projection` (`VisSelect`): the next steps run the
    /// scripts of the characters in view.
    pub fn look(&mut self, view_projection: Mat4) {
        self.planes = Some(cells::planes(view_projection));
    }

    /// [`Characters::look`], then poses and skins every visible character into its stage meshes and hides the rest.
    pub fn apply(&mut self, renderer: &mut Renderer, view_projection: Mat4) {
        self.look(view_projection);
        for i in 0..self.actors.len() {
            let visible = self.visible(i);
            if visible {
                let actor = &self.actors[i];
                let model = &self.models[actor.spawn.model];
                let world = self.pose(actor);
                for (mesh, &slot) in model.meshes.iter().zip(&actor.slots) {
                    let mut vertices = mesh.template.vertices.clone();
                    skin_into(&mut vertices, mesh, &world, actor.placement);
                    renderer.set_vertices(slot, &vertices, mesh_bounds(&vertices));
                }
            }
            if visible != self.actors[i].shown {
                for &slot in &self.actors[i].slots {
                    renderer.set_visible(slot, visible);
                }
                self.actors[i].shown = visible;
            }
        }
    }

    /// Each character's state, for reports and tests.
    pub fn states(&self) -> Vec<State<'_>> {
        self.actors
            .iter()
            .map(|a| State {
                id: a.spawn.id,
                position: a.spawn.position,
                load: &a.spawn.load,
                clip: a.clip.map(|c| c.0),
                state: a.state,
            })
            .collect()
    }

    /// The triggers that have fired.
    pub fn fired(&self) -> usize {
        self.triggers.iter().filter(|t| t.1).count()
    }
}

/// Skins a model mesh's rest vertices into `vertices` (the template's, same order), posed by `world` and placed by
/// `placement`.
pub(super) fn skin_into(vertices: &mut [Vertex], mesh: &load::ModelMesh, world: &[Mat4], placement: Mat4) {
    let palette = mesh.skin.palette(world, placement);
    let rigid = anim::rigid(mesh.node, world, placement);
    for (k, vertex) in vertices.iter_mut().enumerate() {
        let (Some(&p), Some(&n), Some(w)) = (mesh.positions.get(k), mesh.normals.get(k), mesh.weights.get(k)) else {
            continue;
        };
        let (p, n) = anim::skin_vertex(&palette, rigid, w, p, n);
        vertex.position = p.to_array();
        vertex.normal = n.to_array();
    }
}

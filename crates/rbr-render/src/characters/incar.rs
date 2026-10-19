//! The in-car driver and co-driver (rbr-recomp `notes/stage-characters.md`, section 7), drawn in the car with the playback.
//!
//! - **Load.** `E09_RunCharScripts` (0x466710) runs `scripts/richard.lua` and `scripts/robert.lua`; each calls
//!   `app:LoadHumanCharacter(sgs, ini, shaders, "sgs\\driver.ini", name, "Driver" | "CoDriver")` and adds one clip.
//! - **Seat.** The car `.sgc`'s `D_driver_use` / `D_codriver_use` (`CLM_CharacterCar::Unk1d630`, LM `0x1001d630`):
//!   each character is at its seat with the car's rotation, scaled by 0.001 (`Unk188c0`), so its own space is
//!   millimetres from the seat in the car model's axes. The driver is moved by `m_unk628`, the car's wheel less the
//!   model's `D_steeringwheel - D_mother`, so his hands meet the car's wheel (`Unk14`, LM `0x1001ea00`).
//! - **Driver.** The clip is scrubbed by the steering ([`crew::steer_time`]) and the car's `i_steeringwheel` turns by
//!   [`crew::wheel_angle`] ([`Crew::wheel_turn`]). The gear and handbrake moves come from the frames' control words
//!   ([`crew::Ring`]): the hand follows the Hermite keys at the car's `D_hand R_gear`, `_gear p`, `_handbrake`,
//!   `_handbrake p` nodes, its rotation blends between the clip's pose frames, and the arm IK places the elbow.
//! - **Co-driver** (`CLM_CharacterCoDriver::Update`, LM `0x1001e360`): `robert.sga` looping every 14.133333 s.
//! - **Shake** (`Unk23000`, LM `0x10023000`): the frame's `CSigIncarShake` vectors (`Sample::shake`) lean the spine,
//!   the head and the upper arms ([`crew::lean`]); the hands are put back and the arm IK takes the motion up. Only
//!   the driver's seat emits the signal (its seat word is always 0), so the co-driver's slot 1 never fills and he
//!   does not lean (we take its never-written slot as zero).
//! - **Arms** ([`crew::reach`], `Unk22380`): both arms every frame, the frames rebuilt along the bones, the forearm's
//!   twist node turned with the wrist.
//!
//! Inputs: the frame's control payload (`Sample::control`): word 4 the steering, word 2 the handbrake, word 5 bits 0
//! and 1 a gear change. The recordings and the live frames carry the same payload, so `--drive` and `--live` move
//! the crew alike. While the start hold `CCar+0x1670` is positive the driver ignores them (`OnCarControl` asks the
//! global object's `+0x50`, the fence renderer at 0x16fc4f4, whose slot `0x10` (0x603e80) is
//! `car && !CCar::Test1670()`).
//!
//! Drawn in the chase views only ([`crate::views::CarView::shows_crew`]).
//!
//! Ours: the elbow keys at the gear and handbrake are the clip's elbow at the pose frame (the game adds the seat
//! position to them, which looks like a units slip); the fingers during a move take the clip at `frame / 30` (the
//! game's factor reads as 0.002, marked uncertain in our C++); the wheel turns about its node's `z`; the same lighting
//! as the stage characters.

use super::anim::{Clip, Skeleton};
use super::crew::{self, Arm, Entry, Move, Ring, Settings};
use super::load::{Assets, Model, Scripts};
use super::script::{Host, Lua, Value};
use crate::coords::affine;
use crate::playback::Sample;
use crate::renderer::Renderer;
use crate::stage::{Mesh, Stage};
use glam::{Mat4, Vec3, Vec4};
use rbr_formats::data::Data;
use rbr_formats::{ini, rbz, sgc};
use std::collections::HashMap;
use std::io::Read;

/// The crew's load scripts, in `E09_RunCharScripts` order.
const SCRIPTS: [&str; 2] = ["scripts/richard.lua", "scripts/robert.lua"];
/// The character's scale (`CLM_CharacterIncarPeople::Unk0C`, `Unk188c0(0.001, ...)`).
const SCALE: f32 = 0.001;
/// The co-driver's clock wraps after this (`CLM_CharacterCoDriver::Update`).
const CODRIVER_LOOP: f64 = 14.133333;
/// The driver's settings, in `Root/misc.rbz`.
const DRIVER_INI: &str = "misc/lm_driver.ini";
/// Seconds per recorded substep.
const SUBSTEP: f32 = rbr_recording::STEP_SECONDS;
/// Model-space distance within which an interior mesh's centre is the steering wheel's node, m (ours).
pub const WHEEL_MATCH_METRES: f32 = 0.12;

/// `app:LoadHumanCharacter` and the clip calls of a crew script.
#[derive(Default)]
struct HumanHost {
    model: Option<([String; 3], String)>,
    animations: HashMap<String, String>,
    added: Vec<String>,
}

impl Host for HumanHost {
    fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, String> {
        let text = |i: usize| args.get(i).and_then(Value::text).unwrap_or_default();
        match function {
            "app:LoadHumanCharacter" => self.model = Some(([text(0), text(1), text(2)], text(5))),
            "app:LoadAnimation" => {
                self.animations.insert(text(1), text(0));
            }
            "app:AddAnimationToCharacter" => self.added.push(text(1)),
            other => return Err(format!("{other} is not modelled")),
        }
        Ok(Vec::new())
    }
}

/// The 13 bones `Unk237a0` caches, by node index.
#[derive(Debug, Clone, Copy)]
struct Bones {
    /// The nodes `Unk23000` leans, in its order (`m_unk240[0..5]`): `B_spine01`, `B_spine02`, `B_head`,
    /// `B_upper arm L`, `B_upper arm R`, each by the `CSigIncarShake` vector of its place.
    leaning: [usize; 5],
    /// Left and right; the right twist node (`B_hand rotation R`) is the one the driver's moves set from the pose
    /// frames.
    arms: [Arm; 2],
}

impl Bones {
    fn find(skeleton: &Skeleton, world: &[Mat4]) -> Result<Bones, String> {
        let node = |name: &str| skeleton.find(name).ok_or_else(|| format!("no node {name}"));
        let arm = |side: &str| -> Result<Arm, String> {
            Ok(Arm::new(
                world,
                node(&format!("B_upper arm {side}"))?,
                node(&format!("B_lower arm {side}"))?,
                node(&format!("B_hand rotation {side}"))?,
                node(&format!("B_hand {side}"))?,
            ))
        };
        let [left, right] = [arm("L")?, arm("R")?];
        let leaning = [node("B_spine01")?, node("B_spine02")?, node("B_head")?, left.upper, right.upper];
        Ok(Bones { leaning, arms: [left, right] })
    }
}

/// One of the two.
struct Person {
    model: usize,
    clip: usize,
    /// Character space to car model space: the seat, the scale.
    placement: Mat4,
    bones: Bones,
    parents: Vec<Option<usize>>,
    /// Stage mesh indices, one per model mesh.
    slots: Vec<usize>,
}

/// A hand or elbow key: where it is and its tangents (`SKey`).
#[derive(Debug, Clone, Copy)]
struct Key {
    /// `None`: where the hand (or elbow) is at the wheel now.
    at: Option<Vec3>,
    tan_in: Vec3,
    tan_out: Vec3,
}

/// One move's keys and pose frames (`SMove` pairs in `m_unk4b8` and `m_unk4d4`).
#[derive(Debug, Clone, Copy)]
struct MoveKeys {
    hand: (Key, Key),
    elbow: (Key, Key),
    /// The clip frames whose hand and wrist rotations the move blends between; `None` is the wheel pose now.
    poses: (Option<i32>, Option<i32>),
}

/// The driver's rig, from `Unk14`.
struct Rig {
    settings: Settings,
    /// `m_unk628`, character space.
    offset: Vec3,
    moves: HashMap<MoveId, MoveKeys>,
    /// Descendants of `B_hand R` (the fingers), posed from the clip during a move (`Unk28`).
    fingers: Vec<usize>,
}

type MoveId = u8;

fn move_id(mv: Move) -> MoveId {
    mv as MoveId
}

/// The crew in the car.
pub struct Crew {
    models: Vec<Model>,
    clips: Vec<Clip>,
    /// The driver, then the co-driver.
    people: Vec<Person>,
    rig: Rig,
    ring: Ring,
    /// `i_steeringwheel`'s model-space matrix.
    wheel: Option<Mat4>,
    steer: f32,
    /// The driver's `CSigIncarShake` vectors now.
    shake: [Vec3; 5],
    /// Whether the crew's meshes are drawn.
    shown: bool,
    time: f64,
    frames: usize,
}

impl Crew {
    /// Loads the crew of car `car` and adds their meshes and textures to `stage` (before the playback looks up its
    /// meshes: adding sorts the stage). Call [`Crew::locate`] once every mesh is in.
    pub fn load(data: &Data, car: &str, stage: &mut Stage) -> Result<Crew, String> {
        let nodes = car_nodes(data, car)?;
        let seat = |name: &str| nodes.get(name).map(|m| m.w_axis.truncate()).ok_or_else(|| format!("{car}: no {name}"));
        let seats = [seat("D_driver_use")?, seat("D_codriver_use")?];
        let mut scripts = Scripts::open(data)?;
        let mut assets = Assets::new(data);
        let (mut models, mut clips) = (Vec::new(), Vec::new());
        let mut people = Vec::new();
        for (script, seat) in SCRIPTS.iter().zip(seats) {
            let chunk = scripts.chunk(script).ok_or_else(|| format!("{script}: missing or does not parse"))?;
            let mut lua = Lua::new();
            lua.set_global("rootpath", Value::Str(String::new()));
            let mut host = HumanHost::default();
            lua.run(&chunk, &mut host).map_err(|e| format!("{script}: {e}"))?;
            let (paths, _kind) = host.model.ok_or_else(|| format!("{script}: no LoadHumanCharacter"))?;
            let model = assets.model(&paths, &mut models)?;
            let sga = host
                .added
                .first()
                .and_then(|name| host.animations.get(name))
                .ok_or_else(|| format!("{script}: no clip"))?;
            let clip = assets.clip(model, sga, &models, &mut clips)?;
            let skeleton = &models[model].skeleton;
            let rest = clips[clip].pose(skeleton, 0.0);
            people.push(Person {
                model,
                clip,
                placement: Mat4::from_translation(seat) * Mat4::from_scale(Vec3::splat(SCALE)),
                bones: Bones::find(skeleton, &rest).map_err(|e| format!("{}: {e}", paths[0]))?,
                parents: skeleton.nodes.iter().map(|n| n.parent).collect(),
                slots: Vec::new(),
            });
        }
        let settings = Settings::read(&driver_ini(data)?);
        let in_seat = |name: &str| nodes.get(name).map(|m| (m.w_axis.truncate() - seats[0]) / SCALE);
        let rig = rig(&people[0], &models, &clips, settings, in_seat);
        let stick = in_seat("D_hand R_gear").is_some() && in_seat("D_hand R_gear p").is_some();
        let crew = Crew {
            models,
            clips,
            people,
            rig,
            ring: Ring::new(stick),
            wheel: nodes.get("i_steeringwheel").copied(),
            steer: 0.0,
            shake: [Vec3::ZERO; 5],
            shown: true,
            time: 0.0,
            frames: 0,
        };
        let meshes: Vec<Mesh> = (0..crew.people.len()).flat_map(|i| crew.meshes(i, Mat4::IDENTITY)).collect();
        stage.meshes.extend(meshes);
        stage.meshes.sort_by_key(|mesh| mesh.layer);
        stage.textures.extend(std::mem::take(&mut assets.textures));
        Ok(crew)
    }

    /// Finds the crew's meshes in `stage` by their first-frame vertices.
    pub fn locate(&mut self, stage: &Stage) {
        let mut taken = vec![false; stage.meshes.len()];
        for i in 0..self.people.len() {
            let mut slots = Vec::new();
            for mesh in self.meshes(i, Mat4::IDENTITY) {
                if let Some(k) = stage.meshes.iter().enumerate().position(|(k, m)| !taken[k] && *m == mesh) {
                    taken[k] = true;
                    slots.push(k);
                }
            }
            self.people[i].slots = slots;
        }
    }

    /// Starts over at the first frame.
    pub fn reset(&mut self) {
        self.ring = Ring::new(self.ring_stick());
        self.shake = [Vec3::ZERO; 5];
        self.time = 0.0;
        self.frames = 0;
        self.steer = 0.0;
    }

    /// The driver's `LM_Driver.ini`.
    pub fn settings(&self) -> &Settings {
        &self.rig.settings
    }

    /// Whether the car has the gear-stick nodes (else gear changes are made at the wheel).
    pub fn has_stick(&self) -> bool {
        self.ring_stick()
    }

    fn ring_stick(&self) -> bool {
        self.rig.moves.contains_key(&move_id(Move::WheelToGear))
    }

    /// Steps through the playback frames up to the one showing. A drive shown from an earlier frame starts over.
    pub fn follow<'a>(&mut self, frames: impl Iterator<Item = (f64, &'a Sample)>, shown: usize) {
        if shown + 1 < self.frames {
            self.reset();
        }
        for (end, sample) in frames.skip(self.frames).take((shown + 1).saturating_sub(self.frames)) {
            self.step(end, sample);
            self.frames += 1;
        }
    }

    /// One frame ending at `end` seconds.
    pub fn step(&mut self, end: f64, sample: &Sample) {
        let dt = ((end - self.time) as f32).max(SUBSTEP);
        self.time = end;
        if sample.hold <= 0.0 {
            let [_, _, handbrake, _, steer, ..] = sample.control;
            let flags = sample.control[5].to_bits();
            self.steer = steer;
            self.ring.on_control(handbrake, flags & 3 != 0);
        }
        self.ring.advance(dt, |m| self.rig.settings.length(m));
        self.shake = sample.shake;
    }

    /// The move playing, for reports and tests.
    pub fn playing(&self) -> Option<(Entry, f32)> {
        self.ring.playing()
    }

    /// The steering wheel's turn in car model space, for [`crate::playback::Playback::turn`]: the interior mesh
    /// centred within [`WHEEL_MATCH_METRES`] of the wheel's node turns by the matrix.
    pub fn wheel_turn(&self) -> Option<(Vec3, f32, Mat4)> {
        let node = self.wheel?;
        let angle = crew::wheel_angle(self.steer, self.rig.settings.max_steer);
        // The sign is the clip's: its hands turn about the node's `z` by `+angle` (about 45° of the wheel's 60°).
        Some((node.w_axis.truncate(), WHEEL_MATCH_METRES, node * Mat4::from_rotation_z(angle) * node.inverse()))
    }

    /// Poses and skins the crew in the car whose model space goes to stage space by `body`, or hides them when not
    /// `shown`.
    pub fn apply(&mut self, renderer: &mut Renderer, body: Mat4, shown: bool) {
        if shown != self.shown {
            for &slot in self.people.iter().flat_map(|p| &p.slots) {
                renderer.set_visible(slot, shown);
            }
            self.shown = shown;
        }
        if !shown {
            return;
        }
        for (i, person) in self.people.iter().enumerate() {
            for (mesh, &slot) in self.meshes(i, body).iter().zip(&person.slots) {
                renderer.set_vertices(slot, &mesh.vertices, mesh.bounds);
            }
        }
    }

    /// Person `i`'s meshes posed now, placed by `body`.
    fn meshes(&self, i: usize, body: Mat4) -> Vec<Mesh> {
        let person = &self.people[i];
        let model = &self.models[person.model];
        let world = self.pose(i);
        model
            .meshes
            .iter()
            .map(|mesh| {
                let mut placed = mesh.template.clone();
                super::skin_into(&mut placed.vertices, mesh, &world, body * person.placement);
                placed.bounds = crate::renderer::mesh_bounds(&placed.vertices);
                placed
            })
            .collect()
    }

    /// The clip pose of person `i` at clip time `t` (the root's time: `Unk18` and `Unk14` pass it as it is, not from
    /// the clip's range), in character space (the driver moved by `m_unk628`).
    fn clip_pose(&self, i: usize, t: f32) -> Vec<Mat4> {
        let person = &self.people[i];
        let clip = &self.clips[person.clip];
        let mut world = clip.pose(&self.models[person.model].skeleton, t);
        if i == 0 {
            let shift = Mat4::from_translation(self.rig.offset);
            world.iter_mut().for_each(|m| *m = shift * *m);
        }
        world
    }

    /// Person `i`'s node matrices now, in the game's order: the clip, the shake and the hands put back (`Unk23000`),
    /// the driver's hand move (`Unk24`), the arm IK (`Unk22ce0`), the fingers (`Unk28`).
    fn pose(&self, i: usize) -> Vec<Mat4> {
        let person = &self.people[i];
        let bones = person.bones;
        let mut world = if i == 0 {
            self.clip_pose(0, crew::steer_time(self.steer, self.rig.settings.normal))
        } else {
            self.clip_pose(i, self.clips[person.clip].start + self.time.rem_euclid(CODRIVER_LOOP) as f32)
        };
        let hands = bones.arms.map(|arm| world[arm.hand]);
        let shake = if i == 0 { self.shake } else { [Vec3::ZERO; 5] };
        for (&node, &by) in bones.leaning.iter().zip(&shake) {
            let leaned = crew::lean(world[node], by);
            crew::edit(&person.parents, &mut world, &[(node, leaned)]);
        }
        for (arm, hand) in bones.arms.iter().zip(hands) {
            crew::edit(&person.parents, &mut world, &[(arm.hand, hand)]);
        }
        let mut poles = bones.arms.map(|arm| world[arm.lower].w_axis.truncate());
        let mut fingers = None;
        if let (0, Some((entry, t))) = (i, self.ring.playing()) {
            if let Some(keys) = self.rig.moves.get(&move_id(entry.mv)) {
                let [_, right] = bones.arms;
                let hand_now = world[right.hand];
                let at = |key: &Key, live: Vec3| key.at.unwrap_or(live);
                let (from, to) = keys.hand;
                let hand_at = crew::hermite(
                    at(&from, hand_now.w_axis.truncate()),
                    from.tan_out,
                    at(&to, hand_now.w_axis.truncate()),
                    to.tan_in,
                    t,
                );
                let elbow_now = poles[1];
                let (from, to) = keys.elbow;
                poles[1] = crew::hermite(at(&from, elbow_now), from.tan_out, at(&to, elbow_now), to.tan_in, t);
                let pose_of = |frame: Option<i32>| frame.map(|f| self.clip_pose(0, f as f32 / crew::FRAMES_PER_SECOND));
                let (a, b) = (pose_of(keys.poses.0), pose_of(keys.poses.1));
                let hand_a = a.as_ref().map_or(hand_now, |p| p[right.hand]);
                let hand_b = b.as_ref().map_or(hand_now, |p| p[right.hand]);
                let wrist_a = a.as_ref().map_or(world[right.twist], |p| p[right.twist]);
                let wrist_b = b.as_ref().map_or(world[right.twist], |p| p[right.twist]);
                world[right.twist] = with_position(blend(wrist_a, wrist_b, t), world[right.twist].w_axis.truncate());
                let hand = with_position(blend(hand_a, hand_b, t), hand_at);
                crew::edit(&person.parents, &mut world, &[(right.hand, hand)]);
                let (start, end) = self.rig.settings.frames(entry.mv);
                let frame = start as f32 + (end - start) as f32 * t;
                fingers = Some(self.clip_pose(0, frame / crew::FRAMES_PER_SECOND));
            }
        }
        for (arm, pole) in bones.arms.iter().zip(poles) {
            crew::reach(&mut world, arm, pole);
        }
        if let Some(clip) = fingers {
            let hand = bones.arms[1].hand;
            let to_hand = world[hand] * clip[hand].inverse();
            for &f in &self.rig.fingers {
                world[f] = to_hand * clip[f];
            }
        }
        world
    }
}

/// `Unk14`'s rig of the driver: the wheel offset, the move keys from the car's hand nodes (`in_seat`, character
/// space) and the clip's pose frames.
fn rig(
    driver: &Person,
    models: &[Model],
    clips: &[Clip],
    settings: Settings,
    in_seat: impl Fn(&str) -> Option<Vec3>,
) -> Rig {
    let skeleton = &models[driver.model].skeleton;
    let clip = &clips[driver.clip];
    let pose = |frame: i32| clip.pose(skeleton, frame as f32 / crew::FRAMES_PER_SECOND);
    let node_at = |world: &[Mat4], name: &str| skeleton.find(name).map(|i| world[i].w_axis.truncate());
    let normal = pose(settings.normal);
    let offset = match (in_seat("i_steeringwheel"), node_at(&normal, "D_steeringwheel"), node_at(&normal, "D_mother")) {
        (Some(wheel), Some(model_wheel), Some(mother)) => wheel - (model_wheel - mother),
        _ => Vec3::ZERO,
    };
    let [_, right] = driver.bones.arms;
    let elbow_at = |frame: i32| Some(pose(frame)[right.lower].w_axis.truncate() + offset);
    let tangent = |name: &str, default: [f32; 3]| settings.tangent(name, Vec3::from(default));
    let key = |at: Option<Vec3>, tan_in: Vec3, tan_out: Vec3| Key { at, tan_in, tan_out };
    let wheel_hand = key(None, Vec3::ZERO, tangent("Hand From Steeringwheel", [0.0, 0.0, 300.0]));
    let wheel_elbow = key(None, Vec3::ZERO, tangent("Elbow From Steeringwheel", [0.0, 0.0, 300.0]));
    let s = &settings;
    let mut moves = HashMap::new();
    let gear = in_seat("D_hand R_gear").zip(in_seat("D_hand R_gear p"));
    let handbrake = in_seat("D_hand R_handbrake").zip(in_seat("D_hand R_handbrake p"));
    if let Some((gear, pushed)) = gear {
        moves.insert(
            move_id(Move::WheelToGear),
            MoveKeys {
                hand: (
                    wheel_hand,
                    key(Some(gear), tangent("Hand From Gear to Steeringwheel", [0.0, 0.0, -300.0]), Vec3::ZERO),
                ),
                elbow: (
                    wheel_elbow,
                    key(elbow_at(s.gear), tangent("Elbow From Gear to Steeringwheel", [0.0, 0.0, -300.0]), Vec3::ZERO),
                ),
                poses: (None, Some(s.gear)),
            },
        );
        moves.insert(
            move_id(Move::GearPush),
            MoveKeys {
                hand: (
                    key(Some(gear), Vec3::ZERO, tangent("Hand From Gear to Gear Pushed", [0.0, 0.0, -100.0])),
                    key(Some(pushed), tangent("Hand From Gear Pushed to Gear", [0.0, 0.0, -100.0]), Vec3::ZERO),
                ),
                elbow: (
                    key(elbow_at(s.gear), Vec3::ZERO, tangent("Elbow From Gear to Gear Pushed", [0.0, 0.0, -100.0])),
                    key(
                        elbow_at(s.gear_pushed),
                        tangent("Elbow From Gear Pushed to Gear", [0.0, 0.0, -100.0]),
                        Vec3::ZERO,
                    ),
                ),
                poses: (Some(s.gear), Some(s.gear_pushed)),
            },
        );
    }
    if let Some((brake, pulled)) = handbrake {
        moves.insert(
            move_id(Move::WheelToHandbrake),
            MoveKeys {
                hand: (
                    wheel_hand,
                    key(Some(brake), tangent("Hand From Handbrake to Steeringwheel", [0.0, 0.0, 300.0]), Vec3::ZERO),
                ),
                elbow: (
                    wheel_elbow,
                    key(
                        elbow_at(s.handbrake),
                        tangent("Elbow From Handbrake to Steeringwheel", [0.0, 0.0, 300.0]),
                        Vec3::ZERO,
                    ),
                ),
                poses: (None, Some(s.handbrake)),
            },
        );
        moves.insert(
            move_id(Move::HandbrakePull),
            MoveKeys {
                hand: (
                    key(Some(brake), Vec3::ZERO, tangent("Hand From Handbrake to Handbrake Pulled", [0.0, 0.0, 100.0])),
                    key(
                        Some(pulled),
                        tangent("Hand From Handbrake Pulled to Handbrake", [0.0, 0.0, 100.0]),
                        Vec3::ZERO,
                    ),
                ),
                elbow: (
                    key(
                        elbow_at(s.handbrake),
                        Vec3::ZERO,
                        tangent("Elbow From Handbrake to Handbrake Pulled", [0.0, 0.0, 100.0]),
                    ),
                    key(
                        elbow_at(s.handbrake_pulled),
                        tangent("Elbow From Handbrake Pulled to Handbrake", [0.0, 0.0, 100.0]),
                        Vec3::ZERO,
                    ),
                ),
                poses: (Some(s.handbrake), Some(s.handbrake_pulled)),
            },
        );
        if let Some((gear, _)) = gear {
            moves.insert(
                move_id(Move::GearToHandbrake),
                MoveKeys {
                    hand: (
                        key(Some(gear), Vec3::ZERO, tangent("Hand From Gear to Handbrake", [0.0, 300.0, 0.0])),
                        key(Some(brake), tangent("Hand From Handbrake to Gear", [0.0, 300.0, 0.0]), Vec3::ZERO),
                    ),
                    elbow: (
                        key(elbow_at(s.gear), Vec3::ZERO, tangent("Elbow From Gear to Handbrake", [0.0, 300.0, 0.0])),
                        key(
                            elbow_at(s.handbrake),
                            tangent("Elbow From Handbrake to Gear", [0.0, 300.0, 0.0]),
                            Vec3::ZERO,
                        ),
                    ),
                    poses: (Some(s.gear), Some(s.handbrake)),
                },
            );
        }
    }
    if gear.is_none() {
        let still = key(None, Vec3::ZERO, Vec3::ZERO);
        moves.insert(
            move_id(Move::WheelGear),
            MoveKeys { hand: (still, still), elbow: (still, still), poses: (None, None) },
        );
    }
    let hand = right.hand;
    let mut below = vec![false; skeleton.nodes.len()];
    let mut fingers = Vec::new();
    for (i, node) in skeleton.nodes.iter().enumerate() {
        if node.parent.is_some_and(|p| p == hand || below[p]) {
            below[i] = true;
            fingers.push(i);
        }
    }
    Rig { settings, offset, moves, fingers }
}

/// The element-wise blend `a * (1 - t) + b * t` with its three axes renormalised (`Unk24`).
fn blend(a: Mat4, b: Mat4, t: f32) -> Mat4 {
    let mix = |x: Vec4, y: Vec4| x * (1.0 - t) + y * t;
    let axis = |x: Vec4, y: Vec4| mix(x, y).truncate().normalize_or_zero().extend(0.0);
    Mat4::from_cols(
        axis(a.x_axis, b.x_axis),
        axis(a.y_axis, b.y_axis),
        axis(a.z_axis, b.z_axis),
        mix(a.w_axis, b.w_axis),
    )
}

/// `m` moved so that its origin is at `p`.
fn with_position(mut m: Mat4, p: Vec3) -> Mat4 {
    m.w_axis = p.extend(1.0);
    m
}

/// `Misc\\LM_Driver.ini` from `Root/misc.rbz` (the 1.02 patch's when it has one).
fn driver_ini(data: &Data) -> Result<Vec<ini::Section>, String> {
    let path = data.resolve("misc.rbz").or_else(|| data.resolve("Root/misc.rbz")).ok_or("misc.rbz: not found")?;
    let mut zip = rbz::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.replace('\\', "/").to_ascii_lowercase();
        if name == DRIVER_INI {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            return ini::parse(&String::from_utf8_lossy(&bytes))
                .map_err(|e| format!("{DRIVER_INI}: line {}: {}", e.line, e.message));
        }
    }
    Err(format!("{}: no {DRIVER_INI}", path.display()))
}

/// The car `.sgc`'s nodes and meshes by name, as model-space matrices (`sgc::chained_worlds`).
fn car_nodes(data: &Data, car: &str) -> Result<HashMap<String, Mat4>, String> {
    let path = crate::playback::model_path(data, car)?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let model = sgc::parse(&bytes).map_err(|e| format!("{}: {e:?}", path.display()))?;
    let chained = sgc::chained_worlds(&model);
    Ok(model
        .iter()
        .filter_map(|e| match e {
            sgc::Element::Mesh(m) => Some(&m.node),
            sgc::Element::Node(n) => Some(n),
            _ => None,
        })
        .map(|n| (n.name.clone(), affine(chained.get(&n.id).copied().unwrap_or(n.world))))
        .collect())
}

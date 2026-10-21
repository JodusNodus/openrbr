//! What a live answer carries besides the frame: the HUD, damage, particle, sound, pace-note, split, dash, gearbox,
//! shift-point and scenery records of a step (`super::Live` writes them), read from the pure physics' memory and the game.

use rbr_formats::markers::{MarkerKey, TAG_FINISH, TAG_SPLIT, TAG_START};
use rbr_formats::times::StageTimes;
use rbr_formats::trk::DriveLineRecord;
use rbr_gameplay::car_damage::CarriedDamage;
use rbr_gameplay::driveline;
use rbr_gameplay::game::Game;
use rbr_gameplay::markers::{self, MarkerEvent, MarkerTicker, PaceCallout, FIRED};
use rbr_gameplay::race_clock::RaceClock;
use rbr_gameplay::result::Outcome;
use rbr_hybrid::pure::{self, CAR_BYTES, CAR_PTR};
use rbr_physics::car_body::steering_ffb;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout as physics_layout;
use rbr_physics::rigid_body::layout::Body;
use rbr_physics::Variant;
use rbr_recording::damage::{self, Damage};
use rbr_recording::dash::{Dash, StackKey};
use rbr_recording::detach::{self, PartPose};
use rbr_recording::ffb::Steering;
use rbr_recording::flow::Flow;
use rbr_recording::gearbox::Gearbox;
use rbr_recording::hud::{Hud, RaceHud};
use rbr_recording::live as records;
use rbr_recording::pacenote::{self, Callout};
use rbr_recording::particles::{self, PAYLOAD_BYTES};
use rbr_recording::pose;
use rbr_recording::shift::ShiftPoints;
use rbr_recording::sound::{self, FmodBlock, NgpSound, Sound, TyreMax, CAR_ENGINE, ENGINE_STATE};
use rbr_recording::splits::{Split, Splits};

use crate::saves::Saves;
use crate::stage::StageData;

/// What a HUD record reads, as `CCar::Sample9800` (0x4e9800) gives the dash its `CSigCarSensorData` (the HUD's receiver
/// 0x573cc0 draws word 0 times 9.549 as the rpm): the gear from the drive state (`CCar` +0x4b0, then +8; its +0x54c,
/// `CDrive::Get4530`) and the engine speed from the engine state (`CCar` +0x690, then +8; its +0x188,
/// `CEngine::GetState188`). The drive state's +0x45c, which `harness/live.py`'s `hud` reads, is the drive's engine
/// speed the VCU reads (`CDrive::Get4520`); it agrees within a rad/s at idle.
const CAR_DRIVE: u32 = 0x4b0;
/// The VCU definition (`CCar` +0x850) and its `Gear<N>Upshift`, `Gear<N>Downshift` and `RPMLimit` values
/// (`rbr_physics::vcu::layout`).
const CAR_VCU_DEF: u32 = 0x850;
const VCU_UPSHIFT: u32 = 0x18;
const VCU_DOWNSHIFT: u32 = 0x28;
const VCU_GEAR_STRIDE: u32 = 0x20;
const VCU_RPM_LIMIT: u32 = 0x188;
const DRIVE_STATE: u32 = 0x8;
const STATE_GEAR: u32 = 0x54c;
const STATE_ENGINE_SPEED: u32 = 0x188;
/// `CCar` +0x838: the light system (`CCarLightSystem`, `notes/renderer.md` "Lights").
const CAR_LIGHTS: u32 = 0x838;
const CAR_SPEED: u32 = 0x15fc;
/// `CCar` +0x17c: the parameter `E12_SampleX` (0x475ea0) turns into the driveline distance; `CEngine` +0x48: its
/// option block.
const CAR_DRIVELINE_PARAMETER: u32 = 0x17c;
const ENGINE_OPTIONS: u32 = 0x48;
const INTERACTIVE_HANDLE_BASE: u32 = 0x9c40;
/// `CHingedRigidBody::SimFD00` and `CMountedRigidBody::StepBig` clear a detached body's parent (`BODY_PARENT`,
/// `rbr-physics` `entities::layout`).
const BODY_PARENT: u32 = 0x44;

#[derive(Clone, Copy)]
pub struct ActiveInteractivePose {
    pub handle: u32,
    pub position: [f32; 3],
    pub orientation: [f32; 4],
}

/// What the viewer draws from one step: the car and wheel bytes, the HUD record (HUD2, with the race clock), the
/// damage record and the step's tyre-particle payloads (PTC1, `rbr_recording::particles`).
#[derive(Clone)]
pub struct Picture {
    pub body: Vec<u8>,
    pub hud: Vec<u8>,
    pub damage: Vec<u8>,
    pub tyres: Vec<u8>,
    /// The step's other particle signals (PEV1, `rbr_recording::particles::events_record`).
    pub events: Vec<u8>,
    pub detached: Vec<u8>,
    pub splits: Vec<u8>,
    pub steering: Vec<u8>,
    pub scenery: Vec<ActiveInteractivePose>,
    pub sound: Vec<u8>,
    pub pace: Vec<u8>,
    pub dash: Vec<u8>,
    /// FLW1: the step's race-flow events.
    pub flow: Vec<u8>,
    /// The tagged NSD1 record under NGP (NGP.dll's sound words), empty under 1.02.
    pub ngp_sound: Vec<u8>,
    /// GBX1: the gear ratios and the drive speed (`gearbox`), then SHF1: the shift points (`shift_points`).
    pub gearbox: Vec<u8>,
    /// The tagged FMB1 record: what NGP's FMOD block takes from the car beyond SND1 (`rbr_recording::sound::FmodBlock`).
    pub fmod_block: Vec<u8>,
    /// The tagged CDM1 record: the damage handler's carried words, for the HUD's car status.
    pub carried: Vec<u8>,
}

/// The split record after a step: the stage result's splits with the reference times (`rbr_recording::splits`).
pub fn split_record(game: &Game, stage: &StageData, saves: Option<&Saves>) -> Splits {
    let result = game.result();
    let card = game.time_card();
    // The stage record's row before the race (equal to times.ini's until a record is set), else times.ini's.
    let reference = saves
        .map(|s| s.reference())
        .map(|[first, second, finish]| StageTimes { stage: stage.number, splits: [first, second], finish })
        .or(stage.reference);
    let reference = reference.as_ref();
    Splits {
        finished: result.outcome == Outcome::Finished,
        over: game.flow().end().is_some(),
        retired: result.outcome == Outcome::Retired,
        // The card's clock stops at the finish, as `TimeCard::elapsed` does.
        elapsed: (card.finish_time.unwrap_or(game.elapsed()) - card.origin) as f32,
        race_time: game.race_clock().time,
        penalties: result.penalties,
        reference_finish: reference.map(|r| r.finish),
        splits: result
            .splits
            .iter()
            .enumerate()
            .map(|(i, &time)| Split { time, reference: reference.and_then(|r| r.splits.get(i).copied()) })
            .collect(),
        standing: saves.map(Saves::standing),
    }
}

impl Picture {
    pub fn of(
        mem: &Mem,
        race: RaceHud,
        splits: &Splits,
        interactive_handle_end: u32,
        tyres: &[[u8; PAYLOAD_BYTES]],
        detach: &[(u32, u32)],
        variant: Variant,
    ) -> Self {
        let car = mem.u32(CAR_PTR);
        let state = mem.u32(car + CAR_DRIVE + DRIVE_STATE);
        let hud = Hud {
            gear: mem.u32(state + STATE_GEAR),
            engine_speed: mem.f32(mem.u32(car + CAR_ENGINE + ENGINE_STATE) + STATE_ENGINE_SPEED),
            speed: f32::from_bits(mem.u32(car + CAR_SPEED)),
            race: Some(race),
        };
        let handler = mem.u32(car + damage::CAR_DAMAGE_HANDLER);
        let damage = Damage {
            zones: std::array::from_fn(|zone| {
                f32::from_bits(mem.u32(handler + damage::HANDLER_ZONES + 4 * zone as u32))
            }),
            code: mem.u32(handler + damage::HANDLER_EVENT_CODE),
            index: mem.u32(handler + damage::HANDLER_EVENT_INDEX),
        };
        Self {
            body: pure::frame_bytes(mem),
            hud: hud.to_bytes(),
            damage: damage.to_bytes(),
            tyres: particles::record(tyres),
            events: particles::events_record(&[]),
            detached: detached_parts(mem, detach),
            splits: splits.to_bytes(),
            steering: Steering { force: steering_ffb::force(mem, car, variant), ngp: variant == Variant::Ngp778 }
                .to_bytes(),
            scenery: active_interactive_poses(mem, interactive_handle_end),
            sound: Sound {
                water: rbr_physics::engine_driveline::layout::sim_e970(mem, car + CAR_ENGINE, variant) as f32,
                ..Sound::read_from(car, |at| {
                    mem.try_read(at, 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                })
            }
            .to_bytes(),
            pace: pacenote::to_bytes(&[]),
            dash: Dash::default().to_bytes(),
            flow: Flow::default().to_bytes(),
            ngp_sound: ngp_sound(mem, variant),
            gearbox: [gearbox(mem, car + CAR_DRIVE).to_bytes(), shift_points(mem, car).to_bytes()].concat(),
            fmod_block: {
                let block = FmodBlock::read_from(car, |at| {
                    mem.try_read(at, 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                });
                let mut out = Vec::new();
                records::write_record(&mut out, sound::FMB1, &block.to_bytes());
                out
            },
            carried: {
                let mut payload = u32::from(variant == Variant::Ngp778).to_le_bytes().to_vec();
                payload.extend(CarriedDamage::read(|at| mem.u32(handler + at)).to_bytes());
                let mut out = Vec::new();
                records::write_record(&mut out, damage::CDM1, &payload);
                out
            },
        }
    }
}

/// The GBX1 record of the drive at `drive` (`CDrive`; its definition at +4, its state at +8).
pub fn gearbox(mem: &Mem, drive: u32) -> Gearbox {
    use rbr_physics::engine_driveline::layout::{DriveDef, DriveState, GearBand, WorkerDef};
    let (def, state) = (mem.u32(drive + 4), mem.u32(drive + DRIVE_STATE));
    Gearbox {
        drive_speed: DriveState::load(mem, state).drive_speed_key * WorkerDef::load(mem, def).ratio_gain,
        count: DriveDef::load(mem, def).gear_count,
        ratios: std::array::from_fn(|g| GearBand::load(mem, def, g as u32).ratio),
    }
}

/// The SHF1 record of the car at `car`: its VCU definition's shift points and rev limit.
pub fn shift_points(mem: &Mem, car: u32) -> ShiftPoints {
    let def = mem.u32(car + CAR_VCU_DEF);
    let at = |base: u32, gear: usize| mem.f32(def + base + VCU_GEAR_STRIDE * gear as u32);
    ShiftPoints {
        up: std::array::from_fn(|g| at(VCU_UPSHIFT, g)),
        down: std::array::from_fn(|g| at(VCU_DOWNSHIFT, g)),
        limit: mem.f32(def + VCU_RPM_LIMIT),
    }
}

/// The tagged NSD1 record under NGP: the words NGP's sound patches read (`rbr_recording::sound::NgpSound`).
pub fn ngp_sound(mem: &Mem, variant: Variant) -> Vec<u8> {
    if variant != Variant::Ngp778 {
        return Vec::new();
    }
    // A snapshot's NGP.dll where the game loaded it, the boot's at the reference runs' base.
    let base = match rbr_hybrid::ngp::base_in(mem) {
        0 => rbr_physics::ngp::exe_patches::BASE,
        base => base,
    };
    let word = |file: u32| mem.f32(file - 0x1000_0000 + base);
    let ngp = NgpSound {
        gear_load_cap: word(sound::NGP_GEAR_LOAD_CAP),
        diff_load_cap: word(sound::NGP_DIFF_LOAD_CAP),
        min_rpm: word(sound::NGP_MIN_RPM),
        idle_rpm: mem.u32(mem.u32(sound::SETUP_OPTIONS) + sound::OPTIONS_IDLE_RPM) as i32 as f32,
    };
    let mut out = Vec::new();
    records::write_record(&mut out, sound::NSD1, &ngp.to_bytes());
    out
}

/// The PEV1 record of a step: the damage, damping and engine-fire emits `pure` kept, then the engine's
/// `CSigEngineNoise` words after the step (`rbr_recording::sound`'s engine state) and the exhaust's inputs (the state's
/// +0x200 and the engine's +0x188, `particles::EXHAUST`).
pub fn particle_events(pure: &pure::Pure) -> Vec<u8> {
    let mut events = pure.take_particle_events();
    let mem = &pure.mem;
    // The engine is embedded in the car (`rbr_recording::sound`'s reader takes it so too).
    let engine = mem.u32(CAR_PTR) + CAR_ENGINE;
    let state = mem.u32(engine + ENGINE_STATE);
    if state != 0 {
        let mut noise = [0; particles::EVENT_PAYLOAD_BYTES];
        if let Some(words) = mem.try_read(state + sound::STATE_NOISE, 0x30) {
            noise[..0x30].copy_from_slice(&words);
            events.push((particles::ENGINE_NOISE, noise));
        }
        let mut exhaust = [0; particles::EVENT_PAYLOAD_BYTES];
        if let (Some(flow), Some(broken)) = (mem.try_read(state + 0x200, 4), mem.try_read(engine + 0x188, 4)) {
            exhaust[..4].copy_from_slice(&flow);
            exhaust[4..8].copy_from_slice(&broken);
            events.push((particles::EXHAUST, exhaust));
        }
    }
    if let Some(lights) = lights_event(mem) {
        events.push((particles::LIGHTS, lights));
    }
    particles::events_record(&events)
}

/// The car's lamps (`particles::LIGHTS`): `CCarLightSystem` (car +0x838) points at its state (+8; each kind's switch
/// at +0x2c + 0x10 * (kind - 1)) and the light block (+0x14; the lamp works and glass words of kind k at
/// +0x1c + 0x20 * (k - 1), the master word at +0xdc). `None` before the car has them.
pub fn lights_event(mem: &Mem) -> Option<[u8; particles::EVENT_PAYLOAD_BYTES]> {
    let system = mem.u32(CAR_PTR) + CAR_LIGHTS;
    let (state, block) = (mem.u32(system + 8), mem.u32(system + 0x14));
    if state == 0 || block == 0 {
        return None;
    }
    let word = |at: u32| mem.try_read(at, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let mut words = [0u32; 13];
    for k in 0..6 {
        words[k] = word(state + 0x2c + 0x10 * k as u32)?;
        let group = block + 0x1c + 0x20 * k as u32;
        for lamp in 0..4 {
            let works = word(group + 8 * lamp)? != 0;
            let glass = word(group + 8 * lamp + 4)? != 0;
            words[7 + k] |= u32::from(works) << lamp | u32::from(glass) << (8 + lamp);
        }
    }
    words[6] = word(block + 0xdc)?;
    let mut out = [0; particles::EVENT_PAYLOAD_BYTES];
    for (k, w) in words.iter().enumerate() {
        out[4 * k..4 * k + 4].copy_from_slice(&w.to_le_bytes());
    }
    Some(out)
}

/// The SND1 payload `sound` with the step's `CSigEngineFire` emits and the frame's tyre block word 1.
pub fn with_fires(sound: &[u8], fires: u32, tyre_max: TyreMax) -> Vec<u8> {
    match Sound::from_bytes(sound) {
        Some(sound) => {
            let mut sound = Sound { fires, ..sound };
            tyre_max.apply(&mut sound);
            sound.to_bytes()
        }
        None => sound.to_vec(),
    }
}

/// The HUD's dash values after a step (`rbr_recording::dash`): the car's driveline distance (`track` the `.trk`
/// records, `values` their arc lengths, `keys` the stage's markers), the sensor values `CCar::Sample9800` (0x4e9800)
/// packs for the lamps, the step's stand-still and car-problem payloads, the key distance of each of the step's
/// callouts, the wrong-way test (`M579590`), the rally false starts for the warning, the start clock and the pace stack
/// with each key's driveline point.
#[allow(clippy::too_many_arguments)]
pub fn dash(
    mem: &Mem,
    variant: Variant,
    track: &[DriveLineRecord],
    values: &[f32],
    keys: &[MarkerKey],
    signals: (Option<[u32; 2]>, Option<u32>),
    callouts: &[&PaceCallout],
    false_starts: u8,
    clock: Option<f32>,
) -> Dash {
    use rbr_physics::engine_driveline::layout as engine;
    let car = mem.u32(CAR_PTR);
    let at = |tag| keys.iter().find(|k| k.tag == tag).map(|k| k.distance);
    let block = mem.u32(car + sound::CAR_SENSOR_BLOCK);
    let options = mem.u32(car + CAR_ENGINE + ENGINE_OPTIONS);
    let parameter = mem.f32(car + CAR_DRIVELINE_PARAMETER);
    let distance = driveline::sample_x(values, parameter);
    let wrong_way = pose::car_pose(&mem.read(car, CAR_BYTES))
        .zip(driveline::sample_direction(track, parameter))
        .is_some_and(|(pose, along)| driveline::wrong_way(pose.velocity, pose.rotate([0.0, 1.0, 0.0]), along));
    let stack = distance.map_or(Vec::new(), |x| {
        markers::pace_stack(keys, x)
            .into_iter()
            .map(|k| StackKey {
                tag: k.tag,
                word1: k.word1,
                distance: k.distance,
                position: point_at(track, k.distance),
            })
            .collect()
    });
    Dash {
        distance,
        start: at(TAG_START),
        finish: at(TAG_FINISH),
        water: engine::sim_e970(mem, car + CAR_ENGINE, variant) as f32,
        brakes: mem.u32(block + 0x80) as f32,
        engine: if options != 0 { mem.f32(options + 0x58) } else { 0.0 },
        battery: mem.f32(block + 0x8c) * sound::BATTERY_SCALE,
        boost: engine::sim_ee60(mem, car + CAR_ENGINE),
        engine_runs: engine::rpm_above_40(mem, car + CAR_ENGINE) != 0,
        still: signals.0,
        problem: signals.1,
        splits: keys.iter().filter(|k| k.tag == TAG_SPLIT).map(|k| k.distance).collect(),
        callouts: callouts.iter().map(|c| c.key.distance).collect(),
        wrong_way,
        false_starts,
        clock,
        stack,
    }
}

/// The driveline's point at the arc length `distance`: the `.trk` records' positions, interpolated between the two
/// records around it by their arc lengths. Ours (the 3D pace notes' placement; the game's 0x574440 is not ported).
pub fn point_at(track: &[DriveLineRecord], distance: f32) -> Option<[f32; 3]> {
    let next = track.iter().position(|r| r.value >= distance)?;
    let to = &track[next];
    let from = &track[next.saturating_sub(1)];
    let span = to.value - from.value;
    let t = if span > 0.0 { (distance - from.value) / span } else { 0.0 };
    let (a, b) = (from.vectors[0], to.vectors[0]);
    Some([a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t])
}

/// The step's pace-note callouts, in emit order.
pub fn pace_callouts(events: &[MarkerEvent]) -> Vec<&PaceCallout> {
    events
        .iter()
        .filter_map(|event| match event {
            MarkerEvent::PaceCallout(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// The step's pace-note callouts with what the co-driver's handler looks up besides: `Game` leaves the payload's
/// `seek_d` at 0, so it is `E12_SeekD` at the callout's x here, as the original's payload has it.
pub fn callouts(events: &[MarkerEvent], markers: &MarkerTicker) -> Vec<Callout> {
    pace_callouts(events)
        .into_iter()
        .map(|c| {
            let index = markers.index_of(c.key);
            let seek_d = if c.seek_d == 0.0 { markers.seek_d(c.x) } else { c.seek_d };
            Callout {
                tag: c.key.tag,
                word1: c.key.word1 & !FIRED,
                seek_d,
                gap: index.map_or(-1.0, |i| markers.key_gap(i)),
                next_tag: index.and_then(|i| markers.next_key(i)).map_or(-1, |k| markers.keys()[k].tag as i32),
            }
        })
        .collect()
}

/// The bodies of `CPhysics::CState`'s +0x450 list, in list order: the already-stepped awake solids.
pub fn awake_nodes(mem: &Mem) -> Vec<u32> {
    let physics = physics_layout::physics_global(mem);
    if physics == 0 {
        return Vec::new();
    }
    let state = physics_layout::state_of(mem, physics);
    if state == 0 {
        return Vec::new();
    }
    let list = physics_layout::entity_list(state, false, true, false);
    let mut node = mem.u32(list);
    let count = mem.u32(list + 8).min(100_000);
    let mut nodes = Vec::new();
    for _ in 0..count {
        if node == 0 {
            break;
        }
        nodes.push(node);
        node = mem.u32(node + 0x10);
    }
    nodes
}

/// Reads the already-stepped awake solids from `CPhysics::CState`'s +0x450 list. `CSolid::Vf28`
/// sends the same key, position (+0x140) and orientation (+0x100) in `CSigSolidGeneralState`.
pub fn active_interactive_poses(mem: &Mem, handle_end: u32) -> Vec<ActiveInteractivePose> {
    awake_nodes(mem)
        .into_iter()
        .filter_map(|node| {
            let body = Body::load(mem, node);
            (body.is_active != 0 && (INTERACTIVE_HANDLE_BASE..handle_end).contains(&body.state_owner)).then_some(
                ActiveInteractivePose {
                    handle: body.state_owner,
                    position: [body.position.x, body.position.y, body.position.z],
                    orientation: [body.orientation.x, body.orientation.y, body.orientation.z, body.orientation.w],
                },
            )
        })
        .collect()
}

/// The detached car parts that are awake (`rbr_recording::detach`): the awake bodies whose animation object is a
/// detachable part's handle (`detach`, from the boot) and whose parent is cleared.
pub fn detached_parts(mem: &Mem, detach: &[(u32, u32)]) -> Vec<u8> {
    let poses: Vec<PartPose> = awake_nodes(mem)
        .into_iter()
        .filter_map(|node| {
            let body = Body::load(mem, node);
            let &(_, part) = detach.iter().find(|(handle, _)| *handle == body.state_owner)?;
            (body.is_active != 0 && mem.u32(node + BODY_PARENT) == 0).then_some(PartPose {
                part,
                position: [body.position.x, body.position.y, body.position.z],
                orientation: [body.orientation.x, body.orientation.y, body.orientation.z, body.orientation.w],
            })
        })
        .collect();
    detach::to_bytes(&poses)
}

/// The tyre payloads of a frame of several steps: each wheel's last, in emit order. The viewer's emitters take one
/// frame's payloads as one frame of wall time (`rbr_render::particles`), so a frame sends a wheel once.
pub fn last_per_wheel(payloads: Vec<[u8; PAYLOAD_BYTES]>) -> Vec<[u8; PAYLOAD_BYTES]> {
    let wheel = |p: &[u8; PAYLOAD_BYTES]| u32::from_le_bytes(p[..4].try_into().unwrap());
    (0..payloads.len())
        .filter(|&i| payloads[i + 1..].iter().all(|q| wheel(q) != wheel(&payloads[i])))
        .map(|i| payloads[i])
        .collect()
}

/// The race clock as the HUD record carries it.
pub fn race_hud(clock: RaceClock, penalties: f32) -> RaceHud {
    RaceHud {
        time: clock.time,
        countdown: clock.countdown,
        penalties,
        running: clock.running,
        jump_start: clock.jumped,
    }
}

//! Reading the live process's answers: an FRM1 frame and the records after it, each skipped by its length if unknown.

use super::Hud;
use crate::stage::InteractivePose;
use rbr_recording::camera::{self, Camera};
use rbr_recording::clock::{StepClock, CLK1};
use rbr_recording::damage::Damage;
use rbr_recording::dash::{Dash, DSH1};
use rbr_recording::detach::{self, PartPose};
use rbr_recording::flow::{Flow, FLW1};
use rbr_recording::gearbox::{Gearbox, GBX1};
use rbr_recording::input::Payload;
use rbr_recording::live as records;
use rbr_recording::menu::{Menu, MNU1};
use rbr_recording::pacenote::{self, Callout, PCN1};
use rbr_recording::particles::{self, Event as ParticleEvent, TyreParticles};
use rbr_recording::realtime::{Step, STEP_BYTES, STP1};
use rbr_recording::shift::{ShiftPoints, SHF1};
use rbr_recording::sound::Inputs;
use rbr_recording::splits::{Splits, SPL1};
use rbr_recording::{ffb, hud};
use rbr_recording::{CONTROL_BYTES, HEADER_BYTES, MAGIC, WHEELS, WHEEL_BYTES};
use std::io::Read;

/// One answer of the live process: the FRM1 frame, and the HUD and damage records after it, the tagged records
/// (`rbr_recording::live`) the viewer knows, and the SCN1 scenery poses that end them.
#[derive(Debug, PartialEq)]
pub(super) struct LiveFrame {
    pub(super) frame: Vec<u8>,
    pub(super) hud: Hud,
    pub(super) damage: Damage,
    /// PTC1, empty without one.
    pub(super) tyres: Vec<TyreParticles>,
    /// PEV1, empty without one.
    pub(super) events: Vec<ParticleEvent>,
    /// DET1, empty without one.
    pub(super) detached: Vec<PartPose>,
    /// SND1 with PCN1 and the HUD's clock.
    pub(super) sound: Option<Inputs>,
    /// SPL1 (`harness/live.py` sends none).
    pub(super) splits: Option<Splits>,
    /// PCN1's callouts and DSH1, for the HUD.
    pub(super) pace: Vec<Callout>,
    pub(super) dash: Option<Dash>,
    /// FLW1, the step's race-flow events.
    pub(super) flow: Option<Flow>,
    /// CLK1, the steps' game time.
    pub(super) clock: Option<StepClock>,
    /// MNU1, where the race stands for the pause menu.
    pub(super) menu: Option<Menu>,
    /// CTL1, the car's control the frame's last step took.
    pub(super) control: Option<[u8; CONTROL_BYTES]>,
    pub(super) poses: Vec<InteractivePose>,
}

/// One FRM1 frame and the HUD and damage records after it, the tagged records (`rbr_recording::live`; the viewer takes
/// PTC1, PEV1, DET1, SND1, PCN1, SPL1, DSH1, CAM1, FLW1, FFB1, CLK1, GBX1, SHF1, MNU1 and CTL1 and skips the others)
/// and the SCN1 scenery poses that end them, or `None` at the end of the stream.
/// The payload periods of a frame that came back, from the payload it echoes (1 for one it cannot read).
pub(super) fn frame_periods(frame: &[u8]) -> u32 {
    frame
        .get(HEADER_BYTES..HEADER_BYTES + CONTROL_BYTES)
        .and_then(|p| Payload::read(p.try_into().ok()?).ok())
        .map_or(1, |p| p.periods())
}

#[cfg(test)]
pub(super) fn read_live_frame(reader: &mut impl Read) -> Result<Option<LiveFrame>, String> {
    match read_answer(reader)? {
        Some(Answer::Frame(frame)) => Ok(Some(*frame)),
        Some(Answer::Step(_)) => Err("the live process sent a step record outside the real-time protocol".to_owned()),
        None => Ok(None),
    }
}

/// What the live process sends: an answer, or under the real-time protocol a step record
/// (`rbr_recording::realtime`) between them.
pub(super) enum Answer {
    Frame(Box<LiveFrame>),
    Step(Step),
}

/// One answer or step record, or `None` at the end of the stream.
pub(super) fn read_answer(reader: &mut impl Read) -> Result<Option<Answer>, String> {
    let mut tag = [0; 4];
    match reader.read_exact(&mut tag) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.to_string()),
    }
    if tag == STP1 {
        let mut body = [0; STEP_BYTES];
        reader.read_exact(&mut body).map_err(|e| format!("the live process's step record: {e}"))?;
        return Ok(Some(Answer::Step(Step::from_body(&body))));
    }
    let frame = read_frame_after(tag, reader)?;
    read_frame_records(frame, reader).map(|frame| Some(Answer::Frame(Box::new(frame))))
}

/// The records after `frame`, the FRM1 frame of an answer.
fn read_frame_records(frame: Vec<u8>, reader: &mut impl Read) -> Result<LiveFrame, String> {
    let record = hud::Hud::read(reader).map_err(|e| format!("the live process's HUD record: {e}"))?;
    let mut hud = Hud {
        gear: record.gear as i32,
        engine_speed: record.engine_speed,
        speed: record.speed,
        race: record.race,
        ..Hud::default()
    };
    let damage = Damage::read(reader).map_err(|e| format!("the live process's damage record: {e}"))?;
    let records = records::read_records(reader).map_err(|e| format!("the live process's records: {e}"))?;
    let sound = Inputs::from_records(&records, &record);
    let (mut tyres, mut events, mut detached, mut splits) = (Vec::new(), Vec::new(), Vec::new(), None);
    let (mut pace, mut dash, mut flow, mut clock, mut menu, mut control) = (Vec::new(), None, None, None, None, None);
    for (tag, payload) in &records {
        match *tag {
            particles::PTC1 => {
                tyres = particles::read_body(payload).map_err(|e| format!("the live process's tyre record: {e}"))?
            }
            particles::PEV1 => {
                events =
                    particles::read_events(payload).map_err(|e| format!("the live process's particle events: {e}"))?
            }
            detach::DET1 => {
                detached = detach::parse(payload).map_err(|e| format!("the live process's detached parts: {e}"))?
            }
            ffb::FFB1 => {
                let steering = ffb::Steering::from_payload(payload)
                    .map_err(|e| format!("the live process's force record: {e}"))?;
                (hud.force, hud.ngp) = (Some(steering.force), steering.ngp);
            }
            SPL1 => {
                splits =
                    Some(Splits::from_payload(payload).map_err(|e| format!("the live process's split record: {e}"))?)
            }
            PCN1 => pace = pacenote::from_bytes(payload).ok_or("the live process's pace-note record")?,
            DSH1 => dash = Some(Dash::from_bytes(payload).ok_or("the live process's dash record")?),
            camera::CAM1 => {
                hud.camera =
                    Some(Camera::read_body(payload).map_err(|e| format!("the live process's camera record: {e}"))?)
            }
            GBX1 => {
                hud.gearbox = Some(
                    Gearbox::from_payload(payload).map_err(|e| format!("the live process's gearbox record: {e}"))?,
                )
            }
            SHF1 => {
                hud.shift = Some(
                    ShiftPoints::from_payload(payload)
                        .map_err(|e| format!("the live process's shift-point record: {e}"))?,
                )
            }
            FLW1 => flow = Some(Flow::from_payload(payload).ok_or("the live process's flow record")?),
            CLK1 => {
                clock = Some(StepClock::read_body(payload).map_err(|e| format!("the live process's step clock: {e}"))?)
            }
            MNU1 => menu = Some(Menu::read_body(payload).map_err(|e| format!("the live process's menu record: {e}"))?),
            records::CTL1 => {
                control = Some(payload.as_slice().try_into().map_err(|_| "the live process's control record")?)
            }
            _ => {}
        }
    }
    hud.volts = dash.as_ref().map(|d| d.battery);
    let mut count = [0; 4];
    reader.read_exact(&mut count).map_err(|e| format!("the live process's scenery count: {e}"))?;
    let count = u32::from_le_bytes(count) as usize;
    if count > 100_000 {
        return Err(format!("the live process sent {count} scenery poses"));
    }
    let mut poses = Vec::with_capacity(count);
    for _ in 0..count {
        let mut bytes = [0; 32];
        reader.read_exact(&mut bytes).map_err(|e| format!("the live process's scenery pose: {e}"))?;
        let word = |i: usize| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
        let float = |i: usize| f32::from_bits(word(i));
        poses.push(InteractivePose {
            handle: word(0),
            position: [float(1), float(2), float(3)],
            orientation: [float(4), float(5), float(6), float(7)],
        });
    }
    Ok(LiveFrame {
        frame,
        hud,
        damage,
        tyres,
        events,
        detached,
        sound,
        splits,
        pace,
        dash,
        flow,
        clock,
        menu,
        control,
        poses,
    })
}

/// One FRM1 frame, or `None` at the end of the stream.
#[cfg(test)]
pub(super) fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, String> {
    let mut magic = [0; 4];
    match reader.read_exact(&mut magic) {
        Ok(()) => read_frame_after(magic, reader).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// One FRM1 frame whose first four bytes were `magic`.
fn read_frame_after(magic: [u8; 4], reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut header = [0u8; HEADER_BYTES];
    header[..4].copy_from_slice(&magic);
    reader.read_exact(&mut header[4..]).map_err(|e| e.to_string())?;
    let word = |i: usize| u32::from_le_bytes(header[4 * i..4 * i + 4].try_into().unwrap());
    if word(0) != MAGIC {
        return Err(format!("the live process sent {:#010x}, not a frame", word(0)));
    }
    let mut frame = header.to_vec();
    frame.resize(HEADER_BYTES + CONTROL_BYTES + word(4) as usize + WHEELS * WHEEL_BYTES, 0);
    reader.read_exact(&mut frame[HEADER_BYTES..]).map_err(|e| e.to_string())?;
    Ok(frame)
}

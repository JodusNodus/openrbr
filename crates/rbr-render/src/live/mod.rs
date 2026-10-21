//! Live mode (`viewer --live <command>`): the car driven by the keyboard, its physics stepped by another process.
//! The command (`harness/live.py`, the original's step in an emulator with the Rust ports bound) reads one 32-byte
//! control payload per physics step on its stdin and answers each with a recording frame (`rbr_recording`, FRM1) on
//! its stdout, which the viewer draws through [`crate::playback`].
//!
//! The payload is the device input at full precision (`rbr_recording::input`): throttle, brake, handbrake, clutch and
//! steering as the keys and the gamepad give them (steering positive to the right), the action flags, the filter's
//! cell per channel (a key steers through the digital button's cell, a gamepad axis through the axis cell), the gear
//! and a version word. The stepping process puts it through the game's own input layer: the filter
//! (`PC_InputFilter.ini`'s rise and fall rates and speed fade), then under 1.02 the quantizer's 1/255 steps, under NGP
//! the unquantized values, as the original's do. `harness/live.py`, which takes the recordings' car-control payload,
//! refuses it. Payloads are sent at most as fast as
//! real time, one per game step of [`rbr_recording::STEP_SECONDS`] ([`STEPS_PER_SECOND`]), and at most [`IN_FLIGHT`]
//! ahead of the frames that came back.
//!
//! Each payload is one game frame of 7 ms of wall time. The game's frame (`rbr_gameplay::tick`) splits its wall time
//! into whole physics steps and ticks every clock (the countdown, the camera's start orbit, the HUD's race clock) by
//! those steps' time, so a frame shorter than a step moves nothing and a longer one is several steps. Under 1.02 a
//! frame is one 7 ms step: the game running at a frame per step. Under NGP (`rbr-standalone --ngp-physics`, 1/720 s
//! steps) it is 5 or 6 steps, and the frame header's substeps say how many; the step clock record (CLK1,
//! [`Live::clock`]) gives what a step is, for the drive time ([`crate::playback::Drive::parse_with`]). Either way the
//! car, the countdown, the go and the race clock run at real speed, and the presentation models take one answer as
//! 7 ms. When the viewer falls behind, the periods it can no longer send one by one ([`IN_FLIGHT`]) go as one longer
//! frame (`Input::frames`), as the game's frame clock takes the whole time since its last frame: its clocks sum the
//! frame's steps once. That answer's sound records, HUD tick and particle frame (`Playback::periods`) stand for all its
//! periods. It is never more than [`MAX_BEHIND`] behind: the game cuts a frame's wall time to 0.1 s, so after a
//! stall (a dragged window, a step slower than real time) it goes on from where it was instead of running fast to
//! catch up. Real time starts at the second drawn frame's pump, so the viewer's load after the first answer and its
//! first frame are neither played nor counted as not played.
//!
//! A command with `--realtime` (`rbr-standalone --live --realtime`, `rbr_recording::realtime`) runs the game on its
//! own clock instead, a frame per physics step: the viewer sends its input when it changes ([`Sender`], from any
//! thread), the pump only takes the answers (one per 7 ms of game time, as above), and a step record after every
//! frame gives the newest force and car control ([`Live::on_step`], [`Live::newest_step`]) without waiting for a
//! drawn frame. The clock starts at the second pump and stops while paused.
//!

use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

pub use crate::stage::InteractivePose;
use rbr_recording::camera::Camera;
use rbr_recording::clock::StepClock;
pub use rbr_recording::damage::Damage;
use rbr_recording::detach::PartPose;
pub use rbr_recording::gearbox::{Gearbox, GBX1};
pub use rbr_recording::hud::RaceHud;
use rbr_recording::input::{Input, CELL_AXIS, CELL_BUTTON};
pub use rbr_recording::menu::Menu;
use rbr_recording::particles::{Event as ParticleEvent, TyreParticles};
pub use rbr_recording::realtime::Step;
use rbr_recording::realtime::{Message, INPUT, PAUSE, RESUME};
use rbr_recording::shift::ShiftPoints;
pub use rbr_recording::sound::{Inputs, Sound};
use rbr_recording::splits::Splits;

use crate::hud::model::HudModel;
use rbr_recording::{CONTROL_BYTES, HEADER_BYTES};

mod pace;
mod read;

pub use pace::{Pace, PaceStats};
use read::{frame_periods, read_answer, Answer, LiveFrame};

/// Steps sent and not answered yet, at most. A pump sends at most this many, so it bounds the steps per drawn frame:
/// 4 keeps up with [`STEPS_PER_SECOND`] down to 36 frames a second (a 60 Hz display needs 2.4).
pub const IN_FLIGHT: u64 = 4;
/// Steps of tyre-particle payloads kept for a viewer that does not take them.
const KEPT_STEPS: usize = 200;
/// How long a pump waits for the answers to its payloads, at most: a frame's answer takes about 1.4 ms on the test
/// machine (5 or 6 NGP steps, p99 6.5 ms), so a slower one is drawn a frame later instead of holding the frame up.
const ANSWER_WAIT: std::time::Duration = std::time::Duration::from_millis(6);
/// Payloads whose times are kept ([`Live::stamps`]).
const KEPT_STAMPS: usize = 64;
/// Payloads (game frames of 7 ms, 1.02's `MaxTimeStep`) per second of real time, about 142.9.
pub const STEPS_PER_SECOND: f64 = 1.0 / rbr_recording::STEP_SECONDS as f64;
/// Seconds of wall time the payloads may lag real time, at most: `CPC_Application::Vf0c` (0x44a3d0) cuts a frame's gap
/// to 0.1 s (0x795e30), and the time past it is not played.
pub const MAX_BEHIND: f64 = 0.1;
/// How long a closed live process may take to end on its own (it writes the rally's files at the end of its input)
/// before it is killed.
const EXIT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
/// Flag bits of the payload's word 5.
pub const GEAR_UP: u32 = 1;
pub const GEAR_DOWN: u32 = 2;
/// Selects `Controls::gear` (`CDrive::M6840`, 0x506840).
pub const SELECT_GEAR: u32 = 4;
/// The call for help: flag 0x20 of the input's action bits (the quantized byte 0's, which the car's payload carries
/// as 0x10).
pub const CALL_FOR_HELP: u32 = rbr_gameplay::control::CALL_FOR_HELP as u32;
/// Accepts the race's message box (`rbr_recording::input::ACCEPT_MESSAGE`).
pub const ACCEPT_MESSAGE: u32 = rbr_recording::input::ACCEPT_MESSAGE as u32;
/// The pause menu's confirmed retire (`rbr_recording::input::RETIRE`).
pub const RETIRE: u32 = rbr_recording::input::RETIRE as u32;

/// One step's controls.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controls {
    pub brake: f32,
    pub throttle: f32,
    pub handbrake: f32,
    pub clutch: f32,
    /// -1 (left) to 1 (right).
    pub steering: f32,
    pub flags: u32,
    /// The gear [`SELECT_GEAR`] selects: 0 reverse, 1 neutral, 2 first.
    pub gear: u8,
    /// The steering is the keys' (the game filters it through its digital button cell), not an axis.
    pub keys_steer: bool,
    /// The ignition is held ([`crate::bindings::Action::Ignition`]): the payload's aux bit.
    pub ignition: bool,
    /// The direct gear actions held and bound, bit `k` for reverse, neutral, first to sixth
    /// (`rbr_recording::input::Input::gears_held`).
    pub gears_held: u8,
    pub gears_bound: u8,
    /// The filter cell per channel when the poll chose them ([`crate::rsf_input`]); else the steering's by
    /// `keys_steer` and the axis cell for the rest.
    pub cells: Option<[u8; 5]>,
}

impl Controls {
    /// The 32-byte input payload (`rbr_recording::input`) of one payload period.
    pub fn payload(&self) -> [u8; CONTROL_BYTES] {
        self.frame_payload(1)
    }

    /// The input payload of a frame of `periods` payload periods.
    pub fn frame_payload(&self, periods: u8) -> [u8; CONTROL_BYTES] {
        let steering = if self.keys_steer { CELL_BUTTON } else { CELL_AXIS };
        Input {
            throttle: self.throttle,
            brake: self.brake,
            handbrake: self.handbrake,
            clutch: self.clutch,
            steering: self.steering,
            flags: self.flags as u8,
            cells: self.cells.unwrap_or([steering, CELL_AXIS, CELL_AXIS, CELL_AXIS, CELL_AXIS]),
            aux: self.ignition,
            gear: self.gear,
            // One period goes as 0, the payload of a frame per period.
            frames: if periods == 1 { 0 } else { periods },
            gears_held: self.gears_held,
            gears_bound: self.gears_bound,
        }
        .to_bytes()
    }
}

/// When a payload was written to the process, when the reader thread had its answer whole and when a pump took it,
/// and the answer's physics steps and steering force (the viewer's `RBR_LATENCY`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stamps {
    pub sent: Instant,
    pub received: Option<Instant>,
    pub taken: Option<Instant>,
    pub substeps: u32,
    pub force: Option<f32>,
}

/// What the live process reports after each frame (its HUD record, `rbr_recording::hud`): the gear, the engine speed
/// and the car speed, and from `rbr-standalone` the race clock (HUD2) and the steering force (FFB1,
/// `rbr_recording::ffb`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hud {
    /// The drive's gear index: 0 reverse, 1 neutral, 2 first gear, and so on.
    pub gear: i32,
    /// rad/s.
    pub engine_speed: f32,
    /// m/s.
    pub speed: f32,
    /// The force the game sends the wheel, [-1, 1] before the player's settings (`rbr_gameplay::ffb`).
    pub force: Option<f32>,
    /// The physics is NGP's (its rounding and dead zone apply to the force).
    pub ngp: bool,
    pub race: Option<RaceHud>,
    /// The camera's mode and start orbit (`rbr-standalone`'s CAM1).
    pub camera: Option<Camera>,
    /// The gear ratios and the drive speed (`rbr-standalone`'s GBX1), for the H-shifter's rev-matching.
    pub gearbox: Option<Gearbox>,
    /// The car's shift points (SHF1) and its battery's voltage (DSH1), for the working dials.
    pub shift: Option<ShiftPoints>,
    pub volts: Option<f32>,
}

impl Hud {
    /// The gear as the dashboard shows it: R, N, 1, 2, ...
    pub fn gear_label(&self) -> String {
        match self.gear {
            0 => "R".to_owned(),
            1 => "N".to_owned(),
            g => (g - 1).to_string(),
        }
    }

    pub fn rpm(&self) -> f32 {
        self.engine_speed * RAD_PER_SECOND_TO_RPM
    }

    pub fn kmh(&self) -> f32 {
        self.speed * 3.6
    }
}

/// rad/s to rpm (the VCU multiplies by 9.5492964).
const RAD_PER_SECOND_TO_RPM: f32 = 60.0 / std::f32::consts::TAU;

/// The stepping process and the frames it sent back.
pub struct Live {
    command: Vec<String>,
    child: Child,
    /// `None` once closed ([`Drop`]); shared with the real-time protocol's [`Sender`]s.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// The command runs the real-time protocol (it has `--realtime`).
    realtime: bool,
    /// Its step records, as the reader thread has them.
    steps: Arc<Mutex<Steps>>,
    /// Each answer with when the reader thread had it whole.
    frames: Receiver<(Instant, Result<LiveFrame, String>)>,
    /// The wall time the payloads are paced to.
    pace: Pace,
    /// Pumps made ([`Live::pump`]), payload periods sent, payloads sent, and answers received.
    pumps: u64,
    sent: u64,
    posted: u64,
    received: u64,
    /// The pacing's figures, printed when the process closes.
    pub stats: PaceStats,
    /// The last [`KEPT_STAMPS`] payloads' times, by payload number ([`Live::stamps`]).
    stamps: VecDeque<(u64, Stamps)>,
    /// The HUD record of the last frame that came back.
    pub hud: Hud,
    /// The damage record of the last frame that came back. Only the last of several frames in one
    /// [`Live::pump`] is kept, so an impact followed by another within one pump loses its event.
    pub damage: Damage,
    /// The detached car parts of the last frame that came back (`DET1`): the parts whose bodies are awake.
    pub detached: Vec<PartPose>,
    /// The split record of the last frame that came back, if the process sent one.
    pub splits: Option<Splits>,
    /// The HUD's state, fed with each frame that comes back (`rbr_render::hud::model`).
    pub hud_model: HudModel,
    /// Latest pose by handle. Handles absent from the awake list keep their last pose after sleeping.
    pub interactive_poses: HashMap<u32, InteractivePose>,
    /// The tyre-particle payloads of each step received since the viewer last took them ([`Live::take_tyres`]).
    tyres: Vec<Vec<TyreParticles>>,
    /// The particle events of the same steps ([`Live::take_tyres`]).
    events: Vec<Vec<ParticleEvent>>,
    /// The sound records of the frames that came back, in step order, until [`Live::take_sounds`].
    sounds: Vec<Inputs>,
    /// The race flow called the rescue helicopter, until [`Live::take_rescue_call`].
    rescue_called: bool,
    /// What a step of the frames' counters is (CLK1; 1.02's from a process that sends none). Parse the frames with it
    /// (`Drive::parse_with`).
    pub clock: StepClock,
    /// The car's lamps of the last frame that came back (PEV1's lights event), if the process sends them.
    pub lamps: Option<crate::lights::Lamps>,
    /// Where the race stands for the pause menu (MNU1 of the last frame that came back; the quick rally's page
    /// without the call for help and the retire from a process that sends none).
    pub menu: Menu,
}

impl Live {
    /// Starts `command` (program and arguments).
    pub fn spawn(command: &[String]) -> Result<Live, String> {
        let (program, args) = command.split_first().ok_or("--live needs a command")?;
        let mut child = crate::console::no_window(&mut Command::new(program))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{program}: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (send, frames) = mpsc::channel();
        let steps = Arc::new(Mutex::new(Steps::default()));
        let reader_steps = Arc::clone(&steps);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let frame = match read_answer(&mut reader) {
                    Ok(Some(Answer::Step(step))) => {
                        reader_steps.lock().unwrap().take(step);
                        continue;
                    }
                    Ok(Some(Answer::Frame(frame))) => Ok(*frame),
                    Ok(None) => Err("the live process ended".to_owned()),
                    Err(e) => Err(e),
                };
                let last = frame.is_err();
                if send.send((Instant::now(), frame)).is_err() || last {
                    break;
                }
            }
        });
        Ok(Live {
            command: command.to_vec(),
            child,
            stdin: Arc::new(Mutex::new(Some(stdin))),
            realtime: command.iter().any(|a| a == "--realtime"),
            steps,
            frames,
            pace: Pace::new(Instant::now()),
            pumps: 0,
            sent: 0,
            posted: 0,
            received: 0,
            stats: PaceStats::default(),
            stamps: VecDeque::new(),
            hud: Hud::default(),
            damage: Damage::default(),
            detached: Vec::new(),
            splits: None,
            hud_model: HudModel::default(),
            interactive_poses: HashMap::new(),
            tyres: Vec::new(),
            events: Vec::new(),
            sounds: Vec::new(),
            rescue_called: false,
            clock: StepClock::default(),
            lamps: None,
            menu: Menu::default(),
        })
    }

    /// Steps with `controls` as far as real time and [`IN_FLIGHT`] allow, and returns the frames that came back.
    /// The flags go with the first step sent, and are cleared then.
    /// The frames that came back are taken before the sending too, so that they do not hold up this pump's steps.
    /// Then it waits up to [`ANSWER_WAIT`] for the answers to every payload sent, so that the frame drawn after it
    /// shows the controls it read, as the game draws the step it just ran (an answer later than that is the next
    /// pump's).
    /// A real-time process takes no payloads: the pump starts its clock at the second pump and takes the answers.
    pub fn pump(&mut self, controls: &mut Controls) -> Result<Vec<Vec<u8>>, String> {
        let mut out = self.received_frames()?;
        let now = Instant::now();
        if self.realtime {
            self.pumps += 1;
            if self.pumps == 2 {
                self.sender().message(RESUME, 0, &Controls::default())?;
            }
            if self.pumps >= 2 {
                self.stats.pump(now, 0);
            }
            return Ok(out);
        }
        // Real time and the figures start at the second pump: the viewer's load after the first answer and its first
        // drawn frame (the first uploads, 0.1-0.4 s on the test machine) are not the drive's time, so none of it is due (or
        // counted as not played).
        self.pumps += 1;
        if self.pumps <= 2 {
            self.pace = Pace::new(now);
        }
        let (due, behind) = self.pace.due(now, self.sent);
        if self.pumps >= 2 {
            self.stats.pump(now, behind);
        }
        while self.sent < due && self.posted - self.received < IN_FLIGHT {
            // The last payload in flight takes every period still due.
            let last = self.posted - self.received + 1 == IN_FLIGHT;
            let periods = if last { (due - self.sent).min(u64::from(u8::MAX)) } else { 1 };
            self.send(&controls.frame_payload(periods as u8), periods)?;
            controls.flags = 0;
        }
        let until = Instant::now() + ANSWER_WAIT;
        while self.received < self.posted {
            match self.frames.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok((at, frame)) => out.push(self.keep(frame?, at)),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return Err("the live process ended".to_owned()),
            }
        }
        Ok(out)
    }

    /// Writes one payload of `periods` payload periods to the process.
    fn send(&mut self, payload: &[u8; CONTROL_BYTES], periods: u64) -> Result<(), String> {
        if self.realtime {
            return Err("a real-time live process takes its input from a Sender".to_owned());
        }
        let mut stdin = self.stdin.lock().unwrap();
        let stdin = stdin.as_mut().ok_or("the live process is closed")?;
        stdin.write_all(payload).and_then(|()| stdin.flush()).map_err(|e| format!("the live process: {e}"))?;
        if self.stamps.len() >= KEPT_STAMPS {
            self.stamps.pop_front();
        }
        self.stamps.push_back((
            self.posted,
            Stamps { sent: Instant::now(), received: None, taken: None, substeps: 0, force: None },
        ));
        self.sent += periods;
        self.posted += 1;
        self.stats.sent(periods);
        Ok(())
    }

    /// The frames that came back and were not taken yet.
    fn received_frames(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::new();
        while let Ok((at, frame)) = self.frames.try_recv() {
            out.push(self.keep(frame?, at));
        }
        Ok(out)
    }

    /// Waits for the frame of the first step (the snapshot's own call; its controls are not used).
    pub fn first_frame(&mut self) -> Result<Vec<u8>, String> {
        if self.realtime {
            self.sender().input(0, &Controls::default())?;
        } else {
            self.send(&Controls::default().payload(), 1)?;
        }
        let (at, frame) = self.frames.recv().map_err(|_| "the live process ended".to_owned())?;
        Ok(self.keep(frame?, at))
    }

    /// Sends one recorded control payload and waits for its frame. Unlike [`Live::pump`], this is not paced to real
    /// time; it is for deterministic recorded-control replays through the same live process and viewer data channel.
    pub fn step_payload(&mut self, payload: &[u8; CONTROL_BYTES]) -> Result<Vec<u8>, String> {
        self.send(payload, 1)?;
        let (at, frame) = self.frames.recv().map_err(|_| "the live process ended".to_owned())?;
        Ok(self.keep(frame?, at))
    }

    /// The tyre-particle payloads and particle events of the steps received since the last call, one list each per
    /// step in order (empty lists from a process that sends no PTC1 or PEV1 record).
    pub fn take_tyres(&mut self) -> Vec<(Vec<TyreParticles>, Vec<ParticleEvent>)> {
        std::mem::take(&mut self.tyres).into_iter().zip(std::mem::take(&mut self.events)).collect()
    }

    /// Whether the race flow called the rescue helicopter since the last call (FLW1).
    pub fn take_rescue_call(&mut self) -> bool {
        std::mem::take(&mut self.rescue_called)
    }

    /// The sound records since the last call, in step order.
    pub fn take_sounds(&mut self) -> Vec<Inputs> {
        std::mem::take(&mut self.sounds)
    }

    /// Payloads written so far: the number the next one gets ([`Live::stamps`]).
    pub fn posted(&self) -> u64 {
        self.posted
    }

    /// Answers taken so far.
    pub fn answered(&self) -> u64 {
        self.received
    }

    /// When payload `number` (0 the first) was written, and its answer read and taken, if it is among the last
    /// [`KEPT_STAMPS`].
    pub fn stamps(&self, number: u64) -> Option<Stamps> {
        self.stamps.iter().find(|(n, _)| *n == number).map(|&(_, stamps)| stamps)
    }

    /// Takes in one answer, which the reader had whole at `at`: keeps its records (the tyre payloads of at most
    /// [`KEPT_STEPS`] steps, the oldest go) and returns its frame.
    fn keep(&mut self, answer: LiveFrame, at: Instant) -> Vec<u8> {
        let number = self.received;
        if let Some((_, stamps)) = self.stamps.iter_mut().find(|(n, _)| *n == number) {
            (stamps.received, stamps.taken) = (Some(at), Some(Instant::now()));
            stamps.substeps = u32::from_le_bytes(answer.frame[12..16].try_into().expect("a frame header"));
            stamps.force = answer.hud.force;
        }
        if self.tyres.len() >= KEPT_STEPS {
            self.tyres.remove(0);
            self.events.remove(0);
        }
        if let Some(lamps) = answer.events.iter().rev().find_map(|event| match *event {
            ParticleEvent::Lights { switches, master, lamps } => Some(crate::lights::Lamps { switches, master, lamps }),
            _ => None,
        }) {
            self.lamps = Some(lamps);
        }
        let periods = frame_periods(&answer.frame);
        let mut frame = answer.frame;
        // The frame echoes the input payload; its five control floats become the car's control (CTL1), which the
        // recordings' FRM1 holds and the crew's wheel and hands follow (`CSigCarControl`): the filter's and, under
        // 1.02, the quantizer's values. The payload's flags, periods and magic stay.
        if let Some(control) = answer.control {
            frame[HEADER_BYTES..HEADER_BYTES + 20].copy_from_slice(&control[..20]);
        }
        self.tyres.push(answer.tyres);
        self.events.push(answer.events);
        // The sound models step a period at a time (ours: a longer frame's last records, again for each period).
        for _ in 0..periods {
            self.sounds.extend(answer.sound.iter().cloned());
        }
        self.hud = answer.hud;
        self.damage = answer.damage;
        self.detached = answer.detached;
        self.splits = answer.splits;
        self.rescue_called |= answer.flow.is_some_and(|f| f.helicopter_called);
        if let Some(clock) = answer.clock {
            self.clock = clock;
        }
        if let Some(menu) = answer.menu {
            self.menu = menu;
        }
        self.hud_model.answer_over(
            periods as f32 * rbr_recording::STEP_SECONDS,
            &answer.pace,
            answer.dash.as_ref(),
            Some(&answer.damage.zones),
            answer.hud.race.is_some_and(|r| r.jump_start),
            answer.flow.as_ref(),
        );
        self.interactive_poses.extend(answer.poses.into_iter().map(|pose| (pose.handle, pose)));
        self.received += 1;
        self.stats.answer(u32::from_le_bytes(frame[12..16].try_into().expect("a frame header")));
        frame
    }

    /// Pauses the drive (the pause menu): [`Live::pump`] is not called until [`Live::resume`], so the process gets no
    /// payload and its game, clocks and car stand still.
    pub fn pause(&mut self) {
        self.pace.pause(Instant::now());
        if self.realtime {
            if let Err(e) = self.sender().message(PAUSE, 0, &Controls::default()) {
                eprintln!("viewer: {e}");
            }
        }
    }

    /// Goes on after [`Live::pause`]: the paused wall time is not due, so the payloads go on at real time from where
    /// they stopped, with no catch-up.
    pub fn resume(&mut self) {
        let paused = self.pace.resume(Instant::now());
        self.stats.resume(paused);
        if self.realtime {
            if let Err(e) = self.sender().message(RESUME, 0, &Controls::default()) {
                eprintln!("viewer: {e}");
            }
        }
    }

    /// Whether the process runs the real-time protocol.
    pub fn realtime(&self) -> bool {
        self.realtime
    }

    /// The real-time process's input, for any thread.
    pub fn sender(&self) -> Sender {
        Sender(Arc::clone(&self.stdin))
    }

    /// Calls `f` on the reader thread with each step record as it comes (the wheel's force).
    pub fn on_step(&self, f: impl FnMut(&Step) + Send + 'static) {
        self.steps.lock().unwrap().on_step = Some(Box::new(f));
    }

    /// The newest step record.
    pub fn newest_step(&self) -> Option<Step> {
        self.steps.lock().unwrap().newest
    }

    /// Keeps every step record with when it came, for [`Live::take_steps`] (`RBR_LATENCY`).
    pub fn keep_steps(&self) {
        self.steps.lock().unwrap().kept.get_or_insert_with(Vec::new);
    }

    /// The step records kept since the last call.
    pub fn take_steps(&self) -> Vec<(Instant, Step)> {
        self.steps.lock().unwrap().kept.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// The same command started again, from its snapshot.
    pub fn restart(&self) -> Result<Live, String> {
        let mut live = Live::spawn(&self.command)?;
        live.hud_model = self.hud_model.fresh();
        Ok(live)
    }
}

/// A real-time process's step records: the newest, the callback for each and those kept.
#[derive(Default)]
struct Steps {
    newest: Option<Step>,
    on_step: Option<OnStep>,
    kept: Option<Vec<(Instant, Step)>>,
}

impl Steps {
    fn take(&mut self, step: Step) {
        if let Some(f) = &mut self.on_step {
            f(&step);
        }
        if let Some(kept) = self.kept.as_mut().filter(|k| k.len() < MOST_KEPT_STEPS) {
            kept.push((Instant::now(), step));
        }
        self.newest = Some(step);
    }
}

/// What [`Live::on_step`] calls.
type OnStep = Box<dyn FnMut(&Step) + Send>;

/// Step records kept between two [`Live::take_steps`], at most.
const MOST_KEPT_STEPS: usize = 1 << 20;

/// Writes a real-time process's messages (`rbr_recording::realtime`); clones share the process's input.
#[derive(Clone)]
pub struct Sender(Arc<Mutex<Option<ChildStdin>>>);

impl Sender {
    /// The newest input, numbered `seq`.
    pub fn input(&self, seq: u32, controls: &Controls) -> Result<(), String> {
        self.message(INPUT, seq, controls)
    }

    fn message(&self, tag: [u8; 4], seq: u32, controls: &Controls) -> Result<(), String> {
        let mut stdin = self.0.lock().unwrap();
        let stdin = stdin.as_mut().ok_or("the live process is closed")?;
        let message = Message { tag, seq, payload: controls.payload() };
        stdin.write_all(&message.to_bytes()).and_then(|()| stdin.flush()).map_err(|e| format!("the live process: {e}"))
    }
}

/// Closes the process's input, which ends it (`rbr-standalone` writes the rally's files then), and kills it only if
/// it has not ended after [`EXIT_WAIT`].
impl Drop for Live {
    fn drop(&mut self) {
        self.stdin.lock().unwrap().take();
        // The reader thread may outlive the process's end by a read; the callback (the wheel) goes now.
        self.steps.lock().unwrap().on_step = None;
        let closed = Instant::now();
        while matches!(self.child.try_wait(), Ok(None)) {
            if closed.elapsed() > EXIT_WAIT {
                eprintln!("viewer: the live process did not end; killed");
                let _ = self.child.kill();
                break;
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = self.child.wait();
        if let Some(line) = self.stats.summary(self.clock.step, self.pace.skipped) {
            eprintln!("viewer: {line}");
        }
    }
}

#[cfg(test)]
mod tests;

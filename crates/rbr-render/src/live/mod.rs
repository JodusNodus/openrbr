//! Live mode (`viewer --live <command>`): the car driven by the player's devices, its physics stepped by another
//! process, `rbr-standalone --live` (`rbr_recording::realtime`). The process runs the game on its own clock, a frame
//! per physics step, each with the newest input the viewer sent ([`Sender`], from any thread, when the input changes);
//! after every frame that stepped a step record gives the force and the car's control ([`Live::on_step`],
//! [`Live::newest_step`]) without waiting for a drawn frame, and every 7 ms of game time an answer (a recording frame,
//! FRM1, and its records) gives the picture, which the viewer draws through [`crate::playback`]. The pump only takes
//! the answers. The clock starts at the second pump, so the viewer's load after the first answer and its first frame
//! are not played, and stops while paused.
//!
//! The input is the device input at full precision (`rbr_recording::input`): throttle, brake, handbrake, clutch and
//! steering as the devices give them (steering positive to the right), the action flags, the filter's cell per channel
//! (a key steers through the digital button's cell, an axis through the axis cell) and the gear. The stepping process
//! puts it through the game's own input layer: the filter (`PC_InputFilter.ini`'s rise and fall rates and speed fade),
//! then under 1.02 the quantizer's 1/255 steps, under NGP the unquantized values, as the original's do.
//!
//! An answer stands for its 7 ms periods (the echoed payload's `Input::frames`; more than one after a stall), which the
//! presentation models step a period at a time; the step clock record (CLK1, [`Live::clock`]) gives what a step is, for
//! the drive time ([`crate::playback::Drive::parse_with`]).
//!
//! [`Live::lockstep`] runs the command with `--frames -` instead: no clock, each [`Live::step_payload`] is one frame,
//! answered before the next, for the tools that step a drive themselves (the captures, `--at`).

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
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
use rbr_recording::realtime::{Logged, Message, INPUT, LOG_MAGIC, PAUSE, RESUME};
use rbr_recording::shift::ShiftPoints;
pub use rbr_recording::sound::{Inputs, Sound};
use rbr_recording::splits::Splits;

use crate::hud::model::HudModel;
use rbr_recording::{CONTROL_BYTES, HEADER_BYTES};

mod read;
mod stats;

use read::{frame_periods, read_answer, Answer, LiveFrame};
pub use stats::LiveStats;

/// Steps of tyre-particle payloads kept for a viewer that does not take them.
const KEPT_STEPS: usize = 200;
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
    /// The filter cell per channel when the poll chose them ([`crate::devices::Input`]); else the steering's by
    /// `keys_steer` and the axis cell for the rest.
    pub cells: Option<[u8; 5]>,
}

impl Controls {
    /// The 32-byte input payload (`rbr_recording::input`).
    pub fn payload(&self) -> [u8; CONTROL_BYTES] {
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
            frames: 0,
            gears_held: self.gears_held,
            gears_bound: self.gears_bound,
        }
        .to_bytes()
    }
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
    pub race: Option<RaceHud>,
    /// The camera's mode and start orbit (`rbr-standalone`'s CAM1).
    pub camera: Option<Camera>,
    /// The gear ratios and the drive speed (`rbr-standalone`'s GBX1), for the H-shifter's rev-matching.
    pub gearbox: Option<Gearbox>,
    /// The car's shift points (SHF1) and its battery's voltage (DSH1), for the working dials.
    pub shift: Option<ShiftPoints>,
    pub volts: Option<f32>,
    /// The car's damage by group (`rbr-standalone`'s CDM1), for the HUD's car status.
    pub car: Option<crate::hud::model::CarStatus>,
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
    /// `None` once closed ([`Drop`]); shared with the [`Sender`]s.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// The process runs `--frames -` ([`Live::lockstep`]).
    lockstep: bool,
    /// Its step records, as the reader thread has them.
    steps: Arc<Mutex<Steps>>,
    /// Each answer with when the reader thread had it whole.
    frames: Receiver<(Instant, Result<LiveFrame, String>)>,
    /// Pumps made ([`Live::pump`]).
    pumps: u64,
    /// The drawn frames' and answers' figures, printed when the process closes.
    pub stats: LiveStats,
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
        Live::start(command, false)
    }

    /// Starts `command` with `--frames -`: each [`Live::step_payload`] is one frame, answered before the next.
    pub fn lockstep(command: &[String]) -> Result<Live, String> {
        let command: Vec<String> = command.iter().cloned().chain(["--frames".to_owned(), "-".to_owned()]).collect();
        let live = Live::start(&command, true)?;
        live.write(&LOG_MAGIC)?;
        Ok(live)
    }

    fn start(command: &[String], lockstep: bool) -> Result<Live, String> {
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
            lockstep,
            steps,
            frames,
            pumps: 0,
            stats: LiveStats::default(),
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

    /// The answers that came back: the clock starts at the second pump (the viewer's load after the first answer
    /// and its first drawn frame are not the drive's time).
    pub fn pump(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let out = self.received_frames()?;
        self.pumps += 1;
        if self.pumps == 2 && !self.lockstep {
            self.sender().message(RESUME, 0, &Controls::default())?;
        }
        if self.pumps >= 2 {
            self.stats.pump(Instant::now());
        }
        Ok(out)
    }

    /// Writes `bytes` to the process.
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let mut stdin = self.stdin.lock().unwrap();
        let stdin = stdin.as_mut().ok_or("the live process is closed")?;
        stdin.write_all(bytes).and_then(|()| stdin.flush()).map_err(|e| format!("the live process: {e}"))
    }

    /// The frames that came back and were not taken yet.
    fn received_frames(&mut self) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::new();
        while let Ok((_, frame)) = self.frames.try_recv() {
            out.push(self.keep(frame?));
        }
        Ok(out)
    }

    /// Waits for the frame of the first step (the snapshot's own call; its controls are not used).
    pub fn first_frame(&mut self) -> Result<Vec<u8>, String> {
        if self.lockstep {
            return self.step_payload(&Controls::default().payload());
        }
        self.sender().input(0, &Controls::default())?;
        self.next_frame()
    }

    /// Runs one frame of `payload` (an input or a recorded control payload) in a [`Live::lockstep`] process and
    /// waits for its answer.
    pub fn step_payload(&mut self, payload: &[u8; CONTROL_BYTES]) -> Result<Vec<u8>, String> {
        if !self.lockstep {
            return Err("step_payload needs a lockstep live process".to_owned());
        }
        self.write(&Logged::unchecked(*payload, 1).to_bytes())?;
        self.next_frame()
    }

    fn next_frame(&mut self) -> Result<Vec<u8>, String> {
        let (_, frame) = self.frames.recv().map_err(|_| "the live process ended".to_owned())?;
        Ok(self.keep(frame?))
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

    /// Takes in one answer: keeps its records (the tyre payloads of at most [`KEPT_STEPS`] steps, the oldest go)
    /// and returns its frame.
    fn keep(&mut self, answer: LiveFrame) -> Vec<u8> {
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
        self.stats.answer(u32::from_le_bytes(frame[12..16].try_into().expect("a frame header")));
        frame
    }

    /// The HUD's state: the model's, with the car's damage (CDM1), rev limit and upshift point for the gear (SHF1).
    pub fn hud_view(&self) -> crate::hud::model::HudView {
        let shift = self.hud.shift;
        crate::hud::model::HudView {
            car: self.hud.car,
            rev_limit: shift.map(|s| s.limit).filter(|r| *r > 0.0),
            upshift: shift.and_then(|s| s.up.get(self.hud.gear as usize).copied()).filter(|u| *u > 0.0),
            ..self.hud_model.view()
        }
    }

    /// Pauses the drive (the pause menu): the process's clock stops, so its game, clocks and car stand still.
    pub fn pause(&mut self) {
        self.stats.pause(Instant::now());
        if self.lockstep {
            return;
        }
        if let Err(e) = self.sender().message(PAUSE, 0, &Controls::default()) {
            eprintln!("viewer: {e}");
        }
    }

    /// Goes on after [`Live::pause`], from where the clock stopped, with no catch-up.
    pub fn resume(&mut self) {
        self.stats.resume(Instant::now());
        if self.lockstep {
            return;
        }
        if let Err(e) = self.sender().message(RESUME, 0, &Controls::default()) {
            eprintln!("viewer: {e}");
        }
    }

    /// The process's input, for any thread.
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

/// Writes the process's messages (`rbr_recording::realtime`); clones share the process's input.
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
        if let Some(line) = self.stats.summary(self.clock.step) {
            eprintln!("viewer: {line}");
        }
    }
}

#[cfg(test)]
mod tests;

//! The composed gameplay frame: `CPC_Application::Vf0c` (0x44a3d0),
//! `CApplication::M004801d0`/`Tic` (0x4801d0/0x4804f0),
//! `TicPhysicsRecording` (0x47f420), and the post-tick animation update
//! (`M004825b0`, 0x4825b0).
//!
//! The host owns device polling and the physics implementation. This value
//! owns the order between the already-ported pieces. `PhysicsStep::step`
//! returns the car sample observed by `CCar::Vf6cx` (0x4ea300); the timekeeper
//! records it before this module advances the elapsed-time clock, matching
//! `CTimeKeeper::Record` (0x4a0540) inside `StepForward` (0x5273a0).

use rbr_formats::markers::StageMarkers;

use crate::camera_go::StartCamera;
use crate::control::{CarControl, QuantizedControl, CALL_FOR_HELP};
use crate::driveline;
use crate::filter::{self, Filter, Filtered, Grid, Raw};
use crate::flow::{FinishZone, FlowEvents, FlowFrame, Mode, RaceFlow, StageEnd};
use crate::markers::{MarkerEvent, MarkerSettings, MarkerTicker, TickInput};
use crate::race_clock::{AssistSettings, ClockEvents, RaceClock};
use crate::result::{self, StageResult};
use crate::stage::{Stage, State};
use crate::tick::{self, Frame, FrameClock, SingleStep};
use crate::timing::{self, Checkpoint, TimeCard, TimeKeeper, CLOCK_START};

/// Samples the game receives from the input and race hosts for one frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameInput {
    /// One device poll (`USInputManagerImpl::Tic`, 0x4cbf50), delivered to
    /// `E16Filter::NotifyB` (0x48ab40) before `CApplication::Tic`.
    pub raw: Raw,
    /// State 5's race-host ready reply (`M004825b0`, 0x4825b0).
    pub race_host_ready: bool,
    /// The camera's `M0047ca40` event (0x47ca40), which arms state 10, from the host. A game with the camera's go
    /// timer ([`Game::with_camera_go`]) gives it itself.
    pub camera_go: bool,
    /// A replay entry fired after the local filter/Pack path. FRM1 stores this
    /// emitted payload, not the raw device input. One entry is used for each
    /// driving substep in this frame; a replay cursor for multi-step frames is
    /// host-owned and not modeled here.
    pub replay_control: Option<QuantizedControl>,
    /// The entry of the frame's later driving substeps, when the host gives one (`None`: [`Self::replay_control`]
    /// for each). A live frame of several steps (NGP's 1/720 s) gives the payload's one-shot flags, such as a gear
    /// change, to its first step only, as a recorded press is one step's.
    pub replay_tail: Option<QuantizedControl>,
    /// The player's call for help, the pause menu's HELPCALL (`E61_M0056e6d0`, 0x56e6d0). Its
    /// `CSigRespawnRequest(1)` reaches `E16Quant::NotifyRespawn` (0x48d080), which sets flag 0x20 in the replay's
    /// sticky flags; the next driving substep's control carries it once (payload flag 0x10, `CCar::UpdateSeed`'s
    /// motion probe and assist). Ignored when the menu would not offer it ([`Game::help_offered`]).
    pub help_call: bool,
    /// The player accepts the race's message box (`M0058a170`'s accept button): its callback ends the race
    /// ([`RaceFlow::acknowledge`]). Ignored unless a box is up.
    pub acknowledge: bool,
    /// The player's `CSigMenuActivate` (the menu's confirm): in the countdown of a championship it arms the countdown
    /// (`CApplication::M0047f9c0` case 10, [`Stage::confirm`]), where the camera gives no go. Ignored otherwise: while
    /// driving it opens the pause menu, which the host runs.
    pub confirm: bool,
    /// The player confirmed the pause menu's retire (`pause_menu::Item::Retire`): the race ends retired
    /// ([`RaceFlow::retire_from_menu`]) before the frame steps the car. Taken in the driving state only, the one the
    /// game opens its pause page in.
    pub retire: bool,
}

/// Values the car and marker host expose during a physics substep.
#[derive(Debug, Clone, Copy, Default)]
pub struct StepSample {
    /// `CCar` +0x17c, the parameter consumed by `E12_SampleX` (0x475ea0).
    pub driveline_parameter: f32,
    /// `CCar` +0x15f8, the scalar `E16Filter::NotifyA` reads from `p[0]`. `CCar::Vf6cx` writes `-MB560[1]` here.
    pub notify_a_speed: f32,
    /// `MB560` of the car's vector at +0x220, `NotifyA`'s `p[4..8]`, which `CCar::Vf28x` emits with the speed. `None`
    /// when the host cannot walk it (a recorded car block): the speed fade input is then `|notify_a_speed|`, a
    /// placeholder.
    pub notify_a_vector: Option<[f32; 4]>,
    /// Host inputs read by `E12_AnimSet::Load3b0` (0x4783b0). `x` is replaced
    /// with the typed driveline sample; the remaining words are supplied by
    /// the stage/animation host.
    pub marker: TickInput,
    /// The code of the `CSigOutsideAssistance` the step emitted (`CCar::Assist8DF0`, 0x4e8df0), which the HUD's
    /// receiver (0x573900) gives the race clock.
    pub outside_assistance: Option<u32>,
    /// The car's speed (m/s), which the HUD's `CSigCamera` receiver (`M579590`) tests for a jump start.
    pub speed: f32,
    /// The code of the `CSigRetireCar` the step emitted (`CCar::Update9AF0`, `E34_RetireA`), which the race flow's
    /// receiver (`E60Flow::M5662c0`, 0x5662c0) takes.
    pub retire: Option<u32>,
    /// The car's position (`CCar` +0x140), which `CCar::Vf28x` sends in `CSigCarGeneralState` with the velocity whose
    /// length is [`Self::speed`] ([`FinishZone`]).
    pub position: [f32; 3],
}

/// One `StepForward(1)` in driving, or one part of a load-time
/// `StepForward(40)` settle call. The host calls `inside_step` at the car's
/// `Vf6cx` sampling point, before the step advances its elapsed clock. A driving
/// step's `control` is `None` only on the step the full replay buffer fires
/// nothing in ([`ReplayRuns`]).
pub trait PhysicsStep {
    fn step(&mut self, control: Option<CarControl>, index: u32, inside_step: &mut dyn FnMut(StepSample));

    /// `CApplication::SetAppState` (0x47f370) stores the new state in
    /// `CApplication` +0x728, which is also the count word every
    /// `CMultiSignal1` selects its connection slot by: the control emit
    /// reaches the car's connection only in state 1. `Game` calls this at each
    /// state change, before the next substep; a host whose physics reads the
    /// word from its own memory writes it there.
    fn app_state(&mut self, _state: State) {}

    /// The drive's gear as NGP's live handler reads it to drop a selection of the gear the drive is in (the drive
    /// state's +0x570, [`CarControl::ngp`]); `None` when the host does not know it.
    fn drive_gear(&self) -> Option<u32> {
        None
    }
}

/// Who turns the filtered control into the car's payload while driving.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlPath {
    /// 1.02: the quantizer (`Pack`, then `NotifyFilt`), the bytes the replay stores.
    #[default]
    Quantized,
    /// NGP 7.4.778's handler (`CarControl::ngp`): the filtered values unquantized. A replay entry still goes through
    /// `NotifyFilt` (NGP calls the original in replay mode).
    Ngp,
}

/// What one substep emitted or recorded.
#[derive(Debug, Clone, Copy)]
pub struct SubstepEvent {
    /// State and counters as observed inside the step call. For the load
    /// settle this remains state 0xd until `StepForward(40)` returns.
    pub stage: Stage,
    /// `None` for the load settle, which runs outside `TicPhysicsRecording`.
    pub generated_control: Option<CarControl>,
    /// The control delivered to the physics host after a possible replay fire.
    pub control: Option<CarControl>,
    /// The driveline distance at the timekeeper call site, if the sample was
    /// within the stage's driveline table.
    pub distance: Option<f32>,
    pub checkpoint: Option<Checkpoint>,
    /// The code of the `CSigOutsideAssistance` the step emitted.
    pub outside_assistance: Option<u32>,
    /// The code of the `CSigRetireCar` the step emitted.
    pub retire: Option<u32>,
}

/// The parts of one outer frame that can be compared with FRM1 and peek data.
#[derive(Debug, Clone)]
pub struct FrameEvents {
    /// The frame clock's whole-step request before the single-step gate.
    pub clock: Frame,
    /// The stage case selected by `CApplication::Tic` (0x4804f0).
    pub tick_state: State,
    /// Physics substeps run this frame: settle calls first, then gated ticks.
    pub substeps: Vec<SubstepEvent>,
    pub marker_events: Vec<MarkerEvent>,
    /// Fired bits after the post-tick `Tic570`/`Load3b0` path (0x47a570/0x4783b0).
    pub marker_fired: usize,
    pub stage: Stage,
    pub time_card: TimeCard,
    pub elapsed: f64,
    /// The HUD's race clock after this frame's tick, and what the tick emitted.
    pub race_clock: RaceClock,
    pub clock_events: ClockEvents,
    /// The race flow's frame: the finish line, the retire's effects and the stage's end.
    pub flow: FlowEvents,
}

/// The run index at which the 1.02 replay buffer is full (`E19QccFire`, 0x49af10): the 134,999th run.
pub const REPLAY_RUNS: u32 = 0x20f56;

/// The 1.02 replay's record of the race (`CReplayManager`, `*0x893634`), which the race's step depends on: a buffer of
/// runs, each one quantized control (the nine bytes) and the count of consecutive steps that gave it, at most 0xffff.
///
/// Original: `TicPhysicsRecording` (0x47f420) runs, per substep, the filter, `E19ClsScan` (0x499ff0) and
/// `E19QccFire` (0x49af10), then `StepForward(1)`. `E19ClsScan` stores the step's control (`ClsTick`'s nine bytes, the
/// sticky flags ORed in): it adds a step to the current run when the control equals the run's in all nine bytes
/// (`E19Cls::Eq`, 0x499490) and the count is below 0xffff, else starts the next run, and sets the fired index
/// (+0x18b864) to the current run's (+0x3c). `E19QccFire` emits the fired run's control, unless the index is
/// [`REPLAY_RUNS`]: then it returns 0, the step runs with no control emitted, and `TicPhysicsRecording` emits
/// `CSigEndRace` with 1 and zero times and steps no more. `CReplayManager::RecInit` (0x49a1d0) empties the buffer when
/// the driving starts. A control that changes every step fills it after about 945 s of driving, before the 1800 s
/// time limit. NGP 7.4.778 replaces both calls (0x47f4c3 to `0x1002b060`, 0x47f4ce to `0x1002a520`) with a buffer
/// that grows by a run whenever one is needed, so its race never fills.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReplayRuns {
    /// The current run's control and step count; `None` before the first step.
    run: Option<(QuantizedControl, u16)>,
    /// The current run's index (+0x3c).
    index: u32,
}

impl ReplayRuns {
    /// `E19ClsScan` with the step's `control`, then `E19QccFire`'s test: whether the step's control is emitted.
    pub fn record(&mut self, control: QuantizedControl) -> bool {
        match self.run {
            Some((run, steps)) if run == control && steps < u16::MAX => self.run = Some((run, steps + 1)),
            Some(_) => {
                self.index += 1;
                self.run = Some((control, 1));
            }
            None => self.run = Some((control, 1)),
        }
        self.index != REPLAY_RUNS
    }

    /// The current run's index.
    pub fn index(&self) -> u32 {
        self.index
    }
}

/// The composed state for a single quick-race stage.
#[derive(Debug, Clone)]
pub struct Game {
    clock: FrameClock,
    gate: SingleStep,
    stage: Stage,
    filter: Filter,
    timekeeper: TimeKeeper,
    markers: MarkerTicker,
    marker_settings: MarkerSettings,
    driveline_values: Vec<f32>,
    max_time_step: f32,
    elapsed: f64,
    filter_speed: f32,
    last_sample: Option<StepSample>,
    race_clock: RaceClock,
    /// The penalties the race clock took so far: the outside-assistance ones and the jump start, s.
    penalties: f32,
    assist_settings: AssistSettings,
    flow: RaceFlow,
    /// The quantized flag a call for help left for the next driving substep.
    pending_flags: u8,
    /// The camera's start orbit, when the game gives the go itself ([`Game::with_camera_go`]).
    start_camera: Option<StartCamera>,
    controls: ControlPath,
    /// NGP's physics: its race steps (not the load's) run the x87 at 53 bits ([`timing::advance_time`]).
    ngp: bool,
    /// The stage's finish zone.
    finish_zone: FinishZone,
    /// The 1.02 replay buffer's runs (not under NGP).
    replay: ReplayRuns,
}

impl Game {
    /// Builds a stage from its DLS marker list and TRK driveline values.
    /// `stage` is normally `Stage::quick_race(track, car).load_ready()` once
    /// the state-5 host reply has arrived. `now` seeds `Vf0c`'s wall timer;
    /// `max_time_step` is the loaded physics definition's +0x1608c value.
    pub fn new(
        stage: Stage,
        grid: Grid,
        markers: &StageMarkers,
        driveline_values: Vec<f32>,
        max_time_step: f32,
        now: f64,
        marker_settings: MarkerSettings,
    ) -> Self {
        Self {
            clock: FrameClock::start(now),
            gate: SingleStep::default(),
            stage,
            filter: Filter::new(grid),
            timekeeper: TimeKeeper::load(markers),
            markers: MarkerTicker::load(markers),
            marker_settings,
            driveline_values,
            max_time_step: tick::step_interval(max_time_step),
            elapsed: CLOCK_START,
            filter_speed: 0.0,
            last_sample: None,
            race_clock: RaceClock::default(),
            penalties: 0.0,
            assist_settings: AssistSettings::QUICK_RACE,
            flow: RaceFlow::new(AssistSettings::QUICK_RACE),
            pending_flags: 0,
            start_camera: None,
            controls: ControlPath::Quantized,
            ngp: false,
            finish_zone: FinishZone::load(None),
            replay: ReplayRuns::default(),
        }
    }

    /// The stage's finish zone ([`FinishZone::load`] of its `.dls`; the original's default zone unless set).
    pub fn with_finish_zone(self, finish_zone: FinishZone) -> Self {
        Self { finish_zone, ..self }
    }

    /// The 1.02 replay buffer's runs so far.
    pub fn replay(&self) -> ReplayRuns {
        self.replay
    }

    /// The go from the camera's start orbit (`crate::camera_go`) instead of the host's [`FrameInput::camera_go`]: the
    /// orbit starts with the countdown (state 10) and gives the go after `pi / 1.2` s of frame time.
    pub fn with_camera_go(self) -> Self {
        let start_camera =
            Some(if self.stage.state() == State::Countdown { StartCamera::start() } else { StartCamera::Done });
        Self { start_camera, ..self }
    }

    /// The input filter's grid, before the race starts: a profile's ([`Grid::from_profile`], `E20ReadProfile`).
    pub fn set_filter_grid(&mut self, grid: Grid) {
        self.filter = Filter::new(grid);
    }

    /// The path from the filtered control to the car's payload ([`ControlPath::Quantized`] unless set).
    pub fn with_controls(self, controls: ControlPath) -> Self {
        Self { controls, ..self }
    }

    /// Under NGP's physics: the clock adds at the race steps' 53-bit precision (1.02's f32 sum unless set).
    pub fn with_ngp(self, ngp: bool) -> Self {
        Self { ngp, ..self }
    }

    pub fn controls(&self) -> ControlPath {
        self.controls
    }

    /// The camera's start orbit, with [`Game::with_camera_go`].
    pub fn start_camera(&self) -> Option<StartCamera> {
        self.start_camera
    }

    /// The game flow's settings the penalties are taken from ([`AssistSettings::QUICK_RACE`] unless set).
    pub fn set_assist_settings(&mut self, settings: AssistSettings) {
        self.assist_settings = settings;
        self.flow.set_settings(settings);
    }

    /// The settings [`Game::set_assist_settings`] gave.
    pub fn assist_settings(&self) -> AssistSettings {
        self.assist_settings
    }

    /// The race flow: the finish, the retire and the stage's end.
    pub fn flow(&self) -> &RaceFlow {
        &self.flow
    }

    pub fn race_clock(&self) -> RaceClock {
        self.race_clock
    }

    /// The penalties the race clock took so far, s (the outside-assistance ones and the jump start).
    pub fn penalties(&self) -> f32 {
        self.penalties
    }

    /// The stage result as it stands: the outcome, the race time at the finish, the penalties and the splits.
    pub fn result(&self) -> StageResult {
        match self.flow.end() {
            Some(end) => result::ended(&end, &self.timekeeper.card, self.flow.splits(), self.penalties),
            None => result::stage_result(&self.timekeeper.card, self.flow.splits(), self.race_clock, self.penalties),
        }
    }

    /// The pause menu offers the call for help (`0x56a620`): the race clock's gate ([`RaceClock::help_offered`]) and
    /// the rescue helicopter idle.
    pub fn help_offered(&self) -> bool {
        self.race_clock.help_offered() && !self.flow.helicopter_active()
    }

    /// The stage's end if the race is left now ([`RaceFlow::end_now`]): the flow's end once it came.
    pub fn end_now(&self) -> StageEnd {
        self.flow.end_now(&self.race_clock)
    }

    /// Grant credits to `CApplication`'s single-step gate (`M0047cd50`,
    /// 0x47cd50). While active, `TicPhysicsRecording` consumes one credit per
    /// substep.
    pub fn grant_single_steps(&mut self, credits: i32) {
        self.gate = self.gate.grant(credits);
    }

    /// Clear the single-step gate (`M0047cd70`, 0x47cd70).
    pub fn clear_single_step_gate(&mut self) {
        self.gate = SingleStep::default();
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn time_card(&self) -> TimeCard {
        self.timekeeper.card
    }

    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    pub fn markers(&self) -> &MarkerTicker {
        &self.markers
    }

    /// Runs one outer frame in the original's order:
    ///
    /// 1. `Vf0c` splits wall time and calls `M004801d0` (0x4801d0) for load
    ///    settle before input polling.
    /// 2. `TicFwd`/`NotifyB` poll and store raw input (0x4cc550/0x48ab40).
    /// 3. `Tic` selects countdown or driving; each driving substep runs
    ///    filter -> `Pack` -> payload, replay fire, then `StepForward(1)`
    ///    (0x48c500/0x48c910/0x48d130/0x47f420/0x5273a0).
    /// 4. On the returned in-step car sample, `Record` runs before the clock
    ///    advance (0x4ea300/0x4a0540), then `StepForward` advances time
    ///    (0x5273a0).
    /// 5. `M004825b0` runs the animation ticker after `Tic` (0x4825b0), so
    ///    the marker keys tick after the physics substeps.
    pub fn frame(&mut self, now: f64, input: FrameInput, physics: &mut impl PhysicsStep) -> FrameEvents {
        let frame = self.clock.frame(now, self.max_time_step);
        self.clock = frame.clock;
        let mut substeps = Vec::new();

        let state_at_frame_start = self.stage.state();
        if state_at_frame_start == State::Setup && input.race_host_ready {
            self.set_stage(self.stage.load_ready(), physics);
        }

        // `M004801d0`'s state 0xd calls StepForward(40), independent of
        // `Vf0c`'s requested `nsteps`, then spends those steps on +0x72c.
        if state_at_frame_start == State::Load {
            for index in 0..tick::SETTLE_SUBSTEPS[0] {
                substeps.push(self.physics_substep(physics, None, None, index));
            }
            self.set_stage(self.stage.load_tick(tick::SETTLE_SUBSTEPS[0]), physics);
            // The countdown's state 10 sets the camera's mode 10, which starts the orbit (`M487680(10,1)`).
            if self.stage.state() == State::Countdown && self.start_camera.is_some() {
                self.start_camera = Some(StartCamera::start());
            }
        }

        // Input poll (`TicFwd`) precedes the application tick, including
        // frames where the app is loading or counting down.
        self.filter.notify(&input.raw);
        let tick_state = self.stage.state();
        let mode = Mode(self.assist_settings.mode);
        if tick_state == State::Countdown && input.confirm && mode.confirm_arms() {
            self.set_stage(self.stage.confirm(), physics);
        }
        if tick_state == State::Countdown {
            self.set_stage(self.stage.countdown_tick(frame.facc), physics);
            if self.stage.state() == State::Driving {
                // `Tic` starts the replay's record (`RecInit`) and sets the player's camera (`M00566390`).
                self.replay = ReplayRuns::default();
                if self.start_camera.is_some() {
                    self.start_camera = Some(StartCamera::Done);
                }
            }
        }
        // The pause of a message box (state 2): the box takes the input; its accept ends the race.
        let mut acknowledged = None;
        let mut replay_full = None;
        if tick_state == State::Menu && input.acknowledge {
            acknowledged = self.flow.acknowledge(&self.race_clock);
            if acknowledged.is_some() {
                self.set_stage(self.stage.race_over(), physics);
            }
        }

        // The pause menu's retire: the game is in state 2 behind the menu, and the confirm's `CSigEndRace` moves it to
        // the end page (9), so the frame steps nothing.
        if tick_state == State::Driving && input.retire {
            acknowledged = self.flow.retire_from_menu(&self.race_clock);
            if acknowledged.is_some() {
                self.set_stage(self.stage.race_over(), physics);
            }
        }

        if tick_state == State::Driving && self.stage.state() == State::Driving {
            if input.help_call && self.help_offered() {
                self.pending_flags |= CALL_FOR_HELP;
            }
            let (count, gate) = self.gate.take(frame.nsteps);
            self.gate = gate;
            for index in 0..count {
                self.filter.set_speed(self.filter_speed);
                let filtered = self.filter.tic(self.max_time_step);
                let quantized = QuantizedControl::pack(&filtered);
                let generated = quantized.car_control();
                // `CReplayManager::ClsTick` (0x4996c0) ORs the sticky flags into the entry; `E19ClsScan` clears
                // them once it is stored.
                let replay = if index == 0 { input.replay_control } else { input.replay_tail.or(input.replay_control) };
                let pending = std::mem::take(&mut self.pending_flags);
                let (control, fires) = match (replay, self.controls) {
                    (None, ControlPath::Ngp) => {
                        let fired = Filtered { flags: filtered.flags | u32::from(pending), ..filtered };
                        (CarControl::ngp(&fired, physics.drive_gear()), true)
                    }
                    (replay, _) => {
                        let mut fired = replay.unwrap_or(quantized);
                        fired.flags |= pending;
                        (fired.car_control(), self.ngp || self.replay.record(fired))
                    }
                };
                substeps.push(self.physics_substep(physics, fires.then_some(control), Some(generated), index));
                if !fires {
                    replay_full = self.flow.replay_full();
                    self.set_stage(self.stage.race_over(), physics);
                    break;
                }
            }
        }

        // `CCar::Vf28x`'s `CSigCarGeneralState` reaches the finish zone (`E12_Rep::CbB50`) before the driveline
        // parameter reaches the marker ticker. Its `CSigEndRace` takes the race clock before this frame's HUD tick.
        let mut zone_end = None;
        let sample = self.last_sample;
        if self.stage.state() == State::Driving && sample.is_some_and(|s| self.finish_zone.check(s.position, s.speed)) {
            zone_end = self.flow.finish_zone(&self.race_clock);
            if zone_end.is_some() {
                self.set_stage(self.stage.race_over(), physics);
            }
        }

        // The marker ticker `Load3b0` runs once a frame from the car's frame update: `CCar::Vf28x` emits the driveline
        // parameter (0x4ea1f7, the float signal at manager +0x51a8), whose overflow receiver 0x478930 ticks the set
        // when the frame counter (`CApplication` +0x18, counted by `M004801d0`) moved. It runs in the start scene
        // (state 10) as well as in the race: the original's tanner playback takes no jump start for its car parked
        // past the start line, so the start scene fires the key, where `Load3b0` warns and skips the jump start.
        let mut marker_events = Vec::new();
        let marker_state = self.stage.state();
        if matches!(marker_state, State::Driving | State::Countdown) {
            if let Some(sample) = self.last_sample {
                let distance = driveline::sample_x(&self.driveline_values, sample.driveline_parameter);
                let mut marker_input = sample.marker;
                marker_input.x = distance.unwrap_or(sample.driveline_parameter);
                marker_input.seek_d = 0.0;
                marker_input.lookahead_scale = self.marker_settings.callout_distance;
                marker_input.pace_speed_threshold = self.marker_settings.marker_threshold;
                marker_input.start_line.game_host_result = self.assist_settings.school() as i32;
                marker_input.start_line.menu_host_result = self.race_clock.counting_down() as i32;
                marker_input.start_line.application_state = marker_state.value();
                marker_input.start_line.race_host_result = self.stage.track();
                marker_input.finish_to_menu = Mode(self.assist_settings.mode).finish_to_menu();
                let ticker = std::mem::replace(&mut self.markers, MarkerTicker::from_keys(Vec::new()));
                let (next, events) = ticker.tick(marker_input);
                self.markers = next;
                marker_events = events;
            }
        }

        // The HUD's race clock. The start line's jump start reaches it before the frame's tick: the peeks show the
        // clock with the countdown's rest before the countdown takes the frame's `dt` (`tests/race_clock_peek.rs`).
        // The tick is `0x579df0` (whose first call is `M576710`) and `M573d90`, with the frame's `facc`; the camera's
        // `CSigCamera` (`M579590`) comes after it.
        let mut clock_events = ClockEvents::default();
        let mut flow_events = FlowEvents { end: acknowledged.or(replay_full).or(zone_end), ..FlowEvents::default() };
        if self.stage.state() == State::Driving {
            let clock_at_markers = self.race_clock.time;
            let settings = self.assist_settings;
            for event in &marker_events {
                match event {
                    MarkerEvent::StartLine(effects) => {
                        if effects.jump_start {
                            let (clock, penalty) = self.race_clock.jump_start(settings);
                            self.race_clock = clock;
                            clock_events.jump_start = clock_events.jump_start.or(penalty);
                        }
                        if effects.clear_menu_flag {
                            self.race_clock.running = true;
                        }
                    }
                    MarkerEvent::FinishMenu => self.race_clock.running = false,
                    _ => {}
                }
            }
            let (clock, events) = self.race_clock.tick(frame.facc, settings);
            self.race_clock = clock;
            clock_events = ClockEvents { jump_start: clock_events.jump_start, ..events };
            let speed = self.last_sample.map_or(0.0, |sample| sample.speed);
            if self.race_clock.moving_start(settings, speed) {
                let (clock, penalty) = self.race_clock.jump_start(settings);
                self.race_clock = clock;
                clock_events.jump_start = clock_events.jump_start.or(penalty);
            }
            self.penalties += clock_events.penalty.unwrap_or(0.0) + clock_events.jump_start.unwrap_or(0.0);
            let retires: Vec<u32> = substeps.iter().filter_map(|s| s.retire).collect();
            flow_events = self.flow.frame(FlowFrame {
                dt: frame.facc,
                state: self.stage.state(),
                clock_at_markers,
                clock: self.race_clock,
                markers: &marker_events,
                retires: &retires,
                excluded: settings.excluded(clock_events.jump_start.is_some()),
            });
            if flow_events.end.is_some() {
                self.set_stage(self.stage.race_over(), physics);
            } else if flow_events.message.is_some() {
                self.set_stage(self.stage.pause(), physics);
            }
        }
        // `E16CamHolder::Tic` is part of the post-tick M004825b0 path; its
        // M0047ca40 event arms the countdown for the next outer frame.
        // With the go timer, `M004825b0`'s state 10 runs the camera (`M8840`) with the frame's `facc`.
        let mut camera_go = input.camera_go;
        if let Some(camera) = self.start_camera.filter(|_| self.stage.state() == State::Countdown) {
            let (camera, go) = camera.tick(frame.facc, mode.camera_go());
            self.start_camera = Some(camera);
            camera_go |= go;
        }
        if camera_go {
            self.set_stage(self.stage.go(), physics);
        }

        FrameEvents {
            clock: frame,
            tick_state,
            substeps,
            marker_fired: self.markers.keys().iter().filter(|key| key.word1 & crate::markers::FIRED != 0).count(),
            marker_events,
            stage: self.stage,
            time_card: self.timekeeper.card,
            elapsed: self.elapsed,
            race_clock: self.race_clock,
            clock_events,
            flow: flow_events,
        }
    }

    /// Moves the stage to `next`. A state change is `SetAppState`'s store, which the physics host is given; its
    /// `CSigAppState` emit reaches only the car sound (out of scope).
    fn set_stage(&mut self, next: Stage, physics: &mut impl PhysicsStep) {
        if next.state() != self.stage.state() {
            physics.app_state(next.state());
        }
        self.stage = next;
    }

    fn record_step(
        &mut self,
        sample: StepSample,
        generated_control: Option<CarControl>,
        control: Option<CarControl>,
    ) -> SubstepEvent {
        self.last_sample = Some(sample);
        if let Some(code) = sample.outside_assistance {
            self.race_clock = self.race_clock.outside_assistance(code);
        }
        self.filter_speed = match sample.notify_a_vector {
            // `Vf28x` clamps the speed at 0 before the emit.
            Some(vector) => {
                filter::notify_a(if sample.notify_a_speed < 0.0 { 0.0 } else { sample.notify_a_speed }, vector)
            }
            None => sample.notify_a_speed.abs(),
        };
        let distance = driveline::sample_x(&self.driveline_values, sample.driveline_parameter);
        let checkpoint = distance.map(|x| {
            // `Record` is inside Vf6cx and observes the current elapsed time;
            // StepForward advances the clock only after this call returns.
            let (keeper, checkpoint) = self.timekeeper.record(x, self.elapsed, self.max_time_step);
            self.timekeeper = keeper;
            checkpoint
        });
        let pc53 = self.ngp && self.stage.state() != State::Load;
        self.elapsed = timing::advance_time(self.elapsed, self.max_time_step, pc53);
        SubstepEvent {
            stage: self.stage,
            generated_control,
            control,
            distance,
            checkpoint: checkpoint.flatten(),
            outside_assistance: sample.outside_assistance,
            retire: sample.retire,
        }
    }

    fn physics_substep(
        &mut self,
        physics: &mut impl PhysicsStep,
        control: Option<CarControl>,
        generated_control: Option<CarControl>,
        index: u32,
    ) -> SubstepEvent {
        let mut event = None;
        physics.step(control, index, &mut |sample| {
            assert!(event.is_none(), "PhysicsStep called its in-step callback more than once");
            event = Some(self.record_step(sample, generated_control, control));
        });
        event.expect("PhysicsStep must call its in-step callback once")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Physics that only answers the in-step callback.
    struct Still;

    impl PhysicsStep for Still {
        fn step(&mut self, _control: Option<CarControl>, _index: u32, inside_step: &mut dyn FnMut(StepSample)) {
            inside_step(StepSample::default());
        }
    }

    /// With the camera's go timer, the countdown (state 10) waits for the orbit: the go on the 375th frame of 7 ms
    /// (the first is the last settle frame, at no frame time here), the driving from the frame after it.
    #[test]
    fn the_camera_gives_the_go_after_its_orbit() {
        let dt = 0.007f32;
        let stage = Stage::quick_race(54, 6).load_ready();
        let markers = StageMarkers { keys: Vec::new() };
        let mut game = Game::new(
            stage,
            Grid::from_settings(&crate::ini::Ini::parse("")),
            &markers,
            Vec::new(),
            dt,
            0.0,
            MarkerSettings::default(),
        )
        .with_camera_go();
        while game.stage().state() == State::Load {
            game.frame(0.0, FrameInput::default(), &mut Still);
        }
        assert_eq!(game.start_camera(), Some(StartCamera::start()));
        let mut now = 0.0f64;
        let mut frames = 0;
        while game.stage().state() == State::Countdown {
            now += f64::from(dt);
            frames += 1;
            let events = game.frame(now, FrameInput::default(), &mut Still);
            assert!(events.substeps.is_empty());
        }
        assert_eq!(frames, 376, "375 frames of orbit, then Tic's countdown below 0");
        assert_eq!(game.start_camera(), Some(StartCamera::Done));
        now += f64::from(dt);
        assert_eq!(game.frame(now, FrameInput::default(), &mut Still).substeps.len(), 1);
    }

    /// Physics whose first step emits a retire code.
    struct Retires(Option<u32>);

    impl PhysicsStep for Retires {
        fn step(&mut self, _control: Option<CarControl>, _index: u32, inside_step: &mut dyn FnMut(StepSample)) {
            inside_step(StepSample { retire: self.0.take(), ..StepSample::default() });
        }
    }

    /// A retire code 4 posts `MSG_RETIRE_OFFSTAGE` a second later: the game pauses (state 2, no steps, the race clock
    /// held) until the player accepts the box, which ends the race retired on the next frame.
    #[test]
    fn the_off_stage_box_pauses_the_game_until_it_is_accepted() {
        let dt = 0.007f32;
        let mut game = Game::new(
            Stage::driving(54, 6),
            Grid::from_settings(&crate::ini::Ini::parse("")),
            &StageMarkers { keys: Vec::new() },
            Vec::new(),
            dt,
            0.0,
            MarkerSettings::default(),
        );
        let mut physics = Retires(Some(4));
        let mut now = 0.0f64;
        let mut frame = |game: &mut Game, accept: bool| {
            now += f64::from(dt);
            game.frame(now, FrameInput { acknowledge: accept, ..FrameInput::default() }, &mut physics)
        };
        let posted = (1..1000).find(|_| frame(&mut game, true).flow.message.is_some());
        assert_eq!(posted, Some(143));
        assert_eq!(game.stage().state(), State::Menu);
        let clock = game.race_clock();
        for _ in 0..500 {
            let events = frame(&mut game, false);
            assert!(events.substeps.is_empty() && events.flow == FlowEvents::default());
        }
        assert_eq!(game.race_clock(), clock);
        let end = frame(&mut game, true).flow.end.expect("the accept ends the race");
        assert_eq!((end.cause, end.finished()), (crate::flow::EndCause::Offstage, false));
        assert_eq!(game.stage().state(), State::Ended);
    }

    /// The pause menu's retire ends the race retired with the race clock as it stands, on a frame that steps nothing;
    /// a second one does nothing.
    #[test]
    fn the_menu_s_retire_ends_the_race_without_a_step() {
        let dt = 0.007f32;
        let mut game = Game::new(
            Stage::driving(54, 6),
            Grid::from_settings(&crate::ini::Ini::parse("")),
            &StageMarkers { keys: Vec::new() },
            Vec::new(),
            dt,
            0.0,
            MarkerSettings::default(),
        );
        let mut now = 0.0f64;
        for _ in 0..300 {
            now += f64::from(dt);
            assert_eq!(game.frame(now, FrameInput::default(), &mut Still).substeps.len(), 1);
        }
        let clock = game.race_clock();
        now += f64::from(dt);
        let events = game.frame(now, FrameInput { retire: true, ..FrameInput::default() }, &mut Still);
        assert!(events.substeps.is_empty());
        let end = events.flow.end.expect("the retire ends the race");
        assert_eq!((end.cause, end.finished(), end.end_race.time), (crate::flow::EndCause::Retired, false, clock.time));
        assert_eq!(game.stage().state(), State::Ended);
        now += f64::from(dt);
        assert_eq!(game.frame(now, FrameInput { retire: true, ..FrameInput::default() }, &mut Still).flow.end, None);
    }

    /// A frame of several steps (NGP's 1/720 s in a 7 ms frame) fires the replay entry on its first step and the
    /// tail entry on the others.
    #[test]
    fn a_frame_of_several_steps_fires_the_tail_after_the_first() {
        let dt = 1.0f32 / 720.0;
        let mut game = Game::new(
            Stage::driving(11, 5),
            Grid::from_settings(&crate::ini::Ini::parse("")),
            &StageMarkers { keys: Vec::new() },
            Vec::new(),
            dt,
            0.0,
            MarkerSettings::default(),
        );
        let first = QuantizedControl { flags: 1, throttle: 200, ..QuantizedControl::default() };
        let input = FrameInput {
            replay_control: Some(first),
            replay_tail: Some(QuantizedControl { flags: 0, ..first }),
            ..FrameInput::default()
        };
        let events = game.frame(0.007, input, &mut Still);
        let flags: Vec<u32> = events.substeps.iter().map(|s| s.control.unwrap().flags).collect();
        let (once, none) = (first.car_control().flags, QuantizedControl { flags: 0, ..first }.car_control().flags);
        assert_ne!(once, none);
        assert_eq!(flags, [once, none, none, none, none]);
    }

    /// One 1.02 step of wall time, as the frame clock widens it.
    const DT: f64 = 0.007f32 as f64;

    fn new_game(stage: Stage) -> Game {
        Game::new(
            stage,
            Grid::from_settings(&crate::ini::Ini::parse("")),
            &StageMarkers { keys: Vec::new() },
            Vec::new(),
            0.007,
            0.0,
            MarkerSettings::default(),
        )
    }

    /// In a championship (`M8840` gives no go in mode 3) the countdown waits for the player's confirm (`M0047f9c0`
    /// case 10), which arms 2.0 s; the driving starts 286 frames of 7 ms later and ends the orbit.
    #[test]
    fn m0047f9c0_a_championship_start_waits_for_the_confirm() {
        let mut game = new_game(Stage::quick_race(54, 6).load_ready()).with_camera_go();
        game.set_assist_settings(AssistSettings { mode: 3, ..AssistSettings::QUICK_RACE });
        while game.stage().state() == State::Load {
            game.frame(0.0, FrameInput::default(), &mut Still);
        }
        let mut now = 0.0f64;
        let mut frame = |game: &mut Game, input: FrameInput| {
            now += DT;
            game.frame(now, input, &mut Still)
        };
        for _ in 0..2000 {
            assert!(frame(&mut game, FrameInput::default()).substeps.is_empty());
        }
        assert_eq!(game.stage().state(), State::Countdown);
        assert!(game.start_camera().and_then(StartCamera::angle).is_some());
        let mut frames = 1;
        frame(&mut game, FrameInput { confirm: true, ..FrameInput::default() });
        while game.stage().state() == State::Countdown {
            frame(&mut game, FrameInput { confirm: true, ..FrameInput::default() });
            frames += 1;
        }
        assert_eq!(frames, 286);
        assert_eq!(game.start_camera(), Some(StartCamera::Done));
        assert_eq!(frame(&mut game, FrameInput::default()).substeps.len(), 1);
    }

    /// The quick race takes no confirm: its go is the camera's.
    #[test]
    fn a_quick_race_ignores_the_confirm() {
        let mut game = new_game(Stage::quick_race(54, 6).load_ready().load_tick(150));
        game.frame(DT, FrameInput { confirm: true, ..FrameInput::default() }, &mut Still);
        assert!(!game.stage().armed());
    }

    /// Physics whose car stands at `position`.
    struct Parked([f32; 3]);

    impl PhysicsStep for Parked {
        fn step(&mut self, _control: Option<CarControl>, _index: u32, inside_step: &mut dyn FnMut(StepSample)) {
            inside_step(StepSample { position: self.0, ..StepSample::default() });
        }
    }

    /// `CbB50` after the frame's steps: a car standing in the zone ends the race finished on that frame, with the race
    /// clock before the frame's tick; one outside it drives on.
    #[test]
    fn cbb50_a_car_standing_in_the_finish_zone_ends_the_race() {
        let zone = FinishZone::load(Some([10.0f32, 0.0, 0.0, 5.0].map(f32::to_bits)));
        let mut outside = new_game(Stage::driving(54, 6)).with_finish_zone(zone);
        let mut now = 0.0f64;
        for _ in 0..100 {
            now += DT;
            assert_eq!(outside.frame(now, FrameInput::default(), &mut Parked([0.0; 3])).flow.end, None);
        }
        let mut inside = new_game(Stage::driving(54, 6)).with_finish_zone(zone);
        let events = inside.frame(DT, FrameInput::default(), &mut Parked([10.0, 3.0, 4.0]));
        assert_eq!(events.substeps.len(), 1);
        let end = events.flow.end.expect("the zone ends the race");
        assert_eq!((end.cause, end.finished()), (crate::flow::EndCause::FinishZone, true));
        assert_eq!(inside.stage().state(), State::Ended);
    }

    #[test]
    fn e19_cls_scan_counts_runs_of_equal_controls() {
        let mut runs = ReplayRuns::default();
        let a = QuantizedControl::default();
        let b = QuantizedControl { gear: 1, ..a };
        for _ in 0..3 {
            assert!(runs.record(a));
        }
        assert_eq!(runs.index(), 0);
        assert!(runs.record(b));
        assert_eq!(runs.index(), 1);
        for _ in 1..u16::MAX {
            runs.record(b);
        }
        assert_eq!(runs.index(), 1, "0xffff steps in one run");
        runs.record(b);
        assert_eq!(runs.index(), 2, "the run is full");
    }

    /// `E19QccFire`: a control that changes every step retires the race on the step that starts run 0x20f56, which
    /// still runs (with no control emitted); then nothing more is stepped.
    #[test]
    fn e19_qcc_fire_a_full_replay_buffer_retires() {
        let mut game = new_game(Stage::driving(54, 6));
        let mut emitted = Emitted(Vec::new());
        let mut now = 0.0f64;
        let mut ended = None;
        for step in 0..=REPLAY_RUNS + 1 {
            now += DT;
            let control = QuantizedControl { steering: (step % 2) as i8, ..QuantizedControl::default() };
            let events =
                game.frame(now, FrameInput { replay_control: Some(control), ..FrameInput::default() }, &mut emitted);
            if let Some(end) = events.flow.end {
                ended = Some((step, end, events.substeps.len()));
                break;
            }
        }
        let (step, end, substeps) = ended.expect("the buffer fills");
        assert_eq!((step, substeps), (REPLAY_RUNS, 1));
        assert_eq!((end.cause, end.finished(), end.end_race.time), (crate::flow::EndCause::ReplayFull, false, 0.0));
        assert_eq!(emitted.0.len() as u32, REPLAY_RUNS + 1);
        assert!(emitted.0[..REPLAY_RUNS as usize].iter().all(Option::is_some) && emitted.0.last() == Some(&None));
        assert!(game.frame(now + DT, FrameInput::default(), &mut emitted).substeps.is_empty());
        // NGP's buffer grows: the same drive drives on.
        let mut ngp = new_game(Stage::driving(54, 6)).with_ngp(true);
        for step in 0..=REPLAY_RUNS {
            let control = QuantizedControl { steering: (step % 2) as i8, ..QuantizedControl::default() };
            let input = FrameInput { replay_control: Some(control), ..FrameInput::default() };
            assert_eq!(ngp.frame(f64::from(step + 1) * DT, input, &mut Still).flow.end, None);
        }
    }

    /// Physics that keeps whether each step was given a control.
    struct Emitted(Vec<Option<()>>);

    impl PhysicsStep for Emitted {
        fn step(&mut self, control: Option<CarControl>, _index: u32, inside_step: &mut dyn FnMut(StepSample)) {
            self.0.push(control.map(|_| ()));
            inside_step(StepSample::default());
        }
    }
}

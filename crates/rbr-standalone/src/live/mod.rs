//! Live mode (`rbr-standalone --live --snapshot Y --stage NAME`): the viewer's stepping
//! process, speaking `harness/live.py`'s protocol (`rbr-render`'s `live`). Per 32-byte control payload on stdin it
//! writes one FRM1 frame, a 32-byte HUD2 record (`rbr_recording::hud`: live.py's HUD1 and the race clock) and a DMG1
//! record (`rbr_recording::damage`: the damage zones and the last damage event) on stdout, then the tagged records
//! (`rbr_recording::live`): the step's tyre-particle payloads (PTC1, `rbr_recording::particles`), the detached parts
//! (DET1, `rbr_recording::detach`), the sound inputs (SND1, `rbr_recording::sound`), the step's pace-note callouts
//! (PCN1, `rbr_recording::pacenote`), the splits (SPL1, `rbr_recording::splits`), the HUD's dash values (DSH1,
//! `rbr_recording::dash`), the camera's mode and start orbit (CAM1, `rbr_recording::camera`), the race flow's events
//! (FLW1, `rbr_recording::flow`) and the steering force (FFB1, `rbr_recording::ffb`), the step clock (CLK1,
//! `rbr_recording::clock`), under NGP its sound words (NSD1, `rbr_recording::sound::NgpSound`, after FFB1), the gear
//! ratios and drive speed (GBX1, `rbr_recording::gearbox`), the shift points (SHF1, `rbr_recording::shift`), the pause
//! menu's (MNU1, `rbr_recording::menu`), what NGP's FMOD block takes from the car (FMB1,
//! `rbr_recording::sound::FmodBlock`), the damage handler's carried words for the HUD's car status (CDM1,
//! `rbr_recording::damage::CDM1`), the car's control the last step took (CTL1, `rbr_recording::live::CTL1`), and
//! last the SCN1 scenery poses: the frame's pre and post count from the snapshot's step, and the car and wheels after
//! the payload's step.
//!
//! The first payload is the snapshot's own step, whose control the game had already emitted, so its bytes are not
//! used. Each later payload is one `Game::frame`: the gameplay loop emits the payload and the pure physics runs
//! `StepForward(1)` on the snapshot's memory for each of the frame's substeps, as the headless run does
//! (`crate::physics::Pure`). A payload is one step of wall time (one substep) unless [`Live::pace`] gives it a frame time: the
//! viewer's `--live` sends one per 7 ms (1.02's step), which under NGP's 1/720 s step is a frame of 5 or 6 substeps
//! (the frame clock carries the rest), so every clock the game ticks by the frame's time (the countdown, the camera's
//! orbit, the race clock, the finish timer) runs at real speed under both. Such a frame's records are the frame's: the
//! car after its last step, its events, and each wheel's last tyre payload. The game starts
//! driving: a snapshot taken mid-drive has its load and countdown behind it, and the game's clock and time card do
//! not feed the physics.
//!
//! `--live --boot --stage NAME --car N` starts from the boot's image instead (`boot::build`, the
//! stage start): before the first payload the game runs the load settle as the headless boot run does. With the
//! camera's go timer (`rbr_gameplay::camera_go`, `--live --boot`), the payloads are then the frames of the countdown
//! (state 10: the camera's start orbit, no physics step; answered with the car as it stands, pre equal to post and no
//! substeps) until the camera gives the go and the driving starts. Each payload stands for its frame time, as in the
//! drive, so the go comes after `pi / 1.2` s of frames (375 of 7 ms). In a championship (modes 3 to 5) the orbit gives
//! no go: the player's confirm, an input payload's `ACCEPT_MESSAGE` (a recorded payload confirms at once), arms the 2 s
//! countdown (`FrameInput::confirm`). Without the timer (the autopilot
//! and the recorded-boot checks) the go comes before the first payload, as the headless boot gives it at the
//! recording's step. Each payload of the drive is one frame of one step, from the settle's last step.
//!
//! A payload with the call-for-help flag (quantized 0x20, the viewer's call for help) gives the game that request
//! (`FrameInput::help_call`) instead of the flag, so the race clock's gate applies; the assist and its penalty are
//! logged to stderr.
//!
//! A retire's message box (`MSG_RETIRE_BYSTAND`, `MSG_RETIRE_OFFSTAGE`) pauses the game (state 2): the payloads are
//! answered without a step, FLW1 says the box is up, until an input payload with `ACCEPT_MESSAGE` accepts it and the
//! race ends. A recorded payload (the autopilot, the recorded checks) accepts it at once: ours, a recorded drive has no
//! player to read the box.
//!
//! The viewer's pause menu (`rbr_gameplay::pause_menu`) pauses by sending nothing. Each answer ends with a pause-menu
//! record (MNU1, `rbr_recording::menu`, before SCN1): the mode, whether the game is driving with no box up, whether the
//! call for help is offered and whether the race is over. An input payload with `RETIRE` (the menu's confirmed
//! retire) ends the race retired on its frame, with no step.
//!
//! The race flow runs as in the headless run: past the finish line the car is still driven until the finish timer
//! (20 s) or the end-of-stage marker ends the race. From then on (the application state 9, the end page) no payload is
//! stepped: each is answered with the last frame, whose split record (SPL1) says the race is over and how it ended.
//!
//! A call the pure host cannot run stops the run. The stop goes to stderr once, and from then on every payload is
//! answered with the last good frame and HUD, so the car freezes and the viewer stays up. A payload that is not
//! quantized is a protocol error and ends the run.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

use rbr_formats::data::Data;
use rbr_gameplay::control::{ngp_direct_gears, QuantizedControl, CALL_FOR_HELP};
use rbr_gameplay::filter::{Grid, Raw};
use rbr_gameplay::flow::{EndPage, FlowEvents, Message, StageEnd};
use rbr_gameplay::game::{ControlPath, FrameInput, Game};
use rbr_gameplay::ini::Ini;
use rbr_gameplay::markers::{MarkerEvent, MarkerSettings, PaceCallout};
use rbr_gameplay::race_clock::RaceClock;
use rbr_gameplay::stage::{Stage, State};
use rbr_gameplay::tick::SETTLE_SUBSTEPS;
use rbr_hybrid::pure::{self, Playback, CAR_BYTES, CAR_PTR};
use rbr_hybrid::timekeeper::layout as keeper_layout;
use rbr_physics::mem::Mem;
use rbr_physics::snapshot::Snapshot;
use rbr_recording::camera::{Camera, MODE_CHASE, MODE_START_ORBIT};
use rbr_recording::clock::StepClock;
use rbr_recording::dash::DSH1;
use rbr_recording::flow::{Flow, MessageBox};
use rbr_recording::input::{Input, Payload, ACCEPT_MESSAGE, RETIRE};
use rbr_recording::live as records;
use rbr_recording::menu::Menu;
use rbr_recording::pacenote::{self, PCN1};
use rbr_recording::particles;
use rbr_recording::sound::{TyreMax, SND1};
use rbr_recording::{CONTROL_BYTES, MAGIC};

use crate::physics::{boot_playback, chain, load_substeps, why, Pure, DT};
use crate::saves::Saves;
use crate::stage::StageData;
use crate::{boot, rally};

mod picture;
mod realtime;

pub use realtime::{serve_frames, serve_realtime};

use picture::{
    callouts, damper_frame, dash, last_per_wheel, pace_callouts, particle_events, race_hud, split_record, with_fires,
    Picture,
};

/// The snapshot's stepping: the gameplay loop and the pure physics, one substep per payload.
pub struct Live<'a> {
    playback: Playback,
    game: Game,
    stage: &'a StageData,
    /// The frame clock's timer reading, [`Self::frame_seconds`] on per payload.
    now: f64,
    /// The wall time one payload period stands for: one step unless [`Live::pace`] sets it. An input payload stands
    /// for `Input::frames` periods.
    frame_seconds: f64,
    /// The step clock record (CLK1): the physics step (the image's `MaxTimeStep`, or the retail constant when the
    /// image has none) and the load settle's steps.
    clock: StepClock,
    /// The image's step and, for a snapshot, the substeps of its own call (the first payload's step).
    start: u32,
    own_substeps: Option<u32>,
    /// Steps answered so far (the snapshot's own step is 0).
    answered: u32,
    /// Countdown frames answered so far (no step).
    waited: u32,
    /// The last good frame, answered again once the run has stopped.
    last: Picture,
    /// The car's control the last step took (CTL1).
    control: [u8; CONTROL_BYTES],
    /// What the frames since the last picture emitted ([`Live::picture`]).
    pending: Pending,
    /// The outside-assistance penalties the race clock added so far.
    penalties: f32,
    stopped: bool,
    /// The handles of the detachable car parts (the boot's image; none for a snapshot).
    detach: Vec<(u32, u32)>,
    /// The `.trk` driveline values (the car's distance in the dash record).
    track: Vec<f32>,
    /// The player's stage record and personal best (`--saves`).
    saves: Option<Saves>,
    /// The start clock's time of day, s after midnight (`set_start_clock`).
    time_of_day: Option<f32>,
    /// What follows a championship stage (`set_rally_next`): its end page's choice and text.
    rally_next: RallyNext,
}

/// What follows a championship stage (`--rally-next`): the end page of a finish in a car that can't drive is 0x18
/// unless the stage is the rally's last (the season's stage index 5), and its text names the service or the next stage
/// (`CFailedSeasonFinishedWithBadCarUI::vf0c`, 0x5b6900).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RallyNext {
    #[default]
    Stage,
    Service,
    End,
}

impl RallyNext {
    pub fn parse(name: &str) -> Option<RallyNext> {
        match name {
            "stage" => Some(RallyNext::Stage),
            "service" => Some(RallyNext::Service),
            "end" => Some(RallyNext::End),
            _ => None,
        }
    }
}

impl<'a> Live<'a> {
    pub fn new(snap: &Snapshot, stage: &'a StageData, settings: MarkerSettings) -> Result<Self, String> {
        let h = snap.metadata();
        let playback = Playback::new(snap, false);
        crate::physics::require_ngp(&playback)?;
        let dt = pure::max_time_step(snap).unwrap_or(DT);
        let game = new_game(Stage::driving(stage.number, stage.car), stage, dt, settings);
        let settle = load_substeps(&playback.pure.mem).div_ceil(SETTLE_SUBSTEPS[0]) * SETTLE_SUBSTEPS[0];
        Ok(Self::with(playback, game, stage, StepClock { step: dt, settle }, h.step, Some(h.substeps), Vec::new()))
    }

    /// The boot's stepping (`--live --boot`): `race` on `name` from the stage start. The load settle runs here, as the
    /// headless boot runs it before its first driving frame. With `camera_go` the camera's start orbit gives the go
    /// during the payloads; without it the go is given here too.
    pub fn boot(
        data: &Data,
        name: &str,
        race: &boot::Race,
        stage: &'a StageData,
        settings: MarkerSettings,
        camera_go: bool,
    ) -> Result<Self, String> {
        let (playback, image) = boot_playback(data, stage, name, race, None, false)?;
        let built: Vec<String> = image.built.iter().map(|(what, n)| format!("{what} {n}")).collect();
        eprintln!("rbr-standalone: boot built {}", built.join(", "));
        let dt = keeper_layout::max_time_step(&playback.pure.mem);
        let mut game = new_game(
            Stage::quick_race(stage.number, stage.car).load_ready_with(load_substeps(&playback.pure.mem)),
            stage,
            dt,
            settings,
        );
        if camera_go {
            game = game.with_camera_go();
        }
        game.set_assist_settings(race.assist_settings());
        let mut physics = Pure {
            playback: &playback,
            trace: None,
            track: &stage.track,
            own_step: true,
            word6: 0,
            substeps: SETTLE_SUBSTEPS[0],
            tyre_max: TyreMax::default(),
            twin: None,
        };
        let mut settled = 0;
        // Without the camera, the go's two frames as the headless boot runs them: the second takes a step's time.
        let now = if camera_go { 0.0 } else { f64::from(dt) };
        panic::catch_unwind(AssertUnwindSafe(|| {
            while game.stage().state() == State::Load {
                settled += game.frame(0.0, FrameInput::default(), &mut physics).substeps.len() as u32;
            }
            if !camera_go {
                let go = FrameInput { camera_go: true, ..FrameInput::default() };
                game.frame(0.0, go, &mut physics);
                game.frame(now, FrameInput::default(), &mut physics);
            }
        }))
        .map_err(|e| format!("the boot's settle, step {settled}: {}", why(e.as_ref())))?;
        let mut live =
            Self::with(playback, game, stage, StepClock { step: dt, settle: settled }, settled, None, image.detach);
        live.now = now;
        Ok(live)
    }

    fn with(
        playback: Playback,
        game: Game,
        stage: &'a StageData,
        clock: StepClock,
        start: u32,
        own_substeps: Option<u32>,
        detach: Vec<(u32, u32)>,
    ) -> Self {
        let mut playback = playback;
        playback.pure.keep_tyre_particles = true;
        playback.pure.take_tyre_particles();
        playback.pure.take_particle_events();
        playback.pure.take_engine_fires();
        let splits = split_record(&game, stage, None);
        // Before any frame ran no reset has: the memory words stand as the step left them.
        let dampers = damper_frame(&playback.pure.mem, &[]);
        let mut last = Picture::of(
            &playback.pure.mem,
            race_hud(game.race_clock(), 0.0),
            &splits,
            stage.interactive_handle_end,
            &[],
            &detach,
            dampers,
        );
        let track: Vec<f32> = stage.track.iter().map(|r| r.value).collect();
        // The countdown's frames repeat this one: its dash is the car's as it stands (not an empty record, whose
        // zero battery and engine would light the warnings).
        last.dash = dash(&playback.pure.mem, &stage.track, &track, game.markers().keys(), (None, None), &[], 0, None)
            .to_bytes();
        Self {
            playback,
            game,
            stage,
            now: 0.0,
            frame_seconds: f64::from(clock.step),
            clock,
            start,
            own_substeps,
            answered: 0,
            waited: 0,
            last,
            control: [0; CONTROL_BYTES],
            pending: Pending::default(),
            penalties: 0.0,
            stopped: false,
            detach,
            track,
            time_of_day: None,
            saves: None,
            rally_next: RallyNext::default(),
        }
    }

    /// Answers one payload with the frame and HUD record after its step, as `harness/live.py` writes them.
    pub fn answer(&mut self, payload: &[u8; CONTROL_BYTES]) -> Result<Vec<u8>, String> {
        self.answer_over(payload, 1)
    }

    /// [`Live::answer`], a recorded payload standing for `periods` payload periods (`--pilot autopilot` in a frame
    /// of a viewer behind real time, `Input::frames`); an input payload's own periods are its `frames`.
    pub fn answer_over(&mut self, payload: &[u8; CONTROL_BYTES], periods: u32) -> Result<Vec<u8>, String> {
        let (pre, substeps) = self.run(payload, periods, true)?;
        Ok(self.answer_bytes(payload, pre, substeps))
    }

    /// The frame of [`Live::answer_over`] without its answer, and with `picture` its records: the step it started
    /// at and the substeps it counts.
    fn run(&mut self, payload: &[u8; CONTROL_BYTES], periods: u32, picture: bool) -> Result<(u32, u32), String> {
        let pre = self.start + self.answered;
        let control = match Payload::read(payload).map_err(|e| format!("step {pre}: {e}"))? {
            Payload::Input(input) => Control::Input(input),
            Payload::Recorded(words) => Control::Recorded(
                QuantizedControl::from_payload(words)
                    .ok_or_else(|| format!("step {pre}: the recorded payload is not quantized"))?,
                words[6],
                periods.max(1),
            ),
        };
        let mut substeps = 1;
        // Once the race is over (state 9, its end page) the game steps the car no more: the answers repeat the last
        // frame, whose split record says the race is over.
        if !self.stopped && self.game.stage().state() != State::Ended {
            match if picture { self.step(control) } else { self.frame(control) } {
                Ok(stepped) => substeps = stepped,
                Err(stop) => {
                    eprintln!("rbr-standalone: {stop}; the car is frozen at its last good frame");
                    self.stopped = true;
                }
            }
        } else {
            // A repeated frame: its step's events (callouts, tyre payloads, the flow's) are not sent again.
            self.last.pace = pacenote::to_bytes(&[]);
            self.last.tyres = particles::record(&[]);
            self.last.flow = Flow::default().to_bytes();
        }
        self.answered += substeps;
        Ok((pre, substeps))
    }

    /// The answer of a frame that echoes `payload` and ran `substeps` from step `pre`: the frame and HUD record and
    /// the tagged records of the last picture.
    fn answer_bytes(&self, payload: &[u8; CONTROL_BYTES], pre: u32, substeps: u32) -> Vec<u8> {
        let camera = match self.game.start_camera().and_then(|c| c.angle()) {
            Some(angle) => Camera { mode: MODE_START_ORBIT, angle },
            None => Camera { mode: MODE_CHASE, angle: 0.0 },
        }
        .record();
        let clock = self.clock.record();
        let menu = self.menu().record();
        let records = self.last.hud.len()
            + self.last.damage.len()
            + self.last.tyres.len()
            + self.last.events.len()
            + self.last.detached.len()
            + 16
            + self.last.sound.len()
            + self.last.splits.len()
            + self.last.steering.len()
            + 8
            + self.last.pace.len()
            + 8
            + self.last.dash.len()
            + camera.len()
            + clock.len()
            + self.last.flow.len()
            + self.last.ngp_sound.len()
            + self.last.gearbox.len()
            + menu.len()
            + self.last.fmod_block.len()
            + self.last.carried.len()
            + 8
            + CONTROL_BYTES
            + 8
            + 32 * self.last.scenery.len();
        let mut out = Vec::with_capacity(20 + CONTROL_BYTES + self.last.body.len() + records);
        for word in [MAGIC, pre, pre + substeps, substeps, CAR_BYTES as u32] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(payload);
        out.extend_from_slice(&self.last.body);
        out.extend_from_slice(&self.last.hud);
        out.extend_from_slice(&self.last.damage);
        out.extend_from_slice(&self.last.tyres);
        out.extend_from_slice(&self.last.events);
        out.extend_from_slice(&self.last.detached);
        records::write_record(&mut out, SND1, &self.last.sound);
        records::write_record(&mut out, PCN1, &self.last.pace);
        out.extend_from_slice(&self.last.splits);
        records::write_record(&mut out, DSH1, &self.last.dash);
        out.extend_from_slice(&camera);
        out.extend_from_slice(&clock);
        out.extend_from_slice(&self.last.flow);
        out.extend_from_slice(&self.last.steering);
        out.extend_from_slice(&self.last.ngp_sound);
        out.extend_from_slice(&self.last.gearbox);
        out.extend_from_slice(&menu);
        out.extend_from_slice(&self.last.fmod_block);
        out.extend_from_slice(&self.last.carried);
        records::write_record(&mut out, records::CTL1, &self.control);
        out.extend_from_slice(&records::SCN1);
        out.extend_from_slice(&(self.last.scenery.len() as u32).to_le_bytes());
        for pose in &self.last.scenery {
            out.extend_from_slice(&pose.handle.to_le_bytes());
            for value in pose.position.into_iter().chain(pose.orientation) {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }

    /// The last answer's PTC1 and PEV1 records (`events_out`), and whether the run has stopped.
    pub fn particle_records(&self) -> ([&[u8]; 2], bool) {
        ([&self.last.tyres, &self.last.events], self.stopped)
    }

    /// The gameplay loop.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// The pure physics' memory after the last step.
    pub fn mem(&self) -> &Mem {
        &self.playback.pure.mem
    }

    /// The step clock (CLK1): the physics step and the load settle's steps.
    pub fn step_clock(&self) -> StepClock {
        self.clock
    }

    /// The step the next payload feeds.
    pub fn next_step(&self) -> u32 {
        self.start + self.answered
    }

    /// Makes each payload a game frame of `seconds` of wall time (the viewer's payload period), which the frame clock
    /// splits into whole steps of `dt`. Without it a payload is one step, as the autopilot and the recorded checks want.
    pub fn pace(&mut self, seconds: f64) {
        self.frame_seconds = seconds;
    }

    /// `--dispatch direct` (the default) or `pure`: the pure host's Mem-direct dispatch ([`rbr_hybrid::pure::Pure::direct`]).
    pub fn set_direct(&mut self, on: bool) {
        self.playback.pure.direct = on;
    }

    /// The player's stage record and personal best (`--saves`), compared in the split record and written at the end.
    /// A profile written at the race's end that had no file holds the install's new-profile grid ([`Grid::loaded`]).
    /// Under NGP the profile's input filter grid replaces the settings' as `E20ReadProfile` reads it
    /// ([`Grid::from_profile`]); the 1.02 path keeps the settings' (ours, [`new_game`]).
    pub fn set_saves(&mut self, mut saves: Saves) {
        match saves.filter() {
            Some(filter) if self.game.controls() == ControlPath::Ngp => {
                if let Some(grid) = Grid::from_profile(&filter) {
                    self.game.set_filter_grid(grid);
                }
            }
            Some(_) => {}
            None => saves.set_new_filter(Grid::loaded(&install_filter_ini()).to_profile()),
        }
        self.saves = Some(saves);
    }

    /// The start clock's time of day at the stage's start, s after midnight (`rbr_gameplay::weather::start_clock`); each
    /// frame adds its time.
    pub fn set_start_clock(&mut self, seconds: f32) {
        self.time_of_day = Some(seconds);
    }

    /// What follows the championship stage (`--rally-next`).
    pub fn set_rally_next(&mut self, next: RallyNext) {
        self.rally_next = next;
    }

    /// Whether the player's records are kept (`--saves`).
    pub fn has_saves(&self) -> bool {
        self.saves.is_some()
    }

    /// Whether a step stopped the run (the answers repeat the last good frame since).
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// One frame: the payload's control through the gameplay loop, and the pure physics on the snapshot's memory.
    /// Returns the steps it ran: 0 for a frame of the countdown.
    fn step(&mut self, control: Control) -> Result<u32, String> {
        let substeps = self.frame(control)?;
        if substeps > 0 {
            self.picture();
        }
        Ok(substeps)
    }

    /// [`Live::step`] without the picture: the frame's events are kept for the next [`Live::picture`] (a real-time
    /// answer stands for the frames of its 7 ms).
    fn frame(&mut self, control: Control) -> Result<u32, String> {
        let own = self.own_substeps.filter(|_| self.answered == 0);
        let (word6, periods) = match control {
            Control::Recorded(_, word6, periods) => (word6, periods),
            Control::Input(input) => (0, u32::from(input.frames.max(1))),
        };
        let seconds = self.frame_seconds * f64::from(periods);
        self.now += seconds;
        let mut physics = Pure {
            playback: &self.playback,
            trace: None,
            track: &self.stage.track,
            own_step: own.is_some(),
            word6,
            substeps: own.unwrap_or(1),
            tyre_max: self.pending.tyre_max,
            twin: None,
        };
        // The viewer's call for help goes through the game's gate, as the pause menu's does.
        let input = match control {
            Control::Recorded(control, ..) => FrameInput {
                replay_control: Some(QuantizedControl { flags: control.flags & !CALL_FOR_HELP, ..control }),
                replay_tail: Some(QuantizedControl { flags: 0, ..control }),
                help_call: control.flags & CALL_FOR_HELP != 0,
                acknowledge: true,
                confirm: true,
                ..FrameInput::default()
            },
            Control::Input(input) => FrameInput {
                raw: poll(&input, self.game.controls(), &self.playback.pure.mem),
                help_call: input.flags & CALL_FOR_HELP != 0,
                acknowledge: input.flags & ACCEPT_MESSAGE != 0,
                confirm: input.flags & ACCEPT_MESSAGE != 0,
                retire: input.flags & RETIRE != 0,
                ..FrameInput::default()
            },
        };
        let runs = self.playback.pure.runs();
        let events = panic::catch_unwind(AssertUnwindSafe(|| self.game.frame(self.now, input, &mut physics)))
            .map_err(|e| self.stop_text(e.as_ref(), runs))?;
        self.pending.tyre_max = physics.tyre_max;
        self.time_of_day = self.time_of_day.map(|t| t + seconds as f32);
        if events.substeps.is_empty() && events.tick_state == State::Countdown {
            self.waited += 1;
            // The start board's clock runs during the start orbit.
            self.last.dash = self.dash((None, None), &[]);
            if events.stage.state() == State::Driving {
                eprintln!(
                    "step {}: the driving starts after {} countdown frames",
                    self.start + self.answered,
                    self.waited
                );
            }
            return Ok(0);
        }
        let step = self.start + self.answered;
        if events.substeps.is_empty() && (events.tick_state == State::Menu || events.flow.end.is_some()) {
            // A message box's pause: no step; the box's accept ends the race. The pause menu's retire ends it too,
            // with no step.
            if let Some(end) = events.flow.end {
                self.race_end(step, &end);
                self.last.splits = split_record(&self.game, self.stage, self.saves.as_ref()).to_bytes();
            }
            self.last.flow = self.flow_record(&events.flow);
            return Ok(0);
        }
        if events.substeps.is_empty() {
            return Err(format!("step {step}: Game ran no substeps"));
        }
        let substeps = events.substeps.len() as u32;
        if let Some(control) = events.substeps.iter().rev().find_map(|s| s.control) {
            for (bytes, word) in self.control.chunks_exact_mut(4).zip(control.words()) {
                bytes.copy_from_slice(&word.unwrap_or(0).to_le_bytes());
            }
        }
        for code in events.substeps.iter().filter_map(|s| s.outside_assistance) {
            eprintln!("step {step}: outside assistance, code {code}");
        }
        if events.flow.finish_line {
            eprintln!("step {step}: the finish line, race time {} s", events.race_clock.time);
        }
        if events.flow.helicopter_called {
            eprintln!("step {step}: the rescue helicopter is called");
        }
        if let Some(message) = events.flow.message {
            eprintln!("step {step}: the message box {} is up, the game waits for its accept", message.key());
        }
        if let Some(end) = events.flow.end {
            self.race_end(step, &end);
        }
        if let Some(penalty) = events.clock_events.penalty {
            self.penalties += penalty;
            eprintln!("step {step}: penalty {penalty} s, race time {} s", events.race_clock.time);
        }
        let pending = &mut self.pending;
        pending.substeps += substeps;
        pending.marker_events.extend(events.marker_events);
        pending.race_clock = Some(events.race_clock);
        let (flow, new) = (&mut pending.flow, events.flow);
        flow.end = flow.end.or(new.end);
        flow.finish_line |= new.finish_line;
        flow.rally_incident |= new.rally_incident;
        flow.helicopter_called |= new.helicopter_called;
        flow.message = flow.message.or(new.message);
        Ok(substeps)
    }

    /// The answer's records after the frames since the last picture that stepped: the car, its events and each
    /// wheel's last tyre payload.
    fn picture(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        let Some(race_clock) = pending.race_clock else { return };
        let mut tyres = self.playback.pure.take_tyre_particles();
        if pending.substeps > 1 {
            tyres = last_per_wheel(tyres);
        }
        let splits = split_record(&self.game, self.stage, self.saves.as_ref());
        // The wheel frame finish's damper reset, which the physics step does not run: emit each strut's payload
        // as a typed event and reset its words, so SND1 carries the frame's payloads as the original's signal
        // traffic does.
        let struts = {
            let mem = &self.playback.pure.mem;
            let car = mem.u32(CAR_PTR);
            std::array::from_fn(|k| {
                mem.u32(car + rbr_recording::sound::CAR_WHEELS + 4 * k as u32) + rbr_recording::sound::WHEEL_STRUT
            })
        };
        self.playback.pure.reset_strut_dampers(struts);
        let dampers = damper_frame(&self.playback.pure.mem, &self.playback.pure.take_damper_signals());
        self.last = Picture::of(
            &self.playback.pure.mem,
            race_hud(race_clock, self.penalties),
            &splits,
            self.stage.interactive_handle_end,
            &tyres,
            &self.detach,
            dampers,
        );
        self.last.sound = with_fires(&self.last.sound, self.playback.pure.take_engine_fires(), pending.tyre_max);
        self.last.pace = pacenote::to_bytes(&callouts(&pending.marker_events, self.game.markers()));
        self.last.events = particle_events(&self.playback.pure);
        let signals = self.playback.pure.take_hud_signals();
        let pace = pace_callouts(&pending.marker_events);
        self.last.flow = self.flow_record(&pending.flow);
        self.last.dash = self.dash(signals, &pace);
    }

    /// The end page the race's end opens (`CApplication::M00480810`): `Test3B20` on the car as it is now, and the
    /// rally's last stage from `--rally-next`.
    fn end_page(&self, end: &StageEnd) -> EndPage {
        let drivable = rally::drivable(&self.playback.pure.mem);
        EndPage::of(self.game.flow().mode(), end, drivable, self.rally_next == RallyNext::End)
    }

    /// The race's end: the log, and a finished race's times offered to the stage record and the personal best
    /// (`M00567330`, with `--saves`).
    fn race_end(&mut self, step: u32, end: &StageEnd) {
        let page = self.end_page(end);
        eprintln!("step {step}: the race ended: {:?}, {:?}, end page {:#x}", end.cause, end.end_race, page.value());
        let Some(saves) = self.saves.as_mut().filter(|s| end.finished() && !s.written()) else { return };
        let times = [end.end_race.splits[0], end.end_race.splits[1], end.end_race.time];
        match saves.finish(times) {
            Ok(update) => {
                let standing = saves.standing();
                eprintln!(
                    "step {step}: new stage record {}, new personal best {}; the record is {:.2} s by {:?}, the personal best {:?}",
                    update.record,
                    update.personal,
                    standing.record.time,
                    standing.record.holder,
                    standing.personal.map(|p| p.time),
                );
            }
            Err(e) => eprintln!("step {step}: the records were not saved: {e}"),
        }
    }

    /// Where the race stands for the viewer's pause menu (MNU1).
    fn menu(&self) -> Menu {
        let flow = self.game.flow();
        let state = self.game.stage().state();
        let driving = state == State::Driving && flow.end().is_none() && flow.message().is_none();
        Menu {
            mode: flow.mode().value(),
            driving,
            help: driving && self.game.help_offered(),
            over: state == State::Ended,
        }
    }

    /// The FLW1 record of a frame: its events, and where the flow stands after it.
    fn flow_record(&self, events: &FlowEvents) -> Vec<u8> {
        let flow = self.game.flow();
        let tints = flow.tints();
        Flow {
            helicopter_called: events.helicopter_called,
            rally_incident: events.rally_incident,
            finish_line: events.finish_line,
            ended: events.end.is_some(),
            page: events.end.map_or(0, |end| self.end_page(&end).value()),
            helicopter_active: flow.helicopter_active(),
            message: flow.message().map(|m| match m {
                Message::RetireBystand => MessageBox::RetireBystand,
                Message::RetireOffstage => MessageBox::RetireOffstage,
            }),
            service_next: self.rally_next == RallyNext::Service,
            red: tints.red,
            black: tints.black,
        }
        .to_bytes()
    }

    /// The dash record now (`dash`), with the step's signals and callouts.
    fn dash(&self, signals: (Option<[u32; 2]>, Option<u32>), pace: &[&PaceCallout]) -> Vec<u8> {
        let jumped = self.game.race_clock().jumped;
        dash(
            &self.playback.pure.mem,
            &self.stage.track,
            &self.track,
            self.game.markers().keys(),
            signals,
            pace,
            self.game.assist_settings().false_start_warning(jumped),
            self.time_of_day,
        )
        .to_bytes()
    }

    /// Where a step's panic stopped the run: the step, the reason, the call chain and the bindings run in the step.
    fn stop_text(&self, e: &(dyn Any + Send), runs: u64) -> String {
        format!(
            "step {}: {} in {}, after {} binding runs of the step",
            self.start + self.answered,
            why(e),
            chain(&self.playback.pure),
            self.playback.pure.runs() - runs
        )
    }
}

/// What the frames since the last picture emitted: their steps, callouts and flow events, the race clock after the
/// last and the tyre terms' fold.
#[derive(Default)]
struct Pending {
    substeps: u32,
    marker_events: Vec<MarkerEvent>,
    flow: FlowEvents,
    race_clock: Option<RaceClock>,
    tyre_max: TyreMax,
}

/// A payload's control: the viewer's device poll, or a recorded car control (its quantized bytes and word 6).
#[derive(Clone, Copy)]
enum Control {
    Input(Input),
    /// The control, the payload's word 6 and the payload periods it stands for.
    Recorded(QuantizedControl, u32, u32),
}

/// The poll (`USInputManagerImpl::Tic`, 0x4cbf50) of the viewer's input: the values as the devices give them and the
/// filter's cell per channel; the call for help goes to the game's gate instead (`FrameInput::help_call`). NGP's poll
/// (`0x1003c450`) writes a selected gear as the gear plus one, which its handler takes off again, and with the automatic
/// gearbox off (the car's +0x8a0) adds its direct gear rule ([`ngp_direct_gears`]) over the gear actions.
fn poll(input: &Input, controls: ControlPath, mem: &Mem) -> Raw {
    let flags = u32::from(input.flags & !(CALL_FOR_HELP | ACCEPT_MESSAGE | RETIRE));
    let (flags, gear) = match controls {
        ControlPath::Ngp => {
            let (flags, gear) = (flags, input.gear.saturating_add(1));
            let car = mem.u32(CAR_PTR);
            // The car's VCU scratch (car +0x850, `0x100420f0`), its +0x50 (`0x1005dd30`).
            if input.gears_bound == 0 || car == 0 || mem.u32(car + 0x850 + 0x50) != 0 {
                (flags, gear)
            } else {
                let actions = std::array::from_fn(|k| {
                    let bit = 1 << k;
                    (input.gears_bound & bit != 0).then_some(if input.gears_held & bit != 0 { 1.0 } else { 0.0 })
                });
                // The gear in use the last sensor data reported: the drive state's +0x54c (car +0x4b0, its +8).
                ngp_direct_gears(flags, gear, actions, mem.u32(mem.u32(car + 0x4b0 + 8) + 0x54c))
            }
        }
        _ => (flags, input.gear),
    };
    Raw {
        flags,
        aux: u32::from(input.aux),
        byte2: 0,
        channels: [input.steering, input.throttle, input.brake, input.handbrake, input.clutch],
        gear,
        select: input.cells.map(u32::from),
    }
}

/// The game's `PC_InputFilter.ini` (empty without one: an RSF install renames it, so the built-in values apply).
fn install_filter_ini() -> Ini {
    let text = Data::from_env()
        .and_then(|data| ["PC_InputFilter.ini", "Root/PC_InputFilter.ini"].into_iter().find_map(|f| data.resolve(f)))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    Ini::parse(&text)
}

/// The game a live run steps: `stage` from `start`, with the input filter of the game's `PC_InputFilter.ini` and NGP's
/// control path; a profile's grid replaces it ([`Live::set_saves`]).
fn new_game(start: Stage, stage: &StageData, dt: f32, settings: MarkerSettings) -> Game {
    let values = stage.track.iter().map(|r| r.value).collect();
    let grid = Grid::from_settings(&install_filter_ini());
    Game::new(start, grid, &stage.markers, values, dt, 0.0, settings)
        .with_finish_zone(stage.finish_zone)
        .with_controls(ControlPath::Ngp)
}

#[cfg(test)]
mod tests;

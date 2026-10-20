//! The game flow of a stage race: the flow object's words the race reads (`CGameflow`, E60 0x566150–0x56cbb0, the
//! object at `*0x165fa90`), the gates on them, and the end of the race: the finish, the retire, and the `CSigEndRace`
//! that ends it. Pure: the host gives the frame's `dt`, the application state, the HUD's race clock, the marker events
//! and the retire codes the car emitted; [`RaceFlow::frame`] returns the next value and the stage's end when it comes.
//! `rbr-gameplay/README.md` (git 9c28c0b8) ("Game flow") says what is traced and what is ours.
//!
//! Original: the flow word (+0) is the game mode, which `E59Host90::M00565430` (0x565430) sets with its sub-state
//! (+4). The race end is the HUD's: the finish line's marker (0x16, `E12_AnimSet::Load3b0` 0x4783b0) stops the clock
//! (+0x144) and arms a 20 s timer (`E62GameMenu::M573530`, 0x573530, HUD +0x2c0/+0x2c4); the HUD tick (0x579df0) runs
//! the timer down and emits `CSigEndRace` with the state word 0 when it runs out. The race flow's `CSigRetireCar`
//! receiver (`E60Flow::M5662c0`, 0x5662c0) does not end the race itself: code 1 and 2 call the rescue helicopter
//! (`*0x893638`, a `CPC_RescueHelicopter`, slot 0x18 with 150.0), whose `Tic` (0x49c170) posts `MSG_RETIRE_BYSTAND`
//! after 24 s (0x56a120); code 3 is the rally incident (HUD +0x27c, 0x577550), after which the HUD tick calls the
//! helicopter when the assistance gate (0x5665f0) is shut; code 4 arms HUD +0x280 (0x573950), after which the HUD tick
//! posts `MSG_RETIRE_OFFSTAGE`. The HUD tick also ends the race at 1800 s of race time while the helicopter is idle.
//!
//! A post (`M0058b060`) queues a message box with a callback and emits `CSigMenuActivate` with 0, whose receiver in the
//! driving state (`CApplication::M0047f9c0`, 0x47f9c0) opens the pause menu's page and sets the application state 2:
//! the car is no longer stepped and the HUD's timers wait. The box keeps the menu's input until the player accepts it
//! (`M0058a170`, 0x58a170, the accept button calls the box's callback); the callbacks (0x5693c0, 0x5797e0) emit
//! `CSigEndRace` with 1, a retire, whose receiver opens the end page (state 9). [`RaceFlow::acknowledge`] is the
//! accept.

// The comparisons follow the asm's, so a NaN takes the original's branch: `!(a > b)` is not `a <= b`.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::markers::MarkerEvent;
use crate::race_clock::{AssistSettings, RaceClock};
use crate::stage::{Stage, State};

/// The flow word (`*0x165fa90` +0): the game mode. The values the stage race tests; the others are the menus' modes
/// (2 the Richard Burns Challenge, 4-6 and 8 the multiplayer ones, 9 the season's start), which are out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode(pub u32);

impl Mode {
    /// 0: no race (the menus).
    pub const MENU: Mode = Mode(0);
    /// 1: the quick rally, the boot's (`E95_GameStub` and the replay manager set 1).
    pub const QUICK_RALLY: Mode = Mode(1);
    /// 3: the championship (the season's rally, `E72_Season` sets 3).
    pub const CHAMPIONSHIP: Mode = Mode(3);
    /// 7: the rally school (`0x566240`).
    pub const SCHOOL: Mode = Mode(7);

    pub fn value(self) -> u32 {
        self.0
    }

    /// The sub-state (+4) `M00565430` stores with the mode: 3 for the championship, 11 for any other race mode. For
    /// mode 0 it is 11 unless the season daemon's `M005d2d20` holds (`season_open`), which keeps `previous`.
    pub fn sub_state(self, season_open: bool, previous: u32) -> u32 {
        match self.0 {
            0 if season_open => previous,
            0 => 0xb,
            3 => 3,
            _ => 0xb,
        }
    }

    /// `E59Host90::M00565e80` (0x565e80): the modes 1-5 and 8, in which the finish line goes to the HUD's finish (the
    /// clock stops and the 20 s timer runs) instead of ending the race at once.
    pub fn finish_to_menu(self) -> bool {
        matches!(self.0, 1..=5 | 8)
    }

    /// `0x566240`: the rally school.
    pub fn school(self) -> bool {
        self == Mode::SCHOOL
    }

    /// `E15CamHost::M8840` (0x488840): the start orbit gives the go in modes 1 and 2 only (or in a replay, not
    /// modelled); in the others it turns on at 0.3 rad/s and the player's confirm starts the countdown
    /// ([`Self::confirm_arms`]).
    pub fn camera_go(self) -> bool {
        matches!(self.0, 1 | 2)
    }

    /// `CApplication::M0047f9c0` case 10 (0x47fc25): in modes 3 to 5 the player's `CSigMenuActivate` in the countdown
    /// state arms the countdown at 2.0 s ([`crate::stage::Stage::confirm`]).
    pub fn confirm_arms(self) -> bool {
        matches!(self.0, 3..=5)
    }
}

/// The flow object's host (+0xc98) when it holds a saved rally: its first half-word is 0x7b and the gates read the
/// mode (+0x17850) and the rally's assistance level (+0x17858) from it instead of the flow word and the season daemon.
/// This is the seam for the game's saves (a later task); the reference runs have the half-word 0, which is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedRally {
    pub mode: Mode,
    pub rally_level: u32,
}

/// `E60Flow::M566640` and its twin `M5665f0` (0x566640/0x5665f0), the assistance gate: open unless the mode is the
/// championship with the rally's assistance level (`*0x7d6128` +0x8e84, or the saved rally's) at 2 or more. The car's
/// stuck test (`Update9AF0`), the script box (`E34_RetireA`) and the rally incident read it.
pub fn assistance_gate(settings: AssistSettings, saved: Option<SavedRally>) -> bool {
    let (mode, level) = match saved {
        Some(s) => (s.mode, s.rally_level),
        None => (Mode(settings.mode), settings.rally_level),
    };
    mode != Mode::CHAMPIONSHIP || level < 2
}

/// The finish timer `M573530` arms (HUD +0x2c0, the f32 20.0).
pub const FINISH_DELAY: f32 = 20.0;
/// The race time at which the HUD tick ends the race (the f32 at 0x746828).
pub const TIME_LIMIT: f32 = 1800.0;
/// The rally incident's timer after a retire code 3 (HUD +0x27c, set by 0x577550) and its rate (the f32 at 0x739cb8).
const INCIDENT_TIME: f32 = 1.0;
const INCIDENT_RATE: f32 = 0.98;
/// The off-stage timer after a retire code 4 (HUD +0x280, set by 0x573950).
const OFFSTAGE_TIME: f32 = 1.0;
/// Both timers' value at the HUD reset (0x5772e0, the f32 at 0x795e20): spent, so nothing fires.
const TIMER_IDLE: f32 = -1.0;
/// The helicopter's time on the stage before it posts the bystander's retire (`CRescueHelicopter::Tic`, 0x49c170).
pub const HELICOPTER_TIME: f32 = 24.0;

/// The finish zone's centre and radius without a `.dls` table 8 (`E12_AnimSet::Load020`, 0x47a492: the f32s at
/// 0x72e238, 0x72e234, 0x72b154, and the squared radius 9.0 at 0x72e230).
const DEFAULT_ZONE: [f32; 4] = [2473.0, -3261.0, 10.0, 3.0];
/// `E12_Rep::CbB50`'s speed factor to km/h (the f32 at 0x72dcd8) and its limit (0x795e14).
const KMH: f32 = 3.6;
const STOP_KMH: f32 = 1.0;

/// The stage's finish zone (`*0x7eabac`, `E12_Rep`): a sphere a car that stops in ends the race finished. Retail
/// stages (61 of the 86 `.dls` have the table) place it 66 to 203 m of driveline past the finish line and up to 93 m
/// before the end-of-stage key (0x18; 4 m past it on maps 71, 74 and 80), radius 10 to 30 m, so it ends a race the car
/// stops in after the finish line before the 20 s finish timer does.
///
/// Original: `E12_AnimSet::Load020` (0x47a419) copies the centre from the `.dls` table 8's first three floats and
/// squares the fourth (`rbr_formats::dls::DlsFile::world_vector`), and clears the fired flag (+0x34). The
/// application's constructor connects `E12_Rep::CbB50` (0x478b50) to every state's slot of `CSigCarGeneralState`
/// (signal manager +0x2e10, the list at +0x3018), which `CCar::Vf28x` (0x4ea1b2) emits once a frame with the car's
/// position (+0x140) and velocity (+0x1c0), before it emits the driveline parameter the marker ticker takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FinishZone {
    centre: [f32; 3],
    radius_squared: f32,
    fired: bool,
}

impl FinishZone {
    /// `Load020`'s zone from the `.dls` table 8 words, or its default without the table.
    pub fn load(world_vector: Option<[u32; 4]>) -> Self {
        let [x, y, z, radius] = world_vector.map_or(DEFAULT_ZONE, |w| w.map(f32::from_bits));
        Self { centre: [x, y, z], radius_squared: radius * radius, fired: false }
    }

    /// `CbB50` for the car at `position` with `speed`, the length of its velocity (`0x473b30`, `sqrt((x² + y²) + z²)`
    /// in f32): whether it fires `CSigEndRace` with 0 now. It fires once: the speed times 3.6 at most 1.0, and the
    /// squared distance (`(dz² + dy²) + dx²`, zone less car) at most the squared radius, all in f32 (the product on the
    /// x87 at 24 bits); a NaN fails both.
    pub fn check(&mut self, position: [f32; 3], speed: f32) -> bool {
        if self.fired || !(speed * KMH <= STOP_KMH) {
            return false;
        }
        let d = std::array::from_fn::<f32, 3, _>(|i| self.centre[i] - position[i]);
        if !((d[2] * d[2] + d[1] * d[1]) + d[0] * d[0] <= self.radius_squared) {
            return false;
        }
        self.fired = true;
        true
    }
}

/// A message box the race posts (`M0058b060`), which pauses the game until the player accepts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// `MSG_RETIRE_BYSTAND` (0x56a120): the helicopter's 24 s are over. Its callback is 0x5693c0.
    RetireBystand,
    /// `MSG_RETIRE_OFFSTAGE` (the HUD tick, 0x579ea2): a retire code 4's second is over. Its callback is 0x5797e0.
    RetireOffstage,
}

impl Message {
    /// The string table's key of the box's text.
    pub fn key(self) -> &'static str {
        match self {
            Message::RetireBystand => "MSG_RETIRE_BYSTAND",
            Message::RetireOffstage => "MSG_RETIRE_OFFSTAGE",
        }
    }

    /// The end the box's callback gives: `CSigEndRace` with 1.
    pub fn cause(self) -> EndCause {
        match self {
            Message::RetireBystand => EndCause::Bystander,
            Message::RetireOffstage => EndCause::Offstage,
        }
    }
}

/// The code `CSigRetireCar` carries, as the race flow's receiver (`M5662c0`) dispatches it. The emitters:
/// `CCar::Update9AF0` 1 (its flag word +0x1668) and 4 (stuck against a brake wall with the assistance gate shut),
/// `E34_RetireA` 3 (a script character's box).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retire {
    /// 0: the HUD's `M5735a0`, which returns at once.
    Nothing,
    /// 1 and 2: the rescue helicopter is called.
    Helicopter,
    /// 3: the rally incident (HUD +0x27c, and `CSigRallyIncident` when the assistance gate is shut).
    Incident,
    /// 4: off the stage (HUD +0x280).
    Offstage,
}

impl Retire {
    /// The receiver's switch on the code; any other code does nothing.
    pub fn from_code(code: u32) -> Self {
        match code {
            1 | 2 => Retire::Helicopter,
            3 => Retire::Incident,
            4 => Retire::Offstage,
            _ => Retire::Nothing,
        }
    }
}

/// The `CSigEndRace` payload (0x14 bytes): the HUD's race time (+0x140), its split times (+0x258, +0x25c) and the
/// state word at +0x10. The application's receiver (`CApplication::M00480810`, 0x480810) takes the time as the result
/// when the word is 0 and a retire (DNF) when it is 1. The two message callbacks (0x5693c0, 0x5797e0) leave the times
/// uninitialised (the stack's), which the receiver does not read for a retire; here they are the HUD's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndRace {
    pub time: f32,
    pub splits: [f32; 2],
    pub retired: bool,
}

/// What ended the race.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndCause {
    /// The finish timer ran out, [`FINISH_DELAY`] after the finish line (modes 1-5 and 8).
    Finish,
    /// A marker's own `CSigEndRace`: the end of the stage (0x18), or the finish line outside modes 1-5 and 8 (0x16).
    Marker,
    /// The race time reached [`TIME_LIMIT`] with the helicopter idle.
    TimeLimit,
    /// The helicopter's [`HELICOPTER_TIME`] ran out and the player accepted `MSG_RETIRE_BYSTAND`.
    Bystander,
    /// A retire code 4's timer ran out and the player accepted `MSG_RETIRE_OFFSTAGE`.
    Offstage,
    /// A rally's fourth false start (`M579430`): the player is excluded.
    Excluded,
    /// The pause menu's retire, confirmed (`rbr_gameplay::pause_menu`: 0x56e8a0, 0x56e820).
    Retired,
    /// The car stood (1 km/h or less) in the stage's finish zone ([`FinishZone`]): `CSigEndRace` with 0.
    FinishZone,
    /// The replay buffer is full (`TicPhysicsRecording`, `crate::game::ReplayRuns`): `CSigEndRace` with 1 and zero
    /// times.
    ReplayFull,
    /// Ours: the race was left before its end (the viewer closed, the launcher's rally page): see [`RaceFlow::end_now`].
    Left,
}

/// The end of the stage: what ended it and the `CSigEndRace` it emitted. The stage result (`result::StageResult`)
/// takes its outcome from [`StageEnd::finished`] and its time from the payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageEnd {
    pub cause: EndCause,
    pub end_race: EndRace,
}

impl StageEnd {
    /// The race was finished (state word 0), not given up.
    pub fn finished(&self) -> bool {
        !self.end_race.retired
    }
}

/// The page `CApplication::M00480810` (0x480810) opens on `CSigEndRace`: the mode it gives `M0047f3a0`, which sets the
/// application state 9 and opens that `CGameMenuDaemon` page (the page table `Init` 0x56f390 fills). Modes 0 and 1
/// (the quick rally) and 3 (the championship) are traced; the others' pages are out of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndPage {
    /// 0x10: the quick rally's result (built by 0x59b7a0); the finish handed it the HUD's splits and race time
    /// (`M573530`'s menu call).
    QuickResult,
    /// 0x12: the quick rally's ENDRACE page after a retire (0x59c170: REPLAY, SVREPLAY, EXIT2MM, RESTART).
    QuickRetired,
    /// 0x13: the season's stage standings (`CSeasonDropInUI`, 0x5a8670: POS, NAME, SPLIT_D, TIME, SS; CONTINUE goes on
    /// in the rally). A finished stage whose car can still drive (`Test3B20` non-zero), or the rally's last stage.
    RallyStandings,
    /// 0x18: FAILED (0x56b0d0; CONTINUE opens 0x17). A finished stage before the last with a car that can't drive
    /// (`Test3B20` 0): the time counts, and `E80Daemon::M005d0960` retires the stages after it (`FillNeg` from the next
    /// stage), which ends the rally.
    RallyWrecked,
    /// 0x17: FAILED after a retire (0x56b270: CONTINUE, REPLAY, SVREPLAY); `M005d09b0` retires this stage and the
    /// later ones.
    RallyRetired,
    /// Another mode's page (the receiver's `M0047f3a0` mode): the Challenge 0xe/0xf, multiplayer 0x21/0x22, 0x1c, 0x1d.
    Other(u32),
}

impl EndPage {
    /// The receiver's switch on the flow word (`M00565470`) outside the replay (state 8). `drivable` is `Test3B20` on
    /// the car at the end; `last_stage` is the season's stage index (+0x8e8c) at 5.
    pub fn of(mode: Mode, end: &StageEnd, drivable: bool, last_stage: bool) -> Self {
        let finished = end.finished();
        match mode.0 {
            0 | 1 if finished => EndPage::QuickResult,
            0 | 1 => EndPage::QuickRetired,
            3 if !finished => EndPage::RallyRetired,
            3 if !drivable && !last_stage => EndPage::RallyWrecked,
            3 => EndPage::RallyStandings,
            2 => EndPage::Other(if finished { 0xe } else { 0xf }),
            4 => EndPage::Other(0x21),
            5 if finished && drivable => EndPage::Other(0x13),
            5 => EndPage::Other(0x22),
            6 => EndPage::Other(0x1c),
            9 => EndPage::Other(0x1d),
            n => EndPage::Other(n),
        }
    }

    /// The `M0047f3a0` mode (the page's index).
    pub fn value(self) -> u32 {
        match self {
            EndPage::QuickResult => 0x10,
            EndPage::QuickRetired => 0x12,
            EndPage::RallyStandings => 0x13,
            EndPage::RallyWrecked => 0x18,
            EndPage::RallyRetired => 0x17,
            EndPage::Other(n) => n,
        }
    }
}

/// The stage's phase, as the flow and the application state ([`Stage`]) give it together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// The quick race's set-up and the load (application states 5 and 0xd).
    Load,
    /// The countdown (application state 10).
    Countdown,
    /// Driving, the race on.
    Racing,
    /// Past the finish line: the clock stopped, the car still driven, the finish timer running.
    Finished { left: f32 },
    /// A retire is under way: the helicopter is on the stage, or the off-stage message is due.
    Retiring,
    /// A retire's message box is up (the application state 2) until the player accepts it.
    Message(Message),
    /// The race is over.
    Over(StageEnd),
}

/// What a frame of the flow is given: the HUD tick's inputs and what the race emitted this frame.
#[derive(Debug, Clone, Copy)]
pub struct FlowFrame<'a> {
    /// The frame's `facc`, the HUD tick's `dt`.
    pub dt: f32,
    /// The application state the HUD tick sees.
    pub state: State,
    /// The race clock when the marker ticker ran (the split's value, HUD +0x140), and after this frame's tick.
    pub clock_at_markers: f32,
    pub clock: RaceClock,
    pub markers: &'a [MarkerEvent],
    /// The `CSigRetireCar` codes the frame's substeps emitted, in order.
    pub retires: &'a [u32],
    /// This frame's jump start excluded the player from the rally (`AssistSettings::excluded`).
    pub excluded: bool,
}

/// The HUD's full-screen tints' alphas ([`RaceFlow::tints`]), `None` when not drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tints {
    pub red: Option<f32>,
    pub black: Option<f32>,
}

/// What a frame of the flow emitted besides the end.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FlowEvents {
    /// The stage ended this frame.
    pub end: Option<StageEnd>,
    /// The finish line was passed this frame (the clock stopped, the finish timer armed).
    pub finish_line: bool,
    /// `CSigRallyIncident` (a retire code 3 with the assistance gate shut; the HUD and the sound take it).
    pub rally_incident: bool,
    /// The rescue helicopter was called (`CPC_RescueHelicopter`'s slot 0x18 with 150.0; presentation).
    pub helicopter_called: bool,
    /// A message box was posted this frame: the game pauses (state 2) until [`RaceFlow::acknowledge`].
    pub message: Option<Message>,
}

/// The race flow of one stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaceFlow {
    settings: AssistSettings,
    saved: Option<SavedRally>,
    /// The finish timer (HUD +0x2c0), armed (+0x2c4) when `Some`.
    finish: Option<f32>,
    /// The rally incident's timer (HUD +0x27c) and the off-stage timer (+0x280).
    incident: f32,
    offstage: f32,
    /// The helicopter's time on the stage (+0x14) while it is active.
    helicopter: Option<f32>,
    /// The message box that is up, until the player accepts it.
    message: Option<Message>,
    /// The split times (HUD +0x258...) and their count (+0x254).
    splits: [f32; 3],
    split_count: usize,
    end: Option<StageEnd>,
}

impl RaceFlow {
    /// The flow at the HUD reset (0x5772e0) for a race in `settings`' mode.
    pub fn new(settings: AssistSettings) -> Self {
        Self {
            settings,
            saved: None,
            finish: None,
            incident: TIMER_IDLE,
            offstage: TIMER_IDLE,
            helicopter: None,
            message: None,
            splits: [0.0; 3],
            split_count: 0,
            end: None,
        }
    }

    pub fn mode(&self) -> Mode {
        Mode(self.settings.mode)
    }

    pub fn set_settings(&mut self, settings: AssistSettings) {
        self.settings = settings;
    }

    /// The saved rally the gates read instead of the settings (the save seam).
    pub fn set_saved(&mut self, saved: Option<SavedRally>) {
        self.saved = saved;
    }

    /// The assistance gate in this race ([`assistance_gate`]).
    pub fn assistance_gate(&self) -> bool {
        assistance_gate(self.settings, self.saved)
    }

    /// How the stage ended, once it has.
    pub fn end(&self) -> Option<StageEnd> {
        self.end
    }

    /// The message box that is up (the game is paused), if any.
    pub fn message(&self) -> Option<Message> {
        self.message
    }

    /// Whether the rescue helicopter is on the stage (`IsActive`, 0x49bea0): `HUD::Update` (0x578c30) draws nothing
    /// while it is.
    pub fn helicopter_active(&self) -> bool {
        self.helicopter.is_some()
    }

    /// The HUD's full-screen tints (`HUD::Update`'s 0x573630 and 0x5736b0): the red of the rally incident (+0x27c) and
    /// the black of the off-stage fade (+0x280), each drawn over the 641x482 screen with the alpha `1 - t` while its
    /// timer `t` is not below 0, and nothing while the helicopter is on the stage.
    pub fn tints(&self) -> Tints {
        let alpha = |t: f32| (!(t < 0.0) && !self.helicopter_active()).then_some(1.0 - t);
        Tints { red: alpha(self.incident), black: alpha(self.offstage) }
    }

    /// The player accepts the message box: its callback emits `CSigEndRace` with 1 (a retire) with the race clock
    /// `clock`. `None` when no box is up.
    pub fn acknowledge(&mut self, clock: &RaceClock) -> Option<StageEnd> {
        let message = self.message.take()?;
        Some(self.end_race(message.cause(), clock, true))
    }

    /// The player confirms the pause menu's retire: the box's callback (0x56e8a0 in the quick rally, 0x56e820 in the
    /// championship) emits `CSigEndRace` with 1 with the race clock `clock`. `None` when the race is over or a box is
    /// up.
    pub fn retire_from_menu(&mut self, clock: &RaceClock) -> Option<StageEnd> {
        (self.end.is_none() && self.message.is_none()).then(|| self.end_race(EndCause::Retired, clock, true))
    }

    /// The finish zone's `CSigEndRace` with 0 ([`FinishZone::check`]), with the race clock `clock` and the splits.
    /// `None` when the race is over or a box is up.
    pub fn finish_zone(&mut self, clock: &RaceClock) -> Option<StageEnd> {
        (self.end.is_none() && self.message.is_none()).then(|| self.end_race(EndCause::FinishZone, clock, false))
    }

    /// The full replay buffer's `CSigEndRace` with 1 (`TicPhysicsRecording`, 0x47f4fb): its times are zero.
    pub fn replay_full(&mut self) -> Option<StageEnd> {
        (self.end.is_none() && self.message.is_none()).then(|| {
            let end = StageEnd {
                cause: EndCause::ReplayFull,
                end_race: EndRace { time: 0.0, splits: [0.0; 2], retired: true },
            };
            self.end = Some(end);
            end
        })
    }

    /// The race is still on: not over, and the finish line not passed. A run that stops at the finish stops here.
    pub fn racing(&self) -> bool {
        self.end.is_none() && self.finish.is_none()
    }

    /// The stage's end if the race is left now with the race clock `clock`: the flow's end once it came. Before it
    /// (ours: the game's pause-menu exit leaves no result), a finish when the finish line was passed (the clock is
    /// stopped there) and a retire otherwise, both [`EndCause::Left`].
    pub fn end_now(&self, clock: &RaceClock) -> StageEnd {
        self.end.unwrap_or(StageEnd {
            cause: EndCause::Left,
            end_race: EndRace {
                time: clock.time,
                splits: [self.splits[0], self.splits[1]],
                retired: self.finish.is_none(),
            },
        })
    }

    /// The HUD's split times so far.
    pub fn splits(&self) -> &[f32] {
        &self.splits[..self.split_count]
    }

    /// The phase with the application state `stage`.
    pub fn phase(&self, stage: Stage) -> Phase {
        if let Some(end) = self.end {
            return Phase::Over(end);
        }
        if let Some(message) = self.message {
            return Phase::Message(message);
        }
        match stage.state() {
            State::Setup | State::Load => Phase::Load,
            State::Countdown => Phase::Countdown,
            _ => match self.finish {
                Some(left) => Phase::Finished { left },
                None if self.helicopter.is_some() || self.offstage >= 0.0 => Phase::Retiring,
                None => Phase::Racing,
            },
        }
    }

    /// The race flow's `CSigRetireCar` receiver (`M5662c0`), inside the physics step. In the rally school it only sets
    /// the replay's filter flag (`CReplayManager::Set48_1`).
    fn retire(&mut self, code: u32, events: &mut FlowEvents) {
        if self.mode().school() {
            return;
        }
        match Retire::from_code(code) {
            Retire::Nothing => {}
            Retire::Helicopter => self.call_helicopter(events),
            Retire::Incident => {
                // `HUD::EmitRallyIncident` (0x577550): the timer, and the signal when the gate is shut.
                self.incident = INCIDENT_TIME;
                events.rally_incident |= !self.assistance_gate();
            }
            Retire::Offstage => self.offstage = OFFSTAGE_TIME,
        }
    }

    /// `*0x893638`'s slot 0x18: `CPC_RescueHelicopter::Vf18` (0x452290) passes its argument (150.0) to
    /// `CRescueHelicopter::Activate` (0x49bc90), which does not read it, and places the helicopter on the driveline 30 m
    /// before the car with its time (+0x14) at 0; then it loads the helicopter's character (`load_rescuechopper.lua`,
    /// `run_rescuechopper.lua`, the `Stretcher`). So a call while it is on the stage starts its 24 s again.
    fn call_helicopter(&mut self, events: &mut FlowEvents) {
        events.helicopter_called = true;
        self.helicopter = Some(0.0);
    }

    /// `M0058b060` and `CSigMenuActivate`: the box is up and the game pauses.
    fn post(&mut self, message: Message, events: &mut FlowEvents) {
        self.message = Some(message);
        events.message = Some(message);
    }

    fn end_race(&mut self, cause: EndCause, clock: &RaceClock, retired: bool) -> StageEnd {
        let end = StageEnd {
            cause,
            end_race: EndRace { time: clock.time, splits: [self.splits[0], self.splits[1]], retired },
        };
        self.end = Some(end);
        end
    }

    /// One frame after the HUD's race clock ticked, in the original's order: the retire receiver (inside the frame's
    /// steps), the marker events (`Load3b0`, after `Tic`), then the HUD tick's timers (0x579df0): the incident, the
    /// off-stage message, the finish timer, the time limit; then the helicopter's `Tic` (ours: its place in the frame
    /// is not traced). The first `CSigEndRace` ends the race; nothing runs after it.
    pub fn frame(&mut self, input: FlowFrame) -> FlowEvents {
        let mut events = FlowEvents::default();
        if self.end.is_some() || self.message.is_some() {
            return events;
        }
        for &code in input.retires {
            self.retire(code, &mut events);
        }
        let clock = &input.clock;
        for event in input.markers {
            match event {
                MarkerEvent::SplitHostCall { .. } => {
                    // `E12_GameHost::M72da0` (0x572da0): the split takes the race clock's word (+0x140).
                    if let Some(split) = self.splits.get_mut(self.split_count) {
                        *split = input.clock_at_markers;
                        self.split_count += 1;
                    }
                }
                MarkerEvent::FinishMenu => {
                    // `M573530`: the finish timer (the clock's stop word is `RaceClock`'s).
                    self.finish = Some(FINISH_DELAY);
                    events.finish_line = true;
                }
                MarkerEvent::EndRace { state, .. } => {
                    events.end = Some(self.end_race(EndCause::Marker, clock, *state != 0));
                    return events;
                }
                _ => {}
            }
        }
        // The jump start's own `CSigEndRace` (`M579430`, from the start line's key or the moving start): retired. Its
        // payload's other words are the original's uninitialised stack; a retire has no time.
        if input.excluded {
            events.end = Some(self.end_race(EndCause::Excluded, clock, true));
            return events;
        }
        if input.state != State::Driving {
            return events;
        }
        let dt = input.dt;

        // +0x27c: `t -= 0.98 dt`; on the frame it passes 0, the helicopter when the gate is shut.
        let step = dt * INCIDENT_RATE;
        self.incident -= step;
        if 0.0 > self.incident && !(self.incident + step < 0.0) && !self.assistance_gate() {
            self.call_helicopter(&mut events);
        }
        // +0x280: on the frame it passes 0, `MSG_RETIRE_OFFSTAGE`. The pause (state 2) stops the rest of the tick's
        // timers: the finish timer and the time limit run outside it only.
        self.offstage -= dt;
        if 0.0 > self.offstage && !(self.offstage + dt < 0.0) {
            self.post(Message::RetireOffstage, &mut events);
            return events;
        }
        // +0x2c0/+0x2c4: outside the application states 2 and 9.
        if let Some(left) = self.finish {
            let left = left - dt;
            self.finish = Some(left);
            if 0.0 > left {
                self.finish = None;
                events.end = Some(self.end_race(EndCause::Finish, clock, false));
                return events;
            }
        }
        // The time limit, in the application states 1 and 8, while the helicopter is idle (`IsActive`, 0x49bea0).
        if !(clock.time < TIME_LIMIT) && self.helicopter.is_none() {
            events.end = Some(self.end_race(EndCause::TimeLimit, clock, true));
            return events;
        }
        // `CRescueHelicopter::Tic`: its time; at 24 s it leaves (`Deactivate`) and posts `MSG_RETIRE_BYSTAND`.
        if let Some(time) = self.helicopter {
            let time = time + dt;
            self.helicopter = Some(time);
            if time >= HELICOPTER_TIME {
                self.helicopter = None;
                self.post(Message::RetireBystand, &mut events);
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markers::EndRaceContext;

    const DT: f32 = 0.007;
    const CHAMPIONSHIP_HARD: AssistSettings = AssistSettings { mode: 3, rally_level: 2, level: 1, false_starts: 0 };

    fn driving() -> Stage {
        Stage::driving(54, 6)
    }

    fn frame<'a>(clock: RaceClock, markers: &'a [MarkerEvent], retires: &'a [u32]) -> FlowFrame<'a> {
        FlowFrame {
            dt: DT,
            state: State::Driving,
            clock_at_markers: clock.time,
            clock,
            markers,
            retires,
            excluded: false,
        }
    }

    fn clock(time: f32) -> RaceClock {
        RaceClock { time, running: true, ..RaceClock::default() }
    }

    /// Runs empty frames until the flow ends, at most `limit`; the frame count and the end.
    fn run(flow: &mut RaceFlow, time: f32, limit: usize) -> Option<(usize, StageEnd)> {
        (1..=limit).find_map(|n| flow.frame(frame(clock(time), &[], &[])).end.map(|end| (n, end)))
    }

    /// Runs empty frames until the flow posts a message box, at most `limit`; the frame count and the box.
    fn run_to_message(flow: &mut RaceFlow, time: f32, limit: usize) -> Option<(usize, Message)> {
        (1..=limit).find_map(|n| flow.frame(frame(clock(time), &[], &[])).message.map(|m| (n, m)))
    }

    /// The box is up: nothing runs until the player accepts it, which retires with the box's cause.
    fn accept(flow: &mut RaceFlow, message: Message) -> StageEnd {
        assert_eq!(flow.phase(driving()), Phase::Message(message));
        assert_eq!(run(flow, TIME_LIMIT, 5000), None, "the pause holds every timer");
        let end = flow.acknowledge(&clock(10.0)).expect("a box is up");
        assert_eq!((end.cause, end.finished()), (message.cause(), false));
        assert_eq!(flow.acknowledge(&clock(10.0)), None);
        end
    }

    /// `E12_Rep::CbB50`: 1 km/h or less, inside the sphere (3D, the boundary included), once; `Load020`'s default zone
    /// without a table 8.
    #[test]
    fn cbb50_the_zone_fires_once_for_a_stopped_car_inside() {
        let words = [100.0f32, 200.0, 50.0, 10.0].map(f32::to_bits);
        let at = |dx: f32, dz: f32| [100.0 + dx, 200.0, 50.0 + dz];
        let stop = 1.0 / 3.6;
        assert!(!FinishZone::load(Some(words)).check(at(0.0, 0.0), 0.3));
        assert!(!FinishZone::load(Some(words)).check(at(6.0, 8.01), 0.0));
        assert!(!FinishZone::load(Some(words)).check(at(0.0, 0.0), f32::NAN));
        assert!(FinishZone::load(Some(words)).check(at(6.0, 8.0), stop));
        let mut zone = FinishZone::load(Some(words));
        assert!(zone.check(at(0.0, 0.0), 0.0));
        assert!(!zone.check(at(0.0, 0.0), 0.0));
        assert!(FinishZone::load(None).check([2473.0, -3261.0, 13.0], 0.0));
        assert!(!FinishZone::load(None).check([2473.0, -3261.0, 13.01], 0.0));
    }

    /// The zone's end is a finish with the race clock and the splits; nothing ends after it.
    #[test]
    fn the_finish_zone_ends_finished_and_the_full_replay_retires_with_zero_times() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        flow.frame(FlowFrame {
            clock_at_markers: 30.0,
            ..frame(clock(30.0), &[MarkerEvent::SplitHostCall { value: 0 }], &[])
        });
        flow.frame(frame(clock(90.0), &[MarkerEvent::FinishMenu], &[]));
        let end = flow.finish_zone(&clock(90.0)).expect("the zone ends it");
        assert_eq!(
            (end.cause, end.finished(), end.end_race.time, end.end_race.splits),
            (EndCause::FinishZone, true, 90.0, [30.0, 0.0])
        );
        assert_eq!(flow.replay_full(), None);
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        let end = flow.replay_full().expect("the full buffer ends it");
        assert_eq!(end.end_race, EndRace { time: 0.0, splits: [0.0; 2], retired: true });
        assert_eq!(flow.finish_zone(&clock(1.0)), None);
    }

    #[test]
    fn m8840_and_m0047f9c0_split_the_go_by_mode() {
        assert!(Mode::QUICK_RALLY.camera_go() && Mode(2).camera_go());
        assert!(!Mode::CHAMPIONSHIP.camera_go() && !Mode(4).camera_go() && !Mode::SCHOOL.camera_go());
        assert!((3..=5).all(|m| Mode(m).confirm_arms()));
        assert!(![0, 1, 2, 6, 7, 8].iter().any(|&m| Mode(m).confirm_arms()));
    }

    #[test]
    fn the_gates_follow_the_modes() {
        assert!(Mode::QUICK_RALLY.finish_to_menu());
        assert!(Mode::CHAMPIONSHIP.finish_to_menu());
        assert!(Mode(8).finish_to_menu());
        assert!(!Mode::MENU.finish_to_menu());
        assert!(!Mode::SCHOOL.finish_to_menu());
        assert!(!Mode(6).finish_to_menu());
        assert!(assistance_gate(AssistSettings::QUICK_RACE, None));
        assert!(assistance_gate(AssistSettings { rally_level: 1, ..CHAMPIONSHIP_HARD }, None));
        assert!(!assistance_gate(CHAMPIONSHIP_HARD, None));
        assert!(!assistance_gate(AssistSettings::QUICK_RACE, Some(SavedRally { mode: Mode(3), rally_level: 2 })));
        assert_eq!(Mode::CHAMPIONSHIP.sub_state(false, 0), 3);
        assert_eq!(Mode::QUICK_RALLY.sub_state(false, 0), 0xb);
        assert_eq!(Mode::MENU.sub_state(true, 7), 7);
        assert_eq!(Mode::MENU.sub_state(false, 7), 0xb);
    }

    /// The quick race's mode is the boot's flow word: physics reads it, so it must stay 1.
    #[test]
    fn the_quick_race_is_mode_one() {
        assert_eq!(Mode(AssistSettings::QUICK_RACE.mode), Mode::QUICK_RALLY);
        assert_eq!(Mode::QUICK_RALLY.value(), 1);
    }

    #[test]
    fn the_finish_line_stops_the_race_and_the_timer_ends_it_20_s_later() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        assert_eq!(flow.phase(driving()), Phase::Racing);
        let events = flow.frame(frame(clock(100.0), &[MarkerEvent::FinishMenu], &[]));
        assert!(events.finish_line && events.end.is_none());
        assert!(!flow.racing());
        assert!(matches!(flow.phase(driving()), Phase::Finished { left } if left == FINISH_DELAY - DT));
        let (frames, end) = run(&mut flow, 100.0, 4000).expect("the finish timer ends the race");
        // 20 s of 0.007 s frames in f32: the frame that takes it below 0.
        let mut left = FINISH_DELAY - DT;
        let mut expected = 0;
        while !(0.0 > left) {
            left -= DT;
            expected += 1;
        }
        assert_eq!(frames, expected);
        assert_eq!(end.cause, EndCause::Finish);
        assert!(end.finished());
        assert_eq!(end.end_race.time, 100.0);
        assert!(matches!(flow.phase(driving()), Phase::Over(e) if e == end));
        assert_eq!(flow.frame(frame(clock(100.0), &[], &[])), FlowEvents::default());
    }

    #[test]
    fn a_marker_end_race_ends_at_once() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        let markers = [MarkerEvent::EndRace { context: EndRaceContext::default(), state: 0 }];
        let end = flow.frame(frame(clock(50.0), &markers, &[])).end.expect("ends");
        assert_eq!(end.cause, EndCause::Marker);
        assert!(end.finished());
    }

    #[test]
    fn splits_take_the_clock_at_the_marker() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        let split = [MarkerEvent::SplitHostCall { value: 0 }];
        flow.frame(FlowFrame { clock_at_markers: 30.0, ..frame(clock(30.007), &split, &[]) });
        flow.frame(FlowFrame { clock_at_markers: 60.0, ..frame(clock(60.007), &split, &[]) });
        assert_eq!(flow.splits(), [30.0, 60.0]);
        let markers = [MarkerEvent::EndRace { context: EndRaceContext::default(), state: 0 }];
        let end = flow.frame(frame(clock(90.0), &markers, &[])).end.unwrap();
        assert_eq!(end.end_race.splits, [30.0, 60.0]);
    }

    #[test]
    fn an_exclusion_retires_after_the_markers() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        let split = [MarkerEvent::SplitHostCall { value: 0 }];
        let end = flow.frame(FlowFrame { excluded: true, ..frame(clock(12.0), &split, &[]) }).end.expect("excluded");
        assert_eq!((end.cause, end.finished()), (EndCause::Excluded, false));
        assert_eq!(flow.splits(), [12.0]);
    }

    #[test]
    fn the_time_limit_retires() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        assert_eq!(flow.frame(frame(clock(1799.9), &[], &[])).end, None);
        let end = flow.frame(frame(clock(TIME_LIMIT), &[], &[])).end.expect("the limit");
        assert_eq!(end.cause, EndCause::TimeLimit);
        assert!(!end.finished());
    }

    #[test]
    fn retire_codes_one_and_two_call_the_helicopter_which_ends_the_race_24_s_later() {
        for code in [1, 2] {
            let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
            let events = flow.frame(frame(clock(10.0), &[], &[code]));
            assert!(events.helicopter_called);
            assert_eq!(flow.phase(driving()), Phase::Retiring);
            let (frames, message) = run_to_message(&mut flow, 10.0, 5000).expect("the bystander retire");
            assert_eq!(message, Message::RetireBystand);
            // 24 s of frames, the first in the calling frame.
            assert!((3427..=3430).contains(&(frames + 1)), "{frames}");
            accept(&mut flow, message);
        }
    }

    #[test]
    fn the_helicopter_holds_off_the_time_limit() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        flow.frame(frame(clock(1700.0), &[], &[1]));
        assert_eq!(flow.frame(frame(clock(TIME_LIMIT), &[], &[])).end, None);
    }

    #[test]
    fn retire_code_three_is_an_incident_that_ends_nothing_with_the_gate_open() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        let events = flow.frame(frame(clock(10.0), &[], &[3]));
        assert!(!events.rally_incident && !events.helicopter_called);
        assert_eq!(run(&mut flow, 10.0, 1000), None);
        assert_eq!(flow.phase(driving()), Phase::Racing);
    }

    #[test]
    fn retire_code_three_with_the_gate_shut_calls_the_helicopter_after_the_incident() {
        let mut flow = RaceFlow::new(CHAMPIONSHIP_HARD);
        let events = flow.frame(frame(clock(10.0), &[], &[3]));
        assert!(events.rally_incident && !events.helicopter_called);
        // 1.0 at 0.98 dt a frame: the frame it passes 0.
        let called = (1..200).find(|_| flow.frame(frame(clock(10.0), &[], &[])).helicopter_called);
        assert_eq!(called, Some(145));
        let (_, message) = run_to_message(&mut flow, 10.0, 5000).expect("the bystander retire");
        assert_eq!(accept(&mut flow, message).cause, EndCause::Bystander);
    }

    /// `Activate` sets the helicopter's time to 0 on each call.
    #[test]
    fn a_second_call_starts_the_helicopter_again() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        flow.frame(frame(clock(10.0), &[], &[1]));
        let (first, _) = {
            let mut probe = flow;
            run_to_message(&mut probe, 10.0, 5000).unwrap()
        };
        for _ in 0..1000 {
            flow.frame(frame(clock(10.0), &[], &[]));
        }
        assert!(flow.frame(frame(clock(10.0), &[], &[2])).helicopter_called);
        let (second, _) = run_to_message(&mut flow, 10.0, 5000).unwrap();
        assert_eq!(second, first);
    }

    #[test]
    fn the_tints_follow_their_timers() {
        let mut flow = RaceFlow::new(CHAMPIONSHIP_HARD);
        assert_eq!(flow.tints(), Tints::default());
        flow.frame(frame(clock(10.0), &[], &[3]));
        let red = flow.tints().red.expect("the incident's red");
        assert!((red - DT * INCIDENT_RATE).abs() < 1e-6, "{red}");
        assert_eq!(flow.tints().black, None);
        // The helicopter hides the HUD and its tints.
        while !flow.frame(frame(clock(10.0), &[], &[])).helicopter_called {}
        assert_eq!(flow.tints(), Tints::default());
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        flow.frame(frame(clock(10.0), &[], &[4]));
        assert!(flow.tints().black.is_some_and(|a| (a - DT).abs() < 1e-6));
    }

    #[test]
    fn retire_code_four_ends_the_race_off_stage_a_second_later() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        assert_eq!(flow.frame(frame(clock(10.0), &[], &[4])).end, None);
        let (frames, message) = run_to_message(&mut flow, 10.0, 1000).expect("off stage");
        assert_eq!(message, Message::RetireOffstage);
        assert_eq!(frames + 1, 143);
        assert_eq!(accept(&mut flow, message).cause, EndCause::Offstage);
    }

    #[test]
    fn the_end_page_follows_the_receiver() {
        let end =
            |retired| StageEnd { cause: EndCause::Finish, end_race: EndRace { time: 1.0, splits: [0.0; 2], retired } };
        let (done, out) = (end(false), end(true));
        assert_eq!(EndPage::of(Mode::QUICK_RALLY, &done, false, false), EndPage::QuickResult);
        assert_eq!(EndPage::of(Mode::QUICK_RALLY, &out, true, false), EndPage::QuickRetired);
        assert_eq!(EndPage::of(Mode::CHAMPIONSHIP, &done, true, false), EndPage::RallyStandings);
        assert_eq!(EndPage::of(Mode::CHAMPIONSHIP, &done, false, false), EndPage::RallyWrecked);
        assert_eq!(EndPage::of(Mode::CHAMPIONSHIP, &done, false, true), EndPage::RallyStandings);
        assert_eq!(EndPage::of(Mode::CHAMPIONSHIP, &out, true, false), EndPage::RallyRetired);
        assert_eq!(
            [EndPage::QuickResult, EndPage::QuickRetired, EndPage::RallyStandings, EndPage::RallyWrecked]
                .map(EndPage::value),
            [0x10, 0x12, 0x13, 0x18]
        );
        assert_eq!(EndPage::RallyRetired.value(), 0x17);
    }

    #[test]
    fn leaving_the_race_finishes_past_the_finish_line_and_retires_before() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        let left = flow.end_now(&clock(30.0));
        assert_eq!((left.cause, left.finished(), left.end_race.time), (EndCause::Left, false, 30.0));
        flow.frame(frame(clock(60.0), &[MarkerEvent::FinishMenu], &[]));
        assert!(flow.end_now(&clock(60.0)).finished());
        let (_, end) = run(&mut flow, 60.0, 4000).unwrap();
        assert_eq!(flow.end_now(&clock(99.0)), end);
    }

    #[test]
    fn other_codes_and_the_school_do_nothing() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        assert_eq!(flow.frame(frame(clock(10.0), &[], &[0, 5])), FlowEvents::default());
        let mut school = RaceFlow::new(AssistSettings { mode: 7, ..AssistSettings::QUICK_RACE });
        assert_eq!(school.frame(frame(clock(10.0), &[], &[1, 4])), FlowEvents::default());
        assert_eq!(run(&mut school, 10.0, 5000), None);
    }

    #[test]
    fn the_timers_wait_outside_driving_and_phases_follow_the_stage() {
        let mut flow = RaceFlow::new(AssistSettings::QUICK_RACE);
        flow.frame(frame(clock(10.0), &[MarkerEvent::FinishMenu], &[]));
        let menu = FlowFrame { state: State::Menu, ..frame(clock(10.0), &[], &[]) };
        for _ in 0..5000 {
            assert_eq!(flow.frame(menu).end, None);
        }
        let fresh = RaceFlow::new(AssistSettings::QUICK_RACE);
        let stage = Stage::quick_race(54, 6);
        assert_eq!(fresh.phase(stage), Phase::Load);
        assert_eq!(fresh.phase(stage.load_ready()), Phase::Load);
        assert_eq!(fresh.phase(stage.load_ready().load_tick(150)), Phase::Countdown);
    }
}

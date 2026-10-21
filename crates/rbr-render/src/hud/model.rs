//! The HUD's state over time, fed with each live answer's records: the pace-note symbol and its queue, the warning
//! lamps, the stand-still and car-problem messages and the false-start banner, and the car's damage ([`CarStatus`]).
//! Pure; a [`super::Hud`] draws what [`HudModel::view`] gives.
//!
//! Faithful (traced in the 1.02 exe):
//! - the pace-note queue of `E62GameMenu::M576580` (0x576580, the `CSigPaceNoteCallout` receiver), `HUD::Update`
//!   (0x578c30) and the HUD tick: 8 notes, each shown at least [`NOTE_SECONDS`], a 0.5 s fade in, and a fade out
//!   once the car is within [`NOTE_PASSED`] of the shown note; only the tags with a symbol (`E12_AllowTag`, 0x474b70);
//! - the lamps' table at 0x7d1900 (mode, lit and flashing thresholds) over the values the `CSigCarSensorData`
//!   receiver (0x573cc0) copies, flashing at 1 Hz;
//! - the message codes of the `CSigCarStandStill` (0x5735b0, counts above 5) and `CSigCarProblem` (0x573740)
//!   receivers, shown for [`MESSAGE_SECONDS`];
//! - "FALSE START" for [`FALSE_START_SECONDS`] after a jump start (`HUD::PollInput` 0x579430 sets +0x238 to 3);
//! - the rally's false-start warning (`FSWAR_1` to `FSWAR_3`, 0x573100): HUD +0x148 the count with this one and
//!   +0x14c [`WARNING_SECONDS`] after a jump start in a rally, the panel fading in its last 2 s;
//! - "WRONG WAY" (0x577590): the `CSigCarGeneralState` receiver's test (0x579590, `rbr_gameplay::driveline::wrong_way`)
//!   held for [`WRONG_WAY_WAIT`], then a fade in over [`WRONG_WAY_FADE`] (+0x154..+0x164, the tick 0x579df0); it goes at
//!   once when the test fails;
//! - the odometer and the distance to go (HUD +0x20 and +0x28, 0x5715b0: the driveline distance, and the finish key's
//!   distance less it and the start key's);
//! - the pace stack (`PN_STACK`, 0 to 4, default 0) of `HUD::Update`: the symbol behind the car fading out over the
//!   first half of the way to the next, the next ones at 0.3 alpha, the first brightening to 0.5 and the last fading in
//!   (`pace_stack`); the 3D notes (`AID_MARKER`, default off) draw the next four;
//! - the HUD is not drawn while the rescue helicopter flies (`HUD::Update` returns, `CRescueHelicopter::IsActive`).
//!
//! The time base is the game's frame time: each live answer is one frame of one step ([`STEP_SECONDS`],
//! `rbr_render::live`). Ours: the fade of the messages.

use rbr_gameplay::car_damage::{level, CarriedDamage};
use rbr_gameplay::service::Rules;
use rbr_recording::dash::{Dash, StackKey};
use rbr_recording::flow::{Flow, MessageBox};
use rbr_recording::pacenote::Callout;

use super::pace::{self, Symbol};

/// Frame time per live answer period, s (7 ms, `rbr_recording::live`).
pub const STEP_SECONDS: f32 = rbr_recording::STEP_SECONDS;
/// How long a queued note shows before the next one (HUD +0x1f4, 1.5 s).
pub const NOTE_SECONDS: f32 = 1.5;
/// The shown note fades once the car is this close to it, m (`HUD::Update`, 20.0).
pub const NOTE_PASSED: f32 = 20.0;
/// The fade's time, s (HUD +0x290, 0.5).
const FADE_SECONDS: f32 = 0.5;
/// Below this the faded note is dropped (0.01).
const NOTE_GONE: f32 = 0.01;
/// The queue's length (HUD +0x1b0, 8 notes).
const QUEUE: usize = 8;
/// How long a message shows after its signal (HUD +0x2d0, 4.0).
pub const MESSAGE_SECONDS: f32 = 4.0;
/// How long "FALSE START" shows (HUD +0x238, 3.0).
pub const FALSE_START_SECONDS: f32 = 3.0;
/// How long the rally's false-start warning shows (HUD +0x14c, 10.0), and the last part it fades in (2.0).
pub const WARNING_SECONDS: f32 = 10.0;
const WARNING_FADE: f32 = 2.0;
/// The wrong-way test holds this long before the banner fades in (+0x164 against 2.0), and the fade's length (3.0).
pub const WRONG_WAY_WAIT: f32 = 2.0;
pub const WRONG_WAY_FADE: f32 = 3.0;
/// The stack's alphas (`HUD::Update`: 0.3, the first's 0.2 more by the way to it, the passed one's 0.5 less).
const STACK_ALPHA: f32 = 0.3;
const STACK_FIRST: f32 = 0.2;
const STACK_PASSED: f32 = 0.5;
/// The most notes the stack shows (`PN_STACK`'s slider, 0 to 4).
pub const STACK_MAX: u8 = 4;

/// A pace note as the HUD shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub symbol: Symbol,
    /// The key's flag word (the co-driver's modifiers).
    pub flags: u32,
    /// The called distance to the next note, m (`E12_SeekD`, 0: none).
    pub called: f32,
    /// The key's driveline distance, m, when the record gives it.
    pub at: Option<f32>,
}

/// How a lamp reads its value (the table's mode word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LampMode {
    /// Mode 1: lit above `lit`, flashing above `flash`.
    Above,
    /// Mode 0: lit below `lit`, flashing below `flash`.
    Below,
    /// Mode 2: lit when the value is 0.
    Zero,
}

/// One of the warning lamps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LampSpec {
    pub name: &'static str,
    pub mode: LampMode,
    pub lit: f32,
    pub flash: f32,
}

/// The lamps 0 to 4 of the table at 0x7d1900 (lamp 5, at 999 over a constant 10000, never lights). The names follow
/// the sensor words; the engine's (its option block +0x58) is inferred.
pub const LAMPS: [LampSpec; 5] = [
    LampSpec { name: "hot", mode: LampMode::Above, lit: 120.0, flash: 130.0 },
    LampSpec { name: "brakes", mode: LampMode::Below, lit: 0.8, flash: 0.5 },
    LampSpec { name: "engine", mode: LampMode::Below, lit: 0.8, flash: 0.2 },
    LampSpec { name: "battery", mode: LampMode::Below, lit: 10.0, flash: 8.0 },
    LampSpec { name: "stalled", mode: LampMode::Zero, lit: 0.0, flash: 0.0 },
];

/// A lamp's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lamp {
    Off,
    On,
    Flashing,
}

/// The lamp for `value` (the HUD tick's loop over the table).
pub fn lamp(spec: LampSpec, value: f32) -> Lamp {
    match spec.mode {
        LampMode::Above if value > spec.flash => Lamp::Flashing,
        LampMode::Above if value > spec.lit => Lamp::On,
        LampMode::Below if value < spec.flash => Lamp::Flashing,
        LampMode::Below if value < spec.lit => Lamp::On,
        LampMode::Zero if value == 0.0 => Lamp::On,
        _ => Lamp::Off,
    }
}

/// The lamps' values in [`LAMPS`] order.
pub fn lamp_values(dash: &Dash) -> [f32; 5] {
    [dash.water, dash.brakes, dash.engine, dash.battery, if dash.engine_runs { 1.0 } else { 0.0 }]
}

/// The body damage of the 64 zone values: the mean of the five worst of zones 1 to 31 but 19, the ones that dent a
/// car (`notes/renderer.md` "Damage visuals"), so one hard hit shows. Ours.
pub fn body_damage(zones: &[f32]) -> f32 {
    let mut dents: Vec<f32> = (1..32).filter(|&z| z != 19).filter_map(|z| zones.get(z).copied()).collect();
    dents.sort_by(|a, b| b.total_cmp(a));
    dents.truncate(5);
    if dents.is_empty() {
        return 0.0;
    }
    (dents.iter().sum::<f32>() / dents.len() as f32).clamp(0.0, 1.0)
}

/// The HUD's messages (HUD +0x138, 1 to 6), with the 1.02 English text (`bstrings.eng`). The original's "Press escape
/// to retire" lines are left out (the viewer has no retire key) and "escape" for assistance is the viewer's H: ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    Damaged,
    Wrecked,
    Stuck,
    Assist,
    StopFirst,
    HopelesslyStuck,
}

impl Message {
    pub fn text(self) -> &'static str {
        match self {
            Message::Damaged => "Your car is seriously damaged.",
            Message::Wrecked => "Your car is wrecked.",
            Message::Stuck => "Your car is stuck.",
            Message::Assist => "Press H for assistance.",
            Message::StopFirst => "You must stop the car\nbefore calling for help.",
            Message::HopelesslyStuck => "Your car is hopelessly stuck.\nYou must retire.",
        }
    }
}

/// What a signal does to the message: shows one, clears it, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Show(Message),
    Clear,
    Keep,
}

/// `CSigCarStandStill`'s receiver (0x5735b0): above a count of 5, its kind picks the message.
pub fn stand_still(count: u32, kind: u32) -> Change {
    if count <= 5 {
        return Change::Keep;
    }
    match kind {
        0 => Change::Show(Message::Assist),
        1 => Change::Show(Message::Damaged),
        2 => Change::Show(Message::Wrecked),
        3 => Change::Show(Message::Stuck),
        4 => Change::Show(Message::HopelesslyStuck),
        _ => Change::Keep,
    }
}

/// `CSigCarProblem`'s receiver (0x573740).
pub fn car_problem(kind: u32) -> Change {
    match kind {
        0 => Change::Clear,
        1 | 2 => Change::Show(Message::Stuck),
        3 => Change::Show(Message::HopelesslyStuck),
        4 => Change::Show(Message::StopFirst),
        _ => Change::Keep,
    }
}

/// The note fade (HUD +0x284 start, +0x288 rate, +0x28c time, +0x290 length).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Fade {
    start: f32,
    rate: f32,
    time: f32,
    length: f32,
}

impl Fade {
    fn alpha(&self) -> f32 {
        self.start + self.rate * self.time
    }

    /// A new fade from the current alpha to `to` over [`FADE_SECONDS`].
    fn toward(self, to: f32) -> Fade {
        let start = self.alpha();
        Fade { start, rate: (to - start) / FADE_SECONDS, time: 0.0, length: FADE_SECONDS }
    }

    fn advance(mut self, dt: f32) -> Fade {
        self.time += dt;
        if self.time > self.length {
            self = Fade { start: self.start + self.length * self.rate, rate: 0.0, ..self };
        }
        self
    }
}

/// What the HUD shows now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudView {
    /// The shown note and its alpha.
    pub note: Option<(Note, f32)>,
    /// The car's distance to the shown note, m.
    pub note_distance: Option<f32>,
    pub lamps: Option<[Lamp; 5]>,
    /// The flashing lamps' phase: lit or not.
    pub flash_on: bool,
    /// The message and its alpha.
    pub message: Option<(Message, f32)>,
    /// "FALSE START" and its alpha.
    pub false_start: Option<f32>,
    /// The car's progress from the start to the finish, 0 to 1, and the splits' (the HUD's hidden bar, 0x571e80).
    pub progress: Option<(f32, Vec<f32>)>,
    /// The turbo pressure, Pa, and the water temperature, degrees C (the digidash's).
    pub boost: Option<f32>,
    pub water: Option<f32>,
    /// The body damage, 0 to 1: the mean of the damage zones that dent a body (ours; the game's HUD shows none).
    pub body: Option<f32>,
    /// "WRONG WAY" and its alpha.
    pub wrong_way: Option<f32>,
    /// The rally's false-start warning: the count (1 to 3) and the panel's alpha.
    pub warning: Option<(u8, f32)>,
    /// The odometer (the driveline distance, m) and the distance to go, m.
    pub odometer: Option<(f32, Option<f32>)>,
    /// The start board's clock, s after midnight.
    pub start_clock: Option<f32>,
    /// The pace stack's notes, nearest first, each with its alpha; the passed one first when it still shows.
    pub stack: Vec<StackNote>,
    /// The rescue helicopter flies: the HUD is not drawn.
    pub helicopter: bool,
    /// The race's end page (`flow::EndPage::value`), once the race is over.
    pub end_page: Option<u32>,
    /// The race's message box that is up (the game waits for its accept).
    pub message_box: Option<MessageBox>,
    /// The full-screen red of the rally incident and black of the off-stage fade, as alphas (`flow::Tints`).
    pub red: Option<f32>,
    pub black: Option<f32>,
    /// A championship's next event is a service (page 0x18's text).
    pub service_next: bool,
    /// The car's damage by group (CDM1), for the car status.
    pub car: Option<CarStatus>,
    /// The car's rev limit and its upshift point in the gear, rpm (SHF1), for the dial.
    pub rev_limit: Option<f32>,
    pub upshift: Option<f32>,
}

/// A note of the pace stack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackNote {
    pub symbol: Symbol,
    pub alpha: f32,
    /// The car is past it.
    pub passed: bool,
    /// The key's point on the driveline, physics frame.
    pub position: Option<[f32; 3]>,
    /// The way from the passed note to the next one the car has covered, 0 to 1 (1 without a passed note).
    pub fraction: f32,
}

/// `HUD::Update`'s pace stack over `keys` (DSH1's: the symbol key behind the car, when there is one, then the next
/// ones) at the car's distance `car`: the passed one while the car is in the first half of the way to the next (alpha
/// 0.5 less the fraction covered), then the next `count - 1` (the original's loop stops one short of `PN_STACK`), the
/// first at 0.3 + 0.2 of the fraction, the last of them at 0.3 times the fraction, the rest at 0.3.
pub fn pace_stack(keys: &[StackKey], car: f32, count: u8) -> Vec<StackNote> {
    let notes: Vec<(&StackKey, Symbol)> = keys.iter().filter_map(|k| Some((k, pace::symbol(k.tag)?))).collect();
    let split = notes.iter().position(|(k, _)| k.distance >= car).unwrap_or(notes.len());
    let passed = split.checked_sub(1).map(|i| notes[i]);
    let ahead = &notes[split..];
    let fraction = match (passed, ahead.first()) {
        (Some((p, _)), Some((n, _))) if n.distance > p.distance => (car - p.distance) / (n.distance - p.distance),
        _ => 1.0,
    };
    let mut out = Vec::new();
    if count == 0 {
        return out;
    }
    if let Some((key, symbol)) = passed.filter(|_| fraction < STACK_PASSED) {
        let alpha = STACK_PASSED - fraction;
        out.push(StackNote { symbol, alpha, passed: true, position: key.position, fraction });
    }
    let shown = usize::from(count.min(STACK_MAX)).saturating_sub(1);
    for (i, (key, symbol)) in ahead.iter().take(shown).enumerate() {
        let alpha = if i == 0 {
            fraction * STACK_FIRST + STACK_ALPHA
        } else if i + 2 == usize::from(count) {
            fraction * STACK_ALPHA
        } else {
            STACK_ALPHA
        };
        out.push(StackNote { symbol: *symbol, alpha, passed: false, position: key.position, fraction });
    }
    out
}

/// The odometer and the distance to go (0x5715b0): the driveline distance, and the finish key's distance less it and
/// the start key's (0 without a start key). Without a finish key the game takes the driveline's length; DSH1 has none,
/// so there is no distance to go then.
pub fn odometer(dash: &Dash) -> Option<(f32, Option<f32>)> {
    let distance = dash.distance?;
    Some((distance, dash.finish.map(|f| f - distance - dash.start.unwrap_or(0.0))))
}

/// The HUD's state.
#[derive(Debug, Clone, Default)]
pub struct HudModel {
    /// Newest first.
    queue: [Option<Note>; QUEUE],
    shown: usize,
    queue_timer: f32,
    fade: Fade,
    message: Option<(Message, f32)>,
    false_start: f32,
    jumped: bool,
    blink: f32,
    dash: Option<Dash>,
    body: Option<f32>,
    /// The wrong-way wait (+0x164) and fade (+0x154 start, +0x158 rate, +0x15c time, +0x160 length).
    wrong_way_held: f32,
    wrong_way: Fade,
    /// The false-start warning's count (+0x148) and timer (+0x14c).
    warning: (u8, f32),
    helicopter: bool,
    end_page: Option<u32>,
    /// FLW1's state: the message box, the tints and what follows a championship stage.
    message_box: Option<MessageBox>,
    tints: (Option<f32>, Option<f32>),
    service_next: bool,
    /// The pace stack's size (`PN_STACK`).
    stack: u8,
}

impl HudModel {
    /// A model whose pace stack shows `count` notes (`PN_STACK`, 0 to [`STACK_MAX`]; 0 hides it).
    pub fn with_stack(count: u8) -> HudModel {
        HudModel { stack: count.min(STACK_MAX), ..HudModel::default() }
    }

    /// A new model with the same options (a restart).
    pub fn fresh(&self) -> HudModel {
        HudModel::with_stack(self.stack)
    }

    /// Takes one answer's records: its callouts (PCN1), its dash record (DSH1), the damage zones (DMG1), whether
    /// the jump start was taken (HUD2) and the race flow's events (FLW1), then runs [`STEP_SECONDS`] of the HUD's clock.
    pub fn answer(
        &mut self,
        callouts: &[Callout],
        dash: Option<&Dash>,
        zones: Option<&[f32]>,
        jumped: bool,
        flow: Option<&Flow>,
    ) {
        self.answer_over(STEP_SECONDS, callouts, dash, zones, jumped, flow);
    }

    /// [`HudModel::answer`] for a frame of `seconds` (several payload periods: one tick of their time, as the game's
    /// frame ticks the HUD once).
    pub fn answer_over(
        &mut self,
        seconds: f32,
        callouts: &[Callout],
        dash: Option<&Dash>,
        zones: Option<&[f32]>,
        jumped: bool,
        flow: Option<&Flow>,
    ) {
        if let Some(flow) = flow {
            // A frame the game does not run (after the end) says nothing of the helicopter, which the end page hides.
            self.helicopter = flow.helicopter_active || flow.helicopter_called;
            self.message_box = flow.message;
            self.tints = (flow.red, flow.black);
            if flow.ended {
                self.end_page = Some(flow.page);
                self.service_next = flow.service_next;
            }
        }
        self.body = zones.map(body_damage);
        for (k, callout) in callouts.iter().enumerate() {
            let Some(symbol) = pace::symbol(callout.tag) else { continue };
            let at = dash.and_then(|d| d.callouts.get(k).copied());
            self.callout(Note { symbol, flags: callout.word1, called: callout.seek_d, at });
        }
        if let Some(dash) = dash {
            for change in
                [dash.still.map(|[c, k]| stand_still(c, k)), dash.problem.map(car_problem)].into_iter().flatten()
            {
                match change {
                    Change::Show(m) => self.message = Some((m, MESSAGE_SECONDS)),
                    Change::Clear => self.message = None,
                    Change::Keep => {}
                }
            }
            self.dash = Some(dash.clone());
        }
        if jumped && !self.jumped {
            self.false_start = FALSE_START_SECONDS;
            if let Some(count) = dash.map(|d| d.false_starts).filter(|&n| n > 0) {
                self.warning = (count, WARNING_SECONDS);
            }
        }
        self.wrong_way(dash.is_some_and(|d| d.wrong_way), seconds);
        self.jumped = jumped;
        self.update();
        self.tick(seconds);
    }

    /// `M576580`: the note goes in front, the shown one moves back with the queue, and the fade restarts toward 1.
    fn callout(&mut self, note: Note) {
        self.queue.rotate_right(1);
        self.queue[0] = Some(note);
        self.shown += 1;
        if self.shown >= QUEUE {
            self.shown = QUEUE - 1;
            self.queue_timer = NOTE_SECONDS;
        }
        self.fade = self.fade.toward(1.0);
    }

    /// `HUD::Update`'s queue step: the next note after [`NOTE_SECONDS`], and the fade out near the newest one.
    fn update(&mut self) {
        if self.shown > 0 && self.queue_timer <= 0.0 {
            self.shown -= 1;
            self.queue_timer = NOTE_SECONDS;
        }
        if self.shown == 0 {
            let car = self.dash.as_ref().and_then(|d| d.distance);
            if let (Some(Note { at: Some(at), .. }), Some(car)) = (self.queue[0], car) {
                if at - car <= NOTE_PASSED {
                    self.fade = self.fade.toward(0.0);
                }
            }
        }
    }

    /// The tick's wrong-way part (0x579df0): the fade runs; while the test holds, past [`WRONG_WAY_WAIT`] each
    /// finished fade restarts toward 1 over [`WRONG_WAY_FADE`]; once it fails the banner goes at once.
    fn wrong_way(&mut self, wrong: bool, dt: f32) {
        self.wrong_way = self.wrong_way.advance(dt);
        if !wrong {
            self.wrong_way_held = 0.0;
            if self.wrong_way.alpha() > NOTE_GONE {
                self.wrong_way = Fade::default();
            }
            return;
        }
        self.wrong_way_held += dt;
        if self.wrong_way_held > WRONG_WAY_WAIT && self.wrong_way.time >= self.wrong_way.length {
            let start = self.wrong_way.alpha();
            self.wrong_way = Fade { start, rate: (1.0 - start) / WRONG_WAY_FADE, time: 0.0, length: WRONG_WAY_FADE };
        }
    }

    /// The HUD tick: the fade, the dropped note, the timers and the lamps' blink.
    fn tick(&mut self, dt: f32) {
        self.warning.1 -= dt;
        if self.warning.1 < 0.001 {
            self.warning = (0, 0.0);
        }
        self.fade = self.fade.advance(dt);
        if self.fade.alpha() < NOTE_GONE {
            self.queue[0] = None;
        }
        self.queue_timer -= dt;
        self.false_start = (self.false_start - dt).max(0.0);
        self.message = self.message.and_then(|(m, t)| (t > dt).then_some((m, t - dt)));
        self.blink += dt;
        if self.blink > 1.0 {
            self.blink = 0.0;
        }
    }

    pub fn view(&self) -> HudView {
        let note = self.queue[self.shown].map(|n| (n, self.fade.alpha().clamp(0.0, 1.0)));
        let dash = self.dash.as_ref();
        let car = dash.and_then(|d| d.distance);
        let progress = dash.and_then(|d| {
            let (start, finish, car) = (d.start?, d.finish?, car?);
            let length = finish - start;
            (length > 0.0).then(|| {
                let at = |x: f32| ((x - start) / length).clamp(0.0, 1.0);
                (at(car), d.splits.iter().map(|&s| at(s)).collect())
            })
        });
        HudView {
            note,
            note_distance: note.and_then(|(n, _)| Some(n.at? - car?)),
            lamps: dash.map(|d| {
                let values = lamp_values(d);
                std::array::from_fn(|i| lamp(LAMPS[i], values[i]))
            }),
            flash_on: self.blink >= 0.5,
            message: self.message.map(|(m, t)| (m, t.min(1.0))),
            false_start: (self.false_start > 0.0).then(|| (self.false_start * 0.5).min(1.0)),
            progress,
            boost: dash.map(|d| d.boost),
            water: dash.map(|d| d.water),
            body: self.body,
            wrong_way: Some(self.wrong_way.alpha()).filter(|a| *a != 0.0).map(|a| a.min(1.0)),
            warning: (self.warning.0 > 0)
                .then(|| (self.warning.0, (self.warning.1 / WARNING_FADE).min(1.0) * STACK_PASSED)),
            odometer: dash.and_then(odometer),
            start_clock: dash.and_then(|d| d.clock),
            stack: match (dash, car) {
                (Some(d), Some(car)) => pace_stack(&d.stack, car, self.stack),
                _ => Vec::new(),
            },
            helicopter: self.helicopter,
            end_page: self.end_page,
            message_box: self.message_box,
            red: self.tints.0,
            black: self.tints.1,
            service_next: self.service_next,
            // The live process's own records, set by the viewer.
            ..HudView::default()
        }
    }
}

/// The car's systems by the service park's names (`bstrings.eng`), each with its repair parts' physics ids (`repair.ini`): the radiator, water and oil pumps, oil cooler and head gasket; the turbo, its
/// hosing and the intercooler; the gearbox, its actuator, the clutch and the three differentials; the starter, battery
/// and electrics; the hydraulics, brake circuit and the four calipers; the windscreen, bodywork, doors and their locks,
/// bonnet and its lock, exhaust and its mount, bumpers and wing.
pub const SYSTEMS: [(&str, &[u32]); 6] = [
    ("Engine", &[10, 11, 12, 13, 23]),
    ("Turbo", &[20, 21, 22]),
    ("Gear Box", &[30, 31, 32, 90, 91, 92]),
    ("Electrical System", &[33, 34, 36]),
    ("Brakes", &[35, 37, 108, 128, 148, 168]),
    ("Bodywork", &[28, 29, 40, 41, 42, 43, 44, 45, 46, 47, 50, 51, 52]),
];
/// [`SYSTEMS`]' bodywork.
pub const BODYWORK: usize = 5;
/// Each wheel's parts (id `101 + 20 * wheel + part`, wheels LF, RF, LB, RB): the damper, driveshaft, control arm,
/// steering rod or arm, top mount and hub; the wheel and the tyre.
const SUSPENSION: [u32; 6] = [0, 1, 2, 3, 9, 10];
const TYRE: [u32; 2] = [4, 5];
/// `D_FINE` to `D_LOST` (the service park's `damage_key`: the level, at most 4).
pub const LEVELS: [&str; 5] = ["Fine", "Light", "Medium", "Severe", "Lost"];

/// The car's damage for the HUD (ours; 1.02's HUD shows none): the worst level of each system, and of each wheel's
/// suspension and of its wheel and tyre, each repair part's level as the service park reads it
/// (`rbr_gameplay::car_damage::level`, 0 sound to 5) from the live process's CDM1 record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CarStatus {
    pub systems: [u8; SYSTEMS.len()],
    pub suspension: [u8; 4],
    pub tyres: [u8; 4],
}

impl CarStatus {
    /// The status of the damage `d` under `rules`.
    pub fn of(d: &CarriedDamage, rules: Rules) -> Self {
        let worst = |ids: &mut dyn Iterator<Item = u32>| ids.map(|id| level(d, id, rules)).max().unwrap_or(0);
        let wheel = |w: u32, parts: &[u32]| worst(&mut parts.iter().map(|p| 101 + 20 * w + p));
        CarStatus {
            systems: SYSTEMS.map(|(_, ids)| worst(&mut ids.iter().copied())),
            suspension: std::array::from_fn(|w| wheel(w as u32, &SUSPENSION)),
            tyres: std::array::from_fn(|w| wheel(w as u32, &TYRE)),
        }
    }

    /// The body or a wheel has damage.
    pub fn shows_car(&self) -> bool {
        self.systems[BODYWORK] > 0 || self.suspension.iter().chain(&self.tyres).any(|l| *l > 0)
    }

    /// Each damaged system, then the suspension and the tyres at their worst wheel's level.
    pub fn rows(&self) -> Vec<(&'static str, u8)> {
        let worst = |levels: [u8; 4]| levels.into_iter().max().unwrap_or(0);
        SYSTEMS
            .iter()
            .map(|(name, _)| *name)
            .zip(self.systems)
            .chain([("Suspension", worst(self.suspension)), ("Tyres", worst(self.tyres))])
            .filter(|(_, level)| *level > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callout(tag: u32) -> Callout {
        Callout { tag, word1: 0, seek_d: 50.0, gap: -1.0, next_tag: -1 }
    }

    /// The live answers (frames of one step) that cover `seconds`.
    fn answers(seconds: f32) -> usize {
        (seconds / STEP_SECONDS).ceil() as usize
    }

    fn dash(distance: f32, callouts: Vec<f32>) -> Dash {
        Dash { distance: Some(distance), callouts, engine_runs: true, ..Dash::default() }
    }

    #[test]
    fn the_lamps_follow_the_table() {
        assert_eq!(lamp(LAMPS[0], 86.8), Lamp::Off);
        assert_eq!(lamp(LAMPS[0], 125.0), Lamp::On);
        assert_eq!(lamp(LAMPS[0], 131.0), Lamp::Flashing);
        assert_eq!(lamp(LAMPS[1], 1.0), Lamp::Off);
        assert_eq!(lamp(LAMPS[1], 0.0), Lamp::Flashing);
        assert_eq!(lamp(LAMPS[3], 12.0), Lamp::Off);
        assert_eq!(lamp(LAMPS[3], 9.0), Lamp::On);
        assert_eq!(lamp(LAMPS[4], 1.0), Lamp::Off);
        assert_eq!(lamp(LAMPS[4], 0.0), Lamp::On);
    }

    #[test]
    fn messages_follow_the_receivers() {
        assert_eq!(stand_still(5, 1), Change::Keep);
        assert_eq!(stand_still(6, 1), Change::Show(Message::Damaged));
        assert_eq!(stand_still(6, 0), Change::Show(Message::Assist));
        assert_eq!(car_problem(4), Change::Show(Message::StopFirst));
        assert_eq!(car_problem(0), Change::Clear);
        let mut hud = HudModel::default();
        let still = Dash { still: Some([7, 2]), ..dash(0.0, vec![]) };
        hud.answer(&[], Some(&still), None, false, None);
        assert_eq!(hud.view().message.map(|m| m.0), Some(Message::Wrecked));
        for _ in 0..answers(4.0) {
            hud.answer(&[], Some(&dash(0.0, vec![])), None, false, None);
        }
        assert_eq!(hud.view().message, None, "gone after 4 s");
    }

    #[test]
    fn a_note_fades_in_and_out_near_its_key() {
        let mut hud = HudModel::default();
        hud.answer(&[callout(3)], Some(&dash(100.0, vec![200.0])), None, false, None);
        let (note, alpha) = hud.view().note.expect("shown");
        assert_eq!(note.symbol, pace::symbol(3).unwrap());
        assert!(alpha > 0.0 && alpha < 0.1);
        for _ in 0..answers(0.6) {
            hud.answer(&[], Some(&dash(100.0, vec![])), None, false, None);
        }
        assert_eq!(hud.view().note.unwrap().1, 1.0);
        assert_eq!(hud.view().note_distance, Some(100.0));
        for _ in 0..answers(3.0) {
            hud.answer(&[], Some(&dash(185.0, vec![])), None, false, None);
        }
        assert_eq!(hud.view().note, None, "faded and dropped within 20 m");
    }

    #[test]
    fn queued_notes_show_for_one_and_a_half_seconds_each() {
        let mut hud = HudModel::default();
        hud.answer(&[callout(3)], Some(&dash(0.0, vec![500.0])), None, false, None);
        hud.answer(&[callout(8)], Some(&dash(0.0, vec![600.0])), None, false, None);
        assert_eq!(hud.view().note.unwrap().0.symbol, pace::symbol(3).unwrap());
        for _ in 0..answers(1.6) {
            hud.answer(&[], Some(&dash(0.0, vec![])), None, false, None);
        }
        assert_eq!(hud.view().note.unwrap().0.symbol, pace::symbol(8).unwrap());
    }

    #[test]
    fn notes_without_a_symbol_are_not_shown() {
        let mut hud = HudModel::default();
        hud.answer(&[callout(0x0d), callout(0x16)], None, None, false, None);
        assert_eq!(hud.view().note, None);
    }

    #[test]
    fn a_jump_start_shows_false_start_for_three_seconds() {
        let mut hud = HudModel::default();
        hud.answer(&[], None, None, true, None);
        assert!(hud.view().false_start.is_some());
        for _ in 0..answers(3.0) {
            hud.answer(&[], None, None, true, None);
        }
        assert_eq!(hud.view().false_start, None);
    }

    #[test]
    fn a_sound_car_shows_nothing_and_damage_shows_its_group() {
        let sound = CarStatus::default();
        assert!(!sound.shows_car() && sound.rows().is_empty());
        let status = CarStatus { systems: [3, 0, 0, 0, 0, 0], suspension: [0, 2, 0, 0], tyres: [0, 0, 0, 4] };
        assert!(status.shows_car());
        assert_eq!(status.rows(), [("Engine", 3), ("Suspension", 2), ("Tyres", 4)]);
    }
}

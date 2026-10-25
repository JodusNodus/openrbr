//! A co-driver mod (`notes/sound.md` "Custom co-driver"): RSF's Pacenote plugin's way of calling the notes, with a
//! voice, a style and a range set from its folder (`rbr_formats::rsf::codriver`) and the stage's notes, its `.dls` keys
//! or a note file (`rbr_formats::rsf::pacenotes`). It plays instead of 1.02's pace notes; the countdown and the damage
//! calls stay 1.02's.
//!
//! The rules are the plugin's `PaceNoteHandler` (`PaceNote.dll`, read in its disassembly; nothing of it runs here):
//!
//! - **When.** Each frame the notes not yet called up to `x + max(0, (T + time) v + v^2 / (2 adjust) + distance)` are
//!   called in order, `x` the car's driveline distance, `v` its speed along the driveline, `T` the game's callout
//!   distance option (`0x1660b74`), `time`, `distance` and `adjust` the last `CALLOUT_*` notes' values (the `v^2` term
//!   only for `adjust` at least 0.01). The start, split, finish and end keys are not called ahead; the finish is called
//!   once the car has passed it. A setting note holds until the next spoken call unless sticky.
//! - **What.** A spoken call says its takes (one picked at random, or the note's or the last `SOUND_INDEX` note's
//!   take), then its first 1.02 modifier bit (`NARROWS`, `LONG`, ...), then a distance: from a `DISTANCE_CALL` note
//!   right after it, the distance from that note to the next call with a link, as the range set's nearest call (30 m
//!   and up); without one and unless the note is sticky, the gap to the next call, which only gives that call's link
//!   (`INTO`, `ONTO`) for 10 m and less. A distance between 10 and 30 m says `AND` before the next call.
//!
//! Ours: the speed is the car's (`Sound::speed`), not the driveline's dot product (`E12_Sampler::Dot150`); a jump back
//! of more than 5 m (a reset) calls the notes ahead again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rbr_formats::rsf::codriver::{self, Call, Calls};
use rbr_formats::rsf::pacenotes::{self, id, Note, NAMED};

use crate::data::{legacy_word, Wave};
use crate::voice::{Command, Speech};

/// The jump back that calls the notes again, metres (`0x1021ef74`).
const REWIND: f32 = 5.0;
/// The farthest the next call is looked for, metres (`0x1021eff8`).
const MAX_GAP: f32 = 1000.0;
/// At or below this gap a call says the next one's link, metres (`0x1021ef78`).
const LINK_GAP: f32 = 10.0;
/// The shortest distance call, metres (`0x1021ef9c`).
const MIN_RANGE: f32 = 30.0;

/// What a call says.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Say {
    /// Its takes: indices in the voice's samples.
    Takes(Vec<u32>),
    /// 1.02's sample of its name (`Speech::Word`).
    Legacy(u32),
    Nothing,
}

/// The voice's samples and what each call says.
#[derive(Debug, Clone, Default)]
pub struct Voice {
    pub samples: Vec<Wave>,
    /// Each sample's file name.
    pub names: Vec<String>,
    notes: HashMap<u32, Say>,
    named: HashMap<String, Say>,
    ranges: Vec<(u32, Say)>,
}

impl Voice {
    /// The calls of style `style` and range set `ranges` in the plugin's folder `dir`, said by voice `voice` (a
    /// folder under `sounds/`). A take whose file is missing or does not decode is left out; a call left without
    /// takes plays 1.02's sample of its name.
    pub fn load(dir: &Path, voice: &str, style: &str, ranges: &str) -> Result<(Voice, Calls), String> {
        let folder = codriver::voice_dir(dir, voice).ok_or_else(|| format!("{}: no voice {voice}", dir.display()))?;
        let calls = codriver::load(dir, style, ranges)?;
        let mut files: HashMap<String, u32> = HashMap::new();
        let (mut samples, mut names) = (Vec::new(), Vec::new());
        let mut say = |call: &Call, legacy: Option<u32>| {
            let takes: Vec<u32> = call
                .sounds
                .iter()
                .filter_map(|file| {
                    let key = file.to_ascii_lowercase();
                    if let Some(&k) = files.get(&key) {
                        return Some(k);
                    }
                    let path = rbr_formats::stage_files::in_dir(&folder, file)?;
                    let wave = Wave::decode_ogg(&std::fs::read(path).ok()?).ok()?;
                    samples.push(wave);
                    names.push(file.clone());
                    files.insert(key, samples.len() as u32 - 1);
                    Some(samples.len() as u32 - 1)
                })
                .collect();
            match (takes.is_empty(), legacy) {
                (false, _) => Say::Takes(takes),
                (true, Some(word)) => Say::Legacy(word),
                (true, None) => Say::Nothing,
            }
        };
        let notes = calls.notes.iter().map(|(&id, call)| (id, say(call, legacy_word(&call.name)))).collect();
        let named = calls.named.iter().map(|(name, call)| (name.clone(), say(call, legacy_word(name)))).collect();
        let ranges = calls.ranges.iter().map(|(&m, call)| (m, say(call, legacy_word(&m.to_string())))).collect();
        Ok((Voice { samples, names, notes, named, ranges }, calls))
    }
}

/// Which co-driver mod plays: the plugin's folder (`Plugins/Pacenote`), the voice (a folder under `sounds/`), the
/// style and range set used when the notes name none, and the stage's note file (`None`: the stage's `.dls` keys).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub plugin: PathBuf,
    pub voice: String,
    pub style: String,
    pub ranges: String,
    pub notes: Option<PathBuf>,
}

/// The style and range set when neither the notes nor the player name one: what RSF's notes name.
pub const DEFAULT_STYLE: &str = "Rbr.ini";
pub const DEFAULT_RANGES: &str = "Rbr.ini";

/// A fired spoken call for the HUD: what the viewer shows as the pace note while the voice says it.
#[derive(Debug, Clone, PartialEq)]
pub struct HudCall {
    /// The note's driveline distance, m (the HUD note's `at`).
    pub distance: f32,
    /// The note's flag word (the HUD's modifier bits).
    pub flags: u32,
    /// The called distance to the next note, m (the HUD's `called`; 0 when the call says none).
    pub called: f32,
    /// The call's id.
    pub id: u32,
    /// The note's 1.02 stand-in (`type` bits 0-15; the id itself without the extended-id flag).
    pub standard: u32,
    /// The call's name for a plugin-only call with no game symbol (the style's, else the plugin's own).
    pub name: Option<String>,
}

/// Whether a fired spoken call shows on the HUD: the plugin's control, file and game-flow calls (and 1.02's start,
/// split, finish and end, said outside the spoken path) show nothing. Only the pace notes do.
pub fn hud_visible(id: u32) -> bool {
    !(matches!(
        id,
        id::DISTANCE_CALL
            | id::SOUND_INDEX
            | id::CALLOUT_TIME
            | id::CALLOUT_DISTANCE
            | id::CALLOUT_ADJUST
            | id::STANDARD_CALL
            | id::RANGES_FILE
            | id::PACENOTES_FILE
            | id::PLACEHOLDER
            | id::EMPTY_CALL
            | id::SOUND_FILE
    ) || (id::START..=id::END_OF_TRACK).contains(&id))
}

impl CustomCoDriver {
    /// The co-driver `choice` names, on notes `dls` (the stage's keys) unless it names a note file. A style or range
    /// set the notes spell (`PACENOTES_FILE`, `RANGES_FILE`) is used when installed.
    pub fn load(choice: &Choice, dls: Vec<Note>, callout_time: f32) -> Result<CustomCoDriver, String> {
        let (notes, style, ranges) = resolved(choice, dls)?;
        let (voice, calls) = Voice::load(&choice.plugin, &choice.voice, &style, &ranges)?;
        Ok(CustomCoDriver::new(notes, calls, voice, callout_time))
    }

    /// The co-driver for the HUD: [`load`](Self::load) without the voice (no samples are read), stepped only for
    /// its fired calls ([`step_hud`](Self::step_hud)). The firing is [`step`](Self::step)'s own code, so the screen
    /// follows the voice without an audio device.
    pub fn load_for_hud(choice: &Choice, dls: Vec<Note>, callout_time: f32) -> Result<CustomCoDriver, String> {
        let (notes, style, ranges) = resolved(choice, dls)?;
        let calls = codriver::load(&choice.plugin, &style, &ranges)?;
        Ok(CustomCoDriver::new(notes, calls, Voice::default(), callout_time))
    }
}

/// The notes and the style and range set `choice` names: `dls` unless it names a note file. A style or range set
/// the notes spell (`PACENOTES_FILE`, `RANGES_FILE`) is used when installed.
fn resolved(choice: &Choice, dls: Vec<Note>) -> Result<(Vec<Note>, String, String), String> {
    let notes = match &choice.notes {
        Some(path) => pacenotes::read(path)?,
        None => dls,
    };
    let named = |call, installed: Vec<String>, chosen: &str| {
        pacenotes::named_file(&notes, call)
            .and_then(|n| installed.into_iter().find(|i| i.eq_ignore_ascii_case(&n)))
            .unwrap_or_else(|| chosen.to_owned())
    };
    let style = named(id::PACENOTES_FILE, codriver::styles(&choice.plugin), &choice.style);
    let ranges = named(id::RANGES_FILE, codriver::range_sets(&choice.plugin), &choice.ranges);
    Ok((notes, style, ranges))
}

/// The stage's own notes: its `.dls` `Driveline` keys (`stage` a name or path, `rbr_formats::stage_files`).
pub fn stage_notes(data: &rbr_formats::data::Data, stage: &str) -> Result<Vec<Note>, String> {
    let bytes = rbr_formats::stage_files::read(data, stage, "dls")?;
    let markers = rbr_formats::markers::parse(&bytes).map_err(|e| format!("{stage}.dls: {e}"))?;
    Ok(markers.keys.into_iter().map(Note::from).collect())
}

/// The handler's settings notes (`+0x1c` .. `+0x40`): a value and whether it is sticky.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Setting {
    value: f32,
    sticky: bool,
}

impl Setting {
    fn spend(&mut self) {
        if !self.sticky {
            self.value = 0.0;
        }
    }
}

/// A distance call (`getDistanceCallId`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum DistanceCall {
    None,
    /// `AND` before the next call.
    And,
    Range(u32),
    /// The next call's link.
    Link(usize),
}

/// The co-driver's state over a drive.
#[derive(Debug, Clone)]
pub struct CustomCoDriver {
    notes: Vec<Note>,
    fired: Vec<bool>,
    calls: Calls,
    voice: Voice,
    /// The game's callout distance option, seconds of speed ahead (`MarkerSettings::callout_distance`).
    pub callout_time: f32,
    /// No notes are said (the co-driver switch); they are still passed.
    pub muted: bool,
    time: Setting,
    distance: Setting,
    adjust: Setting,
    /// The take the next calls say (`SOUND_INDEX`; -1 a random one) and whether it is sticky.
    take: (i32, bool),
    /// `AND` waits for the next spoken call (`+0x18`).
    and: bool,
    last_x: Option<f32>,
}

impl CustomCoDriver {
    /// The co-driver for the stage's `notes` (sorted by distance), saying `calls` with `voice`.
    pub fn new(notes: Vec<Note>, calls: Calls, voice: Voice, callout_time: f32) -> CustomCoDriver {
        CustomCoDriver {
            fired: vec![false; notes.len()],
            notes,
            calls,
            voice,
            callout_time,
            muted: false,
            time: Setting::default(),
            distance: Setting::default(),
            adjust: Setting::default(),
            take: (-1, false),
            and: false,
            last_x: None,
        }
    }

    /// The voice's samples, for the mixer (`Speech::Sample`).
    pub fn samples(&self) -> &[Wave] {
        &self.voice.samples
    }

    /// Sample `k`'s file name.
    pub fn sample_name(&self, k: u32) -> Option<&str> {
        self.voice.names.get(k as usize).map(String::as_str)
    }

    /// The notes, with the style and range set `choice` names ([`load`](Self::load)): `dls` unless it names a
    /// note file.
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// The call's name for the HUD: the style's, else the plugin's own ([`NAMED`]).
    pub fn call_name(&self, id: u32) -> Option<String> {
        self.calls
            .notes
            .get(&id)
            .map(|c| c.name.clone())
            .or_else(|| NAMED.iter().find(|(_, i)| *i == id).map(|(name, _)| name.to_string()))
    }

    /// Back to the start: no note fired (a restart).
    pub fn reset(&mut self) {
        self.fired.fill(false);
        self.time = Setting::default();
        self.distance = Setting::default();
        self.adjust = Setting::default();
        self.take = (-1, false);
        self.and = false;
        self.last_x = None;
    }

    /// The speech of one frame with the car at driveline distance `x` going `speed` m/s; `rand` gives the takes.
    pub fn step(&mut self, x: f32, speed: f32, mut rand: impl FnMut() -> u32) -> Vec<Command> {
        let (mut out, _) = self.step_with_hud(x, speed, &mut rand);
        if self.muted {
            out.clear();
        }
        out
    }

    /// [`step`](Self::step) with the frame's fired spoken calls for the HUD.
    pub fn step_with_hud(&mut self, x: f32, speed: f32, mut rand: impl FnMut() -> u32) -> (Vec<Command>, Vec<HudCall>) {
        let (mut out, mut hud) = (Vec::new(), Vec::new());
        if self.last_x.is_some_and(|last| x < last - REWIND) {
            for (fired, note) in self.fired.iter_mut().zip(&self.notes) {
                *fired &= note.distance <= x;
            }
        }
        self.last_x = Some(x);
        // The finish once passed: the last note behind the car.
        if let Some(k) = self.notes.iter().rposition(|n| n.distance < x) {
            if self.notes[k].id() == id::FINISH && !self.fired[k] {
                self.fired[k] = true;
                let take = self.notes[k].take().unwrap_or(self.take.0);
                self.say_id(id::FINISH, take, &mut out, &mut rand);
            }
        }
        for k in 0..self.notes.len() {
            let note = self.notes[k];
            let call = note.id();
            if self.fired[k] || (id::START..=id::END_OF_TRACK).contains(&call) {
                continue;
            }
            let braking = if self.adjust.value < 0.01 { 0.0 } else { speed * speed * 0.5 / self.adjust.value };
            let ahead = (self.callout_time + self.time.value) * speed + braking + self.distance.value;
            if x + ahead.max(0.0) < note.distance {
                break;
            }
            self.fired[k] = true;
            let value = || Setting { value: f32::from(note.value()) / 100.0, sticky: note.sticky() };
            match call {
                id::DISTANCE_CALL => self.and = false,
                id::SOUND_INDEX => self.take = (i32::from(note.value()), note.sticky()),
                id::CALLOUT_TIME => self.time = value(),
                id::CALLOUT_DISTANCE => self.distance = value(),
                id::CALLOUT_ADJUST => self.adjust = value(),
                id::STANDARD_CALL | id::RANGES_FILE | id::PACENOTES_FILE | id::PLACEHOLDER => {}
                _ => self.spoken(k, &mut out, &mut hud, &mut rand),
            }
        }
        (out, hud)
    }

    /// The frame's fired spoken calls for the HUD, without speech: the firing is [`step`](Self::step)'s, so the
    /// screen follows the voice without an audio device (the takes are not picked: they only choose the samples).
    pub fn step_hud(&mut self, x: f32, speed: f32) -> Vec<HudCall> {
        self.step_with_hud(x, speed, || 0).1
    }

    /// A spoken call: the waiting `AND`, the call, its modifier, its distance call.
    fn spoken(&mut self, k: usize, out: &mut Vec<Command>, hud: &mut Vec<HudCall>, rand: &mut impl FnMut() -> u32) {
        let note = self.notes[k];
        if hud_visible(note.id()) {
            hud.push(HudCall {
                distance: note.distance,
                flags: note.flags,
                called: match self.distance_call(k) {
                    DistanceCall::Range(metres) => metres as f32,
                    _ => 0.0,
                },
                id: note.id(),
                standard: note.kind & 0xffff,
                name: self.call_name(note.id()),
            });
        }
        if std::mem::take(&mut self.and) {
            self.say_id(id::AND, self.take.0, out, rand);
        }
        self.say_id(note.id(), note.take().unwrap_or(self.take.0), out, rand);
        if let Some(name) = note.modifier() {
            let say = self.say_named(name).cloned();
            self.say(say.as_ref(), self.take.0, out, rand);
        }
        self.time.spend();
        self.distance.spend();
        self.adjust.spend();
        if !self.take.1 {
            self.take.0 = -1;
        }
        match self.distance_call(k) {
            DistanceCall::None => {}
            DistanceCall::And => self.and = true,
            DistanceCall::Range(metres) => {
                let say = self.voice.ranges.iter().find(|(m, _)| *m == metres).map(|(_, s)| s.clone());
                self.say(say.as_ref(), self.take.0, out, rand);
            }
            DistanceCall::Link(next) => {
                let link = self.calls.notes.get(&self.notes[next].id()).and_then(|c| c.link.clone());
                if let Some(say) = link.and_then(|name| self.say_named(&name).cloned()) {
                    self.say(Some(&say), self.take.0, out, rand);
                }
            }
        }
    }

    /// `getDistanceCallId`: the distance this call ends with.
    fn distance_call(&self, k: usize) -> DistanceCall {
        let note = self.notes[k];
        let mut d = self.call_distance(k);
        let explicit = d > 0.0;
        let mut next = None;
        if !explicit && !note.sticky() {
            next = self.next_call(k, false);
            if let Some(n) = next {
                d = self.notes[n].distance - note.distance;
            }
        }
        d = self.nearest_range(d);
        if !explicit && d >= MIN_RANGE {
            d = 0.0;
        }
        if d <= 0.0 {
            DistanceCall::None
        } else if d > LINK_GAP {
            if d >= MIN_RANGE {
                DistanceCall::Range(d as u32)
            } else {
                DistanceCall::And
            }
        } else {
            next.map_or(DistanceCall::None, DistanceCall::Link)
        }
    }

    /// `getCallDistanceToNext`: with a `DISTANCE_CALL` note next (past the silent ones), the distance from it to the
    /// next call with a link; else 0.
    fn call_distance(&self, k: usize) -> f32 {
        let start = if self.notes[k].id() == id::DISTANCE_CALL { k } else { k + 1 };
        for j in start..self.notes.len() {
            match self.notes[j].id() {
                id::SOUND_FILE
                | id::SOUND_INDEX
                | id::CALLOUT_TIME
                | id::CALLOUT_DISTANCE
                | id::STANDARD_CALL
                | id::RANGES_FILE
                | id::PACENOTES_FILE
                | id::PLACEHOLDER
                | id::START..=id::END_OF_TRACK => {}
                id::DISTANCE_CALL => {
                    return self.next_call(j, true).map_or(0.0, |n| self.notes[n].distance - self.notes[j].distance)
                }
                _ => return 0.0,
            }
        }
        0.0
    }

    /// `getNextCalloutPaceNote`: the next note with a defined call within 1000 m, past the plugin's silent and
    /// setting notes; one without a link ends the search, or with `skip_unlinked` is passed.
    fn next_call(&self, k: usize, skip_unlinked: bool) -> Option<usize> {
        let from = self.notes[k].distance;
        for j in k + 1..self.notes.len() {
            let note = self.notes[j];
            if note.distance - from > MAX_GAP {
                return None;
            }
            let call = note.id();
            let passed = matches!(
                call,
                id::DISTANCE_CALL
                    | id::START..=id::END_OF_TRACK
                    | id::CALLOUT_ADJUST
                    | id::PLACEHOLDER
                    | id::SOUND_FILE
                    | id::SOUND_INDEX
                    | id::ONTO..=id::STANDARD_CALL
                    | id::RANGES_FILE
                    | id::PACENOTES_FILE
            );
            let Some(def) = self.calls.notes.get(&call).filter(|_| !passed) else { continue };
            if def.link.is_some() {
                return Some(j);
            }
            if !skip_unlinked {
                return None;
            }
        }
        None
    }

    /// The range set's call nearest `d`: 10 below 11 m, else the nearer of the two calls around it (the upper from
    /// halfway), the first below the first and the last above the last.
    fn nearest_range(&self, d: f32) -> f32 {
        let n = d.floor() as i32;
        if n < 11 {
            return LINK_GAP;
        }
        let n = n as u32;
        let ranges: Vec<u32> = self.voice.ranges.iter().map(|(m, _)| *m).collect();
        let Some(upper) = ranges.iter().position(|&m| m > n) else {
            return ranges.last().map_or(LINK_GAP, |&m| m as f32);
        };
        if upper == 0 {
            return ranges[0] as f32;
        }
        let (lo, hi) = (ranges[upper - 1], ranges[upper]);
        if n != lo && (n - lo) as f32 / (hi - lo) as f32 >= 0.5 {
            hi as f32
        } else {
            lo as f32
        }
    }

    fn say_named(&self, name: &str) -> Option<&Say> {
        let by_id = self.calls.notes.iter().find(|(_, c)| c.name.eq_ignore_ascii_case(name)).map(|(id, _)| *id);
        by_id.and_then(|id| self.voice.notes.get(&id)).or_else(|| self.voice.named.get(name))
    }

    fn say_id(&self, call: u32, take: i32, out: &mut Vec<Command>, rand: &mut impl FnMut() -> u32) {
        self.say(self.voice.notes.get(&call), take, out, rand);
    }

    /// `playSound`: take `take` (one past the last is the first), or with -1 a random one.
    fn say(&self, say: Option<&Say>, take: i32, out: &mut Vec<Command>, rand: &mut impl FnMut() -> u32) {
        match say {
            Some(Say::Takes(takes)) => {
                let count = takes.len();
                let pick = if take < 0 && count > 1 { rand() as usize % count } else { take.max(0) as usize };
                out.push(Command::Speech(Speech::Sample(takes[if pick < count { pick } else { 0 }])));
            }
            Some(Say::Legacy(word)) => out.push(Command::Speech(Speech::Word(*word))),
            Some(Say::Nothing) | None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::rsf::pacenotes::STICKY;

    fn call(name: &str, link: Option<&str>) -> Call {
        Call { name: name.into(), sounds: vec![format!("{name}.ogg")], link: link.map(str::to_owned) }
    }

    /// Calls: 3 MEDIUMLEFT and 8 MEDIUMRIGHT (linked INTO), 16 OVERCREST (no link), INTO, AND, LONG; ranges 30 to 100.
    /// Each call's sample is its own index in this order.
    fn codriver(notes: &[(u32, u32, f32)]) -> CustomCoDriver {
        let mut calls = Calls::default();
        let mut voice = Voice::default();
        for (k, (call_id, def)) in [
            (3, call("MEDIUMLEFT", Some("INTO"))),
            (8, call("MEDIUMRIGHT", Some("INTO"))),
            (16, call("OVERCREST", None)),
            (id::INTO, call("INTO", None)),
            (id::AND, call("AND", None)),
            (id::LONG, call("LONG", None)),
        ]
        .into_iter()
        .enumerate()
        {
            calls.notes.insert(call_id, def);
            voice.notes.insert(call_id, Say::Takes(vec![k as u32]));
        }
        for (k, m) in [30, 50, 70, 100].into_iter().enumerate() {
            calls.ranges.insert(m, call(&m.to_string(), None));
            voice.ranges.push((m, Say::Takes(vec![10 + k as u32])));
        }
        let notes = notes.iter().map(|&(kind, flags, distance)| Note { kind, flags, distance }).collect();
        CustomCoDriver::new(notes, calls, voice, 4.0)
    }

    fn said(c: &mut CustomCoDriver, x: f32, speed: f32) -> Vec<u32> {
        c.step(x, speed, || 0)
            .into_iter()
            .map(|cmd| match cmd {
                Command::Speech(Speech::Sample(s)) => s,
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn notes_are_called_a_speed_lookahead_ahead_once() {
        let mut c = codriver(&[(3, 0, 100.0), (8, 0, 400.0)]);
        // 4 s at 20 m/s: 80 m ahead.
        assert!(said(&mut c, 0.0, 20.0).is_empty());
        assert_eq!(said(&mut c, 20.0, 20.0), [0]);
        assert!(said(&mut c, 30.0, 20.0).is_empty());
        // A reset back to 0 calls it again.
        assert_eq!(said(&mut c, 0.0, 30.0), [0]);
    }

    #[test]
    fn a_short_gap_says_the_next_call_s_link_and_a_marker_its_range() {
        // Medium left, 8 m to a medium right: "medium left into". The medium right has a DISTANCE_CALL after it, 64 m
        // from the next call with a link (the crest has none): "medium right 70". The crest's 42 m gap says nothing.
        let mut c = codriver(&[(3, 0, 100.0), (8, 0, 108.0), (13, 0, 128.0), (16, 0, 150.0), (3, 0, 192.0)]);
        assert_eq!(said(&mut c, 100.0, 25.0), [0, 3, 1, 12, 2, 0]);
    }

    #[test]
    fn sticky_notes_take_no_link_and_explicit_calls_play() {
        // Luppis style: every note sticky, the links and modifiers their own notes.
        let s = STICKY;
        let mut c = codriver(&[(3, s, 100.0), (8, s | 0x400, 100.1), (id::AND, s, 100.2), (16, s, 105.0)]);
        // The 0x400 flag says LONG after the medium right; AND is a note of its own.
        assert_eq!(said(&mut c, 100.0, 20.0), [0, 1, 5, 4, 2]);
    }

    #[test]
    fn settings_hold_for_one_call_unless_sticky() {
        // CALLOUT_DISTANCE 50 m (5000 hundredths), then two notes 60 m ahead at standstill.
        let mut c = codriver(&[(5000 << 16 | id::CALLOUT_DISTANCE, 0, 1.0), (3, 0, 40.0), (8, 0, 60.0)]);
        assert_eq!(said(&mut c, 10.0, 0.0), [0]);
        assert!(said(&mut c, 11.0, 0.0).is_empty());
    }

    #[test]
    fn ranges_snap_to_the_nearer_call() {
        let c = codriver(&[]);
        assert_eq!(
            [5.0, 10.9, 11.0, 39.0, 40.0, 61.0, 99.0, 5000.0].map(|d| c.nearest_range(d)),
            [10.0, 10.0, 30.0, 30.0, 50.0, 70.0, 100.0, 100.0]
        );
    }

    #[test]
    fn only_pace_notes_show_on_the_hud() {
        for id in [3, 8, 16, id::AND, id::INTO, id::ONTO, id::CUT, id::LONG, 0x1234] {
            assert!(hud_visible(id), "{id:#x} shows");
        }
        for id in [
            id::DISTANCE_CALL,
            id::SOUND_INDEX,
            id::SOUND_FILE,
            id::CALLOUT_TIME,
            id::CALLOUT_DISTANCE,
            id::CALLOUT_ADJUST,
            id::STANDARD_CALL,
            id::RANGES_FILE,
            id::PACENOTES_FILE,
            id::PLACEHOLDER,
            id::EMPTY_CALL,
            id::START,
            id::SPLIT,
            id::FINISH,
            id::END_OF_TRACK,
        ] {
            assert!(!hud_visible(id), "{id:#x} shows nothing");
        }
    }

    #[test]
    fn the_hud_follows_the_spoken_calls_without_speech() {
        // A medium left (8 m to a medium right: "medium left into"), a DISTANCE_CALL, a CALLOUT_DISTANCE setting
        // and a crest: only the corners and the link show; the control notes stay silent on both.
        let notes = [
            (3, 0, 100.0),
            (8, 0, 108.0),
            (13, 0, 128.0),
            (5000 << 16 | id::CALLOUT_DISTANCE, 0, 129.0),
            (16, 0, 150.0),
        ];
        let mut voiced = codriver(&notes);
        let mut quiet = CustomCoDriver::new(voiced.notes.clone(), voiced.calls.clone(), Voice::default(), 4.0);
        let (speech, hud) = voiced.step_with_hud(100.0, 25.0, || 0);
        assert_eq!(speech.len(), 4, "the voice still says every word");
        assert_eq!(quiet.step_hud(100.0, 25.0), hud, "without a voice the same calls fire");
        let ids: Vec<u32> = hud.iter().map(|c| c.id).collect();
        // The INTO link is said with the medium left, not as a call of its own.
        assert_eq!(ids, [3, 8, 16]);
        assert_eq!((hud[0].distance, hud[0].standard, hud[0].name.as_deref()), (100.0, 3, Some("MEDIUMLEFT")));
        assert_eq!(hud[1].called, 0.0, "a link gap of 10 m and under says no distance");
        assert!(quiet.step_hud(101.0, 25.0).is_empty(), "nothing fires twice");
    }

    #[test]
    fn a_reset_calls_the_hud_notes_again() {
        let mut c = codriver(&[(3, 0, 100.0)]);
        assert_eq!(c.step_hud(20.0, 20.0).len(), 1);
        assert!(c.step_hud(30.0, 20.0).is_empty());
        c.reset();
        assert_eq!(c.step_hud(20.0, 20.0).len(), 1, "after a restart");
    }
}

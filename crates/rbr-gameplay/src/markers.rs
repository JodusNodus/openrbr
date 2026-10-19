//! The `Driveline` animation-set ticker (`E12_AnimSet::Load3b0`, 0x4783b0): it advances marker fired bits and
//! returns the start-line, split, finish, end-of-stage and pace-note effects for the host to dispatch.
//!
//! The original reads these keys from the stage's `.dls` `Driveline` set. This module starts from
//! [`rbr_formats::markers::StageMarkers`] and keeps the runtime bit 31 in each copied key's `word1`. Host results and
//! signal payload context are typed inputs, so the state transition itself has no memory access or side effects.

use std::cmp::Ordering;

use rbr_formats::markers::{MarkerKey, StageMarkers, TAG_END, TAG_FINISH, TAG_SPLIT, TAG_START};

/// Runtime bit `Load3b0` sets after a marker or pace note has been handled.
pub const FIRED: u32 = 0x8000_0000;

/// The fixed threshold multiplier in the original's pace-note lookahead.
pub const PACE_SPEED_MULTIPLIER: f32 = 3.6;

/// The two application settings `E12_AnimSet::Load3b0` (0x4783b0) reads from globals 0x1660b74 and 0x1660b80.
/// The caller supplies them; the defaults are the values the game starts with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerSettings {
    /// `CALLOUTD` in the Pacenotes menu (0x1660b74, range 0.05 to 8.0): the lookahead is this times `d`.
    pub callout_distance: f32,
    /// Global 0x1660b80: a pace note calls out while this is below `d * 3.6`.
    pub marker_threshold: f32,
}

impl Default for MarkerSettings {
    fn default() -> Self {
        Self { callout_distance: 4.0, marker_threshold: 25.0 }
    }
}

/// E25's tag-to-word mapper (`E25_SlotId`, 0x4b55e0). `-1` means the tag has no bank word.
pub fn slot_word_id(tag: u32) -> i32 {
    match tag {
        0 => 0x1d,
        1 => 0x0e,
        2 => 0x21,
        3 => 0x25,
        4 => 0x19,
        5 => 0x17,
        6 => 0x18,
        7 => 0x1a,
        8 => 0x26,
        9 => 0x22,
        10 => 0x0f,
        11 => 0x1e,
        12 => 0x2c,
        14 => 0x27,
        15 => 0x2d,
        16 => 0x28,
        17 => 0x1c,
        18 | 32 => 0x13,
        19 => 0x12,
        20 => 0x20,
        22 => 0x1b,
        25 => 0x31,
        26 => 0x32,
        27 => 0x11,
        28 => 0x33,
        29 => 0x34,
        30 => 0x35,
        31 => 0x36,
        _ => -1,
    }
}

/// E25's word1-flag mapper (`E25_BitId`, 0x4b5540). `-1` means no extra bank word.
pub fn flag_word_id(bits: u32) -> i32 {
    if bits == 0 {
        -1
    } else if bits & 1 != 0 {
        0x27
    } else if bits & 2 != 0 {
        0x2d
    } else if bits & 4 != 0 {
        0x2a
    } else if bits & 0x10 != 0 {
        0x2b
    } else if bits & 0x20 != 0 {
        0x16
    } else if bits & 0x40 != 0 {
        0x15
    } else if (bits as u8 as i8) < 0 {
        0x29
    } else if bits & 0x400 != 0 {
        0x23
    } else if bits & 0x800 != 0 {
        0x2e
    } else if bits & 0x1000 != 0 {
        0x2f
    } else if bits & 0x2000 != 0 {
        0x24
    } else {
        -1
    }
}

/// Return values the start-line path obtains from the game hosts before making its side-effect decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartLineContext {
    /// `E60Flow::M566240()` (0x566240), the rally school (`AssistSettings::school`).
    pub game_host_result: i32,
    /// `E62_M572fc0()`, the HUD's countdown runs (`RaceClock::counting_down`); read only when `game_host_result == 0`.
    pub menu_host_result: i32,
    /// `CApplication` state at +0x728.
    pub application_state: u32,
    /// The race-host vtable slot 0x13 result (`CPC_TrackLoaderDaemon::Vf4C` 0x467930: the map `InitDemo` started,
    /// e.g. 54 for Tanner); read only when the menu host is nonzero and the state is not 10.
    pub race_host_result: u32,
    /// `E59Host90::M00565470()` result.
    pub event_state: i32,
}

/// Effects of the start-line host calls, in the order they occur in `Load3b0`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartLineEffects {
    /// The warning log is emitted when the line is crossed in application state 10.
    pub warn_in_start_scene: bool,
    /// The jump start, `E62GameMenu::M579430` (0x579430), taken when `race_host_result % 10 < 8`. Not in state 10: after
    /// the warning the asm jumps past the race host and the jump start (0x478627 `jmp 0x47864d`), so a car standing past
    /// the start line has the key fired by the start scene, with no penalty.
    pub jump_start: bool,
    /// The host's result 1 clears `DAT_0165fc68 + 0x144`.
    pub clear_menu_flag: bool,
    /// `event_state == 7` activates the camera object's slot 0x28 with 1.
    pub activate_ghost_camera: bool,
    /// The same `event_state == 7` path clears the ghost manager flag.
    pub clear_ghost_flag: bool,
}

impl StartLineContext {
    fn effects(self) -> StartLineEffects {
        let menu_host_active = self.game_host_result == 0 && self.menu_host_result != 0;
        StartLineEffects {
            warn_in_start_scene: menu_host_active && self.application_state == 10,
            jump_start: menu_host_active && self.application_state != 10 && self.race_host_result % 10 < 8,
            clear_menu_flag: self.game_host_result == 1,
            activate_ghost_camera: self.event_state == 7,
            clear_ghost_flag: self.event_state == 7,
        }
    }
}

/// The four game-host words copied into the `CSigEndRace` payload; word 4 is always zero in this ticker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EndRaceContext {
    pub words: [u32; 4],
}

/// Inputs read outside the marker array by one `Load3b0` call.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TickInput {
    /// Current driveline distance `x`.
    pub x: f32,
    /// `E12_Sampler::Dot150()`; this initializes the local speed-like value `d`.
    pub dot150: f32,
    /// `E12_SeekD(this)`; callout payloads use this value. The assembled ticker keeps `Dot150` for subsequent lookahead checks.
    pub seek_d: f32,
    /// Float at 0x1660b74, multiplied by `d` to get the pace-note lookahead distance.
    pub lookahead_scale: f32,
    /// Float at 0x1660b80, compared with `d * 3.6` before a pace-note callout.
    pub pace_speed_threshold: f32,
    pub start_line: StartLineContext,
    /// `E59_00565e80() != 0`: the finish path goes to the menu and skips EndRace.
    pub finish_to_menu: bool,
    /// Value passed to `E12_GameHost::M72da0` on a split key (`DAT_0165fc68 + 0x140`).
    pub split_host_value: u32,
    pub end_race: EndRaceContext,
}

/// The E12 signal payload plus E25's two tag/flag word lookups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaceCallout {
    pub x: f32,
    /// The key after `Load3b0` sets runtime bit 31.
    pub key: MarkerKey,
    /// `E12_SeekD(this)` in the signal's third word.
    pub seek_d: f32,
    /// E25 `E25_SlotId(key.tag)`.
    pub slot_word_id: i32,
    /// E25 `E25_BitId(key.word1)`.
    pub flag_word_id: i32,
}

/// Events `Load3b0` hands to its host and signal connections, in emission order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerEvent {
    StartLine(StartLineEffects),
    SplitHostCall {
        value: u32,
    },
    PaceCallout(PaceCallout),
    /// The finish's menu gate sets +0x144 and calls `E62GameMenu::M573530`.
    FinishMenu,
    /// The five-word `CSigEndRace`, with a zero state word at +0x10.
    EndRace {
        context: EndRaceContext,
        state: u32,
    },
}

/// Runtime copy of one stage's sorted marker list. Word1 bit 31 records whether E12 already handled the key.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerTicker {
    keys: Vec<MarkerKey>,
}

impl MarkerTicker {
    pub fn load(markers: &StageMarkers) -> Self {
        Self { keys: markers.keys.clone() }
    }

    pub fn from_keys(keys: Vec<MarkerKey>) -> Self {
        Self { keys }
    }

    pub fn keys(&self) -> &[MarkerKey] {
        &self.keys
    }

    /// `E12_SeekD` (0x476bf0) reads the active `Driveline` key list and current holder record. The list is loaded
    /// from the stage's `.dls` AnimData set; the holder points at the first record beyond its current distance.
    pub fn seek_d(&self, x: f32) -> f32 {
        let Some(mut index) = self.keys.iter().position(|key| x < key.distance) else {
            return 0.0;
        };

        // SeekD uses NextTagB(..., 1) when the holder points at a start, finish, or split key.
        while self.keys.get(index).is_some_and(|key| (TAG_START..=TAG_SPLIT).contains(&key.tag)) {
            index += 1;
        }
        if self.keys.get(index).map(|key| key.tag) != Some(0x0d) {
            return 0.0;
        }

        // HostFind skips the distance key and scans to the next tag handled by its switch.
        index += 1;
        while index < self.keys.len() {
            while self.keys.get(index).is_some_and(|key| (TAG_START..=TAG_SPLIT).contains(&key.tag)) {
                index += 1;
            }
            let Some(key) = self.keys.get(index) else {
                break;
            };
            if spoken_tag(key.tag) {
                return key.distance - x;
            }
            index += 1;
        }
        -1.0
    }

    /// `E12_KeyList::NextKey(key, 1)` (0x475de0, through 0x474db0): the key after `index`, past start, finish and
    /// split keys, or `None` at the end of the list.
    pub fn next_key(&self, index: usize) -> Option<usize> {
        (index + 1..self.keys.len()).find(|&k| !(TAG_START..=TAG_SPLIT).contains(&self.keys[k].tag))
    }

    /// `E12_KeyGap` (0x476b10, name ours): the distance from key `index` to the next key with a spoken tag (the
    /// switch at 0x476bbc, the same set `seek_d` scans for), past start, finish and split keys; -1 without one.
    pub fn key_gap(&self, index: usize) -> f32 {
        let Some(key) = self.keys.get(index) else {
            return -1.0;
        };
        let mut next = self.next_key(index);
        while let Some(k) = next {
            if spoken_tag(self.keys[k].tag) {
                return self.keys[k].distance - key.distance;
            }
            next = self.next_key(k);
        }
        -1.0
    }

    /// The index of the key a callout carries (its tag and distance; the fired bit is ignored).
    pub fn index_of(&self, key: MarkerKey) -> Option<usize> {
        self.keys.iter().position(|k| k.tag == key.tag && k.distance == key.distance)
    }

    /// One marker ticker call (`E12_AnimSet::Load3b0`, 0x4783b0), after its non-marker animation work.
    ///
    /// The backwards pass handles at most the last crossed key (`distance < x`) per call. Encountering an already
    /// fired crossed key stops that pass. The forward pass starts at the first key strictly beyond `x`, skips the
    /// four control tags, and marks pace notes in the `x + lookahead` window whether or not their speed gate emits a
    /// callout.
    pub fn tick(mut self, input: TickInput) -> (Self, Vec<MarkerEvent>) {
        let mut events = Vec::new();
        // `d` stays Dot150 for the whole call (its only write is at 0x4783c4); SeekD only fills callout payloads.
        let d = input.dot150;

        if let Some(mut index) = self.keys.len().checked_sub(1) {
            loop {
                let key = self.keys[index];
                if key.distance.partial_cmp(&input.x) != Some(Ordering::Less) {
                    if index == 0 {
                        break;
                    }
                    index -= 1;
                    continue;
                }
                if key.word1 & FIRED != 0 {
                    break;
                }
                self.keys[index].word1 |= FIRED;
                let fired = self.keys[index];
                match fired.tag {
                    TAG_START => events.push(MarkerEvent::StartLine(input.start_line.effects())),
                    TAG_FINISH => {
                        events.push(MarkerEvent::PaceCallout(callout(input.x, fired, input.seek_d)));
                        if input.finish_to_menu {
                            events.push(MarkerEvent::FinishMenu);
                        } else {
                            events.push(MarkerEvent::EndRace { context: input.end_race, state: 0 });
                        }
                    }
                    TAG_SPLIT => {
                        events.push(MarkerEvent::SplitHostCall { value: input.split_host_value });
                        events.push(MarkerEvent::PaceCallout(callout(input.x, fired, input.seek_d)));
                    }
                    TAG_END => {
                        events.push(MarkerEvent::PaceCallout(callout(input.x, fired, input.seek_d)));
                        events.push(MarkerEvent::EndRace { context: input.end_race, state: 0 });
                    }
                    _ => {}
                }
                // The original jumps to the forward-pass label after handling exactly one crossed key.
                break;
            }
        }

        // `E12_KeyList::Find(x)` returns null when x exactly equals any key distance; otherwise it returns the first
        // key strictly beyond x. Special control tags are skipped before the lookahead test in the original; only
        // ordinary pace-note tags are marked during this pass.
        let mut index = if self.keys.iter().any(|key| input.x == key.distance) {
            None
        } else {
            self.keys.iter().position(|key| input.x < key.distance)
        };
        while let Some(i) = index {
            let key = self.keys[i];
            if !matches!(key.tag, TAG_START | TAG_FINISH | TAG_SPLIT | TAG_END) {
                let lookahead = input.lookahead_scale * d + input.x;
                if lookahead <= key.distance {
                    break;
                }
                if key.word1 & FIRED == 0 {
                    if input.pace_speed_threshold < d * PACE_SPEED_MULTIPLIER {
                        events.push(MarkerEvent::PaceCallout(callout(input.x, key, input.seek_d)));
                    }
                    self.keys[i].word1 |= FIRED;
                }
            }
            index = (i + 1 < self.keys.len()).then_some(i + 1);
        }
        (self, events)
    }
}

/// The tags `E12_SeekD` and `E12_KeyGap` stop at (their switches' case 0): the pace notes with a spoken word. The HUD's
/// `E12_AllowTag` (0x474b70) is the same set: the notes with a symbol.
pub fn spoken_tag(tag: u32) -> bool {
    matches!(tag, 0x00..=0x0c | 0x0e | 0x10..=0x14 | 0x19..=0x1c | 0x20)
}

/// How many symbol keys the HUD's pace stack keeps from the next one on (HUD +0x1d0..+0x1e0).
pub const PACE_STACK_KEYS: usize = 5;

/// The keys of `HUD::Update`'s pace stack (0x578c30, with `PN_STACK` and the 3D notes reading them): the symbol key
/// before the car's distance `x` (`E12_FindKey` 0x477d40 then the walk back, 0x475e20), when there is one, then the next
/// [`PACE_STACK_KEYS`] symbol keys from `x` on (0x476f50 for each after the first). Keys are [`spoken_tag`]s.
pub fn pace_stack(keys: &[MarkerKey], x: f32) -> Vec<MarkerKey> {
    let next = keys.iter().position(|k| k.distance >= x).unwrap_or(keys.len());
    let previous = keys[..next].iter().rev().find(|k| spoken_tag(k.tag));
    previous
        .into_iter()
        .chain(keys[next..].iter().filter(|k| spoken_tag(k.tag)).take(PACE_STACK_KEYS))
        .copied()
        .collect()
}

fn callout(x: f32, key: MarkerKey, seek_d: f32) -> PaceCallout {
    PaceCallout { x, key, seek_d, slot_word_id: slot_word_id(key.tag), flag_word_id: flag_word_id(key.word1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pace_stack_is_the_last_symbol_behind_and_the_next_five() {
        let keys: Vec<MarkerKey> =
            [(3, 10.0), (0x15, 20.0), (8, 30.0), (0x0d, 40.0), (1, 50.0), (2, 60.0), (4, 70.0), (5, 80.0), (6, 90.0)]
                .iter()
                .map(|&(tag, d)| key(tag, 0, d))
                .collect();
        let stack = pace_stack(&keys, 35.0);
        let at: Vec<f32> = stack.iter().map(|k| k.distance).collect();
        assert_eq!(at, [30.0, 50.0, 60.0, 70.0, 80.0, 90.0]);
        assert_eq!(pace_stack(&keys, 0.0).len(), PACE_STACK_KEYS, "nothing behind the start");
    }

    fn key(tag: u32, word1: u32, distance: f32) -> MarkerKey {
        MarkerKey { tag, word1, distance }
    }

    fn ticker(keys: &[MarkerKey]) -> MarkerTicker {
        MarkerTicker::from_keys(keys.to_vec())
    }

    fn input(x: f32) -> TickInput {
        TickInput {
            x,
            dot150: 0.0,
            seek_d: 0.0,
            lookahead_scale: 0.05,
            pace_speed_threshold: 0.0,
            ..TickInput::default()
        }
    }

    #[test]
    fn crossing_is_strict_and_handles_one_key_per_call() {
        let original = ticker(&[key(TAG_START, 0, 10.0), key(TAG_SPLIT, 0, 20.0)]);
        let (same, events) = original.clone().tick(input(10.0));
        assert!(events.is_empty());
        assert_eq!(same, original);

        let (once, events) = same.tick(input(30.0));
        assert!(matches!(events.as_slice(), [MarkerEvent::SplitHostCall { .. }, MarkerEvent::PaceCallout(_)]));
        assert_eq!(once.keys()[1].word1 & FIRED, FIRED);
        // The already-fired last crossed key is the backwards pass's stop; it does not work backward to start.
        let (again, events) = once.tick(input(30.0));
        assert!(events.is_empty());
        assert_eq!(again.keys()[0].word1 & FIRED, 0);
    }

    #[test]
    fn next_key_and_key_gap_skip_the_control_and_silent_tags() {
        // A left, a split, a distance key (0x0d, not spoken), then a right.
        let keys = ticker(&[key(1, 0, 10.0), key(TAG_SPLIT, 0, 20.0), key(0x0d, 0, 30.0), key(10, 0, 55.0)]);
        assert_eq!(keys.next_key(0), Some(2));
        assert_eq!(keys.next_key(3), None);
        assert_eq!(keys.key_gap(0), 45.0);
        assert_eq!(keys.key_gap(3), -1.0);
        assert_eq!(keys.index_of(key(10, FIRED, 55.0)), Some(3));
    }

    #[test]
    fn forward_find_returns_null_at_an_exact_key_distance() {
        let original = ticker(&[key(1, 0, 10.0), key(2, 0, 11.0)]);
        let (next, events) = original.clone().tick(input(10.0));
        assert!(events.is_empty());
        assert_eq!(next, original);
    }

    #[test]
    fn finish_and_end_have_their_original_event_order_and_gate() {
        let context = EndRaceContext { words: [1, 2, 3, 4] };
        let mut f = input(11.0);
        f.finish_to_menu = false;
        f.end_race = context;
        let (_, events) = ticker(&[key(TAG_FINISH, 0, 10.0)]).tick(f);
        assert!(
            matches!(events.as_slice(), [MarkerEvent::PaceCallout(_), MarkerEvent::EndRace { context: got, state: 0 }] if *got == context)
        );

        f.finish_to_menu = true;
        let (_, events) = ticker(&[key(TAG_FINISH, 0, 10.0)]).tick(f);
        assert!(matches!(events.as_slice(), [MarkerEvent::PaceCallout(_), MarkerEvent::FinishMenu]));

        f.finish_to_menu = false;
        let (_, events) = ticker(&[key(TAG_END, 0, 10.0)]).tick(f);
        assert!(matches!(events.as_slice(), [MarkerEvent::PaceCallout(_), MarkerEvent::EndRace { .. }]));
    }

    #[test]
    fn start_line_effects_follow_the_host_results() {
        let mut f = input(11.0);
        f.start_line = StartLineContext {
            game_host_result: 0,
            menu_host_result: 1,
            application_state: 10,
            race_host_result: 17,
            event_state: 7,
        };
        let (_, events) = ticker(&[key(TAG_START, 0, 10.0)]).tick(f);
        assert_eq!(
            events,
            vec![MarkerEvent::StartLine(StartLineEffects {
                warn_in_start_scene: true,
                jump_start: false,
                clear_menu_flag: false,
                activate_ghost_camera: true,
                clear_ghost_flag: true,
            })]
        );
        let jump = |application_state, race_host_result| {
            let start_line = StartLineContext { application_state, race_host_result, ..f.start_line };
            match ticker(&[key(TAG_START, 0, 10.0)]).tick(TickInput { start_line, ..f }).1.as_slice() {
                [MarkerEvent::StartLine(effects)] => effects.jump_start,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!([jump(1, 54), jump(1, 17), jump(1, 18), jump(1, 39)], [true, true, false, false]);
    }

    #[test]
    fn forward_pace_note_window_marks_even_when_the_speed_gate_fails() {
        let mut f = input(10.0);
        f.dot150 = 2.0;
        f.lookahead_scale = 1.0;
        f.pace_speed_threshold = 10.0;
        let (next, events) = ticker(&[key(1, 0x400, 11.0)]).tick(f);
        assert!(events.is_empty());
        assert_eq!(next.keys()[0].word1 & FIRED, FIRED);

        f.pace_speed_threshold = 0.0;
        let (next, events) = ticker(&[key(1, 0x400, 11.0)]).tick(f);
        assert!(matches!(
            events.as_slice(),
            [MarkerEvent::PaceCallout(PaceCallout { slot_word_id: 0x0e, flag_word_id: 0x23, .. })]
        ));
        assert_eq!(next.keys()[0].word1 & FIRED, FIRED);
    }

    #[test]
    fn seek_d_uses_the_stage_driveline_key_list() {
        let list = ticker(&[
            key(TAG_START, 0, 10.0),
            key(0x0d, 0, 20.0),
            key(1, 0, 34.0),
            key(TAG_SPLIT, 0, 40.0),
            key(TAG_FINISH, 0, 50.0),
        ]);
        assert_eq!(list.seek_d(15.0), 19.0);
        assert_eq!(list.seek_d(21.0), 0.0);
        assert_eq!(list.seek_d(35.0), 0.0);
        assert_eq!(list.seek_d(60.0), 0.0);
    }

    #[test]
    fn e25_slot_and_flag_maps_keep_their_priority_and_gaps() {
        let tags = [
            (0, 0x1d),
            (1, 0x0e),
            (2, 0x21),
            (3, 0x25),
            (4, 0x19),
            (5, 0x17),
            (6, 0x18),
            (7, 0x1a),
            (8, 0x26),
            (9, 0x22),
            (10, 0x0f),
            (11, 0x1e),
            (12, 0x2c),
            (14, 0x27),
            (15, 0x2d),
            (16, 0x28),
            (17, 0x1c),
            (18, 0x13),
            (19, 0x12),
            (20, 0x20),
            (22, 0x1b),
            (25, 0x31),
            (26, 0x32),
            (27, 0x11),
            (28, 0x33),
            (29, 0x34),
            (30, 0x35),
            (31, 0x36),
            (32, 0x13),
        ];
        for (tag, word_id) in tags {
            assert_eq!(slot_word_id(tag), word_id, "tag {tag}");
        }
        for tag in [13, 21, 23, 24, 33] {
            assert_eq!(slot_word_id(tag), -1, "unmapped tag {tag}");
        }
        assert_eq!(flag_word_id(0), -1);
        assert_eq!(flag_word_id(0x1), 0x27);
        assert_eq!(flag_word_id(0x2), 0x2d);
        assert_eq!(flag_word_id(0x4), 0x2a);
        assert_eq!(flag_word_id(0x10), 0x2b);
        assert_eq!(flag_word_id(0x20), 0x16);
        assert_eq!(flag_word_id(0x40), 0x15);
        assert_eq!(flag_word_id(0x80), 0x29);
        assert_eq!(flag_word_id(0x400), 0x23);
        assert_eq!(flag_word_id(0x800), 0x2e);
        assert_eq!(flag_word_id(0x1000), 0x2f);
        assert_eq!(flag_word_id(0x2000), 0x24);
        assert_eq!(flag_word_id(0x400 | FIRED), 0x23);
    }
}

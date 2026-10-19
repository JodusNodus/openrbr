//! The time keeper's and the time card's words in the emulator's memory (`src/rbr/RBR_E21.h`; the offsets are the asm's
//! of `Record` (0x4a0540), `Update` (0x4a0320) and `Check` (0x4a0450)). The card is read into `rbr_gameplay::timing`'s
//! typed value and the words the port produced are written back.

use rbr_formats::markers::MAX_SPLITS;
use rbr_gameplay::timing::{Checkpoint, CheckpointKind, TimeCard, TimeKeeper};
use rbr_physics::mem::Mem;

/// The application root (`g_1659b70`): its +0x20 is the state (the clock is the double at its +0x20, the step counter at
/// its +0x18), its +0x24 the physics definition (`MaxTimeStep` at +0x1608c).
/// The definition's +0x160f4 word and the root's own +0x38 hold the scratch flush bit (`CSolid::E43Vf74`).
pub const APP: u32 = 0x1659b70;

/// The keeper: the card pointer at +4, the sampled distance (`m_f8`) at +8 and the still count (`m_i0c`) at +0xc.
const CARD: u32 = 0x4;
const SAMPLED: u32 = 0x8;
const STILL: u32 = 0xc;

/// The card: the start and finish distances (`m_f30`, `m_f48`), the split count (+0x10) and passed count (+0x14), `cur`
/// and `last` (`m_f1c`), `flag20`, the hold (`i24`), the origin (`d38`) and its flag (`flag40`), the finish time (`d50`)
/// and its flag (`flag58`). The splits are 0x18-byte entries from +0x60: distance (+0), time (+8) and its set flag (+0x10).
const START: u32 = 0x30;
const FINISH: u32 = 0x48;
const COUNT: u32 = 0x10;
const PASSED: u32 = 0x14;
const CUR: u32 = 0x18;
const LAST: u32 = 0x1c;
const FLAG20: u32 = 0x20;
const HOLD: u32 = 0x24;
const ORIGIN: u32 = 0x38;
const STARTED: u32 = 0x40;
const FINISH_TIME: u32 = 0x50;
const FINISHED: u32 = 0x58;
const ENTRIES: u32 = 0x60;
const ENTRY: u32 = 0x18;
const ENTRY_TIME: u32 = 0x8;
const ENTRY_SET: u32 = 0x10;

/// The `CSigCheckPointTime` payload's `[0, kind, time, index, fired]` words: the kind is 1 for the start, 2 for a split
/// (the index in word 3), 3 for the finish. `Record` and `Check` build it so.
pub fn payload(cp: &Checkpoint) -> [u32; 5] {
    let (kind, index) = match cp.kind {
        CheckpointKind::Start => (1, 0),
        CheckpointKind::Split(n) => (2, n as u32),
        CheckpointKind::Finish => (3, 0),
    };
    [0, kind, cp.time.to_bits(), index, u32::from(cp.fired)]
}

/// The card pointer of the keeper at `keeper`.
pub fn card(mem: &Mem, keeper: u32) -> u32 {
    mem.u32(keeper + CARD)
}

/// The keeper at `keeper`, as the typed port reads it.
pub fn load_keeper(mem: &Mem, keeper: u32) -> TimeKeeper {
    TimeKeeper {
        card: load_card(mem, card(mem, keeper)),
        sampled: mem.f32(keeper + SAMPLED),
        still: mem.u32(keeper + STILL),
    }
}

/// `Update`'s writes: the sampled distance and the still count.
pub fn store_sampling(mem: &mut Mem, keeper: u32, value: &TimeKeeper) {
    mem.set_f32(keeper + SAMPLED, value.sampled);
    mem.set_u32(keeper + STILL, value.still);
}

/// The card at `card`, as the typed port reads it. A split is passed when its set flag is nonzero.
pub fn load_card(mem: &Mem, card: u32) -> TimeCard {
    let count = mem.u32(card + COUNT);
    let entry = |i: u32| card + ENTRIES + ENTRY * i;
    let mut value = (0..count).fold(TimeCard::default(), |c, i| c.push(mem.f32(entry(i))));
    for i in (0..count).filter(|&i| mem.u32(entry(i) + ENTRY_SET) != 0) {
        value = value.append(i as usize, f64_at(mem, entry(i) + ENTRY_TIME));
    }
    value.start = mem.f32(card + START);
    value.finish = mem.f32(card + FINISH);
    value.passed = mem.u32(card + PASSED) as usize;
    value.cur = mem.f32(card + CUR);
    value.last = mem.f32(card + LAST);
    value.started = mem.u32(card + STARTED) != 0;
    value.origin = f64_at(mem, card + ORIGIN);
    value.finish_time = (mem.u32(card + FINISHED) != 0).then(|| f64_at(mem, card + FINISH_TIME));
    value.hold = mem.u32(card + HOLD) as i32;
    value
}

/// The words the port can change, written with the value it produced (an unchanged word is written as it is). The
/// finish time and the splits' times are written only once they are set.
pub fn store_card(mem: &mut Mem, card: u32, value: &TimeCard) {
    mem.set_u32(card + PASSED, value.passed as u32);
    mem.set_f32(card + CUR, value.cur);
    mem.set_f32(card + LAST, value.last);
    mem.set_u32(card + STARTED, u32::from(value.started));
    set_f64(mem, card + ORIGIN, value.origin);
    if let Some(t) = value.finish_time {
        mem.set_u32(card + FINISHED, 1);
        set_f64(mem, card + FINISH_TIME, t);
    }
    for (i, split) in value.splits().iter().enumerate() {
        if let Some(t) = split.time {
            let entry = card + ENTRIES + ENTRY * i as u32;
            mem.set_u32(entry + ENTRY_SET, 1);
            set_f64(mem, entry + ENTRY_TIME, t);
        }
    }
}

/// The keeper's and the card's vtables, and the card's two constructor arguments (+8, +0xc; `Init` passes 0) and its
/// second time (`m_d28`, the constructor's 12 hours like the origin).
const KEEPER_VTABLE: u32 = 0x0073_0b60;
const CARD_VTABLE: u32 = 0x0073_0b54;
const ARGS: u32 = 0x8;
const D28: u32 = 0x28;

/// A new keeper at `keeper` with its card at `card`, holding `value`: what `CTimeKeeper::Init` (0x4a0150) and the card's
/// constructor (0x49ff80, every entry cleared) write, then the timing load's keys (`Load` 0x4a01f0, which
/// `TimeKeeper::load` models: the count, the start, the finish and the splits' distances) and the words `store_card`
/// writes. The constructor leaves `flag20` and the padding words (+4, +0x34, +0x44, +0x4c, +0x5c, each entry's +4 and
/// +0x14) as they are.
pub fn store_new(mem: &mut Mem, keeper: u32, card: u32, value: &TimeKeeper) {
    mem.set_u32(keeper, KEEPER_VTABLE);
    mem.set_u32(keeper + CARD, card);
    store_sampling(mem, keeper, value);
    let new = TimeCard::default();
    mem.set_u32(card, CARD_VTABLE);
    mem.set_u32(card + ARGS, 0);
    mem.set_u32(card + ARGS + 4, 0);
    set_f64(mem, card + D28, new.origin);
    mem.set_u32(card + FINISHED, 0);
    set_f64(mem, card + FINISH_TIME, 0.0);
    mem.set_u32(card + HOLD, value.card.hold as u32);
    let splits = value.card.splits();
    mem.set_u32(card + COUNT, splits.len() as u32);
    for i in 0..MAX_SPLITS {
        let entry = card + ENTRIES + ENTRY * i as u32;
        mem.set_f32(entry, splits.get(i).map_or(0.0, |s| s.distance));
        set_f64(mem, entry + ENTRY_TIME, 0.0);
        mem.set_u32(entry + ENTRY_SET, 0);
    }
    mem.set_f32(card + START, value.card.start);
    mem.set_f32(card + FINISH, value.card.finish);
    store_card(mem, card, &value.card);
}

/// `Check`'s start writes `flag20` with `started` (the port does not keep it; nothing else reads it).
pub fn set_flag20(mem: &mut Mem, card: u32) {
    mem.set_u32(card + FLAG20, 1);
}

/// The step counter (`CState` +0x18), the value `Update` takes mod 100.
pub fn step(mem: &Mem) -> u32 {
    mem.u32(state(mem) + 0x18)
}

/// The physics definition's `MaxTimeStep`, the step `Check` divides by.
pub fn max_time_step(mem: &Mem) -> f32 {
    mem.f32(mem.u32(mem.u32(APP) + 0x24) + 0x1608c)
}

/// The state (`CState`), the application root's +0x20.
fn state(mem: &Mem) -> u32 {
    mem.u32(mem.u32(APP) + 0x20)
}

/// The double at `at` (two words, low first).
pub fn f64_at(mem: &Mem, at: u32) -> f64 {
    f64::from_bits(u64::from(mem.u32(at)) | u64::from(mem.u32(at + 4)) << 32)
}

/// Writes the double `value` at `at`.
fn set_f64(mem: &mut Mem, at: u32, value: f64) {
    let bits = value.to_bits();
    mem.set_u32(at, bits as u32);
    mem.set_u32(at + 4, (bits >> 32) as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card at `at` with a start at 100 m, a finish at 400 m, splits at 200 and 300 m, the first passed at t = 2.
    fn card_at(mem: &mut Mem, at: u32) {
        mem.map(at, &[0; ENTRIES as usize + 2 * ENTRY as usize]);
        mem.set_f32(at + START, 100.0);
        mem.set_f32(at + FINISH, 400.0);
        mem.set_u32(at + COUNT, 2);
        mem.set_u32(at + PASSED, 1);
        mem.set_u32(at + STARTED, 1);
        set_f64(mem, at + ORIGIN, 10.0);
        mem.set_u32(at + ENTRIES, 200f32.to_bits());
        mem.set_u32(at + ENTRIES + ENTRY, 300f32.to_bits());
        mem.set_u32(at + ENTRIES + ENTRY_SET, 1);
        set_f64(mem, at + ENTRIES + ENTRY_TIME, 2.0);
    }

    #[test]
    fn load_then_store_writes_the_same_words() {
        let mut mem = Mem::new();
        card_at(&mut mem, 0x1000);
        let len = ENTRIES as usize + 2 * ENTRY as usize;
        let before = mem.read(0x1000, len);
        let value = load_card(&mem, 0x1000);
        assert_eq!(value.splits().len(), 2);
        assert_eq!(value.splits()[0].time, Some(2.0));
        assert_eq!(value.splits()[1].time, None);
        store_card(&mut mem, 0x1000, &value);
        assert_eq!(mem.read(0x1000, len), before);
    }

    #[test]
    fn the_payload_matches_record_and_check() {
        let cp = Checkpoint { kind: CheckpointKind::Split(1), time: 3.5, fired: false };
        assert_eq!(payload(&cp), [0, 2, 3.5f32.to_bits(), 1, 0]);
        let start = Checkpoint { kind: CheckpointKind::Start, time: -1.0, fired: true };
        assert_eq!(payload(&start), [0, 1, (-1.0f32).to_bits(), 0, 1]);
    }
}

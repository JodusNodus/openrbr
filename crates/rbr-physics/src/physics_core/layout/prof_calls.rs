//! The profiler: the counter frequency, `ProfExit`, `E46ProfSample::EnterName` and `E46ProfChan::Enter`.

use super::lists::{set_word, word};
use crate::mem::Mem;
use crate::physics_core::prof::{self, EnterHost, ProfHost};

/// The performance counter's frequency (`QueryPerformanceFrequency`, stored by the profiler's setup) as a 64-bit word.
const FREQUENCY: u32 = 0x165b208;
/// `E46_ProfFreq` (0x5286a0): `QueryPerformanceFrequency`'s `frequency` stored as the profiler's.
pub fn store_frequency(mem: &mut Mem, frequency: u64) {
    set_word64(mem, FREQUENCY, frequency);
}

/// The sample's fields: the counter at its start (+0x10), the elapsed ticks (+0x18), the elapsed seconds (+0x20) and
/// their reciprocal (+0x28, 0.0 when the elapsed time is not positive).
const START: u32 = 0x10;
const TICKS: u32 = 0x18;
const SECONDS: u32 = 0x20;
const RATE: u32 = 0x28;

pub(super) fn word64(mem: &Mem, at: u32) -> u64 {
    mem.u32(at) as u64 | (mem.u32(at + 4) as u64) << 32
}

pub(super) fn set_word64(mem: &mut Mem, at: u32, v: u64) {
    mem.set_u32(at, v as u32);
    mem.set_u32(at + 4, (v >> 32) as u32);
}

/// `ProfExit(handle)` (0x528690, fastcall; `local` is the 8-byte word in the original's frame the counter goes to): the sample at `*handle` ends. Returns the word `eax` keeps, the high word
/// of the sample's start.
pub fn prof_exit(host: &mut impl ProfHost, mem: &mut Mem, handle: u32, local: u32) -> u32 {
    let sample = mem.u32(handle);
    host.counter(local);
    let now = word64(mem, local);
    let ticks = now.wrapping_sub(word64(mem, sample + START));
    set_word64(mem, sample + TICKS, ticks);
    let (seconds, rate) = prof::timing(ticks as i64, word64(mem, FREQUENCY) as i64);
    set_word64(mem, sample + SECONDS, seconds.to_bits());
    set_word64(mem, sample + RATE, rate.to_bits());
    mem.u32(sample + START + 4)
}

/// The physics object's flag word (`+0x38`) and the CDef's (`+0x160f4`, the log flags), and the log hook's bit.
pub(super) const PHYSICS_FLAGS_AT: u32 = 0x38;
pub(super) const LOG_FLAGS_AT: u32 = 0x160f4;
pub(super) const LOG_HOOK_FLAG: u32 = 0x4000;

/// The profiler's queue: the head and the tail of the sample list, and its count.
const PROF_HEAD_AT: u32 = 0x0165_f108;
const PROF_TAIL_AT: u32 = 0x0165_f10c;
const PROF_COUNT_AT: u32 = 0x0165_f110;
/// A sample's link (+0 next, +4 previous, +8 the owner: the queue's head while linked) and its name (+0x30).
const PROF_PREV_AT: u32 = 4;
const PROF_OWNER_AT: u32 = 8;
const PROF_NAME_AT: u32 = 0x30;
/// A channel's fields: its sample (+0) and its flag byte (+4).
const CHAN_FLAG_AT: u32 = 4;

/// `E46ProfSample::EnterName` (0x528820) on `sample`: a linked sample leaves its queue (and the count drops), then goes to
/// the tail, the name is stored and the counter (+0x10) is read. Returns the eax the counter call leaves.
pub fn prof_enter(host: &mut impl EnterHost, mem: &mut Mem, sample: u32, name: u32) -> u32 {
    let owner = word(mem, sample + PROF_OWNER_AT);
    if owner != 0 {
        let count = word(mem, owner + 8).wrapping_sub(1);
        set_word(mem, owner + 8, count);
        host.unlink(sample);
    }
    let tail = word(mem, PROF_TAIL_AT);
    set_word(mem, sample + PROF_PREV_AT, tail);
    set_word(mem, sample, 0);
    let head = if tail != 0 {
        set_word(mem, tail, sample);
        word(mem, PROF_HEAD_AT)
    } else {
        sample
    };
    set_word(mem, PROF_HEAD_AT, head);
    set_word(mem, PROF_TAIL_AT, sample);
    set_word(mem, sample + PROF_OWNER_AT, PROF_HEAD_AT);
    let count = word(mem, PROF_COUNT_AT).wrapping_add(1);
    set_word(mem, PROF_COUNT_AT, count);
    set_word(mem, sample + PROF_NAME_AT, name);
    host.counter(sample + START)
}

/// `E46ProfChan::Enter` (0x528880) on the channel at `chan`: its pointer is `sample`, which is entered, then its flag byte
/// is stored. Returns the eax of the enter, which the channel's call leaves.
pub fn prof_chan_enter(host: &mut impl EnterHost, mem: &mut Mem, chan: u32, sample: u32, name: u32, flag: u32) -> u32 {
    set_word(mem, chan, sample);
    let eax = prof_enter(host, mem, sample, name);
    mem.set_u8(chan + CHAN_FLAG_AT, flag as u8);
    eax
}

//! The poll replayed over a recorded RSF session (RSS1, `harness/session.py`; `notes/verification.md`, "Input chain
//! check"): each car control call (CTL1) against the payload the chain gives for the polls (POL1) delivered before it.
//!
//! The chain: the game's devices (those it polls) from DEV1 and FMT1, ranges from PRP1 (`GetProperty(DIPROP_RANGE)`),
//! else DirectInput's default; the bindings from the attempt's `.acm` (FIL1); each poll group (a POL1 of each game
//! device, in order) through [`Poll::tic`] and NGP's gears; then [`Filter`] and [`CarControl::ngp`] per CTL1.
//!
//! NGP's input thread polls at 1000 Hz and posts each `CSigCarControlInput` to a strand that the physics thread runs
//! once per burst of steps (steps back to back, under [`BURST_GAP`] apart), shortly before the burst's first car
//! control call ([`DELIVERY_LEAD`]); the moment is not recorded. A burst takes the polls recorded before that estimate;
//! when the payloads disagree, the replay tries the neighbouring poll counts and keeps the first that reproduces the
//! burst ([`Report::moved`]).
//!
//! Two inputs of NGP's handler are physics state the session lacks: the drive's commanded gear (a selection of it is
//! dropped), taken from the recorded flag ([`Report::gear_from_recording`]); and the profile's filter grid, taken as
//! inactive (every recorded value passes unfiltered). The axes' move counters carry over from the previous attempt
//! of the same game process; the first attempt starts them fresh.

use super::{Device, Object, Poll, DEFAULT_RANGE};
use crate::control::CarControl;
use crate::filter::{Cell, Filter, Filtered, Grid, Raw, CHANNELS};
use rbr_formats::saves::action_map::ActionMap;

/// QPC ticks (10 MHz under Wine) between a step's end and the next car control call that keep it in the burst.
pub const BURST_GAP: u64 = 300;
/// How long before a burst's first car control call its polls are delivered, QPC ticks.
pub const DELIVERY_LEAD: u64 = 200;
/// How many poll groups the alignment looks back and ahead of the estimate.
const LOOK_BACK: usize = 30;
const LOOK_AHEAD: usize = 6;
/// DIPROP_RANGE.
const DIPROP_RANGE: u32 = 4;
/// `CSigCarControl` flags from the pause menu's call for help and the reset (`control::FLAG_MAP`), not the poll's.
const MENU_FLAGS: u32 = 0x30;

/// One RSS1 record: tag, QPC, thread, the payload after them.
#[derive(Clone, Debug)]
pub struct Rec {
    pub tag: [u8; 4],
    pub qpc: u64,
    pub thread: u32,
    pub body: Vec<u8>,
}

impl Rec {
    fn u32(&self, at: usize) -> u32 {
        self.body.get(at..at + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn blob(&self, at: usize) -> &[u8] {
        let n = self.u32(at) as usize;
        self.body.get(at + 4..at + 4 + n).unwrap_or(&[])
    }
}

/// The records of an RSS1 file; a record cut off at the end is dropped.
pub fn read(bytes: &[u8]) -> Result<Vec<Rec>, String> {
    if bytes.get(..4) != Some(b"RSS1") {
        return Err("not an RSS1 file".into());
    }
    let (mut at, mut out) = (16, Vec::new());
    while at + 8 <= bytes.len() {
        let tag: [u8; 4] = bytes[at..at + 4].try_into().unwrap();
        let n = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let Some(payload) = bytes.get(at + 8..at + 8 + n).filter(|p| p.len() >= 12) else { break };
        out.push(Rec {
            tag,
            qpc: u64::from_le_bytes(payload[..8].try_into().unwrap()),
            thread: u32::from_le_bytes(payload[8..12].try_into().unwrap()),
            body: payload[12..].to_vec(),
        });
        at += 8 + n;
    }
    Ok(out)
}

/// The profile's action map the game opened (the last `.acm` FIL1), if the session holds one.
pub fn action_map(recs: &[Rec]) -> Option<Result<ActionMap, String>> {
    let acm = recs.iter().rfind(|r| &r.tag == b"FIL1" && r.blob(0).to_ascii_lowercase().ends_with(b".acm"))?;
    let path_len = acm.u32(0) as usize;
    Some(ActionMap::read(acm.blob(4 + path_len)).map_err(|e| format!("the .acm: {e:?}")))
}

/// The counts of one attempt's replay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Car control calls compared, and those equal in all eight words.
    pub compared: usize,
    pub equal: usize,
    /// Equal with the poll delivery the recorded QPCs give, before any alignment.
    pub equal_by_time: usize,
    /// Bursts, and those whose delivered poll count the alignment moved off the estimate.
    pub bursts: usize,
    pub moved: usize,
    /// Calls whose gear selection flag (drop when the drive is in that gear) came from the recording.
    pub gear_from_recording: usize,
    /// Calls skipped: the pause menu's call for help or reset, not the poll's payload.
    pub menu: usize,
    /// Mismatching calls per payload word.
    pub words: [usize; 8],
    /// The first mismatching call: its step, our words, the recorded words.
    pub first: Option<(u32, [u32; 8], [u32; 8])>,
    /// The `.acm` record the devices bound, and the device names.
    pub record: Option<usize>,
    pub devices: Vec<String>,
}

/// One poll group's emitted control.
#[derive(Clone, Copy, Debug)]
struct Emitted {
    qpc: u64,
    raw: Raw,
}

/// The state that outlives an attempt: the game's devices (their move counters), keyed by the main thread.
pub struct Process {
    thread: u32,
    poll: Poll,
}

/// The game's devices from the session's DEV1, FMT1 and PRP1: those it polls (POL1 from the game, ordered by device
/// number). Returns the device numbers and the poll.
fn devices(recs: &[Rec]) -> (Vec<u32>, Poll, Vec<String>) {
    let mut polled: Vec<u32> = recs.iter().filter(|r| &r.tag == b"POL1").map(|r| r.u32(0)).collect();
    polled.sort_unstable();
    polled.dedup();
    let mut devices = Vec::new();
    let mut names = Vec::new();
    for &n in &polled {
        let dev = recs.iter().find(|r| &r.tag == b"DEV1" && r.u32(0) == n);
        let fmt = recs.iter().rev().find(|r| &r.tag == b"FMT1" && r.u32(0) == n);
        let (Some(dev), Some(fmt)) = (dev, fmt) else { continue };
        let guid: [u8; 16] = dev.body[4..20].try_into().unwrap();
        names.push(String::from_utf8_lossy(dev.blob(36)).into_owned());
        let count = fmt.u32(8) as usize;
        let objects: Vec<Object> = (0..count)
            .map(|k| {
                let at = 12 + k * 28 + 16;
                Object { ofs: fmt.u32(at), kind: fmt.u32(at + 4) }
            })
            .collect();
        // PRP1: device, op, property, how, obj, hresult, n, data (lMin, lMax).
        let range = |kind: u32| {
            recs.iter()
                .rev()
                .find(|r| {
                    &r.tag == b"PRP1"
                        && r.u32(0) == n
                        && r.u32(8) == DIPROP_RANGE
                        && r.u32(20) & 0x8000_0000 == 0
                        && (r.u32(12) == 0 || (r.u32(12) == 2 && r.u32(16) == kind))
                })
                .map_or(DEFAULT_RANGE, |r| (r.u32(28) as i32, r.u32(32) as i32))
        };
        devices.push(Device::new(guid, &objects, range));
    }
    let numbers = polled.into_iter().filter(|&n| recs.iter().any(|r| &r.tag == b"FMT1" && r.u32(0) == n)).collect();
    (numbers, Poll::new(devices), names)
}

/// The grid the replay filters with: every cell inactive, so each channel passes as polled.
fn pass_through() -> Grid {
    let cell = Cell {
        active: false,
        nonlinear: false,
        lin: 1.0,
        c2: 0.0,
        c3: 0.0,
        rise: 0.0,
        fall: 0.0,
        fade: 0.0,
        fade_speed: 0.0,
    };
    Grid { cells: [[cell; 4]; CHANNELS], select: [0; CHANNELS] }
}

/// Replays one attempt's session. `process` carries the devices from the previous attempt of the same game.
pub fn replay(recs: &[Rec], process: &mut Option<Process>) -> Result<Report, String> {
    let map = action_map(recs).ok_or("no .acm (FIL1) in the session")??;
    let (numbers, fresh, names) = devices(recs);
    let thread = recs.iter().find(|r| &r.tag == b"CTL1").map_or(0, |r| r.thread);
    let mut poll = match process.take() {
        Some(p)
            if p.thread == thread && p.poll.devices.iter().map(|d| d.guid).eq(fresh.devices.iter().map(|d| d.guid)) =>
        {
            p.poll
        }
        _ => fresh,
    };
    let record = poll.bind(&map);
    let last = *numbers.last().ok_or("no game device polled")?;

    // The poll groups, in record order.
    let mut emitted = Vec::new();
    for r in recs.iter().filter(|r| &r.tag == b"POL1") {
        let n = r.u32(0);
        let Some(k) = numbers.iter().position(|&d| d == n) else { continue };
        if r.u32(4) & 0x8000_0000 == 0 {
            poll.devices[k].update(r.blob(8), 0.0);
        }
        if n == last {
            emitted.push(Emitted { qpc: r.qpc, raw: poll.ngp_gears(poll.tic(true), 0) });
        }
    }
    *process = Some(Process { thread, poll });

    let steps: Vec<u64> = recs.iter().filter(|r| &r.tag == b"STP1").map(|r| r.qpc).collect();
    let calls: Vec<&Rec> = recs.iter().filter(|r| &r.tag == b"CTL1").collect();
    let mut bursts: Vec<Vec<usize>> = Vec::new();
    for (i, c) in calls.iter().enumerate() {
        let prev = steps.partition_point(|&q| q < c.qpc).checked_sub(1).map(|j| steps[j]);
        match (bursts.last_mut(), prev) {
            (Some(b), Some(p)) if c.qpc - p <= BURST_GAP => b.push(i),
            _ => bursts.push(vec![i]),
        }
    }

    let mut report = Report { record, devices: names, bursts: bursts.len(), ..Report::default() };
    let mut state = Delivery { delivered: 0, filter: Filter::new(pass_through()), current: None };
    for burst in &bursts {
        let start = calls[burst[0]].qpc.saturating_sub(DELIVERY_LEAD);
        let estimate = emitted.partition_point(|e| e.qpc < start);
        let lo = estimate.saturating_sub(LOOK_BACK).max(state.delivered);
        let mut candidates: Vec<usize> = (lo..=(estimate + LOOK_AHEAD).min(emitted.len())).collect();
        candidates.sort_by_key(|&k| (k.abs_diff(estimate), k));
        let mut chosen = None;
        for &k in candidates.iter().filter(|&&k| k >= state.delivered) {
            let mut trial = state.clone();
            let out = trial.run(&emitted, k, burst.iter().map(|&i| calls[i]));
            let good = out.iter().all(|o| o.equal());
            if chosen.is_none() || good {
                chosen = Some((k, trial, out));
            }
            if good {
                break;
            }
        }
        let Some((k, next, out)) = chosen else { continue };
        if k != estimate {
            report.moved += 1;
        }
        // The calls' result with the estimate alone, for `equal_by_time`.
        let by_time = if k == estimate {
            out.clone()
        } else {
            state.clone().run(&emitted, estimate.max(state.delivered), burst.iter().map(|&i| calls[i]))
        };
        report.equal_by_time += by_time.iter().filter(|o| !o.menu && o.equal()).count();
        state = next;
        for o in out {
            if o.menu {
                report.menu += 1;
                continue;
            }
            report.compared += 1;
            report.gear_from_recording += usize::from(o.gear_from_recording);
            if o.equal() {
                report.equal += 1;
                continue;
            }
            for (w, (a, b)) in report.words.iter_mut().zip(o.ours.iter().zip(&o.recorded)) {
                *w += usize::from(a != b);
            }
            report.first.get_or_insert((o.step, o.ours, o.recorded));
        }
    }
    Ok(report)
}

/// The filter's side of the delivery: how many poll groups it took, the filter, the last raw control.
#[derive(Clone)]
struct Delivery {
    delivered: usize,
    filter: Filter,
    current: Option<Raw>,
}

#[derive(Clone)]
struct Outcome {
    step: u32,
    ours: [u32; 8],
    recorded: [u32; 8],
    menu: bool,
    gear_from_recording: bool,
}

impl Outcome {
    fn equal(&self) -> bool {
        self.menu || self.ours == self.recorded
    }
}

impl Delivery {
    /// Delivers the poll groups up to `upto` (`E16Filter::NotifyB` each), then one filter tic and NGP's payload per
    /// call.
    fn run<'a>(&mut self, emitted: &[Emitted], upto: usize, calls: impl Iterator<Item = &'a Rec>) -> Vec<Outcome> {
        while self.delivered < upto {
            let raw = emitted[self.delivered].raw;
            self.filter.notify(&raw);
            self.current = Some(raw);
            self.delivered += 1;
        }
        calls
            .map(|c| {
                let recorded: [u32; 8] = std::array::from_fn(|k| c.u32(4 + 4 * k));
                let filtered: Filtered = self.filter.tic(0.0);
                // The drive's commanded gear: the selection is dropped when the recording dropped it.
                let selects = filtered.flags & 0x04 != 0 && filtered.gear != 0;
                let selected = u32::from(filtered.gear).wrapping_sub(1);
                let current = (selects && recorded[5] & 0x04 == 0).then_some(selected);
                let ours = CarControl::ngp(&filtered, current).words().map(|w| w.unwrap_or(0));
                Outcome {
                    step: c.u32(0),
                    ours: if self.current.is_some() { ours } else { [u32::MAX; 8] },
                    recorded,
                    menu: recorded[5] & MENU_FLAGS != 0,
                    gear_from_recording: selects,
                }
            })
            .collect()
    }
}

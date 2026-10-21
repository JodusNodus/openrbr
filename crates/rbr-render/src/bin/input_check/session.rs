//! The poll replayed over a recorded RSF session (RSS1, `harness/session.py`; `notes/verification.md`, "Input chain
//! check"): each car control call (CTL1) against the payload the chain gives for the polls (POL1) delivered before it.
//!
//! The chain: the game's devices (those it polls) from DEV1 and FMT1, ranges from PRP1 (`GetProperty(DIPROP_RANGE)`),
//! else DirectInput's default; the attempt's `.acm` (FIL1) imported into our bindings
//! (`rbr_render::bindings::Bindings::import`, the devices by the vendor and product ids of their DEV1 product GUID)
//! and bound from them ([`Bindings::slots`]), as a live drive binds; each poll group (a POL1 of each game device, in
//! order) through [`Poll::tic`] and NGP's gears; then [`Filter`] and [`CarControl::ngp`] per CTL1.
//!
//! NGP's input thread polls at 1000 Hz and posts each `CSigCarControlInput`; the input filter takes it
//! (`E16Filter::NotifyB`, DLV1) and the physics thread runs a burst of steps (back to back, under [`BURST_GAP`]
//! apart). With DLV1 records a burst takes the polls up to the one the last DLV1 before its first car control call
//! delivered (the last poll group recorded before it). Older sessions have none: a burst takes the polls recorded
//! before an estimate, shortly before its first call ([`DELIVERY_LEAD`]), and when the payloads disagree the replay
//! tries the neighbouring poll counts and keeps the first that reproduces the burst ([`Report::moved`]).
//!
//! Two inputs of NGP's handler are physics state the session lacks: the drive's commanded gear (a selection of it is
//! dropped), taken from the recorded flag ([`Report::gear_from_recording`]); and the profile's filter grid, taken as
//! inactive (every recorded value passes unfiltered). The axes' move counters carry over from the previous attempt
//! of the same game process; the first attempt starts them fresh.

use rbr_formats::saves::action_map::ActionMap;
use rbr_gameplay::control::{ngp_direct_gears, CarControl};
use rbr_gameplay::filter::{Cell, Filter, Filtered, Grid, Raw, CHANNELS};
use rbr_gameplay::poll::{keyboard, Device, Object, Poll, DEFAULT_RANGE, KEYBOARD};
use rbr_render::bindings::{self, Bindings};

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
    /// Equal with the poll delivery the recorded QPCs give (DLV1's, else the estimate), before any alignment.
    pub equal_by_time: usize,
    /// The session has DLV1 records: the delivery is recorded, not searched.
    pub delivered: bool,
    /// DLV1 records whose raw control is the poll group's recorded last before it, and the DLV1 records: the poll
    /// checked on its own (a playback run's car control calls carry the playback's input, not the poll's).
    pub deliveries: (usize, usize),
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

/// A game device's instance GUID and our id of it (`None` the keyboard).
type Id = ([u8; 16], Option<bindings::Device>);

/// The game's devices from the session's DEV1, FMT1 and PRP1: those it polls (POL1 from the game, ordered by device
/// number). Returns the device numbers, the poll, the devices' names and their ids (`None` the keyboard).
fn devices(recs: &[Rec]) -> (Vec<u32>, Poll, Vec<String>, Vec<Id>) {
    let mut polled: Vec<u32> = recs.iter().filter(|r| &r.tag == b"POL1").map(|r| r.u32(0)).collect();
    polled.sort_unstable();
    polled.dedup();
    let (mut numbers, mut devices, mut names) = (Vec::new(), Vec::new(), Vec::new());
    let mut ids: Vec<Id> = Vec::new();
    for &n in &polled {
        let dev = recs.iter().find(|r| &r.tag == b"DEV1" && r.u32(0) == n);
        let fmt = recs.iter().rev().find(|r| &r.tag == b"FMT1" && r.u32(0) == n);
        let (Some(dev), Some(fmt)) = (dev, fmt) else { continue };
        let Some(guid) = dev.body.get(4..20).and_then(|g| <[u8; 16]>::try_from(g).ok()) else { continue };
        numbers.push(n);
        names.push(String::from_utf8_lossy(dev.blob(36)).into_owned());
        if guid == KEYBOARD {
            devices.push(keyboard());
            ids.push((guid, None));
            continue;
        }
        // The product GUID's first field is `MAKELONG(vendor, product)`; the index counts the devices before it.
        let product = dev.u32(20);
        let (vendor, product) = (product as u16, (product >> 16) as u16);
        let index =
            ids.iter().filter(|(_, d)| matches!(d, Some(d) if (d.vendor, d.product) == (vendor, product))).count();
        ids.push((guid, Some(bindings::Device { vendor, product, index: index as u32 })));
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
    (numbers, Poll::new(devices), names, ids)
}

/// A DLV1 record's raw control (`CSigCarControlInput`, [`Raw`]'s fields).
fn raw_of(r: &Rec) -> Option<Raw> {
    let b = r.body.get(8..8 + 0x40)?;
    let u = |at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let channels = std::array::from_fn(|k| f32::from_bits(u(0x14 + 4 * k)));
    let select = std::array::from_fn(|k| u(0x2c + 4 * k));
    Some(Raw { flags: u(0), aux: u(4), byte2: u(8), channels, gear: b[0x28], select })
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
    let (numbers, fresh, names, ids) = devices(recs);
    let thread = recs.iter().find(|r| &r.tag == b"CTL1").map_or(0, |r| r.thread);
    let mut poll = match process.take() {
        Some(p)
            if p.thread == thread && p.poll.devices.iter().map(|d| d.guid).eq(fresh.devices.iter().map(|d| d.guid)) =>
        {
            p.poll
        }
        _ => fresh,
    };
    // The game binds the first record naming as many devices as it has, all of them (`FindRec`).
    let mut ours = Bindings::default();
    let fits = |r: &rbr_formats::saves::action_map::Record| r.devices.len() == ids.len();
    let fitting = ActionMap { records: map.records.iter().filter(|r| fits(r)).cloned().collect() };
    let resolve = |g: &[u8; 16]| ids.iter().find(|(guid, _)| guid == g).map(|&(_, d)| d);
    let record = ours.import(&fitting, resolve).map(|k| {
        let chosen = &fitting.records[k];
        map.records.iter().position(|r| r == chosen).unwrap_or(k)
    });
    let index = |d: Option<bindings::Device>| ids.iter().position(|&(_, id)| id == d);
    let slots: Vec<_> = ours.slots().into_iter().filter_map(|(s, d, o)| Some((s, index(d)?, o))).collect();
    poll.bind(&slots);
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
            // NGP's emit (`0x100ed540`): with the automatic gearbox off, the direct gear actions' selection.
            let raw = poll.tic(true);
            let (flags, gear) = ngp_direct_gears(raw.flags, raw.gear, poll.gear_actions(), 0);
            emitted.push(Emitted { qpc: r.qpc, raw: Raw { flags, gear, ..raw } });
        }
    }
    *process = Some(Process { thread, poll });

    let steps: Vec<u64> = recs.iter().filter(|r| &r.tag == b"STP1").map(|r| r.qpc).collect();
    let calls: Vec<&Rec> = recs.iter().filter(|r| &r.tag == b"CTL1").collect();
    // DLV1: the raw control each delivery took, against the poll group recorded last before it.
    let dlv: Vec<&Rec> = recs.iter().filter(|r| &r.tag == b"DLV1").collect();
    let deliveries: Vec<u64> = dlv.iter().map(|r| r.qpc).collect();
    let delivered = !dlv.is_empty();
    let polled_before = |q: u64| emitted.partition_point(|e| e.qpc < q).checked_sub(1).map(|k| emitted[k].raw);
    let deliveries_equal =
        dlv.iter().filter(|r| raw_of(r).is_some_and(|raw| polled_before(r.qpc) == Some(raw))).count();
    let mut bursts: Vec<Vec<usize>> = Vec::new();
    for (i, c) in calls.iter().enumerate() {
        let prev = steps.partition_point(|&q| q < c.qpc).checked_sub(1).map(|j| steps[j]);
        match (bursts.last_mut(), prev) {
            (Some(b), Some(p)) if c.qpc - p <= BURST_GAP => b.push(i),
            _ => bursts.push(vec![i]),
        }
    }

    let checked = (deliveries_equal, dlv.len());
    let mut report =
        Report { record, devices: names, bursts: bursts.len(), delivered, deliveries: checked, ..Report::default() };
    let mut state = Delivery { delivered: 0, filter: Filter::new(pass_through()), current: None };
    for burst in &bursts {
        let first = calls[burst[0]].qpc;
        let estimate = if delivered {
            // The poll group the last delivery before the burst took: the last one recorded before it.
            let last = deliveries.partition_point(|&q| q < first).checked_sub(1).map(|k| deliveries[k]);
            last.map_or(0, |q| emitted.partition_point(|e| e.qpc < q))
        } else {
            let start = first.saturating_sub(DELIVERY_LEAD);
            emitted.partition_point(|e| e.qpc < start)
        };
        let mut candidates: Vec<usize> = if delivered {
            vec![estimate.max(state.delivered)]
        } else {
            let lo = estimate.saturating_sub(LOOK_BACK).max(state.delivered);
            (lo..=(estimate + LOOK_AHEAD).min(emitted.len())).collect()
        };
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

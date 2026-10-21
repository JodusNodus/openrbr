//! NGP's engine wrapper's wear and death, and its idle table's lookup, against direct calls of the original in the
//! emulator (`scripts/ngp-engine-vectors.py` on `ngp-fwd41b.snap-10880`): 1,000 wrapper calls around the death's
//! threshold (480 deaths, each with one engine-fire emit) and 1,000 lookups on tables of 0 to 4 knots, which read
//! past their knots below two. Then 250 calls each of the 1.02 functions NGP repoints at its `.data` or its code
//! (`Sim8890` with and without the car's walk, `SimE970`, `SimECF0`, and `LaunchClutch` with the launch mode 0, 1, 2)
//! on fuzzed objects.

use std::collections::BTreeMap;

use rbr_physics::engine_driveline::engine::{self, CarWalkHost};
use rbr_physics::engine_driveline::layout::{self, EngineObjects, EngineOptions, EngineState, EngineTuning};
use rbr_physics::mem::Mem;
use rbr_physics::ngp::engine::{idle_table_lookup, is_fatal, rev_limit, worn_health};
use rbr_physics::vcu::layout::{launch_clutch, Tuning, VcuCalls};

#[test]
fn engine_wear_and_death_match_original_calls() {
    let (mut n, mut deaths) = (0, 0);
    for line in include_str!("vectors/ngp-engine.txt").lines().filter(|l| l.starts_with("w ")) {
        let w: Vec<u32> = line[2..].split(' ').map(|x| u32::from_str_radix(x, 16).unwrap()).collect();
        let f = |k: usize| f32::from_bits(w[k]);
        let alive = w[0] != 0;
        let (dt, speed, rpm, health) = (f(1), f(2), f(3), f(4));
        // The wrapper: the health wears only while alive; it dies (alive flag cleared, one emit) below 0.101.
        let after = if alive { worn_health(health, speed, rev_limit(rpm), dt) } else { health };
        let died = alive && is_fatal(after);
        assert_eq!(after.to_bits(), w[5], "{line}");
        assert_eq!(w[6] != 0, alive && !died, "{line}: alive");
        assert_eq!(w[7], u32::from(died), "{line}: emits");
        deaths += usize::from(died);
        n += 1;
    }
    assert_eq!(n, 1000);
    assert!(deaths > 300, "{deaths}");
}

#[test]
fn idle_table_lookup_matches_original_calls() {
    let mut short = 0;
    for line in include_str!("vectors/ngp-engine.txt").lines().filter(|l| l.starts_with("t ")) {
        let mut it = line[2..].split(' ');
        let n: usize = it.next().unwrap().parse().unwrap();
        let w: Vec<f64> = it.map(|x| f64::from_bits(u64::from_str_radix(x, 16).unwrap())).collect();
        let (v, xs, ys, want) = (w[0], &w[1..n + 4], &w[n + 4..2 * n + 7], w[2 * n + 7]);
        let got = idle_table_lookup(n, |k| xs[(k + 1) as usize], |k| ys[(k + 1) as usize], v);
        assert!(got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()), "{line}: {got}");
        short += usize::from(n < 2);
    }
    assert!(short > 300, "{short}");
}

/// One `c` line: the function, the fill's seed, the stack words, the words set over the fill, the words that changed,
/// eax, st0 and the walk.
struct Call {
    func: u32,
    seed: u64,
    words: Vec<u32>,
    sets: Vec<(u32, u32)>,
    diffs: BTreeMap<u32, u32>,
    eax: u32,
    st0: f64,
    walk: Vec<u32>,
}

const SIM8890: u32 = 0x518890;
const SIM_E970: u32 = 0x50e970;
const SIM_ECF0: u32 = 0x50ecf0;
const LAUNCH_CLUTCH: u32 = 0x5375b0;
/// The objects' offsets from the heap block (the script's `OBJECTS`): this, the scratch, and the filled ranges.
const THIS: u32 = 0x3000;
const SCRATCH: u32 = 0xb000;
const FILLED: [(u32, u32, f32); 3] = [(0x4000, 0x6880, 2.0), (0x8000, 0x8274, 1.0), (0xa000, 0xa080, 1.0)];
const END: u32 = 0xd000;

fn hex(x: &str) -> u32 {
    u32::from_str_radix(x, 16).unwrap()
}

fn pairs(field: &str) -> impl Iterator<Item = (u32, u32)> + '_ {
    field.split_whitespace().map(|kv| {
        let (k, v) = kv.split_once('=').unwrap();
        (hex(k), hex(v))
    })
}

/// The `d` line (NGP's tuning words) and the `c` lines.
fn repointed() -> (BTreeMap<u32, u32>, Vec<Call>) {
    let text = include_str!("vectors/ngp-engine.txt");
    let tuning = pairs(&text.lines().find(|l| l.starts_with("d ")).unwrap()[2..]).collect();
    let calls = text
        .lines()
        .filter(|l| l.starts_with("c "))
        .map(|line| {
            let fields: Vec<&str> = line[2..].split('|').collect();
            let mut head = fields[0].split_whitespace().map(hex);
            let (func, seed) = (head.next().unwrap(), u64::from(head.next().unwrap()));
            let mut ret = fields[3].split_whitespace();
            Call {
                func,
                seed,
                words: head.collect(),
                sets: pairs(fields[1]).collect(),
                diffs: pairs(fields[2]).collect(),
                eax: hex(ret.next().unwrap()),
                st0: f64::from_bits(u64::from_str_radix(ret.next().unwrap(), 16).unwrap()),
                walk: fields[4].split_whitespace().map(hex).collect(),
            }
        })
        .collect();
    (tuning, calls)
}

/// The script's `fill`: splitmix64 of (seed, at), its top 24 bits as a float in [-scale, scale).
fn fill(seed: u64, at: u32, scale: f32) -> f32 {
    let mut z = (seed << 32 | u64::from(at)).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ z >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ z >> 27).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ z >> 31) >> 40) as f32 * scale / 8_388_608.0 - scale
}

/// The call's objects before it: the heap block (found from `this`'s def pointer) with the fill and the set words.
fn objects(call: &Call) -> (u32, Mem) {
    let heap = call.sets[0].0 - THIS - 4;
    let mut image = vec![0u8; (END - THIS) as usize];
    let mut put = |at: u32, word: u32| {
        let k = (at - heap - THIS) as usize;
        image[k..k + 4].copy_from_slice(&word.to_le_bytes());
    };
    for (first, end, scale) in FILLED {
        for at in (heap + first..heap + end).step_by(4) {
            put(at, fill(call.seed, at, scale).to_bits());
        }
    }
    for &(at, word) in &call.sets {
        put(at, word);
    }
    let mut mem = Mem::new();
    mem.map(heap + THIS, &image);
    (heap, mem)
}

/// Every word of the block after the port: the recorded change, or the word before the call.
fn check_after(call: &Call, heap: u32, before: &Mem, after: &Mem) {
    for at in (heap + THIS..heap + END).step_by(4) {
        let want = call.diffs.get(&at).copied().unwrap_or_else(|| before.u32(at));
        assert_eq!(after.u32(at), want, "{:#x} seed {:x}: word {at:#x}", call.func, call.seed);
    }
}

/// `Sim8890`'s walk as recorded: S30 on the car, then MB560 (a, b) and its four floats.
struct Walk<'a>(&'a [u32]);

impl CarWalkHost for Walk<'_> {
    fn walk_object(&mut self, _car: u32) -> u32 {
        self.0[0]
    }

    fn transform_point(&mut self, a: u32, b: u32) -> [f32; 4] {
        assert_eq!([a, b], [self.0[1], self.0[2]], "MB560's arguments");
        std::array::from_fn(|k| f32::from_bits(self.0[3 + k]))
    }
}

/// `LaunchClutch`'s drive calls: `Get4520` reads the drive state's engine speed, `Set4820`'s value is kept.
struct Drive {
    engine: f32,
    sent: Option<f32>,
}

impl VcuCalls for Drive {
    fn engine_speed(&mut self, _drive: u32) -> f32 {
        self.engine
    }
    fn set_clutch_slip(&mut self, _drive: u32, value: f32) -> u32 {
        self.sent = Some(value);
        0
    }
    fn lock_speed(&mut self, _: u32) -> f32 {
        unreachable!()
    }
    fn car_speed(&mut self, _: u32) -> f32 {
        unreachable!()
    }
    fn set_diff_lock(&mut self, _: u32, _: u32, _: f32) -> u32 {
        unreachable!()
    }
    fn set_cut_flags(&mut self, _: u32, _: [u32; 4]) -> u32 {
        unreachable!()
    }
    fn set_engine_pedal(&mut self, _: u32, _: f32) {
        unreachable!()
    }
    fn set_brake_light(&mut self, _: u32, _: bool) {
        unreachable!()
    }
    fn set_neutral_light(&mut self, _: u32, _: bool) {
        unreachable!()
    }
    fn query_slip(&mut self, _: u32) -> bool {
        unreachable!()
    }
    fn set_dirty(&mut self, _: u32) {
        unreachable!()
    }
}

#[test]
fn repointed_engine_and_clutch_match_original_calls() {
    let (data, calls) = repointed();
    let d = |at: u32| f32::from_bits(data[&at]);
    let mut tuning = EngineTuning { stall_speed: d(0x1017_1abc), ..EngineTuning::STOCK };
    tuning.air.temperature = d(0x1017_1adc);
    let clutch = Tuning {
        clutch_from: d(0x1017_1ae4),
        clutch_to: d(0x1017_2338),
        clutch_slope: d(0x1017_67c4),
        ..Tuning::STOCK
    };
    let mut counts: BTreeMap<(u32, &str), usize> = BTreeMap::new();
    for call in &calls {
        let (heap, before) = objects(call);
        let mut mem = before.clone();
        let this = heap + THIS;
        let x = || f32::from_bits(call.words[0]);
        let case = match call.func {
            SIM8890 => {
                let eax = layout::sim8890(&mut Walk(&call.walk), &mut mem, this, x(), &tuning);
                assert_eq!(eax, call.eax, "Sim8890 seed {:x}: eax", call.seed);
                if call.walk.is_empty() {
                    "no walk"
                } else {
                    "walk"
                }
            }
            SIM_E970 => {
                let st0 = layout::sim_e970(&mem, this);
                assert_eq!(st0.to_bits(), call.st0.to_bits(), "SimE970 seed {:x}", call.seed);
                ""
            }
            SIM_ECF0 => {
                let objects = EngineObjects::at(&mem, this);
                let speed = EngineState::load(&mem, objects.state).speed;
                let running =
                    engine::stall_guard(speed, &EngineOptions::load(&mem, objects.options), tuning.stall_speed);
                assert_eq!(u32::from(running), call.eax, "SimECF0 seed {:x}", call.seed);
                if speed > tuning.stall_speed {
                    "above"
                } else {
                    "at or below"
                }
            }
            LAUNCH_CLUTCH => {
                let scratch = heap + SCRATCH;
                let drive = mem.u32(scratch + 0x58);
                let mut calls = Drive { engine: mem.f32(mem.u32(drive + 8) + 0x45c), sent: None };
                let eax = launch_clutch(&mut mem, &mut calls, scratch, x(), clutch);
                // Set4820: SetDirty on its tweak at +0x54, then the value at +0x60; its eax is the caller's.
                if let Some(value) = calls.sent {
                    rbr_physics::ntweak::set_dirty(&mut mem, drive + 0x54);
                    mem.set_f32(drive + 0x60, value);
                    "sent"
                } else {
                    assert_eq!(eax, call.eax, "LaunchClutch seed {:x}: eax", call.seed);
                    "launch mode 0"
                }
            }
            other => panic!("{other:#x}"),
        };
        check_after(call, heap, &before, &mem);
        *counts.entry((call.func, case)).or_default() += 1;
    }
    let count = |func, case| counts.get(&(func, case)).copied().unwrap_or(0);
    assert!(count(SIM8890, "walk") > 50 && count(SIM8890, "no walk") > 50, "{counts:?}");
    assert!(count(SIM_ECF0, "above") > 50 && count(SIM_ECF0, "at or below") > 50, "{counts:?}");
    assert!(count(LAUNCH_CLUTCH, "sent") > 50 && count(LAUNCH_CLUTCH, "launch mode 0") > 50, "{counts:?}");
    assert_eq!(calls.len(), 1000);
}

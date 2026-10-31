//! The race clock and the outside-assistance penalty against the original's HUD: `peek/rcv-<drive>.peek.bin`, the
//! probes `peek/rcv-race-clock.peek` took after each `StepForward` of three playback runs where the car is put back on
//! the road (a call for help with the car on its side, a brake wall, a script character's box).
//!
//! The HUD ticks once per outer frame, after the frame's steps, with the frame's `facc`; the records between two steps
//! show either no tick or one. A playback frame's `facc` is wall time in whole steps: one step's time, or a few more
//! when the playback stalled (`tick::FrameClock`'s f64 sum, rounded once). Each record must be the previous one with no
//! tick or with one `RaceClock::tick` of such a `facc`, then, on the step that emitted
//! `CSigOutsideAssistance`, the receiver's restart (the timer at 0, the code). Every field is compared bit for bit.
//!
//! The playback's HUD runs on its own clock, about one tick of one step's time per 8 steps, so the car crosses the
//! start line while the HUD still counts down: each drive takes one jump start (the start line's `M579430`), which
//! the records show before that frame's tick. The test asks for exactly one, with the countdown's rest and 10 s.
//!
//! Runs only when `RBR_RECORDINGS` names the recordings directory; skipped otherwise.

use std::path::PathBuf;

use rbr_gameplay::race_clock::{AssistSettings, RaceClock};
use rbr_recording::peek::{PeekFile, Probe, Record};

mod common;
use common::{peeked_f32, peeked_u32};

/// One step's time in these runs.
const STEP: f32 = 0.007;
/// The longest stall a frame may span, in steps.
const MAX_FRAME_STEPS: u32 = 64;

/// `facc` of a frame of `n` steps: the f64 sum of the widened step, rounded once.
fn facc(n: u32) -> f32 {
    (0..n).fold(0.0f64, |sum, _| sum + f64::from(STEP)) as f32
}
/// The driving state (`CApplication` +0x728); the HUD ticks only there.
const DRIVING: u32 = 1;

/// (drive, the step that emitted the outside assistance, its code, the penalty added, if the run reaches it: the
/// playback's HUD runs about 8 times slower than the steps, so only the earliest assist gets 1 s of HUD time by step 3200)
const DRIVES: [(&str, u32, u32, Option<f32>); 3] = [
    ("rcv-help-corolla-34", 1957, 1, Some(10.0)),
    ("rcv-stuck-206-64", 2700, 2, None),
    ("rcv-retire-impreza00-45", 2144, 2, None),
];

/// The HUD's clock as one record shows it.
fn seen(record: &Record, probes: &[Probe]) -> RaceClock {
    RaceClock {
        countdown: peeked_f32(record, probes, "hud_cd"),
        start_fired: peeked_u32(record, probes, "hud_anim") != 0,
        running: peeked_u32(record, probes, "hud_gate") == 0,
        time: peeked_f32(record, probes, "hud_clock"),
        assist_timer: peeked_f32(record, probes, "hud_t"),
        fade: peeked_f32(record, probes, "hud_fade"),
        code: peeked_u32(record, probes, "hud_code"),
        jumped: false,
    }
}

/// Bit-for-bit equality of every field.
fn same(a: RaceClock, b: RaceClock) -> bool {
    let bits = |c: RaceClock| {
        (
            c.countdown.to_bits(),
            c.start_fired,
            c.running,
            c.time.to_bits(),
            c.assist_timer.to_bits(),
            c.fade.to_bits(),
            c.code,
            c.jumped,
        )
    };
    bits(a) == bits(b)
}

#[test]
fn the_race_clock_follows_the_hud_through_each_assist() {
    let Some(recordings) = std::env::var_os("RBR_RECORDINGS").map(PathBuf::from) else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    for (drive, emit_step, code, penalty) in DRIVES {
        let path = recordings.join("peek").join(format!("{drive}.peek.bin"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = PeekFile::parse(&bytes).expect("peek file");
        let probes = &file.probes;
        let records: Vec<Record> = file.records().collect::<Result<_, _>>().expect("peek records");
        let step_of = |r: &Record| peeked_u32(r, probes, "steps");
        let settings = AssistSettings {
            mode: peeked_u32(&records[0], probes, "flow_mode"),
            rally_level: peeked_u32(&records[0], probes, "help_rly"),
            level: peeked_u32(&records[0], probes, "help_set"),
            false_starts: 0,
        };
        let (mut ticks, mut idle, mut emits) = (0, 0, Vec::new());
        let (mut penalties, mut jumps, mut stalls) = (Vec::new(), Vec::new(), Vec::new());
        let mut jumped = false;
        for pair in records.windows(2) {
            let before = RaceClock { jumped, ..seen(&pair[0], probes) };
            let after = seen(&pair[1], probes);
            let step = step_of(&pair[1]);
            if peeked_u32(&pair[0], probes, "app_state") != DRIVING {
                assert!(same(before, after), "{drive} step {step}: the HUD moved outside driving");
                continue;
            }
            // The receiver's restart, when this step emitted: the timer is 0 and the code the step's.
            let emitted = after.assist_timer == 0.0 && before.assist_timer != 0.0;
            let receive = |c: RaceClock| {
                if emitted {
                    c.outside_assistance(after.code)
                } else {
                    c
                }
            };
            if emitted {
                emits.push((step, after.code));
            }
            // No tick, a tick, or the start line's jump start and then the tick.
            let (jumped_clock, jump) = before.jump_start(settings);
            let ticked = (1..=MAX_FRAME_STEPS).find_map(|n| {
                [(before, None), (jumped_clock, jump)].into_iter().find_map(|(start, jump)| {
                    let (clock, events) = start.tick(facc(n), settings);
                    same(receive(RaceClock { jumped: false, ..clock }), after).then_some((events, jump, n))
                })
            });
            if let Some((events, jump, n)) = ticked {
                ticks += 1;
                if n > 1 {
                    stalls.push((step, n));
                }
                penalties.extend(events.penalty.map(|p| (step, p)));
                if let Some(p) = jump {
                    jumps.push((step, p));
                    jumped = true;
                }
            } else {
                assert!(
                    same(receive(RaceClock { jumped: false, ..before }), after),
                    "{drive} step {step}: neither no tick, a tick nor a jump start gives the HUD's clock\n  before {before:?}\n  after  {after:?}\n  ticked {:?}",
                    before.tick(STEP, settings).0
                );
                idle += 1;
            }
        }
        eprintln!(
            "{drive}: {ticks} HUD frames ticked (longer frames (step, steps): {stalls:?}), {idle} records without a tick, jump starts {jumps:?}, emits {emits:?}, penalties {penalties:?}"
        );
        assert_eq!(jumps.len(), 1, "{drive}: one jump start");
        assert_eq!(jumps[0].1, 10.0, "{drive}: the jump start's penalty");
        assert_eq!(emits, [(emit_step, code)], "{drive}: the outside assistance");
        assert_eq!(
            penalties.iter().map(|&(_, p)| p).collect::<Vec<_>>(),
            penalty.into_iter().collect::<Vec<_>>(),
            "{drive}: the penalty"
        );
    }
}

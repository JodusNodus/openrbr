//! The exhaust pops and bangs of `CCarSound::M6a0` (0x4b06a0, the `EngineNoise` handler, read in the disassembly) from
//! the misfire word `p[4]`, and the engine fire (`CCarSound::OnEngineFire` 0x4ae3a0, `Md1c0` 0x4ad1c0). Pure.
//! rbr-recomp `notes/sound.md` 5.3 and 5.4.
//!
//! Each step with a misfire output: a pop (`Mf280` 0x4af280, `CAR::EXHAUST`), above 0.9 a bang (`Mf3f0` 0x4af3f0,
//! `CAR::BANGS`), and above 0.75 a pop run starts unless one runs: one of four tables at 0x7c3808 (`rand() & 3`) of
//! `CARn::EXHAUSTNEW` pops (`Mf180` 0x4af180) with the gap before the next in samples at 44100 Hz, played by the frame
//! update (0x4b0940). A rising engine speed ends the run. Each kind has one voice: a new pop stops the last.

use rbr_recording::sound::Sound;

use crate::voice::{Command, Repeat, Shot};

/// The misfire levels: a pop on any non-zero output, from the bank's fifth sample up above 0.8 (0x72f610); a bang
/// above 0.9 (0x795e90); a run above 0.75 (0x72f448). The pop's pitch is `p4 * 0.5 + 0.75`.
const POP_HIGH: f32 = 0.8;
const POP_HIGH_FROM: usize = 4;
const BANG: f32 = 0.9;
const RUN: f32 = 0.75;
/// rpm per rad/s (0x7319a4); the clamp is the engine voice's (`crate::engine::MIN_RPM`, `MAX_RPM`: the same
/// instructions, whose low end NGP reads from NGP.dll).
const RPM_PER_RAD: f32 = 9.549_296;
/// The run's clock: samples per second (0x731cdc, negated) at a speed of 1 (`+0x4194c`).
const RUN_RATE: f32 = 44100.0;
/// The four runs (0x7318e4, 0x731914, 0x731944, 0x731974): each entry the pop's sample and the gap after it; the
/// entry with a gap of 0 ends the run unplayed.
const RUNS: [[(u8, u16); 12]; 4] = [
    [
        (0, 904),
        (5, 932),
        (2, 1032),
        (1, 2379),
        (2, 1632),
        (3, 1729),
        (4, 1489),
        (5, 617),
        (6, 770),
        (7, 1800),
        (8, 2930),
        (9, 0),
    ],
    [
        (10, 904),
        (9, 1032),
        (8, 2379),
        (7, 932),
        (6, 2930),
        (5, 1632),
        (4, 1729),
        (3, 1489),
        (2, 617),
        (1, 770),
        (0, 1800),
        (9, 0),
    ],
    [
        (7, 1800),
        (2, 1632),
        (0, 904),
        (2, 1032),
        (5, 932),
        (1, 2379),
        (3, 1729),
        (8, 2930),
        (4, 1489),
        (5, 617),
        (6, 770),
        (9, 0),
    ],
    [
        (10, 904),
        (9, 932),
        (8, 1032),
        (7, 2379),
        (6, 1632),
        (5, 1729),
        (4, 1489),
        (3, 617),
        (2, 770),
        (1, 1800),
        (0, 2930),
        (5, 0),
    ],
];
/// The engine-fire call (`CSigCarDamageSpeech` 6: `sus_broke` in 1.02's table at 0x7c37e0).
pub const FIRE_CALL: u32 = 6;

/// A pop run playing: its table, the next entry and the samples left before it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Run {
    table: usize,
    next: usize,
    wait: i32,
}

/// The exhaust's state between steps.
#[derive(Debug, Clone, Default)]
pub struct Exhaust {
    /// `SOUNDDEF::EXHAUSTPOP` volume times the `ExhaustPop`, `Pop` and `Bang` amps; the `Enginefire` amp.
    pub exhaust_pop_gain: f32,
    pub pop_gain: f32,
    pub bang_gain: f32,
    pub fire_gain: f32,
    /// The banks' sizes: `CAR::EXHAUST`, `CARn::EXHAUSTNEW`, `CAR::BANGS`.
    pub counts: [usize; 3],
    run: Option<Run>,
    /// The last engine speed target, rpm (`+0x41750`, an int).
    rpm: i32,
}

impl Exhaust {
    pub fn new(gains: [f32; 4], counts: [usize; 3]) -> Exhaust {
        let [exhaust_pop_gain, pop_gain, bang_gain, fire_gain] = gains;
        Exhaust { exhaust_pop_gain, pop_gain, bang_gain, fire_gain, counts, ..Exhaust::default() }
    }

    /// The commands and damage-call codes of one step of `dt` seconds; `rand` stands for the original's `rand()`.
    pub fn step(
        &mut self,
        sound: &Sound,
        min_rpm: f32,
        dt: f32,
        mut rand: impl FnMut() -> u32,
    ) -> (Vec<Command>, Vec<u32>) {
        let mut out = Vec::new();
        let [exhaust, new, bangs] = self.counts;
        let misfire = sound.engine_f32(4);
        if misfire != 0.0 && exhaust > 0 {
            let index = if misfire > POP_HIGH && exhaust > POP_HIGH_FROM {
                rand() as usize % (exhaust - POP_HIGH_FROM) + POP_HIGH_FROM
            } else {
                rand() as usize % exhaust
            };
            out.push(Command::Effect {
                shot: Shot::ExhaustPop(index),
                gain: self.exhaust_pop_gain,
                pitch: misfire * 0.5 + RUN,
                repeat: Repeat::Restart,
            });
        }
        if misfire > BANG && bangs > 0 {
            let shot = Shot::Bang(rand() as usize % bangs);
            out.push(Command::Effect { shot, gain: self.bang_gain, pitch: 1.0, repeat: Repeat::Restart });
        }
        if misfire > RUN && self.run.is_none() {
            self.run = Some(Run { table: (rand() & 3) as usize, next: 0, wait: 0 });
        }
        let rpm = (sound.engine_f32(0) * RPM_PER_RAD).clamp(min_rpm, crate::engine::MAX_RPM);
        if rpm > self.rpm as f32 {
            self.run = None;
        }
        self.rpm = rpm as i32;
        // The frame update (0x4b0940): at most one pop a frame.
        if let Some(run) = &mut self.run {
            run.wait += (dt * -RUN_RATE) as i32;
            if run.wait <= 0 {
                let (kind, gap) = RUNS[run.table][run.next];
                run.wait = i32::from(gap);
                if new > 0 {
                    let shot = Shot::Pop(usize::from(kind) % new);
                    out.push(Command::Effect { shot, gain: self.pop_gain, pitch: 1.0, repeat: Repeat::Restart });
                }
                run.next += 1;
                if RUNS[run.table][run.next].1 == 0 {
                    self.run = None;
                }
            }
        }
        let mut calls = Vec::new();
        if sound.fires > 0 {
            calls.push(FIRE_CALL);
            out.push(Command::Effect { shot: Shot::Fire, gain: self.fire_gain, pitch: 1.0, repeat: Repeat::IfIdle });
        }
        (out, calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::MIN_RPM;

    fn sound(rad_s: f32, misfire: f32) -> Sound {
        let mut sound = Sound::default();
        sound.engine[0] = rad_s.to_bits();
        sound.engine[4] = misfire.to_bits();
        sound
    }

    fn shots(out: &[Command]) -> Vec<(Shot, f32)> {
        out.iter()
            .filter_map(|c| match *c {
                Command::Effect { shot, pitch, .. } => Some((shot, pitch)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_misfire_pops_bangs_and_starts_a_run_that_plays_on_a_falling_engine() {
        let mut e = Exhaust::new([1.0; 4], [11, 11, 6]);
        // A run started while the engine speeds up ends at once.
        assert_eq!(shots(&e.step(&sound(500.0, 0.8), MIN_RPM, 0.01, || 1).0).len(), 1);
        // 0.95 with the engine slowing: a pop from the fifth sample up at pitch 1.225, a bang, and run 1 (rand & 3),
        // whose first pop plays at once.
        let (out, _) = e.step(&sound(490.0, 0.95), MIN_RPM, 0.01, || 1);
        assert_eq!(shots(&out), [(Shot::ExhaustPop(5), 1.225), (Shot::Bang(1), 1.0), (Shot::Pop(10), 1.0)]);
        // The engine slows on: the run goes on, its next pop 904 samples (2.05 steps) later.
        let pops: Vec<_> =
            (0..4).map(|k| shots(&e.step(&sound(480.0 - k as f32, 0.0), MIN_RPM, 0.01, || 1).0)).collect();
        assert_eq!(pops.iter().map(Vec::len).collect::<Vec<_>>(), [0, 0, 1, 0]);
        assert_eq!(pops[2][0].0, Shot::Pop(9));
        // A steady engine ends it: the target with its fraction is above the last target, kept as an int.
        assert!((0..100).all(|_| shots(&e.step(&sound(450.0, 0.0), MIN_RPM, 0.01, || 1).0).is_empty()));
    }

    #[test]
    fn a_fire_calls_6_and_burns() {
        let mut e = Exhaust::new([1.0; 4], [11, 11, 6]);
        let (out, calls) = e.step(&Sound { fires: 1, ..Sound::default() }, MIN_RPM, 0.01, || 0);
        assert_eq!(calls, [FIRE_CALL]);
        assert_eq!(shots(&out), [(Shot::Fire, 1.0)]);
    }
}

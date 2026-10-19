//! Milestone B0 (`lead/reports/pure-rust-milestone.md`): the four struts (`CSpringDamper`, wheel+0xec4) run alone in
//! Rust over the tape's steps, from their bytes at the tape's start. Each call gets only the tape's inputs; its
//! outputs and, after every step, every byte of the struts' objects must be the original's. The state is never
//! re-seeded from the tape, and the memory holds only the struts' objects and their owners' ids, so a port that reads
//! anything else fails. The tape: `harness/subtree_tape.py` (its header names the run).

use rbr_physics::mem::Mem;
use rbr_physics::spring_damper::layout;

const TAPE: &str = include_str!("vectors/struts-snap-300.txt");
/// Where the pure run's `Step` writes its three outputs (the original's slots are in the wheel's frame).
const OUT: u32 = 0x1000_0000;

struct Strut {
    controller: u32,
    def: u32,
    state: u32,
    fade: u32,
    owner_id: u32,
}

impl Strut {
    /// The objects compared after every step, as the tape's `end` lines give them.
    fn objects(&self) -> [(&'static str, u32); 3] {
        [("controller", self.controller), ("state", self.state), ("fade", self.fade)]
    }
}

fn word(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap()
}

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
}

/// Panics at the first byte where `mem` differs from `want`, naming the object and its offset.
fn compare(mem: &Mem, at: u32, want: &[u8], what: &str) {
    let got = mem.read(at, want.len());
    if let Some(i) = (0..want.len()).find(|&i| got[i] != want[i]) {
        let w = i & !3;
        panic!("{what} +{w:#x}: {:02x?}, the original's {:02x?}", &got[w..w + 4], &want[w..w + 4]);
    }
}

#[test]
fn struts_run_alone_bit_exact() {
    let mut mem = Mem::new();
    mem.map(OUT, &[0; 12]);
    let mut struts: Vec<Strut> = Vec::new();
    let (mut step, mut steps, mut calls) = (None, 0, 0);
    for line in TAPE.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let place = |step: Option<u32>| format!("step {}, wheel {}", step.unwrap_or(0), f[1]);
        match f[0] {
            "strut" => struts.push(Strut {
                controller: word(f[2]),
                def: word(f[3]),
                state: word(f[4]),
                fade: word(f[5]),
                owner_id: word(f[6]),
            }),
            "start" => {
                let s = &struts[word(f[1]) as usize];
                for (hex, at) in f[2..].iter().zip([s.controller, s.state, s.fade, s.def]) {
                    mem.map(at, &bytes(hex));
                }
                let owner = mem.u32(s.state + 0x18);
                mem.map(owner + 0xeb4, &s.owner_id.to_le_bytes());
            }
            "step" => step = Some(f[1].parse::<u32>().unwrap()),
            "call" => {
                let s = &struts[word(f[1]) as usize];
                let [travel, coordinate, velocity] = [f[2], f[3], f[4]].map(word);
                layout::step(&mut mem, &[s.controller, travel, coordinate, velocity, OUT, OUT + 4, OUT + 8], None);
                let want: Vec<u8> = f[6..9].iter().flat_map(|w| word(w).to_le_bytes()).collect();
                compare(&mem, OUT, &want, &format!("{}: Step outputs", place(step)));
                calls += 1;
            }
            "reset" => {
                let s = &struts[word(f[1]) as usize];
                let want: Vec<u8> = f[3..8].iter().flat_map(|w| word(w).to_le_bytes()).collect();
                compare(&mem, s.controller + 0x1c, &want, &format!("{}: emitted payload", place(step)));
                layout::reset_outputs(&mut mem, s.controller);
                calls += 1;
            }
            "update" => {
                let s = &struts[word(f[1]) as usize];
                layout::update_forces(&mut mem, s.controller);
                layout::reset_outputs(&mut mem, s.controller);
                calls += 1;
            }
            "end" => {
                let s = &struts[word(f[1]) as usize];
                for ((name, at), hex) in s.objects().into_iter().zip(&f[2..5]) {
                    compare(&mem, at, &bytes(hex), &format!("{}: {name}", place(step)));
                }
                steps += usize::from(f[1] == "3");
            }
            "def" => {
                let s = &struts[word(f[1]) as usize];
                compare(&mem, s.def, &bytes(f[2]), &format!("after the last step, wheel {}: def", f[1]));
            }
            "foreign" => panic!("the tape has a write from outside the struts' calls: {line}"),
            other => panic!("unknown tape line {other}"),
        }
    }
    assert_eq!(struts.len(), 4);
    assert!(steps > 0 && calls > 0);
    eprintln!("{steps} steps, {calls} calls: bit-exact");
}

//! `--twin mem` (Phase B, `lead/reports/phase-b-plan.md` 2.3): a second playback from the same image, stepped with the
//! same payloads, and compared with the first after every step: the car and wheel blocks (FRM1's bytes) and every
//! owned byte of the car's tabled objects (`rbr_physics::store::car_objects`). Both images' tables are checked with
//! `lower(lift(image)) == image` first.
//!
//! `mem` runs the second side as the first, the Mem path against itself; `mem-direct` runs it with B1's Mem-direct
//! dispatch ([`rbr_hybrid::pure::Pure::direct`]). B2's typed storage is a further kind of the second side.

use rbr_hybrid::pure::{frame_bytes, word_name, Playback, CAR_PTR};
use rbr_physics::mem::Mem;
use rbr_physics::store;
use rbr_recording::CONTROL_BYTES;

/// What the second side runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The Mem storage on the pure host, as the first side.
    Mem,
    /// The Mem storage on the pure host with the leaf calls of the entity walk, wheel, collision and rigid body run in
    /// place.
    MemDirect,
}

impl Kind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "mem" => Ok(Kind::Mem),
            "mem-direct" => Ok(Kind::MemDirect),
            _ => Err(format!("--twin {value}: mem or mem-direct")),
        }
    }
}

pub struct Twin {
    kind: Kind,
    pub playback: Playback,
    /// Steps compared, and the first one that differed with what differed.
    steps: usize,
    first: Option<(usize, String)>,
    /// The owned bytes of each side's tables, from the round trip at the start (or its failure).
    round_trip: [Result<usize, String>; 2],
}

impl Twin {
    /// The twin of `a` on `b`, a second playback of the same image.
    pub fn new(kind: Kind, a: &Playback, mut b: Playback) -> Self {
        b.pure.direct = kind == Kind::MemDirect;
        let round_trip = [&a.pure.mem, &b.pure.mem].map(|mem| store::round_trip(mem, &objects(mem)));
        for (side, r) in ["first", "second"].iter().zip(&round_trip) {
            eprintln!("twin: lower(lift) of the {side} side's image: {}", Self::round_trip_text(r));
        }
        let mut twin = Twin { kind, playback: b, steps: 0, first: None, round_trip };
        twin.compare(a, "the start");
        twin
    }

    /// Steps the second side as the first was stepped, then compares them.
    pub fn step(&mut self, a: &Playback, payload: Option<&[u8; CONTROL_BYTES]>, substeps: u32) {
        let b = &self.playback;
        b.step(payload, substeps);
        // The host's queues the first side's caller drains.
        b.pure.take_tyre_terms();
        b.pure.take_outside_assistance();
        b.pure.take_retire();
        self.steps += 1;
        let at = format!("step {}", self.steps);
        self.compare(a, &at);
    }

    /// Notes the first difference of the two images, at `when`.
    fn compare(&mut self, a: &Playback, when: &str) {
        if self.first.is_some() {
            return;
        }
        if let Some(what) = difference(&a.pure.mem, &self.playback.pure.mem) {
            self.first = Some((self.steps, format!("{when}: {what}")));
        }
    }

    fn round_trip_text(r: &Result<usize, String>) -> String {
        match r {
            Ok(bytes) => format!("{bytes} owned bytes equal"),
            Err(e) => e.clone(),
        }
    }

    /// The report's line.
    pub fn summary(&self) -> String {
        let round_trip = Self::round_trip_text;
        let compare = match &self.first {
            None => format!("{} steps, car, wheels and owned bytes equal after each", self.steps),
            Some((_, what)) => format!("{} steps, first difference at {what}", self.steps),
        };
        format!(
            "twin {:?}: {compare}; lower(lift) first side {}, second side {}; {} calls run in place",
            self.kind,
            round_trip(&self.round_trip[0]),
            round_trip(&self.round_trip[1]),
            self.playback.pure.direct_runs()
        )
    }

    /// Whether every compare and both round trips held.
    pub fn equal(&self) -> bool {
        self.first.is_none() && self.round_trip.iter().all(Result::is_ok)
    }
}

fn objects(mem: &Mem) -> Vec<store::Tabled> {
    store::car_objects(mem, mem.u32(CAR_PTR))
}

/// The first difference between `a` and `b`: a word of the car and wheel blocks, else an owned byte of a tabled object.
fn difference(a: &Mem, b: &Mem) -> Option<String> {
    let (x, y) = (frame_bytes(a), frame_bytes(b));
    if let Some(i) = x.chunks(4).zip(y.chunks(4)).position(|(p, q)| p != q) {
        let word = |bytes: &[u8]| u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
        return Some(format!("{} {:#010x} vs {:#010x}", word_name(i), word(&x), word(&y)));
    }
    objects(a).iter().find_map(|o| {
        o.first_difference(a, b).map(|(field, offset)| format!("{} at {:#010x}: {field} (+{offset:#x})", o.name, o.at))
    })
}

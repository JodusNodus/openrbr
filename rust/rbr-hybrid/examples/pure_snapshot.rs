//! A snapshot of the pure host's state at a later step of a recording, for `harness/rust_step.py`: plays SNAPSHOT
//! with the recording's controls up to step STEP, emits STEP's control and writes the memory as a snapshot of STEP
//! (the snapshot's registers, fxsave and regions, read through the pure memory). A chain that drifts from the
//! recording late can then be compared with the original from just before. The pure allocator's window is one more
//! region, so blocks the pure run allocated are there too.
//!
//!     cargo run --release -p rbr-hybrid --example pure_snapshot -- SNAPSHOT RECORDING STEP OUT

use std::io::Write;

use rbr_hybrid::pure::{Playback, ALLOCATOR_HEAP_BASE, ALLOCATOR_HEAP_END};
use rbr_physics::snapshot::Snapshot;
use rbr_recording::Frames;

const MAGIC: u32 = 0x5352_4252;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, snap_path, recording, step, out] = &args[..] else {
        panic!("usage: pure_snapshot SNAPSHOT RECORDING STEP OUT");
    };
    let snap = Snapshot::open(snap_path).expect("snapshot");
    let step: u32 = step.parse().expect("STEP");
    let bytes = std::fs::read(recording).expect("recording");
    let frames: Vec<_> = Frames::new(&bytes).map(|f| f.expect("recording frame")).collect();
    let h = snap.metadata();
    let playback = Playback::new(&snap, false);
    for pre in h.step..step {
        let frame = frames.iter().find(|f| f.pre == pre).expect("a recorded frame for every step");
        playback.step((pre != h.step).then_some(&frame.control), frame.post - frame.pre);
    }
    if step != h.step {
        playback.emit(&frames.iter().find(|f| f.pre == step).expect("the step's frame").control);
    }

    let mem = &playback.pure.mem;
    let mut file = std::io::BufWriter::new(std::fs::File::create(out).expect("OUT"));
    let mut table = Vec::new();
    let mut at = 0u32;
    let heap = (ALLOCATOR_HEAP_BASE, ALLOCATOR_HEAP_END - ALLOCATOR_HEAP_BASE, 0x04, 0x2_0000, ALLOCATOR_HEAP_BASE);
    let regions: Vec<_> =
        snap.regions().iter().map(|r| (r.base, r.size, r.protect, r.kind, r.allocation_base)).chain([heap]).collect();
    for &(base, size, protect, kind, allocation_base) in &regions {
        file.write_all(&mem.read(base, size as usize)).unwrap();
        for w in [base, size, at, protect, kind, allocation_base] {
            table.extend(w.to_le_bytes());
        }
        at += size;
    }
    let reg = h.registers.words();
    let stack = mem.read(h.stack_lo, h.stack_len as usize);
    let header = [MAGIC, 1, step, h.substeps, h.thread, h.teb];
    let tail = [h.stack_lo, h.stack_len, regions.len() as u32];
    for w in header.iter().chain(&reg).chain(&tail) {
        file.write_all(&w.to_le_bytes()).unwrap();
    }
    file.write_all(snap.fxsave()).unwrap();
    file.write_all(&stack).unwrap();
    file.write_all(&table).unwrap();
    file.write_all(&at.to_le_bytes()).unwrap();
    file.write_all(&MAGIC.to_le_bytes()).unwrap();
}

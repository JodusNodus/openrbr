//! Runs one bound function purely in Rust (`rbr_hybrid::pure`) from a load-time entry dump and compares the memory
//! with the dump at its return (`lead/reports/rust-load-path.md`, "Composed load").
//!
//! Usage: pure_call ENTRY AFTER [--keep-going] [--objects PATCH]...
//!
//! ENTRY is a version-2 entry dump (`RBRRecompSnapAt-<addr>-<k>.bin`, possibly with Rust-built bytes written over it,
//! `harness/compose_load.py`), AFTER the dump at the same call's return. Every byte of the writable regions both hold
//! is compared. With `--objects`, the bytes of the patch files' objects are also counted by class: the bytes the
//! Rust build wrote (`w`, `W`) and those code not built there writes (`l`) are compared, the others listed.
//! The stack below the entry's return slot is the call's frame, not compared.

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::process::ExitCode;

use rbr_hybrid::pure::{Pure, Stop, Stopped};
use rbr_hybrid::Regs;
use rbr_physics::snapshot::Snapshot;

/// One object of a patch file (`harness/compose_load.py` documents the format): its kind, address and the class of
/// each byte.
struct Object {
    kind: String,
    at: u32,
    class: Vec<u8>,
}

fn objects(path: &str) -> Vec<Object> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut out = Vec::new();
    let mut p = 0;
    let word = |p: usize| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    while p < bytes.len() {
        let (at, len, kind_len) = (word(p), word(p + 4) as usize, bytes[p + 8] as usize);
        let kind = String::from_utf8_lossy(&bytes[p + 9..p + 9 + kind_len]).into_owned();
        let data = p + 9 + kind_len;
        out.push(Object { kind, at, class: bytes[data + len..data + 2 * len].to_vec() });
        p = data + 3 * len;
    }
    out
}

fn main() -> ExitCode {
    // Bindings call each other directly: a deep call wants more stack than the main thread's.
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .expect("spawn")
        .join()
        .expect("the run panicked outside the call")
}

fn run() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let keep_going = args.iter().any(|a| a == "--keep-going");
    args.retain(|a| a != "--keep-going");
    let mut patches = Vec::new();
    while let Some(i) = args.iter().position(|a| a == "--objects") {
        patches.push(args.remove(i + 1));
        args.remove(i);
    }
    let [entry, after] = &args[..] else {
        eprintln!("usage: pure_call ENTRY AFTER [--keep-going] [--objects PATCH]...");
        return ExitCode::FAILURE;
    };
    let (entry, after) =
        (Snapshot::open(entry).expect("the entry dump"), Snapshot::open(after).expect("the return dump"));
    let h = entry.metadata();
    assert!(h.version == 2 && !h.returns, "a load-time entry dump");
    let r = h.registers;
    let regs = Regs {
        ecx: r.ecx,
        edx: r.edx,
        esp: r.esp,
        eax: r.eax,
        ebx: r.ebx,
        esi: r.esi,
        edi: r.edi,
        ebp: r.ebp,
        stack: Default::default(),
    };
    let mem = entry.mem();
    let pure = Pure::new(mem.clone(), keep_going);
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(|| pure.enter(h.step, regs)));
    let _ = panic::take_hook();
    match result {
        Ok(ret) => println!("{:#x} returned: eax {:#x}, pops {}", h.step, ret.eax, ret.pop),
        Err(e) => {
            match e.downcast_ref::<Stopped>() {
                Some(Stopped(s)) => println!("stopped: {s:?}"),
                None => println!(
                    "a port failed in {:x?}: {}",
                    pure.chain(),
                    e.downcast_ref::<String>().cloned().unwrap_or_default()
                ),
            }
            return ExitCode::FAILURE;
        }
    }
    for (s, n) in pure.stops.borrow().iter() {
        match s {
            Stop::Unbound { addr, caller } => {
                println!("stop x{n}: unbound {addr:#x} (called from {caller:#x})")
            }
            Stop::Declined { addr, caller, why } => {
                println!("stop x{n}: {addr:#x} declined (called from {caller:#x}): {why}")
            }
        }
    }
    let ran: Vec<String> = pure.ran().iter().map(|(a, n)| format!("{a:#x} x{n}")).collect();
    println!("bindings run: {}", ran.join(", "));

    // Every writable byte both dumps hold, outside the call's frame (below the return slot).
    let mut differing: BTreeMap<u32, u32> = BTreeMap::new();
    let frame = (h.stack_lo, r.esp);
    for region in after.regions().iter().filter(|g| g.protect & 0xcc != 0) {
        let theirs = after.mem().read(region.base, region.size as usize);
        let ours = mem.read(region.base, region.size as usize);
        for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
            let at = region.base + i as u32;
            if a != b && !(frame.0..frame.1).contains(&at) {
                *differing.entry(at & !0xfff).or_default() += 1;
            }
        }
    }
    let total: u32 = differing.values().sum();
    println!(
        "memory against the return dump: {total} bytes differ outside the call's frame, in {} pages",
        differing.len()
    );
    for (page, n) in differing.iter().take(40) {
        println!("  page {page:#x}: {n}");
    }

    let after_mem = after.mem();
    let mut counts: BTreeMap<(String, u8), [u32; 2]> = BTreeMap::new();
    let mut bad = Vec::new();
    for path in &patches {
        for o in objects(path) {
            let (ours, theirs) = (mem.read(o.at, o.class.len()), after_mem.read(o.at, o.class.len()));
            for (i, &c) in o.class.iter().enumerate() {
                let n = counts.entry((o.kind.clone(), c)).or_default();
                n[0] += 1;
                if ours[i] != theirs[i] {
                    n[1] += 1;
                    if matches!(c, b'w' | b'W' | b'l') && bad.len() < 40 {
                        bad.push(format!(
                            "{} {:#x}+{i:#x}: ours {:02x}, the return dump's {:02x}",
                            o.kind, o.at, ours[i], theirs[i]
                        ));
                    }
                }
            }
        }
    }
    for ((kind, class), [n, d]) in &counts {
        println!("{kind} {}: {n} bytes, {d} differ", *class as char);
    }
    for b in &bad {
        println!("  {b}");
    }
    if total == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

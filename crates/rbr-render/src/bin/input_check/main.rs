//! `input_check SESSION_DIR [ATTEMPT...]`: the device poll replayed over a recorded RSF session's attempts through
//! our bindings ([`session`]), one line per attempt: `attempt N: equal E of C (by time T), bursts B, moved M, gear
//! from recording G, menu S, words [..]`; with DLV1 records `attempt N: deliveries equal D of N` (each delivered raw
//! control against the poll group recorded last before it); then the first differing call. Exit 1 when an attempt
//! is not all equal: its car control calls by time (by the recorded deliveries where the session has them, else by
//! the delivery estimate).

mod session;

use session::{read, replay};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first() else {
        eprintln!("usage: input_check SESSION_DIR [ATTEMPT...]");
        std::process::exit(2);
    };
    let mut attempts: Vec<u32> = std::fs::read_dir(dir)
        .map(|d| d.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_default();
    attempts.sort_unstable();
    let only: Vec<u32> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
    let mut process = None;
    let mut all = true;
    for n in attempts {
        let path = Path::new(dir).join(n.to_string()).join("session.bin");
        let Ok(bytes) = std::fs::read(&path) else { continue };
        // Every attempt runs, so the devices' state carries; only the asked ones print.
        let result = read(&bytes).and_then(|recs| replay(&recs, &mut process));
        if !only.is_empty() && !only.contains(&n) {
            continue;
        }
        match result {
            Ok(r) => {
                println!(
                    "attempt {n}: equal {} of {} (by time {}), bursts {}, moved {}, gear from recording {}, menu {}, \
                     words {:?}, record {:?}, devices {:?}",
                    r.equal,
                    r.compared,
                    r.equal_by_time,
                    r.bursts,
                    r.moved,
                    r.gear_from_recording,
                    r.menu,
                    r.words,
                    r.record,
                    r.devices
                );
                if r.delivered {
                    println!("attempt {n}: deliveries equal {} of {}", r.deliveries.0, r.deliveries.1);
                }
                if let Some((step, ours, recorded)) = r.first {
                    println!(
                        "attempt {n}: first difference at step {step}: ours {ours:08x?}, recorded {recorded:08x?}"
                    );
                }
                all &= r.equal_by_time == r.compared;
            }
            Err(e) => {
                println!("attempt {n}: error: {e}");
                all = false;
            }
        }
    }
    std::process::exit(if all { 0 } else { 1 });
}

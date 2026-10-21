//! The device poll (`rbr_gameplay::poll`) over the recorded RSF sessions: every car control call of every attempt
//! under `$RBR_RECORDINGS/rsf/*/` (sessions with CTL1 records) equal in all eight words. Skips without
//! `$RBR_RECORDINGS/rsf`. `notes/verification.md`, "Input chain check".

use rbr_gameplay::poll::session::{action_map, read, replay};
use std::path::PathBuf;

fn sessions() -> Vec<PathBuf> {
    let Some(root) = std::env::var_os("RBR_RECORDINGS").map(|r| PathBuf::from(r).join("rsf")) else { return vec![] };
    let mut dirs: Vec<PathBuf> =
        std::fs::read_dir(root).map(|d| d.filter_map(|e| Some(e.ok()?.path())).collect()).unwrap_or_default();
    dirs.sort();
    dirs
}

#[test]
fn recorded_payloads_come_from_the_polls() {
    let mut checked = 0;
    for dir in sessions() {
        let mut attempts: Vec<u32> = std::fs::read_dir(&dir)
            .map(|d| d.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
            .unwrap_or_default();
        attempts.sort_unstable();
        let mut process = None;
        for n in attempts {
            let Ok(bytes) = std::fs::read(dir.join(n.to_string()).join("session.bin")) else { continue };
            let recs = read(&bytes).unwrap();
            // Sessions from before CTL1 or without the profile's .acm hold nothing to compare.
            if !recs.iter().any(|r| &r.tag == b"CTL1") || action_map(&recs).is_none() {
                continue;
            }
            let r = replay(&recs, &mut process).unwrap();
            eprintln!(
                "{}/{n}: {} of {} equal ({} by time), {} of {} bursts moved",
                dir.display(),
                r.equal,
                r.compared,
                r.equal_by_time,
                r.moved,
                r.bursts
            );
            if r.compared > 0 && r.first.is_some() {
                panic!("{}/{n}: {} of {} equal, first difference {:x?}", dir.display(), r.equal, r.compared, r.first);
            }
            checked += r.compared;
        }
    }
    if checked == 0 {
        eprintln!("skipped: no RSF session with car control records under $RBR_RECORDINGS/rsf");
    }
}

//! The headless run's report: what each run printed to stdout (the gates grep its lines).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use rbr_formats::times::StageTimes;
use rbr_gameplay::result::{Outcome, StageResult};
use rbr_hybrid::pure;

use super::Run;
use crate::args::Args;
use crate::stage::StageData;

/// The stage result as the run prints it: the outcome, the race time and the penalties, then each split with its
/// reference time from `times.ini` and the difference (negative: ahead), and the finish's reference.
fn result_text(result: &StageResult, reference: Option<&StageTimes>) -> String {
    let outcome = match result.outcome {
        Outcome::Running => "running",
        Outcome::Finished => "finished",
        Outcome::Retired => "retired",
    };
    let time = result.time.map_or("no finish time".into(), |t| format!("time {t:.2} s"));
    let splits: Vec<String> = result
        .splits
        .iter()
        .enumerate()
        .map(|(i, split)| {
            let reference = reference.and_then(|r| r.splits.get(i).copied());
            match (split, reference) {
                (Some(t), Some(r)) => format!("{t:.2} s ({:+.2} s against {r:.2} s)", t - r),
                (Some(t), None) => format!("{t:.2} s (no reference)"),
                (None, _) => "not passed".to_owned(),
            }
        })
        .collect();
    let splits = if splits.is_empty() { "no splits".to_owned() } else { splits.join(", ") };
    let finish = reference.map_or(String::new(), |r| format!(", reference finish {:.2} s", r.finish));
    format!("{outcome}, {time}, penalties {:.0} s, splits: {splits}{finish}", result.penalties)
}

/// Contiguous address ranges for pure allocator bytes read before a caller wrote them.
pub fn address_ranges(addresses: &[u32]) -> String {
    if addresses.is_empty() {
        return "none".into();
    }
    let mut ranges = Vec::new();
    let mut start = addresses[0];
    let mut end = start + 1;
    for &address in addresses.iter().skip(1) {
        if address == end {
            end += 1;
        } else {
            ranges.push(format!("{start:#010x}..{end:#010x}"));
            start = address;
            end = address + 1;
        }
    }
    ranges.push(format!("{start:#010x}..{end:#010x}"));
    ranges.join(", ")
}

/// The stage record and the personal best after a finished stage: offers the result to the save files in `dir`
/// (`saves.rs`) and says what changed.
fn records_text(dir: &Path, profile: Option<&str>, stage: &StageData, result: &StageResult) -> String {
    let (Outcome::Finished, Some(time)) = (result.outcome, result.time) else {
        return "stage record and personal best: the stage was not finished".into();
    };
    let [Some(first), Some(second)] =
        [result.splits.first().copied().flatten(), result.splits.get(1).copied().flatten()]
    else {
        return "stage record and personal best: the splits were not both passed".into();
    };
    let done = stage.saves(dir, profile).and_then(|mut saves| {
        let update = saves.finish([first, second, time])?;
        Ok((update, saves.standing()))
    });
    match done {
        Ok((update, standing)) => format!(
            "stage record: {} (now {:.2} s by {}), personal best: {} (now {})",
            if update.record { "new" } else { "kept" },
            standing.record.time,
            standing.record.holder,
            if update.personal { "new" } else { "kept" },
            standing.personal.map_or("none".to_owned(), |p| format!("{:.2} s", p.time)),
        ),
        Err(e) => format!("stage record and personal best: {e}"),
    }
}

/// Prints the report of the run from `path`; returns whether it equals the recording and ran to its end.
pub fn report(path: &str, r: &Run, args: &Args, mask: &[bool], stage: &StageData) -> bool {
    let mut ok = true;
    if !r.built.is_empty() {
        let built: Vec<String> = r.built.iter().map(|(what, n)| format!("{what} {n}")).collect();
        println!("{path}: built {}", built.join(", "));
    }
    println!(
        "{path}: from step {}, {} of {} steps run (Game::frame + pure physics), {} equal to the recording, {} snapshot pointer words, {} allocator placement words, {} run-noise candidates ({} released, {} newly compared while pure held its start value, {} held at stop; {} outside the static mask at start), {} allocator bytes read before caller write, dt {} ({})",
        r.start,
        r.steps,
        args.steps,
        r.equal,
        r.pointers,
        r.allocator_pointers,
        r.noise,
        r.noise_released,
        r.noise_released_by_recording.len(),
        r.noise_kept_at_stop.len(),
        r.noise_unmasked,
        r.allocator_unwritten_reads.len(),
        r.dt,
        if r.dt_image { "the image's" } else { "the retail constant" }
    );
    if !r.noise_released_by_recording.is_empty() {
        let names: Vec<String> = r.noise_released_by_recording.iter().map(|&i| pure::word_name(i)).collect();
        println!("  run-noise words newly compared after a recorded value changed while pure held: {}", names.len());
        for words in names.chunks(16) {
            println!("    {}", words.join(", "));
        }
    }
    let per_s = |d: Duration| r.steps as f64 / d.as_secs_f64().max(1e-9);
    let per_step = |n: u64| n / r.steps.max(1) as u64;
    let counts = &r.stepping_counts;
    let cpu = if counts.instructions > 0 {
        format!("{} instructions, {} cycles, ", per_step(counts.instructions), per_step(counts.cycles))
    } else {
        String::new()
    };
    eprintln!(
        "{path}: timing: {} steps, stepping {:.2} s ({:.0} steps/s), with the compare {:.2} s ({:.0} steps/s); \
         stepping {cpu}{} allocations ({} bytes) per step",
        r.steps,
        r.stepping.as_secs_f64(),
        per_s(r.stepping),
        r.looping.as_secs_f64(),
        per_s(r.looping),
        per_step(counts.allocations),
        per_step(counts.allocated)
    );
    println!(
        "  phase steps run/equal (physics substeps): settle {}/{}, {} ; countdown {}/{}, {} ; driving {}/{}, {}",
        r.phases.settle.steps,
        r.phases.settle.equal,
        r.phases.settle.physics_substeps,
        r.phases.countdown.steps,
        r.phases.countdown.equal,
        r.phases.countdown.physics_substeps,
        r.phases.driving.steps,
        r.phases.driving.equal,
        r.phases.driving.physics_substeps,
    );
    if r.phases.countdown.no_step_game_frames != 0 {
        println!(
            "  countdown also ran {} synthetic Game frames without a recording compare or physics step",
            r.phases.countdown.no_step_game_frames
        );
    }
    if let Some(after_countdown) = &r.noise_kept_after_countdown {
        let unmasked = |words: &[usize]| words.iter().filter(|&&i| !mask[i]).count();
        let through = r.noise_kept_through_driving.as_deref().unwrap_or_default();
        println!(
            "  run-noise words still held after countdown: {} ({} outside the static mask); at stop: {} ({} outside); held through every driving step run: {} ({} outside)",
            after_countdown.len(),
            unmasked(after_countdown),
            r.noise_kept_at_stop.len(),
            unmasked(&r.noise_kept_at_stop),
            through.len(),
            unmasked(through),
        );
        let after_pointers = r.boot_pointers_kept_after_countdown.as_deref().unwrap_or_default();
        let through_pointers = r.boot_pointers_kept_through_driving.as_deref().unwrap_or_default();
        println!(
            "  relocated boot pointers: {} candidates ({} outside the static mask); after countdown: {} still at snapshot addresses; at stop: {}; held through every driving step run: {}",
            r.boot_pointer_candidates,
            r.boot_pointer_candidates_unmasked,
            after_pointers.len(),
            r.boot_pointers_kept_at_stop.len(),
            through_pointers.len(),
        );
        let mut held_by_object = BTreeMap::<String, usize>::new();
        for &i in through.iter().filter(|&&i| !mask[i]) {
            let name = pure::word_name(i);
            let object = name.split('+').next().unwrap_or("?").to_string();
            *held_by_object.entry(object).or_default() += 1;
        }
        if !held_by_object.is_empty() {
            let summary =
                held_by_object.iter().map(|(object, n)| format!("{object} {n}")).collect::<Vec<_>>().join(", ");
            println!("  run-noise held unchanged through driving outside the static mask by object: {summary}");
        }
        let held_words: Vec<String> = through.iter().filter(|&&i| !mask[i]).map(|&i| pure::word_name(i)).collect();
        if !held_words.is_empty() {
            println!(
                "  run-noise word names held unchanged through driving outside the static mask: {}",
                held_words.join(", ")
            );
        }
    }
    if !r.allocator_unwritten_reads.is_empty() {
        println!("  allocator bytes read before caller write: {}", address_ranges(&r.allocator_unwritten_reads));
    }
    if let Some((step, bad)) = &r.first_diff {
        ok = false;
        let names: Vec<String> = bad.iter().take(8).map(|&i| pure::word_name(i)).collect();
        println!("  differs from the recording after step {step}: {} words ({})", bad.len(), names.join(", "));
        let values: Vec<String> = r
            .first_diff_values
            .iter()
            .take(8)
            .map(|(i, want, have)| format!("{} recording={want:#010x} pure={have:#010x}", pure::word_name(*i)))
            .collect();
        if !values.is_empty() {
            println!("  first-difference values: {}", values.join(", "));
        }
        let mut words: Vec<_> = r.differing.iter().collect();
        words.sort_by_key(|(&i, &(step, _))| (step, i));
        let words: Vec<String> = words
            .iter()
            .take(16)
            .map(|(&i, &(step, n))| format!("{} (from step {step}, {n} steps)", pure::word_name(i)))
            .collect();
        println!("  words that differ at some step: {} ({})", r.differing.len(), words.join(", "));
    }
    let (padding, stale): (Vec<_>, Vec<_>) = r.stale.iter().partition(|(&i, _)| pure::is_ngp_padding(i));
    let named = |words: Vec<(&usize, &usize)>| -> Vec<String> {
        words.into_iter().map(|(&i, &n)| format!("{} ({n} steps)", pure::word_name(i))).collect()
    };
    if !stale.is_empty() {
        println!(
            "  not compared: stale-stack and wheel+0x6ec heap words (the original's own residue) and frame-loop words (car+0x660, car+0x670, car+0x814): {}",
            named(stale).join(", ")
        );
    }
    if !padding.is_empty() {
        println!(
            "  masked: lane-3 padding (NGP: the w of the wheel bodies' +0x140 and +0x330, pure::NGP_PADDING_WHEEL): {}",
            named(padding).join(", ")
        );
    }
    if let Some((step, word)) = r.control_diff {
        ok = false;
        println!("  Game's control differs from the recorded payload at step {step}, word {word}");
    }
    match &r.keeper_diff {
        Some((step, what)) => {
            ok = false;
            let at = step.map_or("at the boot".into(), |s| format!("after step {s}"));
            println!("  Game's time card differs from the bound CTimeKeeper's {at}: {what}");
        }
        None => println!(
            "  Game's time card and clock equal the bound CTimeKeeper's at the boot and after each of the {} steps",
            r.steps
        ),
    }
    if let Some(clock) = r.race_clock {
        let pairs = |v: Vec<String>| {
            if v.is_empty() {
                "none".into()
            } else {
                v.join(", ")
            }
        };
        println!(
            "  race clock after the last step: {} s; outside assistance (step: code): {}; penalties (step: s): {}; jump start (step: s): {}",
            clock.time,
            pairs(r.assists.iter().map(|(s, c)| format!("{s}: {c}")).collect()),
            pairs(r.penalties.iter().map(|(s, p)| format!("{s}: {p}")).collect()),
            pairs(r.jump_starts.iter().map(|(s, p)| format!("{s}: {p}")).collect()),
        );
    }
    if let Some(result) = &r.result {
        println!("  stage result: {}", result_text(result, stage.reference.as_ref()));
    }
    match r.state_diff {
        Some((ours, image)) => {
            ok = false;
            println!("  Game's application state {ours} differs from the image's {image} at the boot");
        }
        None => println!("  Game's application state equals the image's at the boot"),
    }
    if let Some((summary, equal)) = &r.twin {
        ok &= equal;
        println!("  {summary}");
    }
    if let Some(stop) = &r.stop {
        ok = false;
        println!("  stopped: {stop}");
    }
    if let Some(ended) = &r.ended {
        println!("  the flow: {ended}");
        if let (Some(dir), Some(result)) = (&args.saves, &r.result) {
            println!("  {}", records_text(dir, args.profile.as_deref(), stage, result));
        }
    }
    ok
}

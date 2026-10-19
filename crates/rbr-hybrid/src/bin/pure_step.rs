//! Runs the physics step purely in Rust (`rbr_hybrid::pure`) from snapshots, with a recording's controls, and compares
//! the car and the wheels with the recording after every step.
//!
//! Usage: pure_step RECORDING SNAPSHOT... [--steps N] [--keep-going]
//!
//! Without `--keep-going` a snapshot's run stops at the first call of an address with no binding (or a binding that
//! declines) and prints it with its caller. With it, such a call is logged and answers zero, so one run lists every
//! address the pure step lacks (the results after the first stop are wrong, so nothing is compared after it). Each
//! stop is printed, in the order first met, with the step it was first met in and the bindings that step had run by
//! then, out of all the bindings the step ran (with `--keep-going`, its stops answered zero): the share of the step
//! that ran pure before it.
//!
//! The steps after the snapshot's own are the recording's calls from its step on (the first frame of each pre step),
//! each with its control and substeps, as `rust_step.py`'s schedule.
//!
//! The compare is `rust_step.py`'s of run A against the recording: words that differ between the snapshot and the
//! recording before the first step are noise, and so are `diverge.py`'s static mask and wheel+0x9ac; a word that is a
//! 4-aligned mapped address here and a 4-aligned user address in the recording is a pointer, counted, not compared.
//! A snapshot at step 0 has no recorded frame before it: its noise is `Compare::at_start`'s.

use std::collections::{BTreeMap, HashMap};
use std::panic::{self, AssertUnwindSafe};
use std::process::ExitCode;

use rbr_hybrid::pure::{self, Compare, Playback, Stop, Stopped};
use rbr_physics::snapshot::Snapshot;
use rbr_recording::{Frame, Frames};

struct Run {
    steps: usize,
    stop: Option<String>,
    first_diff: Option<(u32, Vec<usize>)>,
    pointers: usize,
    stops: BTreeMap<Stop, u64>,
    ran: usize,
    /// When each stop was first met: the stops met before it, and the bindings run (all addresses) by then.
    first_met: BTreeMap<Stop, (usize, u64)>,
    /// Each step run: its step, the bindings run before it, and after it (`None` when it stopped).
    step_runs: Vec<(u32, u64, Option<u64>)>,
}

impl Run {
    /// Where the binding run count `runs` falls: the step, and the bindings it had run by then out of its own.
    fn place(&self, runs: u64) -> String {
        let Some(&(step, before, after)) = self.step_runs.iter().rfind(|s| s.1 <= runs) else {
            return String::new();
        };
        let done = runs - before;
        match after {
            Some(after) => format!(
                "step {step}, after {done} of its {} binding runs ({:.1}%)",
                after - before,
                100.0 * done as f64 / (after - before).max(1) as f64
            ),
            None => format!("step {step}, after {done} binding runs"),
        }
    }
}

fn run(
    path: &str,
    frames: &[Frame],
    by_pre: &HashMap<u32, usize>,
    mask: &[bool],
    steps: usize,
    keep_going: bool,
) -> Result<Run, String> {
    let snap = Snapshot::open(path).map_err(|e| format!("{path}: {e}"))?;
    let h = snap.metadata();
    let mut compare = match h.step.checked_sub(1) {
        Some(step) => {
            let before = by_pre.get(&step).ok_or_else(|| format!("{path}: no recorded frame for step {step}"))?;
            Compare::new(&snap, mask.to_vec(), &frames[*before])
        }
        None => Compare::at_start(&snap, mask.to_vec(), frames),
    };
    let playback = Playback::new(&snap, keep_going);
    let pure = &playback.pure;
    let mut out = Run {
        steps: 0,
        stop: None,
        first_diff: None,
        pointers: 0,
        stops: BTreeMap::new(),
        ran: 0,
        first_met: BTreeMap::new(),
        step_runs: vec![],
    };
    let mut schedule: Vec<(u32, usize)> =
        by_pre.iter().filter(|(&pre, _)| pre >= h.step).map(|(&pre, &at)| (pre, at)).collect();
    schedule.sort_unstable();
    for k in 0..steps {
        let Some(&(step, at)) = schedule.get(k) else {
            out.stop = Some(format!("no recorded frame after step {}", h.step + k as u32));
            break;
        };
        if k == 0 && step != h.step {
            out.stop = Some(format!("no recorded frame for step {}", h.step));
            break;
        }
        let frame = &frames[at];
        let control = (k > 0).then_some(&frame.control);
        let substeps = if k == 0 { h.substeps } else { frame.substeps };
        let before = pure.runs();
        let result = panic::catch_unwind(AssertUnwindSafe(|| playback.step(control, substeps)));
        let after = result.is_ok().then(|| pure.runs());
        out.step_runs.push((step, before, after));
        if let Err(e) = result {
            let chain: Vec<String> = pure.chain().iter().map(|a| format!("{a:#x}")).collect();
            out.stop = Some(match e.downcast_ref::<Stopped>() {
                Some(Stopped(s)) => format!("{s} in {}", chain.join(" > ")),
                None => {
                    format!(
                        "a port failed in {}: {}",
                        chain.join(" > "),
                        e.downcast_ref::<String>()
                            .cloned()
                            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_default()
                    )
                }
            });
            break;
        }
        if !pure.stops.borrow().is_empty() {
            continue; // the step answered a stop with zero: nothing to compare
        }
        out.steps += 1;
        let diff = compare.diff(&pure.mem, frame);
        out.pointers += diff.pointers;
        if !diff.words.is_empty() && out.first_diff.is_none() {
            out.first_diff = Some((step, diff.words));
        }
    }
    out.stops = pure.stops.take();
    out.first_met = pure.first_met.take();
    out.ran = pure.runs() as usize;
    Ok(out)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let keep_going = args.iter().any(|a| a == "--keep-going");
    args.retain(|a| a != "--keep-going");
    let mut steps = 30;
    if let Some(i) = args.iter().position(|a| a == "--steps") {
        steps = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(steps);
        args.drain(i..(i + 2).min(args.len()));
    }
    let [recording, snapshots @ ..] = &args[..] else {
        eprintln!("usage: pure_step RECORDING SNAPSHOT... [--steps N] [--keep-going]");
        return ExitCode::FAILURE;
    };
    let bytes = match std::fs::read(recording) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{recording}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let frames: Vec<Frame> = Frames::new(&bytes).map_while(Result::ok).collect();
    let mut by_pre: HashMap<u32, usize> = HashMap::new();
    for (i, f) in frames.iter().enumerate() {
        by_pre.entry(f.pre).or_insert(i);
    }
    let mask = pure::static_mask(&frames);
    panic::set_hook(Box::new(|_| {})); // stops and port failures are reported per snapshot
    let mut ok = true;
    for path in snapshots {
        // Bindings call each other directly: a deep step wants more stack than the main thread's.
        let r = std::thread::scope(|s| {
            std::thread::Builder::new()
                .stack_size(1 << 28)
                .spawn_scoped(s, || run(path, &frames, &by_pre, &mask, steps, keep_going))
                .expect("spawn")
                .join()
                .expect("the run panicked outside a step")
        });
        let r = match r {
            Ok(r) => r,
            Err(e) => {
                println!("{e}");
                ok = false;
                continue;
            }
        };
        println!(
            "{path}: {} of {steps} steps pure and compared, {} binding runs, {} pointer words",
            r.steps, r.ran, r.pointers
        );
        match &r.first_diff {
            None if r.steps > 0 => {
                println!("  car and wheels equal to the recording at every compared step")
            }
            None => {}
            Some((step, bad)) => {
                ok = false;
                let names: Vec<String> = bad.iter().take(8).map(|&i| pure::word_name(i)).collect();
                println!("  differs from the recording after step {step}: {} words ({})", bad.len(), names.join(", "));
            }
        }
        if let Some(stop) = &r.stop {
            ok = false;
            let at = r.first_met.values().max().map_or(String::new(), |&(_, runs)| format!(" ({})", r.place(runs)));
            println!("  stopped: {stop}{at}");
        }
        let mut stops: Vec<(&Stop, u64, (usize, u64))> = r.stops.iter().map(|(s, &n)| (s, n, r.first_met[s])).collect();
        stops.sort_by_key(|&(_, _, first)| first);
        for (s, n, (_, runs)) in stops {
            ok = false;
            println!("  stop x{n}: {s}; first in {}", r.place(runs));
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

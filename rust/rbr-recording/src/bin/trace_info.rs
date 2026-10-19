//! Summarise an `RBRRecompTrace-<tick>.bin` call trace.
//!
//! ```text
//! trace_info <file>
//! ```

use rbr_recording::trace::{precision, Calls, Precision};
use std::collections::BTreeMap;
use std::process::ExitCode;

// Per function: calls, calls with an x87 input, precisions of inputs and outputs, region bytes.
#[derive(Default)]
struct FunctionStats<'a> {
    calls: usize,
    fpu_inputs: usize,
    precisions: BTreeMap<&'a str, usize>,
    region_bytes: usize,
}

fn label(precision: Precision) -> &'static str {
    match precision {
        Precision::Single => "24-bit",
        Precision::Reserved => "reserved",
        Precision::Double => "53-bit",
        Precision::Extended => "64-bit",
    }
}

fn run(path: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let calls = Calls::new(&bytes).map_err(|e| e.to_string())?;
    println!("calls logged per function: {}", calls.calls_per_function);
    let mut per_function: BTreeMap<u32, FunctionStats> = BTreeMap::new();
    let mut total = 0usize;
    let mut words: BTreeMap<u32, usize> = BTreeMap::new();
    for call in calls {
        let call = call.map_err(|e| e.to_string())?;
        total += 1;
        let entry = per_function.entry(call.addr).or_default();
        entry.calls += 1;
        entry.fpu_inputs += usize::from(call.fpu_in);
        *entry.precisions.entry(label(precision(call.cw_in))).or_default() += 1;
        *entry.precisions.entry(label(precision(call.cw_out))).or_default() += 1;
        entry.region_bytes += call.regions.iter().map(|r| r.pre.len()).sum::<usize>();
        *words.entry(call.cw_in).or_default() += 1;
    }
    println!("calls: {total}");
    println!("input control words:");
    for (cw, count) in &words {
        println!("  {cw:#06x} {count}");
    }
    println!("functions:");
    for (addr, stats) in &per_function {
        let precisions: Vec<String> = stats.precisions.iter().map(|(name, n)| format!("{name} {n}")).collect();
        println!(
            "  {addr:08x} calls {} x87-input {} region-bytes {} [{}]",
            stats.calls,
            stats.fpu_inputs,
            stats.region_bytes,
            precisions.join(", ")
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, path] = args.as_slice() else {
        eprintln!("usage: trace_info <file>");
        return ExitCode::FAILURE;
    };
    match run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

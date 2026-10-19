//! Summarise an `RBRRecompPeek-<tick>.bin` pointer-probe log.
//!
//! ```text
//! peek_info <file>
//! ```

use rbr_recording::peek::{PeekFile, Value};
use std::collections::BTreeMap;
use std::process::ExitCode;

fn run(path: &str) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let file = PeekFile::parse(&bytes).map_err(|e| e.to_string())?;
    let mut counts: Vec<BTreeMap<String, usize>> = (0..file.probes.len()).map(|_| BTreeMap::new()).collect();
    let mut float_stats: Vec<Option<(f32, f32, usize, usize)>> = (0..file.probes.len()).map(|_| None).collect();
    let mut records = 0usize;
    let mut first = None;
    let mut last = None;
    for record in file.records() {
        let record = record.map_err(|e| e.to_string())?;
        records += 1;
        first.get_or_insert(record.step);
        last = Some(record.step);
        for (index, values) in counts.iter_mut().enumerate() {
            let value =
                record.value(index).ok_or_else(|| format!("record at {} has no probe {index}", record.offset))?;
            match value {
                Value::F32(value) if !value.is_nan() || value.to_bits() != 0x7fc0_0000 => {
                    let stat = &mut float_stats[index];
                    *stat = Some(match *stat {
                        Some((min, max, zeros, samples)) => {
                            (min.min(value), max.max(value), zeros + usize::from(value == 0.0), samples + 1)
                        }
                        None => (value, value, usize::from(value == 0.0), 1),
                    });
                }
                Value::F32(_) => *values.entry("<invalid>".to_owned()).or_default() += 1,
                Value::U32(value) => *values.entry(value.to_string()).or_default() += 1,
                Value::U8(value) => *values.entry(value.to_string()).or_default() += 1,
                Value::I32(value) => *values.entry(value.to_string()).or_default() += 1,
            }
        }
    }
    println!("records: {records}");
    if let (Some(first), Some(last)) = (first, last) {
        println!("steps: {first}..={last}");
    }
    for (index, probe) in file.probes.iter().enumerate() {
        println!("{} ({})", probe.name, probe.ty.name());
        if probe.ty.name() == "f32" {
            if let Some((min, max, zeros, samples)) = float_stats[index] {
                println!("  samples: {samples}, zero: {zeros}, min: {min:?}, max: {max:?}");
            }
            if let Some(invalid) = counts[index].get("<invalid>") {
                println!("  <invalid>: {invalid}");
            }
        } else {
            for (value, count) in &counts[index] {
                println!("  {value}: {count}");
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: peek_info <file>");
        return ExitCode::FAILURE;
    };
    if args.next().is_some() {
        eprintln!("usage: peek_info <file>");
        return ExitCode::FAILURE;
    }
    match run(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("peek_info: {error}");
            ExitCode::FAILURE
        }
    }
}

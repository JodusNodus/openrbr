//! `drive_path`: print the car's path through a physics recording.
//!
//! ```text
//! drive_path <file> [--every N] [--csv <out.csv>]
//! ```
//!
//! Prints one row per `N` physics steps (default 100): the step counter (`post` of the frame that first reaches each
//! multiple of `N`), the body position, speed and heading. `--csv` also writes the rows to a file. Read-only.

use rbr_recording::{pose, Frames};
use std::fmt::Write as _;
use std::process::ExitCode;

struct Args {
    path: String,
    every: u32,
    csv: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut path = None;
    let mut every = 100;
    let mut csv = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--every" => {
                let value = it.next().ok_or("--every needs a step count")?;
                every =
                    value.parse().ok().filter(|n| *n > 0).ok_or(format!("--every {value}: not a positive number"))?;
            }
            "--csv" => csv = Some(it.next().ok_or("--csv needs a file name")?.clone()),
            _ if path.is_none() => path = Some(arg.clone()),
            _ => return Err(format!("unexpected argument {arg:?}")),
        }
    }
    Ok(Args { path: path.ok_or("missing recording file")?, every, csv })
}

/// The rows as text, one per sampled step.
fn rows(bytes: &[u8], every: u32) -> Result<String, String> {
    let mut out = String::from("step,x,y,z,speed,heading_deg\n");
    let mut next = every;
    for frame in Frames::new(bytes) {
        let frame = frame.map_err(|e| e.to_string())?;
        if frame.post < next {
            continue;
        }
        next = (frame.post / every + 1) * every;
        let car = pose::car_pose(frame.car).ok_or("car block too short")?;
        let [x, y, z] = car.position;
        let heading = car.heading().map_or(f32::NAN, f32::to_degrees);
        writeln!(out, "{},{x},{y},{z},{},{heading}", frame.post, car.speed()).expect("writing to a String");
    }
    Ok(out)
}

fn run(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let bytes = std::fs::read(&args.path).map_err(|e| format!("{}: {e}", args.path))?;
    let text = rows(&bytes, args.every)?;
    print!("{text}");
    if let Some(csv) = &args.csv {
        std::fs::write(csv, &text).map_err(|e| format!("{csv}: {e}"))?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("drive_path: {e}");
            eprintln!("usage: drive_path <file> [--every N] [--csv <out.csv>]");
            ExitCode::FAILURE
        }
    }
}

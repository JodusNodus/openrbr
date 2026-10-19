//! `rec_info`: summarise a physics recording written by the runtime harness.
//!
//! ```text
//! rec_info <file> [--frame N]
//! ```
//!
//! Without `--frame`: frame count, substeps distribution, first and last pre/post.
//! With `--frame N`: that frame's control as 8 floats and its first 16 car dwords.
//! Read-only: the file is only read.

use rbr_recording::{read_f32, read_u32, Frame, Frames};
use std::collections::BTreeMap;
use std::process::ExitCode;

const CAR_DWORDS: usize = 16;

struct Args {
    path: String,
    frame: Option<usize>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut path = None;
    let mut frame = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--frame" {
            let value = it.next().ok_or("--frame needs a frame number")?;
            frame = Some(value.parse().map_err(|e| format!("--frame {value}: {e}"))?);
        } else if path.is_none() {
            path = Some(arg.clone());
        } else {
            return Err(format!("unexpected argument {arg:?}"));
        }
    }
    Ok(Args { path: path.ok_or("missing recording file")?, frame })
}

fn summarize(bytes: &[u8]) -> Result<(), String> {
    let mut frames = 0usize;
    let mut substeps: BTreeMap<u32, usize> = BTreeMap::new();
    let mut first = None;
    let mut last = None;
    for frame in Frames::new(bytes) {
        let frame = frame.map_err(|e| e.to_string())?;
        frames += 1;
        *substeps.entry(frame.substeps).or_default() += 1;
        first.get_or_insert((frame.pre, frame.post));
        last = Some((frame.pre, frame.post));
    }
    println!("frames: {frames}");
    for (count, n) in &substeps {
        println!("substeps {count}: {n} frames");
    }
    if let (Some((pre, post)), Some((last_pre, last_post))) = (first, last) {
        println!("first: pre={pre} post={post}");
        println!("last: pre={last_pre} post={last_post}");
    }
    Ok(())
}

fn show_frame(bytes: &[u8], index: usize) -> Result<(), String> {
    let frame: Frame =
        Frames::new(bytes).nth(index).ok_or_else(|| format!("no frame {index}"))?.map_err(|e| e.to_string())?;
    println!(
        "frame {index} at offset {}: pre={} post={} substeps={} car_bytes={}",
        frame.offset,
        frame.pre,
        frame.post,
        frame.substeps,
        frame.car.len()
    );
    println!("control f32: {:?}", frame.control_f32s());
    for offset in (0..CAR_DWORDS * 4).step_by(4) {
        let (Some(bits), Some(value)) = (read_u32(frame.car, offset), read_f32(frame.car, offset)) else {
            break;
        };
        println!("car+{offset:#06x}: {bits:#010x} {value:?}");
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let bytes = std::fs::read(&args.path).map_err(|e| format!("{}: {e}", args.path))?;
    match args.frame {
        Some(index) => show_frame(&bytes, index),
        None => summarize(&bytes),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rec_info: {e}\nusage: rec_info <file> [--frame N]");
            ExitCode::FAILURE
        }
    }
}

//! `ngp-compat`: whether another `NGP.dll` can be read like the supported NGP 7.4.778 (`rbr_formats::ngp::compat`).
//!
//! ```text
//! ngp-compat CANDIDATE.dll [--reference NGP.dll] [--rbz PHYSICS.rbz]
//! ```
//!
//! The reference is a supported DLL: `--reference`, else `RBR_NGP_DLL`, else the installed plugin of `RBR_DATA`.
//! `--rbz` also prints the `PhysicsRevision_NGP` of the candidate's `physics.rbz`. Exit status 0 when the DLL is the
//! supported one or what we read from it is unchanged, 1 when it changed or cannot be read, 2 on a bad call. Nothing
//! is written. What to do with the answer: `notes/ngp.md`, "Adding an NGP version".

use rbr_formats::data::Data;
use rbr_formats::ngp::{self, compat};
use std::path::PathBuf;
use std::process::ExitCode;

fn run() -> Result<bool, String> {
    let (mut candidate, mut reference, mut rbz) = (None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--reference" => reference = Some(PathBuf::from(value()?)),
            "--rbz" => rbz = Some(PathBuf::from(value()?)),
            _ if arg.starts_with("--") => return Err(format!("unknown argument {arg}")),
            _ if candidate.is_none() => candidate = Some(PathBuf::from(&arg)),
            _ => return Err(format!("unexpected argument {arg}")),
        }
    }
    let candidate = candidate.ok_or("usage: ngp-compat CANDIDATE.dll [--reference NGP.dll] [--rbz PHYSICS.rbz]")?;
    let data = Data::from_env().unwrap_or_else(|| Data::new("", None));
    let reference = reference
        .or_else(|| ngp::path(&data))
        .ok_or("no reference DLL: give --reference, or set RBR_NGP_DLL (the supported NGP 7.4.778 DLL)")?;
    let read = |path: &PathBuf| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let report = compat::compare(&read(&reference)?, &read(&candidate)?)?;
    print!("{}", report.render());
    if let Some(path) = rbz {
        match compat::physics_revision(&path)? {
            Some(revision) => println!("physics.rbz: PhysicsRevision_NGP {revision}"),
            None => println!("physics.rbz: no PhysicsRevision_NGP: not NGP's"),
        }
    }
    Ok(matches!(report.verdict(), compat::Verdict::Identical | compat::Verdict::ReadsUnchanged))
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("ngp-compat: {e}");
            ExitCode::from(2)
        }
    }
}

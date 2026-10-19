//! `rbz`: list and extract Richard Burns Rally `.rbz` archives.
//!
//! ```text
//! rbz list <file.rbz>
//! rbz extract <file.rbz> <outdir>
//! ```

use rbr_formats::rbz;
use std::path::Path;
use std::process::ExitCode;

fn list(file: &Path) -> Result<(), rbz::Error> {
    let mut archive = rbz::open(file)?;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        println!(
            "{:>10} {:>10}  {:<8} {}",
            entry.size(),
            entry.compressed_size(),
            entry.compression().to_string(),
            entry.name()?
        );
    }
    Ok(())
}

fn extract(file: &Path, outdir: &Path) -> Result<(), rbz::Error> {
    let mut archive = rbz::open(file)?;
    archive.extract(outdir)?;
    eprintln!("extracted {} entries to {}", archive.len(), outdir.display());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [_, "list", file] => list(Path::new(file)),
        [_, "extract", file, outdir] => extract(Path::new(file), Path::new(outdir)),
        _ => {
            eprintln!("usage:\n  rbz list <file.rbz>\n  rbz extract <file.rbz> <outdir>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rbz: {e}");
            ExitCode::FAILURE
        }
    }
}

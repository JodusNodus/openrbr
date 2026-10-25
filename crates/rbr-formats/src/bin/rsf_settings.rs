//! `rsf-settings`: the force-feedback and wheel settings an RSF drive of a car runs with (`rbr_formats::rsf::settings`),
//! as `key=value` lines for `harness/rsf_compare.py`.
//!
//! ```text
//! rsf-settings (--install RSF_DIR | --session SESSION_DIR) --car ID [--slot N] [--profile pf<name>.rbr]
//! ```
//!
//! `--session` reads the session's `meta.txt`; `--profile` gives the game's profile a session does not hold. The slot
//! is the game's car slot (RSF's 5 by default). Nothing is written.

use rbr_formats::rsf::settings::{game_options, Sources};
use rbr_formats::rsf::SLOT;
use std::path::PathBuf;
use std::process::ExitCode;

fn run() -> Result<(), String> {
    let (mut install, mut session, mut car, mut slot, mut profile) = (None, None, None, SLOT, None);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--install" => install = Some(PathBuf::from(value()?)),
            "--session" => session = Some(PathBuf::from(value()?)),
            "--car" => car = Some(value()?.parse::<u32>().map_err(|e| format!("--car: {e}"))?),
            "--slot" => slot = value()?.parse().map_err(|e| format!("--slot: {e}"))?,
            "--profile" => profile = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    let car = car.ok_or("--car ID is needed")?;
    let mut sources = match (install, session) {
        (Some(root), None) => Sources::install(&root, car),
        (None, Some(dir)) => {
            let meta = dir.join("meta.txt");
            Sources::session(&std::fs::read_to_string(&meta).map_err(|e| format!("{}: {e}", meta.display()))?, car)
        }
        _ => return Err("give one of --install and --session".into()),
    };
    if let Some(path) = profile {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        sources.game = Some((game_options(&bytes).map_err(|e| format!("{}: {e}", path.display()))?, name));
    }
    let s = sources.resolve(slot);
    let [g, t, n] = s.sensitivity;
    println!("sensitivity={g},{t},{n}");
    println!("sensitivity_source={}", s.sensitivity_source.map(|x| x.name()).join(","));
    println!("dead_zone={}\ndead_zone_threshold={}", s.dead_zone, s.dead_zone_threshold);
    println!(
        "ffb_rate={}\ninput_rate={}\nhead_gasket_durability={}",
        s.ffb_rate, s.input_rate, s.head_gasket_durability
    );
    if let Some((game, source)) = &s.game {
        println!("enabled={}\nstrength={}\ninverse={}", u8::from(game.enabled), game.strength, u8::from(game.inverse));
        println!(
            "gear_guard={}\nneutral_lock={}\nclutch_help={}",
            u8::from(game.gear_guard),
            u8::from(game.neutral_lock),
            u8::from(game.clutch_help)
        );
        println!("game_source={}", source.name());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rsf-settings: {e}");
            ExitCode::FAILURE
        }
    }
}

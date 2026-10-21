//! The game's particle records of a recorded drive, for its playback: what the recording lacks (the lamps' reverse
//! switch and damage, PEV1 kind 6, and the game's own tyre payloads). `RBR_PARTICLE_EVENTS=<file>` gives them, as
//! `rbr-standalone --boot --recording X --events-out FILE` writes them; without it a playback has none.

use std::collections::HashMap;
use std::path::Path;

use super::{load_events, Step};

/// The recorded events of the drive by step, from `RBR_PARTICLE_EVENTS`; empty when it is unset or empty, or
/// (reported on stderr) when the file does not load.
pub fn events() -> HashMap<u32, Step> {
    match std::env::var_os("RBR_PARTICLE_EVENTS") {
        Some(path) if !path.is_empty() => load_events(Path::new(&path)).unwrap_or_else(|e| {
            eprintln!("particles: recorded events: {e}");
            HashMap::new()
        }),
        _ => HashMap::new(),
    }
}

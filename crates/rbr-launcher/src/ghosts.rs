//! The quick race's ghost car: the game's best and saved ghosts, by map number.

use std::path::Path;

use crate::catalog;
use crate::Form;
use rbr_gameplay::weather::Weather;

/// The ghost car to race in a quick race: none, the best one kept per map, or a saved ghost of `./GhostCars/`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GhostChoice {
    #[default]
    Off,
    Best,
    File(String),
}

/// Where the ghosts are, in the player's folder (the game's `./GhostCars/`).
pub const GHOST_DIR: &str = "GhostCars";
/// The best ghost's name (ours: the game names a ghost when the player saves it).
pub const BEST_GHOST: &str = "Best";

impl GhostChoice {
    pub fn parse(text: &str) -> GhostChoice {
        match text {
            "" | "off" => GhostChoice::Off,
            "best" => GhostChoice::Best,
            file => GhostChoice::File(file.to_owned()),
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            GhostChoice::Off => "off".to_owned(),
            GhostChoice::Best => "best".to_owned(),
            GhostChoice::File(file) => file.clone(),
        }
    }

    /// The viewer's arguments for map `map`: `--best-ghost` with the weather words a new best keeps, or `--ghost` with
    /// the file (none when the file is gone).
    pub fn viewer_args(&self, here: &Path, map: u32, weather: &rbr_formats::ghost::Weather) -> Vec<String> {
        let dir = here.join(GHOST_DIR);
        match self {
            GhostChoice::Off => Vec::new(),
            GhostChoice::Best => vec![
                "--best-ghost".into(),
                dir.join(rbr_formats::ghost::SavedGhost::file_name(BEST_GHOST, map)).display().to_string(),
                "--ghost-weather".into(),
                rbr_render::ghost::weather_arg(weather),
            ],
            GhostChoice::File(file) if dir.join(file).is_file() => {
                vec!["--ghost".into(), dir.join(file).display().to_string()]
            }
            GhostChoice::File(_) => Vec::new(),
        }
    }
}

/// The weather words of the saved ghost `file` in `./GhostCars/` (its header only), or `None` when it does not read.
pub fn ghost_words(here: &Path, file: &str) -> Option<rbr_formats::ghost::Weather> {
    use std::io::Read;
    let mut header = Vec::new();
    let file = std::fs::File::open(here.join(GHOST_DIR).join(file)).ok()?;
    file.take(rbr_formats::ghost::SAVED_HEADER_LEN as u64).read_to_end(&mut header).ok()?;
    rbr_formats::ghost::SavedGhost::header_weather_words(&header)
}

/// The saved ghosts of map `map` in `./GhostCars/` (`<name>_<map:02>.ghc`, the best one too), sorted.
pub fn ghost_files(here: &Path, map: u32) -> Vec<String> {
    let suffix = format!("_{map:02}.ghc");
    let mut files: Vec<String> = std::fs::read_dir(here.join(GHOST_DIR))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|f| f.ends_with(&suffix))
        .collect();
    files.sort();
    files
}

impl Form {
    /// The stage and weather of the chosen saved ghost: loading a `.ghc` sets the race's weather from its words
    /// (`E18_ConnGhostMemory::Dispatch` 0x4930d0). `None` without a file ghost, or when its words fit no season row of
    /// the map (then the Weather field decides).
    pub fn ghost_weather(&self) -> Option<(catalog::Stage, Weather)> {
        let GhostChoice::File(file) = &self.ghost else {
            return None;
        };
        let words = ghost_words(&self.records(), file)?;
        self.catalog.ghost_stage_weather(&self.season, self.catalog.maps[self.map].number, &words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_ghost_lists_its_weather_words() {
        let here = std::env::temp_dir().join(format!("rbr-launcher-ghost-{}", std::process::id()));
        std::fs::create_dir_all(here.join(GHOST_DIR)).unwrap();
        let weather =
            rbr_formats::ghost::Weather { track_letter: 3, cloud: 2, wetness: 2, wear: 1, ..Default::default() };
        let ghost = rbr_formats::ghost::SavedGhost { map: 54, car: 6, weather, stream: Default::default() };
        std::fs::write(here.join(GHOST_DIR).join("me_54.ghc"), ghost.write()).unwrap();
        assert_eq!(ghost_words(&here, "me_54.ghc"), Some(weather));
        assert_eq!(ghost_words(&here, "gone_54.ghc"), None);
        assert_eq!(ghost_files(&here, 54), ["me_54.ghc"]);
        std::fs::remove_dir_all(&here).unwrap();
    }
}

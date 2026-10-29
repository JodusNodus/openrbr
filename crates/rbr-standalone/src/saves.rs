//! The stage record and the personal best of the player's save files (`--saves DIR [--profile NAME]`): the standing the
//! split display and the finish panel compare with, and the new best written at the race's end
//! (`rbr_gameplay::record`). `DIR` holds `pfresult.rst` and `pf<name>.rbr`, as the game's `SavedGames` does.

use std::path::Path;

use rbr_formats::saves::profile::{InputFilter, ProfileRecord};
use rbr_formats::saves::results::ResultRecord;
use rbr_formats::saves::SaveDir;
use rbr_formats::times::StageTimes;
use rbr_gameplay::record::{self, Book, Finish, Update};
use rbr_recording::splits::{Mark, Standing, Verdict};

/// One stage's records, loaded when the race starts.
pub struct Saves {
    dir: SaveDir,
    profile: String,
    book: Book,
    stage: u32,
    car: u32,
    /// The record's row when the race started: what the splits are compared with, whatever the race writes.
    before: ResultRecord,
    /// What the race wrote, once it has finished.
    result: Option<(Update, f32)>,
    /// The files are not written (a replay's finish only compares).
    dry: bool,
}

impl Saves {
    /// Loads the records of `stage` for the profile `profile` (the first profile in `dir` without one named, else `player`, the launcher's default).
    pub fn open(dir: &Path, profile: Option<&str>, times: &[StageTimes], stage: u32, car: u32) -> Result<Self, String> {
        let dir = SaveDir { path: dir.to_path_buf() };
        let profile = match profile {
            Some(name) => name.to_owned(),
            None => {
                dir.profiles().map_err(|e| e.to_string())?.into_iter().next().unwrap_or_else(|| "player".to_owned())
            }
        };
        let book = Book::load(&dir, &profile, times)?;
        let before = record::stage_row(&book.table, stage);
        Ok(Self { dir, profile, book, stage, car, before, result: None, dry: false })
    }

    /// The records compared as the race ends, the files not written (a replay).
    pub fn dry(self) -> Self {
        Self { dry: true, ..self }
    }

    /// The profile's input filter grid (`E16Filter::ReadBin` 0x48bd60, as `E20ReadProfile` reads it), when the profile
    /// file exists.
    pub fn filter(&self) -> Option<InputFilter> {
        self.dir.load_profile(&self.profile).ok().map(|file| file.filter)
    }

    /// The grid a profile this race creates is written with (the profile file is absent until then).
    pub fn set_new_filter(&mut self, filter: InputFilter) {
        if let Some(file) = &mut self.book.file {
            file.filter = filter;
        }
    }

    /// The splits' and the finish's reference times, from the record before the race.
    pub fn reference(&self) -> [f32; 3] {
        self.before.times
    }

    /// The race has been written to the files.
    pub fn written(&self) -> bool {
        self.result.is_some()
    }

    /// Re-arms the saves after an in-process restart (`Live::restart`): the next finish writes again, compared
    /// against the record as it now stands. A fresh boot's book has no submitted race (`community_submitted`).
    pub fn rearm(&mut self) {
        self.result = None;
        self.book.community_submitted = false;
        self.before = record::stage_row(&self.book.table, self.stage);
    }

    /// Offers the finished stage to the files: the personal best and the record, and writes what changed. Once.
    pub fn finish(&mut self, times: [f32; 3]) -> Result<Update, String> {
        let finish = Finish { stage: self.stage, car: self.car, times };
        let update = self.book.submit(&finish);
        self.result = Some((update, times[2]));
        if !self.dry {
            self.book.save(&self.dir, &self.profile, update)?;
        }
        Ok(update)
    }

    /// The record, the personal best and the result page's message.
    pub fn standing(&self) -> Standing {
        let (row, best) = self.book.standing(self.stage);
        let verdict = match self.result {
            Some((_, time)) => match record::verdict(&row, best.as_ref(), time) {
                record::Verdict::None => Verdict::None,
                record::Verdict::PersonalBest => Verdict::PersonalBest,
                record::Verdict::StageRecord => Verdict::StageRecord,
            },
            None => Verdict::None,
        };
        let mark = |car, splits, time, holder: String| Mark { car, splits, time, holder };
        Standing {
            record: mark(row.w04, [row.times[0], row.times[1]], row.times[2], row.name()),
            personal: best.map(|b: ProfileRecord| mark(b.w04, [b.f08[0], b.f08[1]], b.f08[2], String::new())),
            verdict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rearm` after a finish: the next finish writes again, compared against the record as it now stands.
    #[test]
    fn rearm_writes_the_next_finish_against_the_new_record() {
        let dir = std::env::temp_dir().join(format!("rbr-saves-rearm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let times = vec![StageTimes { stage: 11, splits: [60.0, 120.0], finish: 185.0 }];
        let mut saves = Saves::open(&dir, Some("Ann"), &times, 11, 0).unwrap();
        assert_eq!(saves.reference(), [60.0, 120.0, 185.0]);
        saves.finish([50.0, 100.0, 150.0]).unwrap();
        assert!(saves.written());
        assert!(dir.join("pfresult.rst").is_file());
        saves.rearm();
        assert!(!saves.written());
        assert_eq!(saves.reference(), [50.0, 100.0, 150.0]);
        // Without the re-arm the second finish would be dropped (`race_end` checks `written`).
        saves.finish([51.0, 101.0, 151.0]).unwrap();
        assert!(saves.written());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

//! The stage record and the personal best of the player's save files (`--saves DIR [--profile NAME]`): the standing the
//! split display and the finish panel compare with, and the new best written at the race's end
//! (`rbr_gameplay::record`). `DIR` holds `pfresult.rst` and `pf<name>.rbr`, as the game's `SavedGames` does.

use std::path::Path;

use rbr_formats::saves::profile::ProfileRecord;
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
        Ok(Self { dir, profile, book, stage, car, before, result: None })
    }

    /// The splits' and the finish's reference times, from the record before the race.
    pub fn reference(&self) -> [f32; 3] {
        self.before.times
    }

    /// The race has been written to the files.
    pub fn written(&self) -> bool {
        self.result.is_some()
    }

    /// Offers the finished stage to the files: the personal best and the record, and writes what changed. Once.
    pub fn finish(&mut self, times: [f32; 3]) -> Result<Update, String> {
        let finish = Finish { stage: self.stage, car: self.car, times };
        let update = self.book.submit(&finish);
        self.result = Some((update, times[2]));
        self.book.save(&self.dir, &self.profile, update)?;
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

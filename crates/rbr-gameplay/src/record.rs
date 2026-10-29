//! The stage record and the personal best of a quick race (`CQRResultUI`, `MSG_QRBEST`, `BEAT_PERSONAL`,
//! `STAGERECORD`, `PERSONALBEST`), in the game's own save files (`rbr_formats::saves`).
//!
//! What the original does, traced:
//! - The stage record is a row of the gameflow's table of 37 rows of 0x34 bytes (`pfresult.rst`, read by
//!   `CMenuSaving::ReadFromBinStream` 0x585c10 into the table at gameflow +8): stage id, the car, the two split times,
//!   the finish time and the holder's name. `M5653c0` (0x5653c0) copies a stage's row out; for a stage the table does
//!   not have it gives the car 0 and 300, 600 and 1200 s.
//! - The personal best is the profile's record for the stage (`E69Profile::FindEntry` 0x5994b0, 0x18 bytes at profile
//!   +0xac): stage id, car, split 1, split 2, finish time.
//! - At the end of the race `CApplication::M00480810` (0x480810) calls `M00567330` (0x567330) with the stage, the car,
//!   the two split times (HUD +0x258, +0x25c) and the race time (HUD +0x140, penalties included) of a finished race
//!   (any mode but 6, 7 and 9; a retired quick race in mode 4 only, with 1800 s). It finds the stage's row; with a
//!   profile it first offers the profile the time (`E69Profile::InsertTime` 0x599580: a new stage is appended; an
//!   existing one is replaced when the new time is not slower); then the row is replaced when the new time is faster
//!   than the row's, and the holder's name is the profile's, copied up to its NUL.
//! - The result page's message (`CQRResultBaseUI::DrawGS` 0x5c3880, drawn after that call): when the row's time is
//!   below the race time, the personal best's message (`BEAT_PERSONAL`) shows if the profile has a record for the
//!   stage that is not below the race time; otherwise nothing. When it is not below, the stage record's message
//!   (`MSG_QRBEST`, with the profile's name).
//!
//! Ours: the files are written at once, where the game writes them from its menus' save; a missing `pfresult.rst` is
//! built from `times.ini` (the retail table holds the same times under "Richard Burns", checked by a test over
//! `RBR_SAVES`); and the holder is the selected profile (the game uses its profile slot 0 in a quick race).
//!
//! Ours: a stage the table has no row for (a community stage, ids above the retail range) keeps its record and the
//! profiles' bests in [`COMMUNITY_FILE`] next to the game's files, by the same rules ([`Community`]): the game's table
//! and profiles hold 37 rows each, for the retail stages, and the game drops a stage it has no row for.

use rbr_formats::saves::profile::{Profile, ProfileFile, ProfileRecord};
use rbr_formats::saves::results::{ResultRecord, Results, RECORDS};
use rbr_formats::saves::{write_atomic, SaveDir};
use rbr_formats::times::StageTimes;

/// What `M5653c0` gives for a stage the table does not have: the splits and the finish time.
pub const MISSING_TIMES: [f32; 3] = [300.0, 600.0, 1200.0];
/// The finish time a retired race offers (`0x44e10000`).
pub const RETIRED_TIME: f32 = 1800.0;
/// The holder of the retail table's rows (the times.ini times).
pub const DEFAULT_HOLDER: &str = "Richard Burns";

/// One finished stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Finish {
    pub stage: u32,
    pub car: u32,
    /// The splits as the HUD holds them (+0x258, +0x25c), then the race time with penalties (+0x140).
    pub times: [f32; 3],
}

/// What a [`submit`] changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Update {
    /// The profile's record for the stage was written.
    pub personal: bool,
    /// The stage record's row was replaced.
    pub record: bool,
}

/// Which message the result page shows ([`verdict`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verdict {
    #[default]
    None,
    /// `BEAT_PERSONAL`.
    PersonalBest,
    /// `MSG_QRBEST`.
    StageRecord,
}

/// `M5653c0`: the stage's row, or the default row for a stage the table lacks.
pub fn stage_row(table: &Results, stage: u32) -> ResultRecord {
    table.records.iter().take(RECORDS).find(|r| r.id == stage).copied().unwrap_or(ResultRecord {
        id: stage,
        w04: 0,
        times: MISSING_TIMES,
        name: [0; 32],
    })
}

/// `E69Profile::FindEntry`: the profile's record for the stage.
pub fn personal_best(profile: &Profile, stage: u32) -> Option<ProfileRecord> {
    profile.records.iter().take(profile.record_count as usize).find(|r| r.id == stage).copied()
}

/// `E69Profile::InsertTime`: the profile's record for the stage, replaced when the new time is not slower, or added.
fn insert_time(profile: &mut Profile, finish: &Finish) -> bool {
    let [first, second, time] = finish.times;
    let record = ProfileRecord { id: finish.stage, w04: finish.car, f08: [first, second, time] };
    let count = (profile.record_count as usize).min(profile.records.len());
    match profile.records[..count].iter_mut().find(|r| r.id == finish.stage) {
        Some(old) => {
            if old.f08[2] < time {
                return false;
            }
            *old = record;
        }
        None => {
            if count == profile.records.len() {
                return false;
            }
            profile.records[count] = record;
            profile.record_count += 1;
        }
    }
    true
}

/// Copies `name` into the row's name up to and including its NUL, leaving the bytes after it as they were (the
/// original's byte loop).
fn copy_name(to: &mut [u8; 32], name: &str) {
    let bytes = name.as_bytes();
    let n = bytes.len().min(31);
    to[..n].copy_from_slice(&bytes[..n]);
    to[n] = 0;
}

/// `M00567330`: offers a finished stage to the profile and to the stage record's row. `holder` is the name written
/// into a replaced row.
pub fn submit(table: &mut Results, profile: Option<&mut Profile>, finish: &Finish, holder: &str) -> Update {
    let Some(row) = table.records.iter_mut().take(RECORDS).find(|r| r.id == finish.stage) else {
        return Update::default();
    };
    let personal = profile.is_some_and(|p| insert_time(p, finish));
    if row.times[2] <= finish.times[2] {
        return Update { personal, record: false };
    }
    row.w04 = finish.car;
    row.times = finish.times;
    copy_name(&mut row.name, holder);
    Update { personal, record: true }
}

/// `DrawGS`'s choice for a race of `time`, after [`submit`].
pub fn verdict(row: &ResultRecord, personal: Option<&ProfileRecord>, time: f32) -> Verdict {
    if row.times[2] < time {
        match personal {
            Some(best) if best.f08[2].partial_cmp(&time) != Some(std::cmp::Ordering::Less) => Verdict::PersonalBest,
            _ => Verdict::None,
        }
    } else {
        Verdict::StageRecord
    }
}

/// The retail table's rows made from `times.ini` (ours, for a game directory without `pfresult.rst`).
pub fn table_from_times(times: &[StageTimes]) -> Results {
    let mut name = [0; 32];
    copy_name(&mut name, DEFAULT_HOLDER);
    Results {
        records: times
            .iter()
            .take(RECORDS)
            .map(|t| ResultRecord { id: t.stage, w04: 0, times: [t.splits[0], t.splits[1], t.finish], name })
            .collect(),
    }
}

/// The file of the community stages' records, in the save folder (ours).
pub const COMMUNITY_FILE: &str = "community-records.ini";

/// The records of the stages the game's table has no row for (ours): per stage id, the record row and each profile's
/// best. Text, one section per stage: `[<id>]`, `record = <car> <split 1> <split 2> <finish> <holder>` and
/// `best.<profile> = <car> <split 1> <split 2> <finish>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Community {
    pub stages: std::collections::BTreeMap<u32, CommunityStage>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommunityStage {
    pub record: Option<ResultRecord>,
    pub bests: std::collections::BTreeMap<String, ProfileRecord>,
}

impl Community {
    pub fn parse(text: &str) -> Community {
        let mut stages = std::collections::BTreeMap::new();
        let mut current: Option<u32> = None;
        for line in text.lines().map(str::trim) {
            if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = id.trim().parse().ok();
                continue;
            }
            let (Some(id), Some((key, value))) = (current, line.split_once('=')) else { continue };
            let mut words = value.split_whitespace();
            let car = words.next().and_then(|w| w.parse().ok());
            let times: Vec<f32> = words.by_ref().take(3).filter_map(|w| w.parse().ok()).collect();
            let (Some(car), &[a, b, c]) = (car, &times[..]) else { continue };
            let stage: &mut CommunityStage = stages.entry(id).or_default();
            match key.trim() {
                "record" => {
                    let mut name = [0; 32];
                    copy_name(&mut name, &words.collect::<Vec<_>>().join(" "));
                    stage.record = Some(ResultRecord { id, w04: car, times: [a, b, c], name });
                }
                key => {
                    if let Some(profile) = key.strip_prefix("best.") {
                        stage.bests.insert(profile.to_owned(), ProfileRecord { id, w04: car, f08: [a, b, c] });
                    }
                }
            }
        }
        Community { stages }
    }

    pub fn to_text(&self) -> String {
        let mut out =
            String::from("; Records of the community stages (rbr-recomp; the game's table has no rows for them)\n");
        for (id, stage) in &self.stages {
            out += &format!("[{id}]\n");
            if let Some(r) = &stage.record {
                out += &format!("record = {} {} {} {} {}\n", r.w04, r.times[0], r.times[1], r.times[2], r.name());
            }
            for (profile, b) in &stage.bests {
                out += &format!("best.{profile} = {} {} {} {}\n", b.w04, b.f08[0], b.f08[1], b.f08[2]);
            }
        }
        out
    }

    /// The stage's record row: the stored one, else the game's row for a stage it does not have ([`MISSING_TIMES`]).
    pub fn row(&self, stage: u32) -> ResultRecord {
        self.stages.get(&stage).and_then(|s| s.record).unwrap_or(ResultRecord {
            id: stage,
            w04: 0,
            times: MISSING_TIMES,
            name: [0; 32],
        })
    }

    pub fn best(&self, stage: u32, profile: &str) -> Option<ProfileRecord> {
        self.stages.get(&stage).and_then(|s| s.bests.get(profile)).copied()
    }

    /// [`submit`]'s rules on the stage's entry: the best replaced when the new time is not slower, the record when
    /// it is faster.
    pub fn submit(&mut self, finish: &Finish, profile: &str, holder: &str) -> Update {
        let row = self.row(finish.stage);
        let stage = self.stages.entry(finish.stage).or_default();
        let [first, second, time] = finish.times;
        let personal = match stage.bests.get(profile) {
            Some(best) if best.f08[2] < time => false,
            _ => {
                let best = ProfileRecord { id: finish.stage, w04: finish.car, f08: [first, second, time] };
                stage.bests.insert(profile.to_owned(), best);
                true
            }
        };
        if row.times[2] <= time {
            return Update { personal, record: false };
        }
        let mut name = [0; 32];
        copy_name(&mut name, holder);
        stage.record = Some(ResultRecord { id: finish.stage, w04: finish.car, times: finish.times, name });
        Update { personal, record: true }
    }
}

/// The two save files a quick race reads and writes: the records table and the selected profile's file, and the
/// community stages' records.
#[derive(Debug, Clone)]
pub struct Book {
    pub table: Results,
    /// The profile file; its `current` profile is the player.
    pub file: Option<ProfileFile>,
    /// [`COMMUNITY_FILE`], and the profile's name it keys the bests by.
    pub community: Community,
    pub profile: String,
    /// The last [`Book::submit`] went to [`Book::community`].
    pub community_submitted: bool,
}

impl Book {
    /// Loads `pfresult.rst` (built from `times` when absent) and `pf<profile>.rbr` (absent: a fresh profile, ours, as
    /// the launcher makes one, so that a first race has a personal best).
    pub fn load(saves: &SaveDir, profile: &str, times: &[StageTimes]) -> Result<Book, String> {
        let table = match saves.load_results() {
            Ok(table) => table,
            Err(rbr_formats::saves::Error::Io { kind: std::io::ErrorKind::NotFound, .. }) => table_from_times(times),
            Err(e) => return Err(format!("pfresult.rst: {e}")),
        };
        let file = match saves.load_profile(profile) {
            Ok(file) => Some(file),
            Err(rbr_formats::saves::Error::Io { kind: std::io::ErrorKind::NotFound, .. }) => {
                Some(ProfileFile::fresh(profile))
            }
            Err(e) => return Err(format!("pf{profile}.rbr: {e}")),
        };
        // Only a missing file means no records: any other read failure (a permission error, invalid UTF-8)
        // must not silently discard every community record on the next save, as the profile file next to it.
        let community_path = saves.path.join(COMMUNITY_FILE);
        let community = match std::fs::read_to_string(&community_path) {
            Ok(text) => Community::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Community::default(),
            Err(e) => return Err(format!("{}: {e}", community_path.display())),
        };
        Ok(Book { table, file, community, profile: profile.to_owned(), community_submitted: false })
    }

    /// Whether the game's table has a row for `stage`; else it is a community stage's ([`Community`]).
    pub fn has_row(&self, stage: u32) -> bool {
        self.table.records.iter().take(RECORDS).any(|r| r.id == stage)
    }

    /// The player's profile.
    pub fn player(&self) -> Option<&Profile> {
        self.file.as_ref().and_then(|f| f.current())
    }

    /// The stage's record row and the player's best for it.
    pub fn standing(&self, stage: u32) -> (ResultRecord, Option<ProfileRecord>) {
        if !self.has_row(stage) {
            return (self.community.row(stage), self.community.best(stage, &self.profile));
        }
        (stage_row(&self.table, stage), self.player().and_then(|p| personal_best(p, stage)))
    }

    /// [`submit`] on the book, the player's name the holder.
    pub fn submit(&mut self, finish: &Finish) -> Update {
        let holder = self.player().map(Profile::name).unwrap_or_default();
        self.community_submitted = !self.has_row(finish.stage);
        if self.community_submitted {
            return self.community.submit(finish, &self.profile, &holder);
        }
        let profile = self.file.as_mut().and_then(|file| {
            let at = (file.profiles.current as usize).min(file.profiles.list.len().checked_sub(1)?);
            file.profiles.list.get_mut(at)
        });
        submit(&mut self.table, profile, finish, &holder)
    }

    /// Writes the table, and the profile file when an update changed it.
    pub fn save(&self, saves: &SaveDir, profile: &str, update: Update) -> Result<(), String> {
        if self.community_submitted {
            if update.record || update.personal {
                // The first save of a fresh folder (the launcher's `NGP/SavedGames`) makes it, as `SaveDir`'s do.
                let path = saves.path.join(COMMUNITY_FILE);
                std::fs::create_dir_all(&saves.path)
                    .and_then(|()| write_atomic(&path, self.community.to_text().as_bytes()))
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            }
            return Ok(());
        }
        if update.record {
            saves.save_results(&self.table).map_err(|e| format!("pfresult.rst: {e}"))?;
        }
        if let (true, Some(file)) = (update.personal, &self.file) {
            saves.save_profile(profile, file).map_err(|e| format!("pf{profile}.rbr: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_community_stages_first_record_makes_the_saves_folder() {
        let dir = std::env::temp_dir().join(format!("rbr-community-new-{}", std::process::id())).join("SavedGames");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
        let saves = SaveDir { path: dir.clone() };
        let mut book = Book::load(&saves, "Ann", &[]).unwrap();
        let update = book.submit(&Finish { stage: 138, car: 5, times: [40.5, 111.5, 145.1] });
        book.save(&saves, "Ann", update).unwrap();
        assert!(dir.join(COMMUNITY_FILE).is_file());
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_community_stage_round_trips_a_holder_with_spaces() {
        let text = "[101]\nrecord = 2 70.0 140.0 250.5 Ann Lee\nbest.Ann Lee = 2 70.0 140.0 250.5\n";
        let community = Community::parse(text);
        assert_eq!(community.row(101).name(), "Ann Lee");
        assert_eq!(community.best(101, "Ann Lee").map(|b| b.w04), Some(2));
        assert_eq!(Community::parse(&community.to_text()), community);
    }

    #[test]
    fn a_community_file_that_cannot_be_read_is_an_error_not_an_empty_book() {
        let dir = std::env::temp_dir().join(format!("rbr-community-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let saves = SaveDir { path: dir.clone() };
        let community = || saves.path.join(COMMUNITY_FILE);
        // Invalid UTF-8.
        std::fs::write(community(), b"[101]\nrecord = \xff\xfe\n").unwrap();
        let err = Book::load(&saves, "Ann", &[]).unwrap_err();
        assert!(err.contains(&community().display().to_string()), "{err}");
        // Any other read failure too (here a directory at the file's path); only a missing file is empty.
        std::fs::remove_file(community()).unwrap();
        std::fs::create_dir(community()).unwrap();
        let err = Book::load(&saves, "Ann", &[]).unwrap_err();
        assert!(err.contains(&community().display().to_string()), "{err}");
        std::fs::remove_dir(community()).unwrap();
        assert_eq!(Book::load(&saves, "Ann", &[]).unwrap().community, Community::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_community_stage_keeps_its_records_in_its_own_file() {
        let dir = std::env::temp_dir().join(format!("rbr-community-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let saves = SaveDir { path: dir.clone() };
        let times = [StageTimes { stage: 54, splits: [60.0, 120.0], finish: 185.0 }];
        let mut book = Book::load(&saves, "Ann", &times).unwrap();
        assert!(!book.has_row(101));
        assert_eq!(book.standing(101).0.times, MISSING_TIMES);
        let finish = Finish { stage: 101, car: 2, times: [70.0, 140.0, 250.5] };
        let update = book.submit(&finish);
        assert_eq!(update, Update { personal: true, record: true });
        book.save(&saves, "Ann", update).unwrap();
        assert!(!dir.join("pfresult.rst").exists(), "the game's table is not written");
        let again = Book::load(&saves, "Ann", &times).unwrap();
        let (row, best) = again.standing(101);
        assert_eq!((row.w04, row.times), (2, [70.0, 140.0, 250.5]));
        assert_eq!(best.map(|b| b.f08[2]), Some(250.5));
        let mut slower = again.clone();
        assert_eq!(slower.submit(&Finish { times: [1.0, 2.0, 300.0], ..finish }), Update::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn table() -> Results {
        table_from_times(&[
            StageTimes { stage: 10, splits: [97.0, 194.0], finish: 255.0 },
            StageTimes { stage: 54, splits: [57.0, 119.0], finish: 185.0 },
        ])
    }

    fn profile() -> Profile {
        let mut p = ProfileFile::fresh("Ann").profiles.list.remove(0);
        p.name[..4].copy_from_slice(b"Ann\0");
        p
    }

    fn finish(time: f32) -> Finish {
        Finish { stage: 54, car: 6, times: [58.0, 120.0, time] }
    }

    #[test]
    fn a_stage_the_table_lacks_has_the_default_row() {
        let row = stage_row(&table(), 3);
        assert_eq!((row.id, row.w04, row.times), (3, 0, MISSING_TIMES));
        assert_eq!(stage_row(&table(), 54).times, [57.0, 119.0, 185.0]);
    }

    #[test]
    fn a_slower_time_changes_nothing_but_a_first_personal_best() {
        let (mut t, mut p) = (table(), profile());
        let update = submit(&mut t, Some(&mut p), &finish(200.0), "Ann");
        assert_eq!(update, Update { personal: true, record: false });
        assert_eq!(personal_best(&p, 54).unwrap().f08, [58.0, 120.0, 200.0]);
        assert_eq!(stage_row(&t, 54).times[2], 185.0);
        // A slower second time leaves the best; an equal one replaces it.
        assert_eq!(submit(&mut t, Some(&mut p), &finish(210.0), "Ann"), Update::default());
        assert_eq!(personal_best(&p, 54).unwrap().f08[2], 200.0);
        let again = Finish { times: [59.0, 121.0, 200.0], ..finish(0.0) };
        assert!(submit(&mut t, Some(&mut p), &again, "Ann").personal);
        assert_eq!((p.record_count, personal_best(&p, 54).unwrap().f08[0]), (1, 59.0));
    }

    #[test]
    fn a_faster_time_takes_the_row_with_the_holders_name() {
        let (mut t, mut p) = (table(), profile());
        let update = submit(&mut t, Some(&mut p), &finish(180.0), "Ann");
        assert_eq!(update, Update { personal: true, record: true });
        let row = stage_row(&t, 54);
        assert_eq!((row.w04, row.times, row.name()), (6, [58.0, 120.0, 180.0], "Ann".to_owned()));
        // The same time does not take it again.
        assert!(!submit(&mut t, Some(&mut p), &finish(180.0), "Bob").record);
    }

    #[test]
    fn without_a_profile_only_the_row_changes_and_an_unknown_stage_nothing() {
        let mut t = table();
        assert_eq!(submit(&mut t, None, &finish(100.0), "Ann"), Update { personal: false, record: true });
        let other = Finish { stage: 99, ..finish(1.0) };
        assert_eq!(submit(&mut t, None, &other, "Ann"), Update::default());
    }

    #[test]
    fn a_name_overwrites_up_to_its_nul_and_keeps_the_rest() {
        let mut name = [b'x'; 32];
        copy_name(&mut name, "Ann");
        assert_eq!(&name[..6], b"Ann\0xx");
        let long = "n".repeat(40);
        copy_name(&mut name, &long);
        assert_eq!((name[30], name[31]), (b'n', 0));
    }

    #[test]
    fn the_verdict_follows_the_result_pages_comparisons() {
        let (mut t, mut p) = (table(), profile());
        // Slower than the record and no best of its own: nothing.
        let row = stage_row(&t, 54);
        assert_eq!(verdict(&row, None, 200.0), Verdict::None);
        // A first time becomes the best (submitted before the page): the personal best's message.
        submit(&mut t, Some(&mut p), &finish(200.0), "Ann");
        let (row, best) = (stage_row(&t, 54), personal_best(&p, 54));
        assert_eq!(verdict(&row, best.as_ref(), 200.0), Verdict::PersonalBest);
        // Slower than the best: nothing. Faster than the record: the record's message.
        assert_eq!(verdict(&row, best.as_ref(), 230.0), Verdict::None);
        submit(&mut t, Some(&mut p), &finish(180.0), "Ann");
        let (row, best) = (stage_row(&t, 54), personal_best(&p, 54));
        assert_eq!(verdict(&row, best.as_ref(), 180.0), Verdict::StageRecord);
    }

    #[test]
    fn the_book_round_trips_through_the_save_files() {
        let dir = std::env::temp_dir().join(format!("rbr-record-book-{}", std::process::id()));
        let saves = SaveDir { path: dir.clone() };
        let times = [StageTimes { stage: 54, splits: [57.0, 119.0], finish: 185.0 }];
        let mut book = Book::load(&saves, "Ann", &times).unwrap();
        assert_eq!(book.standing(54).0.times, [57.0, 119.0, 185.0]);
        let update = book.submit(&finish(170.0));
        assert_eq!(update, Update { personal: true, record: true });
        book.save(&saves, "Ann", update).unwrap();
        let back = Book::load(&saves, "Ann", &times).unwrap();
        let (row, best) = back.standing(54);
        assert_eq!((row.times, row.name()), ([58.0, 120.0, 170.0], "Ann".to_owned()));
        assert_eq!(best.unwrap().f08, [58.0, 120.0, 170.0]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_retail_table_holds_the_times_ini_times() {
        let (Some(data), Some(saves)) = (rbr_formats::data::Data::from_env(), std::env::var_os("RBR_SAVES")) else {
            eprintln!("skipping: RBR_DATA or RBR_SAVES not set");
            return;
        };
        let times = rbr_formats::times::read(&data.resolve("Root/misc.rbz").expect("misc.rbz")).unwrap();
        let bytes = std::fs::read(std::path::Path::new(&saves).join("pfresult.rst")).unwrap();
        let table = Results::read(&bytes).unwrap();
        for t in &times {
            let row = stage_row(&table, t.stage);
            assert_eq!(row.times, [t.splits[0], t.splits[1], t.finish], "stage {}", t.stage);
        }
    }
}

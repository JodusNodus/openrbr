//! The stage's track emitters (`CTrackEmittersManager`, `.dls` table 4; `notes/particles.md`, Track emitters).
//!
//! The game (`E83PartHost::M5e96f0` 0x5e96f0) clones, for each table-4 row `{name, dummy id, squared distance}`, the
//! particle file's emitter of that name and places it at the stage dummy (table-2 record) with the id. Each frame
//! (`M5e9490` 0x5e9490, from `E83System::Update`) an emitter emits at its file rate while its row is switched on and
//! the camera (`CSigCamera` +0x24, handler 0x5e9470) is nearer than the distance; else its rate is 0 (`FilterB`
//! 0x5d93f0). The switch starts off (the manager's +0x204 words are zeroed) and only `CSigAnimationDummyChange`
//! (handler 0x5e9550, emitted by `E12_AnimSet::Tic570` 0x47a570 for a dummy whose dirty bit an animation channel set,
//! 0x477a50 / 0x475e40) turns it on, to the dummy's active bit.
//!
//! So in 1.02 the retail rows never emit: the 82 rows (the chimneys of stages 10, 14, 41 and 46, `Chimney` and
//! `Chimney_Smoke`) name emitters no `PS_*.ini` has (the game logs "was not found"), and no animation channel names
//! their dummies. [`TrackEmitters::step`] follows that rule. The chimney smoke of [`TrackEmitters::light`] is ours (the
//! `--enhance particles` path): each row whose dummy is active (table-2 word 11) smokes while the camera is within its
//! distance, with the file's emitter when it has one, else [`chimney_smoke`].

use glam::Vec3;
use rbr_formats::dls::DlsFile;

use super::sim::{EmitterDef, Simulation};

/// One table-4 row, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The dummy's position, physics axes.
    pub at: Vec3,
    /// The squared distance within which the camera must be.
    pub reach2: f32,
    /// The dummy's active bit (table-2 word 11 not zero).
    pub active: bool,
    /// The particle file's emitter of the row's name.
    pub pool: Option<usize>,
}

/// The stage's track emitters.
#[derive(Debug, Clone, Default)]
pub struct TrackEmitters {
    pub rows: Vec<Row>,
    /// Ours: the rows are switched on by their dummies ([`TrackEmitters::light`]).
    lit: bool,
    /// Ours: the pool every row without its own emitter smokes from.
    ours: Option<usize>,
}

/// The game's most rows (`M5e96f0`: "Too many emitters").
const MAX_ROWS: usize = 0x40;
/// Ours: the chimney smoke's particles a second, life (s), rise and how much it follows the wind.
const SMOKE_RATE: f32 = 5.0;
const SMOKE_LIFE: f32 = 9.0;
const SMOKE_RISE: f32 = 1.2;
const SMOKE_WIND: f32 = 0.15;
/// Ours: the seconds of smoke made at once when the playback starts over, so the plumes stand at once.
const WARM: f32 = SMOKE_LIFE;

impl TrackEmitters {
    /// The rows of `dls` whose dummy exists (the game leaves an emitter without one at the origin), at most
    /// [`MAX_ROWS`], each with its emitter in `sim` (`sim.find`, the file's names) when there is one.
    pub fn new(dls: &DlsFile<'_>, sim: &Simulation) -> TrackEmitters {
        let rows = dls
            .emitters
            .iter()
            .take(MAX_ROWS)
            .filter_map(|&[name, id, reach2]| {
                let dummy = dls.table_2.iter().find(|t| t.ids[0] == id)?;
                let name = string_at(&dls.string_bytes, name)?;
                Some(Row {
                    at: Vec3::from(dummy.position),
                    reach2: f32::from_bits(reach2),
                    active: dummy.word_11 != 0.0,
                    pool: sim.find(name),
                })
            })
            .collect();
        TrackEmitters { rows, lit: false, ours: None }
    }

    /// Turns on our chimney smoke (ours, `--enhance particles`): adds its pool to `sim` when a row needs it.
    pub fn light(&mut self, sim: &mut Simulation) {
        if std::mem::replace(&mut self.lit, true) {
            return;
        }
        let rows = self.rows.iter().filter(|r| r.pool.is_none() && r.active).count();
        if rows > 0 {
            self.ours = chimney_smoke(sim, rows).map(|def| sim.add_pool(def));
        }
    }

    /// Whether row `row` emits with the camera at `eye`: the game's rule, unless lit ([`TrackEmitters::light`]).
    fn on(&self, row: &Row, eye: Vec3) -> bool {
        // The game's switch (the manager's +0x204 word) is never set for the retail rows (see the module doc).
        self.lit && row.active && row.at.distance_squared(eye) < row.reach2
    }

    /// One frame of `dt` seconds with the camera at `eye`, physics axes.
    pub fn step(&self, sim: &mut Simulation, eye: Vec3, dt: f32) {
        for row in &self.rows {
            let Some(pool) = row.pool.or(self.ours) else { continue };
            if self.on(row, eye) {
                let rate = sim.pools[pool].def.emit_rate;
                sim.emit(pool, row.at, row.at, Vec3::ZERO, rate * dt, f32::MIN);
            }
        }
    }

    /// Ours: makes [`WARM`] seconds of our smoke at once (the playback starting over), stepping only its pool.
    pub fn warm(&self, sim: &mut Simulation, eye: Vec3, dt: f32) {
        let Some(pool) = self.ours else { return };
        let mut gone = 0.0;
        while gone < WARM {
            for row in self.rows.iter().filter(|r| r.pool.is_none() && self.on(r, eye)) {
                sim.emit(pool, row.at, row.at, Vec3::ZERO, SMOKE_RATE * dt, f32::MIN);
            }
            sim.step_pool(pool, dt);
            gone += dt;
        }
    }
}

/// The NUL-terminated string at `offset` of the `.dls` string table.
fn string_at(bytes: &[u8], offset: u32) -> Option<&str> {
    let tail = bytes.get(offset as usize..)?;
    std::str::from_utf8(&tail[..tail.iter().position(|&b| b == 0)?]).ok()
}

/// Ours: a chimney's wood smoke for `rows` chimneys, from the file's `g_firesmoke_emitter` (every retail file has it):
/// its sprite, made pale grey, larger, slower and longer lived, rising and drifting with the wind. `None` when the
/// file has no such emitter.
pub fn chimney_smoke(sim: &Simulation, rows: usize) -> Option<EmitterDef> {
    let mut def = sim.pools[sim.find("g_firesmoke_emitter")?].def.clone();
    def.name = "chimney smoke (ours)".to_owned();
    def.max = (SMOKE_RATE * SMOKE_LIFE * 1.3) as usize * rows.max(1);
    def.emit_rate = SMOKE_RATE;
    def.shape = rbr_formats::pss::Shape::Cone { speed: -SMOKE_RISE, diversion: 0.4 };
    def.force_variance = 0.3;
    def.acceleration = Vec3::new(0.0, 0.0, 0.5);
    def.friction = 0.25;
    def.wind = SMOKE_WIND;
    def.lands = false;
    def.look.colors = [[0.62, 0.62, 0.64, 0.0], [0.66, 0.66, 0.68, 0.4], [0.72, 0.72, 0.74, 0.0]];
    def.look.color_at = 0.12;
    def.look.sizes = [[0.7, 0.7], [3.0, 3.0], [7.0, 7.0]];
    def.look.size_at = 0.4;
    def.look.size_variance = 0.6;
    def.look.life = SMOKE_LIFE;
    def.look.weight = 0.0;
    def.look.trail = 0.0;
    def.look.additive = false;
    def.look.offset_height = 0.5;
    Some(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(active: bool, pool: Option<usize>) -> TrackEmitters {
        let rows = vec![Row { at: Vec3::new(10.0, 0.0, 0.0), reach2: 100.0, active, pool }];
        TrackEmitters { rows, lit: false, ours: None }
    }

    #[test]
    fn the_games_rows_stay_off_unless_lit() {
        let unlit = rows(true, Some(0));
        assert!(!unlit.on(&unlit.rows[0], Vec3::ZERO), "the manager's switch is never set in 1.02");
        let mut lit = rows(true, None);
        lit.lit = true;
        assert!(lit.on(&lit.rows[0], Vec3::new(5.0, 0.0, 0.0)), "within 10 m");
        assert!(!lit.on(&lit.rows[0], Vec3::new(-0.5, 0.0, 0.0)), "the squared distance is exclusive past 10 m");
        let mut inactive = rows(false, None);
        inactive.lit = true;
        assert!(!inactive.on(&inactive.rows[0], Vec3::new(10.0, 0.0, 0.0)), "the dummy's active bit");
    }

    #[test]
    fn strings_end_at_their_nul() {
        assert_eq!(string_at(b"Chimney\0Chimney_Smoke\0", 8), Some("Chimney_Smoke"));
        assert_eq!(string_at(b"abc", 1), None);
    }

    /// The retail rows: 82 on six stages, all chimneys the stage's particle file does not name.
    #[test]
    fn retail_rows_name_no_emitter_of_their_file() {
        let Some(data) = std::env::var_os("RBR_DATA") else { return };
        let maps = std::path::Path::new(&data).join("Maps");
        let mut total = 0;
        for stage in ["track-10_O", "track-14_O", "track-41_M", "track-41_O", "track-46_E", "track-46_O"] {
            let bytes = std::fs::read(rbr_formats::stage_files::file_in(&maps, stage, "dls")).unwrap();
            let dls = rbr_formats::dls::parse(&bytes).unwrap();
            let file = if stage.starts_with("track-1") { "PS_Arctic.ini" } else { "PS_MontBlanc.ini" };
            let text = std::fs::read(maps.join(file)).unwrap();
            let pss = rbr_formats::pss::parse(&String::from_utf8_lossy(&text)).unwrap();
            let sim = Simulation::new(&pss, 1);
            let emitters = TrackEmitters::new(&dls, &sim);
            assert_eq!(emitters.rows.len(), dls.emitters.len(), "{stage}: every row has its dummy");
            assert!(emitters.rows.iter().all(|r| r.pool.is_none() && r.active), "{stage}");
            assert!(chimney_smoke(&sim, emitters.rows.len()).is_some(), "{stage}: g_firesmoke_emitter");
            total += emitters.rows.len();
        }
        assert_eq!(total, 82);
    }
}

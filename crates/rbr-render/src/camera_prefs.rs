//! The player's camera choices, ours: the view last chosen and each car's nudges of the four views
//! ([`crate::views`]), set on the pause menu's Camera page ([`crate::hud::camera_page`]). The viewer keeps them in the
//! `[camera]` section of the launcher's `launcher-settings.ini` (`viewer --cameras`), which the launcher carries over
//! when it writes the file:
//!
//! ```ini
//! [camera]
//! view = cockpit
//! head = off
//! COROLLA cockpit = 0 0.04 -0.1 -2
//! COROLLA bonnet = 0 0.02 0
//! rsf-24 chase = 0 0.5 -1
//! ```
//!
//! A car is its `Cars.ini` folder for the game's cars and `rsf-<id>` for an RSF car ([`car_key`]). A nudge is
//! right, up and forward in metres in the car's frame, and for the fixed views optionally a tilt in degrees up,
//! written only when it is not 0; the chase takes up (its height) and forward (nearer the car;
//! [`crate::game_camera::Settings::nudged`]). `head = off` keeps the in-car view fixed to the car ([`crate::head`];
//! on unless set off, from the pause menu's Camera page or the launcher's Graphics page).

use crate::game_camera::Settings;
use crate::playback::Playback;
use crate::views::{CarView, Mounts, Nudge};
use glam::Vec3;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The section's name.
pub const SECTION: &str = "camera";

/// One step of the Camera page, m: the fixed views' and the chase's; and of the tilt, degrees.
pub const STEP: f32 = 0.02;
pub const CHASE_STEP: f32 = 0.1;
pub const TILT_STEP: f32 = 0.5;
/// How far a fixed view moves from the car's own place, m, and tilts, degrees, each way (ours: keeps a typo from
/// losing the car).
pub const REACH: f32 = 1.0;
pub const TILT_REACH: f32 = 15.0;

/// One car's nudges, by [`CarView::index`].
pub type Nudges = [Nudge; CarView::ALL.len()];

/// The view last chosen and the cars' nudges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraPrefs {
    pub view: Option<CarView>,
    /// The in-car view's head is off ([`crate::head`]).
    pub head_off: bool,
    pub cars: BTreeMap<String, Nudges>,
}

/// The key of a car: `rsf-<id>` for an RSF car, else the game car's folder in capitals.
pub fn car_key(folder: &str, rsf_id: Option<u32>) -> String {
    match rsf_id {
        Some(id) => format!("rsf-{id}"),
        None => folder.to_ascii_uppercase(),
    }
}

impl CameraPrefs {
    /// The `[camera]` section of `text`; nothing for what is missing or malformed.
    pub fn parse(text: &str) -> CameraPrefs {
        let sections = rbr_formats::ini::parse(text).unwrap_or_default();
        let Some(section) = sections.iter().find(|s| s.name == SECTION) else {
            return CameraPrefs::default();
        };
        let mut prefs = CameraPrefs::default();
        for (key, value) in &section.entries {
            if key == "view" {
                prefs.view = CarView::parse(value);
                continue;
            }
            if key == "head" {
                prefs.head_off = value.trim() == "off";
                continue;
            }
            let Some((car, view)) = key.rsplit_once(' ').and_then(|(car, v)| Some((car.trim(), CarView::parse(v)?)))
            else {
                continue;
            };
            let words: Option<Vec<f32>> = value.split_whitespace().map(|w| w.parse().ok()).collect();
            let nudge = match words.as_deref() {
                Some(&[x, y, z]) => Nudge { shift: Vec3::new(x, y, z), tilt: 0.0 },
                Some(&[x, y, z, tilt]) => Nudge { shift: Vec3::new(x, y, z), tilt },
                _ => continue,
            };
            prefs.cars.entry(car.to_owned()).or_default()[view.index()] = nudge;
        }
        prefs
    }

    /// The section's text: the view and every nudge that is not zero.
    pub fn to_text(&self) -> String {
        let mut out = format!("[{SECTION}]\n");
        if let Some(view) = self.view {
            out.push_str(&format!("view = {}\n", view.name()));
        }
        if self.head_off {
            out.push_str("head = off\n");
        }
        for (car, nudges) in &self.cars {
            for view in CarView::ALL {
                let Nudge { shift: v, tilt } = nudges[view.index()];
                if nudges[view.index()] != Nudge::default() {
                    let tilt = if tilt != 0.0 { format!(" {tilt}") } else { String::new() };
                    out.push_str(&format!("{car} {} = {} {} {}{tilt}\n", view.name(), v.x, v.y, v.z));
                }
            }
        }
        out
    }

    /// Car `key`'s nudges (zeros when it has none).
    pub fn nudges(&self, key: &str) -> Nudges {
        self.cars.get(key).copied().unwrap_or_default()
    }

    /// Sets car `key`'s nudges, dropping the car when they are all zero.
    pub fn set_nudges(&mut self, key: &str, nudges: Nudges) {
        match nudges.iter().all(|v| *v == Nudge::default()) {
            true => self.cars.remove(key),
            false => self.cars.insert(key.to_owned(), nudges),
        };
    }

    /// `file` (a whole settings file) with its `[camera]` section replaced by this one, the rest kept as it is.
    pub fn replace_in(&self, file: &str) -> String {
        let mut out = String::new();
        let mut inside = false;
        for line in file.lines() {
            let header =
                line.trim().strip_prefix('[').and_then(|rest| rest.split_once(']')).map(|(name, _)| name.trim());
            if let Some(name) = header {
                inside = name == SECTION;
            }
            if !inside {
                out.push_str(line);
                out.push('\n');
            }
        }
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        out.push_str(&self.to_text());
        out
    }
}

/// The viewer's camera choices for one drive: the file they live in and the car's key.
#[derive(Debug, Clone, Default)]
pub struct Session {
    path: Option<PathBuf>,
    car: Option<String>,
    pub prefs: CameraPrefs,
}

impl Session {
    /// The choices in `path` (none without it or when it does not read) for the car `car` ([`car_key`]).
    pub fn load(path: Option<&Path>, car: Option<String>) -> Session {
        let prefs = path.and_then(|p| std::fs::read_to_string(p).ok()).map(|t| CameraPrefs::parse(&t));
        Session { path: path.map(Path::to_path_buf), car, prefs: prefs.unwrap_or_default() }
    }

    /// The view a drive starts in: `given` (`--view`), else the one last chosen, else the chase.
    pub fn start_view(&self, given: Option<CarView>) -> CarView {
        given.or(self.prefs.view).unwrap_or_default()
    }

    /// The car's nudges.
    pub fn nudges(&self) -> Nudges {
        self.car.as_deref().map(|car| self.prefs.nudges(car)).unwrap_or_default()
    }

    /// Whether the in-car view's head is on.
    pub fn head(&self) -> bool {
        !self.prefs.head_off
    }

    /// Puts the car's nudges on its fixed views and its chase, and the head on the in-car view.
    pub fn apply(&self, mounts: &mut Mounts, playback: &mut Playback) {
        let nudges = self.nudges();
        mounts.nudges = nudges;
        playback.set_chase_settings(Settings::default().nudged(nudges[CarView::Chase.index()].shift));
        playback.set_head(self.head());
    }

    /// The player chose `view` (C): kept for the next drive.
    pub fn chose(&mut self, view: CarView) {
        self.prefs.view = Some(view);
        self.save();
    }

    /// The view, the car's nudges and the head the Camera page set (not yet written: [`Session::save`]).
    pub fn set(&mut self, view: CarView, nudges: Nudges, head: bool) {
        self.prefs.view = Some(view);
        self.prefs.head_off = !head;
        if let Some(car) = &self.car {
            self.prefs.set_nudges(car, nudges);
        }
    }

    /// Writes the choices into the file's `[camera]` section, the rest of the file kept.
    pub fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let file = std::fs::read_to_string(path).unwrap_or_default();
        if let Err(e) = std::fs::write(path, self.prefs.replace_in(&file)) {
            eprintln!("viewer: {}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nudge(x: f32, y: f32, z: f32, tilt: f32) -> Nudge {
        Nudge { shift: Vec3::new(x, y, z), tilt }
    }

    #[test]
    fn prefs_round_trip_and_keep_the_rest_of_the_file() {
        let mut prefs = CameraPrefs { view: Some(CarView::Cockpit), head_off: true, ..Default::default() };
        let mut nudges = prefs.nudges("COROLLA");
        nudges[CarView::Cockpit.index()] = nudge(0.0, 0.04, -0.1, -2.0);
        nudges[CarView::Bonnet.index()] = nudge(0.0, 0.02, 0.0, 0.0);
        prefs.set_nudges("COROLLA", nudges);
        let mut chase = prefs.nudges("rsf-24");
        chase[CarView::Chase.index()] = nudge(0.0, 0.5, -1.0, 0.0);
        prefs.set_nudges("rsf-24", chase);
        let text = prefs.to_text();
        // The tilt is a 4th value only when it is not 0.
        assert_eq!(
            text,
            "[camera]\nview = cockpit\nhead = off\nCOROLLA cockpit = 0 0.04 -0.1 -2\nCOROLLA bonnet = 0 0.02 0\n\
             rsf-24 chase = 0 0.5 -1\n"
        );
        assert_eq!(CameraPrefs::parse(&text), prefs);
        // A tilt alone is written too.
        let mut tilted = CameraPrefs::default();
        tilted.set_nudges("XSARA", [nudge(0.0, 0.0, 0.0, 1.5), Nudge::default(), Nudge::default(), Nudge::default()]);
        assert_eq!(tilted.to_text(), "[camera]\nXSARA cockpit = 0 0 0 1.5\n");
        // Replaced in place of the old section, the other sections kept.
        let file = "[launcher]\ncar = COROLLA\n\n[camera]\nview = chase\n\n[controls]\nbrake = KeyB\n";
        let replaced = prefs.replace_in(file);
        assert_eq!(replaced, format!("[launcher]\ncar = COROLLA\n\n[controls]\nbrake = KeyB\n\n{text}"));
        assert_eq!(CameraPrefs::parse(&replaced), prefs);
        assert_eq!(rbr_formats::ini::parse(&replaced).unwrap().len(), 3);
        // Zeros drop the car; bad lines are skipped.
        prefs.set_nudges("COROLLA", Default::default());
        assert!(!prefs.cars.contains_key("COROLLA"));
        let bad = CameraPrefs::parse("[camera]\nview = roof\nX cockpit = 1 2\nY roof = 1 2 3\nZ bonnet = 1 2 3 4 5\n");
        assert_eq!(bad, CameraPrefs::default());
    }

    #[test]
    fn a_file_of_three_value_lines_is_written_back_the_same() {
        let file = "[launcher]\ncar = COROLLA\n\n[camera]\nview = bumper\nCOROLLA cockpit = 0 0.04 -0.1\n\
                    COROLLA chase = 0 0.5 -1\nrsf-24 bumper = -0.02 0.06 0\n";
        let prefs = CameraPrefs::parse(file);
        assert_eq!(prefs.nudges("COROLLA")[CarView::Cockpit.index()], nudge(0.0, 0.04, -0.1, 0.0));
        assert_eq!(prefs.replace_in(file), file);
    }

    #[test]
    fn a_session_keeps_the_view_and_the_car_s_nudges() {
        let path = std::env::temp_dir().join(format!("camera-prefs-{}.ini", std::process::id()));
        std::fs::write(&path, "[launcher]\ncar = COROLLA\n").unwrap();
        let mut session = Session::load(Some(&path), Some("COROLLA".into()));
        assert_eq!(session.start_view(None), CarView::Chase);
        let mut nudges = session.nudges();
        nudges[CarView::Cockpit.index()] = nudge(0.0, STEP, STEP, -TILT_STEP);
        nudges[CarView::Chase.index()] = nudge(0.0, CHASE_STEP, 0.0, 0.0);
        assert!(session.head(), "on unless set off");
        session.set(CarView::Cockpit, nudges, false);
        session.save();
        session.chose(CarView::Bumper);
        let again = Session::load(Some(&path), Some("COROLLA".into()));
        assert_eq!((again.start_view(None), again.nudges(), again.head()), (CarView::Bumper, nudges, false));
        assert_eq!(again.start_view(Some(CarView::Chase)), CarView::Chase);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("[launcher]\ncar = COROLLA\n\n[camera]\n"));
        // Another car has none.
        assert_eq!(Session::load(Some(&path), Some("XSARA".into())).nudges(), Nudges::default());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn car_keys() {
        assert_eq!(car_key("corolla", None), "COROLLA");
        assert_eq!(car_key("evo_IX", Some(24)), "rsf-24");
    }
}

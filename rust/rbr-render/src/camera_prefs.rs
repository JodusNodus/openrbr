//! The player's camera choices, ours: the view last chosen and each car's nudges of the four views
//! ([`crate::views`]). The viewer keeps them in the `[camera]` section of the launcher's `launcher-settings.ini`
//! (`viewer --cameras`), which the launcher carries over when it writes the file:
//!
//! ```ini
//! [camera]
//! view = cockpit
//! COROLLA cockpit = 0 0.04 -0.1
//! rsf-24 chase = 0 0.5 -1
//! ```
//!
//! A car is its `Cars.ini` folder for the game's cars and `rsf-<id>` for an RSF car ([`car_key`]). A nudge is
//! right, up and forward in metres in the car's frame; the chase takes up (its height) and forward (nearer the car;
//! [`crate::game_camera::Settings::nudged`]).

use crate::game_camera::Settings;
use crate::playback::Playback;
use crate::views::{CarView, Mounts};
use glam::Vec3;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winit::keyboard::KeyCode;

/// The section's name.
pub const SECTION: &str = "camera";

/// One step of the adjust mode, m: the fixed views' and the chase's.
pub const STEP: f32 = 0.02;
pub const CHASE_STEP: f32 = 0.1;
/// How far a fixed view moves from the car's own place, m, each way (ours: keeps a typo from losing the car).
pub const REACH: f32 = 1.0;

/// One car's nudges, by [`CarView::index`].
pub type Nudges = [Vec3; CarView::ALL.len()];

/// The view last chosen and the cars' nudges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraPrefs {
    pub view: Option<CarView>,
    pub cars: BTreeMap<String, Nudges>,
}

/// The key of a car: `rsf-<id>` for an RSF car, else the game car's folder in capitals.
pub fn car_key(folder: &str, rsf_id: Option<u32>) -> String {
    match rsf_id {
        Some(id) => format!("rsf-{id}"),
        None => folder.to_ascii_uppercase(),
    }
}

/// `nudge` moved by `step` (right, up, forward), with the fixed views kept within [`REACH`].
pub fn nudge(view: CarView, nudge: Vec3, step: Vec3) -> Vec3 {
    match view {
        CarView::Chase => nudge + Vec3::new(0.0, step.y, step.z),
        _ => (nudge + step).clamp(Vec3::splat(-REACH), Vec3::splat(REACH)),
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
            let Some((car, view)) = key.rsplit_once(' ').and_then(|(car, v)| Some((car.trim(), CarView::parse(v)?)))
            else {
                continue;
            };
            let words: Option<Vec<f32>> = value.split_whitespace().map(|w| w.parse().ok()).collect();
            if let Some([x, y, z]) = words.as_deref().and_then(|w| <[f32; 3]>::try_from(w).ok()) {
                prefs.cars.entry(car.to_owned()).or_insert([Vec3::ZERO; CarView::ALL.len()])[view.index()] =
                    Vec3::new(x, y, z);
            }
        }
        prefs
    }

    /// The section's text: the view and every nudge that is not zero.
    pub fn to_text(&self) -> String {
        let mut out = format!("[{SECTION}]\n");
        if let Some(view) = self.view {
            out.push_str(&format!("view = {}\n", view.name()));
        }
        for (car, nudges) in &self.cars {
            for view in CarView::ALL {
                let v = nudges[view.index()];
                if v != Vec3::ZERO {
                    out.push_str(&format!("{car} {} = {} {} {}\n", view.name(), v.x, v.y, v.z));
                }
            }
        }
        out
    }

    /// Car `key`'s nudges (zeros when it has none).
    pub fn nudges(&self, key: &str) -> Nudges {
        self.cars.get(key).copied().unwrap_or([Vec3::ZERO; CarView::ALL.len()])
    }

    /// Sets car `key`'s nudges, dropping the car when they are all zero.
    pub fn set_nudges(&mut self, key: &str, nudges: Nudges) {
        match nudges.iter().all(|v| *v == Vec3::ZERO) {
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

/// The adjust mode's keys: the step each moves the view by (right, up, forward), in [`STEP`]s.
pub fn adjust_step(code: KeyCode) -> Option<Vec3> {
    match code {
        KeyCode::ArrowRight => Some(Vec3::X),
        KeyCode::ArrowLeft => Some(Vec3::NEG_X),
        KeyCode::PageUp => Some(Vec3::Y),
        KeyCode::PageDown => Some(Vec3::NEG_Y),
        KeyCode::ArrowUp => Some(Vec3::Z),
        KeyCode::ArrowDown => Some(Vec3::NEG_Z),
        _ => None,
    }
}

/// What a key did in [`Session::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Not the adjust mode's: the key goes on.
    No,
    /// Taken; the views did not move.
    Taken,
    /// Taken, and the car's nudges changed: apply them ([`Session::apply`]).
    Moved,
}

/// The viewer's camera choices for one drive: the file they live in, the car's key and the adjust mode.
#[derive(Debug, Clone, Default)]
pub struct Session {
    path: Option<PathBuf>,
    car: Option<String>,
    pub prefs: CameraPrefs,
    pub adjusting: bool,
}

impl Session {
    /// The choices in `path` (none without it or when it does not read) for the car `car` ([`car_key`]).
    pub fn load(path: Option<&Path>, car: Option<String>) -> Session {
        let prefs = path.and_then(|p| std::fs::read_to_string(p).ok()).map(|t| CameraPrefs::parse(&t));
        Session { path: path.map(Path::to_path_buf), car, prefs: prefs.unwrap_or_default(), adjusting: false }
    }

    /// The view a drive starts in: `given` (`--view`), else the one last chosen, else the chase.
    pub fn start_view(&self, given: Option<CarView>) -> CarView {
        given.or(self.prefs.view).unwrap_or_default()
    }

    /// The car's nudges.
    pub fn nudges(&self) -> Nudges {
        self.car.as_deref().map(|car| self.prefs.nudges(car)).unwrap_or([Vec3::ZERO; CarView::ALL.len()])
    }

    /// Puts the car's nudges on its fixed views and its chase.
    pub fn apply(&self, mounts: &mut Mounts, playback: &mut Playback) {
        let nudges = self.nudges();
        mounts.nudges = nudges;
        playback.set_chase_settings(Settings::default().nudged(nudges[CarView::Chase.index()]));
    }

    /// The player chose `view` (C): kept for the next drive.
    pub fn chose(&mut self, view: CarView) {
        self.prefs.view = Some(view);
        self.save();
    }

    /// The camera adjust binding (`bindings::Action::CameraAdjust`): starts or ends the adjust mode. Ending writes
    /// the file.
    pub fn toggle_adjust(&mut self) {
        if self.car.is_none() {
            return;
        }
        self.adjusting = !self.adjusting;
        if !self.adjusting {
            self.save();
        }
    }

    /// A key pressed while `view` is shown, in the adjust mode: the [`adjust_step`] keys nudge the view, Backspace
    /// puts it back and Escape ends it, writing the file.
    pub fn key(&mut self, code: KeyCode, view: CarView) -> Handled {
        let Some(car) = self.car.clone().filter(|_| self.adjusting) else {
            return Handled::No;
        };
        if code == KeyCode::Escape {
            self.toggle_adjust();
            return Handled::Taken;
        }
        let mut nudges = self.prefs.nudges(&car);
        let at = &mut nudges[view.index()];
        match (code, adjust_step(code)) {
            (KeyCode::Backspace, _) => *at = Vec3::ZERO,
            (_, Some(step)) => *at = nudge(view, *at, step * if view == CarView::Chase { CHASE_STEP } else { STEP }),
            _ => return Handled::No,
        }
        self.prefs.set_nudges(&car, nudges);
        Handled::Moved
    }

    /// The HUD's line while adjusting `view`; `done` names the camera adjust binding.
    pub fn hud_line(&self, view: CarView, done: &str) -> Option<String> {
        let v = self.nudges()[view.index()];
        self.adjusting.then(|| match view {
            CarView::Chase => format!(
                "adjust chase: Up/Down nearer {:+.1} m, PgUp/PgDn height {:+.1} m; Backspace resets, {done} or Esc done",
                v.z, v.y
            ),
            _ => format!(
                "adjust {}: right {:+.2}, up {:+.2}, forward {:+.2} m (arrows, PgUp/PgDn); Backspace resets, {done} or Esc done",
                view.name(),
                v.x,
                v.y,
                v.z
            ),
        })
    }

    /// Writes the choices into the file's `[camera]` section, the rest of the file kept.
    fn save(&self) {
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

    #[test]
    fn prefs_round_trip_and_keep_the_rest_of_the_file() {
        let mut prefs = CameraPrefs { view: Some(CarView::Cockpit), ..Default::default() };
        let mut nudges = prefs.nudges("COROLLA");
        nudges[CarView::Cockpit.index()] = Vec3::new(0.0, 0.04, -0.1);
        prefs.set_nudges("COROLLA", nudges);
        let mut chase = prefs.nudges("rsf-24");
        chase[CarView::Chase.index()] = Vec3::new(0.0, 0.5, -1.0);
        prefs.set_nudges("rsf-24", chase);
        let text = prefs.to_text();
        assert_eq!(text, "[camera]\nview = cockpit\nCOROLLA cockpit = 0 0.04 -0.1\nrsf-24 chase = 0 0.5 -1\n");
        assert_eq!(CameraPrefs::parse(&text), prefs);
        // Replaced in place of the old section, the other sections kept.
        let file = "[launcher]\ncar = COROLLA\n\n[camera]\nview = chase\n\n[controls]\nbrake = KeyB\n";
        let replaced = prefs.replace_in(file);
        assert_eq!(replaced, format!("[launcher]\ncar = COROLLA\n\n[controls]\nbrake = KeyB\n\n{text}"));
        assert_eq!(CameraPrefs::parse(&replaced), prefs);
        assert_eq!(rbr_formats::ini::parse(&replaced).unwrap().len(), 3);
        // Zeros drop the car; bad lines are skipped.
        prefs.set_nudges("COROLLA", [Vec3::ZERO; 4]);
        assert!(!prefs.cars.contains_key("COROLLA"));
        let bad = CameraPrefs::parse("[camera]\nview = roof\nX cockpit = 1 2\nY roof = 1 2 3\n");
        assert_eq!(bad, CameraPrefs::default());
    }

    #[test]
    fn the_adjust_mode_nudges_the_view_and_writes_the_file_when_it_ends() {
        let path = std::env::temp_dir().join(format!("camera-prefs-{}.ini", std::process::id()));
        std::fs::write(&path, "[launcher]\ncar = COROLLA\n").unwrap();
        let mut session = Session::load(Some(&path), Some("COROLLA".into()));
        assert_eq!(session.start_view(None), CarView::Chase);
        assert_eq!(session.key(KeyCode::ArrowUp, CarView::Cockpit), Handled::No, "not adjusting");
        session.toggle_adjust();
        let line = session.hud_line(CarView::Cockpit, "X").unwrap();
        assert!(session.adjusting && line.ends_with("X or Esc done"), "{line}");
        assert_eq!(session.key(KeyCode::ArrowUp, CarView::Cockpit), Handled::Moved);
        assert_eq!(session.key(KeyCode::PageUp, CarView::Cockpit), Handled::Moved);
        assert_eq!(session.key(KeyCode::PageUp, CarView::Chase), Handled::Moved);
        assert_eq!(session.key(KeyCode::KeyQ, CarView::Cockpit), Handled::No);
        let nudges = session.nudges();
        assert!((nudges[CarView::Cockpit.index()] - Vec3::new(0.0, STEP, STEP)).length() < 1e-6);
        assert!((nudges[CarView::Chase.index()] - Vec3::new(0.0, CHASE_STEP, 0.0)).length() < 1e-6);
        assert_eq!(session.key(KeyCode::Escape, CarView::Cockpit), Handled::Taken);
        assert!(!session.adjusting && session.hud_line(CarView::Cockpit, "X").is_none());
        session.chose(CarView::Bumper);
        let again = Session::load(Some(&path), Some("COROLLA".into()));
        assert_eq!((again.start_view(None), again.nudges()), (CarView::Bumper, nudges));
        assert_eq!(again.start_view(Some(CarView::Chase)), CarView::Chase);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("[launcher]\ncar = COROLLA\n\n[camera]\n"));
        // Backspace puts the view back; another car has none.
        session.toggle_adjust();
        session.key(KeyCode::Backspace, CarView::Cockpit);
        assert_eq!(session.nudges()[CarView::Cockpit.index()], Vec3::ZERO);
        assert_eq!(Session::load(Some(&path), Some("XSARA".into())).nudges(), [Vec3::ZERO; 4]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn nudges_stay_near_the_car_but_the_chase_moves_freely() {
        assert_eq!(car_key("corolla", None), "COROLLA");
        assert_eq!(car_key("evo_IX", Some(24)), "rsf-24");
        assert_eq!(nudge(CarView::Bonnet, Vec3::new(0.99, 0.0, 0.0), Vec3::X * 0.02), Vec3::new(1.0, 0.0, 0.0));
        // The chase has no sideways nudge; its distance is held by its settings.
        assert_eq!(nudge(CarView::Chase, Vec3::ZERO, Vec3::new(0.1, 0.1, -3.0)), Vec3::new(0.0, 0.1, -3.0));
    }
}

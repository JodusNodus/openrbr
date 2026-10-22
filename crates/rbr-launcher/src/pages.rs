//! The window's frame: the look, the tabs, the status line and the smaller pages.

use std::path::Path;
use std::time::Duration;

use egui::{Color32, Id, RichText};
use rbr_sound::menu::MenuPlayer;

use crate::catalog::{self, car_label};
use crate::settings::or_folder;
use crate::settings_page::RSF_FIELD;
use rbr_gameplay::season as season_model;

use crate::{rally_page, Form};

/// How often a running viewer is checked for its exit.
const POLL: Duration = Duration::from_millis(250);

/// The launcher's pages, by what the player wants to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// The quick race: car, stage, conditions, the race options and Start.
    Drive,
    Championship,
    /// The car setup.
    Setup,
    Controls,
    /// The window, the camera, the HUD, the pace notes, the sound and the renderer's additions.
    Graphics,
    /// The install folder, the advanced paths and the programs.
    Settings,
}

impl Page {
    /// The tabs, in order.
    const TABS: [(Page, &'static str); 6] = [
        (Page::Drive, "Drive"),
        (Page::Championship, "Championship"),
        (Page::Setup, "Car setup"),
        (Page::Controls, "Controls"),
        (Page::Graphics, "Graphics & sound"),
        (Page::Settings, "Settings"),
    ];
}

/// The width of the Drive and Graphics & sound pages' field column, and of a field.
pub const DRIVE_FIELDS: f32 = 520.0;
pub const FIELD: f32 = 340.0;
/// The width of the fields' labels, the same on every section.
const LABEL: f32 = 130.0;
/// What needs attention, what is fine, and the Start button.
pub const WARN: Color32 = Color32::from_rgb(230, 180, 60);
pub const GOOD: Color32 = Color32::from_rgb(110, 190, 110);
pub const ACCENT: Color32 = Color32::from_rgb(40, 120, 70);

/// The launcher's look: readable sizes and roomier spacing than egui's defaults.
fn theme(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        use egui::{FontId, TextStyle};
        for (text, size) in [
            (TextStyle::Body, 14.5),
            (TextStyle::Button, 14.5),
            (TextStyle::Monospace, 13.5),
            (TextStyle::Small, 11.5),
            (TextStyle::Heading, 20.0),
        ] {
            let family =
                if text == TextStyle::Monospace { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
            style.text_styles.insert(text, FontId::new(size, family));
        }
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size.y = 24.0;
    });
}

/// A section's heading.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(16.5).strong());
    ui.add_space(2.0);
}

/// A section's fields: a label and its field a row, the labels lined up across sections.
pub fn fields<R>(ui: &mut egui::Ui, id: &str, rows: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing([16.0, 8.0]).min_col_width(LABEL).show(ui, rows).inner
}

/// Why Start cannot run, and what fixes it.
#[derive(Clone, PartialEq)]
pub struct Issue {
    pub text: String,
    pub fix: Option<Fix>,
}

/// A status line's fix button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fix {
    /// The Settings page, for the game folder and the paths.
    Settings,
    /// "Add cars…" (`car_packs`).
    AddCars,
}

impl Fix {
    pub fn label(self) -> &'static str {
        match self {
            Fix::Settings => "Open Settings",
            Fix::AddCars => "Add cars…",
        }
    }
}

/// What the launcher says without NGP's archive.
pub const NO_NGP: &str = "This game folder has no NGP. Install NGP 7 into it (RallySimFans includes it), or choose \
                          a game folder that has it.";
/// What the launcher says without a car NGP drives.
pub const NO_CARS: &str = "No cars yet. Add cars from gvrc.de's NGP car packs, or choose a RallySimFans game folder.";
/// What the player sees when the viewer or `rbr-standalone` is missing; the paths go to the log
/// ([`Form::log_missing_programs`]).
const INCOMPLETE: &str = "ReRBR is incomplete: its driving window or physics program is missing. Reinstall ReRBR.";

impl Form {
    /// What the game lacks to drive: NGP's archive ([`NO_NGP`]), or a car NGP drives ([`NO_CARS`]).
    pub fn no_ngp(&self) -> Option<String> {
        if self.ngp_revision.is_none() {
            return Some(NO_NGP.to_owned());
        }
        self.catalog.cars.is_empty().then(|| NO_CARS.to_owned())
    }

    /// The viewer and `rbr-standalone`, when missing.
    fn missing_programs(&self) -> impl Iterator<Item = &Path> {
        [&self.paths.viewer, &self.paths.standalone].into_iter().map(|p| p.as_path()).filter(|p| !p.is_file())
    }

    /// Says on stderr which programs are missing and how a checkout builds them.
    pub fn log_missing_programs(&self) {
        for path in self.missing_programs() {
            eprintln!(
                "launcher: {} is missing: cargo build --release -p rbr-render -p rbr-standalone (or --viewer, \
                 --standalone)",
                path.display()
            );
        }
    }

    /// What the install lacks, on every page's status line: a missing program, NGP's archive or a car NGP drives;
    /// with the fix when there is one.
    pub fn readiness(&self) -> Option<Issue> {
        if self.missing_programs().next().is_some() {
            return Some(Issue { text: INCOMPLETE.into(), fix: None });
        }
        let fix = if self.ngp_revision.is_none() { Fix::Settings } else { Fix::AddCars };
        self.no_ngp().map(|text| Issue { text, fix: Some(fix) })
    }

    /// The page a fresh start opens on: Settings while something is missing, else Drive.
    pub fn first_page(&self) -> Page {
        if self.readiness().is_some() {
            Page::Settings
        } else {
            Page::Drive
        }
    }

    /// What blocks Start: a running viewer, [`Form::readiness`], a car that cannot start, or the selection.
    pub fn blocker(&self) -> Option<Issue> {
        if self.child.is_some() {
            return Some(Issue { text: "the viewer is running".into(), fix: None });
        }
        if let Some(issue) = self.readiness() {
            return Some(issue);
        }
        let car = self.car()?;
        if let Some(reason) = car.unavailable() {
            return Some(Issue { text: format!("{}: {reason}", car.name), fix: None });
        }
        self.selection().err().map(|text| Issue { text, fix: None })
    }

    pub fn apply(&mut self, fix: Fix) {
        match fix {
            Fix::Settings => self.page = Page::Settings,
            Fix::AddCars => {
                // To a page that shows the progress.
                if matches!(self.page, Page::Controls | Page::Graphics) {
                    self.page = Page::Settings;
                }
                self.add_cars();
            }
        }
    }

    /// The menus' sound for the page shown: the service park at a championship's service stop, while the viewer is
    /// not running.
    pub fn menu_sound(&mut self) {
        let park =
            self.child.is_none() && self.rally.in_service() && matches!(self.page, Page::Championship | Page::Setup);
        if !park && self.menu_sound.is_none() {
            return;
        }
        let player = self.menu_sound.get_or_insert_with(|| {
            MenuPlayer::start(&self.data, self.volume).map_err(|e| eprintln!("launcher: no menu sound: {e}")).ok()
        });
        if let Some(player) = player {
            player.set_volume(self.volume);
            player.set_park(park);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        theme(ctx);
        self.poll();
        let (bindings, ffb) = (self.bindings.clone(), self.ffb.clone());
        self.controls.poll(&mut self.bindings);
        // The first start with a gamepad and nothing bound to it: the Controls page offers its set-up, once the game
        // is ready to drive (Settings says what it lacks first).
        if self.child.is_none() && self.readiness().is_none() && self.controls.offer(&self.bindings) {
            self.page = Page::Controls;
        }
        if self.controls.capturing() {
            ctx.request_repaint();
        } else if self.controls.has_pads() {
            // The live bars, and the gamepad events taken while the viewer is driven with it.
            ctx.request_repaint_after(if self.page == Page::Controls { Duration::from_millis(30) } else { POLL });
        }
        if self.child.is_some() {
            ctx.request_repaint_after(POLL);
        }
        let old_selection =
            (self.map, self.car, self.options, self.wear, self.tyre, self.rally.options, self.rally.team);
        let old_setup = self.setup.name.clone();
        let old_ghost = self.ghost.clone();
        let old_graphics = (
            self.graphics,
            self.render_scale,
            self.anti_aliasing,
            self.present,
            self.mirrors,
            self.fullscreen,
            self.start_time,
        );
        let old_paths = (self.ngp_physics.clone(), self.rsf_root.clone());
        let old_head_gasket = self.head_gasket;
        let old_rsf = or_folder(Some(&self.rsf_scanned), &self.folder.rsf_root);
        let old_season = self.season_cars.clone();
        let old_scanned = self.rsf_scanned.clone();
        let old_folder = self.install_folder.clone();
        let old_recent = (self.recent_cars.clone(), self.recent_maps.clone());
        let old_codriver = (
            self.codriver,
            self.codriver_voice.clone(),
            self.codriver_style.clone(),
            self.codriver_ranges.clone(),
            self.codriver_pack.clone(),
            self.notes_pack.clone(),
            self.my_pacenotes,
        );
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("ReRBR").size(20.0).strong());
                ui.add_space(20.0);
                for (page, name) in Page::TABS {
                    let tab = egui::Button::selectable(self.page == page, RichText::new(name).size(15.5));
                    if ui.add(tab.min_size(egui::vec2(0.0, 28.0))).clicked() {
                        self.page = page;
                    }
                }
            });
            ui.add_space(4.0);
        });
        let readiness = self.readiness();
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| match &readiness {
                Some(issue) => {
                    ui.colored_label(WARN, format!("⚠ {}", issue.text));
                    // Not the button to the page shown.
                    if let Some(fix) = issue.fix.filter(|f| !(*f == Fix::Settings && self.page == Page::Settings)) {
                        if ui.button(fix.label()).clicked() {
                            self.apply(fix);
                        }
                    }
                }
                None => {
                    let text = if self.status.is_empty() { "Ready" } else { &self.status };
                    ui.colored_label(Color32::GRAY, text);
                }
            });
            ui.add_space(2.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.page {
            Page::Drive => self.drive_page(ui),
            Page::Championship => self.championship_page(ui),
            Page::Setup => self.setup_page(ui),
            Page::Controls => {
                let rsf = self.rsf();
                if self.controls.ui(ui, &mut self.bindings, &mut self.ffb, &rsf) {
                    self.import_rsf();
                }
            }
            Page::Graphics => self.graphics_page(ui),
            Page::Settings => {
                egui::ScrollArea::vertical().id_salt("settings").show(ui, |ui| self.settings_page(ui));
            }
        });
        // A page changed what the status line shows: drawn again with it.
        if self.readiness() != readiness {
            ctx.request_repaint();
        }
        if old_codriver.4 != self.codriver_pack {
            // Another voice pack: its voices, styles and range sets, with the choices aligned to what it
            // offers (before the save below, so the aligned choices persist).
            self.rescan_codrivers();
            self.normalize_codriver();
        }
        // The Stage notes row caches its text (its inputs: the note pack, the checkbox, the co-driver switch
        // and the voice): mark it dirty and repaint, or a switch keeps the old file until the map changes.
        self.invalidate_notes_cache(
            ctx,
            (old_codriver.5.clone(), old_codriver.6, old_codriver.0, old_codriver.1.clone()),
        );
        if self.bindings != bindings
            || self.ffb != ffb
            || old_selection
                != (self.map, self.car, self.options, self.wear, self.tyre, self.rally.options, self.rally.team)
            || old_setup != self.setup.name
            || old_ghost != self.ghost
            || old_paths != (self.ngp_physics.clone(), self.rsf_root.clone())
            || old_head_gasket != self.head_gasket
            || old_season != self.season_cars
            || old_graphics
                != (
                    self.graphics,
                    self.render_scale,
                    self.anti_aliasing,
                    self.present,
                    self.mirrors,
                    self.fullscreen,
                    self.start_time,
                )
            || old_folder != self.install_folder
            || old_recent != (self.recent_cars.clone(), self.recent_maps.clone())
            || old_codriver
                != (
                    self.codriver,
                    self.codriver_voice.clone(),
                    self.codriver_style.clone(),
                    self.codriver_ranges.clone(),
                    self.codriver_pack.clone(),
                    self.notes_pack.clone(),
                    self.my_pacenotes,
                )
        {
            self.save();
        }
        if self.rsf_root != self.rsf_scanned && !ctx.memory(|m| m.has_focus(Id::new(RSF_FIELD))) {
            self.rescan_rsf();
        }
        if (&old_paths.0, &old_scanned) != (&self.ngp_physics, &self.rsf_scanned) {
            self.list_cars();
        }
        // An RSF folder set where there was none: its settings are offered for import, once.
        if old_rsf.trim().is_empty() && !or_folder(Some(&self.rsf_scanned), &self.folder.rsf_root).trim().is_empty() {
            self.controls.import_offer = true;
        }
        if self.controls.offer_ui(ctx) {
            self.import_rsf();
            self.page = Page::Controls;
        }
    }

    /// "Import from RSF" (`crate::rsf_import`) into the page's settings, saved as any change is.
    fn import_rsf(&mut self) {
        let root = self.rsf();
        let reg = crate::rsf_import::user_reg(self.controls.rsf_prefix.trim());
        let user_reg = std::fs::read(&reg).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
        let cars: Vec<u32> =
            self.catalog.all_cars.iter().filter_map(|c| c.rsf.as_ref().filter(|r| !r.added)).map(|r| r.id).collect();
        let to = crate::rsf_import::Target {
            bindings: &mut self.bindings,
            ffb: &mut self.ffb,
            aids: &mut self.options.aids,
            head_gasket: &mut self.head_gasket,
        };
        let lines = crate::rsf_import::import(
            Path::new(root.trim()),
            user_reg.as_deref(),
            &cars,
            &self.controls.connected(),
            to,
        );
        lines.iter().for_each(|l| eprintln!("launcher: imported from RSF: {l}"));
        self.controls.import_status = Some(Ok(lines));
        if user_reg.is_none() {
            eprintln!("launcher: {}: not read", reg.display());
        }
    }

    /// The recent picks still in the catalog: the cars' and the maps' indices.
    pub fn recent(&self) -> (Vec<usize>, Vec<usize>) {
        let (cars, maps) = (&self.catalog.cars, &self.catalog.maps);
        (
            self.recent_cars.iter().filter_map(|name| cars.iter().position(|c| c.name == *name)).collect(),
            self.recent_maps.iter().filter_map(|&n| maps.iter().position(|m| m.number == n)).collect(),
        )
    }

    /// The Drive page with the car or the map list open on the selection (`--screenshot`).
    pub fn screenshot_list(&mut self, ctx: &egui::Context, maps: bool) {
        let (cars_recent, maps_recent) = self.recent();
        if maps && !self.map_picker.open {
            self.map_picker.open_on(self.map, &maps_recent);
        }
        if !maps && !self.car_picker.open {
            self.car_picker.open_on(self.car, &cars_recent);
        }
        self.map_picker.open &= maps;
        self.car_picker.open &= !maps;
        self.ui(ctx);
    }

    /// The offered car of team `team` (its car `0x1660808`, `SetupAI`'s table), when NGP drives it.
    pub(crate) fn team_car(&self, team: u32) -> Option<usize> {
        let index = *season_model::TEAM_CARS.get(team as usize)?;
        self.catalog.cars.iter().position(|c| c.rsf.is_none() && c.index == index)
    }

    /// The Championship page (`rally_page`). The season drives its team's car when NGP drives it, else the car picked
    /// for it (ours: kept per profile in `[season]`; the game's season always drives the team's car). A season with
    /// neither asks for one.
    fn championship_page(&mut self, ui: &mut egui::Ui) {
        if self.no_cars_yet(ui) {
            return;
        }
        let profile = self.rally.profile.clone();
        if self.rally.take_new_season() {
            match self.catalog.cars.get(self.season_pick).filter(|_| self.team_car(self.rally.team).is_none()) {
                Some(car) => self.season_cars.set(&profile, &car.name.clone()),
                None => self.season_cars.remove(&profile),
            }
        }
        let cars = &self.catalog.cars;
        let picked = self.season_cars.of(&profile).and_then(|name| cars.iter().position(|c| c.name == name));
        let season_car = self.rally.season_team().map(|team| self.team_car(team).or(picked));
        let (recent, _) = self.recent();
        let tip = "The Drive page's driver aids, and the Graphics & sound options, apply to the season too";
        ui.colored_label(Color32::GRAY, tip);
        ui.add_space(4.0);
        if season_car == Some(None) {
            ui.label(format!("NGP doesn't drive the team's car of {profile}'s season. Choose the car it drives:"));
            ui.horizontal(|ui| {
                self.season_picker.ui(ui, &mut self.season_pick, &recent);
                let car = self.catalog.cars.get(self.season_pick).filter(|c| c.unavailable().is_none());
                if ui.add_enabled(car.is_some(), egui::Button::new("Drive the season with it")).clicked() {
                    if let Some(car) = car {
                        self.season_cars.set(&profile, &car.name);
                    }
                }
            });
            return;
        }
        if let Some(Some(i)) = season_car {
            self.car = i;
        }
        let here = self.here();
        let archive = self.ngp_archive();
        let (slot, car_name) = self.car().map_or((0, String::new()), |c| (c.index, c.name.clone()));
        let team_cars: Vec<Option<String>> =
            (0..7).map(|t| self.team_car(t).map(|i| self.catalog.cars[i].name.clone())).collect();
        let (picker, pick, cars) = (&mut self.season_picker, &mut self.season_pick, &self.catalog.cars);
        // The new season's car: the team's when NGP drives it, else the list.
        let mut car_row = |ui: &mut egui::Ui, team: u32| match team_cars.get(team as usize).cloned().flatten() {
            Some(name) => {
                ui.label(format!("{name} (the team's)"));
                true
            }
            None => {
                picker.ui(ui, pick, &recent);
                cars.get(*pick).is_some_and(|c| c.unavailable().is_none())
            }
        };
        let request = egui::ScrollArea::vertical()
            .id_salt("championship")
            .show(ui, |ui| {
                let (data, season) = (&self.data, &self.season);
                self.rally.ui(ui, data, season, &here, slot, &car_name, Path::new(&archive), &mut car_row)
            })
            .inner;
        match request {
            Some(rally_page::Request::Launch(start)) => self.start_rally_stage(start),
            Some(rally_page::Request::Setup) => self.page = Page::Setup,
            None => {}
        }
        if let Some(text) = self.rally.take_saved_setup() {
            // The continued season's setup (`PTSC`) becomes the current one.
            self.select_setup();
            if let Err(e) = self.setup.load_text(&text) {
                self.status = format!("the season's setup: {e}");
            }
        }
    }

    /// A page that needs a car, without any: what to do, and "Add cars…". Whether there are none.
    fn no_cars_yet(&mut self, ui: &mut egui::Ui) -> bool {
        if !self.catalog.cars.is_empty() {
            return false;
        }
        ui.add_space(8.0);
        ui.colored_label(WARN, NO_CARS);
        self.add_cars_ui(ui);
        true
    }

    /// The Car setup page (`setup_page`), for the selected car on the stage's surface.
    fn setup_page(&mut self, ui: &mut egui::Ui) {
        if self.no_cars_yet(ui) {
            return;
        }
        self.select_setup();
        let car = self.car().map_or_else(|| "no car".to_owned(), car_label);
        ui.label(
            RichText::new(format!("{car} · {}", catalog::map_label(&self.catalog.maps[self.map]))).size(16.0).strong(),
        );
        ui.colored_label(Color32::GRAY, "Used at the next Start. Change the car or the stage on the Drive page.");
        ui.add_space(4.0);
        let here = self.here();
        self.setup.ui(ui, &self.data, &here);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::settings::{load, Settings};
    use crate::{Start, SETTINGS_FILE};

    /// The first start opens on Settings while NGP, a car or a program is missing, with the fix for it, and on Drive
    /// once everything is there (`RBR_DATA`, `RBR_PATCH`, `RBR_NGP_PHYSICS`: the test archive drives retail slots).
    #[test]
    fn the_first_start_opens_on_what_needs_attention() {
        let env = |key| std::env::var(key).ok().filter(|v| !v.is_empty());
        let (Some(data), Some(patch), Some(ngp)) = (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS")) else {
            eprintln!("skipping: RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS not set");
            return;
        };
        let here = std::env::temp_dir().join(format!("rbr-launcher-first-page-{}", std::process::id()));
        std::fs::create_dir_all(&here).unwrap();
        let program = std::env::current_exe().unwrap();
        let form = |archive: &str, programs: bool| {
            let start = Start {
                viewer: if programs { program.clone() } else { here.join("viewer") },
                standalone: program.clone(),
                settings: here.join(SETTINGS_FILE),
                data: Some(PathBuf::from(&data)),
                patch: Some(PathBuf::from(&patch)),
            };
            // No RallySimFans cars: an empty RSF folder.
            let text = format!("[launcher]\nngp_physics = {archive}\nrsf_root = {}\n", here.display());
            load(&start, Settings::parse(&text)).unwrap()
        };
        let retail = format!("{data}/Root/physics.rbz");
        let page = |form: &Form| (form.first_page(), form.readiness().map(|i| (i.text, i.fix)));
        assert_eq!(page(&form(&ngp, true)), (Page::Drive, None));
        let no_ngp = (Page::Settings, Some((NO_NGP.to_owned(), Some(Fix::Settings))));
        assert_eq!(page(&form(&retail, true)), no_ngp);
        assert_eq!(page(&form(&ngp, false)), (Page::Settings, Some((INCOMPLETE.to_owned(), None))));
        let mut no_cars = form(&ngp, true);
        no_cars.catalog.cars.clear();
        assert_eq!(page(&no_cars), (Page::Settings, Some((NO_CARS.to_owned(), Some(Fix::AddCars)))));
        std::fs::remove_dir_all(&here).unwrap();
    }
}

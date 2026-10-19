//! The window's frame: the look, the tabs, the status line and the smaller pages.

use std::path::Path;
use std::time::Duration;

use egui::{Color32, Id, RichText};
use rbr_sound::menu::MenuPlayer;

use crate::catalog::{self, car_label};
use crate::settings_page::RSF_FIELD;
use rbr_gameplay::season as season_model;

use crate::{rally_page, Form};

/// How often a running viewer is checked for its exit.
const POLL: Duration = Duration::from_millis(250);

/// The launcher's pages, by what the player wants to do.
#[derive(Clone, Copy, PartialEq, Eq)]
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
#[derive(Clone, Copy, PartialEq)]
pub enum Fix {
    /// The Settings page, for the install folder and the paths.
    Settings,
}

impl Fix {
    pub fn label(self) -> &'static str {
        match self {
            Fix::Settings => "Open Settings",
        }
    }
}

/// What the launcher says without NGP's archive.
pub const NO_NGP: &str =
    "This install has no NGP physics.rbz. Install NGP (or RallySimFans, which includes it), or set \
                          NGP physics.rbz under Advanced.";

impl Form {
    /// What the game lacks to drive: NGP's archive ([`NO_NGP`]), or a car NGP drives.
    pub fn no_ngp(&self) -> Option<String> {
        if self.ngp_revision.is_none() {
            return Some(NO_NGP.to_owned());
        }
        let text =
            "NGP drives none of the cars: its physics.rbz has no NGP cars in the retail slots. RallySimFans brings \
                    NGP cars (the RSF folder under Advanced)";
        self.catalog.cars.is_empty().then(|| text.to_owned())
    }

    /// What the install lacks, on every page's status line: a missing program, NGP's archive or a car NGP drives;
    /// with the fix when there is one.
    fn readiness(&self) -> Option<Issue> {
        for path in [&self.paths.viewer, &self.paths.standalone] {
            if !path.is_file() {
                let text = format!(
                    "{} is missing: cargo build --release -p rbr-render -p rbr-standalone (or --viewer, --standalone)",
                    path.display()
                );
                return Some(Issue { text, fix: None });
            }
        }
        self.no_ngp().map(|text| Issue { text, fix: Some(Fix::Settings) })
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
        // The first start with a gamepad and nothing bound to it: the Controls page offers its set-up.
        if self.child.is_none() && self.controls.offer(&self.bindings) {
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
        let old_graphics = (self.graphics, self.render_scale, self.present, self.fullscreen, self.start_time);
        let old_paths = (self.ngp_physics.clone(), self.rsf_root.clone());
        let old_season = self.season_cars.clone();
        let old_scanned = self.rsf_scanned.clone();
        let old_folder = self.install_folder.clone();
        let old_recent = (self.recent_cars.clone(), self.recent_maps.clone());
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
                    if let Some(fix) = issue.fix {
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
            Page::Drive => self.drive_page(ui, readiness.is_some()),
            Page::Championship => self.championship_page(ui),
            Page::Setup => self.setup_page(ui),
            Page::Controls => self.controls.ui(ui, &mut self.bindings, &mut self.ffb),
            Page::Graphics => self.graphics_page(ui),
            Page::Settings => self.settings_page(ui),
        });
        // A page changed what the status line shows: drawn again with it.
        if self.readiness() != readiness {
            ctx.request_repaint();
        }
        if self.bindings != bindings
            || self.ffb != ffb
            || old_selection
                != (self.map, self.car, self.options, self.wear, self.tyre, self.rally.options, self.rally.team)
            || old_setup != self.setup.name
            || old_ghost != self.ghost
            || old_paths != (self.ngp_physics.clone(), self.rsf_root.clone())
            || old_season != self.season_cars
            || old_graphics != (self.graphics, self.render_scale, self.present, self.fullscreen, self.start_time)
            || old_folder != self.install_folder
            || old_recent != (self.recent_cars.clone(), self.recent_maps.clone())
        {
            self.save();
        }
        if self.rsf_root != self.rsf_scanned && !ctx.memory(|m| m.has_focus(Id::new(RSF_FIELD))) {
            self.rescan_rsf();
        }
        if (&old_paths.0, &old_scanned) != (&self.ngp_physics, &self.rsf_scanned) {
            self.list_cars();
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

    /// The Car setup page (`setup_page`), for the selected car on the stage's surface.
    fn setup_page(&mut self, ui: &mut egui::Ui) {
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

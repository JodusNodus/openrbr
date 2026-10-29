//! The Settings page: what the player needs to drive, the game folder, the cars, the co-driver, the advanced paths and
//! the programs.

use std::path::{Path, PathBuf};

use egui::{Color32, Id};

use crate::disc_install::DiscSetup;
use crate::pages::{fields, section, Page, GOOD, NO_NGP, WARN};
use crate::{controls_page, install, Form};

/// The RSF field's egui id: its folder is scanned when it loses focus.
pub const RSF_FIELD: &str = "rsf_root";

impl Form {
    /// What the player needs to drive, a row each with its next step beside it: the game folder, NGP, the cars and the
    /// wheel; below them the co-driver, the advanced fields (NGP's DLL, the paths that override the folder's parts) and
    /// the version.
    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        let running = self.child.is_some();
        section(ui, "Ready to drive");

        // The game folder, and the disc setup under it.
        let mut set = false;
        fields(ui, "row game", |ui| {
            tick(ui, "Game folder", true, |ui| {
                ui.add_enabled_ui(!running, |ui| {
                    let hint = "the Richard Burns Rally folder, with RichardBurnsRally_SSE.exe";
                    let edit = ui
                        .add(egui::TextEdit::singleline(&mut self.install_folder).hint_text(hint).desired_width(340.0));
                    let picked = ui.button("Browse…").clicked()
                        && install::pick_folder("Your Richard Burns Rally folder")
                            .map(|d| self.install_folder = d)
                            .is_some();
                    self.disc_setup.open |= ui.button("Set up from the disc…").clicked();
                    set = picked || edit.lost_focus();
                });
            })
        });
        if let Some(dir) = ui.add_enabled_ui(!running, |ui| self.disc_setup.ui(ui)).inner {
            self.install_folder = dir;
            set = true;
        }
        if set && install::Folder::read(&self.install_folder) != self.folder {
            self.reload = true;
        }
        let rsf = self.rsf();
        if Path::new(rsf.trim()).join("rsfdata").is_dir() {
            fields(ui, "row rsf", |ui| {
                tick(ui, "RallySimFans", true, |ui| {
                    _ = ui.label(format!("its cars and stages are listed ({})", rsf.trim()))
                })
            });
        }

        // NGP: its archive and its DLL in one row, and the download's confirmation under it.
        let has_ngp = self.ngp_revision.is_some();
        let dll_problem = self.ngp_dll.problem(&self.data, &rsf);
        let ngp_ok = has_ngp && dll_problem.is_none();
        let can_download = self.can_download_ngp();
        let mut download = false;
        fields(ui, "row ngp", |ui| {
            tick(ui, "NGP", ngp_ok, |ui| match (has_ngp, &dll_problem) {
                (true, None) => {
                    ui.label(&self.ngp_label);
                    download = can_download && ui.button("Download NGP 7.4.778…").clicked();
                }
                (has, problem) => {
                    ui.label(match problem {
                        Some(problem) if has => problem.as_str(),
                        _ => NO_NGP,
                    });
                    download = can_download && ui.button("Download NGP…").clicked();
                }
            })
        });
        if download {
            self.ask_download_ngp();
        }
        self.ngp_download_ui(ui);

        // The cars: what drives, Download cars… for more, and under it the job, the list and the added cars.
        let cars = self.catalog.cars.iter().filter(|c| c.unavailable().is_none());
        let (mut from_rsf, mut added, mut from_ngp) = (0, 0, 0);
        for car in cars {
            match &car.rsf {
                Some(r) if r.added => added += 1,
                Some(_) => from_rsf += 1,
                None => from_ngp += 1,
            }
        }
        let count = [(from_rsf, "from RallySimFans"), (added, "added"), (from_ngp, "from NGP")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, from)| format!("{n} {from}"))
            .collect::<Vec<_>>();
        let has_cars = !count.is_empty();
        fields(ui, "row cars", |ui| {
            tick(ui, "Cars", has_cars, |ui| {
                ui.label(if has_cars { count.join(", ") } else { "None yet".to_owned() });
                self.download_cars_button(ui);
            })
        });
        if self.has_adding_output() {
            ui.horizontal_wrapped(|ui| self.adding_job_ui(ui));
            self.adding_status_ui(ui);
        }
        self.download_cars_panel(ui);
        if !self.adding.cars.is_empty() {
            let title = format!("Added cars ({})", self.adding.cars.len());
            egui::CollapsingHeader::new(title).id_salt("added cars").show(ui, |ui| self.added_cars_ui(ui));
        }

        // The wheel.
        let pads = self.controls.has_pads();
        let set_up = !controls_page::unbound(&self.bindings);
        fields(ui, "row wheel", |ui| {
            tick(ui, "Wheel", !pads || set_up, |ui| match (pads, set_up) {
                (false, _) => _ = ui.label("None connected: the keyboard drives (its keys are on Controls)"),
                (true, true) => _ = ui.label("Bound"),
                (true, false) => {
                    ui.label("Connected, not bound yet");
                    if ui.button("Bind it on Controls").clicked() {
                        self.page = Page::Controls;
                    }
                }
            })
        });

        // The co-driver: a general choice, the same for every stage.
        self.codriver_section(ui);

        ui.add_space(6.0);
        egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
            self.ngp_dll.ui(ui, &self.data, &rsf, !running);
            ui.add_space(4.0);
            let hint = |path: &Option<PathBuf>, none: &str| path.as_ref().map_or(none.to_owned(), |p| p.display().to_string());
            fields(ui, "overrides", |ui| {
                ui.label("NGP physics.rbz");
                let ngp = hint(&self.folder.ngp_physics, "NGP's physics.rbz");
                ui.add(egui::TextEdit::singleline(&mut self.ngp_physics).hint_text(ngp).desired_width(420.0));
                ui.end_row();
                ui.label("RSF folder");
                let rsf = hint(&self.folder.rsf_root, "a RallySimFans game folder");
                ui.add(egui::TextEdit::singleline(&mut self.rsf_root).id(Id::unique(RSF_FIELD)).hint_text(rsf).desired_width(420.0));
                ui.end_row();
                ui.label("Game data");
                ui.label(self.paths.data.display().to_string());
                ui.end_row();
                ui.label("1.02 patch files");
                ui.label(self.paths.patch.as_ref().map_or("none".into(), |p| p.display().to_string()));
                ui.end_row();
                for (label, path) in [("Viewer", &self.paths.viewer), ("Physics program", &self.paths.standalone)] {
                    ui.label(label);
                    ui.horizontal(|ui| {
                        let ok = path.is_file();
                        ui.colored_label(if ok { GOOD } else { WARN }, if ok { "✔" } else { "✖" });
                        ui.label(path.display().to_string());
                    });
                    ui.end_row();
                }
            });
            ui.colored_label(
                Color32::GRAY,
                "Empty fields take the game folder's. --data (or RBR_DATA) and RBR_PATCH set the game data and the 1.02 \
                 patch files. rerbr --help lists the options.",
            );
        });
        ui.add_space(8.0);
        ui.colored_label(
            Color32::GRAY,
            format!("ReRBR {} · your files: {}", env!("CARGO_PKG_VERSION"), self.here().display()),
        )
        .on_hover_text("Settings, saves, records, ghosts, setups and added cars");
    }
}

/// A row of what the player needs: its label, a tick or a cross, and `what` (what is there, or the next step).
fn tick(ui: &mut egui::Ui, label: &str, ok: bool, what: impl FnOnce(&mut egui::Ui)) {
    ui.label(label);
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(if ok { GOOD } else { WARN }, if ok { "✔" } else { "✖" });
        what(ui);
    });
    ui.end_row();
}

/// The game folder's field, its Browse button and the disc setup's; whether a folder was set (Enter, the field
/// left, picked, or set up from the disc).
pub fn folder_row(ui: &mut egui::Ui, folder: &mut String, setup: &mut DiscSetup) -> bool {
    ui.horizontal(|ui| {
        ui.label("Game folder");
        let hint = "the Richard Burns Rally folder, with RichardBurnsRally_SSE.exe";
        let edit = ui.add(egui::TextEdit::singleline(folder).hint_text(hint).desired_width(420.0));
        let picked = ui.button("Browse…").clicked()
            && install::pick_folder("Your Richard Burns Rally folder").map(|dir| *folder = dir).is_some();
        setup.open |= ui.button("Set up from the disc…").clicked();
        picked || edit.lost_focus()
    })
    .inner
        | setup.ui(ui).map(|dir| *folder = dir).is_some()
}

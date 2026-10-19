//! The Settings page: what the player needs to drive, the game folder, the cars, the advanced paths and the programs.

use std::path::{Path, PathBuf};

use egui::{Color32, Id, RichText};

use crate::disc_install::DiscSetup;
use crate::pages::{fields, section, Page, ACCENT, GOOD, NO_NGP, WARN};
use crate::{controls_page, install, Form};

/// The RSF field's egui id: its folder is scanned when it loses focus.
pub const RSF_FIELD: &str = "rsf_root";

impl Form {
    /// What the player needs to drive (the game folder, NGP and cars, each ticked or with its next step, and Drive
    /// once all are there), the game folder, the cars, the advanced fields that override the folder's parts, and the
    /// programs.
    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        let running = self.child.is_some();
        section(ui, "Ready to drive");
        let cars = self.catalog.cars.iter().filter(|c| c.unavailable().is_none());
        let (mut rsf, mut added, mut ngp) = (0, 0, 0);
        for car in cars {
            match &car.rsf {
                Some(r) if r.added => added += 1,
                Some(_) => rsf += 1,
                None => ngp += 1,
            }
        }
        let count = [(rsf, "from RallySimFans"), (added, "added"), (ngp, "from NGP")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, from)| format!("{n} {from}"))
            .collect::<Vec<_>>();
        let has_ngp = self.ngp_revision.is_some();
        let has_cars = !count.is_empty();
        let mut add = false;
        fields(ui, "ready", |ui| {
            tick(ui, "Game folder", true, |ui| _ = ui.label("Richard Burns Rally 1.02"));
            tick(ui, "NGP", has_ngp, |ui| match has_ngp {
                true => _ = ui.label(&self.ngp_label),
                false => _ = ui.label(NO_NGP),
            });
            tick(ui, "Cars", has_cars, |ui| match has_cars {
                true => _ = ui.label(count.join(", ")),
                false => {
                    ui.label("None yet: add gvrc.de's NGP car packs, or choose a RallySimFans game folder.");
                    add = ui.add_enabled(!running && has_ngp, egui::Button::new("Add cars…")).clicked();
                }
            });
            let pads = self.controls.has_pads();
            let set_up = !controls_page::unbound(&self.bindings);
            tick(ui, "Wheel", !pads || set_up, |ui| match (pads, set_up) {
                (false, _) => _ = ui.label("None connected: the keyboard drives (its keys are on Controls)"),
                (true, true) => _ = ui.label("Set up"),
                (true, false) => {
                    ui.label("Connected, not set up yet");
                    if ui.button("Set up your wheel").clicked() {
                        self.page = Page::Controls;
                        self.controls.start_setup();
                    }
                }
            });
        });
        if add {
            self.add_cars();
        }
        if has_ngp && has_cars && self.readiness().is_none() {
            ui.horizontal(|ui| {
                let text = RichText::new("Drive").size(16.0).strong().color(Color32::WHITE);
                if ui.add(egui::Button::new(text).fill(ACCENT).min_size(egui::vec2(120.0, 32.0))).clicked() {
                    self.page = Page::Drive;
                }
                ui.colored_label(Color32::GRAY, "All set: pick a car and a stage on the Drive page.");
            });
        }
        section(ui, "Game folder");
        let set =
            ui.add_enabled_ui(!running, |ui| folder_row(ui, &mut self.install_folder, &mut self.disc_setup)).inner;
        if set && install::Folder::read(&self.install_folder) != self.folder {
            self.reload = true;
        }
        ui.colored_label(
            Color32::GRAY,
            "Richard Burns Rally 1.02 with NGP, or a RallySimFans install, which has both and adds its cars and \
             stages. ReRBR only reads it.",
        );
        let rsf = self.rsf();
        let has_rsf = Path::new(rsf.trim()).join("rsfdata").is_dir();
        ui.horizontal(|ui| match has_rsf {
            true => {
                ui.colored_label(GOOD, "✔");
                ui.label(format!("RallySimFans: its cars and stages are listed ({})", rsf.trim()));
            }
            false => _ = ui.colored_label(Color32::GRAY, "RallySimFans: not in this folder (optional)"),
        });
        section(ui, "Cars");
        ui.colored_label(
            Color32::GRAY,
            "Download cars from gvrc.de/NGP.html: a car's \"NGP Physics\" .zip and its \"Model\" .7z, into one \
             folder. Then press Add cars… and choose that folder. RallySimFans is not needed.",
        );
        self.add_cars_ui(ui);
        self.added_cars_ui(ui);
        ui.add_space(6.0);
        egui::CollapsingHeader::new("Advanced").show(ui, |ui| {
            let hint = |path: &Option<PathBuf>, none: &str| path.as_ref().map_or(none.to_owned(), |p| p.display().to_string());
            fields(ui, "overrides", |ui| {
                ui.label("NGP physics.rbz");
                let ngp = hint(&self.folder.ngp_physics, "NGP's physics.rbz");
                ui.add(egui::TextEdit::singleline(&mut self.ngp_physics).hint_text(ngp).desired_width(420.0));
                ui.end_row();
                ui.label("RSF folder");
                let rsf = hint(&self.folder.rsf_root, "a RallySimFans game folder");
                ui.add(egui::TextEdit::singleline(&mut self.rsf_root).id(Id::new(RSF_FIELD)).hint_text(rsf).desired_width(420.0));
                ui.end_row();
                ui.label("Game data");
                ui.label(self.paths.data.display().to_string());
                ui.end_row();
                ui.label("1.02 patch files");
                ui.label(self.paths.patch.as_ref().map_or("none".into(), |p| p.display().to_string()));
                ui.end_row();
            });
            ui.colored_label(
                Color32::GRAY,
                "Empty fields take the game folder's. --data (or RBR_DATA) and RBR_PATCH set the game data and the 1.02 patch files.",
            );
        });
        section(ui, "About");
        fields(ui, "about", |ui| {
            ui.label("ReRBR");
            ui.label(format!("launcher version {}", env!("CARGO_PKG_VERSION")));
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
            ui.label("Settings file");
            ui.label(self.paths.settings.display().to_string());
            ui.end_row();
        });
        ui.colored_label(
            Color32::GRAY,
            "launcher --help lists the options: --list, --command, --screenshot and the paths.",
        );
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

//! The Settings page: the install folder, what it gives, the advanced paths and the programs.

use std::path::{Path, PathBuf};

use egui::{Color32, Id};

use crate::disc_install::DiscSetup;
use crate::pages::{fields, section, GOOD, NO_NGP, WARN};
use crate::{install, Form};

/// The RSF field's egui id: its folder is scanned when it loses focus.
pub const RSF_FIELD: &str = "rsf_root";

impl Form {
    /// The install folder, what it gives, the advanced fields that override its parts, and the programs.
    pub fn settings_page(&mut self, ui: &mut egui::Ui) {
        let running = self.child.is_some();
        section(ui, "Game");
        let set =
            ui.add_enabled_ui(!running, |ui| folder_row(ui, &mut self.install_folder, &mut self.disc_setup)).inner;
        if set && install::Folder::read(&self.install_folder) != self.folder {
            self.reload = true;
        }
        ui.colored_label(
            Color32::GRAY,
            "RBR 1.02 with NGP; a RallySimFans install has both, and the community cars and stages. It is only read, \
             never written.",
        );
        ui.add_space(6.0);
        let found = |path: Option<String>| path.unwrap_or_else(|| "not in the install folder".into());
        let has_ngp = self.ngp_revision.is_some();
        let ngp = match self.ngp_archive() {
            p if has_ngp => format!("{p} ({}, {} cars)", self.ngp_label, self.catalog.cars.len()),
            _ => NO_NGP.to_owned(),
        };
        let rsf = self.rsf();
        let has_rsf = Path::new(rsf.trim()).join("rsfdata").is_dir();
        let rsf = match has_rsf {
            true => format!("{} (adds the community cars and stages)", rsf.trim()),
            false => "none: a RallySimFans install adds the community cars and stages".to_owned(),
        };
        let rows = [
            ("Game data", Some(self.paths.data.display().to_string()), true),
            ("1.02 patch files", self.paths.patch.as_ref().map(|p| p.display().to_string()), true),
            ("NGP physics", Some(ngp), has_ngp),
            ("RallySimFans", Some(rsf), has_rsf),
        ];
        fields(ui, "found", |ui| {
            for (label, path, ok) in rows {
                ui.label(label);
                ui.horizontal(|ui| {
                    let ok = ok && path.is_some();
                    ui.colored_label(if ok { GOOD } else { WARN }, if ok { "✔" } else { "✖" });
                    ui.label(found(path));
                });
                ui.end_row();
            }
        });
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
            });
            ui.colored_label(
                Color32::GRAY,
                "Empty fields take the install folder's. --data (or RBR_DATA) and RBR_PATCH set the game data and the 1.02 patch files.",
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

/// The install folder's field, its Browse button and the disc setup's; whether a folder was set (Enter, the field
/// left, picked, or set up from the disc).
pub fn folder_row(ui: &mut egui::Ui, folder: &mut String, setup: &mut DiscSetup) -> bool {
    ui.horizontal(|ui| {
        ui.label("RBR install folder");
        let hint = "the game folder, with RichardBurnsRally_SSE.exe";
        let edit = ui.add(egui::TextEdit::singleline(folder).hint_text(hint).desired_width(420.0));
        let picked = ui.button("Browse…").clicked() && install::pick_folder().map(|dir| *folder = dir).is_some();
        setup.open |= ui.button("Set up from the disc…").clicked();
        picked || edit.lost_focus()
    })
    .inner
        | setup.ui(ui).map(|dir| *folder = dir).is_some()
}

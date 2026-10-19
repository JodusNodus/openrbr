//! The Graphics & sound page: the window, the camera, the HUD and pace notes, the sound and the renderer's additions.

use egui::{Color32, ComboBox, DragValue};
use rbr_render::camera_prefs;
use rbr_render::coords;
use rbr_render::mirrors::Rate;
use rbr_render::present::Present;
use rbr_render::renderer::{AntiAliasing, Enhance};

use crate::pages::{fields, section, DRIVE_FIELDS, FIELD};
use crate::Form;

/// The renderer additions on (`viewer --enhance`), by [`Enhance::NAMES`].
pub type Graphics = [bool; Enhance::NAMES.len()];

/// Each renderer addition's label and hover tip, by [`Enhance::NAMES`].
pub const GRAPHICS_LABELS: [(&str, &str); Enhance::NAMES.len()] = [
    ("Sharp distant textures", "16x anisotropic filtering: the road stays sharp further out at grazing angles"),
    (
        "Cabin light",
        "In the cockpit: the sun only through the glass, with the roof, pillars and dash shading each other, darker \
         footwells and corners, a sheen on the dash, and the headlights' glow at night",
    ),
    (
        "Moving clouds",
        "The game's own clouds drifting slowly and lit by the sun and the time of day, a moving layer overhead as \
         cloudy as they are, and its shadows on the stage in sunshine",
    ),
    (
        "Soft car shadow",
        "The car's own shape as a soft shadow on the ground and fences under it, in place of the game's shadow decal",
    ),
    (
        "HDR and bloom",
        "Renders in float colour with bloom, an exposure that adapts to the view (in the cockpit, mostly to the \
         outside) and a filmic tone curve that keeps bright skies and snow from clipping",
    ),
    ("Weather and particle effects", "GPU rain and snow around the car, rings on wet roads, sunlit and soft particles"),
    (
        "Aerial perspective",
        "A physically based sky and distance haze for the sun's height, through dusk, mist in the valleys at dawn, \
         dusk and in fog, sun shafts through the trees and clouds (with HDR), and the sky's light on the car and in \
         the cabin",
    ),
    ("Sun shadows", "Real-time sun shadows near the camera under clear skies, and sunlit foliage"),
    ("Flowing water", "Rivers and lakes that flow, ripple and fade into their banks"),
    ("Puddles on wet roads", "Standing water in the road's dips and a damp sheen after rain"),
];

/// The index of `name` in [`Enhance::NAMES`].
pub fn graphics_index(name: &str) -> usize {
    Enhance::NAMES.iter().position(|n| *n == name).expect("an Enhance name")
}

/// The additions on by default: all but the moving clouds, not yet reviewed in play, and the puddles, a small change
/// under the forest stages' dark reflections. The cabin light costs next to nothing.
pub const DEFAULT_GRAPHICS: Graphics = [true, true, false, true, true, true, true, true, true, false];

/// The render scales the Graphics & sound page offers, percent of the window's width and height (`viewer --render-scale`): the
/// frame is drawn smaller and the post pass scales it up, for high refresh rates at 1440p and above. 100, the first,
/// is the default.
pub const RENDER_SCALES: [u32; 4] = [100, 75, 67, 50];

/// A render scale's label.
fn scale_label(percent: u32) -> String {
    format!("{percent}%")
}

/// An anti-aliasing choice's label.
fn anti_aliasing_label(choice: AntiAliasing) -> &'static str {
    match choice {
        AntiAliasing::Msaa4 => "4x multisampling",
        AntiAliasing::Off => "Off",
    }
}

/// The mirrors' rate a new install starts with (`viewer --mirrors`): the most often updated costing at most 1 ms a frame
/// at 1440p on an M-series Mac (`lead/reports/mirrors.md`).
pub const DEFAULT_MIRRORS: Rate = Rate::Every(1);

/// A mirrors' rate's label.
fn mirrors_label(rate: Rate) -> &'static str {
    match rate {
        Rate::Off => "Off",
        Rate::Every(1) => "Every frame",
        Rate::Every(2) => "Every 2nd frame",
        Rate::Every(_) => "Every 3rd frame",
    }
}

/// Each frame presentation's label and hover tip (`viewer --present`), by [`Present::ALL`].
const PRESENT_LABELS: [(&str, &str); Present::ALL.len()] = [
    (
        "Fast, no tearing (recommended)",
        "Shows the newest frame at each refresh: no tearing, and no wait for the refresh (where the system has no such \
         mode, as on macOS, this is vsync)",
    ),
    ("Vsync", "Each frame waits for the display's refresh: a frame just too slow for one refresh waits for two"),
    (
        "Uncapped (may tear)",
        "Shows each frame as soon as it is drawn: the lowest lag, but it tears where the compositor allows tearing \
         (Hyprland with an immediate window rule); elsewhere it shows like the fast mode",
    ),
];

/// The HUD's options (`viewer --no-hud`, `--mph`, `--fps`, `--pace-stack`, `--pace-3d`), and the cockpit's working dials
/// (`--no-dials`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudChoice {
    pub show: bool,
    pub mph: bool,
    pub fps: bool,
    /// The pace stack's notes, 0 to 4 (the game's `PN_STACK`; 0, its default, hides it).
    pub stack: u8,
    /// The 3D pace notes (the game's `AID_MARKER`, off by default).
    pub pace_3d: bool,
    /// The working dials of the RSF cars with RBRHUD gauges in the install (on by default).
    pub dials: bool,
}

impl Default for HudChoice {
    fn default() -> Self {
        HudChoice { show: true, mph: false, fps: false, stack: 0, pace_3d: false, dials: true }
    }
}

/// The loudest sound volume the viewer takes (`viewer --volume`).
pub const MAX_VOLUME: f32 = 4.0;

impl Form {
    /// Whether HDR is on, which the render scale needs.
    pub fn hdr(&self) -> bool {
        self.graphics[graphics_index("hdr")]
    }

    /// The Graphics & sound page: the window, the camera, the HUD, the pace notes and the sound on the left; the
    /// renderer's additions on the right.
    pub fn graphics_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("graphics").show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(DRIVE_FIELDS);
                    self.view_fields(ui);
                });
                ui.add_space(20.0);
                ui.vertical(|ui| self.enhancements(ui));
            });
        });
    }

    fn view_fields(&mut self, ui: &mut egui::Ui) {
        let hdr = self.hdr();
        section(ui, "Display");
        fields(ui, "display", |ui| {
            let tip =
                "Fills the monitor at its own resolution, borderless: the compositor can show the frames directly";
            ui.label("Fullscreen").on_hover_text(tip);
            ui.checkbox(&mut self.fullscreen, "at the monitor's resolution").on_hover_text(tip);
            ui.end_row();

            ui.label("Resolution").on_hover_text("The driving window's size when not fullscreen");
            ui.add_enabled_ui(!self.fullscreen, |ui| {
                ComboBox::from_id_salt("size").width(FIELD).height(320.0).selected_text(size_label(self.size)).show_ui(
                    ui,
                    |ui| {
                        for &size in &self.sizes {
                            ui.selectable_value(&mut self.size, size, size_label(size));
                        }
                    },
                );
            });
            ui.end_row();

            let tip = "Draws the 3D view at this share of the window's size and scales it up: softer, but faster, for \
                       high refresh rates at 1440p and above. Needs HDR";
            ui.label("Render scale").on_hover_text(tip);
            ui.add_enabled_ui(hdr, |ui| {
                ComboBox::from_id_salt("render scale")
                    .selected_text(scale_label(if hdr { self.render_scale } else { 100 }))
                    .show_ui(ui, |ui| {
                        for percent in RENDER_SCALES {
                            ui.selectable_value(&mut self.render_scale, percent, scale_label(percent));
                        }
                    })
                    .response
                    .on_hover_text(tip)
                    .on_disabled_hover_text(tip);
            });
            ui.end_row();

            let tip = "4x multisampling smooths the edges, the foliage's and fences' cut-outs too. Off: one sample a \
                       pixel, the cut-outs as in 1.02: faster, with jagged edges";
            ui.label("Anti-aliasing").on_hover_text(tip);
            ComboBox::from_id_salt("anti-aliasing")
                .width(FIELD)
                .selected_text(anti_aliasing_label(self.anti_aliasing))
                .show_ui(ui, |ui| {
                    for choice in AntiAliasing::ALL {
                        ui.selectable_value(&mut self.anti_aliasing, choice, anti_aliasing_label(choice));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let (label, tip) = PRESENT_LABELS[Present::ALL.iter().position(|&p| p == self.present).unwrap_or(0)];
            ui.label("Frame presentation").on_hover_text(tip);
            ComboBox::from_id_salt("present")
                .width(FIELD)
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (present, (label, tip)) in Present::ALL.into_iter().zip(PRESENT_LABELS) {
                        ui.selectable_value(&mut self.present, present, label).on_hover_text(tip);
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = "In the cockpit: the rear-view and door mirrors show the stage, the car and the cars behind, \
                       redrawn this often (each mirror in turn). Off: they only reflect the sky";
            ui.label("Mirrors").on_hover_text(tip);
            ComboBox::from_id_salt("mirrors")
                .width(FIELD)
                .selected_text(mirrors_label(self.mirrors))
                .show_ui(ui, |ui| {
                    for rate in Rate::ALL {
                        ui.selectable_value(&mut self.mirrors, rate, mirrors_label(rate));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            ui.label("FPS counter");
            ui.checkbox(&mut self.hud.fps, "top-right corner");
            ui.end_row();
        });

        section(ui, "Camera");
        fields(ui, "camera", |ui| {
            let tip =
                "The horizontal field of view of the in-car views and the free camera; the chase view keeps its own";
            ui.label("Field of view").on_hover_text(tip);
            ui.add(egui::Slider::new(&mut self.fov, coords::FOV_RANGE_DEGREES).step_by(1.0).suffix("°"))
                .on_hover_text(tip);
            ui.end_row();

            ui.label("Cameras");
            self.camera_row(ui);
            ui.end_row();
        });

        section(ui, "HUD and pace notes");
        fields(ui, "hud", |ui| {
            ui.label("HUD");
            ui.checkbox(&mut self.hud.show, "shown");
            ui.end_row();

            let tip = "Needles, lights and displays that follow the car in the cockpit, from the RBRHUD gauges in the \
                       RSF install";
            ui.label("Working dials").on_hover_text(tip);
            ui.checkbox(&mut self.hud.dials, "RSF cars").on_hover_text(tip);
            ui.end_row();

            ui.label("Speed unit");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.hud.mph, false, "km/h");
                ui.radio_value(&mut self.hud.mph, true, "mph");
            });
            ui.end_row();

            ui.label("Co-driver").on_hover_text("Off: no pace notes at all");
            ui.checkbox(&mut self.codriver, "calls the pace notes");
            ui.end_row();

            let tip = "The next notes stacked at the right of the screen (0: none), and the notes as 3D symbols over \
                       the road";
            ui.label("Shown notes").on_hover_text(tip);
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut self.hud.stack).range(0..=4).prefix("stack ")).on_hover_text(tip);
                ui.checkbox(&mut self.hud.pace_3d, "3D notes");
            });
            ui.end_row();

            let tip = "How far ahead the co-driver calls, and the threshold of the road markers";
            ui.label("Call-outs").on_hover_text(tip);
            ui.horizontal(|ui| {
                ui.add(
                    DragValue::new(&mut self.marker.callout_distance).range(0.05..=8.0).speed(0.05).prefix("distance "),
                )
                .on_hover_text(tip);
                ui.add(DragValue::new(&mut self.marker.marker_threshold).prefix("threshold ")).on_hover_text(tip);
            });
            ui.end_row();
        });

        section(ui, "Sound");
        fields(ui, "sound", |ui| {
            ui.label("Volume").on_hover_text("1 is the game's volume");
            ui.add(egui::Slider::new(&mut self.volume, 0.0..=MAX_VOLUME).max_decimals(2));
            ui.end_row();
        });
    }

    /// One box for each renderer addition, with its tip on hover, and the presets.
    fn enhancements(&mut self, ui: &mut egui::Ui) {
        section(ui, "Enhancements");
        ui.colored_label(
            Color32::GRAY,
            "Additions beyond the game's look. The stage always draws at its highest quality.",
        );
        ui.add_space(4.0);
        for (k, (label, tip)) in GRAPHICS_LABELS.into_iter().enumerate() {
            ui.checkbox(&mut self.graphics[k], label).on_hover_text(tip);
        }
        ui.add_space(8.0);
        // The presets leave the render scale as it is: it changes the frame's size, not the additions.
        ui.horizontal(|ui| {
            if ui.button("All on").clicked() {
                self.graphics = [true; rbr_render::renderer::Enhance::NAMES.len()];
            }
            if ui.button("All off").clicked() {
                self.graphics = Default::default();
            }
            if ui.button("Defaults").clicked() {
                self.graphics = DEFAULT_GRAPHICS;
            }
        });
    }

    /// The Cameras row: the driving window's camera keys, and "Reset this car's cameras" (its nudges in the
    /// settings file's `[camera]` section, `rbr_render::camera_prefs`).
    fn camera_row(&mut self, ui: &mut egui::Ui) {
        let Some(car) = self.car() else { return };
        let key = camera_prefs::car_key(&car.name, car.rsf.as_ref().map(|r| r.id));
        let reset = ui
            .add_enabled(!self.read_only, egui::Button::new("Reset this car's cameras"))
            .on_hover_text("While driving: C changes the view (kept for the next drive); the pause menu's Camera moves it for this car");
        if reset.clicked() {
            let file = std::fs::read_to_string(&self.paths.settings).unwrap_or_default();
            let mut prefs = camera_prefs::CameraPrefs::parse(&file);
            let had = prefs.cars.contains_key(&key);
            prefs.set_nudges(&key, [Default::default(); 4]);
            self.status = match std::fs::write(&self.paths.settings, prefs.replace_in(&file)) {
                Ok(()) if had => format!("{}: cameras reset", car.name),
                Ok(()) => format!("{}: cameras not adjusted", car.name),
                Err(e) => format!("{}: {e}", self.paths.settings.display()),
            };
        }
    }
}

fn size_label((w, h): (u32, u32)) -> String {
    format!("{w} x {h}")
}

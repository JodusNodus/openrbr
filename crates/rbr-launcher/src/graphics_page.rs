//! The Graphics & sound page: the window, the camera, the HUD, the co-driver, the sound and the renderer's additions.

use egui::{Color32, ComboBox, DragValue};
use rbr_render::camera_prefs;
use rbr_render::coords;
use rbr_render::hud::{Part, Parts};
use rbr_render::mirrors::Rate;
use rbr_render::present::Present;
use rbr_render::renderer::{AntiAliasing, Detail, Enhance, Reflections};
use rbr_sound::preview::{self as codriver_preview, Preview};

use crate::command::ORIGINAL_VOICE;
use crate::pages::{fields, section, DRIVE_FIELDS, FIELD, GOOD, WARN};
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
        "Soft car shadow",
        "The car's own shape as a soft shadow on the ground and fences under it, in place of the game's shadow decal",
    ),
    (
        "Soft decals",
        "Skid marks painted on the stage (some RallySimFans stages) blended softly into the road instead of cut out \
         into hard black shapes",
    ),
    (
        "Enhanced gravel",
        "Gravel and dirt roads get the fine grain and relief of a scanned gravel surface, lit by the sun, over the \
         stage's own road colour (about 0.1 ms)",
    ),
    (
        "Lighting",
        "A physically based sky and light for the time of day, with moving volumetric clouds, haze and mist, sun \
         shafts, real-time sun shadows and sunlit foliage, ambient occlusion and bounced light worked out from the \
         stage when it loads, and reflection probes. Costs frame time (about 1 ms or more) and a longer load (a \
         second or two)",
    ),
    (
        "Realistic tyre marks",
        "Tyre marks as wide as the tread and swept wider in a slide: ruts with berms in gravel, rubber on tarmac only \
         when sliding, grooves in snow, flattened grass, longer trails",
    ),
    ("Weather and particle effects", "GPU rain and snow around the car, rings on wet roads, sunlit and soft particles"),
    ("Flowing water", "Rivers and lakes that flow, ripple and fade into their banks"),
    ("Puddles on wet roads", "Puddles on the wet road in rain, reflecting the sky and the stage"),
];

/// The index of `name` in [`Enhance::NAMES`].
#[cfg(test)]
pub fn graphics_index(name: &str) -> usize {
    Enhance::NAMES.iter().position(|n| *n == name).expect("an Enhance name")
}

/// The additions on by default: all but the tyre marks, not yet reviewed in play. The cabin light costs next to nothing.
pub const DEFAULT_GRAPHICS: Graphics = [true, true, true, true, true, true, false, true, true, true];

/// The render scales the Graphics & sound page offers, percent of the window's width and height (`viewer --render-scale`): the
/// frame is drawn smaller and the post pass scales it up, for high refresh rates at 1440p and above. 100, the first,
/// is the default.
pub const RENDER_SCALES: [u32; 4] = [100, 75, 67, 50];

/// A render scale's label.
fn scale_label(percent: u32) -> String {
    format!("{percent}%")
}

/// A screen-space reflections choice's label.
fn reflections_label(choice: Reflections) -> &'static str {
    match choice {
        Reflections::High => "High",
        Reflections::Medium => "Medium",
        Reflections::Low => "Low",
        Reflections::Off => "Off",
    }
}

/// A detail choice's label.
fn detail_label(choice: Detail) -> &'static str {
    match choice {
        Detail::High => "High",
        Detail::Ultra => "Ultra",
    }
}

/// An anti-aliasing choice's label.
fn anti_aliasing_label(choice: AntiAliasing) -> &'static str {
    match choice {
        AntiAliasing::Taa => "Temporal",
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

/// The HUD's options (`viewer --no-hud`, `--mph`, `--fps`, `--pace-stack`, `--hud-hide`), and the
/// cockpit's working dials (`--no-dials`) and its steering wheel (`--hide-wheel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudChoice {
    pub show: bool,
    pub mph: bool,
    pub fps: bool,
    /// The pace stack's notes, 0 to 4 (the game's `PN_STACK`; 0, its default, hides it).
    pub stack: u8,
    /// The working dials of the RSF cars with RBRHUD gauges in the install (on by default).
    pub dials: bool,
    /// The cockpit's steering wheel is hidden (off by default).
    pub hide_wheel: bool,
    /// The HUD's parts shown (all by default).
    pub parts: Parts,
}

/// Each HUD part's label and hover tip, by [`Part::ALL`].
const PART_LABELS: [(&str, &str); Part::ALL.len()] = [
    ("Stage clock", "The race time and its penalties"),
    ("Stage progress", "The progress bar with the splits, the distance driven and the distance to go"),
    ("Pace note", "The symbol of each call at the top"),
    ("Split times", "The time and difference as each split is passed"),
    ("Gear, speed, revs", "The rev dial with its shift light, gear, speed, revs, water and turbo"),
    ("Pedals, steering", "The pedal arcs inside the dial and the steering under it"),
    ("Car state", "The lit warning lamps and the damaged parts at the bottom left, shown only when there are any"),
    ("Messages", "The stand-still and car-problem messages"),
    ("Camera name", "The view's name at the top right"),
];

impl Default for HudChoice {
    fn default() -> Self {
        HudChoice {
            show: true,
            mph: false,
            fps: false,
            stack: 0,
            dials: true,
            hide_wheel: false,
            parts: Parts::default(),
        }
    }
}

/// The loudest sound volume the viewer takes (`viewer --volume`).
pub const MAX_VOLUME: f32 = 4.0;

/// The loudest each sound group takes (`viewer --volumes`).
pub const MAX_GROUP_VOLUME: f32 = 2.0;

/// A call style or range set file's stem: without `.ini`.
fn ini_stem(file: &str) -> &str {
    match file.len().checked_sub(4) {
        Some(n) if file[n..].eq_ignore_ascii_case(".ini") => &file[..n],
        _ => file,
    }
}

/// The stem's base when it is a `-xx` language variant (exactly two ASCII lowercase letters after the last `-`):
/// ReRBR speaks English only, so variants stay out of the dropdowns unless chosen.
fn base_stem(file: &str) -> (&str, Option<&str>) {
    let stem = ini_stem(file);
    match stem.rsplit_once('-') {
        Some((base, lang)) if !base.is_empty() && lang.len() == 2 && lang.bytes().all(|b| b.is_ascii_lowercase()) => {
            (stem, Some(base))
        }
        _ => (stem, None),
    }
}

/// Whether the call style or range set file is offered: `-xx` language variants stay out unless chosen.
pub fn show_call_file(file: &str, current: &str) -> bool {
    base_stem(file).1.is_none() || file.eq_ignore_ascii_case(current)
}

/// A Corner calls file's label: what the player hears for each corner. Unknown stems show as themselves.
pub fn style_label(file: &str) -> String {
    let (stem, base) = base_stem(file);
    match base.unwrap_or(stem).to_ascii_lowercase().as_str() {
        "numeric" => "Numbers, 1 tightest (\"one left\")".into(),
        "numeric-swapped" => "Numbers, 1 tightest (\"left one\")".into(),
        "numeric-descending" => "Numbers, 6 tightest (\"six left\")".into(),
        "numeric-descending-swapped" => "Numbers, 6 tightest (\"left six\")".into(),
        "descriptive" => "Words (\"easy left\", \"hairpin\")".into(),
        "rbr" => "Original RBR calls".into(),
        "rbr-enhanced" => "Original RBR calls, more of them".into(),
        _ => stem.to_owned(),
    }
}

/// A Distance calls file's label. Unknown stems show as themselves.
pub fn range_label(file: &str) -> String {
    let (stem, base) = base_stem(file);
    match base.unwrap_or(stem).to_ascii_lowercase().as_str() {
        "rbr" => "Standard RBR distances".into(),
        "extended" => "Extended, finer distances".into(),
        _ => stem.to_owned(),
    }
}

/// The Co-driver dropdown's choice: off, the original 1.02 voice, or a voice by its source pack (none: the
/// RallySimFans install's plugin).
#[derive(Debug, Clone, PartialEq, Eq)]
enum VoiceChoice {
    Off,
    Original,
    Voice { pack: Option<String>, voice: String },
}

/// The Stage notes dropdown's choice: the original notes, the RallySimFans install's, or a local pack's.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NotesChoice {
    Original,
    Rsf,
    Pack(String),
}

impl VoiceChoice {
    /// The dropdown's choice for the saved fields: an unset voice plays its source's default (`default_voice`:
    /// its `PaceNote.ini`'s, else its first voice), so an old settings file without `codriver_voice` still names
    /// the voice heard; with no voice source at all the original voice drives.
    fn of(codriver: bool, pack: Option<&str>, voice: Option<&str>, default_voice: Option<&str>) -> Self {
        match voice.or(default_voice) {
            _ if !codriver => VoiceChoice::Off,
            Some(ORIGINAL_VOICE) | None => VoiceChoice::Original,
            Some(voice) => VoiceChoice::Voice { pack: pack.map(str::to_owned), voice: voice.to_owned() },
        }
    }

    /// The dropdown's text: the voice and its source.
    fn label(&self) -> String {
        match self {
            VoiceChoice::Off => "Off".into(),
            VoiceChoice::Original => "Original (1.02)".into(),
            VoiceChoice::Voice { pack: None, voice } => format!("{voice} (RallySimFans install)"),
            VoiceChoice::Voice { pack: Some(pack), voice } => format!("{voice} ({pack})"),
        }
    }
}

impl NotesChoice {
    /// The dropdown's choice for the saved fields: without a notes source the original notes drive (as
    /// `resolve_notes` finds none), so an old settings file without a pack still reads sensibly.
    fn of(my_pacenotes: bool, notes_pack: Option<&str>, has_rsf: bool) -> Self {
        match notes_pack {
            _ if !my_pacenotes => NotesChoice::Original,
            Some(pack) => NotesChoice::Pack(pack.to_owned()),
            None if has_rsf => NotesChoice::Rsf,
            None => NotesChoice::Original,
        }
    }

    /// The dropdown's text.
    fn label(&self) -> String {
        match self {
            NotesChoice::Original => "The stage's own notes".into(),
            NotesChoice::Rsf => "RallySimFans install".into(),
            NotesChoice::Pack(pack) => pack.clone(),
        }
    }
}

impl Form {
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
                       high refresh rates at 1440p and above";
            ui.label("Render scale").on_hover_text(tip);
            ComboBox::from_id_salt("render scale")
                .selected_text(scale_label(self.render_scale))
                .show_ui(ui, |ui| {
                    for percent in RENDER_SCALES {
                        ui.selectable_value(&mut self.render_scale, percent, scale_label(percent));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip =
                "Temporal: one sample a pixel, jittered and gathered over the frames: smooth edges, the foliage's \
                       and fences' cut-outs too. Off: one sample a pixel, the cut-outs as in 1.02: a little faster, \
                       with jagged edges";
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

            let tip = "Reflections of the scenery on screen in wet roads and water. Medium and Low: fewer rays a pixel \
                       and fewer steps a ray, grainier; Low about 1 ms a frame less at 1080p in the wet. Off: the baked reflections of the stage and the sky alone; saves \
                       5 to 6.5 ms a frame at 1080p in the wet where a road fills the view";
            ui.label("Wet reflections").on_hover_text(tip);
            ComboBox::from_id_salt("reflections")
                .width(FIELD)
                .selected_text(reflections_label(self.reflections))
                .show_ui(ui, |ui| {
                    for choice in Reflections::ALL {
                        ui.selectable_value(&mut self.reflections, choice, reflections_label(choice));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = "Ultra, for strong GPUs: sun shadows at twice the texels (sharper shadows of posts, cables and \
                       trunks) and sharper reflections in the car's paint and the wet roads. About 1 to 2 ms a frame \
                       more at 1440p, 2.5 to 3.5 ms in the wet, and 0.5 GB more video memory on long stages";
            ui.label("Detail").on_hover_text(tip);
            ComboBox::from_id_salt("detail")
                .width(FIELD)
                .selected_text(detail_label(self.detail))
                .show_ui(ui, |ui| {
                    for choice in Detail::ALL {
                        ui.selectable_value(&mut self.detail, choice, detail_label(choice));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = if self.upscaler {
                "Upscales the car's and the stage's textures 4x (to at most 4096 texels a side), so the bonnet, the \
                 livery, the cockpit, the gravel and the signs stay sharp up close. The first drive with \
                 a car or on a stage waits while they are made; then they are kept and load at once"
            } else if crate::upscaler::available() {
                "Needs the upscaler, about 50 MB: Download upscaler… gets it"
            } else {
                "Needs the upscaler, which Real-ESRGAN does not build for this platform"
            };
            ui.label("Textures").on_hover_text(tip);
            if self.upscaler {
                ui.checkbox(&mut self.upscale_textures, "upscaled").on_hover_text(tip);
            } else if crate::upscaler::available() {
                ui.vertical(|ui| self.upscaler_download_ui(ui));
            } else {
                ui.add_enabled_ui(false, |ui| ui.checkbox(&mut self.upscale_textures, "upscaled"))
                    .response
                    .on_disabled_hover_text(tip);
            }
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

            let tip =
                "Writes how long each frame, each stage on the CPU and each pass on the GPU takes, once a second \
                       (a .log to paste in a bug report) and the last 600 frames at the end (a .trace.json for \
                       ui.perfetto.dev). Costs well under 0.2 ms a frame";
            ui.label("Performance log").on_hover_text(tip);
            ui.vertical(|ui| {
                ui.checkbox(&mut self.perf_log, "write one for each drive").on_hover_text(tip);
                if self.perf_log {
                    ui.weak(format!("In {}", self.logs_dir().display()));
                }
            });
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

        section(ui, "HUD");
        fields(ui, "hud", |ui| {
            ui.label("HUD");
            ui.checkbox(&mut self.hud.show, "shown");
            ui.end_row();

            let tip = "The countdown, false start, wrong way and the finish and result pages always show with the HUD";
            ui.label("HUD parts").on_hover_text(tip);
            ui.add_enabled_ui(self.hud.show, |ui| {
                ui.vertical(|ui| {
                    for (row, labels) in self.hud.parts.0.chunks_mut(3).zip(PART_LABELS.chunks(3)) {
                        ui.horizontal(|ui| {
                            for (shown, (label, tip)) in row.iter_mut().zip(labels) {
                                let cell = egui::Layout::left_to_right(egui::Align::Center);
                                ui.allocate_ui_with_layout(egui::vec2(140.0, 20.0), cell, |ui| {
                                    ui.set_min_width(140.0);
                                    ui.checkbox(shown, *label).on_hover_text(*tip);
                                });
                            }
                        });
                    }
                });
            });
            ui.end_row();

            let tip = "Needles, lights and displays that follow the car in the cockpit, from the RBRHUD gauges in the \
                       RSF install";
            ui.label("Working dials").on_hover_text(tip);
            ui.checkbox(&mut self.hud.dials, "RSF cars").on_hover_text(tip);
            ui.end_row();

            let tip = "Leaves the car's steering wheel out of the cockpit view, for driving with a wheel of your own";
            ui.label("Cockpit wheel").on_hover_text(tip);
            ui.checkbox(&mut self.hud.hide_wheel, "Hide").on_hover_text(tip);
            ui.end_row();

            ui.label("Speed unit");
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.hud.mph, false, "km/h");
                ui.radio_value(&mut self.hud.mph, true, "mph");
            });
            ui.end_row();

            let tip = "The next notes stacked at the right of the screen (0: none)";
            ui.label("Shown notes").on_hover_text(tip);
            ui.add(DragValue::new(&mut self.hud.stack).range(0..=4).prefix("stack ")).on_hover_text(tip);
            ui.end_row();
        });

        self.codriver_section(ui);

        section(ui, "Sound");
        fields(ui, "sound", |ui| {
            ui.label("Volume").on_hover_text("1 is the game's volume");
            ui.add(egui::Slider::new(&mut self.volume, 0.0..=MAX_VOLUME).max_decimals(2));
            ui.end_row();
            ui.label("Engine").on_hover_text("The engine, the gearbox whine, and an FMOD car's whole sound");
            ui.add(egui::Slider::new(&mut self.engine_volume, 0.0..=MAX_GROUP_VOLUME).max_decimals(2));
            ui.end_row();
            ui.label("Co-driver").on_hover_text("The pace notes, the countdown and the damage calls");
            ui.add(egui::Slider::new(&mut self.codriver_volume, 0.0..=MAX_GROUP_VOLUME).max_decimals(2));
            ui.end_row();
            ui.label("Effects").on_hover_text("The turbo, pops, impacts, weather and the rest");
            ui.add(egui::Slider::new(&mut self.effects_volume, 0.0..=MAX_GROUP_VOLUME).max_decimals(2));
            ui.end_row();
            ui.label("Tyres").on_hover_text("Rolling, rumble and gravel");
            ui.add(egui::Slider::new(&mut self.tyres_volume, 0.0..=MAX_GROUP_VOLUME).max_decimals(2));
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
                self.graphics = [true; Enhance::NAMES.len()];
            }
            if ui.button("All off").clicked() {
                self.graphics = Default::default();
            }
            if ui.button("Defaults").clicked() {
                self.graphics = DEFAULT_GRAPHICS;
            }
        });
    }

    /// The Co-driver dropdown's choice from the fields, resolving an unset voice to its source's default.
    fn voice_choice(&self) -> VoiceChoice {
        let default = self.codriver_mods.as_ref().and_then(|mods| mods.voice(None));
        VoiceChoice::of(
            self.codriver,
            self.codriver_pack.as_deref(),
            self.codriver_voice.as_deref(),
            default.as_deref(),
        )
    }

    /// The Co-driver section: the voice and its preview, the stage notes, which file the stage resolves to, the
    /// corner and distance calls (when they change anything), the call timing, and the packs.
    fn codriver_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Co-driver");
        fields(ui, "codriver", |ui| {
            self.codriver_voice_row(ui);
            self.codriver_notes_row(ui);
            self.codriver_status_row(ui);
            let custom = matches!(self.voice_choice(), VoiceChoice::Voice { .. });
            if let Some(mods) = self.codriver_mods.as_ref().filter(|_| custom) {
                if self.codriver_styles_differ {
                    let tip = "Which words the voice says for each corner";
                    Self::call_file_row(ui, "Corner calls", tip, &mods.styles, &mut self.codriver_style, style_label);
                }
                if self.codriver_ranges_differ {
                    let tip = "How the voice says the distances between notes";
                    Self::call_file_row(
                        ui,
                        "Distance calls",
                        tip,
                        &mods.ranges,
                        &mut self.codriver_ranges,
                        range_label,
                    );
                }
            }
            let tip = "How many seconds before a note the co-driver calls it, at your speed";
            ui.label("Call ahead").on_hover_text(tip);
            ui.add(DragValue::new(&mut self.marker.callout_distance).range(0.05..=8.0).speed(0.05).suffix(" s"))
                .on_hover_text(tip);
            ui.end_row();

            // The custom co-driver fires on its own lookahead and never reads the threshold: only the original
            // voice skips notes below it.
            if matches!(self.voice_choice(), VoiceChoice::Original) {
                let tip = "The original co-driver skips a note while the speed is below this";
                ui.label("Quiet below").on_hover_text(tip);
                ui.add(DragValue::new(&mut self.marker.marker_threshold).suffix(" km/h")).on_hover_text(tip);
                ui.end_row();
            }

            self.codriver_packs_row(ui);
        });
    }

    /// The Voice row: who calls the pace notes, with a preview of the corner ladder and one distance call. An
    /// unset voice plays its source's default, so an old settings file still names the voice heard.
    fn codriver_voice_row(&mut self, ui: &mut egui::Ui) {
        let tip = "Who calls the pace notes: off (no pace notes at all), the original 1.02 co-driver, or a \
                     voice from the RallySimFans install's Pacenote plugin (only read) or a pack copied into \
                     your files' folder. Whether a pack covers every note of a stage varies by pack.";
        ui.label("Voice").on_hover_text(tip);
        let current = self.voice_choice();
        ui.horizontal(|ui| {
            let mut picked = current.clone();
            ComboBox::from_id_salt("codriver voice").width(FIELD).selected_text(current.label()).show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, VoiceChoice::Off, "Off");
                ui.selectable_value(&mut picked, VoiceChoice::Original, "Original (1.02)");
                if !self.rsf_voices.is_empty() {
                    ui.label("RallySimFans install");
                    for voice in &self.rsf_voices {
                        ui.selectable_value(
                            &mut picked,
                            VoiceChoice::Voice { pack: None, voice: voice.clone() },
                            voice,
                        );
                    }
                }
                for pack in self.owned_voices.iter().filter(|p| p.problem.is_none()) {
                    ui.label(&pack.name);
                    for voice in &pack.voices {
                        let choice = VoiceChoice::Voice { pack: Some(pack.name.clone()), voice: voice.clone() };
                        ui.selectable_value(&mut picked, choice, voice);
                    }
                }
            });
            match picked {
                _ if picked == current => {}
                // Off keeps the pack and voice, so turning it back on restores them.
                VoiceChoice::Off => self.codriver = false,
                VoiceChoice::Original => {
                    (self.codriver, self.codriver_voice) = (true, Some(ORIGINAL_VOICE.to_owned()));
                }
                // A changed pack rescans and normalizes in `pages.rs`.
                VoiceChoice::Voice { pack, voice } => {
                    (self.codriver, self.codriver_pack, self.codriver_voice) = (true, pack, Some(voice));
                }
            }
            // The preview beside the dropdown: the corner ladder and one distance call with this voice. Built
            // on a thread, so the UI never stalls.
            let idle = self.child.is_none() && self.codriver_job.is_none();
            let preview = ui
                .add_enabled(idle && !matches!(current, VoiceChoice::Off), egui::Button::new("▶ Preview"))
                .on_hover_text(
                    "Hear the corners, tightest to fastest, and one distance call with this voice, style and \
                     range set",
                );
            if preview.clicked() {
                self.preview_codriver();
            }
        });
        ui.end_row();

        // A chosen source that is gone or broke: the original voice drives instead.
        if let VoiceChoice::Voice { pack, .. } = self.voice_choice() {
            let bad = match pack.as_deref() {
                Some(pack) => match self.owned_voices.iter().find(|p| p.name == pack) {
                    Some(found) => found.problem.clone().map(|p| format!("{pack}: {p}")),
                    None => Some(format!("{pack}: no longer installed")),
                },
                None if !self.rsf_plugin => Some("the RallySimFans install has no Pacenote plugin".to_owned()),
                None => None,
            };
            if let Some(warn) = bad {
                ui.label("");
                ui.colored_label(WARN, format!("⚠ {warn}: the original voice"));
                ui.end_row();
            }
        }
    }

    /// A Corner calls or Distance calls row: the plugin's `files` by `label`, language variants left out unless chosen.
    fn call_file_row(
        ui: &mut egui::Ui,
        name: &str,
        tip: &str,
        files: &[String],
        chosen: &mut String,
        label: fn(&str) -> String,
    ) {
        ui.label(name).on_hover_text(tip);
        ComboBox::from_id_salt(name)
            .selected_text(label(chosen))
            .show_ui(ui, |ui| {
                let shown: Vec<&String> = files.iter().filter(|f| show_call_file(f, chosen)).collect();
                for file in shown {
                    ui.selectable_value(chosen, file.clone(), label(file));
                }
            })
            .response
            .on_hover_text(tip);
        ui.end_row();
    }

    /// The Stage notes row: whose custom stage notes drive. Without a custom voice there is nothing to say them
    /// with, so the dropdown stays disabled.
    fn codriver_notes_row(&mut self, ui: &mut egui::Ui) {
        ui.label("Stage notes").on_hover_text(
            "Whose custom stage notes drive: the stage's original notes, the RallySimFans install's MyPacenotes \
             (only read), or a pack copied into your files' folder.",
        );
        let has_rsf = !self.rsf().trim().is_empty();
        let current = NotesChoice::of(self.my_pacenotes, self.notes_pack.as_deref(), has_rsf);
        let mut picked = current.clone();
        let custom = matches!(self.voice_choice(), VoiceChoice::Voice { .. });
        ui.add_enabled_ui(custom, |ui| {
            ComboBox::from_id_salt("notes pack").width(FIELD).selected_text(current.label()).show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, NotesChoice::Original, "The stage's own notes");
                if has_rsf {
                    ui.selectable_value(&mut picked, NotesChoice::Rsf, "RallySimFans install");
                }
                for pack in &self.owned_notes {
                    ui.selectable_value(&mut picked, NotesChoice::Pack(pack.name.clone()), &pack.name);
                }
            });
        });
        match picked {
            _ if picked == current => {}
            NotesChoice::Original => self.my_pacenotes = false,
            NotesChoice::Rsf => (self.my_pacenotes, self.notes_pack) = (true, None),
            NotesChoice::Pack(pack) => (self.my_pacenotes, self.notes_pack) = (true, Some(pack)),
        }
        ui.end_row();
    }

    /// The status line: which notes file the stage resolves to (the file name in the row, the full path on
    /// hover), refreshed when the map, the notes sources, the co-driver switch or voice, or an import changes it
    /// (never scanned per draw). Without a custom voice there is nothing to say custom notes with.
    fn codriver_status_row(&mut self, ui: &mut egui::Ui) {
        ui.label("");
        if !matches!(self.voice_choice(), VoiceChoice::Voice { .. }) {
            ui.label("Needs a voice from a pack or the RallySimFans install");
        } else {
            self.refresh_notes_cache();
            let response = ui.label(self.cached_notes_short.clone());
            if !self.cached_notes_full.is_empty() {
                response.on_hover_text(self.cached_notes_full.clone());
            }
        }
        ui.end_row();
    }

    /// The Packs row: the import buttons and Manage packs, as before.
    fn codriver_packs_row(&mut self, ui: &mut egui::Ui) {
        ui.label("Packs");
        ui.vertical(|ui| {
            self.import_codriver_ui(ui);
            egui::CollapsingHeader::new("Manage packs").id_salt("codriver packs").show(ui, |ui| {
                // No removals while importing or driving.
                let idle = self.codriver_job.is_none() && self.child.is_none();
                let mut remove_voice = None;
                for pack in &self.owned_voices {
                    ui.horizontal(|ui| {
                        match &pack.problem {
                            Some(problem) => ui.colored_label(WARN, format!("{}: {problem}", pack.name)),
                            None => ui.colored_label(
                                GOOD,
                                format!(
                                    "✔ {}: {} voices, {} call styles, {} range sets",
                                    pack.name,
                                    pack.voices.len(),
                                    pack.styles.len(),
                                    pack.ranges.len()
                                ),
                            ),
                        };
                        if ui.add_enabled(idle, egui::Button::new("Remove")).clicked() {
                            remove_voice = Some(pack.name.clone());
                        }
                    });
                }
                if let Some(pack) = remove_voice {
                    self.codriver_status = crate::codriver_packs::remove_voice(&self.here(), &pack).err().map(Err);
                    if self.codriver_pack.as_deref() == Some(&pack) {
                        self.codriver_pack = None;
                    }
                    self.rescan_codrivers();
                }
                let mut remove_notes = None;
                for pack in &self.owned_notes {
                    ui.horizontal(|ui| {
                        ui.colored_label(GOOD, format!("✔ {}: {} note files", pack.name, pack.files));
                        if ui.add_enabled(idle, egui::Button::new("Remove")).clicked() {
                            remove_notes = Some(pack.name.clone());
                        }
                    });
                }
                if let Some(pack) = remove_notes {
                    self.codriver_status = crate::codriver_packs::remove_notes(&self.here(), &pack).err().map(Err);
                    if self.notes_pack.as_deref() == Some(&pack) {
                        self.notes_pack = None;
                    }
                    self.rescan_codrivers();
                }
            });
        });
        ui.end_row();
    }

    /// Plays the corner ladder and one distance call in the chosen voice through the menus' sound, built on its own
    /// thread.
    fn preview_codriver(&mut self) {
        let build: Box<dyn FnOnce() -> Result<Preview, String> + Send> = match self.voice_choice() {
            VoiceChoice::Off => return,
            VoiceChoice::Original => {
                let data = self.data.clone();
                Box::new(move || codriver_preview::original_preview(&data))
            }
            VoiceChoice::Voice { .. } => {
                let Some(mods) = &self.codriver_mods else { return };
                let Some(voice) = mods.voice(self.codriver_voice.as_deref()) else { return };
                let (plugin, style, ranges) =
                    (mods.dir.clone(), self.codriver_style.clone(), self.codriver_ranges.clone());
                Box::new(move || codriver_preview::custom_preview(&plugin, &voice, &style, &ranges))
            }
        };
        let gain = self.codriver_volume;
        if let Some(player) = self.menu_player() {
            player.preview(gain, build);
        }
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::settings::{load, Settings};
    use crate::{Start, SETTINGS_FILE};

    fn write(path: PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
    }

    /// The co-driver rows drawn offscreen to `RBR_SCREENSHOTS`: a locally owned voice pack and Tanner notes with
    /// no RSF install, and the RSF install's plugin. Needs `RBR_DATA`, `RBR_PATCH`, `RBR_NGP_PHYSICS` and `RBR_RSF`.
    #[test]
    fn screenshots_of_the_codriver_rows() {
        let env = |key: &str| std::env::var_os(key).map(|v| v.to_string_lossy().into_owned());
        let (Some(shots), Some(data), Some(patch), Some(ngp), Some(rsf)) =
            (env("RBR_SCREENSHOTS"), env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS"), env("RBR_RSF"))
        else {
            eprintln!("skipping: RBR_SCREENSHOTS, RBR_DATA, RBR_PATCH, RBR_NGP_PHYSICS or RBR_RSF not set");
            return;
        };
        let root = std::env::temp_dir().join(format!("rbr-launcher-codriver-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        // A voice download and a notes-only download, as imported.
        let download = root.join("voice");
        write(download.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        write(download.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(download.join("config/pacenotes/calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(download.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(download.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        let notes_src = root.join("notes");
        write(
            notes_src.join("Tanner/tanner.ini"),
            "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n",
        );
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        crate::codriver_packs::import(&home, &download, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        crate::codriver_packs::import(&home, &notes_src, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let start_in = |home: &Path| Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: home.join(SETTINGS_FILE),
            data: Some(PathBuf::from(&data)),
            patch: Some(PathBuf::from(&patch)),
        };
        // Without RSF: the owned packs drive.
        let settings = Settings {
            map: Some(54),
            ngp_physics: Some(ngp.clone()),
            rsf_root: Some(String::new()),
            codriver_pack: Some("voice".into()),
            notes_pack: Some("notes".into()),
            ..Settings::default()
        };
        let mut local = load(&start_in(&home), settings).unwrap();
        local.read_only = true;
        assert!(local.codriver_mods.is_some());
        assert!(!local.rsf_plugin);
        // With RSF: the install's plugin drives, as before.
        let settings = Settings { map: Some(54), ngp_physics: Some(ngp), rsf_root: Some(rsf), ..Settings::default() };
        let elsewhere = root.join("rsf-home");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let mut installed = load(&start_in(&elsewhere), settings).unwrap();
        installed.read_only = true;
        assert!(installed.codriver_mods.is_some());
        assert!(installed.rsf_plugin);
        for (name, form) in [("codriver-local.png", &mut local), ("codriver-rsf.png", &mut installed)] {
            // The status line names the stage and the file (the full path rides the tooltip, not the layout).
            form.refresh_notes_cache();
            let (stage, file) = form.cached_notes_short.split_once(": ").unwrap();
            assert!(!stage.is_empty(), "{}", form.cached_notes_short);
            assert!(file.ends_with(".ini"), "{}", form.cached_notes_short);
            assert!(!file.contains('/'), "{}", form.cached_notes_short);
            assert!(form.cached_notes_full.ends_with(".ini"), "{}", form.cached_notes_full);
            let rgba = crate::capture::draw(|ctx| {
                ctx.root_ui(|root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        form.codriver_section(ui);
                    })
                });
            })
            .unwrap();
            crate::capture::write_png(&Path::new(&shots).join(name), &rgba).unwrap();
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The dropdowns read the saved fields: an unset voice names its source's default, and without a source the
    /// original voice and notes drive. Needs no game data.
    #[test]
    fn the_dropdowns_read_the_fields() {
        use super::{NotesChoice, VoiceChoice};
        use crate::command::ORIGINAL_VOICE;
        let voice =
            |pack: Option<&str>, voice: &str| VoiceChoice::Voice { pack: pack.map(Into::into), voice: voice.into() };
        assert_eq!(VoiceChoice::of(false, Some("pack"), Some("v"), None), VoiceChoice::Off);
        assert_eq!(VoiceChoice::of(true, Some("pack"), Some(ORIGINAL_VOICE), None), VoiceChoice::Original);
        assert_eq!(VoiceChoice::of(true, None, Some("Janne"), Some("steve")), voice(None, "Janne"));
        assert_eq!(VoiceChoice::of(true, Some("pack"), None, Some("steve")), voice(Some("pack"), "steve"));
        assert_eq!(VoiceChoice::of(true, Some("pack"), None, None), VoiceChoice::Original);
        assert_eq!(voice(None, "Janne").label(), "Janne (RallySimFans install)");
        assert_eq!(NotesChoice::of(false, Some("pack"), true), NotesChoice::Original);
        assert_eq!(NotesChoice::of(true, Some("pack"), false), NotesChoice::Pack("pack".into()));
        assert_eq!(NotesChoice::of(true, None, true), NotesChoice::Rsf);
        assert_eq!(NotesChoice::of(true, None, false), NotesChoice::Original);
    }

    /// The Stage notes row follows the note pack, the checkbox, the co-driver switch and the voice: switching one
    /// dirties the cache (as `pages.rs` does), and the next refresh names the file actually used — never a stale
    /// file, and never a custom file when the viewer would not get `--pacenotes`.
    #[test]
    fn stage_notes_row_follows_sources_and_voice() {
        use crate::command::ORIGINAL_VOICE;
        let env = |key: &str| std::env::var_os(key).map(|v| v.to_string_lossy().into_owned());
        let (Some(data), Some(patch), Some(ngp)) = (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS")) else {
            eprintln!("skipping: RBR_DATA, RBR_PATCH or RBR_NGP_PHYSICS not set");
            return;
        };
        let root = std::env::temp_dir().join(format!("rbr-launcher-notes-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
        };
        let download = root.join("voice");
        write(download.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        write(download.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(download.join("config/pacenotes/calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(download.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(download.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        let notes_src = root.join("notes");
        write(
            notes_src.join("Tanner/tanner.ini"),
            "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n",
        );
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let no_cancel = std::sync::atomic::AtomicBool::new(false);
        crate::codriver_packs::import(&home, &download, &no_cancel).unwrap();
        crate::codriver_packs::import(&home, &notes_src, &no_cancel).unwrap();
        let start = Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: home.join(SETTINGS_FILE),
            data: Some(PathBuf::from(&data)),
            patch: Some(PathBuf::from(&patch)),
        };
        let settings = Settings {
            map: Some(54),
            ngp_physics: Some(ngp),
            rsf_root: Some(String::new()),
            codriver_pack: Some("voice".into()),
            notes_pack: Some("notes".into()),
            ..Settings::default()
        };
        let mut form = load(&start, settings).unwrap();
        form.read_only = true;
        let stage = form.catalog.maps.iter().find(|m| m.number == 54).map(|m| m.name()).unwrap();
        let ctx = egui::Context::default();
        let sources = |form: &crate::Form| {
            (form.notes_pack.clone(), form.my_pacenotes, form.codriver, form.codriver_voice.clone())
        };
        // The chosen pack's file drives, named by its stage.
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: tanner.ini"), "{}", form.cached_notes_short);
        // Unticking the checkbox without invalidation keeps the stale file (the bug): with it, the stage's own
        // notes take over.
        let before = sources(&form);
        form.my_pacenotes = false;
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: tanner.ini"), "{}", form.cached_notes_short);
        form.invalidate_notes_cache(&ctx, before);
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: the stage's own notes"));
        // Dropping the note pack (no RSF install) names the missing source.
        let before = sources(&form);
        form.my_pacenotes = true;
        form.notes_pack = None;
        form.invalidate_notes_cache(&ctx, before);
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: no note pack, its own notes are used"));
        // Switching the co-driver off or to the 1.02 voice never claims a custom file is used.
        let before = sources(&form);
        form.notes_pack = Some("notes".into());
        form.codriver = false;
        form.invalidate_notes_cache(&ctx, before);
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: co-driver off, custom notes inactive"));
        let before = sources(&form);
        form.codriver = true;
        form.codriver_voice = Some(ORIGINAL_VOICE.into());
        form.invalidate_notes_cache(&ctx, before);
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: 1.02 voice, custom notes inactive"));
        // A note pack alone cannot drive custom calls with the original voice. Rescanning a missing or
        // removed voice pack dirties the row, which must say the notes are inactive.
        form.codriver_mods = None;
        form.cached_notes_dirty = true;
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: no custom voice, custom notes inactive"));
        let before = sources(&form);
        form.codriver = false;
        form.invalidate_notes_cache(&ctx, before);
        form.refresh_notes_cache();
        assert_eq!(form.cached_notes_short, format!("{stage}: co-driver off, custom notes inactive"));
        // Unchanged inputs dirty nothing.
        form.cached_notes_dirty = false;
        form.invalidate_notes_cache(&ctx, sources(&form));
        assert!(!form.cached_notes_dirty);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Corner and Distance calls show human labels for the known stems, the bare stem otherwise; `-xx` language
    /// variants stay out of the dropdowns unless chosen. Needs no game data.
    #[test]
    fn corner_and_distance_labels_and_language_variants() {
        use super::{range_label, show_call_file, style_label};
        assert_eq!(style_label("Numeric.ini"), "Numbers, 1 tightest (\"one left\")");
        assert_eq!(style_label("Numeric-Swapped.ini"), "Numbers, 1 tightest (\"left one\")");
        assert_eq!(style_label("Numeric-Descending.ini"), "Numbers, 6 tightest (\"six left\")");
        assert_eq!(style_label("Numeric-Descending-Swapped.ini"), "Numbers, 6 tightest (\"left six\")");
        assert_eq!(style_label("Descriptive.ini"), "Words (\"easy left\", \"hairpin\")");
        assert_eq!(style_label("Rbr.ini"), "Original RBR calls");
        assert_eq!(style_label("Rbr-Enhanced.ini"), "Original RBR calls, more of them");
        assert_eq!(style_label("Mine.ini"), "Mine");
        assert_eq!(range_label("Rbr.ini"), "Standard RBR distances");
        assert_eq!(range_label("Extended.ini"), "Extended, finer distances");
        assert_eq!(range_label("Mine.ini"), "Mine");
        // Variants label as their base stem but stay hidden unless chosen.
        assert_eq!(style_label("Numeric-cz.ini"), style_label("Numeric.ini"));
        assert!(!show_call_file("Numeric-cz.ini", "Numeric.ini"));
        assert!(show_call_file("Numeric-cz.ini", "Numeric-cz.ini"));
        assert!(show_call_file("Numeric.ini", "Numeric.ini"));
        // `Rbr-Enhanced` is no variant: more than two letters after the last dash.
        assert!(show_call_file("Rbr-Enhanced.ini", "Numeric.ini"));
    }

    /// The RSF install's styles and range sets all load the same packages, so they are detected as identical, and
    /// its default voice's preview says every corner and the distance. Needs `RBR_RSF`.
    #[test]
    fn the_rsf_installs_styles_are_identical_and_its_preview_speaks() {
        let Some(rsf) = std::env::var_os("RBR_RSF").map(|v| v.to_string_lossy().into_owned()) else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let Some(mods) = crate::command::CodriverMods::scan(&rsf) else {
            eprintln!("skipping: no Pacenote plugin in RBR_RSF");
            return;
        };
        assert!(!mods.styles.is_empty() && !mods.ranges.is_empty());
        let (styles_differ, ranges_differ) =
            crate::codriver_packs::call_sets_differ(&mods.dir, &mods.styles, &mods.ranges);
        assert!(!styles_differ, "every top-level style points at the same packages");
        assert!(!ranges_differ, "every range set points at the same packages");
        let Some(voice) = mods.voice(None) else {
            eprintln!("skipping: the install names no voice");
            return;
        };
        let preview = rbr_sound::preview::custom_preview(&mods.dir, &voice, &mods.styles[0], &mods.ranges[0]).unwrap();
        assert_eq!(preview.calls.len(), rbr_sound::preview::LADDER.len() + 1, "every corner and the distance");
        assert!(preview.wave.samples.len() > preview.calls.len() * preview.wave.rate as usize / 10);
    }
}

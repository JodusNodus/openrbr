//! The Graphics & sound page: the window, the camera, the HUD, the sound and the renderer's quality.

use egui::{Color32, ComboBox, DragValue};
use rbr_render::camera_prefs;
use rbr_render::coords;
use rbr_render::hud::{Part, Parts};
use rbr_render::mirrors::Rate;
use rbr_render::present::Present;
use rbr_render::renderer::{AntiAliasing, Detail, Reflections};

use crate::pages::{fields, section, DRIVE_FIELDS, FIELD};
use crate::Form;

/// The Quality section's choices: the renderer's additions (`viewer --enhance`), the wet roads (`--enhance wet` and
/// `--reflections`), the detail (`--detail`) and the mirrors (`--mirrors`). Each is kept under its own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Graphics {
    pub lighting: bool,
    /// Our rain, snow and particles (`particles`).
    pub particles: bool,
    /// The scanned asphalt, gravel, snow and ground (`asphalt`, `gravel`, `snow`, `ground`), one switch.
    pub surfaces: bool,
    pub cabin: bool,
    pub contact: bool,
    pub grass: bool,
    /// Our tyre marks, which no preset changes: free, and off until reviewed in play.
    pub marks: bool,
    pub wet_roads: WetRoads,
    pub detail: Detail,
    pub mirrors: Rate,
}

/// The additions always on, as they cost nothing: 16x anisotropic filtering, the soft decals, the flowing water.
pub const ALWAYS_ON: [&str; 3] = ["anisotropy", "decals", "water"];

/// A new install's choices: the High preset.
pub const DEFAULT_GRAPHICS: Graphics = Quality::High.graphics(false);

/// The width of the Quality section's boxes, beside the left column.
const QUALITY_FIELD: f32 = 170.0;

/// The wet roads: as in 1.02, or our film and puddles (`--enhance wet`) reflecting the baked stage and sky (low), and
/// the scenery on screen too, on the wet roads and the water (medium and high, `--reflections`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WetRoads {
    Off,
    Low,
    Medium,
    High,
}

impl WetRoads {
    /// The choices offered, in the launcher's order.
    pub const ALL: [WetRoads; 4] = [WetRoads::High, WetRoads::Medium, WetRoads::Low, WetRoads::Off];

    /// Its name in the settings.
    pub fn name(self) -> &'static str {
        match self {
            WetRoads::Off => "off",
            WetRoads::Low => "low",
            WetRoads::Medium => "medium",
            WetRoads::High => "high",
        }
    }

    pub fn parse(text: &str) -> Option<WetRoads> {
        WetRoads::ALL.into_iter().find(|w| w.name() == text)
    }

    /// The viewer's screen-space reflections for it.
    pub fn reflections(self) -> Reflections {
        match self {
            WetRoads::Off | WetRoads::Low => Reflections::Off,
            WetRoads::Medium => Reflections::Medium,
            WetRoads::High => Reflections::High,
        }
    }

    fn label(self) -> &'static str {
        match self {
            WetRoads::Off => "Off",
            WetRoads::Low => "Low",
            WetRoads::Medium => "Medium",
            WetRoads::High => "High",
        }
    }
}

/// The Quality section's presets (their cost: `notes/renderer.md` "Cost on an M3 Pro").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// The game's look with the free additions.
    Low,
    /// Every addition but our lighting, the costliest.
    Medium,
    High,
    /// High with the dense grass and the Ultra detail.
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];

    /// Its choices, the tyre marks `marks`.
    pub const fn graphics(self, marks: bool) -> Graphics {
        let high = Graphics {
            lighting: true,
            particles: true,
            surfaces: true,
            cabin: true,
            contact: true,
            grass: false,
            marks,
            wet_roads: WetRoads::High,
            detail: Detail::High,
            mirrors: Rate::Every(1),
        };
        match self {
            Quality::Low => Graphics {
                lighting: false,
                particles: false,
                surfaces: false,
                cabin: false,
                contact: false,
                wet_roads: WetRoads::Off,
                mirrors: Rate::Off,
                ..high
            },
            Quality::Medium => {
                Graphics { lighting: false, wet_roads: WetRoads::Medium, mirrors: Rate::Every(3), ..high }
            }
            Quality::High => high,
            Quality::Ultra => Graphics { grass: true, detail: Detail::Ultra, ..high },
        }
    }

    /// The preset `graphics` are, if any.
    pub fn of(graphics: Graphics) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.graphics(graphics.marks) == graphics)
    }

    fn label(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
            Quality::Ultra => "Ultra",
        }
    }
}

/// The render scales the Graphics & sound page offers, percent of the window's width and height (`viewer --render-scale`): the
/// frame is drawn smaller and the post pass scales it up, for high refresh rates at 1440p and above. 100, the first,
/// is the default.
pub const RENDER_SCALES: [u32; 4] = [100, 75, 67, 50];

/// A render scale's label.
fn scale_label(percent: u32) -> String {
    format!("{percent}%")
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

impl Form {
    /// The Graphics & sound page: the window, the camera, the HUD and the sound on the left; the renderer's quality on
    /// the right.
    pub fn graphics_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("graphics").show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(DRIVE_FIELDS);
                    self.view_fields(ui);
                });
                ui.add_space(20.0);
                ui.vertical(|ui| self.quality(ui));
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

    /// The Quality section: the presets, then each choice with its tip on hover.
    fn quality(&mut self, ui: &mut egui::Ui) {
        section(ui, "Quality");
        ui.colored_label(
            Color32::GRAY,
            "Additions beyond the game's look. The stage always draws at its highest quality; sharp distant textures, \
             soft decals and flowing water are always on.",
        );
        ui.add_space(4.0);
        let g = &mut self.graphics;
        fields(ui, "quality", |ui| {
            let tip = "Sets the choices below. Low: the game's look. Medium: all but Lighting, the costliest. High: \
                       all but Dense grass and Ultra detail. Ultra: all. The render scale, anti-aliasing and tyre \
                       marks stay as they are";
            ui.label("Preset").on_hover_text(tip);
            ComboBox::from_id_salt("preset")
                .width(QUALITY_FIELD)
                .selected_text(Quality::of(*g).map_or("Custom", Quality::label))
                .show_ui(ui, |ui| {
                    for preset in Quality::ALL {
                        if ui.selectable_label(Quality::of(*g) == Some(preset), preset.label()).clicked() {
                            *g = preset.graphics(g.marks);
                        }
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = "A physically based sky and light for the time of day, with moving volumetric clouds, haze and \
                       mist, sun shafts, real-time sun shadows and sunlit foliage, ambient occlusion and bounced light \
                       worked out from the stage when it loads, and reflection probes. The costliest choice: up to \
                       half the frame time on a dry day, and a few seconds longer to load";
            ui.label("Lighting").on_hover_text(tip);
            ui.checkbox(&mut g.lighting, "sky, sun and shadows").on_hover_text(tip);
            ui.end_row();

            let tip = "GPU rain and snow around the car, rings on wet roads, sunlit and soft particles, wheel spray \
                       and smoke, and the car's dirt. Costly in heavy rain (the mirrors draw it too), next to nothing \
                       in the dry";
            ui.label("Weather effects").on_hover_text(tip);
            ui.checkbox(&mut g.particles, "rain, snow and particles").on_hover_text(tip);
            ui.end_row();

            let tip = "In wet weather, a film of water and puddles on the roads, the ground about them soaked. Low: \
                       they reflect the stage's baked reflections and the sky. Medium and High: also the scenery on \
                       screen, on the wet roads and on rivers and lakes, Medium grainier. Off: the game's wet roads. \
                       Each step up costs more frame time in the rain";
            ui.label("Wet roads").on_hover_text(tip);
            ComboBox::from_id_salt("wet roads")
                .width(QUALITY_FIELD)
                .selected_text(g.wet_roads.label())
                .show_ui(ui, |ui| {
                    for choice in WetRoads::ALL {
                        ui.selectable_value(&mut g.wet_roads, choice, choice.label());
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = "Asphalt, gravel, snow and the off-road ground get the fine grain and relief of scanned \
                       surfaces, lit by the sun, over the stage's own colours; the ground loses the baked tint that \
                       turns it blue and over-green. Costs little";
            ui.label("Surfaces").on_hover_text(tip);
            ui.checkbox(&mut g.surfaces, "scanned").on_hover_text(tip);
            ui.end_row();

            let tip = "In the cockpit: the sun only through the glass, with the roof, pillars and dash shading each \
                       other, darker footwells and corners, a sheen on the dash, and the headlights' glow at night";
            ui.label("Cabin light").on_hover_text(tip);
            ui.checkbox(&mut g.cabin, "in the cockpit").on_hover_text(tip);
            ui.end_row();

            let tip = "The car's own shape as a soft shadow on the ground and fences under it, in place of the game's \
                       shadow decal";
            ui.label("Car shadow").on_hover_text(tip);
            ui.checkbox(&mut g.contact, "soft").on_hover_text(tip);
            ui.end_row();

            let tip = "A field of grass blades over the grassy ground within 20 m of the road, swaying in the wind, \
                       drawn within 50 m of the camera. Needs Lighting. Costly (the sun's shadows draw it too), and \
                       about 150 MB of video memory";
            ui.label("Dense grass").on_hover_text(tip);
            ui.checkbox(&mut g.grass, "by the road").on_hover_text(tip);
            ui.end_row();

            let tip = "Ultra, for strong GPUs: sun shadows at twice the texels (sharper shadows of posts, cables and \
                       trunks) and sharper reflections in the car's paint and the wet roads. 15 to 40 % more frame \
                       time, and 0.5 GB more video memory on long stages";
            ui.label("Detail").on_hover_text(tip);
            ComboBox::from_id_salt("detail")
                .width(QUALITY_FIELD)
                .selected_text(detail_label(g.detail))
                .show_ui(ui, |ui| {
                    for choice in Detail::ALL {
                        ui.selectable_value(&mut g.detail, choice, detail_label(choice));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = "In the cockpit: the rear-view and door mirrors show the stage, the car and the cars behind, \
                       redrawn this often (each mirror in turn). Costly in the rain, which they draw too. Off: they \
                       only reflect the sky";
            ui.label("Mirrors").on_hover_text(tip);
            ComboBox::from_id_salt("mirrors")
                .width(QUALITY_FIELD)
                .selected_text(mirrors_label(g.mirrors))
                .show_ui(ui, |ui| {
                    for rate in Rate::ALL {
                        ui.selectable_value(&mut g.mirrors, rate, mirrors_label(rate));
                    }
                })
                .response
                .on_hover_text(tip);
            ui.end_row();

            let tip = "Tyre marks as wide as the tread and swept wider in a slide: ruts with berms in gravel, rubber \
                       on tarmac only when sliding, grooves in snow, flattened grass, longer trails. The presets leave \
                       them as they are";
            ui.label("Tyre marks").on_hover_text(tip);
            ui.checkbox(&mut g.marks, "realistic").on_hover_text(tip);
            ui.end_row();
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

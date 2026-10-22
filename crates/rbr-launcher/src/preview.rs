//! The Drive page's previews of the selected stage and car, as the game's quick-race menus show them.
//!
//! The stage: its picture (`rbr_render::stage_picture`), the stage length and the surface. The length is
//! `CMenuMap3DUI::vf0c` (0x584310)'s "%2.1f KM" of `E95_O::M4a0e20` (0x4a0e20).
//! Where the game has neither (its other maps show the warning sprite 0x4df and 0.0 km), we draw the driveline from
//! above (`.trk`, `E09_LoadDriveLine`) and take its length (ours; the game's menu draws a 3D map model of the stage,
//! `FUN_00583cb0`, which is not read). A RallySimFans stage shows its loading picture, else its driveline, with its own
//! length, author and country, and the reason it cannot load.
//!
//! The car: its model turning, rendered by `rbr_render` offscreen, as the game's menus show the car in 3D
//! (`LoadFrontEndCars` in `RichardBurnsRally.ini`, `CPC_Application::vf04` 0x44c890; the menu loads the car with
//! `E09_LoadCar` in `CPC_TrackLoaderDaemon::Vf1C` 0x46f330). An RSF car is drawn with the model RSF gives it, its own or
//! the retail stand-in (`rsf::select`). The frames are rendered on a thread of their own, once per car, and the last
//! few cars are kept, so a selection does not stall the window. The stage pictures and drivelines are read on another
//! thread, the last [`KEEP_STAGES`] kept, so that browsing the map list does not stall it either.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use egui::{Color32, ColorImage, Stroke, TextureHandle, TextureOptions};
use rbr_formats::data::Data;
use rbr_formats::{rsf, stage_files, trk};
use rbr_render::stage_picture::{picture_sprite, rsf_picture, Pictures, Pixels};

use crate::catalog::{Car, Map};

/// The preview column's width, points.
pub const WIDTH: f32 = 384.0;
/// The car turntable: frames per turn, their size and the frame rate shown.
const FRAMES: usize = 60;
const CAR_SIZE: [usize; 2] = [384, 216];
const FPS: f64 = 20.0;
/// The cars whose frames are kept, and the stages whose pictures are.
const KEEP: usize = 3;
const KEEP_STAGES: usize = 32;
/// The camera's horizontal field of view, degrees.
const CAR_FOV: f32 = 40.0;

/// The stage length the menu shows, metres (`E95_O::M4a0e20`, 0x4a0e20); `None` for its 0.
pub fn game_length(map: u32) -> Option<f32> {
    let metres = match map {
        10 | 0x0d | 0x0e | 0x14 => 6100.0,
        0x0b | 0x2a | 0x41 => 7900.0,
        0x0c => 10200.0,
        0x0f => 7700.0,
        0x15 => 6600.0,
        0x16 => 7000.0,
        0x17 => 4800.0,
        0x18 => 5900.0,
        0x19 => 6900.0,
        0x1f => 10100.0,
        0x20 => 6000.0,
        0x21 => 8200.0,
        0x22 => 9500.0,
        0x23 => 10000.0,
        0x24 => 9600.0,
        0x29 => 4500.0,
        0x2b => 5600.0,
        0x2c => 11100.0,
        0x2d | 0x3e => 7800.0,
        0x2e => 4300.0,
        0x33 => 13800.0,
        0x34 => 8700.0,
        0x35 => 6700.0,
        0x36 => 3900.0,
        0x37 => 13700.0,
        0x38 => 4000.0,
        0x3d => 5000.0,
        0x3f => 7100.0,
        0x40 => 8600.0,
        0x42 => 6800.0,
        0x47 => 2200.0,
        _ => return None,
    };
    Some(metres)
}

/// What the stage preview draws.
enum Picture {
    Image(TextureHandle),
    /// The driveline from above, in 0..1 of a square, start first.
    Outline(Vec<[f32; 2]>),
    None,
}

struct StagePreview {
    picture: Picture,
    /// Length, surface, author, country.
    lines: Vec<String>,
    /// Why the stage cannot load.
    problem: Option<String>,
}

/// A stage preview as the stage thread makes it: the picture's pixels, else the driveline if there is one.
struct StageData {
    picture: Result<Pixels, Option<Vec<[f32; 2]>>>,
    lines: Vec<String>,
    problem: Option<String>,
}

/// The car turntable's frames, or why there are none.
type CarFrames = Result<Vec<Frame>, String>;

/// A turntable frame, uploaded once shown.
enum Frame {
    Image(ColorImage),
    Texture(TextureHandle),
}

/// A car to render: its key, the data and the car's folder or RSF pack.
struct CarRequest {
    key: String,
    data: Data,
    car: CarModel,
}

enum CarModel {
    Retail(String),
    Rsf { pack: PathBuf, root: PathBuf },
}

type CarResult = (String, Result<Vec<ColorImage>, String>);

pub struct Previews {
    data: Data,
    /// The last stages shown, newest first.
    stages: VecDeque<(u32, StagePreview)>,
    stage_pending: Option<u32>,
    stage_requests: Sender<Map>,
    stage_results: Receiver<(u32, StageData)>,
    /// Stages read whose textures are not made yet.
    stage_arrived: Vec<(u32, StageData)>,
    cars: VecDeque<(String, CarFrames)>,
    pending: Option<String>,
    /// Rendered cars whose textures are not made yet.
    arrived: Vec<CarResult>,
    requests: Sender<CarRequest>,
    results: Receiver<CarResult>,
}

impl Previews {
    pub fn new(data: Data) -> Previews {
        let (requests, worker_requests) = channel();
        let (worker_results, results) = channel();
        std::thread::Builder::new()
            .name("car preview".into())
            .spawn(move || car_worker(worker_requests, worker_results))
            .expect("a thread");
        let (stage_requests, worker_requests) = channel();
        let (worker_results, stage_results) = channel();
        let mut pictures = StagePictures { data: data.clone(), pictures: Pictures::new(data.clone()) };
        std::thread::Builder::new()
            .name("stage preview".into())
            .spawn(move || {
                while let Ok(mut map) = worker_requests.recv() {
                    // Only the last highlighted stage matters.
                    while let Ok(newer) = worker_requests.try_recv() {
                        map = newer;
                    }
                    let preview = pictures.preview(&map);
                    if worker_results.send((map.number, preview)).is_err() {
                        return;
                    }
                }
            })
            .expect("a thread");
        Previews {
            data,
            stages: VecDeque::new(),
            stage_pending: None,
            stage_requests,
            stage_results,
            stage_arrived: Vec::new(),
            cars: VecDeque::new(),
            pending: None,
            arrived: Vec::new(),
            requests,
            results,
        }
    }

    /// Draws the stage's and the car's previews, one above the other.
    pub fn ui(&mut self, ui: &mut egui::Ui, map: &Map, car: &Car, rsf_root: &str) {
        ui.vertical(|ui| {
            ui.set_width(WIDTH);
            self.stage_ui(ui, map);
            ui.add_space(12.0);
            self.car_ui(ui, car, rsf_root);
        });
    }

    /// Makes the textures of the stages the thread has sent.
    fn take_stages(&mut self, ctx: &egui::Context, arrived: impl IntoIterator<Item = (u32, StageData)>) {
        for (number, data) in arrived {
            if self.stage_pending == Some(number) {
                self.stage_pending = None;
            }
            let picture = match data.picture {
                Ok((w, h, rgba)) => Picture::Image(texture(ctx, &format!("stage {number}"), w, h, &rgba)),
                Err(Some(line)) => Picture::Outline(line),
                Err(None) => Picture::None,
            };
            self.stages.retain(|(n, _)| *n != number);
            self.stages.push_front((number, StagePreview { picture, lines: data.lines, problem: data.problem }));
            self.stages.truncate(KEEP_STAGES);
        }
    }

    /// Draws the stage's picture and its few lines.
    pub fn stage_ui(&mut self, ui: &mut egui::Ui, map: &Map) {
        let arrived: Vec<_> = self.stage_arrived.drain(..).chain(self.stage_results.try_iter()).collect();
        self.take_stages(ui.ctx(), arrived);
        let height = WIDTH * 184.0 / 384.0;
        let Some((_, preview)) = self.stages.iter().find(|(n, _)| *n == map.number) else {
            if self.stage_pending != Some(map.number) {
                self.stage_pending = Some(map.number);
                let _ = self.stage_requests.send(map.clone());
            }
            let (rect, painter) = ui.allocate_painter(egui::vec2(WIDTH, height), egui::Sense::hover());
            painter.rect_filled(rect.rect, 4.0, Color32::from_gray(24));
            ui.label("");
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            return;
        };
        match &preview.picture {
            Picture::Image(texture) => {
                // Centred with its aspect kept (an RSF loading picture is square).
                let (rect, _) = ui.allocate_exact_size(egui::vec2(WIDTH, height), egui::Sense::hover());
                let size = texture.size_vec2();
                let fitted = size * (rect.width() / size.x).min(rect.height() / size.y);
                egui::Image::new(texture).paint_at(ui, egui::Rect::from_center_size(rect.center(), fitted));
            }
            Picture::Outline(points) => {
                let (rect, painter) = ui.allocate_painter(egui::vec2(WIDTH, height), egui::Sense::hover());
                painter.rect_filled(rect.rect, 4.0, Color32::from_gray(24));
                let area = rect.rect.shrink(12.0);
                let side = area.width().min(area.height());
                let origin = area.center() - egui::vec2(side, side) * 0.5;
                let at = |p: &[f32; 2]| origin + egui::vec2(p[0], p[1]) * side;
                let line: Vec<egui::Pos2> = points.iter().map(at).collect();
                painter.add(egui::Shape::line(line, Stroke::new(2.0_f32, Color32::from_rgb(230, 200, 120))));
                if let (Some(start), Some(finish)) = (points.first(), points.last()) {
                    painter.circle_filled(at(start), 4.0, Color32::from_rgb(90, 200, 90));
                    painter.circle_filled(at(finish), 4.0, Color32::from_rgb(220, 70, 70));
                }
            }
            Picture::None => {
                let (rect, painter) = ui.allocate_painter(egui::vec2(WIDTH, height), egui::Sense::hover());
                painter.rect_filled(rect.rect, 4.0, Color32::from_gray(24));
            }
        }
        ui.label(preview.lines.join(" · "));
        if let Some(problem) = &preview.problem {
            ui.colored_label(Color32::from_rgb(230, 120, 80), format!("Cannot load: {problem}"));
        }
    }

    /// Draws the car turning (its frames are rendered on a thread of their own).
    pub fn car_ui(&mut self, ui: &mut egui::Ui, car: &Car, rsf_root: &str) {
        let key = car_key(car);
        self.arrived.extend(self.results.try_iter());
        for (done, frames) in std::mem::take(&mut self.arrived) {
            if let Err(e) = &frames {
                eprintln!("launcher: car preview {done}: {e}");
            }
            if self.pending.as_deref() == Some(done.as_str()) {
                self.pending = None;
            }
            self.cars.retain(|(k, _)| *k != done);
            self.cars.push_front((done, frames.map(|images| images.into_iter().map(Frame::Image).collect())));
            self.cars.truncate(KEEP);
        }
        let size = egui::vec2(WIDTH, WIDTH * CAR_SIZE[1] as f32 / CAR_SIZE[0] as f32);
        match self.cars.iter_mut().find(|(k, _)| *k == key).map(|(_, f)| f) {
            Some(Ok(frames)) if !frames.is_empty() => {
                let time = ui.input(|i| i.time);
                let k = (time * FPS) as usize % frames.len();
                // Each frame is uploaded when first shown, so a new car does not upload its whole turn at once.
                if let Frame::Image(image) = &mut frames[k] {
                    let image = std::mem::take(image);
                    frames[k] =
                        Frame::Texture(ui.ctx().load_texture(format!("{key} {k}"), image, TextureOptions::LINEAR));
                }
                if let Frame::Texture(texture) = &frames[k] {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                }
                ui.ctx().request_repaint_after(Duration::from_secs_f64(1.0 / FPS));
            }
            Some(Err(e)) => {
                let e = e.clone();
                let (rect, painter) = ui.allocate_painter(size, egui::Sense::hover());
                painter.rect_filled(rect.rect, 4.0, Color32::from_gray(24));
                ui.colored_label(Color32::from_rgb(230, 120, 80), format!("No car model: {e}"));
            }
            _ => {
                self.request(car, &key, rsf_root);
                let (rect, painter) = ui.allocate_painter(size, egui::Sense::hover());
                painter.rect_filled(rect.rect, 4.0, Color32::from_gray(24));
                let font = egui::FontId::proportional(13.0);
                painter.text(rect.rect.center(), egui::Align2::CENTER_CENTER, "Loading the car…", font, Color32::GRAY);
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
        }
        if let Some(Ok(Some(stand_in))) = car.rsf.as_ref().map(|r| &r.model) {
            ui.label(egui::RichText::new(format!("Stand-in model: {stand_in}")).color(Color32::GRAY));
        }
    }

    /// Asks the worker for the car's frames, once.
    fn request(&mut self, car: &Car, key: &str, rsf_root: &str) {
        if self.pending.as_deref() == Some(key) {
            return;
        }
        let model = match &car.rsf {
            None => CarModel::Retail(car.name.clone()),
            Some(rsf) => match (&rsf.pack, &rsf.model) {
                (Err(e), _) | (_, Err(e)) => {
                    self.cars.push_front((key.to_owned(), Err(e.clone())));
                    self.cars.truncate(KEEP);
                    return;
                }
                (Ok(pack), Ok(_)) => CarModel::Rsf { pack: pack.clone(), root: PathBuf::from(rsf_root.trim()) },
            },
        };
        self.pending = Some(key.to_owned());
        let _ = self.requests.send(CarRequest { key: key.to_owned(), data: self.data.clone(), car: model });
    }

    /// Waits for the stage's preview and the car's frames, which the next draw shows (the `--screenshot` pages).
    pub fn wait_for(&mut self, map: &Map, car: &Car, rsf_root: &str) {
        self.stage_pending = Some(map.number);
        let _ = self.stage_requests.send(map.clone());
        self.stage_arrived.extend(self.stage_results.recv_timeout(Duration::from_secs(60)));
        self.request(car, &car_key(car), rsf_root);
        self.arrived.extend(self.results.recv_timeout(Duration::from_secs(120)));
    }
}

/// The stage thread's data and its menu bank.
struct StagePictures {
    data: Data,
    pictures: Pictures,
}

impl StagePictures {
    fn preview(&mut self, map: &Map) -> StageData {
        let data = self.data.clone();
        let driveline = || first_driveline(&data, map);
        let (picture, length) = match &map.rsf {
            Some(stage) => {
                let picture = rsf_picture(&stage.folder, stage.id);
                let line = driveline();
                let length = stage.length.map(|km| km * 1000.0).or(line.as_ref().map(|l| l.1));
                (picture.ok_or(line.map(|l| l.0)), length)
            }
            None => {
                let picture = picture_sprite(map.number).and_then(|sprite| self.pictures.sprite(sprite));
                match (picture, game_length(map.number)) {
                    (Some(picture), Some(length)) => (Ok(picture), Some(length)),
                    (picture, length) => {
                        let line = driveline();
                        let length = length.or(line.as_ref().map(|l| l.1));
                        (picture.ok_or(line.map(|l| l.0)), length)
                    }
                }
            }
        };
        let mut lines = Vec::new();
        if let Some(metres) = length {
            lines.push(format!("{:.1} km", metres / 1000.0));
        }
        lines.push(crate::stage_browser::SURFACES[map.surface().min(2) as usize].to_owned());
        if let Some(stage) = &map.rsf {
            lines.extend(stage.country.clone());
            lines.extend(stage.author.as_ref().map(|a| format!("by {a}")));
        }
        StageData { picture, lines, problem: map.rsf.as_ref().and_then(|s| s.problem.clone()) }
    }
}

fn car_key(car: &Car) -> String {
    match &car.rsf {
        Some(rsf) => format!("rsf {}", rsf.id),
        None => car.name.clone(),
    }
}

fn texture(ctx: &egui::Context, name: &str, width: usize, height: usize, rgba: &[u8]) -> TextureHandle {
    ctx.load_texture(name, ColorImage::from_rgba_unmultiplied([width, height], rgba), TextureOptions::LINEAR)
}

/// The driveline of the map's first stage variant from above, fitted into 0..1 with its aspect kept,
/// and its length in metres.
fn first_driveline(data: &Data, map: &Map) -> Option<(Vec<[f32; 2]>, f32)> {
    let stage = map.stages.first()?;
    let bytes = stage_files::read(data, &stage.name, "trk").ok()?;
    let records = trk::parse(&bytes).ok()?.chunks.into_iter().find_map(|c| match c.payload {
        trk::ChunkPayload::DriveLine(d) => Some(d.records),
        _ => None,
    })?;
    let points: Vec<[f32; 2]> = records.iter().map(|r| [r.vectors[0].x, r.vectors[0].y]).collect();
    let step = |a: &trk::Vec3, b: &trk::Vec3| ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt();
    let length = records.windows(2).map(|w| step(&w[0].vectors[0], &w[1].vectors[0])).sum();
    Some((fit(&points), length))
}

/// `points` scaled into 0..1 with their aspect kept, centred, y down.
fn fit(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in points {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-3);
    let pad = [(span - (hi[0] - lo[0])) * 0.5, (span - (hi[1] - lo[1])) * 0.5];
    points.iter().map(|p| [(p[0] - lo[0] + pad[0]) / span, 1.0 - (p[1] - lo[1] + pad[1]) / span]).collect()
}

/// The worker: renders each requested car's turntable, the latest request first, and sends its frames back.
fn car_worker(requests: Receiver<CarRequest>, results: Sender<CarResult>) {
    let mut gpu = None;
    while let Ok(mut request) = requests.recv() {
        // Only the last selection matters.
        while let Ok(newer) = requests.try_recv() {
            request = newer;
        }
        let frames = render_car(&mut gpu, request.data, &request.car);
        if results.send((request.key, frames)).is_err() {
            return;
        }
    }
}

fn render_car(
    gpu: &mut Option<rbr_render::renderer::Gpu>,
    data: Data,
    car: &CarModel,
) -> Result<Vec<ColorImage>, String> {
    use rbr_render::{camera::Orbit, car as render_car, renderer, stage::Selection};
    let (data, name) = match car {
        CarModel::Retail(name) => (data, name.clone()),
        CarModel::Rsf { pack, root } => {
            let selected = rsf::select(data, &pack.display().to_string(), rsf::SLOT, Some(root))?;
            let name = selected.folder().to_owned();
            (selected.data, name)
        }
    };
    let mut car = render_car::load(&data, &name, render_car::placement_at(None)).map_err(|e| e.to_string())?;
    // Whole windows, as the viewer's car alone.
    car.crack(&rbr_render::damage::Cracks::new(car.windows()));
    let car = car.at_rest();
    let (lo, hi) = car.bounds().ok_or("the car has no geometry")?;
    let fov = CAR_FOV.to_radians();
    let mut orbit = Orbit::framing(lo, hi, fov);
    orbit.pitch = 0.2;
    let (stage, _) = car.into_stage(Selection::default());
    if gpu.is_none() {
        let instance = rbr_render::renderer::instance();
        *gpu = Some(renderer::request_gpu(&instance, None).map_err(|e| e.to_string())?);
    }
    let gpu = gpu.as_ref().expect("set above");
    let mut renderer = renderer::Renderer::new(gpu, wgpu::TextureFormat::Rgba8Unorm, &stage, Default::default());
    let size = (CAR_SIZE[0] as u32, CAR_SIZE[1] as u32);
    let aspect = size.0 as f32 / size.1 as f32;
    let start = orbit.yaw;
    (0..FRAMES)
        .map(|k| {
            // Turning to the left, as seen from above.
            orbit.yaw = start - std::f32::consts::TAU * k as f32 / FRAMES as f32;
            let (_, mut rgba) = renderer
                .render_offscreen(size, orbit.view_projection(fov, aspect), orbit.sky_view_projection(fov, aspect))
                .map_err(|e| e.to_string())?;
            // The game's back buffer has no alpha; the draws leave 0 in it on the number plates (the viewer's
            // `write_png`).
            rgba.chunks_exact_mut(4).for_each(|texel| texel[3] = 255);
            Ok(ColorImage::from_rgba_unmultiplied(CAR_SIZE, &rgba))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_shows_the_game_lengths() {
        assert_eq!(game_length(54), Some(3900.0));
        assert_eq!(game_length(5), None);
    }

    #[test]
    fn a_driveline_fits_the_square_with_its_aspect() {
        let fitted = fit(&[[0.0, 0.0], [10.0, 5.0]]);
        assert_eq!(fitted, [[0.0, 0.75], [1.0, 0.25]]);
    }
}

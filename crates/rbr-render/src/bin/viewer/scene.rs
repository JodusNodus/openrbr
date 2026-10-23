//! Loading what the viewer shows, and the reports of what loaded.

use crate::args::{Args, HudOptions};
use crate::loading::Step;
use rbr_formats::data::Data;
use rbr_recording::detach::PartPose;
use rbr_render::camera::Orbit;
use rbr_render::camera_prefs;
use rbr_render::car::{self, CarReport};
use rbr_render::characters::incar::Crew;
use rbr_render::characters::{self, Characters};
use rbr_render::damage::{Cracks, MeshDamage};
use rbr_render::dials::Dials;
use rbr_render::fences;
use rbr_render::ghost;
use rbr_render::glass;
use rbr_render::hud::model::HudModel;
use rbr_render::live::{Controls, Live};
use rbr_render::mirrors;
use rbr_render::particles;
use rbr_render::playback::{Drive, Playback};
use rbr_render::relight;
use rbr_render::renderer::{Enhance, Gpu, Renderer};
use rbr_render::stage::{self, Mesh, Stage};
use rbr_render::stage_probe::StageProbe;
use rbr_render::views::Mounts;
use std::path::Path;

/// What the viewer shows: a stage, with the car on it when `--car` names one, or the car alone under
/// an orbit camera.
pub struct Scene {
    pub stage: Stage,
    /// The orbit camera of a car shown alone. `None` uses the free-fly camera of a window.
    pub orbit: Option<Orbit>,
    /// The car is at the start: the start view is the chase view.
    pub chase: bool,
    /// A recorded drive on the car (`--drive`), or the live one (`--live`); the camera follows it.
    pub playback: Option<Playback>,
    pub live: Option<Live>,
    /// The car's fixed camera views, with the playback.
    pub mounts: Option<Mounts>,
    /// The player's camera choices for the car.
    pub cameras: camera_prefs::Session,
    /// The particles a drive throws.
    pub particles: Option<particles::Drawn>,
    /// The stage's characters (`rbr_render::characters`).
    pub characters: Option<Characters>,
    /// The driver and co-driver in the car, with the playback.
    pub crew: Option<Crew>,
    /// The windscreen's water and wipers in the in-car views.
    pub glass: Option<glass::Drawn>,
    /// The weather's rain or snow.
    pub precipitation: Option<particles::Precipitation>,
    /// The fences a drive goes through.
    pub fences: Option<fences::Drawn>,
    /// The cockpit's working dials, with a drive.
    pub dials: Option<Dials>,
    pub hud: HudOptions,
    /// The ghost car and the ghost recording, with a drive.
    pub ghost: Option<ghost::Session>,
    /// The stage's clock (`--clock`), moving the light on with the drive's time.
    pub day_clock: Option<relight::Clock>,
    /// How much sun gets into the cabin, with the cabin light (`car::cabin::SunIn`).
    pub cabin_sun: Option<car::cabin::SunIn>,
}

impl Scene {
    /// The renderer for the scene: its stage, the car, the weather's rain or snow and the live drive's scenery poses.
    /// `progress` takes the share of the stage's meshes made ([`Renderer::with_progress`]).
    pub fn renderer(
        &self,
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        enhance: Enhance,
        render_scale: f32,
        progress: &mut dyn FnMut(f32),
    ) -> Renderer {
        let mut renderer = Renderer::with_progress(gpu, format, &self.stage, enhance, progress);
        renderer.set_render_scale(render_scale);
        if let Some(playback) = &self.playback {
            let crew: Vec<usize> = self.crew.iter().flat_map(|crew| crew.slots()).collect();
            renderer.set_car(&playback.car_meshes(), &crew);
        }
        renderer.set_precipitation(self.precipitation);
        renderer.set_cabin_sun(self.cabin_sun.clone());
        if let Some(live) = &self.live {
            renderer.set_interactive_poses(&live.interactive_poses);
        }
        renderer
    }
}

/// [`load_scene`], with its report, at the `--at` frame.
pub fn load(args: &Args, data: &Data, progress: &mut dyn FnMut(Step)) -> Result<Scene, String> {
    let mut scene = load_scene(args, data, progress)?;
    report_load(&scene.stage);
    match (args.at, &mut scene.playback, &scene.live) {
        (Some(step), Some(playback), None) => playback.seek_to_step(step),
        // A live screenshot shows its last frame, `--at` payloads after the first.
        (Some(_), Some(playback), Some(_)) => playback.step(playback.samples().len() as isize),
        _ => {}
    }
    Ok(scene)
}

/// The stage (or the car alone) with the car on it, telling `progress` each [`Step`] as it starts. The car is loaded
/// after the stage's textures, which [`Stage::load_textures`] replaces.
fn load_scene(args: &Args, data: &Data, progress: &mut dyn FnMut(Step)) -> Result<Scene, String> {
    let error = |e: car::Error| e.to_string();
    let rsf_id = args.rsf_ghost_car.as_ref().map(|(id, _)| *id);
    let car_key = args.car.as_deref().map(|car| camera_prefs::car_key(car, rsf_id));
    let cameras = camera_prefs::Session::load(args.cameras.as_deref(), car_key);
    if (args.drive.is_some() || args.live.is_some()) && (args.stage.is_none() || args.car.is_none()) {
        return Err("--drive and --live need a stage and --car".to_owned());
    }
    if args.live.is_some() && args.drive.is_some() {
        return Err("--live takes no --drive".to_owned());
    }
    if args.view.is_some() && args.drive.is_none() && args.live.is_none() {
        return Err("--view needs --drive or --live".to_owned());
    }
    if (args.ghost.is_some() || args.save_ghost.is_some()) && args.drive.is_none() && args.live.is_none() {
        return Err("--ghost and --save-ghost need --drive or --live".to_owned());
    }
    if args.best_ghost.is_some() && args.live.is_none() {
        return Err("--best-ghost needs --live".to_owned());
    }
    match (&args.stage, &args.car) {
        (None, Some(name)) => {
            progress(Step::Car);
            let mut car = car::load(data, name, car::placement_at(None)).map_err(error)?;
            if args.enhance.cabin {
                car.add_inner_glass();
            }
            damage_static(&mut car, args);
            let car = car.at_rest();
            let (lo, hi) = car.bounds().ok_or("the car has no geometry")?;
            let orbit = Orbit::framing(lo, hi, args.fov);
            let (stage, report) = car.into_stage(args.selection);
            report_car(&report);
            Ok(Scene {
                stage,
                orbit: Some(orbit),
                chase: false,
                playback: None,
                live: None,
                mounts: None,
                cameras: camera_prefs::Session::default(),
                particles: None,
                characters: None,
                crew: None,
                glass: None,
                precipitation: None,
                fences: None,
                dials: None,
                hud: args.hud,
                ghost: None,
                day_clock: None,
                cabin_sun: None,
            })
        }
        (stage_name, car_name) => {
            let name = stage_name.as_deref().unwrap_or(stage::DEFAULT_STAGE);
            let maps = data.install().join("Maps");
            progress(Step::Stage);
            let mut stage = stage::load(&maps, name, args.selection).map_err(|e| e.to_string())?;
            stage.atmosphere.fog = args.fog;
            // The sun's shadows take the light maps' place (`sun_shadow.w` in `shader.wgsl`): not loaded.
            if args.enhance.sun {
                stage.texture_names.retain(|texture, _| texture.set != stage::TextureSet::Shadow);
            }
            stage.load_textures(&maps).map_err(|e| e.to_string())?;
            let day_clock = Some(light(args, &mut stage, name));
            if let Some(dir) = &args.textures {
                eprintln!("texture pack {}: {} textures replaced", dir.display(), stage.apply_texture_pack(dir));
            }
            if args.enhance.decals {
                rbr_render::decals::blend_ground_decals(&mut stage);
            }
            progress(Step::Characters);
            let mut characters = load_characters(data, name, &mut stage, args);
            // Before the car: the playback keeps the car's mesh indices.
            let fences = if args.fences { fences::load(data, &mut stage) } else { None };
            let (mut playback, mut live, mut mounts, mut crew, mut car_bounds, mut ghost, mut dials) =
                (None, None, None, None, None, None, None);
            let mut cabin_sun = None;
            // The `--at` frames that first list a detached part (`start_live`).
            let mut detaching = Vec::new();
            if let Some(car_name) = car_name {
                match (&args.drive, &args.live) {
                    (None, None) => {
                        progress(Step::Car);
                        let mut car = car::load(data, car_name, car::placement_at(stage.start)).map_err(error)?;
                        damage_static(&mut car, args);
                        report_car(&car.at_rest().add_to_stage(&mut stage));
                    }
                    (drive, command) => {
                        let drive = match (drive, command) {
                            (Some(path), _) => {
                                Drive::parse(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)?
                            }
                            (None, Some(command)) => {
                                progress(Step::Physics);
                                let command = with_start_time(command, day_clock.as_ref());
                                let (process, drive, detached) = start_live(args, &command)?;
                                (live, detaching) = (Some(process), detached);
                                drive
                            }
                            (None, None) => unreachable!(),
                        };
                        progress(Step::Car);
                        let mut car = car::load(data, car_name, car::placement_at(None)).map_err(error)?;
                        if args.enhance.cabin {
                            car.add_inner_glass();
                        }
                        let (model, damage) = (car.meshes.clone(), car.damage.clone());
                        let (flares, (lo, hi)) = (car.flares.clone(), car.bounds().ok_or("the car has no geometry")?);
                        report_car(&car.add_to_stage(&mut stage));
                        // Both sets are in the stage before the playback looks up their meshes: adding sorts it.
                        let cabin = args.enhance.cabin.then_some(model.as_slice());
                        let (mut interior, mut interior_damage, sun_in) =
                            add_interior(data, car_name, &mut stage, cabin)?;
                        cabin_sun = sun_in;
                        car::follow_dents(&interior, &mut interior_damage, &model, &damage);
                        let rsf = args.rsf_install.as_deref().filter(|_| args.dials && args.rsf_car.is_some());
                        dials = rsf.and_then(|root| load_dials(root, car_name, &mut stage));
                        for _ in 1..args.gauge_mode {
                            dials.iter_mut().for_each(Dials::next_mode);
                        }
                        if let Some(root) = rsf.filter(|_| args.enhance.cabin && dials.is_some()) {
                            mark_gauges(root, car_name, &mut stage, &mut interior);
                        }
                        let mirrors = (args.enhance.mirrors != mirrors::Rate::Off)
                            .then(|| -> Result<_, String> {
                                let eye = Mounts::load(data, car_name, lo, hi, &model, &interior)?.cockpit.eye;
                                let found = mirrors::add(&mut stage, &mut interior, &mut interior_damage, eye);
                                eprintln!(
                                    "mirrors: {} found, {} face sets not flat",
                                    found.mirrors.len(),
                                    found.unfitted.len()
                                );
                                Ok(found.mirrors)
                            })
                            .transpose()?;
                        crew = load_crew(data, car_name, &mut stage);
                        let mut loaded = Playback::load(data, car_name, &stage, &model, &damage, drive)?;
                        let probe = StageProbe::load(data, name).map(std::sync::Arc::new);
                        if let Some(characters) = &mut characters {
                            characters.set_probe(probe.clone());
                        }
                        loaded.set_probe(probe);
                        loaded.add_interior(&stage, &interior, &interior_damage)?;
                        loaded.set_mirrors(mirrors.unwrap_or_default());
                        loaded.add_flares(&stage, &flares)?;
                        if let Some(zones) = &args.damage {
                            loaded.set_damage(zones);
                        }
                        for &(code, index) in &args.cracks {
                            loaded.on_damage(code, index);
                        }
                        if let Some(process) = &live {
                            for (frame, poses) in &detaching {
                                loaded.step(frame - loaded.index() as isize);
                                loaded.set_detached(poses);
                            }
                            take_damage(&mut loaded, process);
                        }
                        let options = ghost::Options {
                            play: args.ghost.as_deref(),
                            save: args.save_ghost.as_deref(),
                            best: args.best_ghost.as_deref(),
                            weather: args.ghost_weather,
                            live: live.is_some(),
                            rsf: ghost::Rsf {
                                save: args.rsf_ghosts.as_deref(),
                                install: args.rsf_install.as_deref(),
                                car: args.rsf_ghost_car.as_ref().map(|(id, group)| (*id, group.as_deref())),
                            },
                        };
                        ghost = Some(ghost::Session::load(data, &mut stage, name, car_name, &loaded, options)?);
                        let mut car_mounts = Mounts::load(data, car_name, lo, hi, &model, &interior)?;
                        cameras.apply(&mut car_mounts, &mut loaded);
                        playback = Some(loaded);
                        mounts = Some(car_mounts);
                        car_bounds = Some((lo, hi));
                    }
                }
            }
            progress(Step::Particles);
            let mut particles = match (&playback, car_name) {
                (Some(_), Some(car)) if args.particles => particles::Drawn::load(data, &stage, Some(car)),
                _ => None,
            };
            if let (Some(drawn), true) = (&mut particles, args.enhance.marks) {
                drawn.particles.lay_tyre_marks();
            }
            // A recording's lamps and tyre payloads, when given (`particles::sidecar`).
            if let (Some(_), Some(playback)) = (&args.drive, &mut playback) {
                let recorded = particles::sidecar::events();
                playback.set_recorded_lamps(&particles::recorded_lamps(&recorded));
                if let Some(drawn) = &mut particles {
                    drawn.particles.set_recorded(recorded);
                }
            }
            let section = stage.atmosphere.section.as_deref();
            // `--enhance particles` keeps its rule by the stage's wetness; otherwise the game's by the preset.
            let precipitation = if args.enhance.particles {
                particles.as_ref().map_or_else(
                    || particles::Precipitation::by_wetness(section, stage.selection.weather, true, true),
                    |p| p.particles.precipitation_by_wetness(),
                )
            } else {
                particles
                    .as_ref()
                    .map_or_else(|| particles::Precipitation::of(section, true, true), |p| p.particles.precipitation())
            };
            // The renderer draws the rain and snow itself under `--enhance particles`.
            if let (Some(drawn), true) = (&mut particles, args.enhance.particles) {
                drawn.particles.drop_weather();
                drawn.particles.light_chimneys();
                drawn.particles.use_wheel_spray(args.enhance.wet);
            }
            let glass = match (&playback, car_name) {
                (Some(playback), Some(car)) => glass::Drawn::load(data, car, precipitation, playback, args.wipers),
                _ => None,
            };
            if let Some(characters) = &mut characters {
                characters.locate(&stage);
                if let (Some(glass), Some(playback)) = (&glass, &playback) {
                    characters.set_windscreen(glass.windscreen().clone(), playback.model_to_car());
                }
            }
            if let Some(crew) = &mut crew {
                crew.locate(&stage);
            }
            let fences = fences
                .zip(car_bounds)
                .and_then(|(loaded, car)| fences::Drawn::new(loaded, &stage, car))
                .zip(car_name.as_deref().zip(playback.as_ref()))
                .map(|(drawn, (car, playback))| drawn.with_car_and_wind(data, car, name, playback));
            Ok(Scene {
                stage,
                orbit: None,
                chase: car_name.is_some(),
                playback,
                live,
                mounts,
                cameras,
                particles,
                characters,
                crew,
                glass,
                precipitation,
                fences,
                dials,
                hud: args.hud,
                ghost,
                day_clock,
                cabin_sun,
            })
        }
    }
}

/// An `--at` frame, and the detached parts' poses it first lists.
type Detaching = (isize, Vec<PartPose>);

/// The live process (`--live`) and its drive: the first frame, then the `--at` frames, with the frames that first list
/// a detached part, so that it starts where its body left the car. A screenshot steps the process in lockstep
/// (`Live::lockstep`), a window drives it in real time.
/// The live command with the start board's clock at the hour the stage is drawn at (`--start-time`), unless it names
/// one or races a championship stage (the season's clock).
fn with_start_time(command: &[String], clock: Option<&relight::Clock>) -> Vec<String> {
    let mut command = command.to_vec();
    let named = command.iter().any(|a| a == "--start-time" || a == "--championship");
    if let (Some(clock), false) = (clock, named) {
        let minutes = (clock.start * 60.0).round() as u32 % (24 * 60);
        command.extend(["--start-time".to_owned(), format!("{}:{:02}", minutes / 60, minutes % 60)]);
    }
    command
}

fn start_live(args: &Args, command: &[String]) -> Result<(Live, Drive, Vec<Detaching>), String> {
    let mut process = match &args.screenshot {
        Some(_) => Live::lockstep(command)?,
        None if args.at.is_some() || args.at_drive.is_some() => {
            return Err("--at and --at-drive with --live need --screenshot".to_owned())
        }
        None => Live::spawn(command)?,
    };
    process.hud_model = HudModel::with_stack(args.hud.stack);
    let first = process.first_frame()?;
    let mut drive = Drive::parse_with(&first, process.clock)?;
    // `--at N`: N more frames with no keys pressed, before the screenshot; with `--at-drive` each frame sends the
    // recording's control of the step it feeds, so the first one goes through the countdown's frames, which step nothing.
    let recorded = match &args.at_drive {
        Some(path) => recorded_controls(path)?,
        None => Vec::new(),
    };
    let neutral = Controls::default().payload();
    let (mut stepped, mut seen, mut detaching) = (0, Vec::new(), Vec::new());
    for k in 0..args.at.unwrap_or(0) {
        let frame = process.step_payload(recorded.get(stepped).unwrap_or(&neutral))?;
        stepped += rbr_recording::Frames::new(&frame).map(|f| f.map_or(0, |f| f.substeps as usize)).sum::<usize>();
        drive.extend(Drive::parse_with(&frame, process.clock)?);
        if process.detached.iter().any(|p| !seen.contains(&p.part)) {
            seen.extend(process.detached.iter().map(|p| p.part));
            detaching.push((k as isize + 1, process.detached.clone()));
        }
    }
    Ok((process, drive, detaching))
}

/// The stage's light and the clock that moves it on (`rbr_render::relight`): relit at `--clock`'s time, else under the
/// bake's own sun (`relight::own_sun`). `--no-relight` keeps the baked light.
fn light(args: &Args, stage: &mut Stage, name: &str) -> relight::Clock {
    let bake = relight::fit(stage);
    eprintln!(
        "relight: bake sun {:.3}, strength {:.3}, r {:.3}, open {:.3}",
        bake.bake_sun, bake.strength, bake.r, bake.open
    );
    stage.atmosphere.sun_direction = relight::own_sun(&bake, stage.atmosphere.sun_direction);
    // The clock and the relight anchor on the stage's own sun (the section's where the fit does not hold).
    let bake = relight::Relight { bake_sun: -stage.atmosphere.sun_direction, ..bake };
    // Every stage draws through the relight (`relit` in `shader.wgsl`).
    stage.atmosphere.relight = args.relight.then_some(bake);
    let (map, letter) = clock_stage(name);
    let letter = stage.atmosphere.section.as_deref().map_or(letter, section_letter);
    let base = stage.atmosphere.clone();
    let clock = relight::Clock::new(base, bake, map, letter, args.clock, args.clock_rate, args.relight)
        .physical(args.enhance.sky);
    let (atmosphere, day) = clock.atmosphere(0.0);
    eprintln!(
        "clock: {:.1} h, sun {:.1} degrees up, bake at {:.1} h ({letter}), light {:.2}, ambient {:.2}, x{}",
        clock.start, day.elevation, clock.bake_hour, day.sun, day.ambient, clock.rate
    );
    stage.atmosphere = atmosphere;
    clock
}

/// The stage's characters, with their meshes in `stage`; `None` (reported) when they do not load.
fn load_characters(data: &Data, name: &str, stage: &mut Stage, args: &Args) -> Option<Characters> {
    let options = characters::load::Options { birdstrike: args.birdstrike, seed: args.seed };
    match Characters::load_with(data, name, stage, options) {
        Ok(mut characters) => {
            characters.set_rescue_step(args.rescue_at);
            let r = &characters.report;
            eprintln!(
                "characters: {} of {} items placed ({} other group variants, {} declined by Load, {} scripted events), \
                 failed {:?}, unmodelled calls {:?}",
                r.spawned, r.items, r.other_variant, r.declined, r.scripted, r.failed, r.unknown_calls
            );
            Some(characters)
        }
        Err(e) => {
            eprintln!("characters: not loaded: {e}");
            None
        }
    }
}

/// The car's driver and co-driver, with their meshes in `stage`; `None` (reported) when they do not load.
fn load_crew(data: &Data, car: &str, stage: &mut Stage) -> Option<Crew> {
    Crew::load(data, car, stage).map_err(|e| eprintln!("crew: not loaded: {e}")).ok()
}

/// The car's working dials, reported; `None` when it has no gauges.
fn load_dials(root: &Path, car: &str, stage: &mut Stage) -> Option<Dials> {
    let (dials, skipped) = Dials::load(root, car, stage);
    for skip in &skipped {
        eprintln!("dials: config not drawn: {skip:?}");
    }
    if dials.is_some() {
        eprintln!("dials: RBRHUD gauges of {car} drawn");
    }
    dials
}

/// A car without a drive dented and cracked as `--damage` and `--crack` say.
fn damage_static(car: &mut car::Car, args: &Args) {
    if let Some(zones) = &args.damage {
        car.dent(zones);
    }
    let mut cracks = Cracks::new(car.windows());
    for &(code, index) in &args.cracks {
        cracks.on_damage(code, index);
    }
    car.crack(&cracks);
}

/// The interior set's meshes, what damage does to them, and with the cabin light how much sun gets into the cabin.
type Interior = (Vec<Mesh>, Vec<MeshDamage>, Option<car::cabin::SunIn>);

/// Adds the car's interior set, for the in-car views, to `stage` and returns its meshes and what damage does to them.
/// With the cabin light, `exterior`, the exterior set's meshes, shade it (`car::cabin`), and it returns how much sun
/// gets into the cabin.
fn add_interior(data: &Data, car_name: &str, stage: &mut Stage, exterior: Option<&[Mesh]>) -> Result<Interior, String> {
    let mut interior = car::load_interior(data, car_name, car::placement_at(None))
        .map_err(|e| e.to_string())?
        .after_textures_of(stage);
    let sun_in = exterior.and_then(|exterior| car::cabin::bake(&mut interior.meshes, &interior.damage, exterior));
    let meshes = (interior.meshes.clone(), interior.damage.clone(), sun_in);
    let report = interior.add_to_stage(stage);
    eprintln!(
        "car interior set (in-car views): {} meshes drawn, {} missing textures, {} bad triangles",
        report.meshes_drawn,
        report.missing_textures.len(),
        report.bad_triangles
    );
    Ok(meshes)
}

/// Marks the dial faces of `interior`, whose meshes are in `stage`, for the cabin light (`car::cabin::mark_gauges`), in
/// both.
fn mark_gauges(root: &Path, car_name: &str, stage: &mut Stage, interior: &mut [Mesh]) {
    let faces = car::cabin::gauge_faces(root, car_name, stage);
    for mesh in interior {
        let mut marked = mesh.clone();
        if car::cabin::mark_gauges(&mut marked, &faces) {
            if let Some(staged) = stage.meshes.iter_mut().find(|m| *m == mesh) {
                *staged = marked.clone();
            }
            *mesh = marked;
        }
    }
}

fn report_car(report: &CarReport) {
    eprintln!(
        "car: {} meshes drawn, {} skipped, {} missing textures, {} bad triangles",
        report.meshes_drawn,
        report.skipped.len(),
        report.missing_textures.len(),
        report.bad_triangles
    );
    for (name, reason) in &report.skipped {
        eprintln!("skipped {name}: {reason}");
    }
    for (class, files) in &report.shader_files {
        eprintln!("shader {class}: {} {}", files.vertex.display(), files.pixel.display());
    }
    for request in &report.missing_textures {
        eprintln!("missing texture: {request}");
    }
    eprintln!(
        "base texture only (open: dirt and the game's paint and reflection; the scrape is drawn on CarPaint and CarPaintInt): {:?}",
        report.base_only_shaders
    );
    if !report.unknown_shaders.is_empty() {
        eprintln!("unknown shader classes: {:?}", report.unknown_shaders);
    }
}

/// The map number and variant letter of `track-NN_X` (0 and `O` otherwise).
fn clock_stage(name: &str) -> (u32, char) {
    rbr_formats::stage_files::parts(name).unwrap_or((0, 'O'))
}

/// The bake's letter of a `TrackSettings.ini` section: `54E_crisp_clear` is `E`, `54O_E_norain_lightcloud` is `E`.
fn section_letter(section: &str) -> char {
    let rest = section.trim_start_matches(|c: char| c.is_ascii_digit());
    let mut chars = rest.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some('O' | 'S'), Some('_'), Some(letter)) => letter,
        (Some(letter), _, _) => letter,
        _ => 'O',
    }
}

fn report_load(stage: &Stage) {
    eprintln!(
        "atmosphere: TrackSettings.ini section {}",
        stage.atmosphere.section.as_deref().unwrap_or("none (defaults)")
    );
    eprintln!(
        "{}: {} meshes, {} draws, {} triangles, {} textures",
        stage.name,
        stage.meshes.len(),
        stage.draw_count(),
        stage.triangle_count(),
        stage.textures.len()
    );
    for request in &stage.report.missing_textures {
        eprintln!("missing texture: {request}");
    }
    eprintln!(
        "missing textures: {}, bad triangles: {}, skipped draws: {}",
        stage.report.missing_textures.len(),
        stage.report.bad_triangles,
        stage.report.skipped_draws
    );
    eprintln!(
        "water items parsed/drawn/withheld: {}/{}/{}; reflection items parsed/drawn/withheld: {}/{}/{}",
        stage.report.water_items_parsed,
        stage.report.water_items_drawn,
        stage.report.water_items_not_drawn,
        stage.report.reflection_items_parsed,
        stage.report.reflection_items_drawn,
        stage.report.reflection_items_not_drawn
    );
    eprintln!(
        "interactive draw records parsed/drawn/withheld: {}/{}/{}; visibility instances parsed/drawn/withheld: {}/{}/{}",
        stage.report.interactive_draw_items_parsed,
        stage.report.interactive_draw_items_drawn,
        stage.report.interactive_draw_items_not_drawn,
        stage.report.interactive_instances_parsed,
        stage.report.interactive_instances_drawn,
        stage.report.interactive_instances_withheld
    );
    for (class, count) in &stage.report.interactive_withheld_by_class {
        eprintln!("interactive withheld: {class} ({count} instances)");
    }
}

/// Dents and cracks the car as the live process's last damage record says, and places its detached parts.
pub fn take_damage(playback: &mut Playback, process: &Live) {
    playback.set_damage(&process.damage.zones);
    playback.on_damage(process.damage.code, process.damage.index);
    playback.set_detached(&process.detached);
}

/// The driving controls of a recording (`--at-drive`): its payloads from the live boot's first driving step on.
fn recorded_controls(path: &Path) -> Result<Vec<[u8; rbr_recording::CONTROL_BYTES]>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let frames = rbr_recording::Frames::new(&bytes).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    Ok(frames.iter().filter(|f| f.pre >= LIVE_BOOT_FIRST_STEP).map(|f| f.control).collect())
}

/// The live boot's first driving step (four 40-substep settle frames).
const LIVE_BOOT_FIRST_STEP: u32 = 160;

//! Loads stages from the retail install. Runs only when `RBR_DATA` is set (the extracted
//! `original/retail-1.0/install` tree), so `cargo test` works without game data. The all-stages test
//! prints a per-stage report: run it with `--nocapture`.

use rbr_formats::lbs::{self, Chunk};
use rbr_gameplay::weather::{self, Draws, Preset, Season};
use rbr_render::stage::{self, Anchor, Layer, Lod, Selection, Stage, TextureRef, TextureSet, Wear, Weather};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The `Maps` directory of the retail install, or `None` when `RBR_DATA` is not set.
fn maps_dir() -> Option<PathBuf> {
    let Ok(data_dir) = std::env::var("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return None;
    };
    Some(Path::new(&data_dir).join("Maps"))
}

/// Every stage name in `maps`: the `.lbs` files other than the global clouds and skybox sets.
fn stage_names(maps: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(maps)
        .expect("Maps directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "lbs"))
        .filter_map(|path| path.file_stem()?.to_str().map(str::to_owned))
        .filter(|name| !name.eq_ignore_ascii_case("clouds") && !name.eq_ignore_ascii_case("skyboxes"))
        .collect();
    names.sort();
    names
}

/// Distinct textures the draws name that have no `[TextureInfo]` entry: an index out of range, or a
/// missing entry.
fn unnamed_textures(stage: &Stage) -> usize {
    stage
        .meshes
        .iter()
        .flat_map(|m| &m.draws)
        .flat_map(|d| d.state.textures.into_iter().chain([d.state.shadow]).flatten())
        .filter(|texture| texture.set != TextureSet::WaterStandIn && !stage.texture_names.contains_key(texture))
        .collect::<BTreeSet<_>>()
        .len()
}

/// Sky faces (the tree line, the sky dome's strips) and the clouds are sky-layer draws. Their textures must decode.
fn undecoded_sky_textures(stage: &Stage) -> usize {
    stage
        .meshes
        .iter()
        .filter(|m| m.layer == Layer::Sky)
        .flat_map(|m| &m.draws)
        .flat_map(|d| d.state.textures.into_iter().flatten())
        .filter(|texture| stage.textures.get(texture).is_none_or(|t| t.image.is_none()))
        .collect::<BTreeSet<_>>()
        .len()
}

fn camera_anchored(stage: &Stage) -> usize {
    stage.meshes.iter().filter(|m| m.anchor == Anchor::Camera).count()
}

#[test]
fn track_10_o_loads_with_every_texture_and_index() {
    let Some(maps) = maps_dir() else { return };
    let mut stage = stage::load(&maps, "track-10_O", Selection::default()).expect("track-10_O loads");
    stage.load_textures(&maps).expect("textures load");

    assert_eq!(stage.meshes.len(), 514, "meshes");
    assert_eq!(stage.draw_count(), 1022, "draws");
    assert_eq!(stage.triangle_count(), 321_300, "triangles in draws (with the US_PARTLY_CLOUDED02 face)");
    assert!(stage.start.is_some(), "car start location");
    assert_eq!(stage.report.missing_textures, Vec::<String>::new());
    assert_eq!(stage.report.bad_triangles, 0);
    assert_eq!(stage.report.skipped_draws, 0);
    assert_eq!(stage.textures.len(), 44, "textures including interactive-object stage references");
    assert_eq!(unnamed_textures(&stage), 0);
}

#[test]
fn weather_picks_the_cloud_faces() {
    let Some(maps) = maps_dir() else { return };
    let faces = |weather| {
        let selection = Selection { weather, wear: Wear::New };
        camera_anchored(&stage::load(&maps, "track-10_O", selection).expect("track-10_O loads"))
    };
    // One face each, the section's `CloudName`: `10O_N_nosnow_lightcloud` `Us_partly_clouded02`,
    // `10O_N_lightsnow_heavycloud` `Us_overcast02`, `10O_N_heavysnow_heavycloud` `Us_overcast`.
    assert_eq!(faces(Weather::Dry), 1, "dry: partly clouded");
    assert_eq!(faces(Weather::Damp), 1, "damp: overcast");
    assert_eq!(faces(Weather::Wet), 1, "wet: overcast");
}

#[test]
fn every_stage_loads_for_every_weather_and_wear() {
    let Some(maps) = maps_dir() else { return };
    let names = stage_names(&maps);
    assert_eq!(names.len(), 86, "stage files");

    let mut loads = 0;
    let mut issues = 0;
    // Indices that do not resolve, and textures with no file: bad triangles, skipped draws, unnamed and missing
    // textures, and sky-layer textures that do not decode. These must be zero: a missing weather folder falls back
    // to a drier one (`Weather::fallbacks`).
    let mut bad_indices = 0;
    for name in &names {
        for weather in Weather::ALL {
            let selection = Selection { weather, wear: Wear::New };
            let mut stage =
                stage::load(&maps, name, selection).unwrap_or_else(|e| panic!("{name} {}: {e}", weather.as_str()));
            for wear in Wear::ALL {
                // Wear changes only the texture requests, so the same geometry serves every wear.
                stage.selection.wear = wear;
                stage
                    .load_textures(&maps)
                    .unwrap_or_else(|e| panic!("{name} {}/{}: {e}", weather.as_str(), wear.as_str()));
                loads += 1;
                let missing = &stage.report.missing_textures;
                let unnamed = unnamed_textures(&stage);
                let bad = stage.report.bad_triangles;
                let skipped = stage.report.skipped_draws;
                let sky = undecoded_sky_textures(&stage);
                bad_indices += bad + skipped + unnamed + missing.len() + sky;
                if !missing.is_empty() || unnamed > 0 || bad > 0 || skipped > 0 || sky > 0 {
                    issues += 1;
                    println!(
                        "{name} {}/{}: missing {} (first: {}), unnamed {unnamed}, bad triangles {bad}, skipped draws {skipped}, undecoded sky {sky}",
                        weather.as_str(),
                        wear.as_str(),
                        missing.len(),
                        missing.first().map_or("-", String::as_str),
                    );
                }
            }
        }
    }
    println!("{} stages, {loads} selections loaded, {issues} with issues", names.len());
    assert_eq!(bad_indices, 0, "bad triangles, skipped draws, unnamed, missing and undecoded sky textures");
}

/// The fallback to a drier folder (`Weather::fallbacks`) is ours, for textures the install lacks. It reaches only
/// maps the season table keeps dry in every row, so the game never asks a stage for the textures it lacks.
#[test]
fn fallbacks_reach_only_maps_the_season_table_keeps_dry() {
    let Some(maps) = maps_dir() else { return };
    let Some(data) = rbr_formats::data::Data::from_env() else { return };
    if data.patch().is_none() {
        eprintln!("skipping: RBR_PATCH not set (the season table is in the exe)");
        return;
    }
    let season = Season::load(&data).expect("the season table");
    let mut falling_back = BTreeSet::new();
    for name in stage_names(&maps) {
        let map: u32 =
            name.strip_prefix("track-").and_then(|n| n.split_once('_')).and_then(|(m, _)| m.parse().ok()).unwrap();
        for weather in [Weather::Damp, Weather::Wet] {
            let selection = Selection { weather, wear: Wear::New };
            let mut stage = stage::load(&maps, &name, selection).expect("stage loads");
            stage.load_textures(&maps).expect("textures load");
            if !stage.report.fallback_textures.is_empty() {
                falling_back.insert(map);
            }
        }
    }
    for map in &falling_back {
        assert!(
            season.rows_of(*map).all(|k| season.rows[k].wetness == 0),
            "map {map} has a stage that falls back, but its season rows are wet or damp"
        );
    }
    assert!(!falling_back.is_empty(), "no stage falls back: the test's maps are wrong");
    eprintln!("maps that fall back to a drier folder: {falling_back:?}");
}

/// The weathers the launcher can start a stage in (`rbr-launcher`: each map's good and bad rows, with the wear it
/// takes) read every texture from that weather's own folder. The install lacks some `damp` and `wet` files, but the
/// season table gives those maps wetness 0 wherever it does, so their stages never reach the fallback.
#[test]
fn launcher_weathers_read_their_own_texture_folders() {
    let Some(maps) = maps_dir() else { return };
    let Some(data) = rbr_formats::data::Data::from_env() else { return };
    if data.patch().is_none() {
        eprintln!("skipping: RBR_PATCH not set (the season table is in the exe)");
        return;
    }
    let season = Season::load(&data).expect("the season table");
    let map_numbers: BTreeSet<u32> = stage_names(&maps)
        .iter()
        .filter_map(|name| name.strip_prefix("track-")?.split_once('_')?.0.parse().ok())
        .collect();
    let mut loads = 0;
    for map in map_numbers {
        for preset in [Preset::Good, Preset::Bad] {
            for wear in 0..=2 {
                let Ok(race) = weather::quick_race(&season, map, preset, Draws { row: None, wear }) else {
                    continue;
                };
                let folder = ["dry", "damp", "wet"][race.wetness.min(2) as usize];
                let selection = Selection { weather: Weather::parse(folder).unwrap(), wear: Wear::ALL[wear as usize] };
                let mut stage = stage::load(&maps, &race.stage(), selection).expect("the launcher's stage loads");
                stage.load_textures(&maps).expect("textures load");
                loads += 1;
                let name = race.stage();
                assert_eq!(stage.report.missing_textures, Vec::<String>::new(), "{name} {folder}");
                assert_eq!(
                    stage.report.fallback_textures,
                    Vec::<String>::new(),
                    "{name} {folder}: read from a drier folder"
                );
            }
        }
    }
    assert!(loads > 0, "no launcher weather loaded");
}

#[test]
fn slot_eight_shadow_draws_have_their_uv_in_the_light_map() {
    let Some(maps) = maps_dir() else { return };
    // Shadow draws from slots 7 and 8 (`notes/formats.md` "Shaders and vertex layouts"): track-20_N has 305 slot-7 and 110
    // slot-8 flagged chunks. Every shadow uv lies in the light map.
    let stage = stage::load(&maps, "track-20_N", Selection::default()).expect("track-20_N loads");
    let mut draws = 0;
    let mut outside = 0;
    for mesh in &stage.meshes {
        for draw in mesh.draws.iter().filter(|d| d.state.shadow.is_some()) {
            draws += 1;
            for &i in &mesh.indices[draw.indices.clone()] {
                let uv = mesh.vertices[i as usize].uv2;
                if !(-0.01..=1.01).contains(&uv[0]) || !(-0.01..=1.01).contains(&uv[1]) {
                    outside += 1;
                }
            }
        }
    }
    eprintln!("track-20_N shadow draws {draws}");
    assert_eq!(draws, 415, "shadow draws");
    assert_eq!(outside, 0, "shadow uv outside the light map");
}

#[test]
fn every_stage_resolves_every_dry_texture_and_draws_every_chunk() {
    // Dry is the one weather the install covers for every stage (`every_stage_loads_for_every_weather_and_wear`
    // prints the damp and wet gaps). Every draw's texture must name a file that reads, and the loader must
    // skip no triangle and no render chunk.
    let Some(maps) = maps_dir() else { return };
    for name in stage_names(&maps) {
        for wear in Wear::ALL {
            let selection = Selection { weather: Weather::Dry, wear };
            let mut stage = stage::load(&maps, &name, selection).expect("stage loads");
            stage.load_textures(&maps).expect("textures load");
            assert_eq!(
                stage.report.missing_textures,
                Vec::<String>::new(),
                "{name} dry/{}: missing textures",
                wear.as_str()
            );
            assert_eq!(unnamed_textures(&stage), 0, "{name} dry: unnamed textures");
            assert_eq!(stage.report.bad_triangles, 0, "{name}: bad triangles");
            assert_eq!(stage.report.skipped_draws, 0, "{name}: skipped draws");
        }
    }
}

#[test]
fn every_stage_draws_its_ground_by_level_of_detail() {
    // A geometry chunk's draw list (render chunk byte 0x6c) is its level of detail: list 1 near, list 3 far,
    // list 2 always (`stage::Lod`; the capture's draws, `notes/formats.md` "Stage geometry"). Only the near chunks are
    // sampled with the mip bias. The totals are the chunk counts per list over the 86 stages. Every texture
    // coordinate the stage stores is finite.
    let Some(maps) = maps_dir() else { return };
    let (mut near, mut far, mut biased) = (0, 0, 0);
    for name in stage_names(&maps) {
        let stage = stage::load(&maps, &name, Selection::default()).expect("stage loads");
        for mesh in &stage.meshes {
            for draw in &mesh.draws {
                match draw.lod {
                    Lod::Near => near += 1,
                    Lod::Far => far += 1,
                    Lod::Always => {}
                }
                if draw.state.mip_bias {
                    biased += 1;
                    assert_eq!(draw.lod, Lod::Near, "{name}: a biased draw that is not near");
                }
            }
            for v in &mesh.vertices {
                assert!([v.uv0, v.uv1, v.uv2].iter().flatten().all(|c| c.is_finite()), "{name}: uv {v:?}");
            }
        }
    }
    assert_eq!((near, far, biased), (37_246, 15_853, 37_246));
}

#[test]
fn every_stage_accounts_for_water_and_reflection_items() {
    let Some(maps) = maps_dir() else { return };
    let names = stage_names(&maps);
    assert_eq!(names.len(), 86, "stage files");
    let stand_in = TextureRef { set: TextureSet::WaterStandIn, index: 0 };
    let (mut total_water, mut water_stages, mut total_reflection, mut reflection_stages) = (0, 0, 0, 0);

    for name in names {
        let bytes = fs::read(maps.join(format!("{name}.lbs"))).expect("stage LBS reads");
        let parsed = lbs::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (mut water, mut reflection) = (0, 0);
        for chunk in &parsed.chunks {
            match chunk {
                Chunk::Water(items) => {
                    water += items.faces.iter().map(|face| face.items.len()).sum::<usize>();
                }
                Chunk::Reflection(items) => {
                    reflection += items.faces.iter().map(|face| face.items.len()).sum::<usize>();
                }
                _ => {}
            }
        }
        total_water += water;
        water_stages += usize::from(water > 0);
        total_reflection += reflection;
        reflection_stages += usize::from(reflection > 0);

        let stage = stage::load(&maps, &name, Selection::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(stage.report.water_items_parsed, water, "{name}: parsed water");
        assert_eq!(stage.report.water_items_drawn, water, "{name}: drawn water");
        assert_eq!(stage.report.water_items_not_drawn, 0, "{name}: withheld water");
        assert_eq!(stage.report.reflection_items_parsed, reflection, "{name}: parsed reflection");
        assert_eq!(stage.report.reflection_items_drawn, 0, "{name}: drawn reflection");
        assert_eq!(stage.report.reflection_items_not_drawn, reflection, "{name}: withheld reflection");

        let water_draws: Vec<_> = stage
            .meshes
            .iter()
            .flat_map(|mesh| &mesh.draws)
            .filter(|draw| draw.state.textures[0] == Some(stand_in))
            .collect();
        assert_eq!(water_draws.len(), water, "{name}: water stand-in draws");
        for draw in water_draws {
            assert_eq!(draw.state.alpha_ref, Some(0), "{name}: water alpha test");
            assert!(draw.state.cull_back, "{name}: water back-face culling");
            assert!(draw.state.blend, "{name}: water alpha blending");
            assert!(draw.state.depth_test, "{name}: water depth test");
            assert!(draw.state.depth_write, "{name}: water depth write");
        }
    }
    assert_eq!(total_water, 90, "water items across retail stages");
    assert_eq!(water_stages, 33, "stages with water items");
    assert_eq!(total_reflection, 51, "reflection items across retail stages");
    assert_eq!(reflection_stages, 10, "stages with reflection items");
    assert_eq!(total_water + total_reflection, 141, "surface items overall");
}

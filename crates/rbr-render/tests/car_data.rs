//! Loads the eight retail cars and one at a stage start. Runs only when `RBR_DATA` is set (the
//! extracted `original/retail-1.0/install` tree), so `cargo test` works without game data.

use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_formats::{car_parts, cars, ini, sgc};
use rbr_render::camera::Lens;
use rbr_render::car::{self, Car};
use rbr_render::coords;
use rbr_render::stage::{self, Layer, Mesh, Selection};
use rbr_render::views::{self, Mount, Mounts, COCKPIT_BACK};

const CARS: [&str; 8] = ["ACCENT", "COROLLA", "IMPREZA00", "IMPREZA03", "LANCER_EVO_VII", "PEUGEOT_206", "XSARA", "ZR"];

/// The install (`RBR_DATA`) with the 1.02 patch over it when `RBR_PATCH` is set.
fn data() -> Option<Data> {
    let data = Data::from_env();
    if data.is_none() {
        eprintln!("skipping: RBR_DATA not set");
    }
    data
}

/// Every mesh's triangles point at its own vertices, and every draw names a texture the car holds.
fn check_indices_and_textures(name: &str, car: &Car) {
    for (i, mesh) in car.meshes.iter().enumerate() {
        assert!(!mesh.draws.is_empty(), "{name}: mesh {i} has no draws");
        assert_eq!(mesh.indices.len() % 3, 0, "{name}: mesh {i} indices");
        for &index in &mesh.indices {
            assert!((index as usize) < mesh.vertices.len(), "{name}: mesh {i} index {index}");
        }
        for draw in &mesh.draws {
            for key in draw.state.textures.into_iter().flatten() {
                assert!(car.textures.contains_key(&key), "{name}: texture {key:?}");
            }
            // `CShaderCarBase::Load` biases the diffuse sampler; the capture's car draws sample t0 at -3.
            assert_eq!(draw.state.mip_bias, draw.state.textures[0].is_some(), "{name}: mesh {i} mip bias");
        }
        // The capture's car uv streams are these bytes (`notes/formats.md` "Car models"); none is NaN or infinite.
        assert!(mesh.vertices.iter().all(|v| v.uv0.iter().all(|c| c.is_finite())), "{name}: mesh {i} uv");
    }
}

#[test]
fn all_retail_cars_load_with_their_textures_and_indices() {
    let Some(data) = data() else {
        return;
    };
    for name in CARS {
        let car = car::load(&data, name, Mat4::IDENTITY).unwrap_or_else(|e| panic!("{name}: {e}"));
        let report = &car.report;
        assert!(report.missing_textures.is_empty(), "{name}: {:?}", report.missing_textures);
        assert_eq!(report.bad_triangles, 0, "{name}: bad triangles");
        assert!(report.unknown_shaders.is_empty(), "{name}: {:?}", report.unknown_shaders);
        // A model mesh whose faces are drawn at different points of the draw order is split.
        assert!(report.meshes_drawn <= car.meshes.len(), "{name}: meshes");
        assert!(report.meshes_drawn > 50, "{name}: {} meshes drawn", report.meshes_drawn);
        assert!(car.textures.values().all(|t| t.image.is_some()), "{name}: textures");
        check_indices_and_textures(name, &car);
        assert!(car.meshes.iter().any(|m| m.layer == Layer::Blended), "{name}: no blended glass");
        eprintln!(
            "{name}: {} meshes drawn, {} skipped, {} textures, {} base-only shader classes",
            report.meshes_drawn,
            report.skipped.len(),
            car.textures.len(),
            report.base_only_shaders.len()
        );
    }
}

#[test]
fn xsara_sits_at_the_track_10_o_start() {
    let Some(data) = data() else {
        return;
    };
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, "track-10_O", Selection::default()).expect("track-10_O loads");
    stage.load_textures(&maps).expect("textures load");
    let start = stage.start.expect("car start location");
    let (meshes, textures) = (stage.meshes.len(), stage.textures.len());

    let car = car::load(&data, "XSARA", car::placement_at(Some(start))).expect("XSARA");
    let (lo, hi) = car.bounds().expect("car geometry");
    let centre = (lo + hi) * 0.5;
    let from_start = Vec3::new(centre.x - start.position.x, 0.0, centre.z - start.position.z);
    assert!(from_start.length() < 6.0, "car centre {from_start:?} from the start");

    let car_meshes = car.meshes.len();
    car.add_to_stage(&mut stage);
    assert_eq!(stage.meshes.len(), meshes + car_meshes);
    assert!(stage.textures.len() > textures, "car textures added");
    for mesh in &stage.meshes {
        for draw in &mesh.draws {
            for key in draw.state.textures.into_iter().flatten() {
                assert!(stage.textures.contains_key(&key), "texture {key:?} in the stage");
            }
        }
    }
}

/// The car's `.sgc` model elements.
fn model(data: &Data, name: &str) -> Vec<sgc::Element> {
    let dir = data.install().join("Cars").join(name);
    let model = std::fs::read_dir(&dir)
        .expect("car folder")
        .map(|e| e.expect("entry").path())
        .find(|p| p.extension().is_some_and(|x| x == "sgc"))
        .expect("model");
    sgc::parse(&std::fs::read(&model).expect("model")).expect("model")
}

/// The outside view draws only the exterior set the loader builds (rbr-recomp `notes/formats/sgc.md`, section 9): no interior
/// `i_` part (`CHH_Car::Render` draws those lists only in the in-car views), no `k_` or `d_` part (never built),
/// and only the tarmac wheels and spare wheels.
#[test]
fn all_retail_cars_draw_only_their_exterior_set() {
    let Some(data) = data() else {
        return;
    };
    for name in CARS {
        let car = car::load(&data, name, Mat4::IDENTITY).unwrap_or_else(|e| panic!("{name}: {e}"));
        let skipped: Vec<&str> = car.report.skipped.iter().map(|(n, _)| n.as_str()).collect();
        let meshes: Vec<String> = model(&data, name)
            .into_iter()
            .filter_map(|e| match e {
                sgc::Element::Mesh(m) => Some(m.node.name),
                _ => None,
            })
            .collect();
        assert_eq!(car.report.meshes_drawn + skipped.len(), meshes.len(), "{name}");
        for mesh in &meshes {
            let surface = (mesh.contains("_wheel_") || mesh.contains("sparewheel")) && !mesh.contains("tarmac");
            if ["i_", "k_", "d_"].iter().any(|p| mesh.starts_with(p)) || surface {
                assert!(skipped.contains(&mesh.as_str()), "{name}: {mesh} is drawn");
            }
        }
    }
}

/// The side doors and their interior panels, placed as hinged bodies at rest, cover the same box: the exterior
/// and interior panels of each side in all eight cars. COROLLA's right interior panel is the one exception:
/// its own placement is the unrotated one (its box is 0.33 m off the rule, rbr-recomp `notes/formats/sgc.md`, section 12).
#[test]
fn side_doors_and_interior_panels_match_in_all_retail_cars() {
    let Some(data) = data() else {
        return;
    };
    for name in CARS {
        let elements = model(&data, name);
        let worlds = sgc::chained_worlds(&elements);
        let find = |node: &str| {
            elements
                .iter()
                .find_map(|e| match e {
                    sgc::Element::Mesh(m) if m.node.name == node => Some(m),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{name}: {node}"))
        };
        let placed = |node: &str, side: &str, sign: f32| {
            let mesh = find(node);
            let w = worlds[&mesh.node.id];
            let door = worlds[&find(&format!("e_door_{side}")).node.id];
            let hinge = [door[0][3], door[1][3], door[2][3]];
            let transform = coords::hinged_panel_to_car(w, mesh.node.transform.words(), hinge, sign);
            let points = mesh.vertices.iter().map(|v| transform.transform_point3(Vec3::from(v.position)));
            let min = points.clone().fold(Vec3::splat(f32::MAX), Vec3::min);
            let max = points.fold(Vec3::splat(f32::MIN), Vec3::max);
            (min, max)
        };
        for (side, sign) in [("l", 1.0), ("r", -1.0)] {
            let (outer_min, outer_max) = placed(&format!("e_door_{side}"), side, sign);
            let (inner_min, inner_max) = placed(&format!("i_door_{side}"), side, sign);
            let gap = (outer_min - inner_min).abs().max_element().max((outer_max - inner_max).abs().max_element());
            assert!(gap < 0.05, "{name} {side} door: box {gap} m from its interior panel");
        }
    }
}

/// The `<car>.ini` part sections of car `name`.
fn parts(data: &Data, name: &str) -> Vec<ini::Section> {
    let read = |dos_path: &str| {
        String::from_utf8_lossy(&std::fs::read(data.resolve(dos_path).expect(dos_path)).expect(dos_path)).into_owned()
    };
    let entries = cars::parse(&read("Cars\\Cars.ini")).expect("Cars.ini");
    let entry = entries
        .iter()
        .find(|e| e.file_name.to_ascii_uppercase().contains(&format!("\\{name}\\")))
        .unwrap_or_else(|| panic!("{name}: no Cars.ini entry"));
    ini::parse(&read(&entry.ini_file)).expect("car ini")
}

/// Every `CarRim` face of the tarmac wheels faces out of the car once the
/// transform block's rotation is applied (`coords::part_to_car`). The game draws the rims with back faces culled
/// (`D3DCULL_CCW`, the render state's default; no retail rim material sets the two-sided `Requirements` bit), so an
/// inward rim would show the hub through it. Without the block's turn by pi the rims of COROLLA, LANCER_EVO_VII,
/// PEUGEOT_206, ZR and IMPREZA03's right rear wheel face in.
#[test]
fn all_retail_cars_turn_their_rims_outwards() {
    let Some(data) = data() else {
        return;
    };
    for name in CARS {
        let elements = model(&data, name);
        let worlds = sgc::chained_worlds(&elements);
        let parts = parts(&data, name);
        let mut rims = 0;
        for element in &elements {
            let sgc::Element::Mesh(mesh) = element else {
                continue;
            };
            if !mesh.node.name.starts_with("e_wheel_tarmac_") {
                continue;
            }
            let place = coords::part_to_car(worlds[&mesh.node.id], mesh.node.transform.words());
            let blocks = car_parts::face_blocks(car_parts::section(&parts, &mesh.node.name).expect("part"));
            let mean = |points: &mut dyn Iterator<Item = Vec3>| {
                let (sum, n) = points.fold((Vec3::ZERO, 0.0), |(s, n), p| (s + p, n + 1.0));
                sum / n
            };
            let hub = mean(&mut mesh.vertices.iter().map(|v| place.transform_point3(Vec3::from(v.position))));
            let out = Vec3::X * hub.x.signum();
            for (face, block) in mesh.faces.iter().zip(&blocks) {
                if block.shader != "CarRim" {
                    continue;
                }
                let start = face.start_index as usize / 3;
                let corners = || {
                    mesh.triangles[start..start + face.prim_count as usize]
                        .iter()
                        .flatten()
                        .map(|&i| &mesh.vertices[i as usize])
                };
                let normal = mean(&mut corners().map(|v| place.transform_vector3(Vec3::from(v.normal))));
                assert!(normal.dot(out) > 0.2, "{name} {}: rim normal {normal}", mesh.node.name);
                rims += 1;
            }
        }
        assert!(rims >= 4, "{name}: {rims} rims");
    }
}

/// Every drawn class has its shader pair from the game's id tables: the two `CarColor` classes and
/// `CarColorNoDirt`-style classes load `CarTextureNoDirt`, `CarLight` loads `CarRim`, `CarTire` loads
/// `CarTexture` (`notes/formats.md` "Shaders and vertex layouts").
#[test]
fn shader_files_follow_the_game_id_tables() {
    let Some(data) = data() else {
        return;
    };
    for name in CARS {
        let car = car::load(&data, name, Mat4::IDENTITY).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (class, files) in &car.report.shader_files {
            let pixel = files.pixel.file_name().expect("file").to_string_lossy();
            let expected = match class.as_str() {
                "CarColor" | "CarColorNoDirt" | "CarTextureNoDirt" => "CarTextureNoDirt.pso",
                "CarLight" => "CarRim.pso",
                "CarTire" => "CarTexture.pso",
                "CarGlass" => "CarGlass.pso",
                "CarPlastic" => "CarPlastic.pso",
                "CarRim" => "CarRim.pso",
                "CarTexture" => "CarTexture.pso",
                "CarPaint" => "CarPaint1.pso",
                "CarPaintInt" => "CarPaintInternal.pso",
                "CarIntGlass" => "CarGlassInternal.pso",
                other => panic!("{name}: unexpected class {other}"),
            };
            assert_eq!(pixel, expected, "{name} {class}");
        }
    }
}

/// With `RBR_PATCH`, the car's Fog shaders are the 1.02 patch's files, byte for byte. Without it, they
/// come from the install. `CarGlass.pso` differs between the two (`notes/formats.md` "Shaders and vertex layouts").
#[test]
fn shader_files_are_the_1_02_patch_files_when_the_patch_is_set() {
    let Some(data) = data() else {
        return;
    };
    let car = car::load(&data, "XSARA", Mat4::IDENTITY).expect("XSARA");
    let files = &car.report.shader_files;
    assert!(files.contains_key("CarGlass") && files.contains_key("CarPaint"));
    let Some(patch) = data.patch() else {
        for entry in files.values() {
            assert!(entry.vertex.starts_with(data.install()));
            assert!(entry.pixel.starts_with(data.install()));
        }
        return;
    };
    let fog = |kind: &str, name: &str| patch.join(kind).join("Fog").join(name);
    // CarIntGlass is in General, which the patch does not replace: it comes from the install.
    for (class, entry) in files.iter().filter(|(c, _)| c.as_str() != "CarIntGlass") {
        let vertex = entry.vertex.file_name().expect("file").to_string_lossy();
        let pixel = entry.pixel.file_name().expect("file").to_string_lossy();
        assert_eq!(
            std::fs::read(&entry.vertex).expect("vertex"),
            std::fs::read(fog("VertexShaders", &vertex)).expect("1.02 vertex"),
            "{class} vertex"
        );
        assert_eq!(
            std::fs::read(&entry.pixel).expect("pixel"),
            std::fs::read(fog("PixelShaders", &pixel)).expect("1.02 pixel"),
            "{class} pixel"
        );
    }
    if let Some(entry) = files.get("CarIntGlass") {
        assert!(entry.pixel.starts_with(data.install()));
    }
    let glass = std::fs::read(fog("PixelShaders", "CarGlass.pso")).expect("1.02 CarGlass");
    let retail = std::fs::read(data.install().join("PixelShaders/fog/CarGlass.pso")).expect("retail");
    assert_ne!(glass, retail, "the patch's CarGlass.pso differs from the retail one");
}

/// Faces with an opacity map are blended as `CShaderCarBase::Load` sets them (`car::Pass`): with the alpha test
/// `> 0` and no z write, back faces culled (the windows `e_window_*`, the inner panes `pc_e_window_*` and the vents,
/// six, six and up to two per car), except `CarPlastic`, which keeps z write and drops culling (the light glass, the
/// `lightglass` parts drawn while whole).
/// Counted over the eight cars.
#[test]
fn transparent_faces_are_drawn_in_the_cars() {
    let Some(data) = data() else {
        return;
    };
    let (mut blended, mut plastic, mut cracks) = (0, 0, 0);
    for name in CARS {
        let car = car::load(&data, name, Mat4::IDENTITY).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Every car has the six windows `g_HH_DmgParts` names; the crack overlays are blended without the alpha test.
        assert_eq!(car.windows(), [true; 6], "{name}: windows");
        for (mesh, damage) in car.meshes.iter().zip(&car.damage).filter(|(_, d)| d.crack.is_some()) {
            let state = mesh.draws[0].state;
            assert!(state.blend && state.alpha_ref.is_none() && !state.depth_write, "{name}: crack overlay");
            cracks += usize::from(damage.crack.is_some());
        }
        let drawn = car.meshes.iter().zip(&car.damage).filter(|(_, d)| d.crack.is_none());
        for draw in drawn.flat_map(|(m, _)| &m.draws) {
            let state = draw.state;
            assert_eq!(state.alpha_ref.is_some(), state.blend, "{name}: alpha test");
            match (state.blend, state.depth_write, state.cull_back) {
                (true, false, true) => blended += 1,
                (true, true, false) => plastic += 1,
                (false, true, true) => {}
                other => panic!("{name}: unexpected state {other:?}"),
            }
        }
    }
    assert_eq!((blended, plastic), (106, 35), "blended and plastic draws over the eight cars");
    // Two projections on the front and rear windows and one on each side window.
    assert!(cracks >= 8 * CARS.len(), "{cracks} crack overlays");
}

/// The lens of a mount at the setting's 50 degrees horizontal.
fn lens_of(mount: &Mount) -> Lens {
    Lens { eye: mount.eye, forward: mount.target - mount.eye, up: mount.up, fov: 50f32.to_radians(), near: mount.near }
}

/// The normalised device coordinates (x, y) of the vertices of `meshes` in the image at 16:9 through `lens`, whose
/// normal's y is above `up` (`None`: any).
fn image_points(meshes: &[Mesh], lens: &Lens, up: Option<f32>) -> Vec<(f32, f32)> {
    let view_projection = lens.view_projection(16.0 / 9.0);
    meshes
        .iter()
        .flat_map(|mesh| mesh.vertices.iter())
        .filter(|v| up.is_none_or(|y| v.normal[1] > y))
        .filter_map(|v| {
            let clip = view_projection * Vec3::from(v.position).extend(1.0);
            let inside =
                clip.w > 0.0 && clip.x.abs() <= clip.w && clip.y.abs() <= clip.w && (0.0..=clip.w).contains(&clip.z);
            inside.then(|| (clip.x / clip.w, clip.y / clip.w))
        })
        .collect()
}

/// The in-car views of COROLLA at the setting's 50 degrees and 16:9. The cockpit eye is inside the cabin, and at
/// [`COCKPIT_BACK`] the dash and the wheel fill the lower middle of the image, where the eye at `Cam_internal` shows
/// almost nothing. The bonnet view draws the bonnet's up-facing exterior, which is out of frame from `Cam_bonnet`.
#[test]
fn in_car_views_frame_the_cabin_and_the_bonnet_at_fifty_degrees() {
    let Some(data) = data() else {
        return;
    };
    let exterior = car::load(&data, "COROLLA", Mat4::IDENTITY).expect("COROLLA");
    let interior = car::load_interior(&data, "COROLLA", Mat4::IDENTITY).expect("COROLLA interior");
    let (lo, hi) = exterior.bounds().expect("COROLLA bounds");
    let mounts = Mounts::load(&data, "COROLLA", lo, hi, &exterior.meshes, &interior.meshes).expect("COROLLA views");
    let sections = car::part_sections(&data, "COROLLA").expect("COROLLA sections");
    let unshifted = |name: &str| {
        let section = car_parts::section(&sections, name).expect(name);
        Mount::from_section(section).expect(name)
    };
    assert!((mounts.cockpit.eye - unshifted("Cam_internal").moved_back(COCKPIT_BACK).eye).length() < 1e-6);
    let (cabin_lo, cabin_hi) = interior.bounds().expect("interior bounds");
    let eye = mounts.cockpit.eye;
    assert!(cabin_lo.cmple(eye).all() && eye.cmple(cabin_hi).all(), "cockpit eye {eye} outside the cabin");

    // The dash and the wheel: interior vertices in the middle column, between the bottom and the middle of the image.
    let wheel = |back: f32| {
        let mount = unshifted("Cam_internal").moved_back(back);
        image_points(&interior.meshes, &lens_of(&mount), None)
            .iter()
            .filter(|&&(x, y)| x.abs() < 0.5 && (-0.7..-0.4).contains(&y))
            .count()
    };
    let (at_eye, at_cockpit) = (wheel(0.0), wheel(COCKPIT_BACK));
    assert!(
        at_cockpit > 100 && at_cockpit > 10 * at_eye,
        "dash and wheel: {at_eye} at the eye, {at_cockpit} at the cockpit"
    );

    let bonnet = |mount: &Mount| {
        image_points(&exterior.meshes, &lens_of(mount), Some(0.9)).iter().filter(|&&(_, y)| y < -0.4).count()
    };
    let (at_section, at_bonnet) = (bonnet(&unshifted("Cam_bonnet")), bonnet(&mounts.bonnet));
    assert!(
        at_bonnet > 20 && at_section == 0,
        "bonnet: {at_section} at the section's eye, {at_bonnet} at the bonnet's"
    );
}

/// Every RSF car of `RBR_RSF` whose own model folder is installed (the samples: Evo IX, Alpine A110, BMW M3 E30,
/// Fiesta Rally3) loads outside and in, with every texture read and no bad triangle.
#[test]
fn rsf_car_models_load_with_their_textures() {
    let (Some(data), Some(root)) = (data(), std::env::var_os("RBR_RSF")) else {
        eprintln!("skipping: RBR_DATA or RBR_RSF not set");
        return;
    };
    let install = rbr_formats::rsf::Install::open(std::path::Path::new(&root)).unwrap();
    let mut loaded = 0;
    for rsf_car in &install.cars {
        let Ok(selected) =
            rbr_formats::rsf::select(data.clone(), &rsf_car.pack, rbr_formats::rsf::SLOT, Some(&install.root))
        else {
            continue;
        };
        if selected.model.fallback.is_some() {
            continue;
        }
        let name = selected.folder().to_owned();
        let outside = car::load(&selected.data, &name, Mat4::IDENTITY).unwrap_or_else(|e| panic!("{name}: {e}"));
        let inside =
            car::load_interior(&selected.data, &name, Mat4::IDENTITY).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (set, car) in [("outside", &outside), ("inside", &inside)] {
            assert!(car.report.meshes_drawn > 0, "{name} {set}");
            assert_eq!(car.report.missing_textures, Vec::<String>::new(), "{name} {set}");
            assert_eq!(car.report.bad_triangles, 0, "{name} {set}");
        }
        loaded += 1;
    }
    assert!(loaded > 0, "no RSF car with its own model under {}", install.root.display());
}

/// Every retail car's cockpit is its `Cam_internal` moved the whole [`COCKPIT_BACK`], unturned; the RSF GR Yaris's
/// stops short of its seat shell, but clear of its headrest (Cameras-4), the RSF Audi quattro A2's short of where its
/// seat bolsters enter the frame (`views::FRAME_CLEAR`; no retail seat comes within it), and no RSF car's eye is
/// in its seat: straight ahead the first interior surface is further than the near plane. Every car's bonnet view shows
/// its bonnet at the bottom of the image.
#[test]
fn the_cockpit_eye_stays_out_of_the_seat() {
    let Some(data) = data() else {
        return;
    };
    let internal = |data: &Data, name: &str| {
        let sections = car::camera_sections(data, name).expect(name);
        Mount::from_section(car_parts::section(&sections, "Cam_internal").expect(name)).expect(name)
    };
    let shows_bonnet = |data: &Data, name: &str, interior: &[Mesh]| {
        let exterior = car::load(data, name, Mat4::IDENTITY).expect(name);
        let (lo, hi) = exterior.bounds().expect(name);
        let mounts = Mounts::load(data, name, lo, hi, &exterior.meshes, interior).expect(name);
        // The line through the bottom middle of a 50-degree, 16:9 image meets the body.
        let m = mounts.bonnet;
        let ahead = (m.target - m.eye).normalize();
        let up = ahead.cross(Vec3::Y).cross(ahead).normalize();
        let down = (ahead - up * (25f32.to_radians().tan() * 9.0 / 16.0 * 0.85)).normalize();
        let hit = exterior.meshes.iter().any(|mesh| {
            mesh.indices.chunks_exact(3).any(|t| {
                let at = |k: usize| Vec3::from(mesh.vertices[t[k] as usize].position);
                views::ray_hit(m.eye, down, [at(0), at(1), at(2)]).is_some_and(|d| d < 3.0)
            })
        });
        assert!(hit, "{name}: no bonnet at the bottom of the bonnet view {m:?}");
        mounts
    };
    for name in CARS {
        let interior = car::load_interior(&data, name, Mat4::IDENTITY).expect(name);
        let mounts = shows_bonnet(&data, name, &interior.meshes);
        assert_eq!(mounts.cockpit, internal(&data, name).moved_back(COCKPIT_BACK), "{name}");
    }
    let Some(root) = std::env::var_os("RBR_RSF") else {
        eprintln!("skipping the RSF cars: RBR_RSF not set");
        return;
    };
    let install = rbr_formats::rsf::Install::open(std::path::Path::new(&root)).unwrap();
    let mut short = Vec::new();
    for rsf_car in &install.cars {
        let Ok(selected) =
            rbr_formats::rsf::select(data.clone(), &rsf_car.pack, rbr_formats::rsf::SLOT, Some(&install.root))
        else {
            continue;
        };
        if selected.model.fallback.is_some() {
            continue;
        }
        let name = selected.folder().to_owned();
        let interior = car::load_interior(&selected.data, &name, Mat4::IDENTITY).expect(&name);
        let mounts = shows_bonnet(&selected.data, &name, &interior.meshes);
        let mount = internal(&selected.data, &name);
        let back = mounts.cockpit.eye.z - mount.eye.z;
        if back < COCKPIT_BACK {
            short.push((name.clone(), back));
        }
        let ahead = image_points(&interior.meshes, &lens_of(&mounts.cockpit), None).len();
        assert!(ahead > 0, "{name}: nothing in the cockpit view");
        // Straight ahead and around it (about 26 by 15 degrees), the first interior surface drawn (not blended, and
        // facing the eye where culled at its back): inside a seat, every line meets it within a few centimetres.
        let eye = mounts.cockpit.eye;
        let ahead = (mounts.cockpit.target - eye).normalize();
        let right = ahead.cross(Vec3::Y).normalize();
        let up = right.cross(ahead);
        let first = |dir: Vec3| {
            interior
                .meshes
                .iter()
                .flat_map(|mesh| {
                    mesh.draws.iter().filter(|draw| !draw.state.blend).flat_map(move |draw| {
                        mesh.indices[draw.indices.clone()].chunks_exact(3).filter_map(move |t| {
                            let at = |k: usize| Vec3::from(mesh.vertices[t[k] as usize].position);
                            let t = [at(0), at(1), at(2)];
                            let facing = (t[1] - t[0]).cross(t[2] - t[0]).dot(dir) < 0.0;
                            (facing || !draw.state.cull_back).then(|| views::ray_hit(eye, dir, t)).flatten()
                        })
                    })
                })
                .fold(f32::INFINITY, f32::min)
        };
        let close = (-2..=2)
            .flat_map(|i| {
                (-1..=1).map(move |j| (ahead + right * (0.12 * i as f32) + up * (0.12 * j as f32)).normalize())
            })
            .filter(|&dir| first(dir) < 0.3)
            .count();
        assert!(close < 5, "{name}: {close} of 15 lines ahead meet the interior within 0.3 m");
    }
    short.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(short.iter().any(|(n, back)| n.eq_ignore_ascii_case("yaris_rally2") && *back > 0.0), "{short:?}");
    // The Audi quattro A2's seat bolsters lie 0.26 to 0.3 m from the eye at the frame's bottom edge 0.5 m back.
    assert!(short.iter().any(|(n, back)| n.eq_ignore_ascii_case("AUDI_QUATTRO_A2_GrpB") && *back > 0.0), "{short:?}");
}

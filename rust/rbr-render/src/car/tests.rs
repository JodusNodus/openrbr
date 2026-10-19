use super::*;
use crate::stage::{Draw, Layer, Lod};
use rbr_formats::{ini, sgc};
use rbr_recording::detach;

use super::files::folder;
use super::meshes::{build_meshes, in_draw_order};
use super::parts::{detach_part, part_for};
use super::shading::{draw_state, list_for, pass_for, FaceStyle, List, Pass};

#[test]
fn folder_is_the_second_component() {
    assert_eq!(folder("Cars\\xsara\\xsara.sgc"), Some("xsara"));
    assert_eq!(folder("Cars/Lancer_EVO_VII/x.sgc"), Some("Lancer_EVO_VII"));
    assert_eq!(folder("Cars"), None);
}

#[test]
fn mirrored_mesh_keeps_its_outer_side_and_bad_triangles_are_dropped() {
    let mesh = sgc::Mesh {
        node: sgc::Node {
            name: "e_test".to_owned(),
            id: 1,
            parent: -1,
            transform: sgc::Transform {
                scale: [0.0; 3],
                axis: [0.0; 3],
                angle: 0.0,
                translation: [0.0; 3],
                axis2: [0.0; 3],
                angle2: 0.0,
            },
            world: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
        },
        damageable: false,
        centre: [0.0; 3],
        half_extent: [0.0; 3],
        stream_flags: sgc::STREAM_VERTICES,
        vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            .map(|position| sgc::Vertex { position, normal: [0.0, 0.0, 1.0] })
            .to_vec(),
        uvs: Vec::new(),
        aux: Vec::new(),
        // The second triangle names vertex 9, which the mesh does not have.
        triangles: vec![[0, 1, 2], [0, 1, 9]],
        faces: vec![sgc::Face { name: "m".to_owned(), prim_count: 2, min_vertex: 0, num_vertices: 3, start_index: 0 }],
        morph_targets: Vec::new(),
        morph_lists: Vec::new(),
    };
    let key = TextureRef { set: TextureSet::Car, index: 4 };
    let opaque =
        |texture| FaceStyle { texture, scrape: None, shade: None, pass: Pass::Opaque, order: (List::Opaque, 0) };
    let plain = build_meshes(&mesh, Mat4::IDENTITY, false, &[opaque(Some(key))]);
    assert_eq!(plain.bad_triangles, 1);
    let [(_, plain, MeshDamage { dents: None, crack: None, .. })] = &plain.meshes[..] else {
        panic!("one mesh");
    };
    assert_eq!(plain.indices, vec![0, 1, 2]);
    assert_eq!(
        plain.draws,
        vec![Draw { indices: 0..3, state: draw_state(Some(key), None, Pass::Opaque, None), lod: Lod::Always }]
    );

    let mirrored = build_meshes(&mesh, Mat4::IDENTITY, true, &[opaque(None)]);
    let mirrored = &mirrored.meshes[0].1;
    let corners: Vec<[f32; 3]> = mirrored.indices.iter().map(|&i| mirrored.vertices[i as usize].position).collect();
    assert_eq!(corners, [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]);
    assert_eq!(mirrored.draws[0].state.textures, [None, None]);

    // Two faces drawn at different points: one mesh each, with only the vertices it uses.
    let two = sgc::Mesh {
        triangles: vec![[0, 1, 2], [1, 2, 0]],
        faces: vec![
            sgc::Face { prim_count: 1, ..mesh.faces[0].clone() },
            sgc::Face { prim_count: 1, start_index: 3, ..mesh.faces[0].clone() },
        ],
        ..mesh.clone()
    };
    let pane = FaceStyle { texture: None, scrape: None, shade: None, pass: Pass::Blended, order: (List::Opaque, 1) };
    let later = FaceStyle { order: (List::Opaque, 2), ..opaque(None) };
    let built = build_meshes(&two, Mat4::IDENTITY, false, &[later, pane]);
    assert_eq!(built.meshes.len(), 2);
    assert_eq!(built.meshes[0].0, (List::Opaque, 1));
    assert_eq!(built.meshes[0].1.indices, vec![0, 1, 2]);
    assert_eq!(built.meshes[0].1.vertices[0].position, [1.0, 0.0, 0.0]);
    // The solid face after the blended pane is drawn after it, in the blended layer.
    let ordered = in_draw_order(built.meshes);
    assert!(ordered[0].0.draws[0].state.blend);
    assert_eq!(ordered[1].0.layer, Layer::Blended);
}

#[test]
fn interior_panels_detach_with_their_exterior_part() {
    assert_eq!(detach_part("e_door_l"), detach::part_index("e_door_l"));
    assert_eq!(detach_part("i_door_l"), detach::part_index("e_door_l"));
    assert_eq!(detach_part("i_bumper_f"), detach::part_index("e_bumper_f"));
    assert_eq!(detach_part("i_body"), None);
}

#[test]
fn a_damaged_panel_moves_by_its_placed_delta_and_scrapes() {
    let mut mesh = sgc::Mesh {
        node: sgc::Node {
            name: "e_body".to_owned(),
            id: 1,
            parent: -1,
            transform: sgc::Transform {
                scale: [0.0; 3],
                axis: [0.0; 3],
                angle: 0.0,
                translation: [0.0; 3],
                axis2: [0.0; 3],
                angle2: 0.0,
            },
            world: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
        },
        damageable: true,
        centre: [0.0; 3],
        half_extent: [0.0; 3],
        stream_flags: sgc::STREAM_VERTICES,
        vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            .map(|position| sgc::Vertex { position, normal: [0.0, 0.0, 1.0] })
            .to_vec(),
        uvs: Vec::new(),
        aux: Vec::new(),
        // Drawn from vertex 2 first, so the drawn order differs from the panel's.
        triangles: vec![[2, 0, 1]],
        faces: vec![sgc::Face { name: "m".to_owned(), prim_count: 1, min_vertex: 0, num_vertices: 3, start_index: 0 }],
        morph_targets: vec![sgc::MorphTarget {
            flag: 1,
            deltas: vec![[0.0, 0.0, -0.001], [0.0; 3], [0.002, 0.0, 0.0]],
        }],
        // Zone 1 moves vertex 2 fully and vertex 0 by half.
        morph_lists: vec![Vec::new(), vec![(2, 1.0), (0, 0.5)]],
    };
    let style = FaceStyle { texture: None, scrape: None, shade: None, pass: Pass::Opaque, order: (List::Opaque, 0) };
    // A quarter turn about +y, then 5 m up: the delta turns with the panel.
    let matrix = Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0)) * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let built = build_meshes(&mesh, matrix, false, &[style]);
    let (meshes, damage) = built.meshes.into_iter().map(|(_, m, d)| (m, d)).unzip();
    let mut car = Car {
        name: "x".to_owned(),
        meshes,
        damage,
        flares: Vec::new(),
        textures: BTreeMap::new(),
        report: CarReport::default(),
    };
    let mut zones = [0.0; damage::ZONES];
    zones[1] = 0.5;
    car.dent(&zones);
    let moved: Vec<Vec3> = car.meshes[0].vertices.iter().map(|v| Vec3::from(v.position)).collect();
    // Vertex 2: weight 50, local (0, 1, 0) + 50 * (0.002, 0, 0) = (0.1, 1, 0).
    let expect = [
        matrix.transform_point3(Vec3::new(0.1, 1.0, 0.0)),
        matrix.transform_point3(Vec3::new(0.0, 0.0, -0.025)),
        matrix.transform_point3(Vec3::new(1.0, 0.0, 0.0)),
    ];
    for (got, want) in moved.iter().zip(expect) {
        assert!((*got - want).length() < 1e-5, "{got} {want}");
    }
    let alpha: Vec<u8> = car.meshes[0].vertices.iter().map(|v| v.color[3]).collect();
    assert_eq!(alpha, [damage::scrape_byte(0.5), damage::scrape_byte(0.25), 0]);

    // Without a morph target the panel is not deformed.
    mesh.morph_targets.clear();
    assert!(build_meshes(&mesh, matrix, false, &[style]).meshes[0].2.dents.is_none());
}

#[test]
fn passes_and_lists_follow_class_and_opacity() {
    assert_eq!(pass_for("CarIntGlass", false), Pass::InteriorGlass);
    assert_eq!(pass_for("CarGlass", true), Pass::Blended);
    assert_eq!(pass_for("CarTextureNoDirt", true), Pass::Blended);
    assert_eq!(pass_for("CarGlass", false), Pass::Opaque);
    assert_eq!(pass_for("CarPlastic", true), Pass::Plastic);
    assert_eq!(pass_for("CarPlastic", false), Pass::Opaque);
    let blended = draw_state(None, None, Pass::Blended, None);
    assert!(blended.blend && !blended.depth_write && blended.cull_back);
    assert_eq!(blended.alpha_ref, Some(0));
    let plastic = draw_state(None, None, Pass::Plastic, None);
    assert!(plastic.blend && plastic.depth_write && !plastic.cull_back);
    assert!(!draw_state(None, None, Pass::InteriorGlass, None).cull_back);
    let opaque = draw_state(None, None, Pass::Opaque, None);
    assert!(opaque.depth_write && !opaque.blend && opaque.alpha_ref.is_none());

    assert_eq!(list_for("CarRim", true), List::Wheels);
    assert_eq!(list_for("CarTire", false), List::Wheels);
    assert_eq!(list_for("CarGlass", true), List::Transparent);
    assert_eq!(list_for("CarGlass", false), List::Opaque);
    assert_eq!(list_for("CarTextureNoDirt", true), List::Opaque);
    assert_eq!(list_for("CarIntGlass", false), List::Transparent);
}

#[test]
fn skip_reasons_follow_name_and_part() {
    let parts = ini::parse("[e_wheel_gravel_lf]\r\nFaceBlock0_Shader = CarTire\r\nFaceBlock0_Material = m\r\n[e_light_lf]\r\nFaceBlock0_Shader = CarLight\r\nFaceBlock0_Material = m\r\nSwitch = true\r\nSwitchBox = light_lf\r\nPredicate = on\r\n[e_light_lf_off]\r\nFaceBlock0_Shader = CarLight\r\nFaceBlock0_Material = m\r\nSwitch = true\r\nSwitchBox = light_lf\r\nPredicate = off\r\n[e_box_off]\r\nFaceBlock0_Shader = CarLight\r\nFaceBlock0_Material = m\r\nSwitch = true\r\nSwitchBox = horn\r\nPredicate = off\r\n[e_sparewheelstrap_snow]\r\nFaceBlock0_Shader = CarTextureNoDirt\r\nFaceBlock0_Material = m\r\n[e_empty]\r\nSwitch = false\r\n[e_body]\r\nFaceBlock0_Shader = CarPaint\r\nFaceBlock0_Material = m\r\n")
        .unwrap();
    assert_eq!(part_for("ghost_external", &parts).unwrap_err(), Skip::Ghost);
    assert_eq!(part_for("texture_dirtmap", &parts).unwrap_err(), Skip::Overlay);
    assert_eq!(part_for("Plane_body", &parts).unwrap_err(), Skip::Plane);
    assert_eq!(part_for("i_bumper_f", &parts).unwrap_err(), Skip::LoaderDrops);
    assert_eq!(part_for("k_windscreen", &parts).unwrap_err(), Skip::LoaderDrops);
    assert_eq!(part_for("i_body", &parts).unwrap_err(), Skip::Interior);
    assert_eq!(part_for("e_missing", &parts).unwrap_err(), Skip::NoSection);
    assert!(part_for("e_light_lf", &parts).is_ok());
    assert!(part_for("e_light_lf_off", &parts).is_ok());
    assert_eq!(part_for("e_box_off", &parts).unwrap_err(), Skip::Switched);
    assert_eq!(part_for("e_wheel_gravel_lf", &parts).unwrap_err(), Skip::SurfaceWheel);
    assert_eq!(part_for("e_sparewheelstrap_snow", &parts).unwrap_err(), Skip::SurfaceWheel);
    assert_eq!(part_for("e_empty", &parts).unwrap_err(), Skip::NoShaderSlots);
    assert!(part_for("e_body", &parts).is_ok());
}

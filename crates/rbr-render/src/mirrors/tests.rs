use super::*;
use crate::camera::FreeFly;
use crate::stage::{Address, Anchor, DrawState, Layer, Lod, Shade, Vertex};
use bytemuck::Zeroable;

/// The cockpit's eye in the tests' model space (the car looks along `-z`).
const EYE: Vec3 = Vec3::new(0.3, 1.0, -0.8);

/// One opaque draw of `triangles`, each vertex with `normal`, of the glass class when `glass`.
fn mesh(triangles: &[[Vec3; 3]], normal: Vec3, glass: bool) -> Mesh {
    let vertices: Vec<Vertex> = triangles
        .iter()
        .flatten()
        .map(|&p| Vertex { position: p.into(), normal: normal.into(), ..Vertex::zeroed() })
        .collect();
    Mesh {
        layer: Layer::Solid,
        anchor: Anchor::World,
        bounds: None,
        cell: None,
        indices: (0..vertices.len() as u32).collect(),
        draws: vec![Draw {
            indices: 0..vertices.len(),
            state: DrawState {
                textures: [None, None],
                shadow: None,
                address: [Address::Wrap; 2],
                second_texture: false,
                alpha_ref: None,
                mip_bias: false,
                cull_back: false,
                blend: false,
                additive: false,
                depth_test: true,
                depth_write: true,
                shade: glass.then(|| Shade { glass: true, ..Shade::default() }),
                specular: None,
            },
            lod: Lod::Always,
        }],
        vertices,
    }
}

/// The two triangles of the rectangle about `centre` spanning `u` and `v` (half sizes).
fn rectangle(centre: Vec3, u: Vec3, v: Vec3) -> [[Vec3; 3]; 2] {
    let [a, b, c, d] = [centre - u - v, centre + u - v, centre + u + v, centre - u + v];
    [[a, b, c], [a, c, d]]
}

/// A door mirror's glass, 12 by 14 cm, on the left of the car, facing back.
fn door_glass() -> Mesh {
    mesh(&rectangle(Vec3::new(0.8, 0.8, -1.7), Vec3::X * 0.06, Vec3::Y * 0.07), Vec3::Z, false)
}

fn named(mirror: Option<Mirror>) -> MeshDamage {
    MeshDamage { mirror, ..MeshDamage::default() }
}

#[test]
fn rates_parse_name_and_take_turns() {
    for rate in Rate::ALL {
        assert_eq!(Rate::parse(&rate.name()), Some(rate));
    }
    assert_eq!(Rate::parse("4"), None);
    assert_eq!(Rate::default(), Rate::Off);
    assert!((0..6).all(|frame| !Rate::Off.due(0, frame) && Rate::Every(1).due(1, frame)));
    // Every 2nd frame: two mirrors in turn, one a frame.
    let two: Vec<[bool; 2]> = (0..4).map(|f| [0, 1].map(|k| Rate::Every(2).due(k, f))).collect();
    assert_eq!(two, [[true, false], [false, true], [true, false], [false, true]]);
    assert_eq!((0..6).filter(|&f| Rate::Every(3).due(2, f)).count(), 2);
}

#[test]
fn a_door_mirror_and_a_hanging_centre_one_are_found() {
    // The centre mirror is no named part: an opaque pane over the windscreen, turned down to the driver.
    let centre = mesh(&rectangle(Vec3::new(0.0, 1.1, -1.4), Vec3::X * 0.1, Vec3::Y * 0.03), Vec3::Z, false);
    // A roof panel, too large for a mirror.
    let roof = mesh(&rectangle(Vec3::new(0.0, 1.3, -0.8), Vec3::X * 0.6, Vec3::Z * 0.6), -Vec3::Y, false);
    let interior = [door_glass(), centre, roof];
    let damage = [named(Some(Mirror::Glass)), named(None), named(None)];
    let found = find(&interior, &damage, EYE);
    assert!(found.unfitted.is_empty());
    let kinds: Vec<(Kind, usize)> = found.mirrors.iter().map(|m| (m.kind, m.faces[0].0)).collect();
    assert_eq!(kinds, [(Kind::Side, 0), (Kind::Centre, 1)]);
    let door = &found.mirrors[0];
    assert!(door.centre.distance(Vec3::new(0.8, 0.8, -1.7)) < 1e-5);
    assert!(door.normal.distance(Vec3::Z) < 1e-5);
    assert!((door.half - Vec2::new(0.06, 0.07)).abs().max_element() < 1e-5);
    assert_eq!(door.faces.len(), 2);
}

#[test]
fn a_mirror_behind_a_wall_or_facing_forward_is_none() {
    // An opaque panel between the eye and the glass hides it (the glass class's faces would not: the windows).
    let wall = |glass| mesh(&rectangle(Vec3::new(0.65, 0.85, -1.4), Vec3::X * 0.3, Vec3::Y * 0.3), Vec3::Z, glass);
    let damage = [named(Some(Mirror::Glass)), named(None)];
    assert!(find(&[door_glass(), wall(false)], &damage, EYE).mirrors.is_empty());
    assert_eq!(find(&[door_glass(), wall(true)], &damage, EYE).mirrors.len(), 1);
    // Glass turned so that the eye's line reflects forward shows the dash, not what lies behind.
    let turned = Vec3::new(1.0, 0.0, 0.3).normalize();
    let forward =
        mesh(&rectangle(Vec3::new(0.8, 0.8, -1.7), turned.cross(Vec3::Y) * 0.06, Vec3::Y * 0.07), -turned, false);
    assert!(find(&[forward], &[named(Some(Mirror::Glass))], EYE).mirrors.is_empty());
}

#[test]
fn a_domed_glass_keeps_the_cube() {
    // Eight strips 2 cm wide, each turned 5 degrees from the last: one pane, 14 mm deep.
    let edge = |i: usize| {
        let angle = (i as f32 - 4.0) * 5f32.to_radians();
        Vec3::new(0.8, 0.8, -1.7) + Vec3::new(angle.sin(), 0.0, -angle.cos()) * 0.23 + Vec3::Z * 0.23
    };
    let triangles: Vec<[Vec3; 3]> = (0..8)
        .flat_map(|i| {
            let (a, b, up) = (edge(i), edge(i + 1), Vec3::Y * 0.05);
            [[a - up, b - up, b + up], [a - up, b + up, a + up]]
        })
        .collect();
    let found = find(&[mesh(&triangles, Vec3::Z, false)], &[named(Some(Mirror::Glass))], EYE);
    assert!(found.mirrors.is_empty());
    assert_eq!(found.unfitted.len(), 1);
    assert_eq!(found.unfitted[0].triangles, 16);
    assert!(found.unfitted[0].off_plane > 0.006, "{:?}", found.unfitted);
}

#[test]
fn marks_copy_the_shared_corners() {
    // Two faces sharing an edge, each another mirror's: each gets its own corners, the old ones stay unmarked.
    let a = [Vec3::ZERO, Vec3::X, Vec3::Y];
    let mut interior = [mesh(&[a, [Vec3::X, Vec3::ONE, Vec3::Y]], Vec3::Z, false)];
    interior[0].indices = vec![0, 1, 2, 1, 4, 2];
    let glass = Glass {
        faces: vec![(0, 0)],
        kind: Kind::Side,
        centre: Vec3::ZERO,
        normal: Vec3::Z,
        axes: [Vec3::X, Vec3::Y],
        half: Vec2::ONE,
    };
    let found = Found { mirrors: vec![glass.clone(), Glass { faces: vec![(0, 3)], ..glass }], unfitted: Vec::new() };
    let mut damage = [MeshDamage::default()];
    mark(&mut interior, &mut damage, &found);
    let mesh = &interior[0];
    assert_eq!(mesh.indices, [6, 7, 8, 9, 10, 11]);
    let marks: Vec<f32> = mesh.indices.iter().map(|&i| mesh.vertices[i as usize].specular[0]).collect();
    assert_eq!(marks, [1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
    assert!(mesh.vertices[..6].iter().all(|v| v.specular[0] == 0.0));
    assert_eq!(mesh.vertices[mesh.indices[1] as usize].position, Vec3::X.to_array());
}

/// `p` in `m`'s clip space, divided.
fn ndc(m: Mat4, p: Vec3) -> Vec3 {
    let clip = m * p.extend(1.0);
    clip.truncate() / clip.w
}

/// Twice the signed area of the triangle `t` in `m`'s screen.
fn turn(m: Mat4, t: [Vec3; 3]) -> f32 {
    let [a, b, c] = t.map(|p| ndc(m, p).truncate());
    (b - a).perp_dot(c - a)
}

#[test]
fn a_view_reflects_the_scene_into_the_glass_box() {
    let (fov, aspect, screen) = (70f32.to_radians(), 16.0 / 9.0, (1920, 1080));
    let camera = FreeFly::looking_along(Vec3::ZERO, Vec3::Z);
    let (main, sky) = (camera.view_projection(fov, aspect), camera.sky_view_projection(fov, aspect));
    // A glass 20 by 10 cm a metre ahead and to the right, turned a little toward the eye.
    let normal = Vec3::new(-0.2, 0.0, -1.0).normalize();
    let (u, v) = (normal.cross(Vec3::Y).normalize() * 0.1, Vec3::Y * 0.05);
    let centre = Vec3::new(0.3, 0.0, 1.0);
    let corners = [centre - u - v, centre + u - v, centre + u + v, centre - u + v];
    let placed = Placed { corners, normal, side: true };
    let view = view(main, sky, screen, &placed).expect("the glass is in view");
    assert!(view.size.0.is_multiple_of(STEP) && view.size.1.is_multiple_of(STEP));
    assert!(view.size.0 <= MOST.0 && view.size.1 <= MOST.1);
    // The glass's own outline lies in the image, left and right swapped.
    for corner in corners {
        let p = ndc(view.view_projection, corner);
        assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p}");
    }
    let x = |m: Mat4, p| ndc(m, p).x;
    assert!(
        x(main, corners[0]) < x(main, corners[1])
            && x(view.view_projection, corners[0]) > x(view.view_projection, corners[1])
    );
    // A point on the eye's side shows where the eye's line to its reflection meets the glass.
    let reflect = reflection(centre, normal);
    let point = Vec3::new(-0.5, 0.2, -2.0);
    let image = reflect.transform_point3(point);
    let on_glass = image * (normal.dot(centre) / normal.dot(image));
    let (seen, glass) = (ndc(view.view_projection, point), ndc(view.view_projection, on_glass));
    assert!((seen.truncate() - glass.truncate()).length() < 1e-4, "{seen} {glass}");
    assert!((0.0..=1.0).contains(&seen.z), "{seen}");
    // What lies past the glass is cut away; the far plane lies past the mirrors' reach.
    assert!(ndc(view.view_projection, centre - normal * 0.05).z > 1.0);
    assert!((0.0..=1.0).contains(&ndc(view.view_projection, Vec3::new(0.0, 0.0, -250.0)).z));
    // A face turned to the glass keeps its winding in the image as a face turned to the eye in the main view.
    let facing_eye = [Vec3::new(-0.2, 0.0, 3.0), Vec3::new(0.2, 0.0, 3.0), Vec3::new(0.0, 0.3, 3.0)];
    let facing_glass = [2, 1, 0].map(|i| reflect.transform_point3(facing_eye[i]));
    assert_eq!(turn(main, facing_eye).signum(), turn(view.view_projection, facing_glass).signum());
    // Turned away or behind the eye, no view.
    assert!(super::view(main, sky, screen, &Placed { normal: -normal, ..placed }).is_none());
    let behind = corners.map(|c| c * Vec3::new(1.0, 1.0, -1.0));
    assert!(super::view(main, sky, screen, &Placed { corners: behind, normal: -normal, side: true }).is_none());
}

/// Each retail car's mirrors face the driver and back along the car, at least two and one a door's (as
/// `examples/mirror_survey.rs` lists them). Needs `RBR_DATA`.
#[test]
fn every_retail_car_has_mirrors() {
    let Some(data) = rbr_formats::data::Data::from_env() else { return };
    let retail = ["XSARA", "ACCENT", "ZR", "LANCER_EVO_VII", "PEUGEOT_206", "IMPREZA03", "COROLLA", "IMPREZA00"];
    for name in retail {
        let interior = crate::car::load_interior(&data, name, crate::car::placement_at(None)).unwrap();
        let exterior = crate::car::load(&data, name, crate::car::placement_at(None)).unwrap();
        let (lo, hi) = exterior.bounds().unwrap();
        let mounts = crate::views::Mounts::load(&data, name, lo, hi, &exterior.meshes, &interior.meshes).unwrap();
        let found = find(&interior.meshes, &interior.damage, mounts.cockpit.eye);
        assert!(found.unfitted.is_empty(), "{name}: {:?}", found.unfitted);
        assert!(found.mirrors.len() >= 2 && found.mirrors.iter().any(|m| m.kind == Kind::Side), "{name}: {found:?}");
        assert!(found.mirrors.iter().all(|m| m.normal.z > 0.9), "{name}: {found:?}");
    }
}

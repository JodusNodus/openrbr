//! `RBR_DATA` (and `RBR_RECORDINGS`): the in-car crew loads in every retail car, the steering wheel is found, the crew
//! keeps the stage characters' textures, and the tanner drive's gear changes move the driver's hand to the stick and
//! back.

use rbr_formats::data::Data;
use rbr_render::car;
use rbr_render::characters::crew::Move;
use rbr_render::characters::incar::Crew;
use rbr_render::characters::Characters;
use rbr_render::playback::Drive;
use rbr_render::stage::{self, Selection};

const CARS: [&str; 8] = ["ACCENT", "COROLLA", "IMPREZA00", "IMPREZA03", "LANCER_EVO_VII", "PEUGEOT_206", "XSARA", "ZR"];

#[test]
fn the_crew_sits_in_every_retail_car_and_its_wheel_is_found() {
    let Some(data) = Data::from_env() else { return };
    let mut stage = stage::load(&data.install().join("Maps"), "track-54_E", Selection::default()).expect("stage");
    for name in CARS {
        let crew = Crew::load(&data, name, &mut stage).unwrap_or_else(|e| panic!("{name}: {e}"));
        let node = car::node_place(&data, name, "i_steeringwheel")
            .expect("node place")
            .unwrap_or_else(|| panic!("{name}: no i_steeringwheel"));
        let turn = crew.wheel_turn().unwrap_or_else(|| panic!("{name}: no i_steeringwheel"));
        // The wheel turns about its node: the node stays where it is (steer 0, so the turn is the identity).
        assert!((turn.transform_point3(node) - node).length() < 1e-4, "{name}: the wheel leaves its node");
        let interior = car::load_interior(&data, name, glam::Mat4::IDENTITY).expect("interior");
        let wheels: Vec<_> = interior
            .meshes
            .iter()
            .zip(&interior.damage)
            .filter(|(_, effect)| effect.steering)
            .filter_map(|(m, _)| m.bounds.map(|b| (b.half_extent, m.vertices.len())))
            .collect();
        eprintln!("{name}: gear stick {}, wheel meshes {wheels:?}", crew.has_stick());
        // The rim (36 cm across), and in LANCER_EVO_VII a 14 cm part at the hub that turns with it.
        assert!(wheels.iter().any(|(half, _)| (half.x - 0.18).abs() < 0.01), "{name}: no rim at the wheel");
        assert!(wheels.len() <= 2, "{name}: {} meshes at the wheel", wheels.len());
    }
}

#[test]
fn the_crew_keeps_the_stage_characters_textures() {
    let Some(data) = Data::from_env() else { return };
    let mut stage = stage::load(&data.install().join("Maps"), "track-54_E", Selection::default()).expect("stage");
    Characters::load(&data, "track-54_E", &mut stage).expect("characters");
    let before = stage.textures.clone();
    Crew::load(&data, "COROLLA", &mut stage).expect("crew");
    for (key, texture) in &before {
        assert_eq!(stage.textures[key].request, texture.request, "{key:?}");
    }
    assert!(stage.textures.len() > before.len());
}

#[test]
fn tanner_gear_changes_move_the_hand_to_the_stick_and_back() {
    let (Some(data), Ok(recordings)) = (Data::from_env(), std::env::var("RBR_RECORDINGS")) else { return };
    let mut stage = stage::load(&data.install().join("Maps"), "track-54_E", Selection::default()).expect("stage");
    let mut crew = Crew::load(&data, "COROLLA", &mut stage).expect("crew");
    assert!(crew.has_stick());
    // `Misc/LM_Driver.ini`, not `Unk14`'s defaults (1.0 and 0.1 s).
    assert_eq!((crew.settings().to_gear, crew.settings().to_gear_pushed), (0.3, 0.2));
    let drive = Drive::parse(&std::fs::read(format!("{recordings}/drive-tanner-corolla.bin")).unwrap()).unwrap();
    let (mut flagged, mut pushes, mut was_pushing) = (0, 0, false);
    let mut moves = std::collections::BTreeSet::new();
    for (end, sample) in drive.frames() {
        flagged += usize::from(sample.control[5].to_bits() & 3 != 0);
        crew.step(end, sample);
        let playing = crew.playing();
        if let Some((entry, _)) = playing {
            moves.insert(format!("{:?}", entry.mv));
        }
        let pushing = playing.is_some_and(|(e, _)| e.mv == Move::GearPush);
        pushes += usize::from(pushing && !was_pushing);
        was_pushing = pushing;
    }
    // 17 flagged steps; changes less than a move apart share one trip to the stick.
    assert_eq!(flagged, 17);
    assert!((10..=17).contains(&pushes), "{pushes} pushes");
    assert!(crew.playing().is_none(), "the hand ends on the wheel");
    // No handbrake on this drive.
    assert_eq!(moves.into_iter().collect::<Vec<_>>(), ["GearPush", "WheelToGear"]);
}

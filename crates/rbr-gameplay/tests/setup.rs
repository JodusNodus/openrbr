//! The car setup (`rbr_gameplay::setup`) against the pages' sliders and on every retail car's setup files.
//!
//! The retail checks run only when `RBR_DATA` names the install; skipped otherwise.

use rbr_formats::data::Data;
use rbr_formats::lsp_keys::{key_offset, Class};
use rbr_gameplay::setup::{Bounds, CarSetup, Setup, Slot, CAR_FOLDERS, WHEELS};

/// The setup's objects in `CCompleteCarSetup` (the setup emit, 0x48e620) and the options' in
/// `CCompleteCarSetupOptions`, by the files' group names.
fn setup_object(group: &str) -> (Class, u32) {
    let wheel = |prefix: &str| WHEELS.iter().position(|w| group.strip_prefix(prefix) == Some(*w)).map(|w| w as u32);
    match group {
        "Car" => (Class::CarSetup, 0xc),
        "Drive" => (Class::DriveSetup, 0x48),
        "VehicleControlUnit" => (Class::VcuSetup, 0x6a0),
        _ if group.starts_with("Wheel") => (Class::WheelSetup, 0x1210 + 0x70 * wheel("Wheel").unwrap()),
        _ if group.starts_with("SpringDamper") => (Class::SpringSetup, 0x13d0 + 0x9c * wheel("SpringDamper").unwrap()),
        _ => (Class::TyreSetup, 0x1640 + 0x30 * wheel("Tyre").unwrap()),
    }
}

fn options_object(group: &str) -> (Class, u32) {
    let wheel = |prefix: &str| WHEELS.iter().position(|w| group.strip_prefix(prefix) == Some(*w)).map(|w| w as u32);
    match group {
        "CarOptions" => (Class::CarSetupOptions, 0x10),
        "DriveOptions" => (Class::DriveSetupOptions, 0x220),
        "ControlOptions" => (Class::VcuSetupOptions, 0x12dc),
        _ if group.starts_with("WheelOptions") => {
            (Class::WheelSetupOptions, 0x1b10 + 0x320 * wheel("WheelOptions").unwrap())
        }
        _ if group.starts_with("SpringDamperOptions") => {
            (Class::SpringSetupOptions, 0x13e4 + 0x18c * wheel("SpringDamperOptions").unwrap())
        }
        _ => (Class::TyreSetupOptions, 0x1a14 + 0x3c * wheel("TyreOptions").unwrap()),
    }
}

/// Every slider the pages build: the setup tweak and the options tweak it is given (the minimum's, the top mount's
/// maximum), offsets into the two objects, from each page's `Vf38`.
fn page_sliders() -> Vec<(u32, u32)> {
    let mut sliders = vec![
        // CTyresPage 0x554b30
        (0x165c, 0x1a20),
        (0x168c, 0x1a5c),
        (0x16bc, 0x1a98),
        (0x16ec, 0x1ad4),
        // CBrakesPage 0x554de0
        (0x5a4, 0x112c),
        (0x654, 0x12ac),
        // CRollbarPage 0x555b60
        (0x18, 0x1c),
        (0x28, 0x4c),
        (0x38, 0x7c),
        // CDifferentialTorquePage 0x550750
        (0x54, 0xf4c),
        (0x64, 0xf7c),
        (0x74, 0xfac),
        (0x6ec, 0x1348),
        (0x6fc, 0x1378),
        // CGearsPage 0x556120 (the drop gear)
        (0x4f4, 0xf2c),
    ];
    // CGeometryPageFront 0x554f80, both selections: top mount, steering rod, platform, axis inclination.
    for (wheel, options) in [(0x121c, 0x1b1c), (0x128c, 0x1e3c), (0x12fc, 0x215c), (0x136c, 0x247c)] {
        sliders.extend([
            (wheel, options),
            (wheel + 0x34, options + 0x114),
            (wheel + 0x44, options + 0x174),
            (wheel + 0x54, options + 0x144),
        ]);
    }
    // CSpringsPage 0x550990 and CDampersPage 0x550fd0, per wheel.
    for (spring, options) in [(0x13dc, 0x13f0), (0x1478, 0x157c), (0x1514, 0x1708), (0x15b0, 0x1894)] {
        for (s, o) in
            [(0, 0), (0x10, 0x30), (0x30, 0x60), (0x40, 0x90), (0x50, 0xc0), (0x60, 0xf0), (0x80, 0x120), (0x70, 0x150)]
        {
            sliders.push((spring + s, options + o));
        }
    }
    // CDifferentialMapPage 0x551610: four maps of throttle, brake, speed (the speed range) and factor.
    for i in 0..176 {
        let speed = (22..33).contains(&(i % 44));
        sliders.push((0x70c + 0x10 * i, if speed { 0x1318 } else { 0x12e8 }));
    }
    sliders
}

#[test]
fn every_value_is_a_page_slider() {
    let mut ours: Vec<(u32, u32)> = Setup::specs()
        .iter()
        .map(|spec| {
            let (class, base) = setup_object(&spec.group);
            let at = base + key_offset(class, &spec.key).unwrap_or_else(|| panic!("{} {}", spec.group, spec.key));
            let (class, base) = options_object(&spec.options);
            let bound = match spec.bounds {
                Bounds::Triple(prefix) => format!("{prefix}Min"),
                Bounds::TopMount => "nTopMountPositions".into(),
                Bounds::DropGear => "DropGearOptionsMin".into(),
            };
            (at, base + key_offset(class, &bound).unwrap_or_else(|| panic!("{} {bound}", spec.options)))
        })
        .collect();
    let mut pages = page_sliders();
    ours.sort_unstable();
    pages.sort_unstable();
    assert_eq!(ours, pages);
    // The wrap flag (`E55_MakeSliderA`'s eighth argument) is set on the tyre and brake pressures only.
    let wrapping: Vec<String> = Setup::specs().into_iter().filter(|s| s.wrap).map(|s| s.key).collect();
    assert_eq!(
        wrapping,
        ["MaxBrakePressureFront", "MaxBrakePressureRear", "Pressure", "Pressure", "Pressure", "Pressure"]
    );
}

#[test]
fn retail_setups_parse_and_read_back() {
    let Some(data) = Data::from_env() else {
        eprintln!("RBR_DATA is not set: skipped");
        return;
    };
    let mut outside = Vec::new();
    for (car, folder) in CAR_FOLDERS.iter().enumerate() {
        for surface in ["gravel", "tarmac", "snow"] {
            let what = format!("{folder}{surface}");
            let setup = CarSetup::load(&data, car, surface).unwrap_or_else(|e| panic!("{what}: {e}"));
            assert_eq!(setup.ranges.len(), Setup::specs().len());
            // The written default reads back to the same floats, over nothing and over another setup.
            let text = setup.default.to_text();
            assert_eq!(Setup::from_text(&Setup::default(), &text), Ok(setup.default), "{what}");
            assert!(!setup.gears.is_empty(), "{what}: no gear list");
            let mut default = setup.default;
            let mut ranges = setup.ranges.iter();
            default.visit(&mut |spec, slot| {
                let range = ranges.next().unwrap();
                let v = match slot {
                    Slot::Real(v) => *v,
                    Slot::Unsigned(v) => *v as f32,
                };
                if v < range.min || v > range.max {
                    outside
                        .push(format!("{what} {} {}: {v} outside {}..{}", spec.group, spec.key, range.min, range.max));
                }
            });
        }
    }
    // Defaults the retail options do not cover (the game shows them until the first press clamps them).
    for line in &outside {
        eprintln!("{line}");
    }
}

#[test]
fn the_default_is_the_default_file() {
    let Some(data) = Data::from_env() else {
        eprintln!("RBR_DATA is not set: skipped");
        return;
    };
    let tanner = CarSetup::load(&data, 6, "gravel").unwrap();
    let setup = tanner.default;
    assert_eq!((setup.steering_lock, setup.front_roll_bar, setup.rear_roll_bar), (1.4, 20000.0, 30000.0));
    assert_eq!((setup.front_brake_pressure, setup.rear_brake_pressure), (6500000.0, 5400000.0));
    assert_eq!((setup.center_diff_torque, setup.front_diff_torque, setup.rear_diff_torque), (700.0, 320.0, 600.0));
    assert_eq!((setup.handbrake_release, setup.left_foot_brake_threshold, setup.drop_gear), (0.5, 0.2, 11));
    assert_eq!(setup.diff_maps[3].speed_factor[5], 0.85);
    assert_eq!(setup.diff_maps[1].brake[6], 0.8);
    let lf = setup.corners[0];
    assert_eq!((lf.tyre_pressure, lf.spring_stiffness, lf.bump, lf.fast_bump), (220000.0, 50000.0, 5250.0, 7900.0));
    assert_eq!((lf.top_mount_slot, lf.steering_rod_length, lf.axis_inclination), (4.0, 0.3562, -0.07));
    assert_eq!(setup.corners[2].platform_height, 0.03);
    let spring = tanner.ranges[Setup::specs().iter().position(|s| s.key == "SpringLength").unwrap()];
    assert_eq!((spring.min, spring.max, spring.step, spring.wrap), (0.15, 0.35, 0.005, false));
    assert_eq!(tanner.gears[11], (11, 28.0, 22.0));
}

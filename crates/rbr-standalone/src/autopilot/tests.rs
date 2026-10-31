use std::path::Path;

use super::*;

fn record(x: f32, value: f32) -> DriveLineRecord {
    DriveLineRecord {
        vectors: [Vec3 { x, y: 0.0, z: 0.0 }, Vec3 { x: 10.0, y: 0.0, z: 0.0 }],
        value,
        unknown_u16: [0; 2],
    }
}

/// The controller on `track`, with its values as a stage gives them.
fn driver(track: &[DriveLineRecord], top: f32) -> Driver<'_> {
    Driver::new(track, track.iter().map(|r| r.value).collect(), top, None)
}

#[test]
fn the_spline_passes_its_records_and_the_parameter_inverts_the_distance() {
    let track = [record(0.0, 0.0), record(10.0, 10.0), record(20.0, 20.0)];
    let values = [0.0, 10.0, 20.0];
    assert_eq!(point(&track, 1.0), [10.0, 0.0, 0.0]);
    assert_eq!(point(&track, 1.5), [15.0, 0.0, 0.0]);
    assert_eq!(parameter(&values, 15.0), 1.5);
    assert_eq!(distance(&values, parameter(&values, 4.0)), 4.0);
    assert_eq!(parameter(&values, 25.0), 2.0);
}

#[test]
fn aim_takes_a_point_and_an_optional_coast() {
    assert_eq!("1.5,-2,30".parse(), Ok(Aim { at: [1.5, -2.0], coast: 30 }));
    assert_eq!("1.5,-2".parse(), Ok(Aim { at: [1.5, -2.0], coast: 0 }));
    assert!("1.5".parse::<Aim>().is_err());
}

#[test]
fn a_straight_plans_the_top_speed_and_steers_ahead() {
    let track: Vec<_> = (0..40).map(|i| record(10.0 * i as f32, 10.0 * i as f32)).collect();
    let mut driver = driver(&track, TOP_SPEED);
    assert_eq!(driver.target_speed(0.0), TOP_SPEED);
    // Facing +x (body -Y is forward: the identity quaternion faces -y, so turn 90 degrees about z).
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let sense = Sense {
        pose: CarPose { position: [5.0, 0.0, 0.0], orientation: [0.0, 0.0, half, half], velocity: [10.0, 0.0, 0.0] },
        parameter: 0.5,
        gear: FIRST,
        engine: 500.0,
        held: false,
    };
    let q = driver.control(1000, &sense, false);
    assert_eq!(q.steering, 0);
    assert_eq!((q.throttle, q.brake, q.flags), (255, 0, 0));
}

#[test]
fn a_chicane_is_passed_prop_by_prop_and_slowly() {
    // Along +x (left is +y): a box right of the line at 50 m, one left of it at 61 m (RSF 468's at 722 and 733 m).
    let track: Vec<_> = (0..40).map(|i| record(10.0 * i as f32, 10.0 * i as f32)).collect();
    let square = |x: f32, y: f32| Obstacle { footprint: vec![[x, y], [x + 1.0, y], [x + 1.0, y + 1.0], [x, y + 1.0]] };
    let driver = driver(&track, TOP_SPEED).with_obstacles(&[square(50.0, -1.0), square(61.0, 0.3)]);
    assert_eq!(driver.props.len(), 2);
    assert_eq!(driver.dodge(30.0, 45.0), 0.0);
    assert_eq!(driver.dodge(40.0, 65.0), CLEAR);
    assert!((driver.dodge(54.0, 70.0) - (0.3 - CLEAR)).abs() < 1e-4);
    assert_eq!(driver.target_speed(45.0), DODGE_TOP);
    assert_eq!(driver.target_speed(70.0), TOP_SPEED);
}

#[test]
fn a_car_leaning_to_its_right_steers_right() {
    let track: Vec<_> = (0..40).map(|i| record(10.0 * i as f32, 10.0 * i as f32)).collect();
    let mut driver = driver(&track, TOP_SPEED);
    // Facing +x and rolled 0.4 rad about it: the up axis leans towards -y, the car's right.
    let (half, (s, c)) = (std::f32::consts::FRAC_1_SQRT_2, 0.2f32.sin_cos());
    let pose = CarPose {
        position: [5.0, 0.0, 0.0],
        orientation: [half * s, -half * s, half * c, half * c],
        velocity: [10.0, 0.0, 0.0],
    };
    assert!((lean(&pose) - 0.4f32.sin()).abs() < 1e-5, "{}", lean(&pose));
    let sense = Sense { pose, parameter: 0.5, gear: FIRST, engine: 500.0, held: false };
    assert_eq!(driver.control(1000, &sense, false).steering, 127);
}

#[test]
fn under_ngp_a_standing_start_revs_in_neutral_and_a_bogged_car_starts_again() {
    let track: Vec<_> = (0..40).map(|i| record(10.0 * i as f32, 10.0 * i as f32)).collect();
    let mut driver = driver(&track, TOP_SPEED).with_step(1.0 / 720.0).with_limit(8500.0);
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let sense = |speed: f32, gear: u32, engine: f32| Sense {
        pose: CarPose { position: [5.0, 0.0, 0.0], orientation: [0.0, 0.0, half, half], velocity: [speed, 0.0, 0.0] },
        parameter: 0.5,
        gear,
        engine,
        held: false,
    };
    let gap = driver.steps(SHIFT_GAP);
    let q = driver.control(gap, &sense(0.0, FIRST - 1, 105.0), false);
    assert_eq!((q.flags, q.throttle), (0, 255));
    assert_eq!(driver.control(gap + 1, &sense(0.0, FIRST - 1, 600.0), false).flags, 0x01);
    let mut step = gap + 2;
    let pulse = (0..2 * gap).find_map(|_| {
        step += 1;
        let flags = driver.control(step, &sense(1.5, FIRST, 89.0), false).flags;
        (flags != 0).then_some(flags)
    });
    assert_eq!(pulse, Some(0x02));
}

#[test]
fn a_stuck_car_backs_up_shifts_back_to_first_and_gives_up_after_the_attempts() {
    let track: Vec<_> = (0..40).map(|i| record(10.0 * i as f32, 10.0 * i as f32)).collect();
    let mut driver = driver(&track, TOP_SPEED);
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let sense = |speed: f32, gear: u32| Sense {
        pose: CarPose { position: [5.0, 0.0, 0.0], orientation: [0.0, 0.0, half, half], velocity: [speed, 0.0, 0.0] },
        parameter: 0.5,
        gear,
        engine: 800.0,
        held: false,
    };
    driver.control(1, &sense(10.0, 4), false);
    let mut step = 2;
    let mut drive = |driver: &mut Driver, steps: u32, speed: f32, gear: u32| {
        let mut last = QuantizedControl::default();
        for _ in 0..steps {
            last = driver.control(step, &sense(speed, gear), false);
            step += 1;
        }
        last
    };
    for attempt in 0..ATTEMPTS {
        drive(&mut driver, RECOVER_AFTER - 1, 0.0, 4);
        assert_eq!(driver.mode, Mode::Drive, "attempt {attempt}");
        // Braking and shifting down to reverse.
        let q = drive(&mut driver, 1, 0.0, 4);
        assert!(matches!(driver.mode, Mode::Reverse(_)));
        assert_eq!((q.throttle, q.brake), (0, 255));
        assert_eq!(drive(&mut driver, RECOVER_GAP, 0.0, 4).flags, 0x02);
        let q = drive(&mut driver, 1, 0.5, REVERSE);
        assert!(q.throttle > 0 && q.brake == 0);
        drive(&mut driver, REVERSE_MAX, 0.5, REVERSE);
        assert_eq!(driver.mode, Mode::Forward);
        assert_eq!(drive(&mut driver, RECOVER_GAP, 0.0, REVERSE).flags, 0x01);
        drive(&mut driver, 1, 0.0, FIRST);
        assert_eq!(driver.mode, Mode::Drive);
    }
    assert_eq!(driver.gave_up, None);
    drive(&mut driver, RECOVER_AFTER, 0.0, 4);
    assert!(driver.gave_up.as_deref().is_some_and(|w| w.starts_with("stuck at step")));
}

#[test]
fn a_tangent_far_longer_than_its_chord_does_not_loop() {
    // RSF 302's start: a 10 m segment ending on a 160 m tangent (the next segment's chord).
    let mut track = vec![record(0.0, 0.0), record(10.0, 10.0), record(170.0, 170.0)];
    track[1].vectors[1].x = 160.0;
    track[2].vectors[1].x = 160.0;
    let xs: Vec<f32> = (0..=10).map(|k| point(&track, k as f32 / 10.0)[0]).collect();
    assert!(xs.windows(2).all(|w| w[0] <= w[1] && w[1] <= 10.0), "{xs:?}");
    assert_eq!(point(&track, 1.5)[0], 90.0);
}

#[test]
fn a_segment_that_runs_back_along_its_chord_does_not_bulge() {
    // RSF 370 at 454 m: a 13 m segment ending on a tangent 6.9 times as long, at an angle to it (unfitted, it
    // bulges 4.4 m to the side and runs back 3.2 m).
    let mut track = vec![record(0.0, 0.0), record(13.0, 13.0), record(90.0, 90.0)];
    track[0].vectors[1].x = 13.0;
    track[1].vectors[1] = Vec3 { x: 84.0, y: 30.0, z: 0.0 };
    let p = |u: f32| point(&track, u);
    assert!((0..=10).all(|k| p(k as f32 / 10.0)[1].abs() < 1.5), "{:?}", p(0.5));
    assert!((0..10).all(|k| p(k as f32 / 10.0)[0] <= p((k + 1) as f32 / 10.0)[0]));
}

/// The retail drivelines (`RBR_DATA`) keep every tangent: their autopilot drives are unchanged by the fit.
#[test]
fn no_retail_segment_is_fitted() {
    let Some(data) = std::env::var_os("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let maps = Path::new(&data).join("Maps");
    let mut checked = 0;
    for entry in std::fs::read_dir(&maps).expect("Maps") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "trk") {
            continue;
        }
        let trk = rbr_formats::trk::parse(&std::fs::read(&path).expect("read")).expect("parse");
        let Some(track) = trk.chunks.iter().find_map(|c| match &c.payload {
            rbr_formats::trk::ChunkPayload::DriveLine(d) => Some(&d.records),
            _ => None,
        }) else {
            continue;
        };
        let v = |v: Vec3| [v.x, v.y, v.z];
        for (i, w) in track.windows(2).enumerate() {
            let (p0, m0, p1, m1) = (v(w[0].vectors[0]), v(w[0].vectors[1]), v(w[1].vectors[0]), v(w[1].vectors[1]));
            let chord = dot(sub(p1, p0), sub(p1, p0)).sqrt();
            let long = |m: [f32; 3]| dot(m, m).sqrt() > LOOP * chord;
            assert!(!(runs_back(p0, m0, p1, m1) > BACK || long(m0) || long(m1)), "{} segment {i}", path.display());
        }
        checked += 1;
    }
    assert!(checked >= 80, "{checked} drivelines");
}

#[test]
fn an_outside_assistance_hold_brakes_and_ends_the_recovery() {
    let track: Vec<_> = (0..40).map(|i| record(10.0 * i as f32, 10.0 * i as f32)).collect();
    let mut driver = driver(&track, TOP_SPEED);
    driver.mode = Mode::Reverse(0);
    driver.slow = 100;
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let sense = Sense {
        pose: CarPose { position: [5.0, 0.0, 0.0], orientation: [0.0, 0.0, half, half], velocity: [0.0; 3] },
        parameter: 0.5,
        gear: FIRST - 1,
        engine: 100.0,
        held: true,
    };
    let q = driver.control(1000, &sense, false);
    assert_eq!((q.flags, q.throttle, q.brake), (0, 0, 255));
    assert_eq!((driver.mode, driver.slow), (Mode::Drive, 0));
    assert_eq!(driver.control(1001, &Sense { held: false, ..sense }, false).flags, 0x01);
}

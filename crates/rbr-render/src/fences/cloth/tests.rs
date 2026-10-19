use super::solve::{edge_hit, inside};
use super::*;
use glam::{Mat4, Vec3};
use rbr_formats::fnc::{FncGroup, FncPost};

/// A straight tape line of `n` posts 3 m apart along x, ground at y = 0.
fn tape(n: usize, fence_type: u32, post_type: u32) -> Line {
    let posts = (0..n)
        .map(|i| {
            let x = i as f32 * 3.0;
            FncPost { position: [x, 0.0, 0.0], center: [x + 1.5, 0.3, 0.0], half_extent: [1.5, 0.8, 0.5], color: !0 }
        })
        .collect();
    let half = n as f32 * 1.5;
    Line::new(&FncGroup {
        fence_type,
        post_type,
        material: 0,
        rail_material: 1,
        center: [half, 0.3, 0.0],
        half_extent: [half, 0.8, 0.5],
        posts,
    })
}

/// A 4 x 1.4 x 2 m car box centred at `at`, axis-aligned, moving by `motion` per step.
fn car_at(at: Vec3, motion: Vec3) -> CarShape<'static> {
    CarShape {
        to_stage: Mat4::from_translation(at),
        faces: &[],
        lo: Vec3::new(-1.0, -0.7, -2.0),
        hi: Vec3::new(1.0, 0.7, 2.0),
        motion,
    }
}

/// A camera that sees no line.
fn unseen(_: Vec3, _: Vec3) -> bool {
    false
}

fn far_car() -> CarShape<'static> {
    car_at(Vec3::new(1000.0, 0.0, 1000.0), Vec3::ZERO)
}

#[test]
fn build_counts_follow_the_type_table() {
    let line = tape(3, 0, 4);
    // Posts: one layer of two rows, then top, anchor and foot.
    let post = &line.segments[0];
    assert!(post.post);
    assert_eq!(post.count, 2 + 3);
    assert_eq!(post.springs.len(), 2);
    assert_eq!(post.bends.len(), 2);
    assert_eq!(post.pins, vec![post.start + 2]);
    // Spans: 7 inner columns of 2 rows; 4 springs per cell, 6 bottom, 1 end, 8 to the posts.
    let span = &line.segments[1];
    assert!(!span.post);
    assert_eq!(span.count, 7 * 2);
    assert_eq!(span.springs.len(), 6 * 4 + 6 + 1 + 8);
    assert_eq!(line.segments.len(), 5);
    assert_eq!(line.span_segment, vec![Some(1), Some(3), None]);
    assert_eq!(line.post_segment, vec![0, 2, 4]);
    // The tape hangs from 1 m, 0.074 m tall, and spans post to post.
    let first = line.particles[span.start].pos;
    assert!((first.x - 3.0 / 8.0).abs() < 1e-5 && (first.y - 1.0).abs() < 1e-5);
    assert!((line.particles[span.start + 1].pos.y - 0.926).abs() < 1e-5);
}

#[test]
fn rest_barely_moves_and_falls_asleep() {
    // The post's top follows its column's first particle (`E91SolveDeform`), so a built post settles a few
    // millimetres under its 1.1 m; then the whole line sleeps.
    let mut line = tape(3, 0, 4);
    line.built = true;
    let before = line.particles.clone();
    for _ in 0..300 {
        line.step(0.01, Vec3::ZERO, &far_car());
    }
    let most = line.particles.iter().zip(&before).map(|(a, b)| a.pos.distance(b.pos)).fold(0.0, f32::max);
    assert!(most < 0.01, "{most}");
    assert!(line.segments.iter().all(|s| s.flags & ASLEEP != 0));
}

#[test]
fn the_plane_is_the_ground_along_the_line() {
    let line = tape(3, 0, 4);
    let [nx, ny, nz, d] = line.segments[0].plane;
    assert!((nx.abs() + nz.abs()) < 1e-6 && (ny - 1.0).abs() < 1e-6 && (d + 0.05).abs() < 1e-6);
}

#[test]
fn a_car_through_the_tape_tears_it_and_it_falls() {
    let mut fences = Fences { lines: vec![tape(3, 0, 4)] };
    // The car crosses the line at x = 4.5 (the middle of the first span), driving along +z at 15 m/s.
    let speed = 15.0;
    let dt = 0.01;
    let mut z = -4.0;
    let mut furthest = f32::MIN;
    for _ in 0..60 {
        z += speed * dt;
        let car = car_at(Vec3::new(1.5, 0.7, z), Vec3::new(0.0, 0.0, speed * dt));
        fences.step(dt, Vec3::ZERO, &car, &unseen);
        let line = &fences.lines[0];
        let span = &line.segments[1];
        furthest = (span.start..span.start + span.count).map(|i| line.particles[i].pos.z).fold(furthest, f32::max);
    }
    let line = &fences.lines[0];
    assert!(line.built);
    let span = &line.segments[1];
    assert!(span.flags & LOOSE != 0, "the touched span tears");
    assert!(span.active < span.springs.len(), "springs were cut");
    // The car pushes the tape along +z, about to its front.
    assert!(furthest > 2.0, "{furthest}");
    // The car drives on; the loose tape falls to the ground plane and settles there.
    for _ in 0..600 {
        fences.step(dt, Vec3::ZERO, &far_car_near(Vec3::new(1.5, 0.7, 40.0)), &unseen);
    }
    let line = &fences.lines[0];
    let span = &line.segments[1];
    let lowest = (span.start..span.start + span.count).map(|i| line.particles[i].pos.y).fold(f32::MAX, f32::min);
    assert!(lowest < 0.5, "the torn tape lies low: {lowest}");
    assert!(lowest > -0.1, "but over the ground: {lowest}");
    // The untouched span stays up.
    let other = &line.segments[3];
    assert_eq!(other.flags & LOOSE, 0);
}

fn far_car_near(at: Vec3) -> CarShape<'static> {
    car_at(at, Vec3::ZERO)
}

#[test]
fn a_post_pushed_past_half_a_metre_breaks_loose() {
    let mut line = tape(2, 0, 4);
    line.built = true;
    let post = line.post_segment[0];
    let top = line.segments[post].start + 2;
    line.particles[top].pos.z += 0.8;
    line.particles[line.segments[post].start].pos.z += 0.8;
    line.step(0.01, Vec3::ZERO, &far_car());
    assert!(line.segments[post].flags & LOOSE != 0);
}

#[test]
fn a_far_car_resets_a_built_line() {
    let mut fences = Fences { lines: vec![tape(2, 0, 4)] };
    let rest = fences.lines[0].particles.clone();
    let car = car_at(Vec3::new(1.5, 0.7, 0.0), Vec3::new(0.0, 0.0, 0.2));
    fences.step(0.01, Vec3::ZERO, &car, &unseen);
    assert!(fences.lines[0].built);
    assert_ne!(fences.lines[0].particles, rest);
    // Seen, it stays as the car left it.
    fences.step(0.01, Vec3::ZERO, &far_car(), &|_, _| true);
    assert!(fences.lines[0].built);
    let changed = fences.step(0.01, Vec3::ZERO, &far_car(), &unseen);
    assert_eq!(changed, vec![0]);
    assert!(!fences.lines[0].built);
    assert_eq!(fences.lines[0].particles, rest);
}

#[test]
fn nets_and_two_layer_tape_build() {
    for (span, post) in [(2, 5), (3, 5), (12, 14), (13, 14), (1, 4)] {
        let line = tape(4, span, post);
        let t = TYPES[span as usize];
        let spans = line.span_segment.iter().flatten().count();
        assert_eq!(spans, 3);
        let s = &line.segments[line.span_segment[0].unwrap()];
        assert_eq!(s.count, ((t.cols - 2) * t.rows * t.layers) as usize);
    }
}

#[test]
fn edge_hits_and_inside_on_the_box_mesh() {
    let faces = box_faces(Vec3::splat(-1.0), Vec3::splat(1.0));
    let (at, normal) = edge_hit(&faces, Vec3::new(0.0, 3.0, 0.0), Vec3::new(0.0, -3.0, 0.0)).unwrap();
    assert!((at - Vec3::Y).length() < 1e-6 && (normal - Vec3::Y).length() < 1e-6, "{at} {normal}");
    assert!(edge_hit(&faces, Vec3::new(2.0, 3.0, 0.0), Vec3::new(2.0, -3.0, 0.0)).is_none());
    // From inside, no face is entered.
    assert!(edge_hit(&faces, Vec3::ZERO, Vec3::new(0.0, -3.0, 0.0)).is_none());
    assert!(inside(&faces, Vec3::new(0.3, -0.2, 0.9)));
    assert!(!inside(&faces, Vec3::new(1.3, -0.2, 0.0)));
}

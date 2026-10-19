use rbr_physics::mem::Mem;
use rbr_physics::rigid_body::layout;

#[test]
fn point_velocity_is_the_cross_with_w_plus_the_velocity_in_three_lanes() {
    let mut mem = Mem::new();
    let (body, point, out) = (0x1000, 0x2000, 0x3000);
    mem.map(body, &[0; 0x400]);
    mem.map(point, &[0; 0x10]);
    mem.map(out, &[0; 0x10]);
    for (k, v) in [1.0f32, 2.0, 3.0].into_iter().enumerate() {
        mem.set_f32(body + 0x140 + 4 * k as u32, v);
    }
    for (k, v) in [0.0f32, 0.0, 1.0].into_iter().enumerate() {
        mem.set_f32(body + 0x190 + 4 * k as u32, v);
    }
    for (k, v) in [0.5f32, 0.25, 0.125].into_iter().enumerate() {
        mem.set_f32(body + 0x1C0 + 4 * k as u32, v);
    }
    for (k, v) in [2.0f32, 2.0, 3.0].into_iter().enumerate() {
        mem.set_f32(point + 4 * k as u32, v);
    }
    mem.set_f32(out + 0xC, 7.0);

    layout::point_velocity(&mut mem, body, point, out);

    // (point - pos) = (1, 0, 0); (1, 0, 0) x (0, 0, 1) = (0, -1, 0); plus the velocity.
    assert_eq!([mem.f32(out), mem.f32(out + 4), mem.f32(out + 8)], [0.5, -0.75, 0.125]);
    assert_eq!(mem.f32(out + 0xC), 7.0, "the fourth lane is not written");
}

#[test]
fn def_flag_reads_the_definitions_word_at_plus_1ec() {
    let mut mem = Mem::new();
    let (body, def) = (0x1000, 0x2000);
    mem.map(body, &[0; 0x400]);
    mem.map(def, &[0; 0x200]);
    mem.set_u32(body + 0x34, def);

    assert!(!layout::def_flag(&mem, body));
    mem.set_u32(def + 0x1EC, 4);
    assert!(layout::def_flag(&mem, body));
}

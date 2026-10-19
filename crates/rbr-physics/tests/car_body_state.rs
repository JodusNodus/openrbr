//! `CCarBody::CState::Vf44x` (`car_body::layout::state_vf44x`) against a recording of the state's calls: the order, the
//! bounds and the null owner. The asm decides (see `car_body/state.rs`).

use rbr_physics::car_body::layout::state_vf44x;
use rbr_physics::car_body::state::{StateCalls, BOUND};
use rbr_physics::mem::Mem;

#[derive(Debug, PartialEq)]
enum Call {
    Grid(u32),
    Clear(u32, u32, u32, u32),
    Copy(u32, u32),
}

#[derive(Default)]
struct Record(Vec<Call>);

impl StateCalls for Record {
    fn grid(&mut self, owner: u32) -> u32 {
        self.0.push(Call::Grid(owner));
        0x3000
    }

    fn clear_grid(&mut self, owner: u32, grid: u32, plus: u32, minus: u32) {
        self.0.push(Call::Clear(owner, grid, plus, minus));
    }

    fn copy_from(&mut self, dst: u32, src: u32) -> u32 {
        self.0.push(Call::Copy(dst, src));
        dst
    }
}

#[test]
fn vf44x_calls_in_the_original_order() {
    let (state, scratch) = (0x1000, 0x5000);
    let mut mem = Mem::new();
    mem.set_u32(state + 0x40, 0x2000);
    let mut calls = Record::default();
    let eax = state_vf44x(&mut calls, &mut mem, state, scratch);
    assert_eq!(eax, Some(state + 0x30));
    assert_eq!(
        calls.0,
        vec![
            Call::Grid(0x2000),
            Call::Clear(0x2000, 0x3000, scratch, scratch + 0x10),
            Call::Copy(state + 0x20, scratch),
            Call::Copy(state + 0x30, scratch + 0x10),
        ]
    );
    assert_eq!(mem.u32(state + 0x1c), 1);
    assert_eq!(mem.f32(scratch), BOUND);
    assert_eq!(mem.f32(scratch + 0x8), BOUND);
    assert_eq!(mem.f32(scratch + 0x10), -BOUND);
    assert_eq!(mem.f32(scratch + 0x18), -BOUND);
}

#[test]
fn vf44x_without_an_owner_writes_nothing() {
    let state = 0x1000;
    let mut mem = Mem::new();
    mem.set_u32(state + 0x40, 0);
    mem.set_u32(state + 0x1c, 0);
    let mut calls = Record::default();
    assert_eq!(state_vf44x(&mut calls, &mut mem, state, 0x5000), None);
    assert!(calls.0.is_empty());
    assert_eq!(mem.u32(state + 0x1c), 0);
}

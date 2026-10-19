//! The enable flag of a mounted body (`CMountedRigidBody`, `RBR_E45_MountedA.cpp`): the body's pointer at +0x580 points
//! at a state word. `M1920` tests the state for 1 and `M1940` for 2. `M1910` points the body at a state word, and
//! `M1640` clears a state word.

use super::layout;
use crate::mem::Mem;

/// `E45_M1920` (0x521920): the flag is 1.
pub fn flag_is_one(mem: &Mem, body: u32) -> bool {
    layout::mounted_state(mem, body) == 1
}

/// `E45_M1940` (0x521940): the flag is 2.
pub fn flag_is_two(mem: &Mem, body: u32) -> bool {
    layout::mounted_state(mem, body) == 2
}

/// `E45Smalls::M1910` (0x521910): points the body's flag at `state`.
pub fn set_state(mem: &mut Mem, body: u32, state: u32) {
    layout::set_mounted_state(mem, body, state);
}

/// `E45_M1640` (0x521640): writes zero to the word at `state` (what the state object is is not known).
pub fn clear_state(mem: &mut Mem, state: u32) {
    layout::set_word(mem, state, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: u32 = 0x1000;
    const STATE: u32 = 0x2000;

    /// A body (0x584 bytes) whose flag points at a state word holding `state`.
    fn body(state: u32) -> Mem {
        let mut mem = Mem::new();
        mem.map(BODY, &[0; 0x584]);
        mem.map(STATE, &[0; 4]);
        mem.set_u32(BODY + 0x580, STATE);
        mem.set_u32(STATE, state);
        mem
    }

    #[test]
    fn flag_states_one_and_two() {
        for (state, one, two) in [(0, false, false), (1, true, false), (2, false, true)] {
            let mem = body(state);
            assert_eq!(flag_is_one(&mem, BODY), one);
            assert_eq!(flag_is_two(&mem, BODY), two);
        }
    }

    #[test]
    fn set_state_points_the_flag_at_another_word() {
        let mut mem = body(0);
        mem.map(0x3000, &[0; 4]);
        set_state(&mut mem, BODY, 0x3000);
        mem.set_u32(0x3000, 2);
        assert!(flag_is_two(&mem, BODY));
    }
}

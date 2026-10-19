//! Flag dispatch (`CScratchSim::FlagDispatch`, 0x537a80, called UpdateD in the notes): a packet of
//! request flags turns into gear requests and lockouts. The asm decides the order; the slip query
//! (`E50_QuerySlip`) is an input here, as the drive accessors are.

use super::gear::{request, set_gear};
use super::layout::{Def, Gearbox, Packet, State, Tuning};

/// The flags: 1 requests an upshift, 2 a downshift unless the slip query holds, 4 sets the gear to the
/// packet's value.
const UP: u8 = 1;
const DOWN: u8 = 2;
const SET: u8 = 4;

/// Applies the packet. `query_slip` is `E50_QuerySlip` (read only when the downshift flag is set).
/// Returns whether anything was requested (the asm's return value).
pub fn flag_dispatch(
    packet: Packet,
    query_slip: impl FnOnce() -> bool,
    vcu_enabled: u32,
    gear_count: u32,
    def: &Def,
    tuning: &Tuning,
    state: &mut State,
    drive: &mut Gearbox,
) -> bool {
    let mut on = false;
    if packet.flags & UP != 0 {
        request(drive, gear_count, 1);
        state.shift_lockout = def.shift_lockout_up_s;
        on = true;
    }
    if packet.flags & DOWN != 0 && !query_slip() {
        request(drive, gear_count, -1);
        state.shift_lockout = def.shift_lockout_down_s;
        on = true;
    }
    if packet.flags & SET != 0 {
        set_gear(drive, gear_count, packet.value);
        state.shift_lockout = def.shift_lockout_down_s;
        on = true;
    } else if !on {
        return false;
    }
    if vcu_enabled != 0 {
        // The pedal swap follows the commanded gear: 1 below gear 1 (reverse or neutral), else 0.
        state.pedal_swap = u32::from((drive.commanded as i32) < 1);
    }
    state.shift_cooldown = tuning.cooldown;
    on
}

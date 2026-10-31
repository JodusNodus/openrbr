//! The solid's rest and window slots: the filing, wake and sleep, `E43Vf158`.

use super::sleep_body::Body;
use super::vtable_target;
use crate::mem::Mem;
use crate::physics_core::layout as core_layout;
use crate::physics_core::layout::Filing;
use crate::rigid_body::emit::{BodyEvent, SLEEP, WAKE};
use crate::rigid_body::host::HostUnavailable;
use crate::rigid_body::{self, sleep};

/// The calls `CSolid::E43Vf90` (0x52bf30) and `CSolid::E43Vf94` (0x52c130) make outside the body; `CEntity::E43Vf90`
/// and `E43Vf94`, which they call directly, are `core_layout::entity_rel` and need the body's filing.
pub trait RestHost: core_layout::FilingHost {
    /// The body's slot 18 (0x48; `CSolid::S18`, 0x529dc0, for the solids so far), `E43Vf90`'s only.
    fn slot18(&mut self, mem: &mut Mem, this: u32) -> Result<(), HostUnavailable>;
    /// `CMultiSignal1<const CSigMrRBodyEvent&>::Emit` (0x51f7c0): the body's event.
    fn emit(&mut self, mem: &Mem, event: &BodyEvent) -> Result<(), HostUnavailable>;
}

/// `CSolid`'s filing slots, as `CState::RegWall` (0x5263d0) asks them: 0x8c (`CSolid::E43Vf8C`, 0x5293e0, the very-soft
/// word), 0x80 (`CSolid::E43Vf80`, 0x5293d0, the active word) and 0x88 (`CEntity`'s, 0x519a40, zero).
const SOLID_FILING: [(u32, u32); 3] = [(0x8C, 0x0052_93E0), (0x80, 0x0052_93D0), (0x88, 0x0051_9A40)];
pub(super) const VERY_SOFT: u32 = 0x1EC;
pub(super) const ACTIVE: u32 = 0x1DC;

/// The filing of the solid at `solid` (`core_layout::FilingHost`): its very-soft and active words, `Err` unless its
/// filing slots are `CSolid`'s. The slots only read the words, so asking them before `WallRel`'s unlink is the same.
pub fn solid_filing(mem: &Mem, solid: u32) -> Result<Filing, HostUnavailable> {
    if !SOLID_FILING.iter().all(|&(slot, function)| vtable_target(mem, solid, slot) == function) {
        return Err(HostUnavailable);
    }
    Ok((mem.u32(solid + VERY_SOFT) != 0, mem.u32(solid + ACTIVE) != 0, false))
}

/// The body event at `at` (0x50 bytes, `emit::BodyEvent`): the words at +0x8 and +0xc are not the event's (the
/// original leaves them as its frame holds them).
pub fn store_body_event(mem: &mut Mem, at: u32, event: &BodyEvent) {
    mem.set_u32(at, event.kind);
    mem.set_u32(at + 0x4, event.owner);
    event.position.store(mem, at + 0x10);
    event.orientation.store(mem, at + 0x20);
    event.position_again.store(mem, at + 0x30);
    event.orientation_again.store(mem, at + 0x40);
}

/// `CSolid::E43Vf90` (0x52bf30, slot 36) on the body at `this`: `sleep::wake_up`, the body's slot 18,
/// `CEntity::E43Vf90` and the body's wake event (`emit::emit_body_event` on a loaded body).
pub fn solid_wake(mem: &mut Mem, host: &mut impl RestHost, this: u32) -> Result<(), HostUnavailable> {
    let mut body = Body::load(mem, this);
    sleep::wake_up(&mut body);
    body.store(mem, this);
    host.slot18(mem, this)?;
    core_layout::entity_rel(mem, host, this)?;
    let body = Body::load(mem, this);
    host.emit(mem, &rigid_body::emit::event(&body, WAKE))
}

/// `CSolid::E43Vf94` (0x52c130, slot 37) on the body at `this`: `sleep::fall_asleep`, then `CEntity::E43Vf94` and the
/// body's sleep event.
pub fn solid_sleep(mem: &mut Mem, host: &mut impl RestHost, this: u32) -> Result<(), HostUnavailable> {
    let mut body = Body::load(mem, this);
    sleep::fall_asleep(&mut body);
    body.store(mem, this);
    core_layout::entity_rel(mem, host, this)?;
    let body = Body::load(mem, this);
    host.emit(mem, &rigid_body::emit::event(&body, SLEEP))
}

/// The calls `CSolid::E43Vf158` makes at the original stack depths: slot 0 on a parentless body, the entity's slot-86
/// helper, and the body's event emit.
pub trait VerySoftHost {
    fn slot0(&mut self, mem: &Mem, sp: u32, this: u32) -> Result<(), HostUnavailable>;
    fn entity_vf158(&mut self, mem: &mut Mem, sp: u32, this: u32) -> Result<(), HostUnavailable>;
    fn emit(&mut self, mem: &mut Mem, frame: u32, event: &BodyEvent) -> Result<u32, HostUnavailable>;
}

/// `CSolid::E43Vf158` (0x52c030, slot 86): a parentless body becomes very soft and is refiled; both parent branches
/// clear the armed word and emit the same 0x50-byte event with kind 2. `frame` is `((entry - 4) & !0xf) - 0x64`.
pub fn solid_vf158(mem: &mut Mem, host: &mut impl VerySoftHost, this: u32, frame: u32) -> Result<u32, HostUnavailable> {
    let mut body = Body::load(mem, this);
    if body.parent == 0 {
        host.slot0(mem, frame - 0x10, this)?;
        sleep::become_very_soft(&mut body);
    } else {
        body.sleep_armed = 0;
    }
    body.store(mem, this);
    host.entity_vf158(mem, frame - 0x10, this)?;
    let body = Body::load(mem, this);
    host.emit(mem, frame, &rigid_body::emit::event(&body, SLEEP))
}

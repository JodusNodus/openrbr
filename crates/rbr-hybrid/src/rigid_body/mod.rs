//! Bindings of the rigid-body ports (`rbr_physics::rigid_body`, exe 0x529200-0x52d700). The ABIs are the original's
//! (`src/rbr/RBR_E47_Math.cpp`, `src/rbr/RBR_E47_Def.cpp`, `src/rbr/RBR_E47_Slots.cpp`). The original callees the
//! ports leave to the caller are called through the host.

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::construct::Heap;
use rbr_physics::rigid_body::host::{HostUnavailable, Vf20Host};
use rbr_physics::rigid_body::rotation::MulHost;

use crate::{Binding, Decline, Host};

mod contact;
mod force_pass;
mod pose;
mod quats;
mod slots;
mod window;

use crate::collision::CSTATE_UNKA44;
use crate::engine_driveline::SIGNALS;
use crate::entities::{COPY16_WALK, INTEGRATE, MB560, NAME_WALK, STATE_EMIT, VF20};
use contact::{big_a, big_b, e43vf70, vf81x};
use force_pass::{e43vf64, mounted_vf128, response_run};
use pose::{e43vf6c, integrate_pose};
use quats::{quat_mul, quat_norm, quat_normalize, set_from_quat};
use slots::{
    clamp_velocity_along, damping_emit, e43vf74, e43vfa0, fold_rows, inverse_mass, new_def, quat_diff_rows,
    quat_plus_rows, quat_rows_b8, quat_transform_rows, ring_append, scale_add_rows, state_ctor, step76, transform_rows,
    transform_rows3, vf20_jump, vf24x, vf85,
};
pub(crate) use slots::{e43vf138, impulse_run};
use window::{
    advance_rest, back_step, e43vf158, e43vf90, e43vf94, scratch_reset_after_entity, vf16_helper, vf17x, window_start,
};
pub(crate) use window::{vf16, vf17x_state};

pub const NEW_DEF: u32 = 0x52b8b0;
pub const VF20_JUMP: u32 = 0x529200;
pub const VF16_HELPER: u32 = 0x52b6d0;
pub const STATE_CTOR: u32 = 0x52d280;
pub const VF17X: u32 = 0x529a50;
pub const E43VF74: u32 = 0x529ea0;
pub const VF24X: u32 = 0x52b740;
pub const VF_DX: u32 = 0x52cfd0;
pub const VF_EX: u32 = 0x52d110;
pub const E43VFAC: u32 = 0x52ad40;
pub const E43VFB0: u32 = 0x52add0;
pub const E43VFA4: u32 = 0x52b110;
pub const E43VFA8: u32 = 0x52b1f0;
pub const E43VFB4: u32 = 0x52b2e0;
pub const E43VFB8: u32 = 0x52b380;
pub const E43VFBC: u32 = 0x52aec0;
pub const VF81X: u32 = 0x52c510;
pub const BIG_A: u32 = 0x52b9f0;
pub const VFC8: u32 = 0x52c8f0;
pub const E43VF70: u32 = 0x529340;
pub const BIG_B: u32 = 0x52c220;
pub const E43VF64: u32 = 0x52cb20;
pub const E43VF6C: u32 = 0x52cc30;
pub const E46_IMPULSE640: u32 = 0x529640;
pub const E43VF13C: u32 = 0x5297d0;
pub const E43VFA0: u32 = 0x52ad10;
pub const E43VF140: u32 = 0x52a570;
pub const E43VF138: u32 = 0x52b990;
pub const E43VF128: u32 = 0x52a0e0;
pub const E43VF90: u32 = 0x52bf30;
pub const E43VF94: u32 = 0x52c130;
pub const VF54_HELPER: u32 = 0x52b6b0;

/// The original callees the bindings call.
pub(crate) const OPERATOR_NEW: u32 = 0x70bec2; // operator new (cdecl: size)
const MUL: u32 = 0x528c20; // E46Mat::Mul (thiscall: the matrix; out, b)
const ENTITY_VF48: u32 = 0x51a440; // CEntity::E43Vf48 (thiscall: body; its children's slot 18)
const ENTITY_VF158: u32 = 0x519aa0; // CEntity::E43Vf158 (thiscall: body)
pub(crate) const LOOKUP_TWIN: u32 = 0x4e03b0; // CCellGrid::LookupTwin (thiscall: the grid; x, y, link)
const APPEND_CELL: u32 = 0x4ea980; // CCar::AppendA980 (thiscall: the cell; link)
const SAMPLE_BIG_SITE: u32 = 0x52caed; // VfC8's call of CDriveLine::SampleBig 0x50b300 (thiscall: the drive; at, value; x87 result)
const CELL_GRID: u32 = 0x8eb794; // the grid global `LookupTwin` reads
pub(crate) const DRIVE: u32 = 0x1659184; // the drive global `SampleBig` reads
pub(crate) const WALKER: u32 = 0x7cbb00; // the identity-node pointer global used by E43PairWalk
const E36_COLLECT: u32 = 0x4f3950; // E36Collect (thiscall: the body; the list and count cells)
const COLLECT_OBJECT: u32 = 0x7c7040; // the E36 object `BigB` collects on (its content is ecx)
const RETIRE_A: u32 = 0x4e8fa0; // CBody::E34_RetireA (thiscall: the body)
const DAMPING_EMIT: u32 = 0x52be70; // DampingCollision Emit (thiscall: the signal object; the event)
pub(crate) const BODY_EVENT_EMIT: u32 = 0x51f7c0; // the body event signal's emit (thiscall: the signal; the event)
const BODY_EVENT: u32 = 0xec98; // the body event signal's offset in the `SIGNALS` object
const DETACH_EMIT: u32 = 0x51f580; // the detach signal's emit (thiscall: the signal; the payload), the same template
const SCRATCH_RESET: u32 = 0x529af0; // E47SolidScratch::Vf04 (thiscall: the scratch block), its slot 1
const STEP76: u32 = 0x529c80; // CSolid::Step76 (thiscall: the point, the out vector; ret 8)
const VF85: u32 = 0x52b870; // CSolid::Vf85 (fastcall: the definition's word at +0x1ec is nonzero)
const SAMPLE: u32 = 0x4e6000; // BigA's contact sample (thiscall: the body; the contact's vector, two out words)
const SLOT_SAMPLE_HOOK: u32 = 0x8; // slot 2 of the hook at the body's +0x3cc (BigA's sample sink)
const SLOT_HOOK: u32 = 0x4; // slot 1 of the hook at the body's +0x3cc (E43Vf128's)
pub const RING_APPEND: u32 = 0x529100; // E46_RingAppend (thiscall: the ring tail; src, a, b, c)

/// `E46Mat::Mul`'s operands live this far below the call's return address slot: the original's frame is there, and
/// the nested calls' frames are below (as in `collision`).
const SCRATCH_BELOW_ESP: u32 = 0x800;

pub static BINDINGS: [Binding; 48] = [
    bind(NEW_DEF, "CSolid::NewDef", new_def),
    bind(E46_IMPULSE640, "E46Impulse::Impulse640", clamp_velocity_along),
    bind(E43VF13C, "CSolid::E43Vf13C", inverse_mass),
    bind(E43VF140, "CSolid::E43Vf140", response_run),
    bind(0x51e5c0, "E44_JmpA570", response_run),
    bind(0x521b40, "CMountedRigidBody::M1B40", response_run),
    bind(0x521b20, "CMountedRigidBody::Vf128", mounted_vf128),
    bind(E43VF138, "CSolid::E43Vf138", e43vf138),
    bind(E43VF128, "CSolid::E43Vf128", impulse_run),
    bind(RING_APPEND, "E46_RingAppend", ring_append),
    bind(E43VFA0, "CSolid::E43VfA0", e43vfa0),
    bind(VF20_JUMP, "CSolid::E43Vf20", vf20_jump),
    bind(VF16_HELPER, "E47_Vf16Helper", vf16_helper),
    bind(STATE_CTOR, "E47SolidState::ctor", state_ctor),
    bind(VF17X, "E47SolidState::Vf17x", vf17x),
    bind(E43VF74, "CSolid::E43Vf74", e43vf74),
    bind(VF24X, "CSolid::Vf24x", vf24x),
    bind(E43VFAC, "CSolid::E43VfAC", transform_rows),
    bind(E43VFB0, "CSolid::E43VfB0", transform_rows3),
    bind(E43VFA4, "CSolid::E43VfA4", quat_plus_rows),
    bind(E43VFA8, "CSolid::E43VfA8", quat_diff_rows),
    bind(E43VFB4, "CSolid::E43VfB4", quat_transform_rows),
    bind(E43VFB8, "CSolid::E43VfB8", quat_rows_b8),
    bind(E43VFBC, "CSolid::E43VfBC", fold_rows),
    bind(DAMPING_EMIT, "CSolid::Emit", damping_emit::<DAMPING_EMIT>),
    bind(BODY_EVENT_EMIT, "CMultiSignal1<CSigMrRBodyEvent>::Emit (ours)", damping_emit::<BODY_EVENT_EMIT>),
    bind(DETACH_EMIT, "CMultiSignal1<CSigDetachFromParent>::Emit (ours)", damping_emit::<DETACH_EMIT>),
    bind(E43VF90, "CSolid::E43Vf90", e43vf90),
    bind(E43VF94, "CSolid::E43Vf94", e43vf94),
    bind(0x52c030, "CSolid::E43Vf158", e43vf158),
    bind(VF54_HELPER, "E47_Vf54Helper", scratch_reset_after_entity),
    bind(0x529dc0, "CSolid::E43Vf48", window_start),
    bind(VF_DX, "CSolid::VfDx", back_step),
    bind(VF_EX, "CSolid::VfEx", advance_rest),
    bind(0x52ac80, "CSolid::E43Vf9C", scale_add_rows),
    bind(VF81X, "CSolid::Vf81x", vf81x),
    bind(BIG_A, "CSolid::BigA", big_a),
    bind(VFC8, "CSolid::VfC8", integrate_pose),
    bind(BIG_B, "CSolid::BigB", big_b),
    bind(E43VF6C, "CSolid::E43Vf6C", e43vf6c),
    bind(E43VF70, "CSolid::E43Vf70", e43vf70),
    bind(E43VF64, "CSolid::E43Vf64", e43vf64),
    bind(0x43c6e0, "E01Mat3::SetFromQuat", set_from_quat),
    bind(0x483dd0, "E15Shell::M83DD0", quat_mul),
    bind(0x4921a0, "E18_QuatNormalize", quat_normalize),
    bind(0x4e4c70, "E34Quat::Norm", quat_norm),
    bind(STEP76, "CSolid::Step76", step76),
    bind(VF85, "CSolid::Vf85", vf85),
];

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "rigid-body", run }
}

/// The original callees, called through the host. The host never reports itself unavailable.
struct Original<'a>(&'a dyn Host);

impl Heap for Original<'_> {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.0.call(OPERATOR_NEW, 0, &[size]).eax
    }
}

impl Vf20Host for Original<'_> {
    fn entity_walk_s20(&mut self, body: u32, arg: u32) -> u32 {
        self.0.call(VF20, body, &[arg]).eax
    }
}

/// `E46Mat::Mul` and the `CBody::Vf40` callees, in the emulator. The product's operands are written below the stack
/// (`scratch`), and the original reads and writes them there: the matrix, the diagonal and the output, nine words each.
/// `mem` is a clone of the binding's memory (an emulator-backed clone writes the emulator).
struct Callees<'a> {
    mem: Mem,
    host: &'a dyn Host,
    scratch: u32,
}

impl MulHost for Callees<'_> {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], HostUnavailable> {
        let (matrix, diag, out) = (self.scratch, self.scratch + 0x24, self.scratch + 0x48);
        for (k, v) in m.into_iter().enumerate() {
            self.mem.set_f32(matrix + 4 * k as u32, v);
        }
        for (k, v) in b.into_iter().enumerate() {
            self.mem.set_f32(diag + 4 * k as u32, v);
        }
        self.host.call(MUL, matrix, &[out, diag]);
        Ok(self.mem.f32s(out))
    }
}

impl rbr_physics::collision::body::Vf40Host for Callees<'_> {
    fn state_unka44(&mut self, state: u32) -> u32 {
        self.host.call(CSTATE_UNKA44, state, &[]).eax
    }

    fn entity_vf40(&mut self, body: u32) -> u32 {
        self.host.call(NAME_WALK, body, &[]).eax
    }
}

fn unavailable(_: HostUnavailable) -> Decline {
    Decline::Unsupported("an original callee is unavailable")
}

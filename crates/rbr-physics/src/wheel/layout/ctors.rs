//! The wheel's vtable-slot answers, the scratch and damage clears, `M3B880`, `UnkA860`, `CWheel::Vf28` and the
//! constructors (`CState`, `CDef`, `CSetup`, `CSetupOptions`, `E51Wheel`).

use super::fields::{ON_GROUND, STRUT, TYRE_BLOCK, TYRE_SCRATCH, WHEEL_INDEX};
use super::store::WheelStore;
use crate::car_body::light;
use crate::mem::Mem;
use crate::physics_core::layout::construct::Heap;
use crate::physics_core::layout::material;
use crate::physics_core::layout::SIGNAL_MANAGER;
use crate::rigid_body::emit::EmitHost;
use crate::rigid_body::host::{HostUnavailable, SolidCtorHost};
use crate::rigid_body::layout::Body;
use crate::tyre::store::TyreStore;
use crate::{fpu, ntweak, rigid_body, spring_damper, tyre};

/// `CDef::Unk4C` (0x538b40, a vtable slot, no arguments): the address of a static string.
pub fn cdef_unk4c() -> u32 {
    0x0074_287C
}

/// `CDef::Unk20` (0x538d70, a vtable slot, no arguments): the address of a static block.
pub fn cdef_unk20() -> u32 {
    0x0165_F2CC
}

/// `E51Setup::Vf20` (0x53b550, a vtable slot, no arguments): the address of a static block.
pub fn setup_vf20() -> u32 {
    0x0165_F2A0
}

/// `E51Setup::Vf28` (0x53b570, a vtable slot, no arguments): the address of a static block.
pub fn setup_vf28() -> u32 {
    0x0165_F2F4
}

/// `E51WheelDamage::Zero` (0x53b050, thiscall, no arguments): zeroes the damage record's thirteen words.
pub fn damage_zero(mem: &mut Mem, this: u32) {
    for k in 0..13 {
        mem.set_u32(this + 4 * k, 0);
    }
}

/// `CScratch::Vf04` (0x538f50, thiscall, no arguments): clears the vectors at +0x20, +0x60, +0x70, +0x80, +0x1f0 and
/// +0x200 (sixteen bytes each), zeroes the word at +0x57c and the float at +0x210.
pub fn scratch_vf04(mem: &mut Mem, this: u32) {
    for at in [0x20, 0x60, 0x70, 0x80, 0x1F0, 0x200] {
        for k in 0..4 {
            mem.set_u32(this + at + 4 * k, 0);
        }
    }
    mem.set_u32(this + 0x57C, 0);
    mem.set_f32(this + 0x210, 0.0);
}

/// `CWheel::CScratch::CScratch` (0x53b670, thiscall, returns `this`): the scratch's base init, its vtable word, the
/// material at +0x2a4, the sub-block at +0x404, and `Vf04`. The original's SEH frame leaves nothing else.
pub fn scratch_ctor(mem: &mut Mem, this: u32) -> u32 {
    rigid_body::layout::store_scratch_base_init(mem, this);
    mem.set_u32(this, 0x0074_2804);
    material::construct(mem, this + 0x2A4);
    light::sub_block_init(mem, this + 0x404);
    scratch_vf04(mem, this);
    this
}

/// `E51Wheel::M3B880` (0x53b880, thiscall, no arguments): the overlay `CWheel::Unk9bb0` (0x539bb0), `ClearBias`
/// (0x534000), and, when the flag word's bit 0x400 is set, the strut's travel clamp: the strut's +0x18 (`this+0xedc`) is
/// set to zero when it is below one. The overlay (with bit 2: transforms of the wheel's and the definition's points)
/// and `ClearBias` (with bit 0x800000: its x87 terms, each popped) write only their own frames, `ComputeTravel`'s
/// (0x52dd50) result is popped, and `TravelRange` (0x52dd20) writes its two ends to two of the caller's locals, which
/// nothing reads: none of them is run. Returns eax: the flag word, or with bit 0x400 the strut's +0x18 as it was read.
pub fn refresh_travel<S: WheelStore>(mem: &mut S, this: u32) -> u32 {
    let flags = mem.flag_word();
    if flags & 0x400 == 0 {
        return flags;
    }
    let travel = mem.u32(this + 0xEDC);
    if (travel as i32) < 1 {
        mem.set_u32(this + 0xEDC, 0);
    }
    travel
}

/// `E51Wheel::UnkA860` (0x53a860, thiscall, stack words a, b, c; ret 0xc): `a` is the tyre block (`this+0x7f0`), `b` is
/// `SetUnk10` on the strut (0x52da40, `set_rebound_damping`), and `c` is `Set414` on the tyre scratch (0x532490,
/// `store_414`). The asm's order is the port's.
pub fn tyre_tail_setup(mem: &mut Mem, this: u32, a: u32, b: u32, c: u32) {
    mem.set_u32(this + TYRE_BLOCK, a);
    spring_damper::layout::set_rebound_damping(mem, this + STRUT, b);
    mem.store_414(this + TYRE_SCRATCH, c);
}

/// The calls `CWheel::Vf28` (0x53f230) makes that have an effect. Each is the original callee at its address, run at its
/// entry stack pointer `sp` (the emulator runs an unbound callee, and a bound one runs in Rust).
pub trait FinishCalls {
    /// `CSpringDamper::ResetOutputs` (0x52e010) on the strut.
    fn reset_outputs(&mut self, strut: u32, sp: u32);
    /// `TyreTail_Get10` (0x5326b0) on the tyre scratch. Its result feeds only the car transform's input, which the port does not run.
    fn tyre_tail(&mut self, scratch: u32, sp: u32);
    /// `CSolid::Step77` (0x529d30) on the wheel, with the two stack pointers `a` and `b`.
    fn step77(&mut self, this: u32, a: u32, b: u32, sp: u32);
    /// `CSigSolidGeneralState`'s emit (`E51SigGeneral::Emit`, 0x53d0d0, the same code as the tire-particle emit) on `signal`,
    /// with the payload at `payload`.
    fn emit(&mut self, signal: u32, payload: u32, sp: u32);
    /// `CTyre::CScratch::EmitNoise` (0x535890) on the tyre scratch.
    fn emit_noise(&mut self, scratch: u32, sp: u32);
}

/// The wheel's key (+0x30): the payload's word 0.
const KEY: u32 = 0x30;
/// The payload (`CSigSolidGeneralState`, four words), and `Step77`'s two stack arguments `a` (+0x90) and `b` (+0x94).
const FINISH_PAYLOAD: u32 = 0x40;
const FINISH_A: u32 = 0x90;
const FINISH_B: u32 = 0x94;
/// The constant `Step77` takes as `b` (5.0, read from 0x72ae44).
const STEP77_B: u32 = 0x0072_AE44;
/// The signal manager (`g_signalManager`, a pointer) and the `E51SigGeneral` signal in it (+0x29e0).
const SIG_GENERAL: u32 = 0x29E0;

/// `CWheel::Vf28` (0x53f230, thiscall, no arguments, plain `ret`): the finish of the wheel's step. `frame` is the body's
/// `esp` (the asm's `[esp+k]` is `frame+k`). The original's car transform, sine, cosine, quaternion product and two
/// `Vec4::Clear`s write only frame locals that nothing reads, so the port does not run them. The payload's words 1..=3
/// are not written: they are the frame's residue, as the original leaves them. Ends with [`refresh_travel`] (`M3B880`),
/// whose eax it returns.
pub fn finish<S: WheelStore>(mem: &mut S, calls: &mut impl FinishCalls, this: u32, frame: u32) -> u32 {
    calls.reset_outputs(this + STRUT, frame - 4);
    calls.tyre_tail(this + TYRE_SCRATCH, frame - 4);
    mem.set_u32(frame + FINISH_PAYLOAD, mem.u32(this + KEY));
    mem.set_u32(frame + FINISH_B, mem.u32(STEP77_B));
    calls.step77(this, frame + FINISH_A, frame + FINISH_B, frame - 12);
    calls.emit(mem.u32(SIGNAL_MANAGER) + SIG_GENERAL, frame + FINISH_PAYLOAD, frame - 8);
    calls.emit_noise(this + TYRE_SCRATCH, frame - 4);
    refresh_travel(mem, this)
}

/// `CWheel::CState`'s vtable (0x7427b4), stored after the E47 state's constructor.
const CSTATE_VTABLE: u32 = 0x0074_27B4;

/// `CWheel::CState::CState(owner, name, unused)` (0x538dd0, thiscall, ret 0xc) on the state at `this`: the E47 solid state,
/// its vtable, the vectors and reals, the spring damper at +0x2a0 and the tyre state at +0x2e0, then the scalars, the dirty
/// flag and the `0.5` of the `+0x1e0` real. The third argument is not used. Returns `this`.
pub fn state_ctor(mem: &mut Mem, this: u32, owner: u32, name: u32) -> u32 {
    rigid_body::layout::construct_state(mem, this, owner, name);
    mem.set_u32(this, CSTATE_VTABLE);
    ntweak::construct_vector(mem, this + 0x1B0);
    ntweak::construct_real(mem, this + 0x1D0);
    ntweak::construct_real(mem, this + 0x1E0);
    ntweak::construct_real(mem, this + 0x1F0);
    ntweak::construct_real(mem, this + 0x200);
    ntweak::construct_real(mem, this + 0x210);
    ntweak::construct_real(mem, this + 0x250);
    ntweak::construct_real(mem, this + 0x260);
    ntweak::construct_vector(mem, this + 0x270);
    spring_damper::layout::construct_state(mem, this + 0x2A0, owner);
    tyre::layout::construct_state(mem, this + 0x2E0);
    for at in [0x20, 0x24, 0x28] {
        mem.set_f32(this + at, 0.0);
    }
    mem.set_f32(this + 0x2C, 1.0);
    rigid_body::layout::clear_vec(mem, this + 0xB0);
    ntweak::set_dirty(mem, this + 0x1E0);
    mem.set_f32(this + 0x1E0 + 0xC, 0.5);
    for at in [0x220, 0x230, 0x1A0, 0x290] {
        rigid_body::layout::clear_vec(mem, this + at);
    }
    this
}

/// `CWheel::CDef`'s vtable (0x74281c), stored after the solid definition's constructor.
const CDEF_VTABLE: u32 = 0x0074_281C;

/// `CWheel::CDef::CDef(outer)` (0x5387e0, thiscall, ret 4) on the definition at `this`: the solid definition, the vtable,
/// the tweaks in the asm's order (reals, unsigned, vectors), the spring damper and tyre definitions (+0x450, +0x56c), the
/// reals at +0x904 to +0x964 and the vectors at +0x880 to +0x8f0. The five reals from +0x924 are marked dirty and take a
/// value at +0xc; the float at +0x900 is zero. Returns `this`.
pub fn cdef_ctor(mem: &mut Mem, this: u32, outer: u32) -> u32 {
    rigid_body::layout::solid_def_ctor(mem, this, outer);
    mem.set_u32(this, CDEF_VTABLE);
    for at in [0x1F0, 0x230, 0x240, 0x350, 0x360, 0x3E0, 0x3F0, 0x400, 0x410, 0x420] {
        ntweak::construct_real(mem, this + at);
    }
    for at in [0x200, 0x210, 0x220, 0x430, 0x440] {
        ntweak::construct_unsigned(mem, this + at);
    }
    for at in [0x250, 0x270, 0x290, 0x2B0, 0x2D0, 0x2F0, 0x310, 0x330, 0x370, 0x390, 0x3B0] {
        ntweak::construct_vector(mem, this + at);
    }
    spring_damper::layout::construct_definition(mem, this + 0x450);
    tyre::layout::construct_cdef(mem, this + 0x56C);
    for at in [0x904, 0x914, 0x924, 0x934, 0x944, 0x954, 0x964] {
        ntweak::construct_real(mem, this + at);
    }
    for at in (0x880..0x900).step_by(0x10) {
        rigid_body::layout::clear_vec(mem, this + at);
    }
    mem.set_f32(this + 0x900, 0.0);
    // The values are the float words at 0x742814, 0x742810, 0x74280c, 0x731e08 and 0x73e8a8.
    for (at, value) in [
        (0x924, f32::from_bits(0x46FA_0000)),
        (0x934, f32::from_bits(0x4788_B800)),
        (0x944, f32::from_bits(0x3EB2_B021)),
        (0x954, f32::from_bits(0x45FA_0000)),
        (0x964, f32::from_bits(0x476A_6000)),
    ] {
        ntweak::set_dirty(mem, this + at);
        mem.set_f32(this + at + 0xC, value);
    }
    this
}

/// The size of a wheel definition block (`operator new(0x980)` in `CWheel::M3B610`).
const CDEF_SIZE: u32 = 0x980;

/// `CWheel::M3B610(outer)` (0x53b610, thiscall, ret 4): a definition block from `host`, built by `cdef_ctor` on `outer`.
/// Returns the block, or 0 when the allocation fails.
pub fn new_cdef(mem: &mut Mem, outer: u32, host: &mut impl Heap) -> u32 {
    let block = host.alloc(mem, CDEF_SIZE);
    if block != 0 {
        cdef_ctor(mem, block, outer);
    }
    block
}

/// `CWheel::CSetup`'s vtable (0x72e584).
const SETUP_VTABLE: u32 = 0x0072_E584;

/// `CWheel::CSetup::CSetup` (0x47bba0) on the setup at `this`: an unnamed tweak, its vtable, the real at +0xc, the vector
/// at +0x20 and the reals at +0x40, +0x50 and +0x60 (the words `CopyCF30` reads). Returns `this`.
pub fn setup_ctor(mem: &mut Mem, this: u32) -> u32 {
    ntweak::construct_base(mem, this, 0);
    mem.set_u32(this, SETUP_VTABLE);
    ntweak::construct_real(mem, this + 0xC);
    ntweak::construct_vector(mem, this + 0x20);
    for at in [0x40, 0x50, 0x60] {
        ntweak::construct_real(mem, this + at);
    }
    this
}

/// `CWheel::CSetupOptions`'s vtable (0x72e904).
const OPTIONS_VTABLE: u32 = 0x0072_E904;

/// `CWheel::CSetupOptions::CSetupOptions` (0x47daa0) on the options at `this`: an unnamed tweak, its vtable, the real at
/// +0xc (`nTopMountPositions`), eight vectors from +0x20 (`vecTopMount00`..), three ranges (`E13RealTriple`s)
/// from +0x120, seven vectors from +0x1b0, five reals from +0x290, unsigneds at +0x2e0 and +0x2f0 and reals at +0x300 and
/// +0x310.
pub fn construct_options(mem: &mut Mem, this: u32) {
    ntweak::construct_base(mem, this, 0);
    mem.set_u32(this, OPTIONS_VTABLE);
    ntweak::construct_real(mem, this + 0xC);
    for at in (0x20..0x120).step_by(0x20) {
        ntweak::construct_vector(mem, this + at);
    }
    for at in (0x120..0x1B0).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
    for at in (0x1B0..0x290).step_by(0x20) {
        ntweak::construct_vector(mem, this + at);
    }
    for at in (0x290..0x2E0).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
    for at in [0x2E0, 0x2F0] {
        ntweak::construct_unsigned(mem, this + at);
    }
    for at in [0x300, 0x310] {
        ntweak::construct_real(mem, this + at);
    }
}

/// `CWheel::CSetup`'s `TopMountSlot` (a real, its value at +0xc) and `vecTopMountPosition`, and `CWheel::CSetupOptions`'
/// `vecTopMount00`: vector tweaks of 0x20 bytes, one after another.
const SETUP_TOP_MOUNT_SLOT: u32 = 0xC + 0xC;
const SETUP_TOP_MOUNT: u32 = 0x20;
const OPTIONS_TOP_MOUNTS: u32 = 0x20;
const VECTOR_TWEAK: u32 = 0x20;

/// The geometry pages' `vf48` (`CGeometryPageFront` 0x548a50 and `CGeometryPageRear` 0x548c60, the same loop over the
/// four wheels) for the wheel setup at `setup` and its options at `options`: the setup's `vecTopMountPosition` becomes a
/// copy of the options' top mount its `TopMountSlot` names (truncated).
pub fn select_top_mount(mem: &mut Mem, setup: u32, options: u32) {
    let slot = fpu::trunc_i32(f64::from(mem.f32(setup + SETUP_TOP_MOUNT_SLOT))) as u32;
    mem.copy(
        setup + SETUP_TOP_MOUNT,
        options + OPTIONS_TOP_MOUNTS + slot.wrapping_mul(VECTOR_TWEAK),
        VECTOR_TWEAK as usize,
    );
}

/// `CWheel`'s vtable (0x74288c), stored after the solid base's constructor.
const WHEEL_VTABLE: u32 = 0x0074_288C;

/// The float at 0x72f5e0 (300.0), stored at +0xeb0.
const WHEEL_TAIL_FLOAT: f32 = f32::from_bits(0x4396_0000);

/// `E51Wheel::Ctor(def, name, owner, flags)` (0x53b6e0, thiscall, ret 0x10) on the wheel at `this`. `def` is the car's
/// definition (0 for none), `owner` an optional body (0 for none). Runs the solid base (`host`, with the flags' 0x20), the
/// vtable, the state (the name is the definition's, or 0), the scratch, the spring damper (+0xec4), the tyre scratch
/// (+0xf00), the words between, and the owner's event (`E43Vf90`, 0x52bf30) when there is an owner. The owner is kept
/// only with bit 0x8 of its flags. The state pointer is never null, so the asm's checks on it are not kept. Returns `this`.
pub fn wheel_ctor(
    mem: &mut Mem,
    host: &mut (impl SolidCtorHost + EmitHost),
    this: u32,
    def: u32,
    name: u32,
    owner: u32,
    flags: u32,
) -> Result<u32, HostUnavailable> {
    host.solid_ctor(mem, this, def, name, owner, flags | 0x20)?;
    mem.set_u32(this, WHEEL_VTABLE);
    let base_name = if def != 0 { ntweak::name_or_null(mem, def) } else { 0 };
    state_ctor(mem, this + 0x3D0, this, base_name);
    scratch_ctor(mem, this + 0x800);
    let (controller_def, tyre_def, record) = if def != 0 { (def + 0x450, def + 0x56C, this) } else { (0, 0, 0) };
    spring_damper::layout::construct_controller(mem, this + 0xEC4, controller_def, this + 0x670);
    tyre::layout::construct_scratch(mem, this + 0xF00, tyre_def, this + 0x6B0, record);
    for at in [0xEA0, 0xEA4, 0xEA8, 0xEAC] {
        mem.set_f32(this + at, 0.0);
    }
    mem.set_f32(this + 0xEB0, WHEEL_TAIL_FLOAT);
    mem.set_u32(this + WHEEL_INDEX, 0xFFFF_FFFF);
    mem.set_u32(this + 0xEB8, 0);
    mem.set_u32(this + ON_GROUND, 0);
    let kept = if owner != 0 {
        let mut body = Body::load(mem, this);
        host.wake(&mut body)?;
        body.store(mem, this);
        if mem.u8(owner + 0x60) & 8 != 0 {
            owner
        } else {
            0
        }
    } else {
        0
    };
    mem.set_u32(this + 0x7F4, kept);
    mem.set_u32(this + 0x7F0, 0);
    mem.set_u32(this + 0xEC0, 0);
    Ok(this)
}

/// The size `E51Wheel::CDef::Vf50` asks for.
pub const WHEEL_SIZE: u32 = 0x1420;

/// `E51Wheel::CDef::Vf50(a, b)` (0x53d1c0, thiscall, ret 8) on the definition `def`: a wheel block from `host`, built by
/// [`wheel_ctor`] with no flags. Returns the wheel, or 0 when the allocation fails.
pub fn new_wheel(
    mem: &mut Mem,
    host: &mut (impl Heap + SolidCtorHost + EmitHost),
    def: u32,
    a: u32,
    b: u32,
) -> Result<u32, HostUnavailable> {
    let block = host.alloc(mem, WHEEL_SIZE);
    if block == 0 {
        return Ok(0);
    }
    wheel_ctor(mem, host, block, def, a, b, 0)
}

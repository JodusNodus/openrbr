//! The calls the wheel makes outside itself, one method each (addresses in the docs), and the forwards that only make
//! them. The host answers with its own memory: the ports, the original's code in the emulator, or a record in the tests.

use crate::math::vec::Vec4;
use crate::ntweak;
use crate::rigid_body::host::HostUnavailable;
use crate::wheel::layout::store::WheelStore;

/// `operator new(0x1420)`'s size, the wheel's sub-object.
const SUB_OBJECT_SIZE: u32 = 0x1420;

/// The calls `E51Wheel::M3D1C0` (0x53d1c0) makes: `operator new` (cdecl, 0x70bec2) and the sub-object's constructor
/// (thiscall, 0x53b6e0, with four stack words: the outer object, the two arguments, and 0).
pub trait ConstructCalls {
    /// `operator new(size)`: the block's address, or 0 when the allocation fails.
    fn allocate(&mut self, size: u32) -> u32;
    /// The constructor (0x53b6e0) on the new object `obj`; the result is the eax it returns.
    fn construct(&mut self, obj: u32, outer: u32, a: u32, b: u32) -> u32;
}

/// `E51Wheel::M3D1C0` (0x53d1c0, thiscall, `ret 8`): allocates the wheel's 0x1420-byte sub-object and constructs it from
/// the two arguments. A failed allocation returns 0 and constructs nothing; otherwise the constructor's result. The
/// original's SEH frame restores FS:[0] on both paths and leaves nothing else, so the port has no part for it.
pub fn construct(host: &mut impl ConstructCalls, outer: u32, a: u32, b: u32) -> u32 {
    let obj = host.allocate(SUB_OBJECT_SIZE);
    if obj == 0 {
        return 0;
    }
    host.construct(obj, outer, a, b)
}

/// The calls `CWheel::E43Vf20` (0x539090) makes outside the wheel. The wheel is `wheel`.
pub trait SlotCalls {
    /// `E51Wheel::M3B5B0` (0x53b5b0), the wheel's `Vf40` (vtable slot 0x40): an `E47` call, then an indirect tail jump.
    /// Working name `wheel_vf40`; its meaning is not known.
    fn wheel_vf40(&mut self, wheel: u32);
    /// `CSolid::E43Vf20` (0x529200, the jump thunk to `CEntity::E43Vf20` at 0x51a3a0), forwarding `p`.
    fn solid_vf20(&mut self, wheel: u32, p: u32);
}

/// The calls `CWheel::E43Vf5C` makes on the car (`this+0x44`): its vtable slots 28 and 29, each with a vector argument.
pub trait CarSlotCalls {
    /// The car's slot 28 (`CSolid::E43Vf70`, 0x529340, adds the vector to the force). Working name `car_vf70`.
    fn car_vf70(&mut self, car: u32, p: u32);
    /// The car's slot 29 (`CSolid::E43Vf74`, 0x529ea0, accumulates the vector into the scratch). Working name `car_vf74`.
    fn car_vf74(&mut self, car: u32, p: u32);
}

/// The call `CWheel::Hook` makes on the wheel itself: its vtable slot 96 (0x180), with the argument `p`.
pub trait HookCalls {
    /// The wheel's slot 96 (vtable 0x180; the step's record is 0x53dd30 in the original). Working name `wheel_vf180`.
    fn wheel_vf180(&mut self, wheel: u32, p: u32);
}

/// `CWheel::Hook` (E43Vf58, 0x539a20, vtable slot 22): calls the wheel's slot 96 with `p`. The profiler scope and the try
/// block around that call are the binding's (`physics_core::scope`), not this port's.
pub fn hook(host: &mut impl HookCalls, wheel: u32, p: u32) {
    host.wheel_vf180(wheel, p);
}

/// The call `CWheel::E43Vf6C` makes outside the wheel: a tail call, so the callee's record is its own.
pub trait SolidCalls {
    /// `CSolid::E43Vf6C` (0x52cc30, thiscall, one float) on the wheel `wheel`.
    fn solid_vf6c(&mut self, wheel: u32, f: f32);
}

/// `CWheel::E43Vf6C` (0x539eb0, vtable slot 27): forwards its float `f` to `CSolid::E43Vf6C` as a tail call.
pub fn forward_vf6c(host: &mut impl SolidCalls, wheel: u32, f: f32) {
    host.solid_vf6c(wheel, f);
}

/// The calls `E51Wheel::M3B5B0` (0x53b5b0, the slot that `SlotCalls::wheel_vf40` names) makes: `CSolid::Vf40` on the
/// wheel, then slot 17 of the embedded object as a tail jump.
pub trait EmbeddedCalls {
    /// `CSolid::Vf40` (0x52b6d0, `E47_Vf16Helper`) on the wheel.
    fn solid_vf40(&mut self, wheel: u32) -> Result<(), HostUnavailable>;
    /// Slot 17 (+0x44) of the vtable of the object at `obj`, read after the base call. The tail jump's results are the
    /// wheel's, so the eax comes back here.
    fn embedded_slot17(&mut self, obj: u32) -> Result<u32, HostUnavailable>;
}

/// The calls `E51MultiTire::Emit` (0x53d0d0) makes: `Invoke` on each connection of its signal, and the two `E01Log`
/// messages the emit logs before it stops.
pub trait EmitCalls {
    /// `Invoke(sig)` (thiscall, `ret 4`) on the connection `conn`, through the vtable slot at `target`.
    fn invoke(&mut self, target: u32, conn: u32, sig: u32);
    /// `E01Log(6, 0, ...)`: an emission while the signal is already emitting.
    fn log_recursive(&mut self);
    /// `E01Log(7, 0, ...)`: the count is out of range.
    fn log_range(&mut self, count: u32);
}

/// The calls `CWheel::Unk9900` (0x539900) makes outside the wheel. The tyre scratch and the blend ops answer them with
/// their own memory.
pub trait BlendCalls {
    /// `CTyre::CScratch::BlendTable` (0x534e70) on the tyre scratch: the two coefficients `(blendA, blendB)`.
    fn blend_coefficients(&mut self, scratch: u32, i: u32, j: u32, a: f32, b: f32) -> (f32, f32);
    /// `E50BlendOps::Blend` (0x5381f0) on the blend ops `ops`: blends the material entries `s1` and `s2` with `a`, `b`, `z`.
    fn blend_materials(&mut self, ops: u32, s1: u32, s2: u32, a: f32, b: f32, z: f32);
}

/// `E50BlendOps::Blend(a, b, c, d, e)` (0x5381f0, thiscall, `ret 0x14`) on the ops at `this`, from the entries `a` and `b`.
/// The words at +0x48, +0x58, +0x68 and +0x78 take `b * (d * e) + a * ((1 - e) * c)`; the others take `a * (1 - e) + b * e`.
/// Each word's `CBase` (at its -0xc) is marked dirty before the word is written, as the original does.
pub fn blend_ops<S: WheelStore>(mem: &mut S, this: u32, a: u32, b: u32, c: f32, d: f32, e: f32) {
    let rest = 1.0 - e;
    let de = d * e;
    let rest_c = rest * c;
    for at in [0x48, 0x58, 0x68, 0x78] {
        blend_word(mem, this, a, b, at, rest_c, de);
    }
    for at in [0x18, 0x28, 0x38, 0x88, 0x98, 0xA8, 0xB8, 0xE8, 0xF8] {
        blend_word(mem, this, a, b, at, rest, e);
    }
}

/// One word of `blend_ops`: `a`'s word times `wa`, plus `b`'s times `wb`, each product rounded (the x87 at 24 bits).
fn blend_word<S: WheelStore>(mem: &mut S, this: u32, a: u32, b: u32, at: u32, wa: f32, wb: f32) {
    let (va, vb) = (mem.f32(a + at), mem.f32(b + at));
    ntweak::set_dirty(mem.mem_mut(), this + at - 0xC);
    mem.set_f32(this + at, va * wa + vb * wb);
}

/// The call `CWheel::Unk96e0` (0x5396e0) makes on its local `CE36SubBlock`.
pub trait SubBlockInit {
    /// `CE36SubBlock::Init` (0x4f3e80, thiscall): zeroes the block's three words, which the sector pick then writes in
    /// full.
    fn init_sub_block(&mut self, block: u32);
}

/// The call `CWheel::Unk97a0` makes on a gated-on sub-object.
pub trait SubCalls {
    /// `CWheel::Unk94d0` (0x5394d0) as the asm calls it: `wheel` is the `this` register, the first stack word is the
    /// sub-object `sub`, then `v` (a stack vector), `a`, `b` and `c`. Working name `sub_unk94d0`.
    fn sub_unk94d0(&mut self, wheel: u32, sub: u32, v: Vec4, a: u32, b: u32, c: u32);
}

/// The calls `E51Wheel::M3D030` (0x53d030) makes outside the wheel, one method each. The arguments and results are typed
/// values; the host does its own memory.
pub trait StrutCalls {
    /// `NTweak::CBase::GetName` (0x52db10) on the strut `strut`: the name object (`m_name`, +4).
    fn strut_name(&mut self, strut: u32) -> u32;
    /// `NTweak::CBase::SetDirty` (0x52f450) on the object `object` (the tweak at `this+0x5b0`).
    fn mark_dirty(&mut self, object: u32);
    /// `E51Wheel::M3B900` (0x53b900, thiscall, one float) on the wheel `wheel`: `sum` is the value stored at its
    /// `+0x5bc`. Working name `apply_dirty_sum`; its meaning is not known.
    fn apply_dirty_sum(&mut self, wheel: u32, sum: f32);
    /// `CSpringDamper::UpdateAndReset` (0x52e060) on the strut, the strut's last call.
    fn update_and_reset(&mut self, strut: u32);
}

/// The calls `E43PairWalk::MB560` (0x51b560, thiscall, `ret 0xc`) makes for `E51Wheel::M3AEF0` (0x53aef0): walks `a`
/// against `b` on the walker and returns the result (the `c` the original writes). The walk is the entities port's; the
/// caller answers it (the record, in the tests).
pub trait WalkCalls {
    /// `MB560(walker, a, b, c)`: `a` is the walked vector (`None` when its guard is null, the asm's 0), `car` is `b`, and the
    /// result is what the original writes to `c`.
    fn pair_walk(&mut self, walker: u32, a: Option<Vec4>, car: u32) -> Vec4;
}

/// The two car transforms `E51Wheel::M3A8A0` (0x53a8a0) makes (thiscall, `ret 8`): each takes a source vector and returns
/// the vector the car writes to its output. A source is `None` when the CDef's guard is zero (the original passes null).
/// The car answers them (the car_body port, or the original's record in the tests).
pub trait CarXform {
    /// `CCar::BodyToWorldPair(a, b)` (0x4e69c0) on `car`: the result is the vector written to `b`.
    fn body_to_world_pair(&mut self, car: u32, a: Option<Vec4>) -> Vec4;
    /// `CCar::BodyCompose(a, b)` (0x4e6a30) on `car`: the result is the vector written to `b`.
    fn body_compose(&mut self, car: u32, a: Option<Vec4>) -> Vec4;
}

/// Which car transform a pair goes through.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Transform {
    BodyToWorldPair,
    BodyCompose,
}

/// The transform of each of the eight pairs (`XFORM_PAIRS`), in the order the original makes the calls
/// (`RBR_E51_WheelA.cpp:50-67`).
const TRANSFORMS: [Transform; 8] = [
    Transform::BodyToWorldPair,
    Transform::BodyToWorldPair,
    Transform::BodyCompose,
    Transform::BodyToWorldPair,
    Transform::BodyCompose,
    Transform::BodyCompose,
    Transform::BodyToWorldPair,
    Transform::BodyCompose,
];

/// The car transform of pair `pair` (an index into `TRANSFORMS`) on `car`, with source `a`.
pub fn transform_call(host: &mut impl CarXform, pair: usize, car: u32, a: Option<Vec4>) -> Vec4 {
    match TRANSFORMS[pair] {
        Transform::BodyToWorldPair => host.body_to_world_pair(car, a),
        Transform::BodyCompose => host.body_compose(car, a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the calls, and answers the allocation with `block`.
    struct Log {
        block: u32,
        calls: Vec<String>,
    }

    impl ConstructCalls for Log {
        fn allocate(&mut self, size: u32) -> u32 {
            self.calls.push(format!("new {size:#x}"));
            self.block
        }

        fn construct(&mut self, obj: u32, outer: u32, a: u32, b: u32) -> u32 {
            self.calls.push(format!("ctor {obj:#x} {outer:#x} {a:#x} {b:#x}"));
            0x77
        }
    }

    #[test]
    fn constructs_the_new_block() {
        let mut log = Log { block: 0x9000, calls: Vec::new() };
        assert_eq!(construct(&mut log, 0x1000, 2, 3), 0x77);
        assert_eq!(log.calls, ["new 0x1420", "ctor 0x9000 0x1000 0x2 0x3"]);
    }

    #[test]
    fn failed_allocation_returns_zero() {
        let mut log = Log { block: 0, calls: Vec::new() };
        assert_eq!(construct(&mut log, 0x1000, 2, 3), 0);
        assert_eq!(log.calls, ["new 0x1420"]);
    }
}

//! The CWheel vtable functions the wheel subsystem reaches, as the wheel's own calls see them. `E43Vf20` (0x539090,
//! vtable slot 8) is the sequence `layout::clear_and_forward`; its two calls out of the wheel are the trait below.

/// The calls `CWheel::E43Vf20` makes outside the wheel, one method each (addresses in the docs). The wheel is `wheel`.
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

use crate::rigid_body::host::HostUnavailable;

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

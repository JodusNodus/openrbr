# Car-body damage reset (rust/rbr-physics/src/car_body/damage.rs, CCarDamageHandler::ResetDamage). Run with
# `--set car_body_reset`. Its callees are the damage handler's members (other subsystems, externals here); E36LightBlock::Reset
# is the car-body port (light.rs), so the port calls it itself and it has no spec here. Direct calls only: the drive does not
# reach ResetDamage in the recorded steps. The popped bytes are the callee's `ret` immediates (all 0).

SET_NAME = "car_body_reset"

RESET_WINDOW = 0x3E60  # the handler's words the reset writes (up to the last word at +0x3e58)


def reset_direct(mu, heap, r):
    """ResetDamage() on `this` (a handler in the stub heap): every byte of the window random."""
    handler = heap + 0x20000
    return [], [(handler, bytes(r.getrandbits(8) for _ in range(RESET_WINDOW)))], handler


SPECS = {
    # CCarDamageHandler::ResetDamage() (ret 0): zones, floats, the members' resets, the flags and the table words.
    0x4EC1E0: dict(name="CCarDamageHandler::ResetDamage (4ec1e0)", popped=0, x87=False, thiscall=True,
                   windows=[(0, lambda a: RESET_WINDOW)], direct_fuzz=reset_direct),
    # E39 DriveD StateInitC70 on the member at +0x3a18 (the next member is at +0x3aac).
    0x502C70: dict(name="E39 StateInitC70 (502c70)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x94)]),
    # CEngine::CState::ResetSub78 on the member at +0x3aac (0x78 floats).
    0x50C370: dict(name="CEngine::CState::ResetSub78 (50c370)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x78)]),
    # E51WheelDamage::Zero on the four wheel members (stride 0x38).
    0x53B050: dict(name="E51WheelDamage::Zero (53b050)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x38)]),
    # CU32Value8::Clear on the four members at +0x3d04 (stride 0x20).
    0x5317F0: dict(name="CU32Value8::Clear (5317f0)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0x20)]),
    # CU32Value::Clear on the four members at +0x3ce4 (stride 8).
    0x52D960: dict(name="CU32Value::Clear (52d960)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 8)]),
    # E44 FillOnes on the four vectors at +0x3d84 (stride 0xc).
    0x51CD60: dict(name="E44 FillOnes (51cd60)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 0xC)]),
    # E45 M1640 on the four words at +0x3db4 (stride 4).
    0x521640: dict(name="E45 M1640 (521640)", popped=0, x87=False, thiscall=True, external=True,
                   windows=[(0, lambda a: 4)]),
}

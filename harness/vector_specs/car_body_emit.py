# Car-body signal emits (rust/rbr-physics/src/car_body/emit.rs, tests/car_body_vectors.rs): the nine CMultiSignal1<T>::Emit
# instances as top-level records. The nine share one template (identical code in the asm). Each receiver's Receive (its
# vtable slot 1) is an external record; the receivers are the list nodes the emit walks. Run with `--set car_body_emit`.
# The popped bytes are the callee's `ret` immediates (thiscall: this is ecx, the signal is the stack argument).

SET_NAME = "car_body_emit"


def emit_nodes(mu, a):
    """The list nodes the emit walks (the list at +0x208 and the list at +8 + 16 * index, each node's vtable and next word),
    and the index's word at the source (+4 of the emitter)."""
    emitter = a[0]
    source = word(mu, emitter + 4)
    index = word(mu, source)
    return [(node, 8) for node in emit_lists(mu, emitter, index)]


def emit_lists(mu, emitter, index):
    """Every node the emit walks, in order: the +0x208 list, then the list at +8 + 16 * index."""
    out = []
    for head in (word(mu, emitter + 0x208), word(mu, emitter + 8 + 16 * index)):
        node = head
        while node:
            out.append(node)
            node = word(mu, node + 4)
    return out


def emit_targets(mu, a):
    """The Receive targets (vtable slot 1 of each node), popped 4 (one stack argument: the signal)."""
    emitter = a[0]
    index = word(mu, word(mu, emitter + 4))
    return [(vtarget(mu, node, 1), 4) for node in emit_lists(mu, emitter, index)]


def emit_fuzz(mu, args, rng, put):
    """Direct calls: the +0x208 list may be empty, and the source pointer (+4) may be null (the emit returns at once)."""
    emitter = args[0]
    if rng.random() < 0.3:
        put(emitter + 0x208, 0, "<I")
    if rng.random() < 0.15:
        put(emitter + 4, 0, "<I")


def emit_direct_args(rng, this):
    """Direct calls: the signal is a scratch block in the car (outside the windows the step uses)."""
    return [0x0BF1D7B8 + 0x300]


def emit_spec(name, size=None):
    """An emit: the emitter's flag and lists (0x20c words from `this`), the index word (the source at +4), and the
    signal argument (`size` bytes, when known)."""
    windows = [(0, lambda a: 0x20C), (0, lambda a: 4, 4)]
    if size:
        windows.append((1, lambda a: size))
    return dict(name=name, popped=4, x87=False, thiscall=True, nodes=emit_nodes, dispatch=emit_targets,
                fuzz=emit_fuzz, direct_args=emit_direct_args, windows=windows)


SPECS = {
    0x4E81B0: emit_spec("CMultiSignal1::Emit CSigCarToCarInputFilter (4e81b0)"),
    0x4E8270: emit_spec("CMultiSignal1::Emit CSigCarSensorData (4e8270)", 0x40),
    0x4E8330: emit_spec("CMultiSignal1::Emit CSigSolidGeneralState (4e8330)"),
    0x4E83F0: emit_spec("CMultiSignal1::Emit CSigCarGeneralState (4e83f0)"),
    0x4E84B0: emit_spec("CMultiSignal1::Emit float (4e84b0)", 4),
    0x4E88F0: emit_spec("CMultiSignal1::Emit CSigCarStandStill (4e88f0)", 8),
    0x4E8B30: emit_spec("CMultiSignal1::Emit CSigCarPosition (4e8b30)", 0x14),
    0x4E8BF0: emit_spec("CMultiSignal1::Emit CSigMrRCarState (4e8bf0)", 0xC),
    0x4E8CB0: emit_spec("CMultiSignal1::Emit CSigMrRCarInit (4e8cb0)", 8),
}

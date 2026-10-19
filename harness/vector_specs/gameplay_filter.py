# The input filter's Step (E16Filter::Step, 0x48af90) and the quantizer's Pack (E16SteerState::Pack, 0x48c910), for the
# gameplay crate (rust/rbr-gameplay: filter.rs `step`, control.rs `QuantizedControl::pack`). Run with `--set gameplay_filter`.
# A playback feeds the filter the player's idle input (the recorded control is emitted after the filter), so these are
# direct calls on fuzzed inputs. The grid is the playback's: the E16Filter header of the snapshot's app (the quick race's
# INI grid, as tests/filter_peek.rs checks it). Half of the calls keep those cells; the other half give every cell random
# parameters, because the playback's pedal cells are inactive and their curve and rates are otherwise never run.

SET_NAME = "gameplay_filter"

APP_GLOBAL = 0x7EAC48  # the CApplication pointer (DAT_007eac48)
FILTER_AT = 0x7E0  # E16Filter inside the app (app +0x7e0)
FILTER_HEADER = 0x370  # the grid (5 groups of 0x94) and the speed fade input m_36c at +0x36c
GROUP = 0x94
CELL = 0x24
SPEED_AT = 0x36C


def _playback_header(mu):
    """The E16Filter header of the snapshot's app: the grid as the playback's quick race left it."""
    return bytearray(bytes(mu.mem_read(word(mu, APP_GLOBAL) + FILTER_AT, FILTER_HEADER)))


def _cell(rng):
    """One cell's nine words: active, nonlinear, linear, quadratic, cubic, rise, fall, speed fade, fade speed."""
    return struct.pack(
        "<2I7f",
        rng.random() < 0.7,
        rng.random() < 0.5,
        rng.choice((1.0, rng.uniform(0.2, 1.5))),
        rng.choice((0.0, rng.uniform(-1.0, 1.0))),
        rng.choice((0.0, rng.uniform(-1.0, 1.0))),
        rng.choice((0.0, rng.uniform(0.0, 8.0))),
        rng.choice((0.0, rng.uniform(0.0, 8.0))),
        rng.choice((0.0, rng.uniform(0.0, 1.0))),
        rng.choice((0.0, rng.uniform(0.5, 60.0))),
    )


def _raw(rng, idx):
    """A raw channel value, never zero: steering (channel 0) over [-1, 1] in both signs, a pedal over (0, 1]. Now and
    then the ends and the byte steps 1/255 and 2/255 (the quantizer's rounding)."""
    if rng.random() < 0.2:
        ends = (1.0, 1 / 255, 2 / 255, -1.0, -1 / 255) if idx == 0 else (1.0, 1 / 255, 2 / 255, 254 / 255)
        return rng.choice(ends)
    return rng.uniform(-1.0, 1.0) if idx == 0 else rng.uniform(1e-6, 1.0)


def _previous(rng, idx, a):
    """The filter's last output for the channel: the target itself now and then (no movement), else a state in range."""
    if rng.random() < 0.2:
        return a
    return rng.uniform(-1.0, 1.0) if idx == 0 else rng.choice((0.0, rng.uniform(0.0, 1.0)))


def _step_direct(mu, heap, rng):
    """E16Filter::Step(idx, a, b, dt) on the playback's grid with random selectors (0 to 3) and speed fade input."""
    filt = heap + 0x10000
    header = _playback_header(mu)
    if rng.random() < 0.5:
        for group in range(5):
            for cell in range(4):
                at = group * GROUP + 4 + cell * CELL
                header[at:at + CELL] = _cell(rng)
    for group in range(5):
        struct.pack_into("<I", header, group * GROUP, rng.randrange(4))
    struct.pack_into("<f", header, SPEED_AT, rng.choice((0.0, rng.uniform(0.0, 80.0))))
    idx = rng.randrange(5)
    a = _raw(rng, idx)
    b = _previous(rng, idx, a)
    dt = rng.choice((0.007, rng.uniform(0.001, 0.03)))
    words = [idx] + [struct.unpack("<I", struct.pack("<f", x))[0] for x in (a, b, dt)]
    return words, [(filt, bytes(header))], filt


def _steer(rng):
    return rng.choice((rng.uniform(-1.0, 1.0), rng.choice((1.0, -1.0, 1 / 255, -1 / 255, 0.0))))


def _pedal(rng):
    return rng.choice((rng.uniform(0.0, 1.0), rng.choice((0.0, 1.0, 1 / 255, 254 / 255, 0.5))))


def _pack_direct(_mu, heap, rng):
    """E16SteerState::Pack(p) into a state whose bytes are random, so a write outside +0x74..+0x7c shows as a change.
    p is the filter's output (E16Filter +0x310): flags, aux and byte 2 (any bits, the low byte is kept), steering and the
    four pedal channels in their ranges, and the gear byte."""
    state, filtered = heap + 0x20000, heap + 0x21000
    flags = rng.getrandbits(32) if rng.random() < 0.3 else rng.getrandbits(8)
    p = bytearray(0x2C)
    struct.pack_into("<3I", p, 0, flags, rng.getrandbits(32), rng.getrandbits(32))
    struct.pack_into("<5f", p, 0x14, _steer(rng), _pedal(rng), _pedal(rng), _pedal(rng), _pedal(rng))
    p[0x28] = rng.randrange(256)
    junk = bytes(rng.getrandbits(8) for _ in range(0x80))
    return [filtered], [(state, junk), (filtered, bytes(p))], state


SPECS = {
    # thiscall with the channel index and three floats (ret 0x10); returns a float in st0. Windows: the header (the
    # grid and m_36c), before and after; Step writes nothing, so the two are equal.
    0x48AF90: dict(name="E16Filter::Step (48af90)", popped=0x10, x87=True, thiscall=True,
                   windows=[(0, lambda a: FILTER_HEADER)], direct_fuzz=_step_direct),
    # thiscall with the filter's output pointer (ret 4). Windows: the state (+0x74..+0x7c are the nine bytes) and p.
    0x48C910: dict(name="E16SteerState::Pack (48c910)", popped=4, x87=False, thiscall=True,
                   windows=[(0, lambda a: 0x80), (1, lambda a: 0x2C)], direct_fuzz=_pack_direct),
}

"""E60Flow::M5666b0 (0x5666b0, thiscall on the flow): the damage-select word MotionProbe (0x4e7730) reads. The host
object at the flow's +0xc98 answers with its word at +0x17854 when its first word is 0x7b, else the global at 0x166084c.
The store's 85 inner records of it (MotionProbe's calls) carry no branch tag, so the sides are not
known to be covered: the direct recipe draws the host's first word (0x7b, and the words around it) and starts the call at its
entry.

E60Flow::M566640 (0x566640, thiscall on the same flow): the flow check Update9AF0 makes. Its state word is the host's +0x17850
when the host's first word is 0x7b, else the flow's first word; the flow holds unless that word is 3 and the mode word (the
host's +0x17858, else the global 0x7d6128's +0x8e84) is 2 or more. The recipe draws the state, the mode and the tag, so both
sides of each test run.

    harness/direct_calls.py SNAPSHOT OUT.txt --set g-flow --addr 0x5666B0 --count N --seed SEED
    harness/direct_calls.py SNAPSHOT OUT.txt --set g-flow --addr 0x566640 --count N --seed SEED
    harness/direct_calls.py SNAPSHOT OUT.txt --set g-flow --addr 0x5665F0 --count N --seed SEED
"""

import struct  # noqa: E402 (the recipe packs words)

SET_NAME = "g-flow"

FLOW, HOST = 0x90000, 0x100000  # stub heap offsets (heap + offset): the flow and its host object
DAMAGE_GLOBAL = 0x0166084C


def _flow_direct(mu, heap, rng):
    del mu
    flow, host = heap + FLOW, heap + HOST
    tag = rng.choice((0x7B, 0x7B, 0x7A, 0x7C, 0, rng.getrandbits(16)))
    flow_image = bytearray(0xC9C)
    struct.pack_into("<I", flow_image, 0xC98, host)
    host_image = bytearray(0x17858)
    struct.pack_into("<H", host_image, 0, tag)
    struct.pack_into("<I", host_image, 0x17854, rng.getrandbits(32))
    writes = [(DAMAGE_GLOBAL, struct.pack("<I", rng.getrandbits(32))), (flow, bytes(flow_image)),
              (host, bytes(host_image))]
    return [], writes, flow


def _flow_nodes(mu, a):
    """The host's first word and its +0x17854, and the global (read at entry, so the check sees the inputs)."""
    host = word(mu, a[0] + 0xC98)  # noqa: F821 (vcu_vectors' word)
    return [(host, 4), (host + 0x17854, 4), (DAMAGE_GLOBAL, 4)]


MODE_GLOBAL = 0x007D6128  # the pointer whose +0x8e84 word is the mode (E13's)
MODE_AT = 0x8E84
DAEMON = 0x200000  # stub heap offset of the object the mode global points at


def _flow_check_direct(mu, heap, rng):
    del mu
    flow, host, daemon = heap + FLOW, heap + HOST, heap + DAEMON
    tag = rng.choice((0x7B, 0x7B, 0x7A, 0, rng.getrandbits(16)))
    flow_image = bytearray(0xC9C)
    struct.pack_into("<I", flow_image, 0, rng.choice((3, 3, rng.getrandbits(3))))
    struct.pack_into("<I", flow_image, 0xC98, host)
    host_image = bytearray(0x17858 + 4)
    struct.pack_into("<H", host_image, 0, tag)
    struct.pack_into("<I", host_image, 0x17850, rng.choice((3, 3, rng.getrandbits(3))))
    struct.pack_into("<I", host_image, 0x17858, rng.randrange(4))
    daemon_image = bytearray(MODE_AT + 4)
    struct.pack_into("<I", daemon_image, MODE_AT, rng.randrange(4))
    writes = [(MODE_GLOBAL, struct.pack("<I", daemon)), (flow, bytes(flow_image)), (host, bytes(host_image)),
              (daemon, bytes(daemon_image))]
    return [], writes, flow


def _flow_check_nodes(mu, a):
    """The host's tag and words the check reads, and the mode global with the word its pointer holds."""
    host = word(mu, a[0] + 0xC98)  # noqa: F821 (vcu_vectors' word)
    daemon = word(mu, MODE_GLOBAL)  # noqa: F821
    return [(host, 2), (host + 0x17850, 4), (host + 0x17858, 4), (MODE_GLOBAL, 4), (daemon + MODE_AT, 4)]


SPECS = {0x5666B0: dict(SETS["car_body"][0x5666B0], direct_fuzz=_flow_direct,  # noqa: F821 (the car body set's names)
                        windows=[(0, lambda a: 0xC9C)], nodes=_flow_nodes),
         0x566640: dict(SETS["car_body"][0x566640], direct_fuzz=_flow_check_direct,  # noqa: F821
                        windows=[(0, lambda a: 0xC9C)], nodes=_flow_check_nodes),
         # E60Flow::M5665f0 (0x5665f0): byte-identical to 0x566640 apart from the jumps' targets, so the same recipe.
         0x5665F0: dict(SETS["car_body"][0x566640], name="E60Flow::M5665f0", direct_fuzz=_flow_check_direct,  # noqa: F821
                        windows=[(0, lambda a: 0xC9C)], nodes=_flow_check_nodes)}

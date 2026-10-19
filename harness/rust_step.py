"""Replay StepForward of the original from a snapshot with Rust ports in place of original functions, and compare
it with the original (lead/reports/rust-integration-design.md).

Usage: .venv/bin/python -I -B harness/rust_step.py SNAPSHOT... [--recording DRIVE.bin] [--only ADDR]...
                                                    [--each] [--steps N] [--perturb DELTA] [--excluded] [--full-fill]
                                                    [--no-cache]
  Needs `cargo build --release -p rbr-hybrid` in rust/ (RBR_HYBRID names another library).

  Run B stops at the entry of every function rbr-hybrid binds (or only the --only ones), runs the binding on the
  emulator's memory and returns to the caller as the original's ret would (eax, the x87 result, the popped
  arguments). A binding calls the original callees its port leaves to the caller through a nested run of the
  emulator; a callee that is bound runs in Rust again. A binding can decline (inputs outside its port): then the
  original runs, and the count is reported.

  One step (default): run A is the original (harness/replay.py). Every byte of the two runs is compared, except the
  stack below the call, the bytes that depend on the stubbed Windows calls (as replay.py), and the bytes that depend
  on the stack a binding leaves: the original copies uninitialised locals (lane 3 of an E01Vec4) into the heap and
  the wheels, and a binding leaves other stack than the original did. Run B is made twice, the free stack filled
  with a different word after every binding (FILLS). A byte where run B differs from run A under either fill is
  excluded only by classify's rules: diverge.py's static mask (the car's and wheels' lane 3), a heap pointer in
  both runs, a word that is NaN in both runs and differs only in its sign (x86's default NaN is negative, arm64's
  positive), the original's stale-stack noise (below), or a byte run A never reads (its reader is reported when it
  does).
  --excluded lists all of them as ranges, with the instructions (or bindings) that last wrote them (after the last
  step with --steps; it logs every write, so it is slow). The set found on the 9 tanner-corolla snapshots and their
  chains is in lead/reports/rust-integration-handover.md (Traps). --recording also compares the car and wheels of
  both runs with the drive's frame after the step (against_recording). --each runs B once per bound entry (or --only
  entry), alone, for attribution.
  The fill of a binding's call (the default; --full-fill fills its whole free stack instead): run B's free stack below
  each call's entry gets the bytes run A wrote there during the same call (record_calls, matched by the entry esp and
  the occurrence). A call run A did not make at that entry falls back to the whole free stack; the summary line
  counts the aligned calls and the fallbacks. Either way the fill leaves the bytes run B's original code wrote below
  the entry during the call (its binding's nested runs, track_own_writes): they are the original's stack, not the
  binding's, and a nested run at the original's entry (call_at) leaves what run A's did.
  A copy is not a use: a 16-byte copy's load (COPY_STORES) moves the bytes it reads, so a difference it carries
  opens the bytes its store writes, and the source stays open. Only a read by another instruction is a failure. The
  copies that carry excluded bytes are listed with --excluded.

  --steps N chains N steps of runs A and B (both fills) in lockstep from each snapshot, as the plugin's playback
  does: Emit (0x48cd90, this = [0x893600] + 0xc90) with the recording's control for the step, then
  StepForward(1) (needs --recording). Run B is compared with run A by classify's rules: every byte of the writable
  memory and the stub heap after every step (a byte that differs opens its read window, chain), classified after the
  last step, with the step where the first failing byte began to differ; both use the same stub heap, so their
  pointers agree. Run A is compared with the recording after every step. Ignored there: the words that differ before the
  first step (run noise: pointers, uninitialised memory), diverge.py's static mask, wheel+0x9ac (RECORDING_GARBAGE),
  and heap pointers: where run A holds a 4-aligned address (one the snapshot maps, or in replay.py's stub heap)
  and the recording a 4-aligned user-space address (the recorded run's heap may have grown past the snapshot's),
  an allocator placed a block elsewhere than in the recorded run; those words are counted, not compared. A
  pointer that is 0 on one side only is a difference.
  The original's stale-stack noise, the same kind of rule for run B vs run A: the original computes some values from
  stack it never wrote (lane 3 of a local E01Vec4 that only xyz is written to, copied whole into the car, the
  bodies, the E38 solver's contact records; lead/reports/call-fill.md item 2). Run B's bindings leave other stack,
  so those values differ in run B without a port difference. Run A is also run twice more, as the original only,
  each read of a dead stack byte (esp rose above it, or it lay below a step's entry, and it was not written since)
  getting 00 or 7f (stale_machine, one per FILLS). A word that differs between the two after a step is the
  original's own nondeterminism: where run B differs from run A at a byte that is read, and the byte's word
  differed between the stale-stack runs at every step it differed in run B, it is excluded (classify) and counted
  in the summary by region and lane (lane 3 is offset 0xc mod 16 from the object, object_base; a byte of a known
  object outside lane 3 is printed as a WARNING: real physics would depend on garbage; one whose object is not
  known is listed). The stale-stack runs are part of run A's cache. Their x87 results are run A's, modelled ones
  not counted (Uncounted). diverge.py's static mask (STATIC_FRAME_WORDS, STACK_GARBAGE) covers the car's and
  wheels' part of this class by a fixed list and still applies first.
  Run A of a chain is cached (RunA) under RBR_STEP_CACHE (default cache/run-a next to the worktree; --no-cache runs
  it): per step, its memory changes, call log, x87 results, and the accesses that can decide a read window. The file
  is named by a hash of the snapshot, the recording's controls for the steps, the step count, the bound entries (the
  call log), replay.py, run A's code here, the exe and the unicorn version; the x87 results run A used are checked
  against the oracle when it is loaded. The output is the same with or without it; a cached chain skips run A's
  emulator (about 1.5 MB per 30-step chain).

  x87 results: replay.py's cache (RBR_REPLAY_FPU_CACHE); an fsin/fcos it lacks is modelled (the f64 sin/cos as an
  extended value; it rounds to the hardware's 24-bit result on every cached case). A step that used a modelled
  result is reported as unverified. With RBR_REMOTE (a build directory on cachy) the modelled results are
  fetched from the CPU instead and the runs repeated until none is modelled. --perturb adds DELTA to every x87
  result of a binding (rounded to f32), to show that the comparison sees a wrong port.
"""
import argparse
import collections
import collections.abc
import contextlib
import ctypes
import hashlib
import inspect
import io
import math
import os
import pickle
import struct
import sys
import time
import weakref
import zlib

import numpy as np
import unicorn
from unicorn import UC_HOOK_CODE, UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE
from unicorn.unicorn_py3.unicorn import uclib  # the loaded libunicorn, whose functions the bindings call
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import diverge  # noqa: E402
import replay  # noqa: E402

LIB = os.environ.get("RBR_HYBRID", os.path.join(replay.REPO, "rust", "target", "release", "librbr_hybrid" + (
    ".dylib" if sys.platform == "darwin" else ".so")))
EMIT, EMIT_OWNER, EMIT_OFFSET = 0x48CD90, 0x893600, 0xC90
FSIN, FCOS = 5, 6  # replay-fpu.c cases
# Sound code the stubs can't run, skipped in both runs: {address: where it continues}. The engine sound's sample
# start (0x4af280) after its two rand() calls (kept, the CRT state is shared): its voice needs a DirectSound buffer
# (IDirectSound::CreateSoundBuffer, then Lock and the 3D interface), which replay.py fails, and the voice pointer
# is then used unchecked. Reached at step 1,007 of tanner-corolla. Only sound state is left unwritten. The same for
# the sample start at 0x4b2ed0 (step 12,876), after its rand() calls: its voice (the sound manager's slot 0x50)
# would create a buffer the same way.
SKIPS = {0x4AF2DC: 0x4AF3DB, 0x4B2F41: 0x4B2FF6}
# Frame words run A computes from stack garbage, which differs in the emulator (the stubbed Windows calls leave
# none): lane 3 at wheel+0x9ac (M3AFB0 reads it). Not compared with the recording only; run B is compared with run A.
RECORDING_GARBAGE = [(replay.CAR_BYTES + k * replay.WHEEL_BYTES + 0x9AC) // 4 for k in range(4)]
# car+0x660, car+0x670 and car+0x814: written by CCar::Vf28x (0x4ea020; the first two by its drive call 0x508480 at
# 0x5084ab/0x5084d9, the third by its engine call 0x518cf0 at 0x518d26). The game calls Vf28x once per rendered frame,
# outside StepForward; the replay has no frame loop (FRM1 holds no frame boundaries), and a StepForward read watch on
# snap-2800 for 300 steps saw none of these words read. The direct records show +0x660 and +0x814 changing together
# (vectors/car_body/s28-direct.txt; lead/reports/o670.md, g-vf28.md, standalone-s8.md). Not compared with the recording
# only, like the garbage above. pure.rs's FRAME_LOOP_CAR is the same list.
RECORDING_GARBAGE += [0x660 // 4, 0x670 // 4, 0x814 // 4]
# The wheel lane-3 words diverge.py's static mask holds (wheel+0x99c, 0x9bc, 0x9dc: stack residue the original leaves,
# read only lane-wise, so it stays in lane 3): not compared between run B and run A either.
# wheel+0xe5c and +0xe6c are outside the recorded wheel (0xa00 bytes), so they are not listed here.
# wheel+0x73c: v1-gravel 1000, wheel 1: run A differs from the recording at steps 1292-1295, the stale-stack runs do not
# (lead/reports/g-v1stops.md).
STACK_GARBAGE = [(replay.CAR_BYTES + k * replay.WHEEL_BYTES + off) // 4 for k in range(4)
                 for off in (0x99C, 0x9BC, 0x9DC, 0x73C)]
# The car's lane-3 words of diverge.py's static mask (the same stale-stack class), and the ones the stale-stack runs
# or run A's differences from the recording prove (lead/reports/standalone-s5.md): car+0x161c and car+0x22c (arctic
# 12380: the stale-stack runs differ there), car+0x32c and car+0x38c (tanner-damage 12000, tarmac-lancer 9000: run A
# differs from the recording, the stale-stack runs do not); car+0x19c, 0x20c and 0x2ec (v1-gravel 1000: run A differs
# from the recording from step 1261, the stale-stack runs do not) and car+0x34c, 0x8c, 0x9c, 0x14c, 0x33c (v1-snow 6120: run A
# is bit-exact, the stale-stack runs differ there from step 6126-6128; lead/reports/g-v1stops.md). pure.rs's STALE_CAR
# is the same list. 0x15DC is stale-derived: CCar::Vf64x (0x4e9020) blends 0x38C's lane 3 into it (0.7/0.3), so the
# pure step's own stack shows there (lead/reports/standalone-s12.md).
CAR_STATIC = (0x1CC, 0x8EC, 0x90C, 0x147C, 0x149C, 0x14BC, 0x14CC, 0x14EC, 0x150C, 0x161C, 0x22C, 0x32C, 0x38C,
              0x65C, 0x19C, 0x20C, 0x2EC, 0x34C, 0x8C, 0x9C, 0x14C, 0x33C, 0x15DC)
STATIC_FRAME_WORDS = [off // 4 for off in CAR_STATIC] + STACK_GARBAGE
# The original reads stack its callees left behind: an E01Vec4 local whose lane 3 is never set is copied whole
# (movups) into the heap and the wheels. A binding leaves other stack than the original callee did, so run B is
# made twice, the free stack after every binding filled with one of FILLS. A byte that differs from run A under
# either fill is excluded only by classify's rules. STACK_FILL covers the callee's frames and the host calls'.
STACK_FILL = 0x3000
FILLS = (b"\0\0\0\0", b"\x7f\x7f\x7f\x7f")
STALE = "the original's stale-stack noise"  # classify's class of the words run A's stale-stack runs disagree on
# The 16-byte copies in the exe: a movups or movaps load of an xmm register, stored unchanged by the next store
# (objdump of RichardBurnsRally_SSE.exe: 109 pairs; E01Vec4::CopyFrom at 0x43c5b0 is 0x43c5ba -> 0x43c5c0). A load
# moves the bytes it reads and does not use them, so a window carries over to the store's bytes (Windows).
COPY_STORES = {
    0x43c5ba: 0x43c5c0, 0x465e2d: 0x465e38, 0x465e30: 0x465e3b, 0x465e34: 0x465e3f, 0x4839ba: 0x4839c0,
    0x483eeb: 0x483ef1, 0x48e92d: 0x48e939, 0x48e930: 0x48e93c, 0x4937ba: 0x4937c1, 0x493818: 0x49381f,
    0x4e0d97: 0x4e0d9e, 0x4e393d: 0x4e3943, 0x4e39bd: 0x4e39c3, 0x4e4f2b: 0x4e4f31, 0x4e93a6: 0x4e93ad,
    0x4e93ec: 0x4e93f3, 0x4e9683: 0x4e968a, 0x4e973b: 0x4e9742, 0x4ea0d7: 0x4ea0de, 0x4f3d2a: 0x4f3d30,
    0x4f88e7: 0x4f88ed, 0x4facbf: 0x4facc6, 0x4facdd: 0x4face4, 0x51bcf3: 0x51bcf9, 0x51be9e: 0x51bea5,
    0x51bfbf: 0x51bfc6, 0x51c2f4: 0x51c2fb, 0x51c579: 0x51c580, 0x51c6ae: 0x51c6b4, 0x51c763: 0x51c76a,
    0x51dad6: 0x51dadd, 0x51db0f: 0x51db16, 0x51f1e7: 0x51f1ee, 0x51fae8: 0x51faef, 0x51fc2f: 0x51fc36,
    0x51fd74: 0x51fd7b, 0x51ff22: 0x51ff29, 0x521a21: 0x521a28, 0x521f23: 0x521f2a, 0x521f4e: 0x521f55,
    0x521fb1: 0x521fb8, 0x52206b: 0x522072, 0x522173: 0x52217a, 0x522200: 0x522207, 0x5223b0: 0x5223b7,
    0x524748: 0x52474f, 0x525f40: 0x525f47, 0x529e23: 0x529e2a, 0x52bfc6: 0x52bfcd, 0x52bffe: 0x52c005,
    0x52c0c5: 0x52c0cc, 0x52c0fd: 0x52c104, 0x52c1b6: 0x52c1bd, 0x52c1ee: 0x52c1f5, 0x52c674: 0x52c67b,
    0x52cf89: 0x52cf90, 0x52fc44: 0x52fc4b, 0x53916a: 0x539170, 0x5dbe77: 0x5dbe7e, 0x5dbe9a: 0x5dbea1,
    0x5dbeba: 0x5dbec1, 0x5dc77d: 0x5dc784, 0x5dc7a4: 0x5dc7ab, 0x5dd93b: 0x5dd942, 0x5e915d: 0x5e9164,
    0x5ea3ab: 0x5ea3b1, 0x5ea5f9: 0x5ea600, 0x5ea63b: 0x5ea642, 0x5eae0f: 0x5eae16, 0x5eae7c: 0x5eae83,
    0x5ecdda: 0x5ecde1, 0x5ed2a6: 0x5ed2b0, 0x5f631d: 0x5f632c, 0x5f6320: 0x5f632f, 0x5f6324: 0x5f6333,
    0x5f6328: 0x5f6337, 0x5f98bf: 0x5f98c6, 0x5f98ed: 0x5f98f4, 0x5f99c7: 0x5f99ce, 0x5f99f6: 0x5f99fd,
    0x600904: 0x60090a, 0x609d0e: 0x609d15, 0x609d2c: 0x609d33, 0x60d3f0: 0x60d3f7, 0x60d974: 0x60d97b,
    0x60d99e: 0x60d9a5, 0x60d9d4: 0x60d9db, 0x60dbba: 0x60dbc1, 0x60dbd7: 0x60dbde, 0x60dc50: 0x60dc57,
    0x60dc7a: 0x60dc81, 0x69e0cf: 0x69e0d5, 0x69e11c: 0x69e11f, 0x69e129: 0x69e12f, 0x69e169: 0x69e16f,
    0x69f08c: 0x69f08f, 0x69f0ef: 0x69f0f2, 0x69f16a: 0x69f170, 0x69f17f: 0x69f184, 0x69f547: 0x69f54a,
    0x69f556: 0x69f559, 0x6a036f: 0x6a0372, 0x6a04bf: 0x6a04c7, 0x6a0e52: 0x6a0e59, 0x6a0e5e: 0x6a0e62,
    0x6a2802: 0x6a2808, 0x6a284f: 0x6a2852, 0x6a285c: 0x6a2862, 0x6a289c: 0x6a28a2,
}

LOG = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32)
STOPPED = ctypes.CFUNCTYPE(ctypes.c_int32, ctypes.c_void_p)
FAIL = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_char_p)
FILL_CALL = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32)
# The unicorn functions rbr-hybrid calls itself (its ffi.rs UcApi, in order): the loaded libunicorn's.
UC_API = ("mem_read", "mem_write", "reg_read", "reg_write", "emu_start", "emu_stop", "context_alloc", "context_save",
          "context_restore", "context_free", "hook_add", "strerror")


class UcApi(ctypes.Structure):
    _fields_ = [("uc", ctypes.c_void_p)] + [(name, ctypes.c_void_p) for name in UC_API]


class HarnessSide(ctypes.Structure):
    _fields_ = [("ctx", ctypes.c_void_p), ("log", LOG), ("stopped", STOPPED), ("fail", FAIL),
                ("fill_call", FILL_CALL)]


class Options(ctypes.Structure):
    _fields_ = [("has_fill", ctypes.c_uint32), ("fill", ctypes.c_uint32), ("stack_fill", ctypes.c_uint32),
                ("stack_floor", ctypes.c_uint32), ("has_perturb", ctypes.c_uint32), ("perturb", ctypes.c_double),
                ("ngp_base", ctypes.c_uint32)]


def ngp_base(snap):
    """NGP.dll's base in a snapshot, 0 without NGP: under NGP the CCar::CDef key dict (0x8ef73c) points at NGP's copy
    of its table (file address 0x10171318) instead of the exe's (0x7c6578, notes/ngp-load-path.md)."""
    table = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0]
    return 0 if table == 0x7C6578 else table - 0x171318


def at_snapshot(addrs, base):
    """Bound entries where the snapshot has them: NGP.dll's (file addresses, rbr-hybrid's ngp.rs) at its base, none
    without NGP."""
    ngp = lambda a: 0x10000000 <= a < 0x10200000
    return [a - 0x10000000 + base if ngp(a) else a for a in addrs if base or not ngp(a)]


def book_lib(path=LIB):
    """rbr-hybrid, loaded once: the bindings (Hybrid) and the bookkeeping hooks (Book)."""
    global _lib
    if _lib is None:
        _lib = ctypes.CDLL(path)
        p, u32 = ctypes.c_void_p, ctypes.c_uint32
        out = [ctypes.POINTER(ctypes.POINTER(u32)), ctypes.POINTER(u32)]
        _lib.rbr_book_new.restype = p
        _lib.rbr_book_new.argtypes = [p, u32]
        _lib.rbr_book_free.argtypes = [p]
        _lib.rbr_book_version.restype = u32
        for name, args in (("accesses", [p, u32, p, u32]), ("calls", [p, u32]), ("stale", [u32] * 5 + [ctypes.c_uint8]),
                           ("log_writes", []), ("b_window", [p, p, u32]), ("own_writes", [p, u32, u32, u32]),
                           ("count_entries", [p, u32, u32, u32])):
            getattr(_lib, "rbr_book_" + name).argtypes = [p] + args
        for name, args in (("take_accesses", out), ("take_calls", [ctypes.c_int32] + out), ("written", out),
                           ("b_reads", out), ("entries", out), ("step", [u32]), ("note_write", [u32] * 3),
                           ("commit", []), ("last", [u32, ctypes.POINTER(u32)]), ("last_len", []),
                           ("set_open", [p, u32]), ("fill", [u32, u32]), ("set_record", [p, u32]),
                           ("fill_stats", [ctypes.POINTER(ctypes.c_uint64)])):
            getattr(_lib, "rbr_book_" + name).argtypes = [p] + args
        _lib.rbr_book_last_len.restype = u32
    return _lib


_lib = None


def uc_api(m):
    """The UcApi of machine m's engine: the loaded libunicorn's functions."""
    return UcApi(m.mu._uch.value, *(ctypes.cast(getattr(uclib, "uc_" + name), ctypes.c_void_p).value
                                    for name in UC_API))


def words_of(xs):
    """xs (an iterable of ints, or a uint32 array) as a contiguous uint32 array and its pointer."""
    xs = np.ascontiguousarray(np.fromiter(xs, np.uint32) if not isinstance(xs, np.ndarray) else xs, np.uint32)
    return xs, xs.ctypes.data_as(ctypes.c_void_p)


class Book:
    """A machine's bookkeeping hooks, native (rbr-hybrid's book.rs): the record of run A's accesses
    (record_accesses) and calls (record_calls), the stale-stack runs' fill (fill_stale_reads), and run B's write log
    (last_writers), read windows (track_b_window), own writes and call fill (track_own_writes, install) and entry
    counts (count_entries). The rules that use them are here; book.rs only records, as the Python hooks did."""

    def __init__(self, m):
        self.lib = book_lib()
        self.ptr = self.lib.rbr_book_new(ctypes.byref(uc_api(m)), STACK_FILL)
        weakref.finalize(m.mu, self.lib.rbr_book_free, self.ptr)  # with the engine, which runs nothing after that

    def hook(self, name, *args):
        if not getattr(self.lib, "rbr_book_" + name)(self.ptr, *args):
            raise RuntimeError("rbr_book_%s: a hook could not be added" % name)

    def array(self, name, *args):
        """The array a query returns, copied."""
        p, n = ctypes.POINTER(ctypes.c_uint32)(), ctypes.c_uint32()
        getattr(self.lib, "rbr_book_" + name)(self.ptr, *args, ctypes.byref(p), ctypes.byref(n))
        return np.ctypeslib.as_array(p, (n.value,)).copy() if n.value else np.zeros(0, np.uint32)

    def log_writes(self):
        self.hook("log_writes")

    def step(self, tag):
        """The step the calls from now on belong to (m.tag): record_calls' tag; the call fill's occurrences restart."""
        self.lib.rbr_book_step(self.ptr, tag)

    def set_record(self, log):
        """Run A's call log for the call fill (record_calls' format; None: none)."""
        xs, p = words_of(log if log is not None else ())
        self.lib.rbr_book_set_record(self.ptr, p, len(xs))

    def written(self):
        """The bytes the step's logged writes covered so far, ascending."""
        return self.array("written")


def book(m):
    """Machine m's Book, made at the first use."""
    if not hasattr(m, "book"):
        m.book = Book(m)
    return m.book


class LastWriters:
    """A run's last writers ({address: the instruction (or binding) that last wrote it}, last_writers), in its Book."""

    def __init__(self, b):
        self.b, self.out = b, ctypes.c_uint32()

    def get(self, x, default=None):
        return self.out.value if self.b.lib.rbr_book_last(self.b.ptr, x, ctypes.byref(self.out)) else default

    def __getitem__(self, x):
        w = self.get(x)
        if w is None:
            raise KeyError(x)
        return w

    def __contains__(self, x):
        return self.get(x) is not None

    def __len__(self):
        return self.b.lib.rbr_book_last_len(self.b.ptr)


class Counts(collections.abc.Mapping):
    """A live view of what the installed bindings counted: m.rust {addr: calls}, m.declined {addr: {reason: n}},
    m.uncovered {(addr, binding): host calls a pure step would stop at (rbr_hybrid::pure::uncovered)}."""

    def __init__(self, hybrid, native, kind):
        self.hybrid, self.native, self.kind = hybrid, native, kind

    def _now(self):
        out = {"R": {}, "D": {}, "U": {}}
        for line in self.hybrid.lib.rbr_hybrid_counts(self.native).decode().splitlines():
            kind, addr, n, *why = line.split(" ", 3)
            if kind == "R":
                out[kind][int(addr)] = int(n)
            elif kind == "D":
                out[kind].setdefault(int(addr), {})[why[0]] = int(n)
            else:
                out[kind][int(addr), int(why[0])] = int(n)
        return out[self.kind]

    def __getitem__(self, key):
        return self._now()[key]

    def __iter__(self):
        return iter(self._now())

    def __len__(self):
        return len(self._now())


def stale_sources(lib):
    """The library's sources newer than the built library (src/, build.rs and Cargo.toml of rbr-hybrid and the crates
    it uses; tests and src/bin are not part of it), so a run never tests old bindings; empty when the library is
    current."""
    built = os.path.getmtime(lib)
    crates = [os.path.join(replay.REPO, "rust", c) for c in ("rbr-hybrid", "rbr-physics", "rbr-formats", "rbr-recording")]
    files = [os.path.join(c, f) for c in crates for f in ("build.rs", "Cargo.toml") if os.path.isfile(os.path.join(c, f))]
    files += [os.path.join(d, f) for c in crates for d, _, fs in os.walk(os.path.join(c, "src")) for f in fs
              if f.endswith(".rs") and os.path.join("src", "bin") not in d]
    return [f for f in files if os.path.getmtime(f) > built]


class Hybrid:
    """rbr-hybrid: the bound entries, hooked in a machine's engine by the library itself (its ffi.rs)."""

    def __init__(self, path=LIB):
        if "RBR_HYBRID" not in os.environ:
            if not os.path.exists(path):
                sys.exit("rust_step.py: no %s; build it: cargo build --release -p rbr-hybrid (in rust/)" % path)
            newer = stale_sources(path)
            if newer:
                sys.exit("rust_step.py: %s is older than %d Rust source(s), e.g. %s; rebuild: cargo build --release "
                         "-p rbr-hybrid (in rust/)" % (path, len(newer), os.path.relpath(newer[0], replay.REPO)))
        self.lib = book_lib(path)
        self.lib.rbr_hybrid_bound.restype = ctypes.c_uint32
        self.lib.rbr_hybrid_install.restype = ctypes.c_void_p
        self.lib.rbr_hybrid_counts.restype = ctypes.c_char_p
        self.lib.rbr_hybrid_counts.argtypes = self.lib.rbr_hybrid_free.argtypes = [ctypes.c_void_p]
        buf = (ctypes.c_uint32 * 1024)()
        self.bound = list(buf[:self.lib.rbr_hybrid_bound(buf, 1024)])

    def install(self, m, only=None, perturb=None, fill=None, log_writes=False, record=None):
        """Hooks the bound entries (those in `only` when given) in machine m. m.rust counts the calls the bindings
        ran, m.declined {addr: {reason: count}} the rest. With `fill` (4 bytes), the free stack a binding leaves
        (STACK_FILL bytes below its entry, down to m.stack_floor, and its popped arguments) is filled with it. With
        `record` (run A's call log, record_calls) the fill is only the bytes run A wrote below the same call's entry:
        the bridge calls the book's call fill after each binding returns (book.rs, Book.call_fill), and a call that
        has no logged call under the step (Book.step), entry esp and occurrence falls back to the whole free stack,
        counted in its fill stats (fill_summary). With `perturb`, it is added to each x87 result the bindings return
        (rounded to f32). With log_writes, the original code's and the bindings' writes go to the book's write log
        (last_writers), a binding's attributed to the binding; the stubs' writes (Machine.poke) always do."""
        b, call = book(m), record is not None and fill is not None
        if log_writes:
            b.log_writes()
        poke = m.poke

        def logged_poke(addr, data):  # a stub's write, logged in order with the hooks' (Machine.poke)
            poke(addr, data)
            for at, n, _, eip in m.writes:
                b.lib.rbr_book_note_write(b.ptr, at, n, eip)
            del m.writes[:]
        m.poke = logged_poke

        def fail(_, message):
            m.stop = m.stop or message.decode()

        if call:
            track_own_writes(m, at_snapshot(only or self.bound, ngp_base(m.snap)))  # its block hooks before the bindings': unicorn runs them in order
            b.lib.rbr_book_fill(b.ptr, struct.unpack("<I", fill)[0], m.stack_floor)
            b.set_record(record)
        api = uc_api(m)
        side = HarnessSide(b.ptr, ctypes.cast(b.lib.rbr_book_binding_write, LOG) if log_writes else LOG(),
                           STOPPED(lambda _: bool(m.stop)), FAIL(fail),
                           ctypes.cast(b.lib.rbr_book_fill_call, FILL_CALL) if call else FILL_CALL())
        options = Options(fill is not None and not call, struct.unpack("<I", fill)[0] if fill else 0, STACK_FILL,
                          m.stack_floor, perturb is not None, perturb or 0.0, ngp_base(m.snap))
        hooked = (ctypes.c_uint32 * len(only))(*only) if only else None
        native = self.lib.rbr_hybrid_install(ctypes.byref(api), ctypes.byref(side), ctypes.byref(options), hooked,
                                             len(only) if only else 0)
        if not native:
            raise RuntimeError("rbr_hybrid_install: a hook could not be added")
        m.keep = side  # its callbacks, alive as long as the machine
        weakref.finalize(m.mu, self.lib.rbr_hybrid_free, native)  # with the engine, which runs nothing after that
        m.rust, m.declined, m.uncovered = (Counts(self, native, kind) for kind in "RDU")


def value(significand, exponent):
    """The float an x87 register holds (finite values)."""
    return (-1) ** (exponent >> 15) * math.ldexp(significand, (exponent & 0x7FFF) - 16383 - 63)


def extended(x):
    """A finite float as an x87 register: (significand, sign and exponent)."""
    m, e = math.frexp(abs(x))
    return int(m * 2 ** 64), (math.copysign(1, x) < 0) << 15 | (e - 1 + 16383 if m else 0)


class Modelled(dict):
    """The oracle's results, with fsin/fcos modelled when the cache lacks them; `used` counts the modelled results
    handed out, `modelled` holds their keys (to fetch from cachy)."""

    def __init__(self, known):
        super().__init__(known)
        self.used, self.modelled = 0, set()

    def get(self, key, default=None):
        res = dict.get(self, key)
        if res is None:
            res = model(key)
            if res is not None:
                self.used += 1
                self.modelled.add(key)
        return res if res is not None else default


def model(key):
    """An fsin/fcos result modelled from its operand (the f64 sin/cos as an extended value), or None."""
    if key[0] not in (FSIN, FCOS):
        return None
    s, e = struct.unpack("<QH", key[4:14])
    x = value(s, e)
    return struct.pack("<QH", *extended(math.sin(x) if key[0] == FSIN else math.cos(x))) + bytes(12)


def oracle():
    o = replay.FpuOracle()
    o.known = Modelled(o.known)
    return o


class Uncounted:
    """The oracle of the stale-stack runs (stale_machine): o's results, the modelled ones not counted. Their operands
    differ from run A's only where they derive from the filled garbage, so a modelled result there changes only words
    that depend on it anyway."""

    def __init__(self, o):
        self.o, self.known = o, self

    def get(self, key, default=None):
        res = dict.get(self.o.known, key)
        if res is None:
            res = model(key)
        return res if res is not None else default


def machine(snap, o, hybrid=None, only=None, perturb=None, fill=None, log_writes=False, record=None):
    m = replay.Machine(snap, log_writes=log_writes and not hybrid, oracle=o)  # a hybrid's book logs them (install)
    m.skipped, m.unverified_ok = 0, False
    m.stack_floor = snap.stack_lo + 0x100  # clear of advance()'s control scratch

    def skip(mu, addr, size, _):
        m.skipped += 1
        mu.reg_write(UC_X86_REG_EIP, SKIPS[addr])
    for addr in SKIPS:
        m.mu.hook_add(UC_HOOK_CODE, skip, begin=addr, end=addr)
    if hybrid:
        hybrid.install(m, only, perturb, fill, log_writes, record)
    return m


def finish(m):
    """Runs m from its start; the stop reason or None. x87 results missing from the cache stop it, unless
    m.unverified_ok (they keep the emulator's)."""
    m.run()
    if m.fpu_missing and not m.stop and not m.unverified_ok:
        m.stop = "%d x87 results missing from the cache" % len(m.fpu_missing)
    return m.stop


def words(d):
    """Recording words of car + wheel bytes (-0.0 as 0.0, as diverge.py)."""
    return [w if w != 0x80000000 else 0 for w in struct.unpack("<%dI" % (len(d) // 4), d)]


def against_recording(path, snap, a, b, garbage):
    """Runs A and B against the drive's frame after the step. Words that differ before the step are run noise
    (pointers, uninitialised memory); so are the words where run A differs from the drive (garbage the step
    computes from other uninitialised memory), and `garbage` (frame words that depend on the stack the bindings
    leave). Returns (post step, words, noise words, A's own differences, the words where B differs and A
    doesn't)."""
    before, _ = replay.recording_frame(path, snap.step - 1)
    after, post = replay.recording_frame(path, snap.step)
    pre = words(replay.Machine(snap, log_writes=False).frame())
    noise = {i for i, w in enumerate(words(before)) if w != pre[i]} | garbage
    ref, wa, wb = words(after), words(a.frame()), words(b.frame())
    own = [i for i in range(len(ref)) if ref[i] != wa[i] and i not in noise]
    bad = [i for i in range(len(ref)) if ref[i] != wb[i] and ref[i] == wa[i] and i not in noise]
    return post, len(ref), len(noise), own, bad


def frame_bytes(m, ws):
    """The addresses of frame words (car, then the 4 wheels, as Machine.frame())."""
    car = struct.unpack("<I", m.read(replay.CAR_PTR, 4))[0]
    wheels = struct.unpack("<4I", m.read(car + 0x470, 16))
    out = set()
    for i in ws:
        if 4 * i < replay.CAR_BYTES:
            at = car + 4 * i
        else:
            k, off = divmod(4 * i - replay.CAR_BYTES, replay.WHEEL_BYTES)
            at = wheels[k] + off
        out.update(range(at, at + 4))
    return out


# M3B900's lane-3 leftovers past the recorded wheel (0xa00 bytes): wheel+0xe5c is lane 3 of the CopyFrom'd E+0x1f0
# vector (asm E+0x1fc, o+0xe50), wheel+0xe6c is lane 3 of E+0x130 (asm E+0x13c, o+0xe60). Addresses, not frame words:
# frame_bytes cannot map them.
WHEEL_TAIL_GARBAGE = (0xE5C, 0xE6C)  # excluded only where run A never reads them (classify)


def wheel_tail_bytes(m):
    """The addresses of WHEEL_TAIL_GARBAGE in each wheel (car+0x470 holds the wheel pointers)."""
    car = struct.unpack("<I", m.read(replay.CAR_PTR, 4))[0]
    wheels = struct.unpack("<4I", m.read(car + 0x470, 16))
    return {w + off + j for w in wheels for off in WHEEL_TAIL_GARBAGE for j in range(4)}


def track_reads(m):
    """Records, in m.reads, the instruction that first reads each byte: the reads classify must see. A copy's load
    (COPY_STORES) moves its bytes and is not a use: its bytes go to m.copied, and each copy to m.copies."""
    m.reads, m.copied, m.copies = {}, set(), set()
    pending, eip = {}, replay.reg_reader(m.mu, UC_X86_REG_EIP)

    def on_read(addr, size, value):
        at = eip()
        if at in COPY_STORES:
            m.copied.update(range(addr, addr + size))
            pending[COPY_STORES[at]] = (at, addr, size)
            return
        for x in range(addr, addr + size):
            m.reads.setdefault(x, at)

    def on_write(addr, size, value):
        load = pending.pop(eip(), None) if pending else None
        if load:
            m.copies.add(load + (addr,))
    replay.raw_hook(m, UC_HOOK_MEM_READ, on_read)
    replay.raw_hook(m, UC_HOOK_MEM_WRITE, on_write)


def track_b_window(m):
    """Run B's reads of the bytes that differ from run A, by original code (its Book). The bindings read memory through
    the bridge (ffi.rs), not through the engine's hooks, so a binding's read of an open byte is not seen; its writes
    close the window (the book's binding write log). The chain sets the open bytes (Book.set_open) to the bytes that
    differ at the end of each step. A read of an open byte records the instruction (b_reads) and closes the byte; a
    write closes it. A copy's load (COPY_STORES) of an open byte opens the bytes its store writes, as in Windows."""
    loads, lp = words_of(COPY_STORES)
    stores, sp = words_of(COPY_STORES.values())
    book(m).hook("b_window", lp, sp, len(loads))


def set_open(m, xs):
    """Run B's open bytes (track_b_window) are now xs."""
    xs, p = words_of(xs)
    m.book.lib.rbr_book_set_open(m.book.ptr, p, len(xs))


def b_reads(m):
    """Run B's reads of open bytes: {byte: the instruction that first read it while open}."""
    xs = m.book.array("b_reads").reshape(-1, 2).tolist()
    return dict(map(tuple, xs))


class Windows:
    """Run A's read windows of the bytes that differ from run B, from run A's logged accesses (RunA). open holds the
    bytes whose window is open: the chain adds the bytes run B wrote in a step before run A's step, and the bytes that
    differ after it; run A's next write of a byte closes its window. reads records the instruction of the first read of
    each byte inside its window (a read after the byte's first difference, and before run A writes it again). A copy's
    load (COPY_STORES) of an open byte is not a use: its store opens the bytes it writes (copies: each copy, and
    copied: the bytes such loads read), and the load's own window stays open."""

    def __init__(self):
        self.open, self.reads, self.copied, self.copies, self.pending = set(), {}, set(), set(), {}

    def step(self, accesses, opened):
        """Run A's step: opened (the bytes run B wrote in it) open first, then run A's accesses apply in order."""
        open_, reads, pending = self.open, self.reads, self.pending
        open_ |= opened
        for addr, kind, eip in accesses.tolist():
            size = kind & 0xFF
            if not kind & WRITE:
                if open_.isdisjoint(range(addr, addr + size)):  # neither a read nor a copy of an open byte
                    continue
                if eip in COPY_STORES:
                    hit = {k for k in range(size) if addr + k in open_}
                    if hit:
                        self.copied.update(addr + k for k in hit)
                        pending[COPY_STORES[eip]] = (eip, addr, size, hit)
                    continue
                for x in range(addr, addr + size):
                    if x in open_:
                        open_.discard(x)
                        reads.setdefault(x, eip)
                continue
            if open_:
                open_.difference_update(range(addr, addr + size))
            load = pending.pop(eip, None) if pending else None
            if load:
                at, src, n, hit = load
                open_ |= {addr + k for k in hit}
                self.copies.add((at, src, n, addr))


def nan_sign_only(ma, mb, mb2, xs):
    """The bytes of xs whose word differs between run A and run B (either fill) only as a default NaN's sign: both
    words NaN, equal apart from bit 31 (x86's invalid operations give 0xffc00000, arm64's 0x7fc00000;
    lead/briefs/rust-readability.md rule 6, notes/rust-physics.md "Platform: default NaN")."""
    word = lambda m, w: struct.unpack("<I", m.read(w, 4))[0]  # noqa: E731
    nan = lambda w: w & 0x7f800000 == 0x7f800000 and w & 0x7fffff  # noqa: E731
    ok = {}
    for w in {x & ~3 for x in xs}:
        a = word(ma, w)
        ok[w] = nan(a) and all(b == a or (nan(b) and b ^ a == 0x80000000) for b in (word(mb, w), word(mb2, w)))
    return {x for x in xs if ok[x & ~3]}


def classify(ma, mb, mb2, cands, snap, b_reads=None, nan_sign=None, stale=frozenset()):
    """Sorts the bytes in cands (where run B differs from run A under either fill) by the exclusion rules: (i) the
    static mask (STATIC_FRAME_WORDS), (ii) a heap pointer in both runs (placement), (iii) a default NaN's sign
    (nan_sign: the bytes that differed only so, at every step they differed; nan_sign_only of the runs' words when
    None), (iv) never read by run A (the reads tracked in ma.reads). A byte that run A or run B reads while it differs
    is a failure (b_reads: run B's reads by byte), unless it is in stale (the bytes whose word differed between run
    A's stale-stack runs at every step they differed): then it is the original's stale-stack noise (STALE). A
    difference a NaN's sign causes elsewhere (code that reads the bits) still fails there. Returns ({class: set of bytes}, [(byte, reader) for the bytes read and not excluded])."""
    b_reads = b_reads or {}
    if nan_sign is None:
        nan_sign = nan_sign_only(ma, mb, mb2, cands)
    heap = range(ma.heap_base, ma.heap_base + replay.HEAP_SIZE)
    pointer = lambda w: w % 4 == 0 and (w in heap or snap.has(w))  # noqa: E731
    word = lambda m, w: struct.unpack("<I", m.read(w, 4))[0]  # noqa: E731
    static, tail = frame_bytes(ma, STATIC_FRAME_WORDS), wheel_tail_bytes(ma)
    classes, failures = collections.defaultdict(set), []
    for x in sorted(cands):
        w = x & ~3
        if x in static:
            classes["static"].add(x)
        elif pointer(word(ma, w)) and pointer(word(mb, w)):
            classes["pointer"].add(x)
        elif x in nan_sign:
            classes["default-NaN sign"].add(x)
        elif (x in ma.reads or x in b_reads) and x in stale:
            classes[STALE].add(x)
        elif x in ma.reads or x in b_reads:
            failures.append((x, ma.reads.get(x, b_reads.get(x))))
        elif x in ma.copied:
            classes["copied, not used"].add(x)
        elif x in tail:
            classes["wheel tail, never read"].add(x)
        elif word(mb, w) != word(mb2, w):
            classes["fill-dependent, never read"].add(x)
        else:
            classes["never read"].add(x)
    return classes, failures


def exclusion_summary(m, classes, last, listing, copies=()):
    """The bytes a comparison leaves out, by class, for the summary line; with listing, each class's ranges and the
    copies (run A's, copies) that carry the copied bytes."""
    if not classes:
        return ""
    text = "excluded, not compared: " + ", ".join("%s %d bytes%s" % (c, len(s), stale_breakdown(m, s) if c == STALE
                                                                    else "") for c, s in sorted(classes.items()))
    if listing:
        text += "".join("\n  %s:%s" % (c, excluded(m, s, last)) for c, s in sorted(classes.items()))
        copied = classes.get("copied, not used", set())
        chains = sorted({c for c in copies if any(x in copied for x in range(c[1], c[1] + c[2]))})
        text += "".join("\n  copy by %#x: %#x..%#x -> %#x..%#x" % (eip, src, src + n, dst, dst + n)
                        for eip, src, n, dst in chains)
    return text


def failure_lines(m, mb, mb2, failures, last, last2):
    """The failures grouped by (reader, run B's last writer under each fill, class), one line a group: how many bytes,
    the first and last address, and the class (residue when the fills disagree on the byte, else a port difference:
    the same value under both fills)."""
    groups = collections.OrderedDict()
    for x, reader in sorted(failures):
        w = x & ~3
        fill_dependent = struct.unpack("<I", mb.read(w, 4)) != struct.unpack("<I", mb2.read(w, 4))
        key = (reader, last.get(x), last2.get(x), "residue (fill-dependent)" if fill_dependent else
               "port difference (same under both fills)")
        groups.setdefault(key, []).append(x)
    return "".join("\n    read by %#x; run B's last writer %s (fill 0), %s (fill 7f): %d bytes, %#x..%#x (%s), %s" % (
        reader, "%#x" % w0 if w0 is not None else "none", "%#x" % w7 if w7 is not None else "none", len(xs), xs[0],
        xs[-1], replay.where(m, xs[0]), cls) for (reader, w0, w7, cls), xs in groups.items())


def last_writers(m):
    """Moves m's logged writes (its Book's, install) into its last writers, and returns them: {address: the
    instruction (or binding) that last wrote it} (LastWriters)."""
    m.book.lib.rbr_book_commit(m.book.ptr)
    return LastWriters(m.book)


def excluded(m, xs, last):
    """--excluded: every range of the addresses xs, what it is and the instructions in last that wrote it."""
    runs = []
    for x in sorted(xs):
        if runs and x == runs[-1][1]:
            runs[-1][1] = x + 1
        else:
            runs.append([x, x + 1])
    return "".join("\n    %#x..%#x (%s, %d bytes)%s" % (
        lo, hi, replay.where(m, lo), hi - lo, "" if not last else ", last written at %s" % " ".join(
            "%#x" % e for e in sorted({last[x] for x in range(lo, hi) if x in last}))) for lo, hi in runs)


def report_calls(m):
    calls = ", ".join("%#x x%d" % kv for kv in sorted(m.rust.items())) or "none"
    declined = "; ".join("%#x %s x%d" % (a, why, n) for a, d in sorted(m.declined.items()) for why, n in d.items())
    return "bindings ran %s%s" % (calls, "; declined " + declined if declined else "")


def one_step(path, a, hybrid, o, groups):
    """Run A, then one run B per group of bound entries (None: all, or the --only ones)."""
    snap = replay.Snapshot(path)
    ma = machine(snap, o)
    record = not a.full_fill
    if record:
        record_calls(ma, at_snapshot(hybrid.bound, ngp_base(snap)))
    track_reads(ma)
    if finish(ma):
        print("%s: run A: %s" % (path, ma.stop))
        return False
    log = ma.take_calls(True) if record else None
    used = o.known.used
    sms = [stale_machine(snap, o, fill) for fill in FILLS]
    stale = set() if any(map(finish, sms)) else set(stale_words(sms, snap).tolist())
    timed = replay.stub_dependent(ma)
    stack = replay.stack_below(snap)
    teb = range(snap.teb, snap.teb + 4)
    ok = True
    kept = lambda xs: [x for x in xs if x not in stack and x not in timed and x not in teb]  # noqa: E731
    for only in groups:
        mb, mb2 = (machine(snap, o, hybrid, only, a.perturb, fill, a.excluded, log)
                   for fill in FILLS)
        stop = finish(mb) or finish(mb2)
        head = "%s (step %d): %s" % (path, snap.step, report_calls(mb))
        if stop:
            print("%s; run B: %s" % (head, stop))
            ok = False
            continue
        if only and not mb.rust and not mb.declined:
            print("%s (step %d): %#x not reached" % (path, snap.step, only[0]))
            continue
        cands = set(kept(replay.differing(ma, mb))) | set(kept(replay.differing(ma, mb2)))
        classes, failures = classify(ma, mb, mb2, cands, snap, stale=stale_bytes(cands, stale))
        verdict = "identical to the original's step" if not failures else \
            "%d bytes differ from the original's step and are not excluded" % len(failures)
        modelled = o.known.used
        last = last_writers(mb)
        summary = exclusion_summary(mb, classes, last, a.excluded, ma.copies)
        print("%s; %s (%d stub-dependent bytes ignored)%s%s%s" % (
            head, verdict, len(timed), "; " + summary if summary else "",
            "; UNVERIFIED: %d modelled x87 results (%d in run A)" % (modelled, used) if modelled else "",
            stale_warning(mb, classes, last)))
        if record:
            print("  " + fill_summary(mb))
        for x, reader in failures[:12]:
            print("  %#x (%s): A %02x B %02x, read by %#x" % (x, replay.where(ma, x), ma.read(x, 1)[0], mb.read(x, 1)[0], reader))
        ok &= not failures
        if a.recording:
            fb, fb2 = words(mb.frame()), words(mb2.frame())
            post, n, noise, own, bad = against_recording(
                a.recording, snap, ma, mb, {i for i in range(len(fb)) if fb[i] != fb2[i]})
            print("  vs %s frame %d: run B %s; run A differs at %d words (%s); %d of %d words are run noise" % (
                os.path.basename(a.recording), post, "matches where run A does" if not bad else
                "differs at %d more words, first frame offset %#x" % (len(bad), 4 * bad[0]), len(own),
                " ".join("%#x" % (4 * i) for i in own[:6]), noise, n))
            ok &= not bad
    return ok


def recorded_controls(path):
    """{pre step: control bytes} of a recording."""
    r = replay.recording(path)
    return {pre: r.data[at:at + replay.CONTROL_BYTES] for pre, (at, _, _) in r.frames.items()}


def call_from(m, snap, start, ecx, args):
    """Runs `start` with ecx and stack args from the snapshot's call slot, to the sentinel."""
    slot = snap.regs["esp"] - 4
    m.mu.mem_write(slot, struct.pack("<%dI" % (len(args) + 1), replay.SENTINEL, *args))
    m.mu.reg_write(UC_X86_REG_ECX, ecx)
    m.mu.reg_write(UC_X86_REG_ESP, slot)
    m.start = start
    stop = finish(m)
    m.start = replay.STEP_FORWARD
    return stop


def record_calls(m, addrs):
    """Run A (the original only): each call of an address in addrs logs the stack bytes it writes below its entry
    (within STACK_FILL), with the step (Book.step), its address and entry esp: one entry (runs of consecutive
    addresses) per call in the order they return (its Book: a write hook, a code hook at each address; a call ends
    when esp rises above its entry).
    m.take_calls(close) gives the log so far and empties it, flat (step, address, entry esp, number of runs, the runs as
    start and end), for install's `record`; with close it first ends the calls still open (a step's end)."""
    b = book(m)
    xs, p = words_of(addrs)
    b.hook("calls", p, len(xs))
    m.take_calls = lambda close: b.array("take_calls", int(close))


def track_own_writes(m, addrs):
    """Run B: the stack bytes original code writes below the entry of each open call of an address in addrs (within
    STACK_FILL), the nested runs of its binding included (its Book: a block hook at each address, a write hook on the
    stack). The call fill, when the binding returns, ends that call and leaves them (they are what the original's own
    code left there, not the binding's stack). A call whose binding declined ends when the stack rises above its
    entry."""
    xs, p = words_of(addrs)
    book(m).hook("own_writes", p, len(xs), m.stack_floor, m.snap.stack_lo + len(m.snap.stack) - 1)


def fill_summary(m):
    """The fill of run B's calls: aligned with a logged call of run A (its own writes below the entry), or fallback
    (the whole free stack, for a call run A did not make at that entry)."""
    stats = (ctypes.c_uint64 * 4)()
    m.book.lib.rbr_book_fill_stats(m.book.ptr, stats)
    return "call fill: %d aligned, %d fallback (%d bytes filled, %d left as run B's original code wrote them)" % tuple(
        stats)


def schedule(path, snap):
    """The recorded calls from the snapshot's on, as (pre step, substeps): one substep each, but snap-0's recording
    has its first calls at 40 (0, 40, 80, 120, then 160, 161, ...)."""
    r = replay.recording(path)
    return [(pre, r.frames[pre][2] - pre) for pre in sorted(p for p in r.frames if p >= snap.step)]


def step_of(snap, k):
    """The recording's step of a chain's step k (snap.schedule, set by chain), or None past the recording."""
    return snap.schedule[k][0] if k < len(snap.schedule) else None


def advance(m, snap, k, control):
    """Step k of a chain: step 0 is the snapshot's call; later steps Emit their control, then StepForward with the
    recorded call's substeps (1 after snap-0's first calls)."""
    if not k:
        return finish(m)
    scratch = snap.stack_lo  # the far end of the dumped stack: no frame of the step reaches it
    m.mu.mem_write(scratch, control)
    emit = struct.unpack("<I", m.read(EMIT_OWNER, 4))[0] + EMIT_OFFSET
    return call_from(m, snap, EMIT, emit, [scratch]) or call_from(
        m, snap, replay.STEP_FORWARD, snap.regs["ecx"], [snap.schedule[k][1]])


def fill_stale_reads(m, fill):
    """A stale-stack run (stale_machine): each read of a dead stack byte not written since gets `fill` (one byte) as it
    is read. The stack below esp goes dead at a step's entry (Emit, StepForward), and a byte goes dead when esp rises
    above it (sampled at block starts). Its Book: a block hook, a write and a read hook on the dumped stack."""
    book(m).hook("stale", m.snap.stack_lo, len(m.snap.stack), m.stack_floor, EMIT, replay.STEP_FORWARD, fill[0])


def stale_machine(snap, o, fill):
    """Run A's code with its reads of dead stack filled with fill's first byte (fill_stale_reads)."""
    m = machine(snap, Uncounted(o))
    m.unverified_ok = True
    fill_stale_reads(m, fill[:1])
    return m


def stale_words(ms, snap):
    """The words where the stale-stack runs ms differ (the stack below the call left out), as a sorted array."""
    stack = replay.stack_below(snap)
    return np.unique(np.array([x & ~3 for x in replay.differing(*ms) if x not in stack], np.uint32))


def stale_bytes(xs, words):
    """The bytes of xs in the words `words` (stale_words)."""
    return {x for x in xs if x & ~3 in words}


# The objects whose vectors object_base can place, by vtable: (size, what). Their vectors are 16 bytes from the start.
LANE_OBJECTS = {0x73FC54: (0x3D0, "CSolid"), 0x73D23C: (0x660, "CHingedRigidBody"),
                0x73E62C: (0x590, "CMountedRigidBody"), 0x739978: (0xC424, "E38 solver: contact records")}


def object_base(m, x):
    """The start of the object that holds x, for its vector lanes: the car, a wheel, or an object of LANE_OBJECTS (its
    vtable at the start, the nearest below x that reaches it); None when none is found. The objects are 8- or
    16-aligned, so the address alone does not give the lane."""
    word = lambda at: struct.unpack("<I", m.read(at, 4))[0]  # noqa: E731
    car = word(replay.CAR_PTR)
    if car:
        for b, n in [(car, replay.CAR_BYTES)] + [(w, 0x1428) for w in struct.unpack("<4I", m.read(car + 0x470, 16))]:
            if b <= x < b + n:
                return b
    x &= ~3
    lo = x - max(n for n, _ in LANE_OBJECTS.values())
    while True:
        try:
            data = np.frombuffer(m.read(lo, x + 4 - lo), "<u4")
            break
        except Exception:  # unmapped: from the page of x
            if lo >= x & ~0xFFF:
                return None
            lo = x & ~0xFFF
    for i in np.flatnonzero(np.isin(data, list(LANE_OBJECTS)))[::-1]:
        b = lo + 4 * int(i)
        if x < b + LANE_OBJECTS[int(data[i])][0]:
            return b
    return None


def stale_lanes(m, xs):
    """The bytes xs by lane: ({"lane 3", "not lane 3", "layout unknown"}: bytes); lane 3 is offset 0xc..0xf mod 16
    from the object (object_base)."""
    out, bases = collections.defaultdict(set), {}
    for x in xs:
        w = x & ~3
        if w not in bases:
            bases[w] = object_base(m, w)
        b = bases[w]
        out["layout unknown" if b is None else "lane 3" if (x - b) & 0xF >= 0xC else "not lane 3"].add(x)
    return out


def stale_breakdown(m, xs):
    """The stale-stack class's bytes by region (replay.where) and by lane (stale_lanes), for the summary line."""
    regions = collections.Counter(replay.where(m, x).split("+")[0].rstrip("0123456789") for x in xs)
    lanes = stale_lanes(m, xs)
    return " (%s; %s)" % (", ".join("%s %d" % kv for kv in sorted(regions.items())), ", ".join(
        "%s %d" % (k, len(lanes[k])) for k in ("lane 3", "not lane 3", "layout unknown")))


def stale_warning(m, classes, last):
    """The bytes of the stale-stack class outside lane 3, as a WARNING line (values the original's physics computes
    from stack it never wrote), and those whose lane is not known, listed."""
    lanes = stale_lanes(m, classes.get(STALE, ()))
    off, unknown = lanes["not lane 3"], lanes["layout unknown"]
    return ("\n  WARNING: %d bytes of the original's stale-stack noise are not in lane 3:%s" % (
        len(off), excluded(m, off, last)) if off else "") + (
        "\n  the original's stale-stack noise, lane not known (no object found): %d bytes:%s" % (
            len(unknown), excluded(m, unknown, last)) if unknown else "")


# Run A's cache (RunA): a chain's run A depends only on the snapshot, the recording's controls, the step count, the
# harness files that define it and the exe, so its record is kept under CACHE (RBR_STEP_CACHE; "" turns it off), one
# file per chain. The x87 results it used are checked against the oracle when it is loaded.
CACHE = os.environ.get("RBR_STEP_CACHE", os.path.join(os.path.dirname(replay.REPO), "cache", "run-a"))
CACHE_VERSION = 3
WRITE = 0x100  # a logged access: (address, size | WRITE for a write, eip)
STORES = sorted(set(COPY_STORES.values()))


def record_accesses(m):
    """Logs the accesses of run A that can decide a read window (Windows): m.take_accesses() gives them since the
    last call, as rows of address, size (| WRITE for a write), eip, and starts the next step. A byte's window can open
    only at a step's start (the closed bytes are emptied) and at a copy's store (a write by a COPY_STORES store, logged
    always); after the first access of the byte that is not a copy's load it is closed until such a store, so the
    accesses of closed bytes change nothing and are left out. Its Book: a read and a write hook, a code hook at each
    store."""
    b = book(m)
    loads, lp = words_of(COPY_STORES)
    stores, sp = words_of(STORES)
    b.hook("accesses", lp, len(loads), sp, len(stores))
    m.take_accesses = lambda: b.array("take_accesses").reshape(-1, 3)


class Queries:
    """An oracle that logs the x87 results it hands out (run A's, for the cache): seen {key: the cached result or
    None}, and per step, the modelled results (used, modelled) as Modelled counts them."""

    def __init__(self, o):
        self.o, self.known, self.seen, self.used, self.modelled = o, self, {}, 0, set()

    def get(self, key, default=None):
        raw = self.seen[key] = dict.get(self.o.known, key)
        res = self.o.known.get(key, default)
        if raw is None and res is not None:
            self.used += 1
            self.modelled.add(key)
        return res


def file_hash(h, path):
    with open(path, "rb") as f:
        while True:
            block = f.read(1 << 20)
            if not block:
                return
            h.update(block)


def cache_file(path, snap, steps, controls, bound):
    """The cache file of a chain: named by a hash of what run A depends on."""
    h = hashlib.blake2b(digest_size=16)
    h.update(repr((CACHE_VERSION, book_lib().rbr_book_version(), steps, bound, unicorn.__version__, SKIPS, STACK_FILL,
                   COPY_STORES)).encode())
    for k in range(1, steps):
        h.update(controls.get(step_of(snap, k), b"-"))
    file_hash(h, path)
    file_hash(h, replay.__file__)
    if os.path.exists(replay.ORIG_EXE):
        file_hash(h, replay.ORIG_EXE)
    for fn in (machine, finish, advance, call_from, record_calls, record_accesses, Queries, RunA,
               fill_stale_reads, stale_machine, stale_words, Uncounted, model):
        h.update(inspect.getsource(fn).encode())
    return os.path.join(CACHE, "%s.s%d.%s.bin" % (os.path.basename(path), steps, h.hexdigest()))


class RunA:
    """Run A of a chain (the original), step by step: from the cache when it holds the chain, else from an emulator
    (whose record is cached once complete) and its two stale-stack runs (stale_machine, one per FILLS). step() returns
    a step's record: the stop reason, the accesses that can decide a read window (record_accesses), the call log
    (record_calls; None with --full-fill), the x87 results and sound skips, and the words the stale-stack runs differ
    at after the step (stale_words; None once one of them has stopped, the reason in stale_stop). self.m holds run
    A's memory after the last step: the emulator, or a machine the cached changes are applied to; self.missing the x87
    results missing from the oracle so far. Records are kept only to be cached."""

    def __init__(self, path, snap, a, hybrid, o, controls, cache):
        self.snap, self.o, self.steps, self.controls, self.live, self.missing = snap, o, a.steps, controls, None, set()
        self.file = cache and CACHE and cache_file(path, snap, a.steps, controls, None if a.full_fill else hybrid.bound)
        self.records = self.load()
        if self.records is not None:
            self.m = replay.Machine(snap, log_writes=False, oracle=o)
            self.mem = [np.frombuffer(mem, np.uint8) for _, mem in self.m.views]
            return
        self.records, m = [], machine(snap, o)
        m.oracle = self.queries = Queries(o)
        record_accesses(m)
        if not a.full_fill:
            record_calls(m, at_snapshot(hybrid.bound, ngp_base(snap)))
        self.m = self.live = m
        self.stale, self.stale_stop = [stale_machine(snap, o, fill) for fill in FILLS], None
        if self.file:
            self.prev = [np.frombuffer(mem, np.uint8).copy() for _, mem in m.views]

    def load(self):
        """The cached records, or None (none cached, or an x87 result run A used has changed in the oracle)."""
        if not self.file or not os.path.exists(self.file):
            return None
        with open(self.file, "rb") as f:
            seen, records = pickle.loads(zlib.decompress(f.read()))
        known = self.o.known
        return records if all(dict.get(known, k) == v for k, v in seen.items()) else None

    def step(self, k):
        if self.live is None:
            rec = self.records[k]
            for mem, (idx, data) in zip(self.mem, rec["changes"]):
                mem[idx] = data
            self.o.known.used += rec["used"]
            self.o.known.modelled |= rec["modelled"]
            self.missing |= rec["missing"]
            return rec
        m, q = self.live, self.queries
        q.used, q.modelled = 0, set()
        m.book.step(k)
        control = self.controls.get(step_of(self.snap, k)) if k else b""
        stop = advance(m, self.snap, k, control)
        stale = None
        if self.stale:
            for fill, sm in zip(FILLS, self.stale):
                why = advance(sm, self.snap, k, control)
                if why:
                    self.stale, self.stale_stop = None, "step %d, fill %02x: %s" % (step_of(self.snap, k), fill[0], why)
                    break
            else:
                stale = stale_words(self.stale, self.snap)
        calls = m.take_calls(not stop) if hasattr(m, "take_calls") else None
        rec = {"stop": stop, "accesses": m.take_accesses(), "calls": calls,
               "used": q.used, "modelled": q.modelled, "missing": set(m.fpu_missing), "skipped": m.skipped,
               "stale": stale, "stale_stop": self.stale_stop}
        self.missing |= rec["missing"]
        if self.file:
            rec["changes"] = list(map(changes, self.prev, (mem for _, mem in m.views)))
            self.records.append(rec)
        return rec

    def save(self):
        """Caches the records of a live run: the remaining steps are run first (a chain can stop early in run B)."""
        if self.live is None or not self.file:
            return
        known = self.o.known
        counts = known.used, set(known.modelled)  # the chain's: the steps run here are not part of it
        done = lambda: len(self.records) == self.steps or self.records and self.records[-1]["stop"]  # noqa: E731
        while not done() and step_of(self.snap, len(self.records)) in self.controls:
            self.step(len(self.records))
        known.used, known.modelled = counts
        if not done():
            return
        os.makedirs(CACHE, exist_ok=True)
        data = zlib.compress(pickle.dumps((self.queries.seen, self.records), protocol=5), 6)
        with open(self.file + ".tmp", "wb") as f:
            f.write(data)
        os.replace(self.file + ".tmp", self.file)


def changes(prev, mem):
    """The bytes of a writable view that changed since prev, as (offsets, bytes); prev is brought up to date."""
    cur = np.frombuffer(mem, np.uint8)
    words = np.flatnonzero(np.frombuffer(cur, np.uint64) != np.frombuffer(prev, np.uint64))
    idx = (8 * words[:, None] + np.arange(8)).ravel()
    idx = idx[cur[idx] != prev[idx]].astype(np.uint32)
    prev[idx] = cur[idx]
    return idx, cur[idx]


_static = {}


def recording_static(path):
    """diverge.py's static mask of a recording (frame words, control first), computed once per recording."""
    if path not in _static:
        ref = diverge.load(path)
        _static[path] = diverge.static_mask(ref, ref[1], [])
    return _static[path]


def count_entries(m, count):
    """Counts the original's functions (data/rbr.functions.csv) machine m enters, the nested runs of the bindings
    included (a bound entry is counted before its binding takes over), by its Book; counted_entries adds them to count
    {addr: entries}."""
    with open(os.path.join(replay.REPO, "data", "rbr.functions.csv")) as f:
        starts = {int(line.split("|")[0], 16) for line in f.read().split("\n")[1:] if line}
    xs, p = words_of(starts)
    book(m).hook("count_entries", p, len(xs), min(starts), max(starts))
    m.count_into = count


def counted_entries(m):
    """Adds the entries count_entries counted in m to its counter."""
    rows = m.book.array("entries").reshape(-1, 3).astype(np.uint64)
    m.count_into.update({int(a): int(lo | hi << 32) for a, lo, hi in rows})


class Frozen:
    """A run's words as held at a step's end ({word address: 4 bytes}); other reads, and attributes, go to the run
    `rest`."""

    def __init__(self, held, rest):
        self.held, self.rest = held, rest

    def __getattr__(self, name):
        return getattr(self.rest, name)

    def read(self, addr, n):
        return self.held[addr] if n == 4 and addr in self.held else self.rest.read(addr, n)


def stopped_lines(m, first):
    """For a chain that stopped: the bytes that began to differ in the last three steps where any did (first {byte:
    step}), the likely start of what stopped it."""
    by_step = collections.defaultdict(list)
    for x, step in first.items():
        by_step[step].append(x)
    return "".join("\n    began to differ at step %d: %d bytes, %s" % (step, len(by_step[step]), ", ".join(
        "%#x (%s)" % (x, replay.where(m, x)) for x in sorted(by_step[step])[:6])) for step in sorted(by_step)[-3:])


def chain(path, a, hybrid, o, count=None):
    """--steps: N steps of runs A and B in lockstep from the snapshot. B against A: after every step, the bytes that
    differ under either fill open a window in run A (Windows); at the end, each byte that run A reads in its
    window is a failure, printed with the instruction that reads it and run B's writers, and the step where the first
    of them began to differ. A against the recording after every step. Run A (RunA, from the cache when it holds the
    chain) runs each step before run B, for its call log (the call fill); its accesses apply to the windows after run
    B's step. a.cache (rust_step.py's main sets it) turns the cache on. With count (a Counter),
    the original's functions run B (fill 0) enters are counted in it (harness/long_run.py's worklist). A stop in run B
    (a fault) still classifies the steps before it.
    Returns {"ok", "steps", "complete", "compared", "failures", "first_b": (step, byte, reader) or None, "halt": (step,
    why) or None, "a_bad": (frame, words) or None, "unverified", "declined", "fpu_missing"}."""
    snap = replay.Snapshot(path)
    snap.schedule = schedule(a.recording, snap)
    controls = recorded_controls(a.recording)
    static = recording_static(a.recording)
    cw = replay.CONTROL_BYTES // 4
    run_a = RunA(path, snap, a, hybrid, o, controls, getattr(a, "cache", False))
    ma, windows = run_a.m, Windows()
    record = None if a.full_fill else ()
    mb, mb2 = (machine(snap, o, hybrid, a.only, a.perturb, fill, True, record) for fill in FILLS)
    track_b_window(mb)
    track_b_window(mb2)
    if count is not None:
        count_entries(mb, count)
    last, last2 = LastWriters(mb.book), LastWriters(mb2.book)
    # snap-0 has no recorded frame before it: no run noise is skipped there
    before = replay.recording_frame(a.recording, snap.step - 1)[0] if snap.step else None
    pre_words = words(ma.frame())
    skip = {i for i, w in enumerate(words(before) if before else ()) if w != pre_words[i]} | {
        i - cw for i in range(cw, len(static)) if static[i]} | set(STACK_GARBAGE) | set(STATIC_FRAME_WORDS) | set(
        RECORDING_GARBAGE)
    heap = range(ma.heap_base, ma.heap_base + replay.HEAP_SIZE)
    pointer = lambda w: w % 4 == 0 and (w in heap or snap.has(w))  # noqa: E731
    recorded_pointer = lambda w: w % 4 == 0 and 0x10000 <= w < 0x80000000  # noqa: E731
    stack = replay.stack_below(snap)
    teb = range(snap.teb, snap.teb + 4)
    kept = lambda xs: {x for x in xs if x not in stack and x not in teb}  # noqa: E731
    a_bad, skipped = None, 0
    a_words, ever, first, not_nan_sign, not_stale, stale_ever, stale_stop = set(), set(), {}, set(), set(), set(), None
    pointers, unverified, steps, t0, halt = set(), [], 0, time.time(), None
    a_steps, a_self = {}, {}  # frame word -> the steps run A differs from the recording there, and the stale-stack ones
    for k in range(a.steps):
        step = step_of(snap, k)
        control = controls.get(step) if k else b""
        if control is None:
            halt = (step, "no recorded control")
            print("%s: no recorded control for step %d" % (path, step))
            break
        used = o.known.used
        rec = run_a.step(k)
        for m in (mb, mb2):
            if record is not None:
                m.book.set_record(rec["calls"])  # this step's calls (none once run A stops: the whole free stack)
            m.book.step(k)
        stop = advance(mb, snap, k, control) or advance(mb2, snap, k, control)
        if stop:
            halt = (step, "run B: " + stop)
            print("%s: run B, step %d: %s" % (path, step, stop))
            break
        # The bytes run B wrote in this step open their windows before run A runs it: a difference can arise at
        # such a write, and a read of the byte in run A after it is in the window (run A's own write closes it).
        skipped = rec["skipped"]
        if rec["stop"]:
            halt = (step, "run A: " + rec["stop"])
            print("%s: run A, step %d: %s" % (path, step, rec["stop"]))
            break
        windows.step(rec["accesses"], set(mb.book.written().tolist()) | set(mb2.book.written().tolist()))
        last_writers(mb)
        last_writers(mb2)
        steps += 1
        if o.known.used > used:
            unverified.append(step)
        d1, d2 = kept(replay.differing(ma, mb)), kept(replay.differing(ma, mb2))
        # Run B runs first in a step, so at its reads run A still holds the values of the step's end: the bytes open
        # for run B are the ones that differ now.
        set_open(mb, d1)
        set_open(mb2, d2)
        differs = d1 | d2
        windows.open |= differs
        for x in differs - ever:
            first[x] = step
        ever |= differs
        not_nan_sign |= differs - nan_sign_only(ma, mb, mb2, differs)
        stale = set(rec["stale"].tolist()) if rec["stale"] is not None else set()
        stale_ever |= stale
        stale_stop = rec["stale_stop"]
        not_stale |= differs - stale_bytes(differs, stale)
        # The words around the bytes that ever differed, as at this step's end: a stop in run B's next step leaves
        # the runs mid-step (run A a step ahead), and the bytes are classified with these instead (Frozen).
        held = [{w: m.read(w, 4) for w in {x & ~3 for x in ever}} for m in (ma, mb, mb2)]
        fa = words(ma.frame())
        ref, post = replay.recording_frame(a.recording, step)
        want = words(ref)
        bad = []
        for i in range(len(want)):
            if fa[i] == want[i] or i in skip:
                continue
            if pointer(fa[i]) and recorded_pointer(want[i]):
                pointers.add(i)
            else:
                bad.append(i)
        a_words.update(bad)
        for i in bad:
            a_steps.setdefault(i, []).append(post)
        car = struct.unpack("<I", ma.read(replay.CAR_PTR, 4))[0]
        for x in stale:
            if car <= x < car + replay.CAR_BYTES:
                a_self.setdefault((x - car) // 4, []).append(post)
        if bad and a_bad is None:
            a_bad = (post, bad)
        if getattr(a, "progress", 0) and steps % a.progress == 0:
            print("%s: step %d, %d steps in %.0f s, run B %s, run A %s" % (
                os.path.basename(path), step, steps, time.time() - t0, "differs at %d bytes" % len(differs) if differs
                else "equal", "bit-exact" if a_bad is None else "diverged at frame %d" % a_bad[0]), file=sys.stderr,
                flush=True)
    a_missing = set(run_a.missing)
    run_a.save()
    if count is not None:
        counted_entries(mb)
    ma.reads, ma.copied, ma.copies = windows.reads, windows.copied, windows.copies
    offsets = lambda ws: " ".join("%#x" % (4 * i) for i in ws[:6])  # noqa: E731
    complete = steps == a.steps
    b_reads_ = {**b_reads(mb), **b_reads(mb2)}
    # A stop in run B leaves the windows at the last full step's end: its bytes are still classified, with the runs'
    # words as they were then. A stop in run A leaves nothing to compare.
    compared = complete or bool(halt and not halt[1].startswith("run A") and steps)
    fa_, fb, fb2 = (ma, mb, mb2) if complete or not compared else (Frozen(h, m) for h, m in zip(held, (ma, mb, mb2)))
    classes, failures = classify(fa_, fb, fb2, ever, snap, b_reads_, ever - not_nan_sign, ever - not_stale) \
        if compared else ({}, [])
    first_b = min(((first[x], x, reader) for x, reader in failures), default=None)
    print("%s: %d steps from %d, %s%s" % (path, steps, snap.step, report_calls(mb),
                                         "; sound skips %d" % skipped if skipped else ""))
    summary = exclusion_summary(ma, classes, last, a.excluded, ma.copies)
    if summary:
        print("  %s%s" % (summary, stale_warning(ma, classes, last)))
    lanes = stale_lanes(ma, stale_ever)
    print("  the original's stale-stack runs differ at %d words over the chain (%s)%s" % (
        len(stale_ever), ", ".join("%s %d" % (k, len(lanes[k])) for k in ("lane 3", "not lane 3", "layout unknown")),
        "; they stopped at %s (later steps have no such exclusion)" % stale_stop if stale_stop else ""))
    if record is not None:
        print("  " + fill_summary(mb))
    print("  run B vs run A%s: %s%s" % (
        "" if complete else " up to step %d (stopped at step %d)" % (halt[0] - 1, halt[0]) if compared else
        " (stopped at step %d)" % halt[0],
        "not compared" if not compared else "equal apart from the excluded bytes" if not failures else
        "%d bytes differ and are read by run A or run B in their window, the first from step %d (%#x, %s): %s" % (
            len(failures), first_b[0], first_b[1], replay.where(ma, first_b[1]),
            failure_lines(ma, fb, fb2, failures, last, last2)), "" if complete else stopped_lines(ma, first)))
    print("  run A vs %s: %s; %d pointer words differ (allocator placement, not compared)%s" % (
        os.path.basename(a.recording), "not compared (no step completed)" if not steps else "bit-exact" if a_bad is None
        else "diverges at frame %d: %d words, "
        "first frame offsets %s; %d words differ in some step (%s)" % (
            a_bad[0], len(a_bad[1]), offsets(a_bad[1]), len(a_words), offsets(sorted(a_words))), len(pointers),
        "; UNVERIFIED: %d steps used modelled x87 results (first %d)" % (len(unverified), unverified[0])
        if unverified else ""))
    # Per word: run A's differences from the recording, and the steps where the stale-stack runs differ there (the
    # original's own residue: the evidence for a stale-stack word).
    for i in sorted(set(a_steps) | set(a_self)):
        name = "car+%#x" % (4 * i) if 4 * i < replay.CAR_BYTES else "wheel%d+%#x" % divmod(
            4 * i - replay.CAR_BYTES, replay.WHEEL_BYTES)
        print("  run A vs %s, %s: differs from the recording at steps %s; the stale-stack runs differ there at steps %s" % (
            os.path.basename(a.recording), name, ",".join(map(str, a_steps.get(i, []))) or "none",
            ",".join(map(str, a_self.get(i, []))) or "none"))
    return {"ok": complete and not failures and a_bad is None, "steps": steps, "complete": complete,
            "compared": compared, "failures": len(failures), "first_b": first_b, "halt": halt,
            "a_bad": a_bad and (a_bad[0], len(a_bad[1])),
            "unverified": len(unverified), "declined": dict(mb.declined),
            "fpu_missing": set().union(mb.fpu_missing, mb2.fpu_missing, a_missing)}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshots", nargs="+")
    ap.add_argument("--recording")
    ap.add_argument("--only", action="append", type=lambda s: int(s, 16), metavar="ADDR")
    ap.add_argument("--steps", type=int)
    ap.add_argument("--each", action="store_true", help="one run B per bound entry (or --only entry), alone")
    ap.add_argument("--perturb", type=float, metavar="DELTA")
    ap.add_argument("--excluded", action="store_true", help="list every byte range not compared for the stack")
    ap.add_argument("--no-cache", action="store_true", help="--steps: run run A, not from its cache (RBR_STEP_CACHE)")
    ap.add_argument("--full-fill", action="store_true", help="fill each binding's free stack whole, not only the bytes "
                    "run A wrote below the call's entry (the call fill, the default)")
    a = ap.parse_args()
    a.cache = not a.no_cache
    if a.steps and not a.recording:
        ap.error("--steps needs --recording")
    hybrid, ok = Hybrid(), True
    if a.only:
        unknown = [x for x in a.only if x not in hybrid.bound]
        if unknown:
            ap.error("not bound: %s" % " ".join("%#x" % x for x in unknown))
    groups = [[x] for x in a.only or hybrid.bound] if a.each else [a.only]
    o = oracle()
    for path in a.snapshots:
        # With RBR_REMOTE the modelled x87 results are fetched from cachy (replay.py's oracle) and the runs
        # repeated until none is modelled; only the last round's report is printed.
        for _ in range(8):
            o.known.used, o.known.modelled = 0, set()
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                if a.steps:
                    res = chain(path, a, hybrid, o)
                    good, missing = res["ok"], res["fpu_missing"]
                else:
                    good, missing = one_step(path, a, hybrid, o, groups), set()
            # the results the cache lacks are fetched as well as the modelled ones
            if not (o.known.modelled or missing) or not replay.REMOTE:
                break
            o.fetch(o.known.modelled | missing)
        print(out.getvalue(), end="")
        ok &= good
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

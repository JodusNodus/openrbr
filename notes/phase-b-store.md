# Phase B storage seam (B0)

Plan: `lead/reports/phase-b-plan.md` (one physics code, two storages). B0 adds the seam without changing behaviour or
speed; this note says where its parts are and what is left.

## Store traits

A subsystem's store trait lists its layout's load and store groups, with the same names. `Mem` implements each one
with the layout's bodies, moved unchanged (`let mem = self;` starts each body). The sequence functions in the layouts
are generic over the trait (`fn step<S: SpringStore>(mem: &mut S, ...)`), never `dyn`, so a Mem caller compiles to
the code it had before.

| Trait | Declared in | Mem impl | Groups |
| --- | --- | --- | --- |
| `store::Raw` | `rbr-physics/src/store/mod.rs` | same file | word access (`u32`, `f32`, `set_*`, `u32s`, ...) and `mem()`; the supertrait of every store |
| `SpringStore` | `spring_damper/store.rs` | `spring_damper/layout.rs` | controller, state, definition, setup, fade, the damper kernels' blocks |
| `VcuStore` | `vcu/store.rs` | `vcu/layout.rs` | scratch, def, state, gearbox, drive outputs and locks, packet, knot maps |
| `TyreStore` | `tyre/store.rs` | `tyre/layout/kernels.rs` | the step frame, the kernels' loads and stores, segments and tables, material |
| `WheelStore` | `wheel/store.rs` | `wheel/layout/fields.rs` | the wheel's, CDef's, tyre block's and tail's words, the axis and lerp blocks, the pose refresh's CDef |
| `CarStore` | `car_body/store.rs` | `car_body/layout/car.rs` | `CCar`'s getters and setters, its parts and wheel slots, the body points, the world's timer words |
| `EngineStore: SpringStore` | `engine_driveline/store.rs` | `engine_driveline/layout/mem_store.rs` | the engine's and drive's records (`load_*`/`store_*`), curve and blend tables, drive-line entries |

Generic sequence functions so far: the spring-damper's step, update, setup and kernel calls; the VCU's `copy_maps`,
`copy_def` and `read_packet` (moved from `vcu/setup.rs` and `vcu/dispatch.rs`); the tyre's `emit_noise` and
`value8_clear`; the wheel's step, ground probe, refresh, finish and calls; every car-body sequence (the car step, the
aero, drag, seed, sensors, steering, damage handler and functors, lights); the engine's and drive's steps, sims and
calls, and the drive line's. The bindings in `rbr-hybrid` call the trait methods on `ctx.mem` (`ctx.mem.load_frame(this)`).
Constructors and setup copies stay on `&mut Mem`: they run at the boot or the load, before any `lift`.

A record with `load`/`store` (`DriveWorker::load(mem, at)`) keeps them as generic forwarders to its trait methods
(`load_drive_worker`, `store_drive_worker`), so its callers did not change. Value cells read at computed addresses
(`Vec3`, `Vec4`, `car_body::layout::Real`, `Lane`, `IntLane`, `Seed`) load and store through `Raw`.

A calls trait whose methods take the memory gets the store as a parameter with `Mem` as its default
(`Vf6cxCalls<S = Mem>`), so the bindings' impls are unchanged and the generic sequence takes `impl Vf6cxCalls<S>`.
Where two store traits name the same method and both bound a function, the call is written as UFCS
(`EngineStore::load_blend_terms(mem, at)`); the names stay the old ones.

A sequence function reaches what its trait does not name yet (out pointers into the caller's stack, another
subsystem's words) through `Raw`; on a typed store `Raw` is its inner image, which the poison mode watches.

## Entries and Mem-direct dispatch (B1)

The tyre's and the VCU's binding bodies are in `rbr-physics` as generic entries, one per original function
(`tyre/layout/entries.rs`; `vcu/layout.rs` with `VcuCalls` for the drive, engine, car and light accessors the VCU
calls). The bindings decode the registers and call them; the spring-damper's and the wheel's bindings already called
their layouts' sequences.

`Pure::direct` (`--dispatch direct`, the live run's default; `--twin mem-direct` for the second side) runs a call to a leaf
as a plain call: `call_at` gives the binding its registers and arguments, and writes nothing to the stack, reads
nothing back and opens no frame. A leaf is a binding whose body makes no host call, passes no `Ctx` on, uses no
`esp`, reads no `get_f32` and is not cdecl (`scripts/pure_leaves.py SRC` lists them for
`rbr-hybrid/src/pure/leaves.rs`; the `leaves_are_bound` test checks the list). Only calls made from or to the subsystems in `IN_PLACE` (entities,
wheel, tyre, strut, VCU, collision, rigid body) run so. A leaf's stack words are learned from what it pops on its
runs through the host, and a call runs in place only when it passes exactly that many: a caller that passes more may
leave them for its next callee, as the pose step's parent-frame call (`CSolid::E43Vf6C`, `rigid_body/pose.rs`)
leaves its three arguments for 0x51b4c0, which it calls with none. On the tanner drive's first 3000 steps this was the
only call that read a word left by an earlier one; `0x503860` (`ret 4`, called with none from `CCar::Vf24x`) reads
nothing. A declining leaf goes through the host, where it declines again.

A call the caller does itself on the pure host (`skip_call`: a getter, an identity walk, a frame hook) writes its
words only outside the direct dispatch (`Host::skip_word`). The one exception is a walker's in-place child walk
(`skip_call_writing` in `entities/walks.rs`): without its words the twin differs at the tanner drive's first step
(`car+0x8ec`). Dropping each of the other 15 sites' writes one at a time kept the twin equal.

On the tanner drive a third of the host calls run in place (16.9M in 35248 steps). The 176 NGP bench retires 754.1e9 instructions against B1's
758.7e9 (`call_direct` at the wheel, tyre, spring and VCU sites, now removed) and B0's 767.4e9 with the leaf calls,
and 739.0e9 when the skipped calls write nothing as well.

## Non-leaf calls (B1b)

Under the direct dispatch a call to a binding that is not a leaf runs as a plain call too (`Pure::call_plain`,
`call_framed`): the call's words are written as the host writes them, but nothing is read back from the stack, the
watched emits are not checked, and the binding runs in its own frame (the callees it leaves the placement of run below
it, as through the host). It needs `args` to hold at least the words the callee pops, learned for every binding from its
runs through the host (`Pure::stack_words`; a cdecl binding pops none). The watched emits and the tyre steps always go
through the host (`NOT_DIRECT`). The image is the host path's word for word, so this needs no list of safe callees.

`Host::call_direct` also leaves the call's words unwritten, for a call site whose callee reads its stack words only
from its arguments and whose caller reads none of them after it; there the words must be exactly the callee's pops. The
sites so far: the ground probe's callees (`wheel/step.rs` `Ground`), `E45_M40C0`'s `LookupTwin` and the CRT vector
iterators' element calls.

An `Invoke` trampoline's method runs so too, entered as the stand-in enters it (`Pure::invoke_plain`). Under the
direct dispatch the bindings get `Direct` (`pure/mod.rs`, a transparent wrapper of `Pure`) as their host: its
`call_at` takes the plain calls, and a skipped call writes nothing (`Host::skip_word`). `Pure`'s own `Host` is the
fact-check's, with no test of the dispatch on its call path.

The wheel step's vector callees (clear, copy, cross, scale) and the ground probe's CRT vector iterators over its ray
blocks run in the caller on the pure host (`skip_call`, `wheel/step.rs`); each iterator's element calls are placed as
the stand-in places them.

On the tanner drive 43.7M of the host calls run in place in 35248 steps (the skipped calls are not counted).

## Field tables

`object!` (`store/layout.rs`) tables an object once: the struct, a `Field` constant per row (`ControllerObj::mode.at`,
`.load(mem, base)`, `.store(...)`) and the `Object` impl (`lift`, `lower`, `FIELDS`). Value kinds: `u32`, `i32`,
`f32`, `u8`, `Vec3`, `Vec4`, `store::Real` (an NTweak CReal body) and arrays of them. A punned word (float bits in an
int) is a `u32`; `f32` keeps NaN payloads as well.

```rust
crate::object! {
    /// `CSpringDamper` (the controller; the wheel's strut at `wheel+0xec4`).
    pub struct ControllerObj, size 0x30 {
        vtable @ 0x00: u32,
        definition @ 0x04: u32,
        ...
        signal_valid @ 0x2c: u32,
    }
}
const MODE: u32 = ControllerObj::mode.at;
```

Tabled so far (`store::car_objects`): the tyre scratch (`tyre::layout::ScratchObj`, wheel `+0xf00`), the strut and its
state (`spring_damper::layout::{ControllerObj, StateObj}`), the VCU scratch and state (`vcu::layout::{ScratchObj,
StateObj}`): 14 objects, 3748 owned bytes per car. The spring and VCU layouts take their offsets from the tables. The
tyre's `load_frame`/`store_frame` still use their own offsets; B2 moves them onto the table.

## Checks

- Round trip: `round_trip SNAPSHOT...` (`rbr-hybrid` bin) checks `lower(lift(image)) == image` on every owned byte and
  the owners the tables name (strut state to its wheel, VCU state to the car).
- Twin: `rbr-standalone --recording X --boot|--snapshot Y --twin mem` runs a second playback of the image with the
  same payloads and compares the car and wheel blocks and the tabled objects' owned bytes after every step; it round
  trips both images first. `--twin mem-direct` runs the second side with the Mem-direct dispatch; B2 adds the typed
  storage.
- Poison: the `poison` feature of `rbr-physics` (check builds only) adds `Mem::poison(ranges)`: any access to a
  poisoned byte panics with the owner's name and offset. `Mem::unpoisoned()` is the same storage without it, for
  `lift` and `lower`. Without the feature the guard is an empty inline function.

## Left for later B0 batches

- Store traits for the other subsystems (rigid_body, physics_core, collision, entities, stage, load, ntweak) and the
  Mem-taking functions outside the layouts (collision, physics_core, ngp). The generic sequences reach those
  subsystems through `Raw` today (`ntweak::set_dirty(mem.mem_mut(), at)`, `material::tag_entry(mem.mem(), ...)`).
- `--twin` in the live run (`Live` holds no second playback yet).

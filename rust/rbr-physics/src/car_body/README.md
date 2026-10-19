# Car body: plan

Scope: `src/rbr/RBR_E34_*` (CCar, CState, the quaternion kernels, the car's transforms and aero/drag steps),
`RBR_E35_*` (CCarBody, the damage handler), `RBR_E36_*` except `RBR_E36_Collide` (damage ops, lights, tail
helpers, emit), and `RBR_E39_DamageA` (two constant slots). Reached by the reference drive:
`notes/progress/called.csv` x `scripts/progress.py` `physics_subsystem` = `car-body`: **96 functions**.

Sources: `notes/physics/car-body.md` (semantics, asm checks, the C1-C9 suspect list),
`notes/physics/renames/car-body.csv`, `notes/physics/damage.md`, `notes/physics/tweak-keys.md`.
The asm decides operation order and precision (`notes/rust-physics.md`); the C++ is the reference for meaning.

## Batches (leaf to step)

Each batch: vectors from the step (`harness/vector_specs/car_body.py`), a port with typed structs, and a
vector test (`tests/car_body_vectors.rs`, `RBR_CAR_BODY_VECTORS`). External calls are answered from the
records (as in the entities tests).

| batch | functions (address, C++ name) | notes |
| --- | --- | --- |
| A1 quat kernels | 0x4e4c70 `E34Quat::Norm`, 0x4e4d10 `E34Quat::ToQuat` | pure 4-float math |
| A2 car accessors | 0x4e5cf0 `GetDir`, 0x4e6730 `GetA48`, 0x4e65b0 `StateAtb`, 0x4e65e0 `GetB0C`, 0x4e65f0 `GetB1C`, 0x4e6660 `AddADC`, 0x4e66a0 `SetA1C`, 0x4e76c0 `FfbNorm` | field reads and writes, `SetDirty` (external) |
| A3 car forwarders | 0x4e6600 `Fwd6DB0`, 0x4e6610 `Fwd6DD0`, 0x4e6620 `Fwd43C0` (drive), 0x4e6650 `Fwd18E20` (engine), 0x4e58e0 `Vf24x`, 0x4e6ec0 `Test1670` | external drive or engine calls: answered from the records |
| A4 grid edge | 0x4e7270 `PushGridEdge`, 0x4e72d0 `RefreshEdge` | E37 `AddEdge`, E43 `Vf20` (external) |
| B1 step aero/drag | 0x4e7dc0 `DragAndSpinDamp` (Sim7DC0), 0x4e7ad0 `Sim7AD0` (aero gust), 0x4e7ff0 `UpdateFfbTorque` (Hook1FF0) | MB560, cross, length, scale (E43/E01 helpers, external); Sim7AD0 follows the asm (C1) |
| B2 probe | 0x4e7730 `MotionProbe` (Probe7730) | slot 0x8c gate, sampler (external) |
| B3 car step | 0x4e9930 `StepEngine` (Hook), 0x4ea300 `Vf6cx`, 0x4e91f0 `UpdateSeed`, 0x4ea720 `UpdateA720`, 0x4e9020 `Vf64x`, 0x4ea020 `Vf28x`, 0x4e9af0 `Update9AF0`, 0x4e9c70 `UpdateMrr`, 0x4e9800 `Sample9800`, 0x4e66d0 `Damp66D0`, 0x4ea980 `AppendA980` | the car's own work; calls to drive, engine, VCU and wheels are external records |
| C1 transforms | 0x4e6870 `Xform6870` (C2 bug, see Gaps), 0x4e68e0 `ToWorld`, 0x4e6950 `WorldCompose`, 0x4e69c0 `BodyToWorldPair`, 0x4e6a30 `BodyCompose`, 0x4e8130 `Xform8130` | `E43PairWalk::MB560` (ported in `entities::walk`), `ComposeA` |
| C2 emit | 0x4e81b0, 0x4e8270, 0x4e8330, 0x4e83f0, 0x4e84b0, 0x4e88f0, 0x4e8b30, 0x4e8bf0, 0x4e8cb0 (`CMultiSignal1::Emit`) | signal manager; needs the signal record layout |
| D1 damage | ported in `damage.rs` and `damage_ops.rs`: `Set3FB0` (0x4ebfb0; 40 direct calls: levels 0 to 3, the default word), `CopyFrom` (0x4ec470; 20 direct calls, a random source: the zones, floats and tail), `Test3B20` (0x4ec0d0; 60 direct calls: the enable word, a zero or NaN level, the counted words), `E36DamageOps::AddFunctor` (0x4ef720; 200 direct calls: all 27 mapped indices, the two left out, the full table, out-of-range), the E35 threshold tables (0x4ed370), E36 functor tables (0x4ef910), constructor (0x4ef410), and Slot4 (0x4f1000, called by `UpdateMrr`). `ResetDamage` (0x4ec1e0; 40 direct calls, `car_body_reset`: the zones, the floats, seven member calls answered from their records, the light block by `light::block_reset`). Load table bytes match the lcb1-bodies A720 return dump, apart from the seed word that `UpdateA720` writes after Slot4; the three runtime snapshots groups are recorded in `bound.csv` | `notes/physics/damage.md` |
| D2 body | 0x4eb3e0 `Vf50Def`, 0x4eb510 `CDef` ctor, 0x4eb6a0 `CState::Vf44x`, 0x4eb730 `CCarBody` ctor, 0x4eb820 `Vf9Cx`, 0x4eb8c0 `VfA0x`, 0x4eba80 `NewDefD0`, 0x4ebb00 `Vf50x`, 0x4ebbe0 `Vf28x` | E35 body: load-time and step parts |
| D3 lights | ported in `e36.rs`, `light.rs`: `E36LightBlock::Reset` (0x4f19b0; 20 direct calls, random block), `CCarLightSystem::CDef::Vf20x` (0x4f17e0; 20 direct calls: the static table), `CE36SubBlock::Init` (0x4f3e80; 20 direct calls); `tweak.rs`: `NTweak::CFloat3::LoadBinary` (0x4f3d20; 20 direct calls). `e36.rs`: `E36VecU::CrossNorm` (0x4f3d40; 40 direct calls, `rsqrtss` from `math::rsqrt`), `E36Triple::MinPick` (0x4f3ec0; 40 direct calls). `light.rs`: `EmitLights` (0x4f20c0) and `Update` (0x4f2150), see the status table. The light system's constructors (0x4f1dc0, 0x4f1840) are ported with the car instance (load step 4, `layout::construct_lights`, `construct_light_state`); `DtorBody` is not | E36 light system; load-time parts with RBR_SNAPAT |
| E load-time | 0x4e5610 `CStatePair` ctor, 0x4e61f0 `CDef` ctor, 0x4e6f20 `CDef::Create`, 0x4e6f80 `CState` ctor, 0x4e6400 `CDef::DtorBody`, 0x4e6af0 `E34LinkList10::PushItem`, 0x4ea9d0 `CCar` ctor | constructors and allocation: `RBR_SNAPAT` load dumps, not drive steps |
| F constant slots | 0x4e5230, 0x4e5250, 0x4e5590, 0x4e63f0, 0x4ea830, 0x4ea890, 0x4eb4f0, 0x4eb570, 0x500300, 0x500320 | return constants and vtable forwards; no port needed beyond the test of the return value |

Gaps: none in B1. `Sim7AD0` follows the asm (its in-place wind MAC and its speed weight); its C++ difference is under
Open items.

C++ bug (for the lead, not fixed in `src/rbr`): `CCar::Xform6870` (`RBR_E34_CarB.cpp`, `ComposeA(b, t, b)`) passes `b` as
the first argument. The asm calls `ComposeA(a, t, b)` (0x4e68cc, `call 0x51b4c0`), and the original's record of the inner
call is `(this, a, t, b)`. The C2 note in `car-body.md` said the call was `MB560`; that was an error in my first reading
(I read the function before 0x4e6870). The port follows the record.

## Status

| batch | status | vectors (9 snapshots: vcu-cov 300, 1000, 1600, 2000, 2400, 2800, 3200, 10000, 11000) |
| --- | --- | --- |
| A2 accessors and setters | ported: `GetA48`, `GetB0C`, `GetB1C`, `StateAtb`, `AddADC`, `SetA1C` | 23 calls per snapshot, all passing |
| A1 quat | ported to `math::quat` (shared): `ToQuat` | the drive's 45 calls (`pos`, `diag2`) plus 360 direct calls with fuzzed matrices (`scratch` runner, see below): all four branches |
| B1 drag | ported in `drag.rs`: `DragAndSpinDamp` | the 9 drive records (gate off, one call each) plus 40 direct calls with fuzzed velocities, angular velocities, setup and gate: 21 gated, 31 over the spin limit, all passing |
| B3 lists | ported in `links.rs`: `AppendA980` (0x4ea980) and `CBodyLink::Unlink` (0x4e0020) | 9 drive records (7 first members and 2 sole members, owner = head) plus 40 direct calls with fuzzed lists: all four unlink shapes under both owners (head and a second list), and free links; 42 unlinking, 16 free; all passing |
| B1 aero gust | ported in `aero.rs`: `Sim7AD0` (0x4e7ad0) | the 9 drive snapshots: one call each, all returning before ToWorld (the brake-wall penetration is 0). 120 direct calls with fuzzed penetration, velocity (one in ten nearly still), seed, spin, wind and inverse mass: 49 sent (two MB560 walks, S70, S74), 37 returned after ToWorld, 43 before it; all passing |
| B3 dampers | ported in `damper.rs`: `Damp66D0` (0x4e66d0) | 9 drive records (one call per snapshot) plus 120 direct calls with fuzzed cell lanes and gains: SetDirty on both CReals and the values; all passing |
| B3 signal emitter | ported in `update_9af0.rs`: `Update9AF0` (0x4e9af0) | the 9 drive snapshots: one call each, with arg 0 (the early return). 120 direct calls (args 0 to 7, fuzzed gap, flow and gate words): the timer, the flow path with its gate, the test chain and the final emit; the flag-1 emit (0x4e8770) is not verified (see open items); the probe's own answer is read from the emit after it; all passing |
| B3 composite | ported in `vf64x.rs`: `Vf64x` (0x4e9020) | the 9 drive snapshots: one call each, with the drag, the gust, the spin send, the walk, 0x5224f0, 0x52cb20, the blend and the loop's test (0x521920, false in every record). 60 direct calls with fuzzed drag, gust, vector, flag, keeper bit and def inertia (5 reach 0x495090): all passing. The loop's action (0x521740) is never reached. Bound: 30 steps on the 9 snapshots, all bindings, equal to the original; the lane 3 model is in Open items |
| B3 car step | ported in `vf6cx.rs`: `Vf6cx` (0x4ea300), bound | the 9 drive snapshots (one call each) plus 72 fuzzed calls (`--set car_body_vf6cx --fuzz 1..8` on the 9 snapshots): random brake-wall raycast (hit or miss) and random wheel flag words. All 81 passing. The fuzzed calls reach the impulse, the brake-wall emit and the contact timer (36 calls with a hit), S90 and SimFD00 (all). Not reached: Vf6c, ExclA060, StepBig (the body test 0x521920 is false in every run, see open items). Bound: 30 steps on the 9 snapshots, all bindings, equal to the original (miss path only, see open items) |
| B3 sensor sampler | ported in `sample9800.rs`: `Sample9800` (0x4e9800) | one load-time entry dump of the damage drive (the first call, from `RBR_SNAPAT`), plus 40 direct calls from that dump with fuzzed E39 count and scale, filtered steering torque and position value. The drive reaches it only through Vf28x, which runs outside the step, so no step-snapshot record. All passing |
| B3 seed | ported in `update_seed.rs`: `UpdateSeed` (0x4e91f0) | the 4 entry dumps of the damage drive (its first four calls: the override path, counters 0 to 3) plus 300 direct calls from the first dump with fuzzed override, seed, flags, E19 object, S8C gate, MotionProbe inputs and counter. Every branch is reached: the seed with and without the override, flags 0x20 (S8C, S158) and 0x10 (MotionProbe, the 0x4e8a70 emit, Assist8DF0), E19 hit and miss, the store on marks 0 to 3. 310 calls, all passing; 6 calls with a nested Assist8DF0 run are checked for the records only |
| B3 retire and outside assistance | ported in `assist.rs` (the lane verdict and the traits) and `layout/car_calls.rs` (the sequences): `Test7910` (0x4e7910), `Assist8DF0` (0x4e8df0), `E34_RetireA` (0x4e8fa0), the emits 0x4e8770 and 0x4e89b0 (`emit.rs`), and `M5665f0` (0x5665f0, byte-identical to `flow_holds`) | direct calls on the damage snapshot 11827 (`g-retire` set and `g_flow` for 0x5665f0): Test7910 150 calls, Assist8DF0 120 (its nested UpdateSeed and SetDirty answered from the records), E34_RetireA 120, the two emits 120 each, M5665f0 150; all passing. Test7910's ToWorld input is (0, -1, 0) in the asm (the C++ passes the car's xy). LerpAt's count check traps in the original, so the port declines there |
| B3 wiring | ported in `update_mrr.rs`: `UpdateMrr` (0x4e9c70) | the damage drive's entry dump (one call) plus 60 direct calls from it with random clears of the list entries at +0x480 and +0x490: the loops with 0 to 4 non-null entries, all passing. UpdateSeed is answered by its externals' records (its car writes are not compared) |
| C1 transforms | ported in `xform_8130.rs`: `Xform8130` (0x4e8130) | 60 direct calls (`harness/direct_calls.py` from the entry dump, fuzzed `a` words): BodyCompose (0x4e6a30) answered by its record, S30 (0x519c00) and ComposeA (0x51b4c0) by their records. All passing. Not reached by the step; it runs only from Vf28x |
| B3 sensors and signals | ported in `vf28x.rs`: `Vf28x` (0x4ea020) | the 4 load-time entry dumps (`vectors/dumps/car_body/`, recorded with `--set car_body_vf28x`): these take the long path, not the short one. 60 direct calls fuzzed over the def (`car+0x30`, null for the short path), the keeper bit (the keeper branch: two slot-12 getters and two ComposeA calls) and each body's test word (the step 0x522000). All 64 passing: the car window, the def and the frame (the frame's prologue pushes, the loop counter, the engine pointer slot and the dead `car+0x100` copy are not compared). Xform8130, Sample9800 and the four emits are answered by their records |
| B3 steering step | ported in `step_engine.rs`: `StepEngine` (0x4e9930) | the 9 drive snapshots: one call each (the slew, steering target, rate and demand cells, their four SetDirty calls in order). 120 direct calls with fuzzed seed, slew, steering target, def gains and dt (102 over the 20/s cap, 18 under): all passing. The externals are not answered (see open items) |
| C1 per-step transforms | ported in `transform.rs`: `ToWorld` (0x4e68e0), `WorldCompose` (0x4e6950), `BodyToWorldPair` (0x4e69c0), `BodyCompose` (0x4e6a30) | the drive's 9 snapshots (`--set car_body_step`, `vectors/car_body/world-*.txt`): ToWorld 34 calls, WorldCompose 36, BodyCompose 18, each with its walk or ComposeA record and the frame global mapped from it. BodyToWorldPair is not reached by the drive: 8 direct calls from the 1000 snapshot (`harness/direct_calls.py --set car_body_step --addr 0x4E69C0 --this 0xbf1d7b8`). The C++ matches the asm for all four (no `cpp-fixes` entry). |
| C1 per-step transforms (cont.) | ported in `accessors.rs` (`get_dir`) and `steering_ffb.rs` (`ffb_norm`): `GetDir` (0x4e5cf0), `FfbNorm` (0x4e76c0) | the drive's 9 snapshots never reach either (0 calls). Direct calls from the 1000 snapshot: `GetDir` 40 (`getdir-direct.txt`, fuzzed wheel lanes and empty wheel slots: 4 fallback results, 36 unit), `FfbNorm` 40 (`downforce-direct.txt`: NaN 4, clamp to -1 5, clamp to +1 5, inside 6, gate on 20 (returns 0)). GetDir's `Length` and `Clear` are the shared math (`Vec3::length3`), not externals. The C++ NaN cases are in `lead/briefs/cpp-fixes-from-rust.txt` (20, 21). |
| C1 grid edge and Vf24x | ported in `edge.rs`: `PushGridEdge` (0x4e7270), `RefreshEdge` (0x4e72d0); `forward.rs`: `Vf24x` (0x4e58e0) | `PushGridEdge`: 9 drive snapshots (`--set car_body_edge`, `vectors/car_body/edge-*.txt`): 9 calls, all early returns (the flag is set); 24 direct calls with the flag and grid fuzzed (`edge-direct.txt`) reach the full path (BodyCompose, AddEdge: 13 calls) and the early return (grid null or flag set: 11). `RefreshEdge`: 9 drive calls (flag set), each with its car's E43Vf20 record. `Vf24x`: not reached by the drive, and a direct run records nothing for the tail jump: `unit` test only (the forward's argument). |
| C1 wheel attach | ported in `wheel.rs`: `Vf150x` (0x4e5f20) | the drive never calls it (0 calls in the 9 snapshots). 40 direct calls with the wheel's flags, its def's slot indices (0 to 5) and the car's slots emptied at random (`--set car_body_wheel`, seed 9, `vectors/car_body/wheel-direct.txt`) reach every path: flags 0x0, 0x10, 0x20, 0x30, 0x420, 0x430, 0x820, 0x830; the list entry free or taken; the slot index in and out of range; the slot empty or taken. All passing. The null wheel is a `unit` test (no record) |
| D3 E36 helpers | ported in `e36.rs`: `CrossNorm` (0x4f3d40) and `MinPick` (0x4f3ec0) | direct calls only (no step reaches them): `cross_norm` 40 calls (`car_body_e36`, `vectors/car_body/e36-direct.txt`: random lanes with zeros, infinities and NaNs; the sum's `rsqrtss` from the reference table), `min_pick` 40 calls (the float 0.5 boundary, NaN gives `m_b`). All passing |
| D1 reset | ported in `damage.rs`: `ResetDamage` (0x4ec1e0) | 40 direct calls from random handler bytes (`--set car_body_reset`, `vectors/car_body/reset-direct.txt`): the zone and float clears, the member calls answered from the records (the trait `ResetDamageCalls`), the flag and table words. All passing |
| D1 name | ported in `damage.rs`: `Vf14x` (0x4ec440, slot 5) | 40 direct calls (`--set car_body_vf14x`, `vectors/car_body/vf14x-direct.txt`, handler at car+0x9f4): the gate set (the material name through the recorded Tag520940) and clear (the manager returned: the asm, see cpp item 26). The default name for a null tag is not reached (every tag is an entry) |
| D3 light emit | ported in `light.rs`: `EmitLights` (0x4f20c0) and `Update` (0x4f2150; six gates) | `Update` 40 direct calls (`--set car_body_light`, `vectors/car_body/light-emit.txt`: a whole light system in the stub heap, the gates random, 472 emits checked against their signal windows); `EmitLights` 40 direct calls (the four heads, the flags, the scale zero or not). The emit (0x4f2000) is a call-sequence trait (`LightCalls`), answered from the records. All passing |
| D2 state | ported in `state.rs` and `layout::state_vf44x`: `CState::Vf44x` (0x4eb6a0, slot 17) | the flag, the owner's slot 12 (a vtable call), `CBody::ClearGrid` (the collision port, called directly) and two `CopyFrom`s; unit test (`state::tests`) for the order and the bounds; the owner-less state declines. Lane 3 of the bounds is not modelled. The drive's step reaches it (see `bound.csv`) |
| D2 scatter | ported in `body.rs`: `Vf9Cx` (0x4eb820) and `VfA0x` (0x4eb8c0) | 40 direct calls each (`--set car_body_body`, `vectors/car_body/body-direct.txt`: random points and body owner words, the gate from the manager flag and host word, 16 slot-0 records): lanes 0 to 2 added or subtracted per point; lane 3 is not modelled (the asm copies uninitialised stack). A null owner base faults in the original, so the port asserts it. All passing |
| B3 seed | ported in `apply_seed.rs`: `UpdateA720` (0x4ea720, `apply_seed`) | 40 direct calls from the 1000 snapshot (`--set car_body_a720`, `vectors/car_body/a720-direct.txt`, x87 oracle on cachy): the seed block random, its call words pointing at blocks in the stub heap. The externals (M8770, NopD300, CopyDef, four CopyCF30) and UpdateMrr are answered from their records. The wheels' writes (CopyCF30 and UpdateMrr write through each wheel's def, `wheel+0x34`, up to +0x900; the subobjects at +0xec4 and +0xf00) are windows on the root and the externals (`wheel_windows`, followed from the car at +0x470 + 4k). Windows are 0x1000 for a wheel and 0x904 for a def: writes past them would not be seen. All passing |
| C2 emit | ported in `emit.rs`: `CMultiSignal1<T>::Emit`, nine instances (0x4e81b0, 0x4e8270, 0x4e8330, 0x4e83f0, 0x4e84b0, 0x4e88f0, 0x4e8b30, 0x4e8bf0, 0x4e8cb0; one template, the asm's first 0xa0 bytes identical) | the drive's 9 snapshots reach two: position (0x4e8b30) and MrR state (0x4e8bf0), 9 calls each, `vectors/car_body/emit-*.txt`. The other seven are not reached in these steps, and their direct runs (`--set car_body_emit --addr ... --this <g_signalManager + offset>`) stop on unmapped reads in the emitters' lists (cause not settled; the offsets are from the C++ in `RBR_E34_CarD.cpp`). Their status is `partial` in ports.csv. |
| B2 probe | ported in `probe.rs`: `MotionProbe` (0x4e7730) | direct calls only: the drive's nine steps do not reach it (its callers 0x4e92c7 and 0x4e9c1f are in `StepEngine`). 80 direct calls with fuzzed velocity, gate (car+0x1ec, S8C) and M5666b0 result: Gated 22, Miss 41, TooFast 17, all passing. First and Second (the sampler's verdicts) not reached |
| Load step 4: the car instance | ported in `layout/construct.rs`: `CCar::CDef::Vf50` (0x4eb3e0, `new_car`), `CCar::CCar` (0x4ea9d0, `construct_car`), `CCar::CState` (0x4e6f80) with its `CStatePair`s (0x4e5610), the light system (0x4f1dc0) and its state (0x4f1840); the connections' inline `push_back` in `links.rs`. The base is `rigid_body::layout::construct_solid`; the drive, engine, control unit and passenger parts are their subsystems' constructors | `tests/load_defs.rs` (`car_instance_matches_the_game`): the car built by its factory on the Rust-built definition, against the same run's dump at the factory's return. Every byte of the car (7,520, 16 in name words), its exclusion set (128), the current car word and the five signal lists it joins is equal, on blocks holding what the game's held. The engine state's crash branch (`DbgFlags` 0x200) is `Err`; the drive state's other game mode (0x566690) is not reached |

Reached in the step at snapshot 300 (of 96): 33 functions; the drive's steps reach `Sim7DC0`, `Sim7AD0`, `Hook1FF0`,
`Hook` (StepEngine), `Vf6cx`, `Update9AF0`, `Vf64x`, `Vf28x`, `UpdateMrr`... once per step, others only at load.

## Names

| Name | Original | Meaning |
| --- | --- | --- |
| `layout::Real` | `CReal` (`CBase` + value) | dirty flag at +8 (`SetDirty` writes 1), value at +0xc |
| `layout::STATE` / `state_address` | `CCar::m_stateA00` (+0xa00) | the state object's address (`StateAtb`) |
| `layout::lane_a48` | `GetA48` (+0xa48, stride 0x40) | unknown meaning; `i` is 0 or 1 in the drive |
| `layout::steer_torque_sum` | `CCar` +0xad0 (value +0xadc) | the steered wheels' torque sum (`AddADC`), cleared each step (`notes/ngp-ffb.md`) |
| `layout::cell_at`, `matrix_cell` | `SetA1C` (+0xa1c, 4x4 cells of 0x10) | unknown meaning |
| `layout::target_b0c` | `GetB0C` (+0xb0c) | the steering rack position (`notes/ngp-ffb.md`) |
| `layout::t_override`, `speed`, `ffb_torque`, `steering_rack_ratio` | +0x1670, +0x15fc, CReal +0xae0 (value +0xaec), def +0x23c | `steering_ffb.rs`: `update`; `steering_rack_ratio` is the `SteeringRackRatio` key's value |
| `layout::rate_b1c` | `GetB1C` (+0xb1c) | the rate of +0xb0c (`car-body.md` 2a, inferred) |
| `layout::wall_penetration` | `m_wallPenetration` (+0x166c) | brake-wall penetration (m); `Sim7AD0` acts only above 0 |
| `layout::accumulator` | the vector at +0x370 (`E46_Get370`) | the seed `Sim7AD0` moves to the world; S70 adds into it |
| `layout::aux_vector` | the vector at 0x15d0, four lanes | `Vf64x` blends it with the walk of 0x380 (`0.7` decay, `0.3` share) |
| `layout::steer_input` | `m_steerInput` (+0x16a4) | the slew moves toward it in `StepEngine` |
| `layout::max_ff_torque` | def +0x44a0 | `FfbNorm`'s divisor, the `MaxFFTorque` key's value |
| `layout::steer_slew_at` | `m_steerSlew` (CReal +0xaf0, value +0xafc) | moved by `StepEngine` |
| `layout::inverse_mass`, `layout::wind` | def +0x16c, keeper +0x160b0 | `car-body.md` 201: the wind's scale is `-1 / inverse_mass` |
| `math::quat::Quat` | the 4 words `out[0..4]` | `x, y, z, w` (shared, `src/math/quat.rs`) |

## Platform: default NaN

`to_quat`: the square root of a negative argument gives the default NaN, whose sign differs by platform (x86 negative,
arm64 positive). The recorded outputs are +NaN (`0x7fc00000`, from the emulator's x87), so the test compares a NaN
with any NaN (`same_word`). Comparisons are NaN-safe (the branch is the asm's).

## Platform: default NaN, UpdateFfbTorque

NaN inputs propagate (no sqrt, no default NaN), so the test compares NaN outputs NaN-tolerantly as well (`same_word`), the
same rule as `ToQuat`. Comparisons follow the asm: `t_override > 0` (NaN takes the ramp), the speed clamp keeps NaN.

## Float to int

None in batch A: the branch index `i` of `ToQuat` comes from comparisons, not a conversion.

## Open items

Not ported yet, and why:

- The `ComposeA` family (`Xform6870`, `WorldCompose`, `BodyCompose`, `Xform8130`) keeps the original's addresses for its
  arguments and output (`ComposeA` writes a 16- or 28-byte block into memory). Readability rule 1 asks for typed values:
  this stays until the composed frame has a typed layout. The walks (`MB560`, `PairWalk`) take and return `Vec3`.
- `Vf24x` (0x4e58e0) is a tail jump to the drive's 0x503860 (`car + 0x4b0`). Its vector check is a `unit` test: the drive
  never reaches it in the snapshots, and a direct run of the thunk records no call. Status `unit`, not `checked`.
- `PushGridEdge`'s zero input is `Vec4::default()` (the asm's `E09Vec4::Clear` on a 16-byte local); the record's Clear calls
  are not compared. The port keeps the flag and returns it; the caller stores it (`layout::set_grid_pushed`).
- The emit's signal is an address (each `T`'s payload is the receivers' to read), and its lists are walked in memory (the
  links pattern). The index limit (0x20) is an assert; the original logs and goes on.
- `GetDir` lane 3: the asm scales the caller's word 3 of `out` on the unit path (`mulps` over four lanes) and keeps it on the
  fallback. The caller copies four lanes into its vector (CarD, after `GetDir(loc150)`). The port returns three lanes, and
  the test checks lanes 0 to 2 only. Lane 3 is the caller's stale word: not reproduced, not verified (open).
- `Vf150x` (0x4e5f20): the wheel's `+0x2c` array is null in the snapshot, and the slot-59 append on the wheel would fault on it,
  so the direct fuzz points it at the car's array (`car + 0x2c`). Those appends are not in any window, so the test does not
  check them. `E43Vf150` (0x51a370) is an external: its list append (`+0x48` head, `+0x4c` tail, `+0x50` count, the wheel's links
  `+0x10` to `+0x18`) is applied from its record after the port returns. Its write to the previous tail's `+0x10` is outside
  every window, so it is not checked (open).
- The S30 identity getters (slot 12) are taken as the identity (`this`), as the other ports do; the records show their
  eax equal to `this` (every drive and direct record).
- Walk outputs are the record's (`MB560` and `ComposeA` are externals). The tests check each call's `this`, arguments and
  input lanes, and the output lanes of the transform's `b`; lane 3 of the walk output is not compared (the port's vector
  is three lanes).

- `Xform6870` (0x4e6870): ported in `transform.rs` (the asm's call, see there). 360 direct calls (`harness/direct_calls.py`,
  40 per snapshot) match. Compose is an external (its walk is the entities port's); the test answers it from the record.
- `MotionProbe` (0x4e7730): ported in `probe.rs`. Externals answered from the records: S8C (slot 35, reads car+0x1ec),
  E60Flow::M5666b0 (0x5666b0), ToWorld (0x4e68e0, ported in `transform.rs`; answered from its record here) and the sampler Sampler54A0 (0x4e54a0). Findings:
  - The verdict words are at big+0x44 and big+0x48, not +0x40/+0x44 as the C++ comment says. The asm pushes the sampler's
    buffer address after a `push`, so the buffer is at E+0x30 and the verdict reads at E+0x74/E+0x78 are big+0x44/+0x48.
    The records agree (both words zero in the Miss calls).
  - The sampler's `found` is 1 in every direct call (22 calls reached it), and both verdict words stayed zero: the First
    and Second branches have no records. They are one compare each; the sampler's geometry would need fuzzing.
  - The `ToWorld` input is (0, 0, 1) and the dot is `(o2*i2 + o1*i1) + i0*o0`, as in the asm.
- `AppendA980` (0x4ea980) and `Unlink` (0x4e0020): ported in `links.rs` with opaque handles (`LinkRef`, `ListRef`: the
  original's addresses) and typed `Link` and `List` values. `layout/car.rs` implements the `Links` store on the memory, and
  a neighbour is changed one word at a time, as the asm does. Decided by the lead: no Rc or owned graph for now.
  Coverage is the fuzz's (`append_fuzz`, lists inside the head window at this+0x20 and this+0x80); the drive reaches only
  the first-member (7) and sole-member (2) shapes. A link that is the only member of its list clears the owner's first and last, as the asm's stores to the owner do.
- `Update9AF0` (0x4e9af0): ported in `update_9af0.rs`. Externals answered from the records in call order (each skips
  its nested calls to its own record): the timer (0x4a0290, x87 double), the flow check (0x566640), S8C (slot 35),
  0x4e8df0, the emits 0x4e8770 and 0x4e88f0 (on the signal objects at 0x893600 + offsets), the tests 0x4ec0d0, 0x504840,
  0x50ecf0. Open:
  - The drive calls it with arg 0 only (the early return), so every other path is direct-only.
  - Flag +0x1668 nonzero (the emit 0x4e8770 with code 1): the original's direct run stops in its crash-dump path
    (`CreateFileA` from 0x401c34, reached through the emit) with the flag set, in all snapshots tried. The flag is fixed at
    0 in the fuzz, so that branch is ported from the asm and not verified. The cause is not known (the signal object at
    0x893600 may not be ready in snapshot 300); the drive does not reach the branch either.
  - MotionProbe (0x4e7730) is listed as a ported function, not as an external, so its own call has no record (the recorder
    keeps only externals as inner records). Its answer is read back from the emit that follows (code 0, 3 or 4; no emit
    means `out` 4). MotionProbe's own vectors are in its test.
  - Direct arguments above 5 run (6 and 7 are in the fuzz) without a crash, but the drive never reaches them.
  - The car's words other than +0x9ec are not compared: the externals write them (not modelled), so only the port's own flag is checked.
- `Vf64x` (0x4e9020): ported in `vf64x.rs`. It runs the drag and the gust (their externals as in those tests), walks the
  angular velocity and sends the cross product to S74, then calls 0x5224f0 (car+0x8b0), 0x52cb20 and walks +0x380 into
  the blend, sends 0x495090 when the keeper's bit 0x10 and the flag at +0x16ac are set, and runs the loop of four objects.
  Open:
  - 0x52cb20 writes the 16 bytes at +0x380, the walk's input. Its record gets a node window there, and the test writes
    the recorded output into the car. Its records are matched by the car argument: other objects' calls to 0x52cb20 come
    earlier in the flat list.
  - The identity S30 (slot 12, called through `*0x30` between 0x52cb20 and the walk) is not modelled. It leaves the car as
    it is.
  - S70 and S74 get node windows for the accumulators (+0x370, +0x310) after the call. Only `Vf64x`'s test writes them,
    because its own Sim7AD0 reads the seed after the drag's S70. The DragAndSpinDamp and Sim7AD0 tests do not, since
    nothing in those functions reads them after the sends.
  - The loop's test (0x521920) is false in every recorded call (the drive and the direct runs), so its action (0x521740)
    is never recorded. The port calls it from the asm and does not verify it.
  - 0x495090 takes the flag word at +0x16ac as `this` (the asm leaves it in ecx), then two floats: `blend[0] * k` and
    `aux_scalar * k`, with `k` = -0.10193679 (0x738ab0). The drive does not reach it. The direct calls set the keeper bit
    and the flag half the time, so 5 of 60 reach it.
  - 0x52cb20 leaves ecx changed at return, so its record's first argument (the car) is the check, not the entry ecx.
  - The fourth lane (0x15dc) is modelled. The asm blends all four lanes (`mulps`, `addps`), and MB560 copies lane 3 of its
    input (`a`, here the car's 0x380, passed by address) to its output (0x51b583). The binding walks the car's vector by
    address, so the blend's lane 3 is the car's 0x38c as the emulator left it. Its initial value is written from the
    constant at 0x73bef4 by the function at 0x50f099 (0x5137e4; not identified as the load-time constructor).
    Residue: `E01VecSetA::Cross` (0x43c630) writes lanes 0 to 2 of the cross vector (`loc50`), so `loc50`'s lane 3 is
    stack residue; it reaches the accumulator (0x370) only if S74 reads lane 3 (not checked: the sends' vtable targets
    are not traced). `diverge.py`'s static mask does not cover 0x15dc, and the port does not mask it: the 9 x 30
    all-bindings run (default stack fills) is equal, and the port's record test checks all four lanes.
- `Vf6cx` (0x4ea300): ported in `vf6cx.rs`, bound in `rbr-hybrid/src/car_body/step.rs`. Checked against the asm (llvm-objdump):
  the 28-byte brake-wall buffer is the raycast's output (word 0x10 penetration, word 0x14 the impulse's `f`, bit 0 of byte
  0x18 the contact flag); the impulse is called with (buffer, `f` word, `p` word, `dt`) as pushed, and the emit with the
  buffer's address (16 bytes). The emitters' payloads are written from `P + 4`, with `P` the pointer the emit takes (the
  position payload's first word is the asm's stack slot at 0x70 from the frame, unwritten). The drive reaches the miss path only (the brake-wall penetration is 0 in
  the 9 snapshots): the hit path (impulse, emit, contact time) is verified against the asm and the records' arguments, not
  by a bound run.
- `Sample9800` (0x4e9800): ported in `sample9800.rs`. Open:
  - Called only from Vf28x (0x4ea020, outside the step), so the drive records come from entry dumps, not the step.
  - The first `Emit` payload word 2 is `Get4540 * def+0x24c` in f32; the asm multiplies an x87 value by a single, so the
    product is the same single rounding. The getters return x87 values: `st0` as a double, rounded to single here.
  - Timer gap (`0x4a0290`) returns an x87 double; the port rounds it to single, as the asm's `fstps` does for word 15.
  - The getters' inner calls (`0x52e390` under SimE970) are not recorded: only the listed externals appear.
- `UpdateSeed` (0x4e91f0): ported in `update_seed.rs`. Open:
  - The damage drive's step snapshots (`c7-snap-damage.snap-N.bin` on cachy) are version-2 files, which vcu_vectors
    reads as load-time entry dumps, so the records come from the four entry dumps and direct calls only.
  - Assist8DF0 (0x4e8df0) calls UpdateSeed again (at 0x4e8eda). The port answers Assist8DF0 as a record (the ported version is in the retire row above), so the nested
    run's writes are not modelled: the car words after it are not compared (6 calls).
  - S158 is slot 86 of the car's vtable, 0x4e5ee0 (slot 35 is 0x5293e0, S8C). It is now bound to `CCar::Vf158x`, which
    tail-calls the ported `CSolid::E43Vf158` (0x52c030). The new direct-call record checks its slot-0 call, the parent's
    slot-86 forwarding, the entity relay and the SLEEP event. The older UpdateSeed records do not preserve S158's nested
    calls, so the legacy UpdateSeed vector adapter still skips that opaque subtree; runtime dispatch uses the real binding.
  - The override's word 6 (+0x18 of the local block) is never written by the original; the port leaves it to the stack.
  - The direct calls pass the seed buffer at 0x1a4fcac (`UPDATE_SEED_BUFFER` in the spec), which only the first entry dump
    maps. The records are `vectors/car_body/s91-*.txt`.
- `UpdateMrr` (0x4e9c70): ported in `update_mrr.rs`. Open:
  - Called once per step, from UpdateA720 (0x4ea7f4), which the damage drive reaches once; its step records are not in the
    store (see UpdateSeed's open item on the version-2 snapshots). The entry dump and direct calls cover it.
  - UpdateSeed (called from it) is answered by its externals' records; the car words it writes are not compared.
  - The callees 0x51efe0 and 0x521980 are ported elsewhere (`entities/hinge_efe0.rs` `hinge_frame_update`, `physics_core/mounted_pose.rs`
    `pose_update`). 0x53d030 (`E51Wheel::M3D030`, ported since as `wheel::layout::update_strut`) and 0x503aa0 are recorded as externals
    here; 0x503aa0 is not ported.
  - Direct calls use the entry dump's car (0xc53f7c0). The test's UpdateSeed block is a fake address (0x0F000000).
- `Vf28x` (0x4ea020): ported in `vf28x.rs`. Open:
  - Its records come from the load-time entry dumps and direct calls (`harness/vector_specs/car_body_vf28x.py`): the set
    makes Xform8130 and Sample9800 inner records, adds the externals only Vf28x reaches, and gives ComposeA an out window.
  - The emits (0x4e83f0, 0x4e8330, 0x4e84b0, 0x4e81b0) stay external behind `Vf28xCalls::emit`. Their payloads are the
    frame words (+0xa0, +0x18, +0x50), the car's float and the clamped value. Their meanings are unknown.
  - The keeper branch: the drive's and the car's slot-12 getters are the identity (0x519c00). They leave their pushed
    arguments on the stack, and the ComposeA calls pop them (checked in the direct records).
  - The frame's prologue pushes, the loop counter (E+0x14), the engine pointer the port passes as a value (E+0x1c) and the
    dead copy of car+0x100 (E+0xc0) are not modelled and not compared.
  - Two DirectSound 3D calls from the sound module (0x7b0cdb10 SetVelocity and 0x7b0cdc80 SetPos, under E30DXSound) are
    stubbed in `harness/replay.py`: they take no car state, and the Vf28x path reaches them through the drive.
  - The body test (0x521920) is `*(body+0x580) == 1`. The direct calls fuzz that word, so the entry dumps (all false) are
    not the only evidence for the 0x522000 step.
- `Vf6cx` (0x4ea300): ported in `vf6cx.rs`. Open:
  - The drive reaches the walk, the wheel loop's S80 and the position and state emits, but the raycast misses in all
    nine snapshots. The brake-wall branch (impulse, emit 0x4e4490, contact time) and S90, SimFD00 are checked by the
    fuzzed set: `harness/vector_specs/car_body_vf6cx.py` (set `car_body_vf6cx`, `--fuzz SEED`). Its hooks: the raycast
    result (miss or hit with a random impulse magnitude, contact bit and penetration pointer) and random wheel flag words
    at entry (restored on return). The set is car_body's specs without the other fuzz hooks: those would change the drive
    that Vf64x's records use (its loop test and walk input break).
  - Not reached: the body loop's test M1920 (0x521920) is false in every run, so this test does not reach Vf6c (0x521d90),
    ExclA060 (0x51a060) and StepBig (0x522100). Each has its own direct-call records since (`physics_core::layout::mounted_vf6c`,
    `entities::layout::remove_exclusion`, `entities::layout::step_big`). Forcing M1920 true sends Vf64x's loop into
    0x521740, which is not recorded or modelled.
  - The test's `emit_signals` is a no-op: Update9AF0's own vectors (`update_9af0_matches_original_calls`) cover it, and
    Vf6cx's test does not compare what it does to the car.
  - Direct runs are on hold (cachy), so the fuzzed set is the only check of the branches above.
  - The position is `gain * def+0x24c + value` (`+0x15f4`). The port computes it in f64 and rounds to f32; the asm uses
    x87 for this product. The records match; a rounding difference in another state is not ruled out.
  - The drive line's thunk (0x50a4f0) jumps to 0x475ea0, which is not in the spec list. The test answers the thunk from the
    record (its own `eax` and st0 are the result).
  - The passenger step (0x523160), NopA (0x536f70) and Nop3B0 (0x5043b0) have no windows, so their effects on the car are
    not captured. The records show only E43Vf6C changes the velocity in the drive (the input of the walk), so the test
    relies on that.
  - The wheel and body externals are matched by their `this` (the first argument): nested calls of the same function
    (a wheel's S80 inside the loop) come first in the flat list. ExclA060 is matched by its car argument (its `this` is
    not verified).
  - S80, S90 and SimFD00 carry a node window for the wheel's two flag words (when the pointer maps), since they change
    them; S80 is also the solids' E43Vf80, so the window is optional.
  - The fourth lane is not modelled (three lanes, as the lead decided); the test compares the lanes it uses.
- `StepEngine` (0x4e9930): ported in `step_engine.rs`, bound in rbr-hybrid (`car_body/step.rs`, `step_engine`) inside its profiler
  scope (`physics_core::scope`). The cells are written as in the asm: the slew (`f5` capped at 20/s
  by `|(1/dt) * (seed - slew)|`), the steering target (`slew * def+0x22c * def+0x23c`), its rate, the demand cleared.
  Open:
  - The externals are not answered by the test: the scratch sim step (0x537f50), drive step (0x50a1e0), `E43Vf58`
    (0x51a250), `UpdateFfbTorque` (0x4e7ff0) and `Snap9360` (0x4e9360, the drive's snapshot
    flag is 0). The test checks the dirty flags and values of the cells `StepEngine` writes and its SetDirty calls, not
    what the externals write.
  - The demand's value is not checked: after `StepEngine` clears it, an external adds to it (`AddADC`, 0x4e6660), so the
    record's value at +0xadc differs from 0. Its dirty flag is checked.
  - `Damp66D0` is called as the ported `damper::update_dampers`, so its effect on the dampers is also the test's own (not
    compared here).
  - `SetDirty` (0x52f450) is `movl $1, 8(%ecx)`: `set_cell` writes the dirty flag directly, and the test checks the
    SetDirty records in order.
  - The profiler channel (`E46ProfChan` Enter and `ProfExit`) is the binding's scope, not the port's: it writes the sample
    0x8ef668 and the profiler queue, and the binding runs it in the original's order.
  - Constants read from the exe: 20.0 (0x72f614), 1.0 (0x795e14).
  - The drive reaches only the under-cap branch (one call per snapshot, each at a slew under 20/s); the direct calls
    cover both. `harness/direct_calls.py` retries the x87 oracle 64 times per call (was 8): the scratch sim and drive
    steps run x87 code that the oracle fetches step by step.
- `Sim7AD0` (0x4e7ad0): ported in `aero.rs`. Externals answered from the records: ToWorld (the seed; its input is
  checked), MB560 twice (the force `dir`, then the spin `xout`), S70 (slot 28) and S74 (slot 29). Open:
  - C++ difference (for the lead): the C++ weights the drag term by `a0 * 0.01` (`t = a0 * 0.01f`, with `a0 = clamp(2 spd)`).
    The asm multiplies the speed (`mulss 0x72f440, %xmm0` at 0x4e7c73, where xmm0 still holds the speed), so the port uses
    `clamp(0.01 spd)`. The direct calls check it: they send with speeds from 0.5 to 100, where the two differ.
  - Constants: 0x795e20 is -1.0 (not +1.0): it is the wind's numerator (`-1 / inverse_mass`) and the spin's lower clamp.
  - The ToWorld and MB560 records are logged in return order: their own nested calls (an S30 identity and an MB560
    walk) come before the record of ToWorld, so the test counts only the calls after it.
  - Not ported: the null branches. The asm tests the keeper's sum (`keeper + 0x160a0`) and reads the wind at `+0x10`
    past it, so a null keeper faults there too (the C++ `pp != 0`); the velocity null check (`this == 0xfffffe50`) is
    for a car that is never at that address.
  - The drive never reaches the sends (the penetration is 0), so the send path is checked by the direct calls only.
- `DragAndSpinDamp` (0x4e7dc0): ported in `drag.rs`, externals answered from the records (MB560 walks, S70, S74, the
  slot-0 gate). Open:
  - Fourth lane: modelled as three lanes. S70 (0x529340) and S74 (0x529ea0) write all four lanes into the car at
    +0x370 and +0x310 (lane 3 at +0x37c and +0x31c). Cross never writes lane 3 of its output, so S74's lane 3 is a stale
    stack word: 0x51b160 in every drive snapshot (a return address), 0x48ce18 in the direct harness. Runtime check
    (unicorn read hook on car+0x31c and car+0x37c over the drive step, nine snapshots, the car at 0xbf1d7b8): no read of
    either word. Lanes 0 to 2 are read (0x310, 0x318, 0x370, 0x378). Static readers exist in other objects' code (for
    example 0x531b20, which is not this car); the step never reaches them for this car. Lane 3 is not in the physics
    state in the drive; a path outside these steps (a crash or reset) is not checked.
  - Slot 0 (0x519950) is a getter (it returns `car + 0x38`, or 0x73cba0 when that is null) with no effect, so the gate
    costs nothing in the port. The records also show it before S70 and S74 when the gate is on (2 extra per gated
    call). The asm makes no such call, so this is a harness artifact (not answered; the test uses the leading record).
  - Null checks in the asm (the velocity pointer is null only for `this == -0x1b0`, the angular velocity pointer only
    for `this == -0x180`) are not ported: a car is never at those addresses.
  - The S30 identity (0x519c00) leaves its pushed buffer (`ret` with no immediate), and MB560's third argument is that
    buffer. The spec's dispatch `popped` for slot 12 is 0 (the real identity), not 4.

## Fuzzed direct calls

`ToQuat` is called inside the step, where `--fuzz` does not reach, so its rare branches come from a direct run: the
scratch runner (not committed) starts the call at its entry with random matrices (the pool of `quat_fuzz` in
`harness/vector_specs/car_body.py`), `this` = the car of the snapshot's own call, and writes the usual record format.
`harness/vector_specs/car_body.py` also has `quat_fuzz` for `--fuzz` on calls made outside the step.

## Module layout

`layout/`: the only raw-offset code, one file per object: `car.rs` (the car's own words, the `CReal` record, the
`Links` store), `car_parts.rs` (the parts the ports find: links, passengers, sensors, drive, wheels, grid, seed block),
`car_calls.rs` (the car's sequence functions), `construct.rs` (the constructors), `body_points.rs` (`CCarBody`'s
scatters and `CState::Vf44x`), `damage_ctor.rs` and `damage_handler.rs` (the damage objects). The other modules are
the ported functions, one per batch row above (`accessors.rs`, `aero.rs`, `drag.rs`, `probe.rs`, `transform.rs`,
`damage.rs`, `body.rs`, `light.rs`, `vf6cx.rs` and the rest); the quaternion kernels are the shared `math::quat`.

## Unknown meaning

To fill as the batches land. Known so far: the C++ names of the drive, the wheel and the damage fields are
those of `renames/car-body.csv`; `CCar::Hook1FF0` (`UpdateFfbTorque`) and `Clamp76C0` (`FfbNorm`) are
"inferred" there. Two fields have conflicting names (`0xb0c`, `0x22c`/`0x23c`, `0x24c`, `0x44a0`: see the
conflict rows of the rename map); the port uses working names until the asm settles them.

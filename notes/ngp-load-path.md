# NGP load path: where the `*_NGP` keys land (2026-10-16)

NGP adds no parser. At startup it changes the exe's own LSP key tables (`notes/formats/physics-lsp-fields.md`), so the
car files' `*_NGP` keys are read by the stock `CBase::Parse` path into the stock definition objects:

- Renames: `0x10060db0` (thiscall on a record's name slot: if the name equals the old string, point it at the new
  one) renames 41 stock records, 28 of them in tables NGP leaves in place, for example `CDrive::CSetupOptions` `Gear0OptionsMin` ->
  `CenterDiffMapLockPctMin_NGP`. The stock key is gone; its member takes the NGP value.
- Extended tables: for 8 classes NGP copies the stock table into its `.data`, renames some records, appends its own,
  and points the class dict at the copy (dict `+0xc` table, `+8` count; the setters are `0x10060b40` and
  `0x100609b0`). The appended records use the stock format (`{name, 0, member / 4, 0}`).
- New keys bind members the 1.02 layout already has but no stock key reads: `CCar::CDef` `+0x1f0` and `+0x200`
  (`FrontDownforce_NGP`, `RearDownforce_NGP`) are two of the six `0x52fea0`-constructed members at `+0x1f0..0x240`
  (`src/rbr/RBR_E34_Car.h`). `CBase::Parse` calls each member's own `Parse`, so every bound offset needs a constructed
  member object; checked for these two only. Some offsets lie past the last stock key of their class (3 in
  `CDrive::CDef`, 20 in `CEngine::CDef`, 15 in `CTyre::CDef`, 5 in `CWheel::CDef`): whether the 1.02 objects reach
  that far, or NGP enlarges them, is open.
- `DriveTrain_NGP` shares `DriveType`'s member (`CDrive::CDef +0x634`).

In Rust: `rbr_formats::lsp_keys_ngp` (`ngp_key_offset`, generated from this note by `scripts/gen-ngp-lsp-keys.py`).
Every key in NGP's 421 `.lsp` files resolves through it (`all_ngp_lsp_keys_resolve_or_are_ngp`), except two stock
keys NGP renames while its own files keep using them: `SubTics` (`physics.lsp` and the cars, 43 uses; renamed to
`reserved_NGP`) and the wheels' `vecDriveAxleJointPos_VF` (504 uses; renamed to `vecPosOffset_NGP`, same member
`CWheel::CDef +0x2f0`). Under NGP the parser skips them, so those members keep their defaults unless a car sets the
NGP name. Not yet checked against a snapshot's wheel definition.

For Rust: the NGP variant of `lsp_keys` maps each `*_NGP` key to the class and member below, and the 1.02 definition
layout takes the values. Then the per-car values NGP computes from these members into its own `.data` (clutch help
speeds, the 0.94195 group, the tyre group at `10176934`) are the next part of the load path.

How the tables were read: the class dicts in `snap1.snap-10880` (NGP.dll at `0x791a0000` there, `0x10000000` in the
file) point at NGP's copies; each record's name pointer tells stock from NGP. Key names in the car files that the DLL
builds with a format string (`Tq_%02d_NGP`, `Brake*_%s_NGP`, `Bendable_%02d_NGP`) appear here fully expanded.

## Rust: the NGP car from its files (2026-10-17)

`ntweak::plan`/`apply` take the variant; under NGP a key resolves through `lsp_keys_ngp::ngp_key_info` (an NGP record
takes the stock record at its offset, any stock material for a new surface). Besides the tables, NGP changes the load
path in four places, all ported:

- `CEngine::CDef::CDef`: the call at `0x50f247` builds `+0x2820` as a `CEngine::CRealPair` (`0x50bc90`), which adds the
  real at `+0x2830` (`TorqueCut_NGP`, the one key with no member in 1.02), and the default stores at `0x5177f4` and
  `0x51780b` are NOPed. `scripts/gen-engine-def.py` applies these patches and emits the difference (`NGP_WORDS`).
- `E39_Ctor870` (`0x50291b`, the drive definition's `+0x4ec` compound): its last unsigned (`+0x58c`,
  `BrakeWearLimit_NGP`) gets NGP's copy of `CUnsigned`'s vtable (`0x1003c6c0`), whose reader (`0x1003c7f0`) takes the
  unsigned, then up to 30 floats into `+0x59c` (three rows of ten: 1.02's table there, which no 1.02 key reads). The
  car files give them as unnamed rows after the key.
- `CTyre::CDef::CDef`: the stores at `0x530dd3`, `0x530ea2` and `0x530ecc` NOPed (25000, 0.0015708 and 403 at `+0xdc`,
  `+0xfc`, `+0x194` stay 0).
- `CBrakesPage::vf48` (`0x5487d0`) returns at once: the setup's brake copy would overwrite NGP's keys at `+0x4bc`.

Checked by `ngp_definitions_match_the_game` (`rust/rbr-physics/tests/load_defs.rs`, `RBR_NGP_PHYSICS`, `RBR_NGP_LOAD`)
against `ngp-load11c.snap-40` (map 11, slot 5 `s_i2003`, the Fiesta Rally3; map 11 is a snow stage, tyre type 3;
vectors cut by `harness/load_{wheel,car,registry}_refs.py`). The definitions at step 40 equal those of the rpl11
snapshot at step 6438. Every byte matches but those NGP writes after the files, which the test leaves out
(`NGP_LATER`), and the wheels' setup copy (it needs the car instance; the bytes come from the 1.02 build):
`0x1003f5b0` and its callees rewrite the drive type, the engine tables (`0x1005ab10`), `Tq_xx_RPM_NGP` (rpm to rad/s),
`EngineType_NGP` (11 to 1) and scale wheel and tyre values by the car's factor (1.00705 here); the materials' grip
comes from NGP's stage tables; `MaxTimeStep` becomes 1/720. Those are layer 2.

## Per-car values in NGP's `.data`

Compared across three cars on stage 41 (snapshots `cov-fwd41b`, `cov-rwd41` and `cov-rpl41` in
`cachy:~/rbr-ngp-runs`, two per run): the words that stay fixed within a run and differ between cars. Without pointers,
init guards and heap addresses, 42 float words remain, and the ones read so far come from the car's keys:

| words | Fiesta Rally4 / BMW M3 / Fiesta Rally3 | from |
| --- | --- | --- |
| `1017130c`, `10171310`, `10171314` | 457.1, 731.4, 3656.9 / 437.5, 700, 3500 / ... | `0.5712 / InverseMass` x 0.625, 1, 5 |
| `10172ca8`, `10172f58`, `10172f5c`, `10172f64`, `10172f60` | | the same base x 33.75, 35, 75, 1000, and 1 / (base x 40) |
| `10171aa0`, `10171aa4`, `10171acc`, `10171ad0` | 315 / 285 / 400 and multiples | `MaxTorque_NGP` (read as an integer) x 1, 3, 1.3, 13 |
| `1017234c` | 0.25 / 0.3 / 0.25 | `BrakeCoefficient_NGP` / 100 (an integer read, `0x1004ab40`) |
| `10171468`, `1017146c` | 1/1340, 1/20.1 / 1/2040, 1/30.6 / 1/1320, 1/19.8 | 1/x and 1/(0.015 x) of a per-car x, source not found |
| `10176934` + `0x314 * wheel`, `+0x40`, `+0x60`, `+0x80` | -0.045, 0.19, 0.199, 0.099 / -0.048, 0.21, 0.225, 0.101 | the tyre's `HubOffset_NGP`, `TreadWidth_NGP`, `CarcassWidth_NGP`, `SideWallHeight_NGP` |

Not read yet: `10171aa8`/`10171aac` (`0x1004ab40` sets 500 and 3500, then scales them), `10172db0`/`10172db4`, and the words at
`10178138`.. (these drift slightly, likely state). The writers: `0x1003f5b0` (per-car init; called from `0x100f06b0`,
a listener NGP registers at startup) and its callees `0x1004ab40` (engine, 14 of the words), `0x1005e6b0` (with
`0x10059960`, `0x1005ab10`), `0x10045230`, `0x1005d050`. The decompilation reads cleanly, so these are ports from the
binary like the step functions, checked by comparing NGP's `.data` after load.

## Rust: the per-car init (layer 2)

`rbr_physics::ngp::car_init` ports `0x1003f5b0` and its callees, plus the tyre-definition copy `0x100f1690` (a car-load
listener: the four tyre definitions into `0x101768c8`, 0x314 bytes each). NGP's seven built-in tyre types come from
`scripts/gen-ngp-tyre-types.py` (`ngp/tyre_types.rs`). `--boot --ngp-physics` maps NGP's `.data` zeroed at the
reference base `0x791a0000` and copies the tyres after the car's placements. After `UpdateA720` it runs the init, as
`0x100f06b0` does when the race starts. The game's setup pointers (`0x7ecd00`, `0x7ecd04`) are stored for both variants.

Check: `harness/ngp_init_diff.py IMAGE ngp-data/load40`, with `$RBR_NGP` set, on the boot's `--dump-image`. Every
word the init writes matches `snap-40`, in the definitions and in `.data`, except:

- the atmosphere words (`10171ad8`.., `101767c8`..), which follow the car's height at runtime;
The tyre definitions' `+0x10c` (`FlexHeatFtr_NGP`) is 0 in the game: NGP's third NOP in `CTyre::CDef::CDef`
(0x530ecc) drops the store of its default 5, not that of `+0x194` (403, which stays). No car file sets the key. It
scales NGP's inclination heat factor (`ngp::inclination::heat_scale`), so the boot's old 5 diverged slot 6 at step
3228.

The tyre copies in the game differ from the boot's in words the init does not read: the left rear's type is 3 and the
others' 0, and the rears hold tables at `+0x2c4`. The copy seems to be taken at another time than the boot's.

The init's constants must be the DLL's exact bits. `9.1e-7` and `0.06615` are 1 ulp from Rust's literals, which put
the engine tables and the tyre heat factors 1 ulp off.

Not ported:

- the CRC check of the car files (`0x100e88e0`, `0x10041810`; on a mismatch the drive definition's `+0x654` is set to 3);
- the force-feedback ini percentages;
- the listener unlinks;
- the idle table's shared count (`0x101767ec`) and the game object's two words.

NGP's heap objects (2026-10-17): the engine init builds the idle table `0x101767e8` (vtable `0x10148930`, the knot count,
then the speeds and values as two `std::vector<double>`) from the first `Tq_xx_RPM_NGP`/`Tq_xx_NGP` point and those with
a positive value (`0x10003a40` per point). The speed ring `0x101767b0` is a `std::deque<float>` NGP's static init
(`0x10001980`) fills with 107 zeros: 27 blocks of four, a map of 32. The engine wrapper pops and pushes one speed per
step, so the first steps allocate blocks 27 to 31 (NGP's `operator new` `0x1011fe32`, a pure-host stand-in). Without the
table the wrapper skips its idle terms, which put the engine's words off from the first frame.

The car definition's `+0x1488` keeps the file address of NGP's table vtable (`ntweak::layout::NGP_TABLE_VTABLE`).

## Rust: the boot drive (layers 3 and 4)

`rbr-standalone --boot --ngp-physics` also maps the exe's `.text` and applies NGP's 462 edits to the exe after the loads
(`ngp/exe_patches.rs`, `scripts/gen-ngp-exe-patches.py` from `ngp-load11c.snap-40`; the harness's own hooks are left
out). It places NGP's three collision-system copies at the reference's heap variable `0x01a8f228`, which the patched
reads point at (`ngp::stage::collision_copies`), the speed ring and the idle table. After the per-car init it runs NGP's
stage init `0x1004e360` (`ngp::stage::stage_init`): gravity, `MaxTimeStep` 1/720 s and the material table, as words
from `scripts/gen-ngp-stage-materials.py` (unicorn on the function, per value of the definition's `+0x1607c`). The load
budget is the exe's immediate at `0x482bc0`, which NGP patches from 150 to 1500 substeps: 38 settle frames of 40.

The settle runs at 24-bit x87 precision under NGP too (`notes/ngp.md`, "x87 precision").

Check (`cov-rpl11.bin`, stage 11, slot 5, from its `.auto` `11 5 0 3`): from the step-0 snapshot of the same start
(`ngp-s0-11.snap-0`), 293 of 300 steps equal the recording, all 262 driving steps among them. The boot equals that run
step for step once `UpdateMrr`'s own `UpdateSeed` call is dropped. That was a harness artifact: in NGP playback the
plugin's hook of the car's control input (`CarControlHook`) dropped every call for the player's car, `UpdateMrr`'s too,
so those references start with the car's step count, its drive words (`+0x48`, `+0x4c`, `+0x7c`, `+0x80`) and
`car+0x864` at 0, and the boot's `car+0xe18`, `car+0xee0` differ from them. The hook now takes only calls from NGP.dll
(installed in the NGP test install only; the old DLL is `Plugins/RBRRecompHarness.dll.pre-hookfix`). On a recording
with it (`cachy:rbr-ngp-runs/ngp-hf-11.bin`, the same start), the boot runs 2,000 steps with all 1,962 driving steps
equal. Recordings made before the fix (`cov-*`, `ngp-load11c`, `ngp-s0-11`, `ngp-cw11`) keep the artifact.

The 1-ulp drift of `car+0xf24` in the settle frame at step 120 was the engine wrapper's result: its `fdiv` by 1.05
returns in st0, so it rounds to 24 bits in the load (`ngp::engine::result`). The settle is now exact (38/38 frames).
That recording then differs from step 3679, from the step-0 snapshot too: its playback (`idle-pb.bin`) holds ignition
from step 3320, the port cranks 0.5 s of simulation time later, and the original never cranks (NGP's hold is wall
clock, and playback doesn't run at real time). Reference drives for the boot should not press ignition.

The stage x car matrix runs NGP pairs with `harness/matrix.py --ngp` (the autopilot from the boot, presses no
ignition; the original twice in the NGP test install). The harness's recording stops a little past 2 GiB, 133k NGP
steps, so pairs take at most 130k. First pair (`ngp-snow-corolla-11`: stage 11, slot 6, tyre 6): the autopilot ran
300k steps without a decline, the original's two runs are identical over their 132,963 steps (noise mask), and the
boot differed from step 120 (the third settle frame), where slot 5 on tyre 3 (`ngp-hf-11`) is exact. Three causes,
fixed: NGP's edits to `CSpringDamper::Step` (stops always damped, its per-car fade constants; `rbr-hybrid/bound.csv`,
0x52e090), `CHingedRigidBody::SimCF10`/`SimEA80` and `E51Wheel::M3AEF0`'s gain rounding at the thread's precision, and the boot's load
at PC=24 (the game thread's) with the tyre definitions' `+0x10c` 0 (above). Past step 50k: `CollideInit`, `CGround::VfE8` and
`CCar::Sampler54A0` read NGP's collision copy through the patched operands, and `CDrive::M9BC0` (run through NGP's
trampoline) calls NGP's rewrites of its leaves, `M7D60`'s (`0x10045fa0`) included (`ngp::drive::clutch_state`). The recording is
`cachy:rbr-ngp-runs/ngp-matrix/drive-ngp-snow-corolla-11.bin.zst`.

## Extended tables

### CCar::CDef (dict `0x8ef73c`, NGP table `0x10171318`, 20 records: 16 stock, 4 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `reserved_NGP` | `+0x4444` | `SubTics` (renamed) |
| `ForceFeedbackSensitivity_NGP` | `+0x4494` | `MaxFFTorque` (renamed) |
| `FrontDownforce_NGP` | `+0x1f0` | new |
| `RearDownforce_NGP` | `+0x200` | new |

### CDrive::CDef (dict `0x1659134`, NGP table `0x10171480`, 69 records: 14 stock, 55 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `ShiftLatency_NGP` | `+0xa58` | `WheelInertia` (renamed) |
| `DriveTrain_NGP` | `+0x634` | shares `DriveType` |
| `BrakeMass_LF_NGP` | `+0x15c` | new |
| `BrakeConvFtr_LF_NGP` | `+0x16c` | new |
| `BrakeLayerMassPct_LF_NGP` | `+0x17c` | new |
| `BrakeRadiusEff_LF_NGP` | `+0x18c` | new |
| `BrakeSpecHeat_LF_NGP` | `+0x19c` | new |
| `BrakeHeatAbsRatio_LF_NGP` | `+0x1ac` | new |
| `BrakePlateArea_LF_NGP` | `+0x1bc` | new |
| `BrakeVentArea_LF_NGP` | `+0x1cc` | new |
| `BrakeIntConvec_LF_NGP` | `+0x1dc` | new |
| `BrakePistonArea_LF_NGP` | `+0x1ec` | new |
| `BrakeMass_RF_NGP` | `+0x24c` | new |
| `BrakeConvFtr_RF_NGP` | `+0x25c` | new |
| `BrakeLayerMassPct_RF_NGP` | `+0x26c` | new |
| `BrakeRadiusEff_RF_NGP` | `+0x27c` | new |
| `BrakeSpecHeat_RF_NGP` | `+0x28c` | new |
| `BrakeHeatAbsRatio_RF_NGP` | `+0x29c` | new |
| `BrakePlateArea_RF_NGP` | `+0x2ac` | new |
| `BrakeVentArea_RF_NGP` | `+0x2bc` | new |
| `BrakeIntConvec_RF_NGP` | `+0x2cc` | new |
| `BrakePistonArea_RF_NGP` | `+0x2dc` | new |
| `BrakeMass_LR_NGP` | `+0x33c` | new |
| `BrakeConvFtr_LR_NGP` | `+0x34c` | new |
| `BrakeLayerMassPct_LR_NGP` | `+0x35c` | new |
| `BrakeRadiusEff_LR_NGP` | `+0x36c` | new |
| `BrakeSpecHeat_LR_NGP` | `+0x37c` | new |
| `BrakeHeatAbsRatio_LR_NGP` | `+0x38c` | new |
| `BrakePlateArea_LR_NGP` | `+0x39c` | new |
| `BrakeVentArea_LR_NGP` | `+0x3ac` | new |
| `BrakeIntConvec_LR_NGP` | `+0x3bc` | new |
| `BrakePistonArea_LR_NGP` | `+0x3cc` | new |
| `BrakeMass_RR_NGP` | `+0x42c` | new |
| `BrakeConvFtr_RR_NGP` | `+0x43c` | new |
| `BrakeLayerMassPct_RR_NGP` | `+0x44c` | new |
| `BrakeRadiusEff_RR_NGP` | `+0x45c` | new |
| `BrakeSpecHeat_RR_NGP` | `+0x46c` | new |
| `BrakeHeatAbsRatio_RR_NGP` | `+0x47c` | new |
| `BrakePlateArea_RR_NGP` | `+0x48c` | new |
| `BrakeVentArea_RR_NGP` | `+0x49c` | new |
| `BrakeIntConvec_RR_NGP` | `+0x4ac` | new |
| `BrakePistonArea_RR_NGP` | `+0x4bc` | new |
| `BrakeTempThresholdLow_NGP` | `+0x4ec` | new |
| `BrakeTempThresholdHigh_NGP` | `+0x4fc` | new |
| `BrakeTempThresholdMid_NGP` | `+0x50c` | new |
| `BrakeTempWearThresholdLow_NGP` | `+0x51c` | new |
| `BrakeWearFactor_NGP` | `+0x52c` | new |
| `BrakeForceWearFactor_NGP` | `+0x53c` | new |
| `LowBrakeTempCOF_NGP` | `+0x54c` | new |
| `MidBrakeTempCOF_NGP` | `+0x55c` | new |
| `HighBrakeTempCOF_NGP` | `+0x56c` | new |
| `WornBrakeCOF_NGP` | `+0x57c` | new |
| `BrakeWearLimit_NGP` | `+0x58c` | new |
| `GearActuatorDisengageTime_NGP` | `+0xa68` | new |
| `GearActuatorEngageTime_NGP` | `+0xa78` | new |

### CDrive::CSetup (dict `0x165915c`, NGP table `0x101718d0`, 28 records: 17 stock, 11 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `HighSpeedBreakReboundFront_NGP` | `+0x4bc` | new |
| `HighSpeedDampingReboundFront_NGP` | `+0x4cc` | new |
| `HighSpeedBreakReboundRear_NGP` | `+0x4dc` | new |
| `HighSpeedDampingReboundRear_NGP` | `+0x4ec` | new |
| `BumpStopStiffnessFront_NGP` | `+0x4fc` | new |
| `BumpStopDampingBumpFront_NGP` | `+0x50c` | new |
| `BumpStopDampingReboundFront_NGP` | `+0x51c` | new |
| `BumpStopStiffnessRear_NGP` | `+0x52c` | new |
| `BumpStopDampingBumpRear_NGP` | `+0x53c` | new |
| `BumpStopDampingReboundRear_NGP` | `+0x54c` | new |
| `HandbrakePercentage_NGP` | `+0x56c` | new |

### CEngine::CDef (dict `0x16591b0`, NGP table `0x10171ae8`, 133 records: 17 stock, 116 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `EngineType_NGP` | `+0x26ec` | shares `Turbo` |
| `TurboInertia_NGP` | `+0xc` | new |
| `TurboCompressorCoefficient_NGP` | `+0x1c` | new |
| `TurboTurbineCoefficient_NGP` | `+0x2c` | new |
| `TurboMinBlowOffPressure_NGP` | `+0x3c` | new |
| `TurboMaxBlowOffPressure_NGP` | `+0x4c` | new |
| `TurboBlowOffCoefficient_NGP` | `+0x5c` | new |
| `TurboMinWasteGatePressure_NGP` | `+0x6c` | new |
| `TurboMaxWasteGatePressure_NGP` | `+0x7c` | new |
| `TurboWasteGateCoefficient_NGP` | `+0x8c` | new |
| `TurboLinearFrictionFactor_NGP` | `+0xac` | new |
| `TurboSquareFrictionFactor_NGP` | `+0xbc` | new |
| `TurboMaxOilFlowPressure_NGP` | `+0xcc` | new |
| `TurboMinOilFlowPressure_NGP` | `+0xdc` | new |
| `MaxOilGalleryFlowPressure_00_NGP` | `+0x10c` | new |
| `MinOilGalleryFlowPressure_00_NGP` | `+0x11c` | new |
| `MinOilFlowRatePercentage_00_NGP` | `+0x12c` | new |
| `MaxOilGalleryFlowPressure_01_NGP` | `+0x14c` | new |
| `MinOilGalleryFlowPressure_01_NGP` | `+0x15c` | new |
| `MinOilFlowRatePercentage_01_NGP` | `+0x16c` | new |
| `MaxOilGalleryFlowPressure_02_NGP` | `+0x18c` | new |
| `MinOilGalleryFlowPressure_02_NGP` | `+0x19c` | new |
| `MinOilFlowRatePercentage_02_NGP` | `+0x1ac` | new |
| `MaxOilGalleryFlowPressure_03_NGP` | `+0x1cc` | new |
| `MinOilGalleryFlowPressure_03_NGP` | `+0x1dc` | new |
| `MinOilFlowRatePercentage_03_NGP` | `+0x1ec` | new |
| `Tq_00_RPM_NGP` | `+0x2244` | new |
| `Tq_01_RPM_NGP` | `+0x2264` | new |
| `Tq_02_RPM_NGP` | `+0x2284` | new |
| `Tq_03_RPM_NGP` | `+0x22a4` | new |
| `Tq_04_RPM_NGP` | `+0x22c4` | new |
| `Tq_05_RPM_NGP` | `+0x22e4` | new |
| `Tq_06_RPM_NGP` | `+0x2304` | new |
| `Tq_07_RPM_NGP` | `+0x2324` | new |
| `Tq_08_RPM_NGP` | `+0x2344` | new |
| `Tq_09_RPM_NGP` | `+0x2364` | new |
| `Tq_10_RPM_NGP` | `+0x2384` | new |
| `Tq_11_RPM_NGP` | `+0x23a4` | new |
| `Tq_12_RPM_NGP` | `+0x23c4` | new |
| `Tq_13_RPM_NGP` | `+0x23e4` | new |
| `Tq_14_RPM_NGP` | `+0x2404` | new |
| `Tq_15_RPM_NGP` | `+0x2424` | new |
| `Tq_00_NGP` | `+0x2254` | shares `VE00` |
| `Tq_01_NGP` | `+0x2274` | shares `VE01` |
| `Tq_02_NGP` | `+0x2294` | shares `VE02` |
| `Tq_03_NGP` | `+0x22b4` | shares `VE03` |
| `Tq_04_NGP` | `+0x22d4` | shares `VE04` |
| `Tq_05_NGP` | `+0x22f4` | shares `VE05` |
| `Tq_06_NGP` | `+0x2314` | shares `VE06` |
| `Tq_07_NGP` | `+0x2334` | shares `VE07` |
| `Tq_08_NGP` | `+0x2354` | shares `VE08` |
| `Tq_09_NGP` | `+0x2374` | shares `VE09` |
| `Tq_10_NGP` | `+0x2394` | shares `VE10` |
| `Tq_11_NGP` | `+0x23b4` | shares `VE11` |
| `Tq_12_NGP` | `+0x23d4` | shares `VE12` |
| `Tq_13_NGP` | `+0x23f4` | shares `VE13` |
| `Tq_14_NGP` | `+0x2414` | shares `VE14` |
| `Tq_15_NGP` | `+0x2434` | shares `VE15` |
| `TurboTurbineBackPressure_00_AV_NGP` | `+0x2444` | new |
| `TurboTurbineBackPressure_01_AV_NGP` | `+0x2464` | new |
| `TurboTurbineBackPressure_02_AV_NGP` | `+0x2484` | new |
| `TurboTurbineBackPressure_03_AV_NGP` | `+0x24a4` | new |
| `TurboTurbineBackPressure_04_AV_NGP` | `+0x24c4` | new |
| `TurboTurbineBackPressure_05_AV_NGP` | `+0x24e4` | new |
| `TurboTurbineBackPressure_06_AV_NGP` | `+0x2504` | new |
| `TurboTurbineBackPressure_07_AV_NGP` | `+0x2524` | new |
| `TurboTurbineBackPressure_08_AV_NGP` | `+0x2544` | new |
| `TurboTurbineBackPressure_09_AV_NGP` | `+0x2564` | new |
| `TurboTurbineBackPressure_10_AV_NGP` | `+0x2584` | new |
| `TurboTurbineBackPressure_11_AV_NGP` | `+0x25a4` | new |
| `TurboTurbineBackPressure_12_AV_NGP` | `+0x25c4` | new |
| `TurboTurbineBackPressure_13_AV_NGP` | `+0x25e4` | new |
| `TurboTurbineBackPressure_14_AV_NGP` | `+0x2604` | new |
| `TurboTurbineBackPressure_15_AV_NGP` | `+0x2624` | new |
| `TurboTurbineBackPressure_00_NGP` | `+0x2454` | new |
| `TurboTurbineBackPressure_01_NGP` | `+0x2474` | new |
| `TurboTurbineBackPressure_02_NGP` | `+0x2494` | new |
| `TurboTurbineBackPressure_03_NGP` | `+0x24b4` | new |
| `TurboTurbineBackPressure_04_NGP` | `+0x24d4` | new |
| `TurboTurbineBackPressure_05_NGP` | `+0x24f4` | new |
| `TurboTurbineBackPressure_06_NGP` | `+0x2514` | new |
| `TurboTurbineBackPressure_07_NGP` | `+0x2534` | new |
| `TurboTurbineBackPressure_08_NGP` | `+0x2554` | new |
| `TurboTurbineBackPressure_09_NGP` | `+0x2574` | new |
| `TurboTurbineBackPressure_10_NGP` | `+0x2594` | new |
| `TurboTurbineBackPressure_11_NGP` | `+0x25b4` | new |
| `TurboTurbineBackPressure_12_NGP` | `+0x25d4` | new |
| `TurboTurbineBackPressure_13_NGP` | `+0x25f4` | new |
| `TurboTurbineBackPressure_14_NGP` | `+0x2614` | new |
| `TurboTurbineBackPressure_15_NGP` | `+0x2634` | new |
| `CylinderVolume_NGP` | `+0x2644` | new |
| `DissipatedHeatFraction_NGP` | `+0x269c` | new |
| `InletPipeAirFlowCoefficient_NGP` | `+0x26ac` | new |
| `OutletPipeAirFlowCoefficient_NGP` | `+0x26bc` | new |
| `InletPipeArea_NGP` | `+0x26cc` | new |
| `OutletPipeArea_NGP` | `+0x26dc` | new |
| `IdleSpeedThrottle_NGP` | `+0x26fc` | new |
| `TurboHosingDamageLossCoefficient_NGP` | `+0x270c` | new |
| `EngineHeatTransferCoefficient_NGP` | `+0x2728` | new |
| `EngineCoolantDuctArea_NGP` | `+0x2738` | new |
| `EngineCoolantHeatStateMassUpperBound_NGP` | `+0x2758` | new |
| `ThermostatOpeningTemperature_NGP` | `+0x2768` | new |
| `WaterPumpMassFlowRate_NGP` | `+0x2778` | new |
| `RadiatorCoolantHeatStateMassUpperBound_NGP` | `+0x2790` | new |
| `RadiatorCoolantMassLowerBound_NGP` | `+0x27a0` | new |
| `RadiatorCoolantMassUpperBound_NGP` | `+0x27b0` | new |
| `RadiatorArea_NGP` | `+0x27c0` | new |
| `BaseAirCooling_NGP` | `+0x27d0` | new |
| `VelocityAirCooling_NGP` | `+0x27e0` | new |
| `OilCapacity_NGP` | `+0x2810` | new |
| `OilPumpOutput_NGP` | `+0x2820` | new |
| `TorqueCut_NGP` | `+0x2830` | new |
| `EngineMass_NGP` | `+0x2870` | new |
| `CylinderOilFrictionTorqueCoefficient_NGP` | `+0x2840` | new |
| `HighTemperatureFrictionLossTorqueCoefficient_NGP` | `+0x2850` | new |
| `FrictionLossTemperatureThreshold_NGP` | `+0x2860` | new |

### CMaterialManager (dict `0x1659ad8`, NGP table `0x10172350`, 149 records: 134 stock, 15 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `Bendable_01_NGP` | `+0x1146c` | new |
| `Bendable_02_NGP` | `+0x115cc` | new |
| `Bendable_03_NGP` | `+0x1172c` | new |
| `Bendable_04_NGP` | `+0x1188c` | new |
| `Bendable_05_NGP` | `+0x119ec` | new |
| `Bendable_06_NGP` | `+0x11b4c` | new |
| `Bendable_07_NGP` | `+0x11cac` | new |
| `Bendable_08_NGP` | `+0x11e0c` | new |
| `Bendable_09_NGP` | `+0x11f6c` | new |
| `Bendable_10_NGP` | `+0x120cc` | new |
| `Bendable_11_NGP` | `+0x1222c` | new |
| `Bendable_12_NGP` | `+0x1238c` | new |
| `Bendable_13_NGP` | `+0x124ec` | new |
| `Bendable_14_NGP` | `+0x1264c` | new |
| `Bendable_15_NGP` | `+0x127ac` | new |

### CTyre::CDef (dict `0x165f1b0`, NGP table `0x10172cb0`, 16 records: 1 stock, 15 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `CarcassStiffnessLateral_NGP` | `+0x40` | `CarcassStiffnessLateral` (renamed) |
| `CarcassStiffnessTorsional_NGP` | `+0x50` | `CarcassStiffnessTorsional` (renamed) |
| `CarcassWidth_NGP` | `+0xc0` | `CarcassWidth` (renamed) |
| `CarcassRadius_NGP` | `+0x70` | `CarcassRadius` (renamed) |
| `TreadDepth_NGP` | `+0x80` | `TreadDepth` (renamed) |
| `TreadWidth_NGP` | `+0xa0` | `TreadWidth` (renamed) |
| `TreadCoverage_NGP` | `+0xb0` | `TreadCoverage` (renamed) |
| `Performance_NGP` | `+0x30` | `ReferenceFriction` (renamed) |
| `HubOffset_NGP` | `+0x60` | `HubOffset` (renamed) |
| `CarcassThickness_NGP` | `+0x90` | new |
| `SideWallHeight_NGP` | `+0xe0` | new |
| `PressureRatio_NGP` | `+0x20` | new |
| `CarcassStiffnessVertical_NGP` | `+0xd0` | new |
| `PressureFuncFtr_NGP` | `+0xf0` | new |
| `FlexHeatFtr_NGP` | `+0x100` | new |

### CVehicleControlUnit::CDef (dict `0x165f214`, NGP table `0x10172db8`, 26 records: 23 stock, 3 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `UpShiftDelay_NGP` | `+0x14c` | new |
| `DownShiftDelay_NGP` | `+0x15c` | new |
| `ClutchHelpDecrement_NGP` | `+0x16c` | new |

### CWheel::CDef (dict `0x165f2cc`, NGP table `0x10172f68`, 33 records: 26 stock, 7 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `vecPosOffset_NGP` | `+0x2f0` | `vecDriveAxleJointPos_VF` (renamed) |
| `BumpStopRebound_NGP` | `+0x1f0` | new |
| `TopMountDamageForceThreshold_NGP` | `+0x924` | new |
| `TopMountDamageMaxForce_NGP` | `+0x934` | new |
| `AxisInclinationDamageAngle_NGP` | `+0x944` | new |
| `HubDamageTorqueThreshold_NGP` | `+0x954` | new |
| `HubDamageMaxTorque_NGP` | `+0x964` | new |

## Renamed in place in the stock tables

Read back from the stock tables in `snap1.snap-10880`.

| class | key | stock key | member |
| --- | --- | --- | --- |
| CDrive::CSetupOptions | `FrontBrakeVelSlope_NGP` | `FrontBrakes` | `+0xdbc` |
| CDrive::CSetupOptions | `RearBrakeVelSlope_NGP` | `RearBrakes` | `+0xf3c` |
| CDrive::CSetupOptions | `BumpStopStiffnessMin_NGP` | `FrontBrake1Mass` | `+0xe7c` |
| CDrive::CSetupOptions | `BumpStopStiffnessMax_NGP` | `FrontBrake1Inertia` | `+0xe8c` |
| CDrive::CSetupOptions | `BumpStopStiffnessStep_NGP` | `FrontBrake1Diameter` | `+0xe9c` |
| CDrive::CSetupOptions | `BumpStopDampingMin_NGP` | `FrontBrake1RadiusEff` | `+0xeac` |
| CDrive::CSetupOptions | `BumpStopDampingMax_NGP` | `FrontBrake1SpecHeat` | `+0xebc` |
| CDrive::CSetupOptions | `BumpStopDampingStep_NGP` | `FrontBrake1HeatAbsRatio` | `+0xecc` |
| CDrive::CSetupOptions | `CenterDiffMapLockPctMin_NGP` | `Gear0OptionsMin` | `+0xbac` |
| CDrive::CSetupOptions | `CenterDiffMapLockPctMax_NGP` | `Gear0OptionsMax` | `+0xbbc` |
| CDrive::CSetupOptions | `FrontDiffMapLockPctMin_NGP` | `Gear1OptionsMin` | `+0xbcc` |
| CDrive::CSetupOptions | `FrontDiffMapLockPctMax_NGP` | `Gear1OptionsMax` | `+0xbdc` |
| CDrive::CSetupOptions | `RearDiffMapLockPctMin_NGP` | `Gear2OptionsMin` | `+0xbec` |
| CDrive::CSetupOptions | `RearDiffMapLockPctMax_NGP` | `Gear2OptionsMax` | `+0xbfc` |
| CDrive::CSetupOptions | `Features_NGP` | `Gear3OptionsMin` | `+0xc0c` |
| CDrive::CSetupOptions | `TorqueSplit_NGP` | `Gear3OptionsMax` | `+0xc1c` |
| CDrive::CSetupOptions | `FrontTyreCoolingFtr_NGP` | `Gear4OptionsMin` | `+0xc2c` |
| CDrive::CSetupOptions | `RearTyreCoolingFtr_NGP` | `Gear4OptionsMax` | `+0xc3c` |
| CDrive::CSetupOptions | `TyreHeatFlowFtr_NGP` | `Gear5OptionsMin` | `+0xc4c` |
| CDrive::CSetupOptions | `DriveTrainLoss_NGP` | `Gear5OptionsMax` | `+0xc5c` |
| CDrive::CSetupOptions | `TyrePerformancePctT_NGP` | `Gear6OptionsMin` | `+0xc6c` |
| CDrive::CSetupOptions | `TyrePerformancePctG_NGP` | `Gear6OptionsMax` | `+0xc7c` |
| CDrive::CSetupOptions | `TyrePerformancePctS_NGP` | `Gear7OptionsMin` | `+0xc8c` |
| CEngine::CSetup | `Features_NGP` | `Dummy` | `+0xc` |
| CEngine::CSetupOptions | `IdleRPM_NGP` | `DummyMin` | `+0xc` |
| CEngine::CSetupOptions | `MaxTorque_NGP` | `DummyMax` | `+0x1c` |
| CEngine::CSetupOptions | `BrakeCoefficient_NGP` | `DummyStep` | `+0x2c` |
| CPhysics::CDef | `PhysicsRevision_NGP` | `MaxExpirableCount` | `+0x16070` |

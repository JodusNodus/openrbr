# NGP physics `.lsp` keys (NGP 7.4.778)

NGP (a plugin: `Plugins/NGP.dll`, never executed here — read-only
evidence) extends the 1.02 physics with its own `.lsp` keys, all suffixed
`_NGP`. Scope here is the data NGP ships for the 42 RSF cars plus its
`physics.lsp`:

- `original/ngp/7.4.778/physics.rbz` (zip): only `Physics/physics.lsp`
  differs from retail; every other entry is byte-identical to retail 1.0
  (compared by name/size/CRC).
- `original/ngp/cars-rsf/<car>/`: 42 cars, each with `common.lsp`,
  `gravel/snow/tarmac.lsp`, `r_gravel/r_snow/r_tarmac.lsp`,
  `setups/d_gravel/d_snow/d_tarmac.lsp` (420 files), plus `repair.ini`
  (different format, out of scope) and a plain-text file named after the
  car (name / revision+date / spec date / 3D-model credit lines; not
  parsed).
- `strings -a -n 6` of `NGP.dll` corroborates key names (never run).

## Reader compatibility

All 421 NGP files (`Physics/physics.lsp` from the zip + the 420 car
files) parse with the existing generic `lsp::parse`: **0 failures, no
grammar change needed**. Shapes classify exactly like retail (168
`common`+`surface` Def files in cars-rsf, 126 `r_options`, 126 `setup`,
1 `physics`). No new tolerated-syntax case was found, so neither
`rust/rbr-formats/src/lsp.rs` nor `notes/formats/physics-lsp.md` changed.
No unknown `+`/`-` flag bits and no non-`_NGP` unknown keys anywhere.

Key inventory method: the same block→class walk as
`rust/rbr-formats/tests/lsp_keys.rs` (bare key checked against the
`lsp_keys::Class` tables of its block, parents included). Result:
**153 distinct unknown keys, every one `*_NGP`** (154 key/class uses —
`Features_NGP` appears in two classes), plus 15 new surface names in
`physics.lsp` (`Bendable_01…15_NGP`; surface instance names are not loader
keys, so they are diffed separately, not walked). The NGP test in
`tests/lsp_keys.rs` (gated on `RBR_NGP`)
pins this: every key must resolve in 1.02 tables or be in
`lsp_keys::NGP_KEYS`, every material name must resolve in the
`MaterialManager` table or be in `lsp_keys::NGP_SURFACES`.

Conventions below: `files` = cars-rsf file kinds using the key (`c` =
`common.lsp` ×42, `g/s/t` = surface files ×126, `r` = `r_*` ×126, `d` =
`d_*` ×126, `p` = `physics.lsp`); `cars` = how many of the 42 cars use
it. Shape is `1num` (one scalar), `int` (scalar, always integral),
`vec3`. Ranges are min..max of the key's own value across all 42 cars.
DLL = verbatim presence in `NGP.dll` strings (`*` marks the 2 keys with
only a prefixed hit — see below). Guesses about meaning are marked
**(guess)**; names alone are not semantics.

## New keys by class (153)

### `CarDef` — `common.lsp` (`DefManager/Car`), 3 keys

| key | files | shape | range | DLL |
| --- | --- | --- | --- | --- |
| `FrontDownforce_NGP` | c (23 cars) | int | 45..1800 | yes |
| `RearDownforce_NGP` | c (26 cars) | int | 180..1900 | yes |
| `ForceFeedbackSensitivity_NGP` | c (42) + p | num | 75..130 (cars), 100.0 (physics.lsp) | yes |

19 cars omit `FrontDownforce_NGP`, 16 omit `RearDownforce_NGP`
(older/rally-school-era cars); the rest set both.

### `DriveDef` — `common.lsp` (`DefManager/Car/Drive`), 54 keys

Brake hardware/thermal per-corner keys (`_LF/_RF/_LR/_RR`) plus wear,
temperature and shift-actuator keys. A subset (`BrakeMass`, `BrakePlateArea`,
`BrakeRadiusEff`, `BrakeVentArea`, front always, rear sometimes;
`BrakePistonArea`, front/rear sometimes) is re-specified per surface in
some cars' `tarmac.lsp` only (35/15/19/13 cars), i.e. tarmac brake
overrides; all other keys are `common.lsp`-only, all 42 cars.

| key | files | shape | range | DLL |
| --- | --- | --- | --- | --- |
| `BrakeMass_{LF,RF,LR,RR}_NGP` | c+tarmac | num | F 2.545..16.459, R 1.908..16.078 | yes |
| `BrakePistonArea_{LF,RF,LR,RR}_NGP` | c+tarmac | num | F 0.003257..0.006221, R 0.001414..0.005682 | yes |
| `BrakePlateArea_{LF,RF,LR,RR}_NGP` | c+tarmac | num | 0.060745..0.116303 | yes |
| `BrakeRadiusEff_{LF,RF,LR,RR}_NGP` | c+tarmac | num | F 0.105..0.171, R 0.096..0.16 | yes |
| `BrakeVentArea_{LF,RF,LR,RR}_NGP` | c+tarmac | num | 0..0.137238 | yes |
| `BrakeConvFtr_{LF,RF,LR,RR}_NGP` | c | num | F 22.5..70, R 10..30 | yes |
| `BrakeHeatAbsRatio_{LF,RF,LR,RR}_NGP` | c | num | 0.75 (all cars) | yes |
| `BrakeIntConvec_{LF,RF,LR,RR}_NGP` | c | int | 1200 (all cars) | yes |
| `BrakeLayerMassPct_{LF,RF,LR,RR}_NGP` | c | num | 0.1 (all cars) | yes |
| `BrakeSpecHeat_{LF,RF,LR,RR}_NGP` | c | int | 450 (all cars) | yes |
| `BrakeTempThresholdLow_NGP` | c | int | 273150000 (all cars) | yes |
| `BrakeTempThresholdMid_NGP` | c | int | 803150000..873150000 | yes |
| `BrakeTempThresholdHigh_NGP` | c | int | 1073150000..1103150000 | yes |
| `BrakeTempWearThresholdLow_NGP` | c | int | 523150000 (all cars) | yes |
| `LowBrakeTempCOF_NGP` | c | num | 0.33..0.5 | yes |
| `MidBrakeTempCOF_NGP` | c | num | 0.445..0.61 | yes |
| `HighBrakeTempCOF_NGP` | c | num | 0.01 (all cars) | yes |
| `WornBrakeCOF_NGP` | c | num | 0.1 (all cars) | yes |
| `BrakeWearFactor_NGP` | c | num | 4e-13 (all cars) | yes |
| `BrakeForceWearFactor_NGP` | c | int | 50 (all cars) | yes |
| `BrakeWearLimit_NGP` | c | int | 1500000000 (all cars) | yes |
| `GearActuatorEngageTime_NGP` | c | num | 0.05..0.09 | yes |
| `GearActuatorDisengageTime_NGP` | c | num | 0.025..0.045 | yes |
| `ShiftLatency_NGP` | c | num | 0.095..0.155 | yes |

Names suggest a brake temperature/wear model and shift-actuator timing
**(guess)**; several constants are identical across all 42 cars.

### `EngineDef` — `common.lsp` (`DefManager/Car/Engine`), 52 keys

A torque-curve table plus what look like engine/oil/cooling thermal keys
**(guess)**. All 42 cars, `common.lsp` only.

| key | shape | range | DLL |
| --- | --- | --- | --- |
| `EngineType_NGP` | int | 10 (17 cars) / 11 (25 cars) | yes |
| `Tq_00_RPM_NGP` … `Tq_15_RPM_NGP` | int | 0, then 900..1000 up to 7650..9975 (`_15`) | yes |
| `Tq_00_NGP` … `Tq_15_NGP` | num | 0..0.7 (`_00`) … 0 (`_15`, all cars) | yes |
| `CylinderVolume_NGP` | num | 0.0002498..0.0007485 | yes |
| `DissipatedHeatFraction_NGP` | num | 0.3 (all cars) | yes |
| `InletPipeArea_NGP` | num | 0.00108..0.00215 | yes |
| `IdleSpeedThrottle_NGP` | num | 0.086..0.12 | yes |
| `EngineHeatTransferCoefficient_NGP` | int | 2500 (all cars) | yes |
| `EngineCoolantDuctArea_NGP` | num | 0.6..0.75 | yes |
| `EngineCoolantHeatStateMassUpperBound_NGP` | num | 1.5..2.3 | yes |
| `ThermostatOpeningTemperature_NGP` | int | 353..355 | yes |
| `WaterPumpMassFlowRate_NGP` | num | 2.0..3.1 | yes |
| `RadiatorCoolantHeatStateMassUpperBound_NGP` | num | 5.8..10.5 | yes |
| `RadiatorCoolantMassLowerBound_NGP` | num | 0.3..0.5 | yes |
| `RadiatorCoolantMassUpperBound_NGP` | num | 5.8..10.5 | yes |
| `RadiatorArea_NGP` | num | 1.5..2.5 | yes |
| `BaseAirCooling_NGP` | int | 500 (all cars) | yes |
| `VelocityAirCooling_NGP` | int | 20..22 | yes |
| `OilCapacity_NGP` | num | 3.5..16 | yes |
| `OilPumpOutput_NGP` | num | 0.0002 (all cars) | yes |
| `EngineMass_NGP` | int | 20..60 | yes |
| `TorqueCut_NGP` | num | 0.25..0.5 | yes |

The 16 `Tq_NN`/`Tq_NN_RPM` pairs replace the retail `VE00…VE15` +
`Tq00…Tq95` scheme (both dropped from NGP `common.lsp` — the `r_*`
`Tq00…Tq95` tables are still shipped, see below). `Tq_13…Tq_15_NGP`
are 0 for most cars (curve ends earlier).

### `WheelDef` — surface files (`DefManager/Wheel`), 2 keys, all 42 cars

| key | files | shape | range | DLL |
| --- | --- | --- | --- | --- |
| `vecPosOffset_NGP` | g/s/t | vec3 | −0.08..0.12 per component | yes |
| `BumpStopRebound_NGP` | g/s/t | num | 1.0..1.11 | only `EBumpStopRebound_NGP` * |

### `TyreDef` — surface files (`DefManager/Wheel/Tyre`), 7 keys, all 42 cars

Per-surface overrides shadowing the retail carcass keys (which NGP
surface files drop — see below).

| key | shape | range | DLL |
| --- | --- | --- | --- |
| `Performance_NGP` | num | 0.95..1.015 | yes |
| `CarcassRadius_NGP` | num | 0.26..0.31 | yes |
| `TreadDepth_NGP` | num | 0.01 (all cars/surfaces) | yes |
| `CarcassWidth_NGP` | num | 0.145..0.295 | yes |
| `TreadWidth_NGP` | num | 0.11..0.27 | yes |
| `SideWallHeight_NGP` | num | 0.082..0.14 | yes |
| `HubOffset_NGP` | num | −0.07..−0.028 | yes |

### `DriveSetupOptions` — `r_*` files, 20 keys

| key | cars | shape | range | DLL |
| --- | --- | --- | --- | --- |
| `Features_NGP` | 42 | int | 260..214070 (per-car bitmask; also in `d_*` `Engine`, see below) | yes |
| `DriveTrainLoss_NGP` | 42 | int | 8 / 10 / 12 | yes |
| `FrontBrakeVelSlope_NGP` | 42 | num | 0.0027..0.003 | yes |
| `RearBrakeVelSlope_NGP` | 42 | num | 0.0027..0.003 | yes |
| `CenterDiffMapLockPctMin_NGP` | 42 | int | 0 (all cars) | yes |
| `CenterDiffMapLockPctMax_NGP` | 42 | int | 0..100 | yes |
| `FrontDiffMapLockPctMin_NGP` | 42 | int | 0 (all cars) | yes |
| `FrontDiffMapLockPctMax_NGP` | 42 | int | 0..100 | yes |
| `RearDiffMapLockPctMin_NGP` | 42 | int | 0 (all cars) | yes |
| `RearDiffMapLockPctMax_NGP` | 42 | int | 0..100 | yes |
| `TorqueSplit_NGP` | 4 | int | 50 / 60 / 65 (Escort Cosworth GrpA, Fiesta WRC 2019, i20 WRC 2020, Delta GrpA) | yes |
| `TyrePerformancePctG_NGP` | 9 | int | 103..105 | yes |
| `TyrePerformancePctS_NGP` | 9 | int | 103..105 | yes |
| `TyrePerformancePctT_NGP` | 9 | int | 110..115 | yes |
| `BumpStopStiffnessMin_NGP` | 20 | int | 50000 (all 20) | yes |
| `BumpStopStiffnessMax_NGP` | 20 | int | 500000..750000 | yes |
| `BumpStopStiffnessStep_NGP` | 20 | int | 2500..5000 | yes |
| `BumpStopDampingMin_NGP` | 20 | int | 5000 (all 20) | yes |
| `BumpStopDampingMax_NGP` | 20 | int | 50000..75000 | yes |
| `BumpStopDampingStep_NGP` | 20 | int | 250..500 | yes |

The `r_*` files document `Features_NGP` in a comment (quoted here as
the only stated semantics in the data): drivetrain type + center/front/
rear diff type × 4096-based multiplier (e.g. RWD + mechanical rear =
`5 + 4*4096 = 16389`), plus an NGP feature field with multiplier 65536
(`0` = 2/3-way dampers, `1` = 4-way, `2` = progressive bump stops).
Observed `r_*` values fit (e.g. 16389, 17462, 214070). The 20-car
bump-stop group and the 9-car `TyrePerformancePct` group are the newer
(WRC/R5/Rally2-4) cars; `TyrePerformancePct` cars are a subset of it.

### `EngineSetupOptions` — `r_*` files, 3 keys, all 42 cars

| key | shape | range | DLL |
| --- | --- | --- | --- |
| `IdleRPM_NGP` | int | 1000 (33 cars) / 1200 (9) | yes |
| `MaxTorque_NGP` | int | 160..590 | yes |
| `BrakeCoefficient_NGP` | int | 20 / 25 / 30 | yes |

### `DriveSetup` — `d_*` files, 11 keys

| key | cars | shape | range | DLL |
| --- | --- | --- | --- | --- |
| `HandbrakePercentage_NGP` | 42 | num | 0.56..1.0 | yes |
| `BumpStopStiffnessFront_NGP` | 20 | int | 168000..390000 | yes |
| `BumpStopStiffnessRear_NGP` | 20 | int | 172500..275000 | yes |
| `BumpStopDampingBumpFront_NGP` | 20 | int | 15250..27000 | yes |
| `BumpStopDampingBumpRear_NGP` | 20 | int | 13000..19000 | yes |
| `BumpStopDampingReboundFront_NGP` | 20 | int | 16750..29500 | yes |
| `BumpStopDampingReboundRear_NGP` | 20 | int | 14500..21000 | yes |
| `HighSpeedBreakReboundFront_NGP` | 4 | num | 0.3 (all 4) | yes |
| `HighSpeedBreakReboundRear_NGP` | 4 | num | 0.3 (all 4) | yes |
| `HighSpeedDampingReboundFront_NGP` | 4 | int | 3670..5180 | yes |
| `HighSpeedDampingReboundRear_NGP` | 4 | int | 3720..4640 | yes |

Same 20-car group as the `r_*` bump-stop ranges; the 4-car high-speed
group is Fiesta RS WRC 2014 / Fiesta WRC 2019 / Focus WRC 2006 /
i20 WRC 2020. (Spelling `Break` is in the files.)

### `EngineSetup` — `d_*` files, 1 key

`Features_NGP`: int, 0 in 39 cars' `d_*` files, 1 in 3 (Fiesta RS WRC
2014, Fiesta WRC 2019, i20 WRC 2020). Replaces retail `Dummy` in the
`Engine` setup block (dropped — see below).

### `PhysDef` — `physics.lsp` root, 1 key

`PhysicsRevision_NGP`: int, 778. Only `EBumpStopRebound_NGP`-style
prefixed hit in the DLL (`APhysicsRevision_NGP`), no verbatim hit.

## NGP.dll string evidence

151 of the 153 file keys appear verbatim in `NGP.dll`. The 2 without a
verbatim hit both have a single-letter-prefixed hit nearby in what looks
like a key-name table (NGP names interleaved with the retail names they
extend, e.g. `CarcassRadius_NGP` next to `CarcassRadius`): file
`BumpStopRebound_NGP` vs DLL `EBumpStopRebound_NGP`, file
`PhysicsRevision_NGP` vs DLL `APhysicsRevision_NGP`; likewise file
surface `Bendable_01_NGP` vs DLL `CBendable_01_NGP`.

The DLL names ~100 further `_NGP` keys that no shipped `.lsp` uses —
notably a turbo series (`TurboInertia_NGP`, `TurboCompressorCoefficient_NGP`,
`TurboTurbineBackPressure_00…15_NGP` + `_AV` pairs, wastegate/blowoff
pressure keys), oil-gallery keys (`Min/MaxOilGalleryFlowPressure_00…03_NGP`,
`MinOilFlowRatePercentage_00…03_NGP`), damage keys
(`TopMountDamageForceThreshold/MaxForce_NGP`,
`HubDamageTorqueThreshold/MaxTorque_NGP`,
`AxisInclinationDamageAngle_NGP`, `TurboHosingDamageLossCoefficient_NGP`),
tyre-thermal keys (`TyreHeatFlowFtr_NGP`, `Front/RearTyreCoolingFtr_NGP`,
`FlexHeatFtr_NGP`, `PressureFuncFtr_NGP`, `PressureRatio_NGP`,
`TreadCoverage_NGP`, `FCarcassThickness_NGP`,
`CarcassStiffness{Lateral,Torsional,Vertical}_NGP`), clutch/shift keys
(`UpShiftDelay_NGP`, `DownShiftDelay_NGP`, `ClutchHelpDecrement_NGP`),
drivetrain/friction keys (`DriveTrain_NGP`,
`CylinderOilFrictionTorqueCoefficient_NGP`,
`HighTemperatureFrictionLossTorqueCoefficient_NGP`,
`FrictionLossTemperatureThreshold_NGP`), inlet/outlet pipe airflow keys,
and `reserved_NGP`. All unused here — listed so the test list stays to
what the files use.

## 1.02 keys NGP drops (per role)

Role sizes: `common` 105→102, `surface` 61→52 (+15 newly-used known
keys), `r_options` 251→195 (+1), `setup` 228→226, `physics` 84→62
(resolved `(class, key)` uses; `tyres`/`localphysics`/`misc` have no NGP
counterpart).

- `physics.lsp`: `Car/CAR_ROOT` loses `AntiRollBarStiffness0/1`,
  `EffWheelRadius`, `MaxFFTorque`, `InverseMass`,
  `vecLocalInverseInertiaDiagonal`, and the whole `ControlUnit`
  `Gear0…Gear7` up/downshift table (22 uses). Kept: `DangerousAcceleration`,
  `SubTics`, `MaxTimeStep`, `BrakeWall*`, `ForceMaterial`, `vecGravity`,
  flags, `CollisionSolver`, all material value keys.
- `common.lsp`: loses `CarId`, `DangerousAcceleration`,
  `Drive/WheelInertia` (3). Hinges, mounts, lights, `VE*`→`Tq_*_NGP`
  otherwise intact.
- surface files: `Car` block shrinks to `SetupSpec` (still 0 everywhere)
  + `CarId`; loses `DragCoefficient`, `FrontArea`, `MaxSteeringLock`,
  `SteeringRackRatio`, `SubTics`, `EffWheelRadius`, all `Drive` inertias +
  `MaxHandbrakePressure` + `NumberOfGears`, `Vcu` release/threshold,
  `DoSuspensionSweep`, `iCollisionMesh`, and the retail `Tyre` carcass keys
  (`CarcassRadius/StiffnessLateral/StiffnessTorsional/CarcassWidth/
  HubOffset/ReferenceFriction/TreadDepth/TreadWidth`) — replaced by the
  `*_NGP` tyre keys above (24 uses).
- `r_*`: loses the physical brake-detail keys (`Front/RearBrake0/1` mass,
  inertia, diameter, `RadiusEff`, `SpecHeat`, `HeatAbsRatio`, `PlateArea`,
  `VentArea`, `IntConvec` — replaced by per-corner `*_NGP` keys in
  `common.lsp`), `Front/RearBrakes`, all `GearNOptionsMin/Max`,
  `EngineOptions/DummyMin/Max/Step` (57). The retail `Tq00…Tq95`,
  gear-list, pressure and torque-range tables are all kept.
- `d_*`: loses `Engine/Dummy` (replaced by `Features_NGP`) and
  `Wheel/vecTopMountPosition` (2).

## Known 1.02 keys NGP uses that retail does not

- `SpringDef::DroopStiffness` (known `CReal@+0x9c`, retail-unused): used in
  every NGP surface `SpringDamper` block (600000).
- `VcuDef::Gear0…Gear6` up/downshift: exactly one surface file,
  `Ford_Fiesta_Rally2_ngp7/tarmac.lsp`, carries a `ControlUnit` gear table
  (retail surface files only set release/threshold there).
- `WheelSetupOptions::vecTopMount07`: used in NGP `r_*` files (retail
  stops at `vecTopMount06`).

## New top-level entries in NGP `physics.lsp`

15 new material names under `MaterialManager` (same value keys as
retail, incl. the known `+BENDABLE` bit): `Bendable_01_NGP`…`Bendable_15_NGP`
(ids 201–215, `Type 4`, `+HARD +VALID +BENDABLE`). NGP also uses 4
materials that are already in the 1.02 `MaterialManager` table but that
no retail file uses (`used_in_retail == false` in `lsp_keys.rs`):
`Metal_Barrier`, `Snowwall`, `Gravel_Fine_Shallow_Damp_Clone`,
`Gravel_Fine_Shallow_Wet_Clone`. No retail material removed. Plus the
root `PhysicsRevision_NGP 778` above.

## Other observations (not `.lsp`)

- NGP surface-file `CarId`s are large per-(car, surface) ints (e.g.
  362903196 / 3404092349 / 516925395 for gravel/snow/tarmac of one car),
  vs retail's 0..7 car index; NGP `common.lsp` sets no `CarId` at all.
- `repair.ini` and the per-car text file are unparsed (not `.lsp`).
- No `tyres.lsp`/school equivalents ship with NGP cars; NGP relies on the
  plugin + these files.

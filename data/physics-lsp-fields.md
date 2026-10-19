# LSP key -> field map

Every `.lsp` key resolves through a static key table in `.data`: one record
`{name, w1, w2, w3}` per key (16 bytes). `w2 * 4` is the member byte offset
in the class object; `w1` is 0 for value keys and the child-table address
for sub-object keys. Tables bind to runtime dicts in static initializers
(either `CDict` inits or `CEntityContainerDict` container inits that also link
parent tables for inheritance). Lookup path: `CBase::Parse` (`0x52f0c0`) ->
class `Unk20`/`Unk1C` dict -> `CDictBase::Find` (`0x52ebf0`, linear `strcmp`) ->
member `Parse`. Value readers: `CReal::Parse` `0x52f770` (`atof`),
`CUnsigned::Parse` `0x52f840` (`atoi`), `CInteger::Parse` `0x52fab0` (`atoi`),
`CVector::Parse` `0x52fb20` (3x `atof`), `CVector4::Parse` `0x52fbe0` (4x `atof`),
`CBitField::Parse` `0x52f930` (`+`/`-` names). No parse-time conversion:
values land in the member raw (`CReal` value at member `+0xc`, etc.).
`kind` below is the C++ member type where a header/ctor join proves it, else `open`.
A `*` after a child class means the w1 link address equals that class dict address.
Composite members read `Name[i]` (first sub-word) or `Name sub-word +0xNN`
(element-relative, except `E13Mixed50` which is array-relative); all resolve to
`ReaderKind`/`CompositeKind` in `lsp_keys.rs` via `scripts/gen-lsp-keys.py`.
Retail use (`used_in_retail`) comes from `scripts/lsp-unused.txt`, verified by
`tests/lsp_keys.rs` against every retail `.lsp` (keys no retail file attributes
to their class stay `Unknown`/`false`).

## CBody::CState

Table `0x007c61f8` (1 records), dict `008eb798`, container init 0x723798, parents 01659218.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| iCollisionMesh | `+0x34` | `CReal` | ctor 0x4e1070 CReal@+0x34

## CCar::CDef

Table `0x007c6578` (18 records), dict `008ef73c`, container init 0x723890, parents 008ef724, 0165f118 (read from the dict in a snap-0 dump: +4 and +0x10, as the wheel definition's). The car's `InverseMass` and `vecLocalInverseInertiaDiagonal` land through the solid definition's table; `iCollisionMesh` (`CBody::CState`) is in neither parent, so the walk skips it.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| ControlUnit | `+0x250` | child -> CVehicleControlUnit::CDef | static table w1 link; class from retail nesting
| Drive | `+0xefc` | child -> CDrive::CDef | static table w1 link; class from retail nesting |
| Engine | `+0x1990` | child -> CEngine::CDef | static table w1 link; class from retail nesting
| LightSystem | `+0x4210` | child -> CCarLightSystem::CDef | static table w1 link; class from retail nesting |
| Driver | `+0x440c` | child -> CPassenger::CDef | static table w1 link; class from retail nesting |
| CoDriver | `+0x4428` | child -> CPassenger::CDef | static table w1 link; class from retail nesting |
| EffWheelRadius | `+0x240` | `CReal` | ctor placement +0x240 |
| SubTics | `+0x4444` | `CUnsigned` | ctor placement +0x4444 |
| MaxSteeringLock | `+0x220` | `CReal` | ctor placement +0x220 |
| SteeringRackRatio | `+0x230` | `CReal` | ctor placement +0x230 |
| AntiRollBarStiffness0 | `+0x4454` | `CReal` | ctor shim pair +0x4454 |
| AntiRollBarStiffness1 | `+0x4464` | `CReal` | ctor shim pair +0x4464 |
| FrontArea | `+0x4474` | `CReal` | ctor +0x4474 (default 2.0) |
| DragCoefficient | `+0x4484` | `CReal` | ctor +0x4484 (default 0.4) |
| MaxFFTorque | `+0x4494` | `CReal` | ctor +0x4494 (default 10.0) |
| DangerousAcceleration | `+0x44a4` | `CReal` | ctor +0x44a4 (default 490.5) |
| SetupSpec | `+0x44b4` | `CUnsigned` | ctor +0x44b4 |
| CarId | `+0x44c4` | `CUnsigned` | ctor +0x44c4 |

## CCar::CSetup

Table `0x007c64e0` (3 records), dict `008ef750`, CDict init 0x723830.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| MaxSteeringLock | `+0xc` | `CReal` | ctor 0x47b620 CReal (3 members +0xc/+0x1c/+0x2c)
| FrontRollBarStiffness | `+0x1c` | `CReal` | ctor 0x47b620 CReal (3 members +0xc/+0x1c/+0x2c)
| RearRollBarStiffness | `+0x2c` | `CReal` | ctor 0x47b620 CReal (3 members +0xc/+0x1c/+0x2c)

## CCar::CSetupOptions

Table `0x007c6698` (31 records), dict `008ef710`, CDict init 0x72386a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| MaxSteeringLockRangeMax | `+0x1c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xc (ctor 0x47d260)
| MaxSteeringLockRangeMin | `+0xc` | `E13RealTriple[0]` | E13RealTriple +0xc (ctor 0x47d260)
| MaxSteeringLockRangeStep | `+0x2c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xc (ctor 0x47d260)
| FrontRollBarStiffnessMax | `+0x4c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x3c (ctor 0x47d260)
| FrontRollBarStiffnessMin | `+0x3c` | `E13RealTriple[0]` | E13RealTriple +0x3c (ctor 0x47d260)
| FrontRollBarStiffnessStep | `+0x5c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x3c (ctor 0x47d260)
| RearRollBarStiffnessMax | `+0x7c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x6c (ctor 0x47d260)
| RearRollBarStiffnessMin | `+0x6c` | `E13RealTriple[0]` | E13RealTriple +0x6c (ctor 0x47d260)
| RearRollBarStiffnessStep | `+0x8c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x6c (ctor 0x47d260)
| vecCenterOfGravity | `+0xa0` | `CVector` | CVector m_unkA0 +0xa0 |
| InverseMass | `+0xc0` | `CReal` | CReal m_unkC0 +0xc0 |
| Tq00 | `+0xd0` | `CReal[0]` | CReal m_unkD0 +0xd0 |
| Tq05 | `+0xe0` | `CReal[1]` | CReal m_unkD0 +0xd0 |
| Tq10 | `+0xf0` | `CReal[2]` | CReal m_unkD0 +0xd0 |
| Tq15 | `+0x100` | `CReal[3]` | CReal m_unkD0 +0xd0 |
| Tq20 | `+0x110` | `CReal[4]` | CReal m_unkD0 +0xd0 |
| Tq25 | `+0x120` | `CReal[5]` | CReal m_unkD0 +0xd0 |
| Tq30 | `+0x130` | `CReal[6]` | CReal m_unkD0 +0xd0 |
| Tq35 | `+0x140` | `CReal[7]` | CReal m_unkD0 +0xd0 |
| Tq40 | `+0x150` | `CReal[8]` | CReal m_unkD0 +0xd0 |
| Tq45 | `+0x160` | `CReal[9]` | CReal m_unkD0 +0xd0 |
| Tq50 | `+0x170` | `CReal[10]` | CReal m_unkD0 +0xd0 |
| Tq55 | `+0x180` | `CReal[11]` | CReal m_unkD0 +0xd0 |
| Tq60 | `+0x190` | `CReal[12]` | CReal m_unkD0 +0xd0 |
| Tq65 | `+0x1a0` | `CReal[13]` | CReal m_unkD0 +0xd0 |
| Tq70 | `+0x1b0` | `CReal[14]` | CReal m_unkD0 +0xd0 |
| Tq75 | `+0x1c0` | `CReal[15]` | CReal m_unkD0 +0xd0 |
| Tq80 | `+0x1d0` | `CReal[16]` | CReal m_unkD0 +0xd0 |
| Tq85 | `+0x1e0` | `CReal[17]` | CReal m_unkD0 +0xd0 |
| Tq90 | `+0x1f0` | `CReal[18]` | CReal m_unkD0 +0xd0 |
| Tq95 | `+0x200` | `CReal[19]` | CReal m_unkD0 +0xd0 |

## CCar::CState

Table `0x007c6510` (6 records), dict `008ef724`, container init 0x7238d8, parents 0165f140.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| vecLocalLinearAcceleration | `+0x90` | open | unverified |
| vecLocalAngularAcceleration | `+0xb0` | open | unverified |
| ControlUnit | `+0x120` | child -> CVehicleControlUnit::CState | static table w1 link; class from retail nesting
| Drive | `+0x190` | child -> CDrive::CState | static table w1 link; class from retail nesting |
| Engine | `+0x750` | child -> CEngine::CState | static table w1 link; class from retail nesting
| LightSystem | `+0x9d0` | child -> CCarLightSystem::CState | static table w1 link (0x8ef7c8, outside the image); class inferred from nesting, unverified

## CCarBody::CDef

Table `0x007c6b28` (1 records), dict `008ef774`, container init 0x723915.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| vecCenterOfGravity | `+0x120` | `CVector` | ctor 0x4eb510 CVector@+0x120

## CCarBody::CState

Table `0x007c6b38` (1 records), dict `008ef78c`, container init 0x723948, parents 01659218.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| iCollisionMesh | `+0xc` | `CUnsigned` | CUnsigned m_u0C +0xc |

## CCarLightSystem::CDef

Table `0x007c6e00` (20 records), dict `008ef7b4`, CDict init 0x723970.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| BrakeLight0 | `+0x13c` | `CUnsigned[0]` | CUnsigned m_a13C +0x13c |
| BrakeLight1 | `+0x14c` | `CUnsigned[1]` | CUnsigned m_a13C +0x13c |
| BrakeLight2 | `+0x15c` | `CUnsigned[2]` | CUnsigned m_a13C +0x13c |
| BrakeLight3 | `+0x16c` | `CUnsigned[3]` | CUnsigned m_a13C +0x13c |
| LowBeam0 | `+0x7c` | `CUnsigned[0]` | CUnsigned m_a7c +0x7c |
| LowBeam1 | `+0x8c` | `CUnsigned[1]` | CUnsigned m_a7c +0x7c |
| LowBeam2 | `+0x9c` | `CUnsigned[2]` | CUnsigned m_a7c +0x7c |
| LowBeam3 | `+0xac` | `CUnsigned[3]` | CUnsigned m_a7c +0x7c |
| HighBeam0 | `+0xbc` | `CUnsigned[0]` | CUnsigned m_aBC +0xbc |
| HighBeam1 | `+0xcc` | `CUnsigned[1]` | CUnsigned m_aBC +0xbc |
| HighBeam2 | `+0xdc` | `CUnsigned[2]` | CUnsigned m_aBC +0xbc |
| HighBeam3 | `+0xec` | `CUnsigned[3]` | CUnsigned m_aBC +0xbc |
| RearLight0 | `+0xfc` | `CUnsigned[0]` | CUnsigned m_aFC +0xfc |
| RearLight1 | `+0x10c` | `CUnsigned[1]` | CUnsigned m_aFC +0xfc |
| RearLight2 | `+0x11c` | `CUnsigned[2]` | CUnsigned m_aFC +0xfc |
| RearLight3 | `+0x12c` | `CUnsigned[3]` | CUnsigned m_aFC +0xfc |
| ReverseLight0 | `+0x17c` | `CUnsigned[0]` | CUnsigned m_a17C +0x17c |
| ReverseLight1 | `+0x18c` | `CUnsigned[1]` | CUnsigned m_a17C +0x17c |
| ReverseLight2 | `+0x19c` | `CUnsigned[2]` | CUnsigned m_a17C +0x17c |
| ReverseLight3 | `+0x1ac` | `CUnsigned[3]` | CUnsigned m_a17C +0x17c |

## CCarLightSystem::CState

Table `0x007c6df0` (1 records), dict `008ef7c8`, CDict init 0x7239a0 (Unk1C side; Unk20 shares g_e41StateTable).

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Dummy | `+0x10` | `CReal` | CReal m_r10 +0x10 |

## CCollisionSolver::CDef

Table `0x007c74c0` (4 records), dict `01657890`, CDict init 0x723ae0.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Restitution | `+0xc` | `CReal` | m_r0C +0x0c |
| HyperRestitution | `+0x1c` | `CReal` | m_r1C +0x1c |
| MinInvEffRedMass | `+0x2c` | `CReal` | m_r2C +0x2c |
| nSteps | `+0x3c` | `CUnsigned` | m_u3C +0x3c |

## CCompleteCarSetup

Table `0x007c7770` (16 records), dict `016590f8`, CDict init 0x723b30.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Car | `+0xc` | child -> CCar::CSetup | static table w1 link; class from retail nesting |
| Drive | `+0x48` | child -> CDrive::CSetup | static table w1 link; class from retail nesting |
| Engine | `+0x684` | child -> CEngine::CSetup | static table w1 link; class from retail nesting
| VehicleControlUnit | `+0x6a0` | child -> CVehicleControlUnit::CSetup | static table w1 link; class from retail nesting
| WheelLF | `+0x1210` | child -> CWheel::CSetup | static table w1 link; class from retail nesting |
| WheelRF | `+0x1280` | child -> CWheel::CSetup | static table w1 link; class from retail nesting |
| WheelLB | `+0x12f0` | child -> CWheel::CSetup | static table w1 link; class from retail nesting |
| WheelRB | `+0x1360` | child -> CWheel::CSetup | static table w1 link; class from retail nesting |
| SpringDamperLF | `+0x13d0` | child -> CSpringDamper::CSetup | static table w1 link; class from retail nesting
| SpringDamperRF | `+0x146c` | child -> CSpringDamper::CSetup | static table w1 link; class from retail nesting
| SpringDamperLB | `+0x1508` | child -> CSpringDamper::CSetup | static table w1 link; class from retail nesting
| SpringDamperRB | `+0x15a4` | child -> CSpringDamper::CSetup | static table w1 link; class from retail nesting
| TyreLF | `+0x1640` | child -> CTyre::CSetup | static table w1 link (dict 0x165f200)
| TyreRF | `+0x1670` | child -> CTyre::CSetup | static table w1 link (dict 0x165f200)
| TyreLB | `+0x16a0` | child -> CTyre::CSetup | static table w1 link (dict 0x165f200)
| TyreRB | `+0x16d0` | child -> CTyre::CSetup | static table w1 link (dict 0x165f200)

## CCompleteCarSetupOptions

Table `0x007c7870` (23 records), dict `0165910c`, CDict init 0x723b6a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| CarOptions | `+0x10` | child -> CCar::CSetupOptions | static table w1 link; class from retail nesting |
| DriveOptions | `+0x220` | child -> CDrive::CSetupOptions | static table w1 link; class from retail nesting |
| ControlOptions | `+0x12dc` | child -> CVehicleControlUnit::CSetupOptions | static table w1 link; class from retail nesting |
| EngineOptions | `+0x13a8` | child -> CEngine::CSetupOptions | static table w1 link; class from retail nesting
| SpringDamperOptionsLF | `+0x13e4` | child -> CSpringDamper::CSetupOptions | static table w1 link; class from retail nesting
| SpringDamperOptionsRF | `+0x1570` | child -> CSpringDamper::CSetupOptions | static table w1 link; class from retail nesting
| SpringDamperOptionsLB | `+0x16fc` | child -> CSpringDamper::CSetupOptions | static table w1 link; class from retail nesting
| SpringDamperOptionsRB | `+0x1888` | child -> CSpringDamper::CSetupOptions | static table w1 link; class from retail nesting
| TyreOptionsLF | `+0x1a14` | child -> CTyre::CSetupOptions | static table w1 link; class from retail nesting
| TyreOptionsRF | `+0x1a50` | child -> CTyre::CSetupOptions | static table w1 link; class from retail nesting
| TyreOptionsLB | `+0x1a8c` | child -> CTyre::CSetupOptions | static table w1 link; class from retail nesting
| TyreOptionsRB | `+0x1ac8` | child -> CTyre::CSetupOptions | static table w1 link; class from retail nesting
| WheelOptionsLF | `+0x1b10` | child -> CWheel::CSetupOptions | static table w1 link; class from retail nesting |
| WheelOptionsRF | `+0x1e30` | child -> CWheel::CSetupOptions | static table w1 link; class from retail nesting |
| WheelOptionsLB | `+0x2150` | child -> CWheel::CSetupOptions | static table w1 link; class from retail nesting |
| WheelOptionsRB | `+0x2470` | child -> CWheel::CSetupOptions | static table w1 link; class from retail nesting |
| TyreType00 | `+0x2790` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)
| TyreType01 | `+0x2e3c` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)
| TyreType02 | `+0x34e8` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)
| TyreType03 | `+0x3b94` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)
| TyreType04 | `+0x4240` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)
| TyreType05 | `+0x48ec` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)
| TyreType06 | `+0x4f98` | child -> CTyre::CTyreTypes | static table w1 link (dict 0x165f1ec)

## CDrive::CDef

Table `0x007c95f0` (15 records), dict `01659134`, CDict init 0x723c30.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| EngineInertia | `+0x658` | `CReal` | ctor 0x506fe0 CReal@+0x658
| GearBoxInertiaIn | `+0x668` | `CReal` | ctor 0x506fe0 CReal@+0x668
| GearBoxInertiaOut | `+0x678` | `CReal` | ctor 0x506fe0 CReal@+0x678
| MaxFootbrakePressure | `+0x614` | `CReal` | ctor 0x506fe0 CReal@+0x614
| MaxHandbrakePressure | `+0x624` | `CReal` | ctor 0x506fe0 CReal@+0x624
| NumberOfGears | `+0x648` | `CUnsigned` | ctor 0x506fe0 CUnsigned@+0x648
| BrakeRatio_LF | `+0x20c` | `CReal` | ctor dword placement 131 |
| BrakeRatio_RF | `+0x2fc` | `E39Hook5140 sub-word +0xe0` | enclosing member E39_Hook5140 array +0x12c (sub +0xe0 CReal)
| BrakeRatio_LR | `+0x3ec` | `E39Hook5140 sub-word +0xe0` | enclosing member E39_Hook5140 array +0x12c (sub +0xe0 CReal)
| BrakeRatio_RR | `+0x4dc` | `E39Hook5140 sub-word +0xe0` | enclosing member E39_Hook5140 array +0x12c (sub +0xe0 CReal)
| WheelInertia | `+0xa58` | `CReal` | ctor 0x506fe0 CReal@+0xa58
| CenterDiffMaxTorque | `+0x4c` | `E39CDefCtorA sub-word +0x40` | enclosing member E39_CDefCtorA array +0xc (ctor 0x502780)
| FrontDiffMaxTorque | `+0xac` | `E39CDefCtorA sub-word +0x40` | enclosing member E39_CDefCtorA array +0xc (ctor 0x502780)
| RearDiffMaxTorque | `+0x10c` | `E39CDefCtorA sub-word +0x40` | enclosing member E39_CDefCtorA array +0xc (ctor 0x502780)
| DriveType | `+0x634` | `CInteger` | ctor 0x506fe0 CInteger@+0x634

## CDrive::CSetup

Table `0x007c94e0` (17 records), dict `0165915c`, CDict init 0x723bd0.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| CenterDiffMaxTorque | `+0xc` | `CReal` | m_unk0C +0x0c |
| FrontDiffMaxTorque | `+0x1c` | `CReal` | m_unk1C +0x1c |
| RearDiffMaxTorque | `+0x2c` | `CReal` | m_unk2C +0x2c |
| MaxBrakePressureFront | `+0x55c` | `CReal` | m_unk55C +0x55c |
| MaxBrakePressureRear | `+0x60c` | `CReal` | m_unk60C +0x60c |
| GearId0 | `+0x3fc` | `CUnsigned` | m_unk3FC[10] +0x3fc |
| GearId1 | `+0x40c` | `CUnsigned[1]` | CUnsigned m_unk3FC +0x3fc |
| GearId2 | `+0x41c` | `CUnsigned[2]` | CUnsigned m_unk3FC +0x3fc |
| GearId3 | `+0x42c` | `CUnsigned[3]` | CUnsigned m_unk3FC +0x3fc |
| GearId4 | `+0x43c` | `CUnsigned[4]` | CUnsigned m_unk3FC +0x3fc |
| GearId5 | `+0x44c` | `CUnsigned[5]` | CUnsigned m_unk3FC +0x3fc |
| GearId6 | `+0x45c` | `CUnsigned[6]` | CUnsigned m_unk3FC +0x3fc |
| GearId7 | `+0x46c` | `CUnsigned[7]` | CUnsigned m_unk3FC +0x3fc |
| GearId8 | `+0x47c` | `CUnsigned[8]` | CUnsigned m_unk3FC +0x3fc |
| GearId9 | `+0x48c` | `CUnsigned[9]` | CUnsigned m_unk3FC +0x3fc |
| FinalDriveId | `+0x49c` | `CUnsigned` | m_unk49C +0x49c |
| DropGearId | `+0x4ac` | `CUnsigned` | m_unk4AC +0x4ac |

## CDrive::CSetupOptions

Table `0x007c96e0` (230 records), dict `01659148`, CDict init 0x723c00.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Gears | `+0xb9c` | `CUnsigned` | ctor 0x47d3c0 CUnsigned@+0xb9c
| FrontBrakes | `+0xdbc` | `CReal` | ctor 0x47d3c0 CReal (FrontBrakes@+0xdbc, RearBrakes@+0xf3c)
| RearBrakes | `+0xf3c` | `CReal` | ctor 0x47d3c0 CReal (FrontBrakes@+0xdbc, RearBrakes@+0xf3c)
| FrontBrakePressureMax | `+0xf1c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xf0c (ctor 0x47d3c0)
| FrontBrakePressureMin | `+0xf0c` | `E13RealTriple[0]` | E13RealTriple +0xf0c (ctor 0x47d3c0)
| FrontBrakePressureStep | `+0xf2c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xf0c (ctor 0x47d3c0)
| RearBrakePressureMax | `+0x109c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x108c (ctor 0x47d3c0)
| RearBrakePressureMin | `+0x108c` | `E13RealTriple[0]` | E13RealTriple +0x108c (ctor 0x47d3c0)
| RearBrakePressureStep | `+0x10ac` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x108c (ctor 0x47d3c0)
| FrontBrake0PistonArea | `+0xdcc` | `E13Real10[0]` | E13Real10 m_unkDCC +0xdcc |
| FrontBrake0Mass | `+0xddc` | `E13Real10 m_unkDCC sub-word +0x10` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0Inertia | `+0xdec` | `E13Real10 m_unkDCC sub-word +0x20` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0Diameter | `+0xdfc` | `E13Real10 m_unkDCC sub-word +0x30` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0RadiusEff | `+0xe0c` | `E13Real10 m_unkDCC sub-word +0x40` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0SpecHeat | `+0xe1c` | `E13Real10 m_unkDCC sub-word +0x50` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0HeatAbsRatio | `+0xe2c` | `E13Real10 m_unkDCC sub-word +0x60` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0PlateArea | `+0xe3c` | `E13Real10 m_unkDCC sub-word +0x70` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0VentArea | `+0xe4c` | `E13Real10 m_unkDCC sub-word +0x80` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake0IntConvec | `+0xe5c` | `E13Real10 m_unkDCC sub-word +0x90` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1PistonArea | `+0xe6c` | `E13Real10[1]` | E13Real10 m_unkDCC +0xdcc |
| FrontBrake1Mass | `+0xe7c` | `E13Real10 m_unkDCC sub-word +0xb0` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1Inertia | `+0xe8c` | `E13Real10 m_unkDCC sub-word +0xc0` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1Diameter | `+0xe9c` | `E13Real10 m_unkDCC sub-word +0xd0` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1RadiusEff | `+0xeac` | `E13Real10 m_unkDCC sub-word +0xe0` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1SpecHeat | `+0xebc` | `E13Real10 m_unkDCC sub-word +0xf0` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1HeatAbsRatio | `+0xecc` | `E13Real10 m_unkDCC sub-word +0x100` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1PlateArea | `+0xedc` | `E13Real10 m_unkDCC sub-word +0x110` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1VentArea | `+0xeec` | `E13Real10 m_unkDCC sub-word +0x120` | enclosing member E13Real10 m_unkDCC +0xdcc |
| FrontBrake1IntConvec | `+0xefc` | `E13Real10 m_unkDCC sub-word +0x130` | enclosing member E13Real10 m_unkDCC +0xdcc |
| RearBrake0PistonArea | `+0xf4c` | `E13Real10[0]` | E13Real10 m_unkF4C +0xf4c |
| RearBrake0Mass | `+0xf5c` | `E13Real10 m_unkF4C sub-word +0x10` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0Inertia | `+0xf6c` | `E13Real10 m_unkF4C sub-word +0x20` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0Diameter | `+0xf7c` | `E13Real10 m_unkF4C sub-word +0x30` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0RadiusEff | `+0xf8c` | `E13Real10 m_unkF4C sub-word +0x40` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0SpecHeat | `+0xf9c` | `E13Real10 m_unkF4C sub-word +0x50` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0HeatAbsRatio | `+0xfac` | `E13Real10 m_unkF4C sub-word +0x60` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0PlateArea | `+0xfbc` | `E13Real10 m_unkF4C sub-word +0x70` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0VentArea | `+0xfcc` | `E13Real10 m_unkF4C sub-word +0x80` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake0IntConvec | `+0xfdc` | `E13Real10 m_unkF4C sub-word +0x90` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1PistonArea | `+0xfec` | `E13Real10[1]` | E13Real10 m_unkF4C +0xf4c |
| RearBrake1Mass | `+0xffc` | `E13Real10 m_unkF4C sub-word +0xb0` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1Inertia | `+0x100c` | `E13Real10 m_unkF4C sub-word +0xc0` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1Diameter | `+0x101c` | `E13Real10 m_unkF4C sub-word +0xd0` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1RadiusEff | `+0x102c` | `E13Real10 m_unkF4C sub-word +0xe0` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1SpecHeat | `+0x103c` | `E13Real10 m_unkF4C sub-word +0xf0` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1HeatAbsRatio | `+0x104c` | `E13Real10 m_unkF4C sub-word +0x100` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1PlateArea | `+0x105c` | `E13Real10 m_unkF4C sub-word +0x110` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1VentArea | `+0x106c` | `E13Real10 m_unkF4C sub-word +0x120` | enclosing member E13Real10 m_unkF4C +0xf4c |
| RearBrake1IntConvec | `+0x107c` | `E13Real10 m_unkF4C sub-word +0x130` | enclosing member E13Real10 m_unkF4C +0xf4c |
| CenterDiffTorqueMax | `+0xd3c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xd2c (ctor 0x47d3c0)
| CenterDiffTorqueMin | `+0xd2c` | `E13RealTriple[0]` | E13RealTriple +0xd2c (ctor 0x47d3c0)
| CenterDiffTorqueStep | `+0xd4c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xd2c (ctor 0x47d3c0)
| FrontDiffTorqueMax | `+0xd6c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xd5c (ctor 0x47d3c0)
| FrontDiffTorqueMin | `+0xd5c` | `E13RealTriple[0]` | E13RealTriple +0xd5c (ctor 0x47d3c0)
| FrontDiffTorqueStep | `+0xd7c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xd5c (ctor 0x47d3c0)
| RearDiffTorqueMax | `+0xd9c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xd8c (ctor 0x47d3c0)
| RearDiffTorqueMin | `+0xd8c` | `E13RealTriple[0]` | E13RealTriple +0xd8c (ctor 0x47d3c0)
| RearDiffTorqueStep | `+0xdac` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xd8c (ctor 0x47d3c0)
| GearList00Pinion | `+0x1c` | `E13Mixed50 m_unk0C sub-word +0x10` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList00Gear | `+0x2c` | `E13Mixed50 m_unk0C sub-word +0x20` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList00TorqueMax | `+0x4c` | `E13Mixed50 m_unk0C sub-word +0x40` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList00TorqueMin | `+0x3c` | `E13Mixed50 m_unk0C sub-word +0x30` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList01Pinion | `+0x6c` | `E13Mixed50 m_unk0C sub-word +0x60` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList01Gear | `+0x7c` | `E13Mixed50 m_unk0C sub-word +0x70` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList01TorqueMax | `+0x9c` | `E13Mixed50 m_unk0C sub-word +0x90` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList01TorqueMin | `+0x8c` | `E13Mixed50 m_unk0C sub-word +0x80` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList02Pinion | `+0xbc` | `E13Mixed50 m_unk0C sub-word +0xb0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList02Gear | `+0xcc` | `E13Mixed50 m_unk0C sub-word +0xc0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList02TorqueMax | `+0xec` | `E13Mixed50 m_unk0C sub-word +0xe0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList02TorqueMin | `+0xdc` | `E13Mixed50 m_unk0C sub-word +0xd0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList03Pinion | `+0x10c` | `E13Mixed50 m_unk0C sub-word +0x100` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList03Gear | `+0x11c` | `E13Mixed50 m_unk0C sub-word +0x110` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList03TorqueMax | `+0x13c` | `E13Mixed50 m_unk0C sub-word +0x130` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList03TorqueMin | `+0x12c` | `E13Mixed50 m_unk0C sub-word +0x120` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList04Pinion | `+0x15c` | `E13Mixed50 m_unk0C sub-word +0x150` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList04Gear | `+0x16c` | `E13Mixed50 m_unk0C sub-word +0x160` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList04TorqueMax | `+0x18c` | `E13Mixed50 m_unk0C sub-word +0x180` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList04TorqueMin | `+0x17c` | `E13Mixed50 m_unk0C sub-word +0x170` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList05Pinion | `+0x1ac` | `E13Mixed50 m_unk0C sub-word +0x1a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList05Gear | `+0x1bc` | `E13Mixed50 m_unk0C sub-word +0x1b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList05TorqueMax | `+0x1dc` | `E13Mixed50 m_unk0C sub-word +0x1d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList05TorqueMin | `+0x1cc` | `E13Mixed50 m_unk0C sub-word +0x1c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList06Pinion | `+0x1fc` | `E13Mixed50 m_unk0C sub-word +0x1f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList06Gear | `+0x20c` | `E13Mixed50 m_unk0C sub-word +0x200` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList06TorqueMax | `+0x22c` | `E13Mixed50 m_unk0C sub-word +0x220` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList06TorqueMin | `+0x21c` | `E13Mixed50 m_unk0C sub-word +0x210` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList07Pinion | `+0x24c` | `E13Mixed50 m_unk0C sub-word +0x240` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList07Gear | `+0x25c` | `E13Mixed50 m_unk0C sub-word +0x250` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList07TorqueMax | `+0x27c` | `E13Mixed50 m_unk0C sub-word +0x270` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList07TorqueMin | `+0x26c` | `E13Mixed50 m_unk0C sub-word +0x260` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList08Pinion | `+0x29c` | `E13Mixed50 m_unk0C sub-word +0x290` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList08Gear | `+0x2ac` | `E13Mixed50 m_unk0C sub-word +0x2a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList08TorqueMax | `+0x2cc` | `E13Mixed50 m_unk0C sub-word +0x2c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList08TorqueMin | `+0x2bc` | `E13Mixed50 m_unk0C sub-word +0x2b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList09Pinion | `+0x2ec` | `E13Mixed50 m_unk0C sub-word +0x2e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList09Gear | `+0x2fc` | `E13Mixed50 m_unk0C sub-word +0x2f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList09TorqueMax | `+0x31c` | `E13Mixed50 m_unk0C sub-word +0x310` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList09TorqueMin | `+0x30c` | `E13Mixed50 m_unk0C sub-word +0x300` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList10Pinion | `+0x33c` | `E13Mixed50 m_unk0C sub-word +0x330` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList10Gear | `+0x34c` | `E13Mixed50 m_unk0C sub-word +0x340` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList10TorqueMax | `+0x36c` | `E13Mixed50 m_unk0C sub-word +0x360` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList10TorqueMin | `+0x35c` | `E13Mixed50 m_unk0C sub-word +0x350` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList11Pinion | `+0x38c` | `E13Mixed50 m_unk0C sub-word +0x380` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList11Gear | `+0x39c` | `E13Mixed50 m_unk0C sub-word +0x390` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList11TorqueMax | `+0x3bc` | `E13Mixed50 m_unk0C sub-word +0x3b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList11TorqueMin | `+0x3ac` | `E13Mixed50 m_unk0C sub-word +0x3a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList12Pinion | `+0x3dc` | `E13Mixed50 m_unk0C sub-word +0x3d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList12Gear | `+0x3ec` | `E13Mixed50 m_unk0C sub-word +0x3e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList12TorqueMax | `+0x40c` | `E13Mixed50 m_unk0C sub-word +0x400` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList12TorqueMin | `+0x3fc` | `E13Mixed50 m_unk0C sub-word +0x3f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList13Pinion | `+0x42c` | `E13Mixed50 m_unk0C sub-word +0x420` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList13Gear | `+0x43c` | `E13Mixed50 m_unk0C sub-word +0x430` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList13TorqueMax | `+0x45c` | `E13Mixed50 m_unk0C sub-word +0x450` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList13TorqueMin | `+0x44c` | `E13Mixed50 m_unk0C sub-word +0x440` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList14Pinion | `+0x47c` | `E13Mixed50 m_unk0C sub-word +0x470` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList14Gear | `+0x48c` | `E13Mixed50 m_unk0C sub-word +0x480` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList14TorqueMax | `+0x4ac` | `E13Mixed50 m_unk0C sub-word +0x4a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList14TorqueMin | `+0x49c` | `E13Mixed50 m_unk0C sub-word +0x490` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList15Pinion | `+0x4cc` | `E13Mixed50 m_unk0C sub-word +0x4c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList15Gear | `+0x4dc` | `E13Mixed50 m_unk0C sub-word +0x4d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList15TorqueMax | `+0x4fc` | `E13Mixed50 m_unk0C sub-word +0x4f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList15TorqueMin | `+0x4ec` | `E13Mixed50 m_unk0C sub-word +0x4e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList16Pinion | `+0x51c` | `E13Mixed50 m_unk0C sub-word +0x510` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList16Gear | `+0x52c` | `E13Mixed50 m_unk0C sub-word +0x520` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList16TorqueMax | `+0x54c` | `E13Mixed50 m_unk0C sub-word +0x540` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList16TorqueMin | `+0x53c` | `E13Mixed50 m_unk0C sub-word +0x530` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList17Pinion | `+0x56c` | `E13Mixed50 m_unk0C sub-word +0x560` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList17Gear | `+0x57c` | `E13Mixed50 m_unk0C sub-word +0x570` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList17TorqueMax | `+0x59c` | `E13Mixed50 m_unk0C sub-word +0x590` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList17TorqueMin | `+0x58c` | `E13Mixed50 m_unk0C sub-word +0x580` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList18Pinion | `+0x5bc` | `E13Mixed50 m_unk0C sub-word +0x5b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList18Gear | `+0x5cc` | `E13Mixed50 m_unk0C sub-word +0x5c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList18TorqueMax | `+0x5ec` | `E13Mixed50 m_unk0C sub-word +0x5e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList18TorqueMin | `+0x5dc` | `E13Mixed50 m_unk0C sub-word +0x5d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList19Pinion | `+0x60c` | `E13Mixed50 m_unk0C sub-word +0x600` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList19Gear | `+0x61c` | `E13Mixed50 m_unk0C sub-word +0x610` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList19TorqueMax | `+0x63c` | `E13Mixed50 m_unk0C sub-word +0x630` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList19TorqueMin | `+0x62c` | `E13Mixed50 m_unk0C sub-word +0x620` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList20Pinion | `+0x65c` | `E13Mixed50 m_unk0C sub-word +0x650` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList20Gear | `+0x66c` | `E13Mixed50 m_unk0C sub-word +0x660` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList20TorqueMax | `+0x68c` | `E13Mixed50 m_unk0C sub-word +0x680` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList20TorqueMin | `+0x67c` | `E13Mixed50 m_unk0C sub-word +0x670` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList21Pinion | `+0x6ac` | `E13Mixed50 m_unk0C sub-word +0x6a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList21Gear | `+0x6bc` | `E13Mixed50 m_unk0C sub-word +0x6b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList21TorqueMax | `+0x6dc` | `E13Mixed50 m_unk0C sub-word +0x6d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList21TorqueMin | `+0x6cc` | `E13Mixed50 m_unk0C sub-word +0x6c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList22Pinion | `+0x6fc` | `E13Mixed50 m_unk0C sub-word +0x6f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList22Gear | `+0x70c` | `E13Mixed50 m_unk0C sub-word +0x700` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList22TorqueMax | `+0x72c` | `E13Mixed50 m_unk0C sub-word +0x720` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList22TorqueMin | `+0x71c` | `E13Mixed50 m_unk0C sub-word +0x710` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList23Pinion | `+0x74c` | `E13Mixed50 m_unk0C sub-word +0x740` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList23Gear | `+0x75c` | `E13Mixed50 m_unk0C sub-word +0x750` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList23TorqueMax | `+0x77c` | `E13Mixed50 m_unk0C sub-word +0x770` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList23TorqueMin | `+0x76c` | `E13Mixed50 m_unk0C sub-word +0x760` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList24Pinion | `+0x79c` | `E13Mixed50 m_unk0C sub-word +0x790` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList24Gear | `+0x7ac` | `E13Mixed50 m_unk0C sub-word +0x7a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList24TorqueMax | `+0x7cc` | `E13Mixed50 m_unk0C sub-word +0x7c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList24TorqueMin | `+0x7bc` | `E13Mixed50 m_unk0C sub-word +0x7b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList25Pinion | `+0x7ec` | `E13Mixed50 m_unk0C sub-word +0x7e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList25Gear | `+0x7fc` | `E13Mixed50 m_unk0C sub-word +0x7f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList25TorqueMax | `+0x81c` | `E13Mixed50 m_unk0C sub-word +0x810` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList25TorqueMin | `+0x80c` | `E13Mixed50 m_unk0C sub-word +0x800` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList26Pinion | `+0x83c` | `E13Mixed50 m_unk0C sub-word +0x830` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList26Gear | `+0x84c` | `E13Mixed50 m_unk0C sub-word +0x840` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList26TorqueMax | `+0x86c` | `E13Mixed50 m_unk0C sub-word +0x860` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList26TorqueMin | `+0x85c` | `E13Mixed50 m_unk0C sub-word +0x850` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList27Pinion | `+0x88c` | `E13Mixed50 m_unk0C sub-word +0x880` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList27Gear | `+0x89c` | `E13Mixed50 m_unk0C sub-word +0x890` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList27TorqueMax | `+0x8bc` | `E13Mixed50 m_unk0C sub-word +0x8b0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList27TorqueMin | `+0x8ac` | `E13Mixed50 m_unk0C sub-word +0x8a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList28Pinion | `+0x8dc` | `E13Mixed50 m_unk0C sub-word +0x8d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList28Gear | `+0x8ec` | `E13Mixed50 m_unk0C sub-word +0x8e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList28TorqueMax | `+0x90c` | `E13Mixed50 m_unk0C sub-word +0x900` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList28TorqueMin | `+0x8fc` | `E13Mixed50 m_unk0C sub-word +0x8f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList29Pinion | `+0x92c` | `E13Mixed50 m_unk0C sub-word +0x920` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList29Gear | `+0x93c` | `E13Mixed50 m_unk0C sub-word +0x930` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList29TorqueMax | `+0x95c` | `E13Mixed50 m_unk0C sub-word +0x950` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList29TorqueMin | `+0x94c` | `E13Mixed50 m_unk0C sub-word +0x940` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList30Pinion | `+0x97c` | `E13Mixed50 m_unk0C sub-word +0x970` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList30Gear | `+0x98c` | `E13Mixed50 m_unk0C sub-word +0x980` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList30TorqueMax | `+0x9ac` | `E13Mixed50 m_unk0C sub-word +0x9a0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList30TorqueMin | `+0x99c` | `E13Mixed50 m_unk0C sub-word +0x990` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList31Pinion | `+0x9cc` | `E13Mixed50 m_unk0C sub-word +0x9c0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList31Gear | `+0x9dc` | `E13Mixed50 m_unk0C sub-word +0x9d0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList31TorqueMax | `+0x9fc` | `E13Mixed50 m_unk0C sub-word +0x9f0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList31TorqueMin | `+0x9ec` | `E13Mixed50 m_unk0C sub-word +0x9e0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList32Pinion | `+0xa1c` | `E13Mixed50 m_unk0C sub-word +0xa10` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList32Gear | `+0xa2c` | `E13Mixed50 m_unk0C sub-word +0xa20` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList32TorqueMax | `+0xa4c` | `E13Mixed50 m_unk0C sub-word +0xa40` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList32TorqueMin | `+0xa3c` | `E13Mixed50 m_unk0C sub-word +0xa30` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList33Pinion | `+0xa6c` | `E13Mixed50 m_unk0C sub-word +0xa60` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList33Gear | `+0xa7c` | `E13Mixed50 m_unk0C sub-word +0xa70` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList33TorqueMax | `+0xa9c` | `E13Mixed50 m_unk0C sub-word +0xa90` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList33TorqueMin | `+0xa8c` | `E13Mixed50 m_unk0C sub-word +0xa80` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList34Pinion | `+0xabc` | `E13Mixed50 m_unk0C sub-word +0xab0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList34Gear | `+0xacc` | `E13Mixed50 m_unk0C sub-word +0xac0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList34TorqueMax | `+0xaec` | `E13Mixed50 m_unk0C sub-word +0xae0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList34TorqueMin | `+0xadc` | `E13Mixed50 m_unk0C sub-word +0xad0` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList35Pinion | `+0xb0c` | `E13Mixed50 m_unk0C sub-word +0xb00` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList35Gear | `+0xb1c` | `E13Mixed50 m_unk0C sub-word +0xb10` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList35TorqueMax | `+0xb3c` | `E13Mixed50 m_unk0C sub-word +0xb30` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList35TorqueMin | `+0xb2c` | `E13Mixed50 m_unk0C sub-word +0xb20` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList36Pinion | `+0xb5c` | `E13Mixed50 m_unk0C sub-word +0xb50` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList36Gear | `+0xb6c` | `E13Mixed50 m_unk0C sub-word +0xb60` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList36TorqueMax | `+0xb8c` | `E13Mixed50 m_unk0C sub-word +0xb80` | enclosing member E13Mixed50 m_unk0C +0xc |
| GearList36TorqueMin | `+0xb7c` | `E13Mixed50 m_unk0C sub-word +0xb70` | enclosing member E13Mixed50 m_unk0C +0xc |
| Gear0OptionsMin | `+0xbac` | `E13UnsignedPair[0]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear0OptionsMax | `+0xbbc` | `E13UnsignedPair m_unkBAC sub-word +0x10` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear1OptionsMin | `+0xbcc` | `E13UnsignedPair[1]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear1OptionsMax | `+0xbdc` | `E13UnsignedPair m_unkBAC sub-word +0x30` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear2OptionsMin | `+0xbec` | `E13UnsignedPair[2]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear2OptionsMax | `+0xbfc` | `E13UnsignedPair m_unkBAC sub-word +0x50` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear3OptionsMin | `+0xc0c` | `E13UnsignedPair[3]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear3OptionsMax | `+0xc1c` | `E13UnsignedPair m_unkBAC sub-word +0x70` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear4OptionsMin | `+0xc2c` | `E13UnsignedPair[4]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear4OptionsMax | `+0xc3c` | `E13UnsignedPair m_unkBAC sub-word +0x90` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear5OptionsMin | `+0xc4c` | `E13UnsignedPair[5]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear5OptionsMax | `+0xc5c` | `E13UnsignedPair m_unkBAC sub-word +0xb0` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear6OptionsMin | `+0xc6c` | `E13UnsignedPair[6]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear6OptionsMax | `+0xc7c` | `E13UnsignedPair m_unkBAC sub-word +0xd0` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear7OptionsMin | `+0xc8c` | `E13UnsignedPair[7]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear7OptionsMax | `+0xc9c` | `E13UnsignedPair m_unkBAC sub-word +0xf0` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear8OptionsMin | `+0xcac` | `E13UnsignedPair[8]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear8OptionsMax | `+0xcbc` | `E13UnsignedPair m_unkBAC sub-word +0x110` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| Gear9OptionsMin | `+0xccc` | `E13UnsignedPair[9]` | E13UnsignedPair m_unkBAC +0xbac |
| Gear9OptionsMax | `+0xcdc` | `E13UnsignedPair m_unkBAC sub-word +0x130` | enclosing member E13UnsignedPair m_unkBAC +0xbac |
| FinalDriveOptionsMin | `+0xcec` | `CUnsigned` | ctor 0x47d3c0 CUnsigned@+0xcec
| FinalDriveOptionsMax | `+0xcfc` | `CUnsigned` | ctor 0x47d3c0 CUnsigned@+0xcfc
| DropGearOptionsMin | `+0xd0c` | `CUnsigned` | ctor 0x47d3c0 CUnsigned@+0xd0c
| DropGearOptionsMax | `+0xd1c` | `CUnsigned` | ctor 0x47d3c0 CUnsigned@+0xd1c

## CDrive::CState

Table `0x007c94d0` (1 records), dict `01659170`, CDict init 0x723c60.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Gear | `+0x540` | open | unverified |

## CEngine::CDef

Table `0x007ca6d8` (17 records), dict `g_e41DefTable`, CDict init 0x723c9a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Turbo | `+0x26ec` | `CUnsigned` | ctor 0x50f080 CUnsigned@+0x26ec
| VE00 | `+0x2254` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE01 | `+0x2274` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE02 | `+0x2294` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE03 | `+0x22b4` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE04 | `+0x22d4` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE05 | `+0x22f4` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE06 | `+0x2314` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE07 | `+0x2334` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE08 | `+0x2354` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE09 | `+0x2374` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE10 | `+0x2394` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE11 | `+0x23b4` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE12 | `+0x23d4` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE13 | `+0x23f4` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE14 | `+0x2414` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)
| VE15 | `+0x2434` | `CRealPair sub-word +0x10` | enclosing member CRealPair array +0x2244 (ctor 0x50bc90)

## CEngine::CSetup

Table `0x007ca698` (1 records), dict `g_e41SetupTable`, CDict init 0x723cf0.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Dummy | `+0xc` | `CUnsigned` | ctor 0x47b6b0 CUnsigned@+0xc

## CEngine::CSetupOptions

Table `0x007ca6a8` (3 records), dict `g_e41SetupOptionsTable`, CDict init 0x723d20.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| DummyMax | `+0x1c` | `E13UnsignedTriple sub-word +0x10` | enclosing member E13UnsignedTriple +0xc (ctor 0x47d800)
| DummyMin | `+0xc` | `E13UnsignedTriple[0]` | E13UnsignedTriple +0xc (ctor 0x47d800)
| DummyStep | `+0x2c` | `E13UnsignedTriple sub-word +0x20` | enclosing member E13UnsignedTriple +0xc (ctor 0x47d800)

## CEngine::CState

Table `0x007ca678` (2 records), dict `g_e41StateTable`, CDict init 0x723cc0.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| InletPressure | `+0x1ac` | open | unverified |
| ExhaustPressure | `+0x1bc` | open | unverified |

## CGround::CDef

Table `0x007ca9b8` (4 records), dict `0165926c`, container init 0x723dd5.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Left | `+0x11c` | `CInteger` | ctor 0x51b990 +0x11c |
| Top | `+0x12c` | `CInteger` | ctor +0x12c |
| Width | `+0x13c` | `CUnsigned` | ctor +0x13c |
| Height | `+0x14c` | `CUnsigned` | ctor +0x14c |

## CHingedRigidBody::CDef

Table `0x007caa50` (30 records), dict `01659284`, container init 0x723e0e, parents 0165f118, 0165929c.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| vecHingeAxis_RF | `+0x2b0` | `CVector` | CVector m_v2b0 +0x2b0 |
| vecZeroAxis_RF | `+0x2d0` | `CVector` | CVector m_v2d0 +0x2d0 |
| vecHingePoint_RF | `+0x2f0` | `CVector` | CVector m_v2f0 +0x2f0 |
| vecHingeAxis_BF | `+0x370` | `CVector` | CVector m_v370 +0x370 |
| vecZeroAxis_BF | `+0x390` | `CVector` | CVector m_v390 +0x390 |
| vecHingePoint_BF | `+0x3b0` | `CVector` | CVector m_v3b0 +0x3b0 |
| vecFaceNormal_BF | `+0x350` | `CVector` | CVector m_v350 +0x350 |
| FaceArea | `+0x340` | `CReal` | CReal m_r340 +0x340 |
| HingeMaxAngle | `+0x470` | `CReal` | CReal m_r470 +0x470 |
| HingeMinAngle | `+0x480` | `CReal` | CReal m_r480 +0x480 |
| HingeDefaultAngle | `+0x490` | `CReal` | CReal m_r490 +0x490 |
| HingeRestitution | `+0x4a0` | `CReal` | CReal m_r4a0 +0x4a0 |
| InertiaHingeAxis | `+0x4b0` | `CReal` | CReal m_r4b0 +0x4b0 |
| SpringRate | `+0x4c0` | `CReal` | CReal m_r4c0 +0x4c0 |
| Damping | `+0x4d0` | `CReal` | CReal m_r4d0 +0x4d0 |
| Unlocked | `+0x4e0` | `CUnsigned` | CUnsigned m_u4e0 +0x4e0 |
| BodyId | `+0x4f0` | `CUnsigned` | CUnsigned m_u4f0 +0x4f0 |
| BreakChain | `+0x500` | `CUnsigned` | CUnsigned m_u500 +0x500 |
| Spring0Type | `+0x1f0` | `Block60[0]` | Block60 0x1f0 (ctor 0x51ca40 CUnsigned@+0x0)
| Spring0Pos | `+0x200` | `Block60 sub-word +0x10` | enclosing member Block60 0x1f0 (CReal)
| Spring0Stiff | `+0x210` | `Block60 sub-word +0x20` | enclosing member Block60 0x1f0 (CReal)
| Spring0Damp | `+0x220` | `Block60 sub-word +0x30` | enclosing member Block60 0x1f0 (CReal)
| Spring0MaxDisp | `+0x230` | `Block60 sub-word +0x40` | enclosing member Block60 0x1f0 (CReal)
| Spring0Span | `+0x240` | `Block60 sub-word +0x50` | enclosing member Block60 0x1f0 (CReal)
| Spring1Type | `+0x250` | `Block60[0]` | Block60 0x250 (ctor 0x51ca40 CUnsigned@+0x0)
| Spring1Pos | `+0x260` | `Block60 sub-word +0x10` | enclosing member Block60 0x250 (CReal)
| Spring1Stiff | `+0x270` | `Block60 sub-word +0x20` | enclosing member Block60 0x250 (CReal)
| Spring1Damp | `+0x280` | `Block60 sub-word +0x30` | enclosing member Block60 0x250 (CReal)
| Spring1MaxDisp | `+0x290` | `Block60 sub-word +0x40` | enclosing member Block60 0x250 (CReal)
| Spring1Span | `+0x2a0` | `Block60 sub-word +0x50` | enclosing member Block60 0x250 (CReal)

## CMaterial

Table `0x007cade8` (19 records), dict `01659aec`, CDict init 0x723e7a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| myStatic | `+0x3c` | `CReal` | CReal m_r3C +0x3c |
| myKinetic | `+0x4c` | `CReal` | CReal m_r4C +0x4c |
| myStaticSoil | `+0x5c` | `CReal` | CReal m_r5C +0x5c |
| myKineticSoil | `+0x6c` | `CReal` | CReal m_r6C +0x6c |
| SolidGroundOffset | `+0xc` | `CReal` | CReal m_r0C +0xc |
| SinkFactor | `+0x7c` | `CReal` | CReal m_r7C +0x7c |
| SoilDensity | `+0x8c` | `CReal` | CReal m_r8C +0x8c |
| SoilParticleMass | `+0x9c` | `CReal` | CReal m_r9C +0x9c |
| SoilThickness | `+0xac` | `CReal` | CReal m_rAC +0xac |
| BumpAmplitude | `+0xbc` | `CReal` | CReal m_rBC +0xbc |
| BumpFrequency | `+0xcc` | `CReal` | CReal m_rCC +0xcc |
| WaterFactor | `+0xdc` | `CReal` | CReal m_rDC +0xdc |
| RollingResistance | `+0xec` | `CReal` | CReal m_rEC +0xec |
| CollMyHigh | `+0xfc` | `CReal` | CReal m_rFC +0xfc |
| CollMyLow | `+0x10c` | `CReal` | CReal m_r10C +0x10c |
| eHigh | `+0x11c` | `CReal` | CReal m_r11C +0x11c |
| eLow | `+0x12c` | `CReal` | CReal m_r12C +0x12c |
| Type | `+0x13c` | `CUnsigned` | m_u13C +0x13c |
| Flags | `+0x14c` | `CBitField (4 bits)` | m_flags +0x14c; VALID/SOFT/HARD/BENDABLE |

## CMaterialManager

Table `0x007caf38` (134 records), dict `01659ad8`, CDict init 0x723eaa.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Undefined | id 0 (`w2=3 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Deeper_Dry | id 1 (`w2=91 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Deeper_Damp | id 2 (`w2=179 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Deeper_Wet | id 3 (`w2=267 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Deeper_Dry | id 5 (`w2=443 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Deeper_Damp | id 6 (`w2=531 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Deeper_Wet | id 7 (`w2=619 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Deep_Dry | id 9 (`w2=795 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Deep_Damp | id 10 (`w2=883 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Deep_Wet | id 11 (`w2=971 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Deep_Dry | id 13 (`w2=1147 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Deep_Damp | id 14 (`w2=1235 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Deep_Wet | id 15 (`w2=1323 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Shallow_Dry | id 17 (`w2=1499 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Shallow_Damp | id 18 (`w2=1587 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Shallow_Wet | id 19 (`w2=1675 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Shallow_Dry | id 21 (`w2=1851 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Shallow_Damp | id 22 (`w2=1939 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Medium_Shallow_Wet | id 23 (`w2=2027 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Spret_Bush_Small | id 24 (`w2=2115 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Spret_Bush_Medium | id 25 (`w2=2203 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Spret_Bush_Large | id 26 (`w2=2291 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Lov_Bush_Small | id 27 (`w2=2379 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Lov_Bush_Medium | id 28 (`w2=2467 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Lov_Bush_Large | id 29 (`w2=2555 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tree_Small | id 30 (`w2=2643 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tree_Medium | id 31 (`w2=2731 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tree_Large | id 32 (`w2=2819 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Deeper_Dry | id 33 (`w2=2907 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Deeper_Damp | id 34 (`w2=2995 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Deeper_Wet | id 35 (`w2=3083 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Clean_Dry | id 37 (`w2=3259 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Clean_Damp | id 38 (`w2=3347 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Clean_Wet | id 39 (`w2=3435 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Deep_Dry | id 41 (`w2=3611 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Deep_Damp | id 42 (`w2=3699 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Deep_Wet | id 43 (`w2=3787 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Sprinkled_Dry | id 45 (`w2=3963 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Sprinkled_Damp | id 46 (`w2=4051 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Sprinkled_Wet | id 47 (`w2=4139 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Shallow_Dry | id 49 (`w2=4315 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Shallow_Damp | id 50 (`w2=4403 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Coarse_Shallow_Wet | id 51 (`w2=4491 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Covered_Dry | id 53 (`w2=4667 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Covered_Damp | id 54 (`w2=4755 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Fine_Covered_Wet | id 55 (`w2=4843 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rock_Small | id 56 (`w2=4931 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rock_Medium | id 57 (`w2=5019 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rock_Large | id 58 (`w2=5107 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Trunk_Small | id 59 (`w2=5195 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Trunk_Medium | id 60 (`w2=5283 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Trunk_Large | id 61 (`w2=5371 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Snowwall | id 62 (`w2=5459 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Metal_Pole | id 63 (`w2=5547 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Metal_Barrier | id 64 (`w2=5635 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Clean_Dry | id 65 (`w2=5723 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Clean_Damp | id 66 (`w2=5811 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Clean_Wet | id 67 (`w2=5899 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Clean_Dry | id 69 (`w2=6075 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Clean_Damp | id 70 (`w2=6163 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Clean_Wet | id 71 (`w2=6251 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Sprinkled_Dry | id 73 (`w2=6427 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Sprinkled_Damp | id 74 (`w2=6515 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Sprinkled_Wet | id 75 (`w2=6603 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Sprinkled_Dry | id 77 (`w2=6779 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Sprinkled_Damp | id 78 (`w2=6867 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Sprinkled_Wet | id 79 (`w2=6955 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Covered_Dry | id 81 (`w2=7131 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Covered_Damp | id 82 (`w2=7219 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Medium_Covered_Wet | id 83 (`w2=7307 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Covered_Dry | id 85 (`w2=7483 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Covered_Damp | id 86 (`w2=7571 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tarmac_Coarse_Covered_Wet | id 87 (`w2=7659 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Snowwall_Bottom | id 88 (`w2=7747 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Hard_Dry | id 97 (`w2=8539 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Hard_Damp | id 98 (`w2=8627 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Hard_Wet | id 99 (`w2=8715 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| SnowOnGravel_Shallow | id 101 (`w2=8891 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| SnowOnGravel_Medium | id 102 (`w2=8979 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| SnowOnGravel_Deep | id 103 (`w2=9067 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Medium_Dry | id 105 (`w2=9243 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Medium_Damp | id 106 (`w2=9331 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Medium_Wet | id 107 (`w2=9419 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Soft_Dry | id 113 (`w2=9947 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Soft_Damp | id 114 (`w2=10035 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Grass_Soft_Wet | id 115 (`w2=10123 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| SnowOnIce_Shallow | id 117 (`w2=10299 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| SnowOnIce_Medium | id 118 (`w2=10387 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| SnowOnIce_Deep | id 119 (`w2=10475 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Tree_Stump | id 120 (`w2=10563 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Bendable_Tree | id 121 (`w2=10651 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rough_Rough_Dry | id 129 (`w2=11355 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rough_Rough_Wet | id 130 (`w2=11443 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Hard_Dry | id 133 (`w2=11707 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Hard_Damp | id 134 (`w2=11795 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Hard_Wet | id 135 (`w2=11883 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rough_VeryRough_Dry | id 137 (`w2=12059 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Rough_VeryRough_Wet | id 138 (`w2=12147 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Medium_Dry | id 141 (`w2=12411 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Medium_Damp | id 142 (`w2=12499 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Medium_Wet | id 143 (`w2=12587 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Soft_Dry | id 149 (`w2=13115 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Soft_Damp | id 150 (`w2=13203 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Soft_Wet | id 151 (`w2=13291 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Shallow_Dry_Clone | id 153 (`w2=13467 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Shallow_Damp_Clone | id 154 (`w2=13555 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Gravel_Fine_Shallow_Wet_Clone | id 155 (`w2=13643 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Water_Surface | id 157 (`w2=13819 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Hard_Dry_Clone | id 161 (`w2=14171 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Hard_Damp_Clone | id 162 (`w2=14259 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dirt_Hard_Wet_Clone | id 163 (`w2=14347 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Very_Sparse_Bush | id 165 (`w2=14523 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Sparse_Bush | id 166 (`w2=14611 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Dense_Bush | id 167 (`w2=14699 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Very_Dense_Bush | id 168 (`w2=14787 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Shallow_Dry | id 177 (`w2=15579 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Shallow_Damp | id 178 (`w2=15667 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Shallow_Wet | id 179 (`w2=15755 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Shallow_Dry | id 181 (`w2=15931 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Shallow_Damp | id 182 (`w2=16019 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Shallow_Wet | id 183 (`w2=16107 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Deep_Dry | id 185 (`w2=16283 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Deep_Damp | id 186 (`w2=16371 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Deep_Wet | id 187 (`w2=16459 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Deep_Dry | id 189 (`w2=16635 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Deep_Damp | id 190 (`w2=16723 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Deep_Wet | id 191 (`w2=16811 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Deeper_Dry | id 193 (`w2=16987 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Deeper_Damp | id 194 (`w2=17075 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| US2_Gravel_Fine_Deeper_Wet | id 195 (`w2=17163 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Deeper_Dry | id 197 (`w2=17339 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Deeper_Damp | id 198 (`w2=17427 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| BR2_Gravel_Coarse_Deeper_Wet | id 199 (`w2=17515 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |
| Spectator | id 255 (`w2=22443 = 88*id+3`) | surface id | `E44_TableBuild 0x520190` |

## CMountedRigidBody::CDef

Table `0x007cb834` (2 records), dict `01659b20`, container init 0x723eeb, parents 0165f118, 01659b08.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| BodyId | `+0x230` | `CUnsigned` | ctor 0x521480 CUnsigned@+0x230
| vecBasePoint_PRF | `+0x200` | `CVector` | ctor 0x521480 CVector@+0x200

## CPassenger::CDef

Table `0x007cb924` (1 records), dict `01659b4c`, CDict init 0x723f37.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| DebugID | `+0xc` | open | unverified |

## CPassenger::CState

Table `0x007cb934` (1 records), dict `01659b38`, CDict init 0x723f67.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| LastDeflection | `+0xc` | open | unverified |

## CPhysics::CDef

Table `0x007cbb10` (16 records), dict `0165aa90`, CDict init 0x72432a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| DefManager | `+0xc` | child -> CDefManager | static table w1 link; class from retail nesting
| MaterialManager | `+0x18` | child -> CMaterialManager | static table w1 link; class from retail nesting |
| CollisionSolver | `+0x16024` | child -> CCollisionSolver::CDef | static table w1 link; class from retail nesting |
| BrakeWallBuffer | `+0x16100` | `CReal` | m_unk16100 +0x16100 |
| BrakeWallFactor | `+0x16110` | `CReal` | m_unk16110 +0x16110 |
| BrakeWallDamping | `+0x16120` | `CReal` | m_unk16120 +0x16120 |
| MaxExpirableCount | `+0x16070` | `CUnsigned` | m_unk16070 +0x16070 |
| MaxTimeStep | `+0x16080` | `CReal` | m_unk16080 +0x16080 |
| MinTimeDelta | `+0x16090` | `CReal` | m_unk16090 +0x16090 |
| vecGravity | `+0x160a0` | `CVector` | m_unk160a0 +0x160a0 |
| Flags | `+0x160c0` | `CBitField (20 bits)` | m_flags +0x160c0; names 0x7cbc10 |
| DamageZone | `+0x16130` | `CUnsigned` | m_unk16130 +0x16130 |
| SingleStepperDelay | `+0x16150` | `CUnsigned` | m_unk16150 +0x16150 |
| ForceMaterial | `+0x16140` | `CUnsigned` | m_unk16140 +0x16140 |
| DbgFlags | `+0x160d4` | `CBitField (27 bits)` | m_dbg +0x160d4; names 0x7cbcb0 |
| LogFlags | `+0x160e8` | `CBitField (20 bits)` | m_log +0x160e8; names 0x7cbd90 |

## CPhysics::CState

Table `0x007cbe30` (1 records), dict `0165a3b0`, container init 0x724365.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| ObjIdQueue | `+0x28` | `CObjIdQueue` | m_queue +0x28 |

## CSolid::CDef

Table `0x007cc3f0` (10 records), dict `0165f118 (g_e47DefTable)`, container init 0x724558.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| vecLocalInverseInertiaDiagonal | `+0x120` | `CVector` | m_v120 +0x120 (RBR_E47_Def.cpp) |
| bSpecial | `+0x140` | `CUnsigned` | m_u140 +0x140 |
| AngularThresholdVelocity | `+0x150` | `CReal` | m_r150 +0x150 |
| InverseMass | `+0x160` | `CReal` | m_r160 +0x160 |
| LinearThresholdVelocity | `+0x170` | `CReal` | m_r170 +0x170 |
| Sleepable | `+0x1a0` | `CUnsigned` | m_u1a0 +0x1a0 |
| SleepDelay | `+0x1b0` | `CReal` | m_r1b0 +0x1b0 |
| LinearSleepVelocity | `+0x1c0` | `CReal` | m_r1c0 +0x1c0 |
| AngularSleepVelocity | `+0x1d0` | `CReal` | m_r1d0 +0x1d0 |
| VerySoft | `+0x1e0` | `CUnsigned` | m_u1e0 +0x1e0 |

## CSpringDamper::CDef

Table `0x007cc518` (16 records), dict `g_springDefTable`, CDict init 0x7245ba.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| DebugID | `+0xc` | `CUnsigned` | ctor 0x52d750 CUnsigned@+0xc
| SpringStiffness | `+0x4c` | `CReal` | ctor 0x52d750 CReal@+0x4c
| SpringLength | `+0x3c` | `CReal` | ctor 0x52d750 CReal@+0x3c
| HelperSpringStiffness | `+0x7c` | `CReal` | ctor 0x52d750 CReal@+0x7c
| HelperSpringLength | `+0x6c` | `CReal` | ctor 0x52d750 CReal@+0x6c
| InnerBumpRubberPos | `+0x1c` | `CReal` | ctor 0x52d750 CReal@+0x1c
| OuterBumpRubberPos | `+0x2c` | `CReal` | ctor 0x52d750 CReal@+0x2c
| DampingBump | `+0xac` | `CReal` | ctor 0x52d750 CReal@+0xac
| DampingRebound | `+0xbc` | `CReal` | ctor 0x52d750 CReal@+0xbc
| DampingBumpRubber | `+0xcc` | `CReal` | ctor 0x52d750 CReal@+0xcc
| BumpRubberStiffness | `+0x8c` | `CReal` | ctor 0x52d750 CReal@+0x8c
| DroopStiffness | `+0x9c` | `CReal` | ctor 0x52d750 CReal@+0x9c
| HighSpeedBreakBump | `+0xdc` | `CReal` | ctor 0x52d750 CReal@+0xdc
| HighSpeedBreakRebound | `+0xec` | `CReal` | ctor 0x52d750 CReal@+0xec
| HighSpeedDampingBump | `+0xfc` | `CReal` | ctor 0x52d750 CReal@+0xfc
| HighSpeedDampingRebound | `+0x10c` | `CReal` | ctor 0x52d750 CReal@+0x10c

## CSpringDamper::CSetup

Table `0x007cc628` (9 records), dict `g_springSetupTable`, CDict init 0x72461a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| SpringLength | `+0xc` | `CReal` | ctor 0x47bc50 CReal@+0xc
| SpringStiffness | `+0x1c` | `CReal` | ctor 0x47bc50 CReal@+0x1c
| HelperSpringLength | `+0x3c` | `CReal` | ctor 0x47bc50 CReal@+0x3c
| HelperSpringStiffness | `+0x4c` | `CReal` | ctor 0x47bc50 CReal@+0x4c
| HelperSpringMinLength | `+0x2c` | `CReal` | ctor 0x47bc50 CReal@+0x2c
| DampingBump | `+0x5c` | `CReal` | ctor 0x47bc50 CReal@+0x5c
| DampingRebound | `+0x6c` | `CReal` | ctor 0x47bc50 CReal@+0x6c
| BumpHighSpeedBreak | `+0x7c` | `CReal` | ctor 0x47bc50 CReal@+0x7c
| DampingBumpHighSpeed | `+0x8c` | `CReal` | ctor 0x47bc50 CReal@+0x8c

## CSpringDamper::CSetupOptions

Table `0x007cc6b8` (24 records), dict `g_setupOptionsTable`, CDict init 0x72464a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| SpringLengthMax | `+0x1c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xc (ctor 0x47d870)
| SpringLengthMin | `+0xc` | `E13RealTriple[0]` | E13RealTriple +0xc (ctor 0x47d870)
| SpringLengthStep | `+0x2c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xc (ctor 0x47d870)
| SpringStiffnessMax | `+0x4c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x3c (ctor 0x47d870)
| SpringStiffnessMin | `+0x3c` | `E13RealTriple[0]` | E13RealTriple +0x3c (ctor 0x47d870)
| SpringStiffnessStep | `+0x5c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x3c (ctor 0x47d870)
| HelperSpringLengthMax | `+0x7c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x6c (ctor 0x47d870)
| HelperSpringLengthMin | `+0x6c` | `E13RealTriple[0]` | E13RealTriple +0x6c (ctor 0x47d870)
| HelperSpringLengthStep | `+0x8c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x6c (ctor 0x47d870)
| HelperSpringStiffnessMax | `+0xac` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x9c (ctor 0x47d870)
| HelperSpringStiffnessMin | `+0x9c` | `E13RealTriple[0]` | E13RealTriple +0x9c (ctor 0x47d870)
| HelperSpringStiffnessStep | `+0xbc` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x9c (ctor 0x47d870)
| DampBumpMax | `+0xdc` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xcc (ctor 0x47d870)
| DampBumpMin | `+0xcc` | `E13RealTriple[0]` | E13RealTriple +0xcc (ctor 0x47d870)
| DampBumpStep | `+0xec` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xcc (ctor 0x47d870)
| DampReboundMax | `+0x10c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xfc (ctor 0x47d870)
| DampReboundMin | `+0xfc` | `E13RealTriple[0]` | E13RealTriple +0xfc (ctor 0x47d870)
| DampReboundStep | `+0x11c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xfc (ctor 0x47d870)
| DampBumpHiSpeedMax | `+0x13c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x12c (ctor 0x47d870)
| DampBumpHiSpeedMin | `+0x12c` | `E13RealTriple[0]` | E13RealTriple +0x12c (ctor 0x47d870)
| DampBumpHiSpeedStep | `+0x14c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x12c (ctor 0x47d870)
| DampSpeedBrakeMax | `+0x16c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x15c (ctor 0x47d870)
| DampSpeedBrakeMin | `+0x15c` | `E13RealTriple[0]` | E13RealTriple +0x15c (ctor 0x47d870)
| DampSpeedBrakeStep | `+0x17c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x15c (ctor 0x47d870)

## CSpringDamper::CState

Table `0x007cc618` (1 records), dict `g_springStateTable`, CDict init 0x7245e7.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| LastDeflection | `+0x1c` | open | unverified |

## CTyre::CDef

Table `0x007ccc50` (10 records), dict `g_tyreDefTable`, CDict init 0x72469a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| DebugId | `+0x10` | `CUnsigned` | ctor 0x530c50 CUnsigned@+0x10
| CarcassStiffnessLateral | `+0x40` | `CReal` | ctor 0x530c50 CReal@+0x40
| CarcassStiffnessTorsional | `+0x50` | `CReal` | ctor 0x530c50 CReal@+0x50
| CarcassWidth | `+0xc0` | `CReal` | ctor 0x530c50 CReal@+0xc0
| CarcassRadius | `+0x70` | `CReal` | ctor 0x530c50 CReal@+0x70
| TreadDepth | `+0x80` | `CReal` | ctor 0x530c50 CReal@+0x80
| TreadWidth | `+0xa0` | `CReal` | ctor 0x530c50 CReal@+0xa0
| TreadCoverage | `+0xb0` | `CReal` | ctor 0x530c50 CReal@+0xb0
| ReferenceFriction | `+0x30` | `CReal` | ctor 0x530c50 CReal@+0x30
| HubOffset | `+0x60` | `CReal` | ctor 0x530c50 CReal@+0x60

## CTyre::CTyreTypes

Table `0x007ccd40` (104 records), dict at 0x165f1ec (`g_tyreSetupTable` in `src/`, whose getters 0x530c30/0x530c40 sit on `CSetup` there; in the exe they are slots 7-8 of vtable 0x72e674, `CTyreTypes`'s, and `TyreType00`'s record links this dict), CDict init 0x72475a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Type | `+0xc` | `CUnsigned` | ctor 0x47c040 CUnsigned@+0xc (also CSetup@+0xc)
| RimRadius | `+0x1c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RimInnerRadius | `+0x2c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| HubOffset | `+0x3c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CarcassWidth | `+0xec` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CarcassRadius | `+0x9c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TreadDepth | `+0xac` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TreadWidth | `+0xcc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CarcassThickness | `+0xbc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CarcassStiffnessLateral | `+0x4c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CarcassStiffnessTorsional | `+0x5c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CarcassStiffnessVertical | `+0x6c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| PressureRatio | `+0x8c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TreadElasticity | `+0x7c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SideWallHeight | `+0x11c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TreadCoverage | `+0xdc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| StfFct_P0 | `+0x1fc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| StfFct_P1 | `+0x20c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| StfFct_P2 | `+0x21c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| StfFct_P3 | `+0x22c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T0 | `+0x23c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T1 | `+0x24c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T2 | `+0x25c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T3 | `+0x26c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T4 | `+0x27c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T5 | `+0x28c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T6 | `+0x29c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubMyFct_T7 | `+0x2ac` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T0 | `+0x2bc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T1 | `+0x2cc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T2 | `+0x2dc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T3 | `+0x2ec` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T4 | `+0x2fc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T5 | `+0x30c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T6 | `+0x31c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| RubWear_T7 | `+0x32c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L0 | `+0x33c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L1 | `+0x34c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L2 | `+0x35c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L3 | `+0x36c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L4 | `+0x37c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L5 | `+0x38c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L6 | `+0x39c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnStf_L7 | `+0x3ac` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L0 | `+0x3bc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L1 | `+0x3cc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L2 | `+0x3dc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L3 | `+0x3ec` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L4 | `+0x3fc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L5 | `+0x40c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L6 | `+0x41c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctStf_L7 | `+0x42c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L0 | `+0x43c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L1 | `+0x44c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L2 | `+0x45c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L3 | `+0x46c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L4 | `+0x47c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L5 | `+0x48c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L6 | `+0x49c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| CrnMyFct_L7 | `+0x4ac` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L0 | `+0x4bc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L1 | `+0x4cc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L2 | `+0x4dc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L3 | `+0x4ec` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L4 | `+0x4fc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L5 | `+0x50c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L6 | `+0x51c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| TrctMyFct_L7 | `+0x52c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L0 | `+0x53c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L1 | `+0x54c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L2 | `+0x55c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L3 | `+0x56c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L4 | `+0x57c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L5 | `+0x58c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L6 | `+0x59c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkCrn_L7 | `+0x5ac` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L0 | `+0x5bc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L1 | `+0x5cc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L2 | `+0x5dc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L3 | `+0x5ec` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L4 | `+0x5fc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L5 | `+0x60c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L6 | `+0x61c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SlpPkTrct_L7 | `+0x62c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SFric_T0 | `+0x12c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SFric_T1 | `+0x13c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SFric_T2 | `+0x14c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SFric_T3 | `+0x15c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SFric_T4 | `+0x16c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| SFric_T5 | `+0x17c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetSlope_T0 | `+0x19c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetSlope_T1 | `+0x1ac` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetSlope_T2 | `+0x1bc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetSlope_T3 | `+0x1cc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetSlope_T4 | `+0x1dc` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetSlope_T5 | `+0x1ec` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WearFtr_T0 | `+0x63c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WearFtr_T1 | `+0x64c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WearFtr_T2 | `+0x65c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WearFtr_T3 | `+0x66c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WearFtr_T4 | `+0x67c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WearFtr_T5 | `+0x68c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetnessDesign | `+0x18c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)
| WetnessWearFtr | `+0x69c` | `CReal` | ctor 0x47c040 CReal stride 0x10 (CTyreTypes layout)

## CTyre::CSetupOptions

Table `0x007ccd10` (3 records), dict `g_tyreSetupOptionsTable`, CDict init 0x724727.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| PressureMax | `+0x1c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xc (ctor 0x47da30)
| PressureMin | `+0xc` | `E13RealTriple[0]` | E13RealTriple +0xc (ctor 0x47da30)
| PressureStep | `+0x2c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xc (ctor 0x47da30)

## CTyre::CState

Table `0x007cccf0` (1 records), dict `g_tyreStateTable`, CDict init 0x7246c7.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| CarcassStiffness | `+0x50` | open | unverified |

## CTyre::CSetup

Table `0x007ccd00` (1 records), dict at 0x165f200 (`g_tyreTypesTable` in `src/`, its getters 0x530bf0/0x530c00 on `CTyreTypes` there; in the exe they are in vtable 0x72e624, `CSetup`'s, and `TyreLF`'s record links this dict), CDict init 0x7246f7.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Pressure | `+0x1c` | `CReal` | ctor 0x47bd60 CReal@+0x1c; t_coroll `d_*` `Tyre` blocks set it |

## CVehicleControlUnit::CDef

Table `0x007ce080` (23 records), dict `g_vcuDefTable`, CDict init 0x72478a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Gear0Upshift | `+0xc` | `CReal2A[0]` | CReal2A array +0xc (ctor 0x536a90)
| Gear0Downshift | `+0x1c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear1Upshift | `+0x2c` | `CReal2A[1]` | CReal2A array +0xc (ctor 0x536a90)
| Gear1Downshift | `+0x3c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear2Upshift | `+0x4c` | `CReal2A[2]` | CReal2A array +0xc (ctor 0x536a90)
| Gear2Downshift | `+0x5c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear3Upshift | `+0x6c` | `CReal2A[3]` | CReal2A array +0xc (ctor 0x536a90)
| Gear3Downshift | `+0x7c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear4Upshift | `+0x8c` | `CReal2A[4]` | CReal2A array +0xc (ctor 0x536a90)
| Gear4Downshift | `+0x9c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear5Upshift | `+0xac` | `CReal2A[5]` | CReal2A array +0xc (ctor 0x536a90)
| Gear5Downshift | `+0xbc` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear6Upshift | `+0xcc` | `CReal2A[6]` | CReal2A array +0xc (ctor 0x536a90)
| Gear6Downshift | `+0xdc` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear7Upshift | `+0xec` | `CReal2A[7]` | CReal2A array +0xc (ctor 0x536a90)
| Gear7Downshift | `+0xfc` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear8Upshift | `+0x10c` | `CReal2A[8]` | CReal2A array +0xc (ctor 0x536a90)
| Gear8Downshift | `+0x11c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| Gear9Upshift | `+0x12c` | `CReal2A[9]` | CReal2A array +0xc (ctor 0x536a90)
| Gear9Downshift | `+0x13c` | `CReal2A sub-word +0x10` | enclosing member CReal2A array +0xc
| LeftFootBrakeThreshold | `+0x19c` | `CReal` | ctor 0x5377a0 CReal (+0x14c..+0x19c)
| CenterDiffHandbrakeRelease | `+0x18c` | `CReal` | ctor 0x5377a0 CReal (+0x14c..+0x19c)
| RPMLimit | `+0x17c` | `CReal` | ctor 0x5377a0 CReal (+0x14c..+0x19c)

## CVehicleControlUnit::CSetup

Table `0x007cd520` (182 records), dict `g_vcuSetupTable`, CDict init 0x7247ea.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| AutoGears | `+0xc` | `CUnsigned` | ctor 0x47b720 CUnsigned (+0xc/+0x1c/+0x2c/+0x3c)
| GearGuard | `+0x1c` | `CUnsigned` | ctor 0x47b720 CUnsigned (+0xc/+0x1c/+0x2c/+0x3c)
| NeutralLock | `+0x2c` | `CUnsigned` | ctor 0x47b720 CUnsigned (+0xc/+0x1c/+0x2c/+0x3c)
| ClutchHelp | `+0x3c` | `CUnsigned` | ctor 0x47b720 CUnsigned (+0xc/+0x1c/+0x2c/+0x3c)
| CenterDiffHandbrakeRelease | `+0x4c` | `CReal` | ctor 0x47b720 CReal@+0x4c/+0x5c
| LeftFootBrakeThreshold | `+0x5c` | `CReal` | ctor 0x47b720 CReal@+0x4c/+0x5c
| CenterDiffThrottle_00 | `+0x6c` | `CReal[0]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_01 | `+0x7c` | `CReal[1]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_02 | `+0x8c` | `CReal[2]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_03 | `+0x9c` | `CReal[3]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_04 | `+0xac` | `CReal[4]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_05 | `+0xbc` | `CReal[5]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_06 | `+0xcc` | `CReal[6]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_07 | `+0xdc` | `CReal[7]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_08 | `+0xec` | `CReal[8]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_09 | `+0xfc` | `CReal[9]` | CReal m_arr00 +0x6c |
| CenterDiffThrottle_10 | `+0x10c` | `CReal[10]` | CReal m_arr00 +0x6c |
| CenterDiffBrake_00 | `+0x11c` | `CReal[0]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_01 | `+0x12c` | `CReal[1]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_02 | `+0x13c` | `CReal[2]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_03 | `+0x14c` | `CReal[3]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_04 | `+0x15c` | `CReal[4]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_05 | `+0x16c` | `CReal[5]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_06 | `+0x17c` | `CReal[6]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_07 | `+0x18c` | `CReal[7]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_08 | `+0x19c` | `CReal[8]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_09 | `+0x1ac` | `CReal[9]` | CReal m_arr01 +0x11c |
| CenterDiffBrake_10 | `+0x1bc` | `CReal[10]` | CReal m_arr01 +0x11c |
| CenterSpeedMapVelocity_00 | `+0x1cc` | `CReal[0]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_01 | `+0x1dc` | `CReal[1]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_02 | `+0x1ec` | `CReal[2]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_03 | `+0x1fc` | `CReal[3]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_04 | `+0x20c` | `CReal[4]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_05 | `+0x21c` | `CReal[5]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_06 | `+0x22c` | `CReal[6]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_07 | `+0x23c` | `CReal[7]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_08 | `+0x24c` | `CReal[8]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_09 | `+0x25c` | `CReal[9]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapVelocity_10 | `+0x26c` | `CReal[10]` | CReal m_arr02 +0x1cc |
| CenterSpeedMapFactor_00 | `+0x27c` | `CReal[0]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_01 | `+0x28c` | `CReal[1]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_02 | `+0x29c` | `CReal[2]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_03 | `+0x2ac` | `CReal[3]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_04 | `+0x2bc` | `CReal[4]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_05 | `+0x2cc` | `CReal[5]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_06 | `+0x2dc` | `CReal[6]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_07 | `+0x2ec` | `CReal[7]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_08 | `+0x2fc` | `CReal[8]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_09 | `+0x30c` | `CReal[9]` | CReal m_arr03 +0x27c |
| CenterSpeedMapFactor_10 | `+0x31c` | `CReal[10]` | CReal m_arr03 +0x27c |
| LFCenterDiffThrottle_00 | `+0x32c` | `CReal[0]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_01 | `+0x33c` | `CReal[1]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_02 | `+0x34c` | `CReal[2]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_03 | `+0x35c` | `CReal[3]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_04 | `+0x36c` | `CReal[4]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_05 | `+0x37c` | `CReal[5]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_06 | `+0x38c` | `CReal[6]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_07 | `+0x39c` | `CReal[7]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_08 | `+0x3ac` | `CReal[8]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_09 | `+0x3bc` | `CReal[9]` | CReal m_arr04 +0x32c |
| LFCenterDiffThrottle_10 | `+0x3cc` | `CReal[10]` | CReal m_arr04 +0x32c |
| LFCenterDiffBrake_00 | `+0x3dc` | `CReal[0]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_01 | `+0x3ec` | `CReal[1]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_02 | `+0x3fc` | `CReal[2]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_03 | `+0x40c` | `CReal[3]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_04 | `+0x41c` | `CReal[4]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_05 | `+0x42c` | `CReal[5]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_06 | `+0x43c` | `CReal[6]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_07 | `+0x44c` | `CReal[7]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_08 | `+0x45c` | `CReal[8]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_09 | `+0x46c` | `CReal[9]` | CReal m_arr05 +0x3dc |
| LFCenterDiffBrake_10 | `+0x47c` | `CReal[10]` | CReal m_arr05 +0x3dc |
| LFCenterSpeedMapVelocity_00 | `+0x48c` | `CReal[0]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_01 | `+0x49c` | `CReal[1]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_02 | `+0x4ac` | `CReal[2]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_03 | `+0x4bc` | `CReal[3]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_04 | `+0x4cc` | `CReal[4]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_05 | `+0x4dc` | `CReal[5]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_06 | `+0x4ec` | `CReal[6]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_07 | `+0x4fc` | `CReal[7]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_08 | `+0x50c` | `CReal[8]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_09 | `+0x51c` | `CReal[9]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapVelocity_10 | `+0x52c` | `CReal[10]` | CReal m_arr06 +0x48c |
| LFCenterSpeedMapFactor_00 | `+0x53c` | `CReal[0]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_01 | `+0x54c` | `CReal[1]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_02 | `+0x55c` | `CReal[2]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_03 | `+0x56c` | `CReal[3]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_04 | `+0x57c` | `CReal[4]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_05 | `+0x58c` | `CReal[5]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_06 | `+0x59c` | `CReal[6]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_07 | `+0x5ac` | `CReal[7]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_08 | `+0x5bc` | `CReal[8]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_09 | `+0x5cc` | `CReal[9]` | CReal m_arr07 +0x53c |
| LFCenterSpeedMapFactor_10 | `+0x5dc` | `CReal[10]` | CReal m_arr07 +0x53c |
| FrontDiffThrottle_00 | `+0x5ec` | `CReal[0]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_01 | `+0x5fc` | `CReal[1]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_02 | `+0x60c` | `CReal[2]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_03 | `+0x61c` | `CReal[3]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_04 | `+0x62c` | `CReal[4]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_05 | `+0x63c` | `CReal[5]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_06 | `+0x64c` | `CReal[6]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_07 | `+0x65c` | `CReal[7]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_08 | `+0x66c` | `CReal[8]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_09 | `+0x67c` | `CReal[9]` | CReal m_arr08 +0x5ec |
| FrontDiffThrottle_10 | `+0x68c` | `CReal[10]` | CReal m_arr08 +0x5ec |
| FrontDiffBrake_00 | `+0x69c` | `CReal[0]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_01 | `+0x6ac` | `CReal[1]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_02 | `+0x6bc` | `CReal[2]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_03 | `+0x6cc` | `CReal[3]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_04 | `+0x6dc` | `CReal[4]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_05 | `+0x6ec` | `CReal[5]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_06 | `+0x6fc` | `CReal[6]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_07 | `+0x70c` | `CReal[7]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_08 | `+0x71c` | `CReal[8]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_09 | `+0x72c` | `CReal[9]` | CReal m_arr09 +0x69c |
| FrontDiffBrake_10 | `+0x73c` | `CReal[10]` | CReal m_arr09 +0x69c |
| FrontSpeedMapVelocity_00 | `+0x74c` | `CReal[0]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_01 | `+0x75c` | `CReal[1]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_02 | `+0x76c` | `CReal[2]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_03 | `+0x77c` | `CReal[3]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_04 | `+0x78c` | `CReal[4]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_05 | `+0x79c` | `CReal[5]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_06 | `+0x7ac` | `CReal[6]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_07 | `+0x7bc` | `CReal[7]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_08 | `+0x7cc` | `CReal[8]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_09 | `+0x7dc` | `CReal[9]` | CReal m_arr10 +0x74c |
| FrontSpeedMapVelocity_10 | `+0x7ec` | `CReal[10]` | CReal m_arr10 +0x74c |
| FrontSpeedMapFactor_00 | `+0x7fc` | `CReal[0]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_01 | `+0x80c` | `CReal[1]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_02 | `+0x81c` | `CReal[2]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_03 | `+0x82c` | `CReal[3]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_04 | `+0x83c` | `CReal[4]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_05 | `+0x84c` | `CReal[5]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_06 | `+0x85c` | `CReal[6]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_07 | `+0x86c` | `CReal[7]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_08 | `+0x87c` | `CReal[8]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_09 | `+0x88c` | `CReal[9]` | CReal m_arr11 +0x7fc |
| FrontSpeedMapFactor_10 | `+0x89c` | `CReal[10]` | CReal m_arr11 +0x7fc |
| RearDiffThrottle_00 | `+0x8ac` | `CReal[0]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_01 | `+0x8bc` | `CReal[1]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_02 | `+0x8cc` | `CReal[2]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_03 | `+0x8dc` | `CReal[3]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_04 | `+0x8ec` | `CReal[4]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_05 | `+0x8fc` | `CReal[5]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_06 | `+0x90c` | `CReal[6]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_07 | `+0x91c` | `CReal[7]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_08 | `+0x92c` | `CReal[8]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_09 | `+0x93c` | `CReal[9]` | CReal m_arr12 +0x8ac |
| RearDiffThrottle_10 | `+0x94c` | `CReal[10]` | CReal m_arr12 +0x8ac |
| RearDiffBrake_00 | `+0x95c` | `CReal[0]` | CReal m_arr13 +0x95c |
| RearDiffBrake_01 | `+0x96c` | `CReal[1]` | CReal m_arr13 +0x95c |
| RearDiffBrake_02 | `+0x97c` | `CReal[2]` | CReal m_arr13 +0x95c |
| RearDiffBrake_03 | `+0x98c` | `CReal[3]` | CReal m_arr13 +0x95c |
| RearDiffBrake_04 | `+0x99c` | `CReal[4]` | CReal m_arr13 +0x95c |
| RearDiffBrake_05 | `+0x9ac` | `CReal[5]` | CReal m_arr13 +0x95c |
| RearDiffBrake_06 | `+0x9bc` | `CReal[6]` | CReal m_arr13 +0x95c |
| RearDiffBrake_07 | `+0x9cc` | `CReal[7]` | CReal m_arr13 +0x95c |
| RearDiffBrake_08 | `+0x9dc` | `CReal[8]` | CReal m_arr13 +0x95c |
| RearDiffBrake_09 | `+0x9ec` | `CReal[9]` | CReal m_arr13 +0x95c |
| RearDiffBrake_10 | `+0x9fc` | `CReal[10]` | CReal m_arr13 +0x95c |
| RearSpeedMapVelocity_00 | `+0xa0c` | `CReal[0]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_01 | `+0xa1c` | `CReal[1]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_02 | `+0xa2c` | `CReal[2]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_03 | `+0xa3c` | `CReal[3]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_04 | `+0xa4c` | `CReal[4]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_05 | `+0xa5c` | `CReal[5]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_06 | `+0xa6c` | `CReal[6]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_07 | `+0xa7c` | `CReal[7]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_08 | `+0xa8c` | `CReal[8]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_09 | `+0xa9c` | `CReal[9]` | CReal m_arr14 +0xa0c |
| RearSpeedMapVelocity_10 | `+0xaac` | `CReal[10]` | CReal m_arr14 +0xa0c |
| RearSpeedMapFactor_00 | `+0xabc` | `CReal[0]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_01 | `+0xacc` | `CReal[1]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_02 | `+0xadc` | `CReal[2]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_03 | `+0xaec` | `CReal[3]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_04 | `+0xafc` | `CReal[4]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_05 | `+0xb0c` | `CReal[5]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_06 | `+0xb1c` | `CReal[6]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_07 | `+0xb2c` | `CReal[7]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_08 | `+0xb3c` | `CReal[8]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_09 | `+0xb4c` | `CReal[9]` | CReal m_arr15 +0xabc |
| RearSpeedMapFactor_10 | `+0xb5c` | `CReal[10]` | CReal m_arr15 +0xabc |

## CVehicleControlUnit::CSetupOptions

Table `0x007cd460` (12 records), dict `0165f250`, CDict init 0x72481a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| DiffMapLockRangeMax | `+0x1c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0xc (ctor 0x47d710)
| DiffMapLockRangeMin | `+0xc` | `E13RealTriple[0]` | E13RealTriple +0xc (ctor 0x47d710)
| DiffMapLockRangeStep | `+0x2c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0xc (ctor 0x47d710)
| DiffMapSpeedRangeMax | `+0x4c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x3c (ctor 0x47d710)
| DiffMapSpeedRangeMin | `+0x3c` | `E13RealTriple[0]` | E13RealTriple +0x3c (ctor 0x47d710)
| DiffMapSpeedRangeStep | `+0x5c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x3c (ctor 0x47d710)
| CenterDiffHandbrakeReleaseRangeMax | `+0x7c` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x6c (ctor 0x47d710)
| CenterDiffHandbrakeReleaseRangeMin | `+0x6c` | `E13RealTriple[0]` | E13RealTriple +0x6c (ctor 0x47d710)
| CenterDiffHandbrakeReleaseRangeStep | `+0x8c` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x6c (ctor 0x47d710)
| LeftFootBrakeThresholdRangeMax | `+0xac` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x9c (ctor 0x47d710)
| LeftFootBrakeThresholdRangeMin | `+0x9c` | `E13RealTriple[0]` | E13RealTriple +0x9c (ctor 0x47d710)
| LeftFootBrakeThresholdRangeStep | `+0xbc` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x9c (ctor 0x47d710)

## CVehicleControlUnit::CState

Table `0x007cd450` (1 records), dict `g_vcuStateTable`, CDict init 0x7247b7.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Dummy | `+0xc` | open | unverified |

## CWheel::CDef

Table `0x007ce288` (27 records), dict `0165f2cc (= g_wheelDefTable)`, container init 0x72487e, parents 0165f118, 0165f2b4.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| IsSteered | `+0x210` | `CUnsigned` | ctor 0x5387e0 CUnsigned@+0x210
| DoSuspensionSweep | `+0x200` | `CUnsigned` | ctor 0x5387e0 CUnsigned@+0x200
| vecDriveAxleJointPos_VF | `+0x2f0` | `CVector` | ctor 0x5387e0 CVector@+0x2f0
| vecTopMount_VF | `+0x290` | `CVector` | ctor 0x5387e0 CVector@+0x290
| vecJointMount_VF | `+0x250` | `CVector` | ctor 0x5387e0 CVector@+0x250
| vecJointAxis_VF | `+0x270` | `CVector` | ctor 0x5387e0 CVector@+0x270
| vecSteeringMount_VF | `+0x2b0` | `CVector` | ctor 0x5387e0 CVector@+0x2b0
| vecSteeringAxis_VF | `+0x2d0` | `CVector` | ctor 0x5387e0 CVector@+0x2d0
| DriveAxleMountOffset | `+0x420` | `CReal` | ctor 0x5387e0 CReal@+0x420
| vecStrutRodMount_SF | `+0x370` | `CVector` | ctor 0x5387e0 CVector@+0x370
| vecStrutHubPoint_SF | `+0x390` | `CVector` | ctor 0x5387e0 CVector@+0x390
| vecStrutHubAxis_SF | `+0x3b0` | `CVector` | ctor 0x5387e0 CVector@+0x3b0
| StrutDimX | `+0x3e0` | `CReal` | ctor 0x5387e0 CReal@+0x3e0
| StrutDimZ | `+0x400` | `CReal` | ctor 0x5387e0 CReal@+0x400
| WishboneSign | `+0x360` | `CReal` | ctor 0x5387e0 CReal@+0x360
| SteeringSign | `+0x350` | `CReal` | ctor 0x5387e0 CReal@+0x350
| SpringDamper | `+0x450` | child -> CSpringDamper::CDef | ctor 0x5387e0 builds CSpringDamper::CDef@+0x450 (call 0x52d750)
| SteeringRodLength | `+0x230` | `CReal` | ctor 0x5387e0 CReal@+0x230
| WishboneLength | `+0x240` | `CReal` | ctor 0x5387e0 CReal@+0x240
| DebugId | `+0x220` | `CUnsigned` | ctor 0x5387e0 CUnsigned@+0x220
| AntiRollBarId | `+0x430` | `CUnsigned` | ctor 0x5387e0 CUnsigned@+0x430
| AntiRollBarSlot | `+0x440` | `CUnsigned` | ctor 0x5387e0 CUnsigned@+0x440
| vecTopMountDeformUpAxis_RF | `+0x310` | `CVector` | ctor 0x5387e0 CVector@+0x310
| vecTopMountDeformSideAxis_RF | `+0x330` | `CVector` | ctor 0x5387e0 CVector@+0x330
| TopMountMaxDeformSide | `+0x904` | `CReal` | ctor 0x5387e0 CReal@+0x904
| TopMountMaxDeformUp | `+0x914` | `CReal` | ctor 0x5387e0 CReal@+0x914
| Tyre | `+0x56c` | child -> CTyre::CDef | static table w1 link; class from retail nesting

## CWheel::CSetup

Table `0x007ce478` (5 records), dict `0165f2a0`, CDict init 0x7248d7.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| vecTopMountPosition | `+0x20` | `CVector` | CVector m_unk20 +0x20 |
| TopMountSlot | `+0xc` | `CReal` | ctor 0x47bba0 CReal@+0xc
| SteeringRodLength | `+0x40` | `CReal` | ctor 0x47bba0 CReal@+0x40
| StrutPlatformHeight | `+0x50` | `CReal` | ctor 0x47bba0 CReal@+0x50
| WheelAxisInclination | `+0x60` | `CReal` | ctor 0x47bba0 CReal@+0x60

## CWheel::CSetupOptions

Table `0x007ce4c8` (34 records), dict `0165f2f4`, CDict init 0x72490a.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| nTopMountPositions | `+0xc` | `CReal` | ctor 0x47daa0 CReal@+0xc (count as real)
| vecTopMount00 | `+0x20` | `CVector[0]` | CVector m_unk20 +0x20 |
| vecTopMount01 | `+0x40` | `CVector[1]` | CVector m_unk20 +0x20 |
| vecTopMount02 | `+0x60` | `CVector[2]` | CVector m_unk20 +0x20 |
| vecTopMount03 | `+0x80` | `CVector[3]` | CVector m_unk20 +0x20 |
| vecTopMount04 | `+0xa0` | `CVector[4]` | CVector m_unk20 +0x20 |
| vecTopMount05 | `+0xc0` | `CVector[5]` | CVector m_unk20 +0x20 |
| vecTopMount06 | `+0xe0` | `CVector[6]` | CVector m_unk20 +0x20 |
| vecTopMount07 | `+0x100` | `CVector[7]` | CVector m_unk20 +0x20 |
| SteeringRodMax | `+0x130` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x120 (ctor 0x47daa0)
| SteeringRodMin | `+0x120` | `E13RealTriple[0]` | E13RealTriple +0x120 (ctor 0x47daa0)
| SteeringRodStep | `+0x140` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x120 (ctor 0x47daa0)
| WheelAxisInclMax | `+0x160` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x150 (ctor 0x47daa0)
| WheelAxisInclMin | `+0x150` | `E13RealTriple[0]` | E13RealTriple +0x150 (ctor 0x47daa0)
| WheelAxisInclStep | `+0x170` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x150 (ctor 0x47daa0)
| PlatformHeightMax | `+0x190` | `E13RealTriple sub-word +0x10` | enclosing member E13RealTriple +0x180 (ctor 0x47daa0)
| PlatformHeightMin | `+0x180` | `E13RealTriple[0]` | E13RealTriple +0x180 (ctor 0x47daa0)
| PlatformHeightStep | `+0x1a0` | `E13RealTriple sub-word +0x20` | enclosing member E13RealTriple +0x180 (ctor 0x47daa0)
| vecTopMount_VF | `+0x1b0` | `CVector` | ctor 0x47daa0 CVector@+0x1b0
| vecJointMount_VF | `+0x1d0` | `CVector` | ctor 0x47daa0 CVector@+0x1d0
| vecJointAxis_VF | `+0x1f0` | `CVector` | ctor 0x47daa0 CVector@+0x1f0
| vecSteeringMount_VF | `+0x210` | `CVector` | ctor 0x47daa0 CVector@+0x210
| vecStrutRodMount_SF | `+0x230` | `CVector` | ctor 0x47daa0 CVector@+0x230
| vecStrutHubPoint_SF | `+0x250` | `CVector` | ctor 0x47daa0 CVector@+0x250
| vecStrutHubAxis_SF | `+0x270` | `CVector` | ctor 0x47daa0 CVector@+0x270
| StrutDimX | `+0x290` | `CReal` | ctor 0x47daa0 CReal@+0x290
| StrutDimZ | `+0x2a0` | `CReal` | ctor 0x47daa0 CReal@+0x2a0
| WishboneSign | `+0x2c0` | `CReal` | ctor 0x47daa0 CReal@+0x2c0
| SteeringSign | `+0x2b0` | `CReal` | ctor 0x47daa0 CReal@+0x2b0
| WishboneLength | `+0x2d0` | `CReal` | ctor 0x47daa0 CReal@+0x2d0
| AntiRollBarId | `+0x2e0` | `CUnsigned` | ctor 0x47daa0 CUnsigned@+0x2e0
| AntiRollBarSlot | `+0x2f0` | `CUnsigned` | ctor 0x47daa0 CUnsigned@+0x2f0
| InnerBumpRubberPos | `+0x300` | `CReal` | ctor 0x47daa0 CReal@+0x300
| OuterBumpRubberPos | `+0x310` | `CReal` | ctor 0x47daa0 CReal@+0x310

## CWheel::CState

Table `0x007ce438` (4 records), dict `0165f2b4`, container init 0x7248a8, parents 0165f140.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| SpringDeflection | `+0x260` | open | unverified |
| DeflectionSpeed | `+0x260` | open | unverified |
| vecWorldFrameSpringEnd | `+0x270` | open | unverified |
| Tyre | `+0x2e0` | child -> CTyre::CState | static table w1 link; class from retail nesting

## HingedState

Table `0x007caa40` (1 records), dict `0165929c`, container init 0x723e38, parents 0165f140.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Dummy | `+0x4` | open | unverified |

## MountedState

Table `0x007cb854` (1 records), dict `01659b08`, container init 0x723f18, parents 0165f140.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| Dummy | `+0x4` | open | unverified |

## shared entity-state table

Table `0x007cc490` (6 records), dict `0165f140`, container init 0x724588, parents 01659218.

| key | offset | kind | evidence |
| --- | --- | --- | --- |
| vecRelativeAngularVelocity | `+0xa0` | open | unverified |
| vecParentRotation | `+0x10` | open | unverified |
| vecRelativeLinearVelocity | `+0xd0` | open | unverified |
| Progress | `+0x90` | open | unverified |
| IsActive | `+0xf0` | open | unverified |
| WipedOut | `+0x100` | open | unverified |

The `HingedState`/`MountedState` tables hang off `CHingedRigidBody::CDef` and
`CMountedRigidBody::CDef` (dictB links). Which `CState` each belongs to is unproven
(no `Unk20` returns them directly).

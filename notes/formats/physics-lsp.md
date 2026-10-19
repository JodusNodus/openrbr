# `.lsp` physics/setup scripts

**Verdict: `.lsp` is a Lisp-like text format** — nested parenthesised lists
with `"quoted"` names, bare `key value` pairs, floats/ints and `+FLAG` /
`-FLAG` toggles. The game tokenises it with `FUN_0052ec80` and applies each
file to the live `NPhysics::NTweak` tree (`CDef`/`CSetup`/`CSetupOptions`
objects). All addresses below are VAs for the official 1.02
`RichardBurnsRally_SSE.exe`; decompiled C in
`out/decomp/RichardBurnsRally_SSE.exe.c`.

Retail 1.0 ships **89 `.lsp` files**: 87 in `Root/physics.rbz`
(`Physics/<car>/{common,gravel,snow,tarmac,r_gravel,r_snow,r_tarmac}.lsp`,
`Physics/<car>/setups/d_{gravel,snow,tarmac}.lsp`,
`Physics/{physics,localphysics}.lsp`, `Physics/{michelin,pirelli}/tyres.lsp`,
`Physics/school/{gravel,tarmac,sfgravel}.lsp`) plus 2 in `Root/misc.rbz`
(`Misc/{prime,state}.lsp`). All are ASCII, CRLF, and parse with the grammar
below (verified by `rust/rbr-formats`, module `lsp`).

## Grammar (observed; exe-compatible)

```ebnf
file   := ws item* ws ( ")" ws )? EOF   (* one stray ")" tolerated, see below *)
item   := list | quoted | atom
list   := "(" ws item* ws ")"
quoted := '"' [^"]* '"'                  (* no escapes; may contain spaces and parens *)
atom   := [^()\s";]+                     (* bare word, number, or +FLAG/-FLAG *)
ws     := ( [ \t\r\n\0]+ | ";" [^\r\n]* | "//" [^\r\n]* )*
```

- Every retail file is a single top-level list: `(("Name" ...))`.
- `;` starts a line comment (207 lines, e.g. `("Gravel_..." ;id 17, rev 1203`).
  `//` occurs only 4× (`//Dummy car data` in two `r_*.lsp`); both are treated
  as comments by our reader.
- Comment handling is a **reader convenience, not exe behaviour**: the exe
  tokenizer (`0x52ec80`) knows only whitespace/quotes/parens. Comments parse
  as unknown keys/bare numbers, which the apply step skips (see below), so
  stripping them yields identical results on all retail files (no retail
  comment contains a `"`; every `"` in every file is delimiter-separated).
- Two files (`michelin/tyres.lsp`, `pirelli/tyres.lsp`) ship **one extra
  trailing `)`**; the exe tolerates it (its top-level loop `break`s on `)`,
  `0x52ec80`), and our reader accepts one stray closer. Max nesting depth
  observed is 7 (`prime.lsp`).
- `Misc/state.lsp` ships one trailing `NUL` byte; harmless (the game reads
  files as C strings) and skipped as trivia.

## Exe parser

- **Tokenizer `FUN_0052ec80` @ `0x52ec80`** (free: `0x52ee10`): skips
  `isspace`, strips one leading `(`, NUL-terminates tokens in place, honours
  `"..."` (no escapes; quotes also suppress paren counting inside groups),
  tracks paren nesting for `(...)` group tokens; token-pointer table is
  `0x4000` entries. No `;` handling — see above.
- **Token classifiers**: `0x52ee60` (unquoted group = contains spaces),
  `0x52eed0` (real: optional sign, digits, ≤1 dot), `0x52ef40` (unsigned:
  optional sign, all digits), `0x52f000` (extract `"quoted"` content).
- **Value readers**: `NPhysics::NTweak::CReal::vf08` @ `0x52f770` (`atof`),
  `CUnsigned::vf08` @ `0x52f840` (`atoi`), `CBitField` reader @ `0x52f930`
  (`+NAME`/`-NAME` looked up in a flag table). Vectors (`vecGravity 0 0 -9.81`,
  `vecCenterOfGravity`, …) are consecutive reals; the exact vector-reader
  address is unverified (unknown).
- **Apply walk `FUN_0052f0c0` @ `0x52f0c0`**: walks the token list against a
  `CDef`'s registered children — quoted first token = instance name,
  numbers fill `CReal`/`CUnsigned`, `(...)` groups recurse (child looked up by
  name, created via factory for dict entries), **anything unrecognised is
  skipped** (this is what makes `;`/`//` notes and stale keys harmless).
  Details of the per-class vtables unverified beyond the readers above.
- **Parse entries**: `FUN_0048d940` @ `0x48d940` (buffer + length; if the
  buffer does not start with `((`, it is first XOR-decoded in place by
  `FUN_0055ccd0` @ `0x55ccd0` — key at `0x7d12c8`, period 500; no shipped
  `.lsp` needs this, purpose unknown), `FUN_0048d9f0` @ `0x48d9f0` (whole
  file via file-core), `FUN_0048da90`/`0x48db30` (same via archive helpers),
  `FUN_0048d850` @ `0x48d850` (wraps a raw memory buffer in parens first;
  used by the binary setup-stream code). Emitted file text also travels
  through `CSignalManager::CMultiSignal1<char*>::Emit` (`0x47e840`).
- **Save format**: `CCarSaveFileManager::ReadFromBinaryStream` @ `0x48e1e0`
  expects magic `PTSC` (`0x43535450`); slot paths use `%dslot%d_%s.lsp`
  (`0x49e230`); setup glob `%sSetups/*.lsp` (`0x546a30`). Binary stream
  layout not examined.

## How a car's directory is assembled (load order)

Car index → directory via `FUN_0047c7d0` @ `0x47c7d0` (clamps >7 to 8) and the
pointer table at `0x7c18d4`: `0 Physics/c_xsara/`, `1 Physics/h_accent/`,
`2 Physics/mg_zr/`, `3 Physics/m_lancer/`, `4 Physics/p_206/`,
`5 Physics/s_i2003/`, `6 Physics/t_coroll/`, `7 Physics/s_i2000/`,
`8 Physics/Custom/`. Surface via `FUN_004a1330` @ `0x4a1330` (weather id
`0x10–0x12` → snow, `0x29–0x31`/`0x49`/`0x55`/`0x56`/`0x5e` → tarmac, else
gravel; default branch order in the callers is gravel-first).

1. `CApplication::ReloadPhysicsLsp` @ `0x47fcf0`: `Physics/physics.lsp`
   (success logs `%s, Got Physics ini` + own `__FUNCSIG__`).
2. Car loader @ `0x47fd60`: `"%scommon.lsp"`, then `"%s%s"` with
   `gravel.lsp`/`snow.lsp`/`tarmac.lsp` (`%s%s` = `0x72c4d4`). Missing common
   logs `Car common file not found.` (level 6); missing surface file logs
   `Car definition file not found.`
3. `FUN_0048de30` @ `0x48de30`: `"%sr_%s.lsp"` (setup-option ranges).
4. `FUN_0048df00` @ `0x48df00`: `Physics/%s/tyres.lsp` — `pirelli` iff car
   index 5 or 7, else `michelin`.
5. `FUN_0048e440` @ `0x48e440`: `"%sSetups/d_%s.lsp"` (player setup) +
   options + tyres. Rally-school variant `FUN_0048df80` @ `0x48df80` loads
   `Physics\school\{gravel,tarmac,sfgravel}.lsp` + `r` options + pirelli.
6. `CApplication::InitDemo` @ `0x47fee0` parses an inline default
   (`(("PhysicsEngine" MaxTimeStep 0.03 vecGravity 0 0 -9.8 ...))`) then
   `MISC/PHYSICS.LSP`. The quick-race path (~`0x481830`) loads
   `misc/prime.lsp`, then calls 1 + 2 + tyres.

Later files override earlier ones key-by-key (all apply to the same live
tree); e.g. per-surface `gravel.lsp` only re-specifies wheels/tyres and
`SetupSpec`, leaving the rest from `common.lsp`.

## Sections / keys (names as in files; semantics inferred, unknowns marked)

- **`physics.lsp` (`(("PhysicsEngine" DefManager (...) ...))`)** — engine
  defaults: `DefManager` with `Ground` (`GROUND` + `Width/Height/Top/Left`),
  `Solid` props (`GENERIC`, `TRAFFIC_CONE`, `PIG`, …: `InverseMass`,
  `vecLocalInverseInertiaDiagonal`, sleep params), `Car/CAR_ROOT`
  (baseline mass/inertia, `MaxFFTorque`, `EffWheelRadius`, anti-roll,
  gearshift `ControlUnit`, lights); `MaxTimeStep/MinTimeDelta`,
  `BrakeWall*`, `ForceMaterial`, `vecGravity`, `Flags/DbgFlags/LogFlags`
  (`+`/`-` words, e.g. `+SUB_STEPPING -SETI`; exact meanings unknown),
  `CollisionSolver` (`Restitution`…), and the big `MaterialManager` surface
  table (`myStatic/myKinetic[Soil]`, `SinkFactor`, `SoilThickness/Density`,
  `RollingResistance`, `WaterFactor`, `BumpFrequency/Amplitude`,
  `CollMyHigh/Low`, `eHigh/Low`, `Type` 0/1/2/3 (unknown), `Flags +HARD
  +VALID`).
- **`common.lsp`** — per-car body: `CarBody/CAR_BODY` (`vecCenterOfGravity`),
  `Car/CAR_ROOT` (`CarId`, `MaxSteeringLock`, `SteeringRackRatio`, aero
  `FrontArea`/`DragCoefficient`, `Drive` inertias/brakes/`NumberOfGears`,
  `ControlUnit`), 4× `HingedBody` (doors/bonnet/exhaust: hinge axes/points,
  angle limits, springs, `BodyId`), 3× `MountedBody` (bumpers/wing).
- **`gravel/snow/tarmac.lsp`** — per-surface running gear: 4× `Wheel`
  (`CAR_WHEEL_LF/RF/LB/RB`: mount/axis geometry, `SpringDamper` bump-rubber
  params, `Tyre` carcass stiffness/friction/size) + `Car` (`SetupSpec`).
  The three files differ in ride heights, damper and carcass values.
- **`r_*.lsp` (`("(null)"`)** — setup-option ranges: `CarOptions`
  (`MaxSteeringLockRange*`, rollbar ranges, engine torque curve
  `Tq00…Tq95`, `vecCenterOfGravity`, `InverseMass`), `DriveOptions` (gear
  count, brake pressure ranges, brake-disc physical params),
  `ControlOptions`, `EngineOptions`, per-wheel `SpringDamperOptions*`,
  `TyreOptions*`, `WheelOptions*`.
- **`setups/d_*.lsp` (`("CarSetup"`)** — player setup values: `Car`
  (`MaxSteeringLock`, rollbar stiffness), `Drive` (`*DiffMaxTorque`,
  brake pressures, `GearId0…7`, `FinalDriveId`, `DropGearId`),
  `Engine`/`VehicleControlUnit` (`":-D"` placeholder name, centre-diff
  throttle/brake/speed maps), per-wheel `Wheel*`, `SpringDamper*`, `Tyre*`
  sections. `":-D"` (quoted) is the observed placeholder instance name
  wherever no name applies; its meaning is unknown.
- **`tyres.lsp` (`("(null)"`)** — `TyreTypeNN` dict entries, instance names
  in the form `("(tarmac_dry)")` (parens inside quotes are legal), with rim,
  carcass, wetness, friction/wear/stiffness table keys (`SFric_T*`,
  `CrnStf_L*`, `SlpPk*`, …; exact semantics unknown).
- **`localphysics.lsp`** — flag overrides only (`Flags`/`DbgFlags`/
  `LogFlags`, e.g. `-ENABLE_SINGLESTEPPING`, `+RENDER_SUSPENSION_GEOMETRY`).
- **`prime.lsp`** — archetype `Solid` name entries
  (`("Solid" ("GROUND"))`, …). **`state.lsp`** — a saved snapshot:
  `(("PhysicsState" ("Ground" ("Ground")) ("Peugeot 206" ("Rallybil"
  vecWorldPosition …))))`. **`school/sfgravel.lsp`** — a full `CarSetup`
  like `d_*.lsp`.
- **`repair.ini`** (same dirs, different format — brace/nesting with `;`
  comments, NOT `.lsp`): part count (74) + repair-part tree (physics id,
  importance, debug word, un/mount times + variances, nesting); no parser.
- **`Physics/physics.dat`** (in `physics.rbz`, not per-car): unexamined
  binary (magic bytes `CHELALIC…`); out of scope.

## Reader (`rust/rbr-formats`, module `lsp`)

`parse(&str) -> Result<Vec<Node>, ParseError>` builds a plain tree
(`Atom`/`Quoted`/`List`, no semantics). `RBR_DATA`-gated test parses all 89
retail `.lsp` straight from `physics.rbz`/`misc.rbz`; every file must yield
exactly one top-level list.

## Key tree by file role (retail 1.0 survey)

Survey over all 89 `.lsp` files in `Root/physics.rbz` and `Root/misc.rbz`, parsed with
`rbr_formats::lsp`. Every `(path, key)` was recorded, with its value kinds, how many values
it takes, and the min..max over the files in the role. Paths follow the nesting of named
lists (`("Name" …)`). The materials under `PhysicsEngine/surface types` and the tyre surfaces
(`(tarmac_dry)` …) are collapsed to `<material>` and `<surface>`.

Roles (file counts; 87 `.lsp` in `physics.rbz` + 2 in `misc.rbz`):

| Role | Files | Notes |
|---|---:|---|
| `common` | 8 | `common.lsp`: body, car root, drive, control unit, hinged and mounted bodies |
| `surface` | 24 | `gravel.lsp`, `snow.lsp`, `tarmac.lsp`: wheels and tyres for one surface |
| `r_options` | 24 | `r_gravel.lsp`, `r_snow.lsp`, `r_tarmac.lsp`: option ranges (`(null)` root) |
| `setup` | 27 | `setups/d_*.lsp` (24) and `school/*.lsp` (3): `CarSetup` defaults |
| `tyres` | 2 | `michelin/tyres.lsp`, `pirelli/tyres.lsp`: `TyreType00`…`06`, each with a surface list |
| `physics` | 1 | `physics.lsp`: engine defaults, ground, solids, material table |
| `localphysics` | 1 | flag overrides only |
| `misc` | 2 | `misc/prime.lsp` (archetype solids), `misc/state.lsp` (a saved state) |

Legend: `name:type[min..max]`. Types are `int`, `float`, `string`, and `flag` (`+NAME` /
`-NAME`). `xN` means N values per occurrence; `x{a,b}` means the arity varies. `[v]` alone
means a single value across the role. A bare name with no type (e.g. `CarId`'s sibling
`ControlUnit`) is a key whose value is a sub-list, listed under its own path. `float/int`
means both forms occur. Values are per role, so a range can span cars and surfaces.

### common: 195 keys
- `PhysicsEngine/Car/CAR_ROOT/ControlUnit`: CenterDiffHandbrakeRelease:float[0.5], Gear0Downshift:float[0], Gear0Upshift:float[0], Gear1Downshift:float[0], Gear1Upshift:float[0], Gear2Downshift:float[0], Gear2Upshift:float[6200..8300], Gear3Downshift:float[3000..5400], Gear3Upshift:float[6200..8300], Gear4Downshift:float[3200..5600], Gear4Upshift:float[6200..8300], Gear5Downshift:float[3400..5800], Gear5Upshift:float[6200..8300], Gear6Downshift:float[3400..5800], Gear6Upshift:float[6200..8300], Gear7Downshift:float[3800..5800], Gear7Upshift:float[0], LeftFootBrakeThreshold:float[0.2], RPMLimit:int[8500]
- `PhysicsEngine/Car/CAR_ROOT/Drive`: DriveType:int[3], EngineInertia:float[0.1], GearBoxInertiaIn:float[0.1], GearBoxInertiaOut:float[0.01], MaxHandbrakePressure:int[1.5e+07..3e+07], NumberOfGears:int[8], WheelInertia:float[2]
- `PhysicsEngine/Car/CAR_ROOT/Engine`: Turbo:int[0..1], VE00:float[0.5], VE01:float[0.5..0.6], VE02:float[0.65..0.87], VE03:float[0.7..0.93], VE04:float[0.8..0.98], VE05:float[0.9..1], VE06:float[0.93..1], VE07:float[0.95..1], VE08:float[0.97..0.993], VE09:float[0.98..0.985], VE10:float[0.98..0.99], VE11:float[0.95..1], VE12:float[0.9..1], VE13:float[0.8..0.99], VE14:float[0.5..0.98], VE15:float[0.3..0.8]
- `PhysicsEngine/Car/CAR_ROOT/LightSystem`: BrakeLight0:int[1], BrakeLight1:int[1], BrakeLight2:int[1], HighBeam0:int[1], HighBeam1:int[1], LowBeam0:int[1], LowBeam1:int[1], RearLight0:int[1], RearLight1:int[1], ReverseLight0:int[1], ReverseLight1:int[1]
- `PhysicsEngine/Car/CAR_ROOT`: CarId:int[0..7], ControlUnit, DangerousAcceleration:float[1e+06], DragCoefficient:float[0.4..0.5], Drive, EffWheelRadius:float[0.315], Engine, FrontArea:float[2], InverseMass:float[0.000714..0.000869], LightSystem, MaxSteeringLock:float[2.35], SteeringRackRatio:float[-0.04..0.04], SubTics:int[1], iCollisionMesh:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.0007..0.0025]
- `PhysicsEngine/CarBody/CAR_BODY`: vecCenterOfGravity:float/intx3[-1.45..0.26]
- `PhysicsEngine/HingedBody/CAR_BONNET`: AngularSleepVelocity:float[1], BodyId:int[2], FaceArea:float[2], HingeDefaultAngle:float[-0], HingeMaxAngle:float[-0], HingeMinAngle:float[-1.7..-1.5], InertiaHingeAxis:float[1.5], InverseMass:float[0.2], LinearSleepVelocity:float[0.5], SleepDelay:float[2], Sleepable:int[1], Spring0Damp:float[20], Spring0MaxDisp:float[0.07], Spring0Pos:float[-1.05], Spring0Span:float[1], Spring0Stiff:float[2000], Spring0Type:int[2], vecFaceNormal_BF:floatx3[-0.83939..0], vecHingeAxis_BF:floatx3[0..1], vecHingeAxis_RF:floatx3[0..1], vecHingePoint_BF:floatx3[0..0.7], vecHingePoint_RF:floatx3[-2.265..0.807], vecLocalInverseInertiaDiagonal:floatx3[1], vecZeroAxis_BF:floatx3[0..1], vecZeroAxis_RF:floatx3[0..1]
- `PhysicsEngine/HingedBody/CAR_DOOR_L`: AngularSleepVelocity:float[6], BodyId:int[0..1], FaceArea:float[1], HingeDefaultAngle:float[0.01], HingeMaxAngle:float[0.01], HingeMinAngle:float[-2.1], InertiaHingeAxis:float[5], InverseMass:float[0.05], LinearSleepVelocity:float[0.5], SleepDelay:float[0.5], Sleepable:int[1], Spring0Damp:float[100], Spring0MaxDisp:float[0.05], Spring0Pos:float[-0.75], Spring0Span:float[0.73], Spring0Stiff:float[5000], Spring0Type:int[2], vecFaceNormal_BF:floatx3[-1..0], vecHingeAxis_BF:floatx3[0..1], vecHingeAxis_RF:floatx3[0..1], vecHingePoint_BF:floatx3[-0.5..0], vecHingePoint_RF:floatx3[-2.179..0.836], vecLocalInverseInertiaDiagonal:floatx3[1], vecZeroAxis_BF:floatx3[0..1], vecZeroAxis_RF:floatx3[0..1]
- `PhysicsEngine/HingedBody/CAR_DOOR_R`: AngularSleepVelocity:float[6], BodyId:int[1..2], FaceArea:float[1], HingeDefaultAngle:float[0.01], HingeMaxAngle:float[0.01], HingeMinAngle:float[-2.1], InertiaHingeAxis:float[5], InverseMass:float[0.05], LinearSleepVelocity:float[0.5], SleepDelay:float[0.5], Sleepable:int[1], Spring0Damp:float[100], Spring0MaxDisp:float[0.05], Spring0Pos:float[-0.75], Spring0Span:float[0.73], Spring0Stiff:float[5000], Spring0Type:int[2], vecFaceNormal_BF:floatx3[-1..0], vecHingeAxis_BF:floatx3[0..1], vecHingeAxis_RF:floatx3[-1..0], vecHingePoint_BF:floatx3[-0.5..0], vecHingePoint_RF:floatx3[-2.179..0.537], vecLocalInverseInertiaDiagonal:floatx3[1], vecZeroAxis_BF:floatx3[0..1], vecZeroAxis_RF:floatx3[0..1]
- `PhysicsEngine/HingedBody/CAR_EXHAUSTPIPE`: AngularSleepVelocity:float[1], BodyId:int[3], BreakChain:int[1], HingeDefaultAngle:float[0.05..0.1], HingeMaxAngle:float[0.05..0.2], HingeMinAngle:float[-0.8], InertiaHingeAxis:float[1.5], InverseMass:float[0.2], LinearSleepVelocity:float[0.5], SleepDelay:float[2], Sleepable:int[1], Spring0Damp:float[150], Spring0MaxDisp:float[0.5], Spring0Pos:float[0.05], Spring0Span:float[0.03], Spring0Stiff:float[9000], Spring0Type:int[2], Unlocked:int[1], vecHingeAxis_BF:floatx3[0..1], vecHingeAxis_RF:floatx3[0..1], vecHingePoint_BF:floatx3[-0.3..0], vecHingePoint_RF:floatx3[-0.447..0.3], vecLocalInverseInertiaDiagonal:floatx3[1], vecZeroAxis_BF:floatx3[0..1], vecZeroAxis_RF:floatx3[0..1]
- `PhysicsEngine/MountedBody/CAR_BUMPER_B`: AngularSleepVelocity:float[6], BodyId:int[1], InverseMass:float[0.2], LinearSleepVelocity:float[0.5], SleepDelay:float[0.5], Sleepable:int[1], vecBasePoint_PRF:floatx3[0..0.595], vecLocalInverseInertiaDiagonal:floatx3[1]
- `PhysicsEngine/MountedBody/CAR_BUMPER_F`: AngularSleepVelocity:float[6], BodyId:int[0], InverseMass:float[0.2], LinearSleepVelocity:float[0.5], SleepDelay:float[0.5], Sleepable:int[1], vecBasePoint_PRF:floatx3[-3.156..0.265], vecLocalInverseInertiaDiagonal:floatx3[1]
- `PhysicsEngine/MountedBody/CAR_WING`: AngularSleepVelocity:float[6], BodyId:int[2], InverseMass:float[0.2], LinearSleepVelocity:float[0.5], SleepDelay:float[0.5], Sleepable:int[1], vecBasePoint_PRF:floatx3[-0.087..1.145], vecLocalInverseInertiaDiagonal:floatx3[1]
- `PhysicsEngine`: DefManager
### surface: 181 keys
- `PhysicsEngine/Car/CAR_ROOT/ControlUnit`: CenterDiffHandbrakeRelease:float[0.5], LeftFootBrakeThreshold:float[0.2]
- `PhysicsEngine/Car/CAR_ROOT/Drive`: EngineInertia:float[0.1], GearBoxInertiaIn:float[0.1], GearBoxInertiaOut:float[0.01], MaxHandbrakePressure:int[1.5e+07..3e+07], NumberOfGears:int[7..8], WheelInertia:float[2]
- `PhysicsEngine/Car/CAR_ROOT`: CarId:int[4..5], ControlUnit, DragCoefficient:float[0.4..0.5], Drive, EffWheelRadius:float[0.315], FrontArea:float[2], InverseMass:float[0.000714], MaxSteeringLock:float[2.35], SetupSpec:int[0..2], SteeringRackRatio:float[0.04], SubTics:int[1], iCollisionMesh:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.00076..0.0023]
- `PhysicsEngine/CarBody/CAR_BODY`: vecCenterOfGravity:float/intx3[-1.4..0.22]
- `PhysicsEngine/Wheel/CAR_WHEEL_LB/SpringDamper`: BumpRubberStiffness:int[300000..500000], DampingBumpRubber:int[20000], InnerBumpRubberPos:float[0.12..0.19], OuterBumpRubberPos:float[0.355..0.455]
- `PhysicsEngine/Wheel/CAR_WHEEL_LB/Tyre`: CarcassRadius:float[0.3..0.305], CarcassStiffnessLateral:float[256000..410000], CarcassStiffnessTorsional:float[75000..80000], CarcassWidth:float[0.155..0.225], DebugId:int[2], HubOffset:float[-0.01..0], ReferenceFriction:float[1], TreadDepth:float[0.01..0.015], TreadWidth:float[0.135..0.2]
- `PhysicsEngine/Wheel/CAR_WHEEL_LB`: AntiRollBarId:int[1], AntiRollBarSlot:int[0], DebugId:int[3], DoSuspensionSweep:int[0], DriveAxleMountOffset:float[-0.05], InverseMass:float[0.04], IsSteered:int[0], Radius:float[0.315], SpringDamper, SteeringSign:float[-1], StrutDimX:float[-0.13], StrutDimZ:float[0.425..0.505], TopMountMaxDeformSide:float[0.05], TopMountMaxDeformUp:float[0.05], Tyre, WishboneLength:float[0.45], WishboneSign:float[-1], vecDriveAxleJointPos_VF:floatx3[0..0.26], vecJointAxis_VF:floatx3[-1..0], vecJointMount_VF:floatx3[0..0.27], vecSteeringAxis_VF:floatx3[-0..1], vecSteeringMount_VF:floatx3[0.055..0.27], vecStrutHubAxis_SF:floatx3[0..1], vecStrutHubPoint_SF:floatx3[0..0.11], vecStrutRodMount_SF:floatx3[0..0.14], vecTopMountDeformSideAxis_RF:floatx3[-1..0], vecTopMountDeformUpAxis_RF:floatx3[0..1]
- `PhysicsEngine/Wheel/CAR_WHEEL_LF/SpringDamper`: BumpRubberStiffness:int[300000..500000], DampingBumpRubber:int[20000], InnerBumpRubberPos:float[0.12..0.17], OuterBumpRubberPos:float[0.32..0.41]
- `PhysicsEngine/Wheel/CAR_WHEEL_LF/Tyre`: CarcassRadius:float[0.3..0.305], CarcassStiffnessLateral:float[256000..410000], CarcassStiffnessTorsional:float[75000..80000], CarcassWidth:float[0.155..0.25], DebugId:int[0], HubOffset:float[-0.01..0], ReferenceFriction:float[1], TreadDepth:float[0.01..0.015], TreadWidth:float[0.135..0.225]
- `PhysicsEngine/Wheel/CAR_WHEEL_LF`: AntiRollBarId:int[0], AntiRollBarSlot:int[0], DebugId:int[1], DoSuspensionSweep:int[0], DriveAxleMountOffset:float[-0.05], InverseMass:float[0.04], IsSteered:int[1], Radius:float[0.315], SpringDamper, SteeringSign:float[-1..1], StrutDimX:float[-0.09], StrutDimZ:float[0.425..0.465], TopMountMaxDeformSide:float[0.05], TopMountMaxDeformUp:float[0.05], Tyre, WishboneLength:float[0.32..0.33], WishboneSign:float[-1], vecDriveAxleJointPos_VF:floatx3[-2.55..0.3], vecJointAxis_VF:floatx3[-1..0], vecJointMount_VF:floatx3[-2.59..0.38], vecSteeringAxis_VF:floatx3[-0..1], vecSteeringMount_VF:floatx3[-2.71..0.37], vecStrutHubAxis_SF:floatx3[0..1], vecStrutHubPoint_SF:floatx3[0..0.11], vecStrutRodMount_SF:floatx3[-0.12..0.12], vecTopMountDeformSideAxis_RF:floatx3[-1..0], vecTopMountDeformUpAxis_RF:floatx3[0..1]
- `PhysicsEngine/Wheel/CAR_WHEEL_RB/SpringDamper`: BumpRubberStiffness:int[300000..500000], DampingBumpRubber:int[20000], InnerBumpRubberPos:float[0.12..0.19], OuterBumpRubberPos:float[0.355..0.455]
- `PhysicsEngine/Wheel/CAR_WHEEL_RB/Tyre`: CarcassRadius:float[0.3..0.305], CarcassStiffnessLateral:float[256000..410000], CarcassStiffnessTorsional:float[75000..80000], CarcassWidth:float[0.155..0.225], DebugId:int[3], HubOffset:float[-0.01..0], ReferenceFriction:float[1], TreadDepth:float[0.01..0.015], TreadWidth:float[0.135..0.2]
- `PhysicsEngine/Wheel/CAR_WHEEL_RB`: AntiRollBarId:int[1], AntiRollBarSlot:int[1], DebugId:int[4], DriveAxleMountOffset:float[-0.05], InverseMass:float[0.04], IsSteered:int[0], Radius:float[0.315], SpringDamper, SteeringSign:float[1], StrutDimX:float[-0.13], StrutDimZ:float[0.425..0.505], TopMountMaxDeformSide:float[0.05], TopMountMaxDeformUp:float[0.05], Tyre, WishboneLength:float[0.45], WishboneSign:float[1], vecDriveAxleJointPos_VF:floatx3[-0.26..0.19], vecJointAxis_VF:floatx3[-1..0], vecJointMount_VF:floatx3[-0.27..0.08], vecSteeringAxis_VF:floatx3[-0..1], vecSteeringMount_VF:floatx3[-0.27..0.15], vecStrutHubAxis_SF:floatx3[0..1], vecStrutHubPoint_SF:floatx3[0..0.11], vecStrutRodMount_SF:floatx3[-0.14..0], vecTopMountDeformSideAxis_RF:floatx3[-1..1], vecTopMountDeformUpAxis_RF:floatx3[0..1]
- `PhysicsEngine/Wheel/CAR_WHEEL_RF/SpringDamper`: BumpRubberStiffness:int[300000..500000], DampingBumpRubber:int[20000], InnerBumpRubberPos:float[0.12..0.17], OuterBumpRubberPos:float[0.32..0.41]
- `PhysicsEngine/Wheel/CAR_WHEEL_RF/Tyre`: CarcassRadius:float[0.3..0.305], CarcassStiffnessLateral:float[256000..410000], CarcassStiffnessTorsional:float[75000..80000], CarcassWidth:float[0.155..0.225], DebugId:int[1], HubOffset:float[-0.05..0], ReferenceFriction:float[1], TreadDepth:float[0.01..0.015], TreadWidth:float[0.135..0.2]
- `PhysicsEngine/Wheel/CAR_WHEEL_RF`: AntiRollBarId:int[0], AntiRollBarSlot:int[1], DebugId:int[2], DriveAxleMountOffset:float[-0.05], InverseMass:float[0.04], IsSteered:int[1], Radius:float[0.315], SpringDamper, SteeringSign:float[-1..1], StrutDimX:float[-0.09], StrutDimZ:float[0.425..0.465], TopMountMaxDeformSide:float[0.05], TopMountMaxDeformUp:float[0.05], Tyre, WishboneLength:float[0.32..0.33], WishboneSign:float[1], vecDriveAxleJointPos_VF:floatx3[-2.55..0.22], vecJointAxis_VF:floatx3[-1..0], vecJointMount_VF:floatx3[-2.59..0.075], vecSteeringAxis_VF:floatx3[-0..1], vecSteeringMount_VF:floatx3[-2.71..0.098], vecStrutHubAxis_SF:floatx3[0..1], vecStrutHubPoint_SF:floatx3[0..0.11], vecStrutRodMount_SF:floatx3[-0.12..0.12], vecTopMountDeformSideAxis_RF:floatx3[0..1], vecTopMountDeformUpAxis_RF:floatx3[0..1]
- `PhysicsEngine`: DefManager
### r_options: 251 keys
- `(null)/(null)`: AntiRollBarId:int[0..1], AntiRollBarSlot:int[0..1], CenterDiffHandbrakeReleaseRangeMax:float[1], CenterDiffHandbrakeReleaseRangeMin:float[0], CenterDiffHandbrakeReleaseRangeStep:float[0.05], CenterDiffTorqueMax:float[900], CenterDiffTorqueMin:float[0], CenterDiffTorqueStep:float[10], DampBumpHiSpeedMax:float[10000], DampBumpHiSpeedMin:float[1000], DampBumpHiSpeedStep:float[100], DampBumpMax:float[10000], DampBumpMin:float[1000], DampBumpStep:float[100], DampReboundMax:float[10000], DampReboundMin:float[1000], DampReboundStep:float[100], DampSpeedBrakeMax:float[5], DampSpeedBrakeMin:float[0], DampSpeedBrakeStep:float[0.1], DiffMapLockRangeMax:float[1], DiffMapLockRangeMin:float[0], DiffMapLockRangeStep:float[0.01], DiffMapSpeedRangeMax:float[70], DiffMapSpeedRangeMin:float[0], DiffMapSpeedRangeStep:float[0.555556], DropGearOptionsMax:int[12], DropGearOptionsMin:int[10], DummyMax:int[0..7], DummyMin:int[0..2], DummyStep:float[0], FinalDriveOptionsMax:int[9], FinalDriveOptionsMin:int[9], FrontBrake0Diameter:float[0.38], FrontBrake0HeatAbsRatio:float[0.95], FrontBrake0Inertia:float[0.5], FrontBrake0IntConvec:float[400], FrontBrake0Mass:float[3.1], FrontBrake0PistonArea:float[0.0023], FrontBrake0PlateArea:float[0.1], FrontBrake0RadiusEff:float[0.18], FrontBrake0SpecHeat:int[450], FrontBrake0VentArea:float[0.1], FrontBrake1Diameter:float[0.38], FrontBrake1HeatAbsRatio:float[0.95], FrontBrake1Inertia:float[0.5], FrontBrake1IntConvec:float[400], FrontBrake1Mass:float[3.1], FrontBrake1PistonArea:float[0.0023], FrontBrake1PlateArea:float[0.1], FrontBrake1RadiusEff:float[0.18], FrontBrake1SpecHeat:int[450], FrontBrake1VentArea:float[0.1], FrontBrakePressureMax:float[1e+07], FrontBrakePressureMin:float[0], FrontBrakePressureStep:float[10000], FrontBrakes:int[1], FrontDiffTorqueMax:float[900], FrontDiffTorqueMin:float[0], FrontDiffTorqueStep:float[10], FrontRollBarStiffnessMax:float[50000], FrontRollBarStiffnessMin:float[0], FrontRollBarStiffnessStep:float[200], Gear0OptionsMax:int[2], Gear0OptionsMin:int[2], Gear1OptionsMax:int[0], Gear1OptionsMin:int[0], Gear2OptionsMax:int[3], Gear2OptionsMin:int[3], Gear3OptionsMax:int[4], Gear3OptionsMin:int[4], Gear4OptionsMax:int[5], Gear4OptionsMin:int[5], Gear5OptionsMax:int[6], Gear5OptionsMin:int[6], Gear6OptionsMax:int[7], Gear6OptionsMin:int[7], Gear7OptionsMax:int[8], Gear7OptionsMin:int[8], GearList00Gear:int[-29], GearList00Pinion:int[14], GearList00TorqueMax:float[10000], GearList00TorqueMin:float[-8000], GearList01Gear:int[-32], GearList01Pinion:int[14], GearList01TorqueMax:float[10000], GearList01TorqueMin:float[-8000], GearList02Gear:int[0], GearList02Pinion:int[0], GearList02TorqueMax:float[10000], GearList02TorqueMin:float[-8000], GearList03Gear:int[38], GearList03Pinion:int[11..13], GearList03TorqueMax:float[10000], GearList03TorqueMin:float[-8000], GearList04Gear:int[34], GearList04Pinion:int[15..16], GearList04TorqueMax:float[10000], GearList04TorqueMin:float[-8000], GearList05Gear:int[28..30], GearList05Pinion:int[17..19], GearList05TorqueMax:float[10000], GearList05TorqueMin:float[-8000], GearList06Gear:int[26..28], GearList06Pinion:int[21..23], GearList06TorqueMax:float[10000], GearList06TorqueMin:float[-8000], GearList07Gear:int[25], GearList07Pinion:int[23..28], GearList07TorqueMax:float[10000], GearList07TorqueMin:float[-8000], GearList08Gear:int[25], GearList08Pinion:int[28], GearList08TorqueMax:float[10000], GearList08TorqueMin:float[-8000], GearList09Gear:int[35..38], GearList09Pinion:int[9], GearList09TorqueMax:float[10000], GearList09TorqueMin:float[-8000], GearList10Gear:int[21], GearList10Pinion:int[28], GearList10TorqueMax:float[10000], GearList10TorqueMin:float[-8000], GearList11Gear:int[22], GearList11Pinion:int[28], GearList11TorqueMax:float[10000], GearList11TorqueMin:float[-8000], GearList12Gear:int[22], GearList12Pinion:int[26], GearList12TorqueMax:float[10000], GearList12TorqueMin:float[-8000], Gears:int[7..8], HelperSpringLengthMax:float[0.2], HelperSpringLengthMin:float[0.05], HelperSpringLengthStep:float[0.005], HelperSpringStiffnessMax:float[30000], HelperSpringStiffnessMin:float[5000], HelperSpringStiffnessStep:float[1000], InnerBumpRubberPos:float[0.12..0.19], InverseMass:float[0.000714..0.000869], LeftFootBrakeThresholdRangeMax:float[1], LeftFootBrakeThresholdRangeMin:float[0], LeftFootBrakeThresholdRangeStep:float[0.05], MaxSteeringLockRangeMax:float[2.2], MaxSteeringLockRangeMin:float[1], MaxSteeringLockRangeStep:float[0.02], OuterBumpRubberPos:float[0.32..0.455], PlatformHeightMax:float[0.05], PlatformHeightMin:float[0], PlatformHeightStep:float[0.001], PressureMax:float[250000..300000], PressureMin:float[100000..150000], PressureStep:float[1000], RearBrake0Diameter:float[0.38], RearBrake0HeatAbsRatio:float[0.95], RearBrake0Inertia:float[0.5], RearBrake0IntConvec:float[400], RearBrake0Mass:float[3.1], RearBrake0PistonArea:float[0.0023], RearBrake0PlateArea:float[0.1], RearBrake0RadiusEff:float[0.18], RearBrake0SpecHeat:int[450], RearBrake0VentArea:float[0.1], RearBrake1Diameter:float[0.38], RearBrake1HeatAbsRatio:float[0.95], RearBrake1Inertia:float[0.5], RearBrake1IntConvec:float[400], RearBrake1Mass:float[3.1], RearBrake1PistonArea:float[0.0023], RearBrake1PlateArea:float[0.1], RearBrake1RadiusEff:float[0.18], RearBrake1SpecHeat:int[450], RearBrake1VentArea:float[0.1], RearBrakePressureMax:float[1e+07], RearBrakePressureMin:float[0], RearBrakePressureStep:float[10000], RearBrakes:int[1], RearDiffTorqueMax:float[900], RearDiffTorqueMin:float[0], RearDiffTorqueStep:float[10], RearRollBarStiffnessMax:float[50000], RearRollBarStiffnessMin:float[0], RearRollBarStiffnessStep:float[200], SpringLengthMax:float[0.35], SpringLengthMin:float[0.15], SpringLengthStep:float[0.005], SpringStiffnessMax:float[90000], SpringStiffnessMin:float[15000], SpringStiffnessStep:float[1000], SteeringRodMax:float[0.3375..0.455], SteeringRodMin:float[0.32..0.445], SteeringRodStep:float[5e-05], SteeringSign:float[-1..1], StrutDimX:float[-0.13..-0.09], StrutDimZ:float[0.425..0.505], Tq00:float[0], Tq05:float[0], Tq10:float[100..171], Tq15:float[117..226], Tq20:float[127..285], Tq25:float[143..412], Tq30:float[161..502], Tq35:float[169..507], Tq40:float[171..513], Tq45:float[175..493], Tq50:float[177..453], Tq55:float[179..419], Tq60:float[181..379], Tq65:float[184..331], Tq70:float[185..297], Tq75:float[184..283], Tq80:float[183..268], Tq85:float[0..176], Tq90:float[0..167], Tq95:float[0], WheelAxisInclMax:float[0..0.1], WheelAxisInclMin:float[-0.2..-0.1], WheelAxisInclStep:float[0.001], WishboneLength:float[0.32..0.45], WishboneSign:float[-1..1], nTopMountPositions:int[0..6], vecCenterOfGravity:float/intx3[-1.45..0.26], vecJointAxis_VF:floatx3[-1..0], vecJointMount_VF:floatx3[-2.59..0.38], vecSteeringMount_VF:floatx3[-2.71..0.37], vecStrutHubAxis_SF:floatx3[0..1], vecStrutHubPoint_SF:floatx3[0..0.11], vecStrutRodMount_SF:floatx3[-0.14..0.14], vecTopMount00:floatx3[-2.5325..0.74], vecTopMount01:floatx3[-2.535..0.7], vecTopMount02:floatx3[-2.5375..0.7], vecTopMount03:floatx3[-2.54..0.7], vecTopMount04:floatx3[-2.5425..0.7], vecTopMount05:floatx3[-2.545..0.7], vecTopMount06:floatx3[-2.5475..0.7]
- `(null)`: CarOptions, ControlOptions, DriveOptions, EngineOptions, SpringDamperOptionsLB, SpringDamperOptionsLF, SpringDamperOptionsRB, SpringDamperOptionsRF, TyreOptionsLB, TyreOptionsLF, TyreOptionsRB, TyreOptionsRF, WheelOptionsLB, WheelOptionsLF, WheelOptionsRB, WheelOptionsRF
### setup: 228 keys
- `CarSetup/<:-D>`: BumpHighSpeedBreak:float[1..1.5], CenterDiffBrake_00:float[0], CenterDiffBrake_01:float[0.1..1], CenterDiffBrake_02:float[0.2..1], CenterDiffBrake_03:float[0.4..1], CenterDiffBrake_04:float[0.7..1], CenterDiffBrake_05:float[1], CenterDiffBrake_06:float[1], CenterDiffBrake_07:float[1], CenterDiffBrake_08:float[1], CenterDiffBrake_09:float[1], CenterDiffBrake_10:float[1], CenterDiffHandbrakeRelease:float[0.5], CenterDiffMaxTorque:float[700..800], CenterDiffThrottle_00:float[0], CenterDiffThrottle_01:float[0.0625], CenterDiffThrottle_02:float[0.1], CenterDiffThrottle_03:float[0.25], CenterDiffThrottle_04:float[0.375], CenterDiffThrottle_05:float[0.5], CenterDiffThrottle_06:float[0.625], CenterDiffThrottle_07:float[0.75], CenterDiffThrottle_08:float[1], CenterDiffThrottle_09:float[1], CenterDiffThrottle_10:float[1], CenterSpeedMapFactor_00:float[0], CenterSpeedMapFactor_01:float[0..0.1], CenterSpeedMapFactor_02:float[0.1..0.2], CenterSpeedMapFactor_03:float[0.2..0.4], CenterSpeedMapFactor_04:float[0.3..0.6], CenterSpeedMapFactor_05:float[0.4..1], CenterSpeedMapFactor_06:float[0.7..1], CenterSpeedMapFactor_07:float[1], CenterSpeedMapFactor_08:float[1], CenterSpeedMapFactor_09:float[1], CenterSpeedMapFactor_10:float[1], CenterSpeedMapVelocity_00:float[0], CenterSpeedMapVelocity_01:float[5.56], CenterSpeedMapVelocity_02:float[11.1], CenterSpeedMapVelocity_03:float[16.7], CenterSpeedMapVelocity_04:float[22.2], CenterSpeedMapVelocity_05:float[27.8], CenterSpeedMapVelocity_06:float[33.3], CenterSpeedMapVelocity_07:float[38.9], CenterSpeedMapVelocity_08:float[44.4], CenterSpeedMapVelocity_09:float[50], CenterSpeedMapVelocity_10:float[55.6], DampingBump:int[4200..6000], DampingBumpHighSpeed:int[3100..7900], DampingRebound:int[4700..9000], DropGearId:int[10..11], Dummy:int/float[0..2], FinalDriveId:int[9], FrontDiffBrake_00:float[0], FrontDiffBrake_01:float[0], FrontDiffBrake_02:float[0.167], FrontDiffBrake_03:float[0.33], FrontDiffBrake_04:float[0.5], FrontDiffBrake_05:float[0.67], FrontDiffBrake_06:float[0.833], FrontDiffBrake_07:float[1], FrontDiffBrake_08:float[1], FrontDiffBrake_09:float[1], FrontDiffBrake_10:float[1], FrontDiffMaxTorque:float[200..320], FrontDiffThrottle_00:float[0], FrontDiffThrottle_01:float[0], FrontDiffThrottle_02:float[0.167], FrontDiffThrottle_03:float[0.33], FrontDiffThrottle_04:float[0.5], FrontDiffThrottle_05:float[0.67], FrontDiffThrottle_06:float[0.833], FrontDiffThrottle_07:float[1], FrontDiffThrottle_08:float[1], FrontDiffThrottle_09:float[1], FrontDiffThrottle_10:float[1], FrontSpeedMapFactor_00:float[0], FrontSpeedMapFactor_01:float[0], FrontSpeedMapFactor_02:float[0], FrontSpeedMapFactor_03:float[0], FrontSpeedMapFactor_04:float[0..0.16], FrontSpeedMapFactor_05:float[0..0.267], FrontSpeedMapFactor_06:float[0.1..0.33], FrontSpeedMapFactor_07:float[0.2..0.4], FrontSpeedMapFactor_08:float[0.3..0.5], FrontSpeedMapFactor_09:float[0.67], FrontSpeedMapFactor_10:float[1], FrontSpeedMapVelocity_00:float[0], FrontSpeedMapVelocity_01:float[2.78], FrontSpeedMapVelocity_02:float[5.56], FrontSpeedMapVelocity_03:float[8.33], FrontSpeedMapVelocity_04:float[11.1], FrontSpeedMapVelocity_05:float[13.8], FrontSpeedMapVelocity_06:float[16.7], FrontSpeedMapVelocity_07:float[22.2], FrontSpeedMapVelocity_08:float[27.7], FrontSpeedMapVelocity_09:float[38.9], FrontSpeedMapVelocity_10:float[55.6], GearId0:int[1], GearId1:int[2], GearId2:int[3], GearId3:int[4], GearId4:int[5], GearId5:int[6], GearId6:int[7], GearId7:int[8], HelperSpringLength:float[0.05..0.07], HelperSpringMinLength:float[0.02..0.03], HelperSpringStiffness:int[10000..15000], LFCenterDiffBrake_00:float[0], LFCenterDiffBrake_01:float[0], LFCenterDiffBrake_02:float[0], LFCenterDiffBrake_03:float[0..0.2], LFCenterDiffBrake_04:float[0..0.3], LFCenterDiffBrake_05:float[0.2..0.5], LFCenterDiffBrake_06:float[0.4..0.8], LFCenterDiffBrake_07:float[0.7..1], LFCenterDiffBrake_08:float[1], LFCenterDiffBrake_09:float[1], LFCenterDiffBrake_10:float[1], LFCenterDiffThrottle_00:float[0], LFCenterDiffThrottle_01:float[0.0625], LFCenterDiffThrottle_02:float[0.1], LFCenterDiffThrottle_03:float[0.25], LFCenterDiffThrottle_04:float[0.375], LFCenterDiffThrottle_05:float[0.5], LFCenterDiffThrottle_06:float[0.625], LFCenterDiffThrottle_07:float[0.75], LFCenterDiffThrottle_08:float[1], LFCenterDiffThrottle_09:float[1], LFCenterDiffThrottle_10:float[1], LFCenterSpeedMapFactor_00:float[0], LFCenterSpeedMapFactor_01:float[0..0.1], LFCenterSpeedMapFactor_02:float[0.1..0.2], LFCenterSpeedMapFactor_03:float[0.2..0.3], LFCenterSpeedMapFactor_04:float[0.3..0.4], LFCenterSpeedMapFactor_05:float[0.4..0.5], LFCenterSpeedMapFactor_06:float[0.6..0.7], LFCenterSpeedMapFactor_07:float[0.7..1], LFCenterSpeedMapFactor_08:float[0.8..1], LFCenterSpeedMapFactor_09:float[0.9..1], LFCenterSpeedMapFactor_10:float[1], LFCenterSpeedMapVelocity_00:float[0], LFCenterSpeedMapVelocity_01:float[5.56], LFCenterSpeedMapVelocity_02:float[11.1], LFCenterSpeedMapVelocity_03:float[16.7], LFCenterSpeedMapVelocity_04:float[22.2], LFCenterSpeedMapVelocity_05:float[27.8], LFCenterSpeedMapVelocity_06:float[33.3], LFCenterSpeedMapVelocity_07:float[38.9], LFCenterSpeedMapVelocity_08:float[44.4], LFCenterSpeedMapVelocity_09:float[50], LFCenterSpeedMapVelocity_10:float[55.6], LeftFootBrakeThreshold:float[0.2], MaxBrakePressureFront:int[6.5e+06..1e+07], MaxBrakePressureRear:int[5e+06..5.4e+06], Pressure:int/float[200000..280000], RearDiffBrake_00:float[0..1], RearDiffBrake_01:float[0.3..1], RearDiffBrake_02:float[0.7..1], RearDiffBrake_03:float[1], RearDiffBrake_04:float[1], RearDiffBrake_05:float[1], RearDiffBrake_06:float[1], RearDiffBrake_07:float[1], RearDiffBrake_08:float[1], RearDiffBrake_09:float[1], RearDiffBrake_10:float[1], RearDiffMaxTorque:float[600..900], RearDiffThrottle_00:float[0..1], RearDiffThrottle_01:float[0..1], RearDiffThrottle_02:float[0..1], RearDiffThrottle_03:float[0.25..1], RearDiffThrottle_04:float[0.5..1], RearDiffThrottle_05:float[0.75..1], RearDiffThrottle_06:float[1], RearDiffThrottle_07:float[1], RearDiffThrottle_08:float[1], RearDiffThrottle_09:float[1], RearDiffThrottle_10:float[1], RearSpeedMapFactor_00:float[0..1], RearSpeedMapFactor_01:float[0..1], RearSpeedMapFactor_02:float[0..1], RearSpeedMapFactor_03:float[0..1], RearSpeedMapFactor_04:float[0..1], RearSpeedMapFactor_05:float[0..0.85], RearSpeedMapFactor_06:float[0..0.7], RearSpeedMapFactor_07:float[0..0.55], RearSpeedMapFactor_08:float[0..0.4], RearSpeedMapFactor_09:float[0..0.3], RearSpeedMapFactor_10:float[0..0.2], RearSpeedMapVelocity_00:float[0], RearSpeedMapVelocity_01:float[5.56], RearSpeedMapVelocity_02:float[11.1], RearSpeedMapVelocity_03:float[16.7], RearSpeedMapVelocity_04:float[22.2], RearSpeedMapVelocity_05:float[27.8], RearSpeedMapVelocity_06:float[33.3], RearSpeedMapVelocity_07:float[38.9], RearSpeedMapVelocity_08:float[44.4], RearSpeedMapVelocity_09:float[50], RearSpeedMapVelocity_10:float[55.6], SpringLength:float[0.2..0.31], SpringStiffness:int[40000..75000], SteeringRodLength:float[0.3311..0.45046], StrutPlatformHeight:float[0.01..0.035], TopMountSlot:int[0..6], WheelAxisInclination:float[-0.095..0.005], vecTopMountPosition:floatx3[-2.545..0.74]
- `CarSetup/Car`: FrontRollBarStiffness:float/int[10000..28000], MaxSteeringLock:float[1.4], RearRollBarStiffness:float/int[13000..30000]
- `CarSetup`: Car, Drive, Engine, SpringDamperLB, SpringDamperLF, SpringDamperRB, SpringDamperRF, TyreLB, TyreLF, TyreRB, TyreRF, VehicleControlUnit, WheelLB, WheelLF, WheelRB, WheelRF
### tyres: 315 keys
- `(null)/<surface>`: CarcassRadius:float[0.305], CarcassStiffnessLateral:float[256000], CarcassStiffnessTorsional:float[75000], CarcassStiffnessVertical:float[200000], CarcassThickness:float[0.005], CarcassWidth:float[0.225], CrnMyFct_L0:float[1.01], CrnMyFct_L1:float[1.01], CrnMyFct_L2:float[0.99], CrnMyFct_L3:float[0.96], CrnMyFct_L4:float[0.94], CrnMyFct_L5:float[0.92], CrnMyFct_L6:float[0.88], CrnMyFct_L7:float[0.7], CrnStf_L0:int[6000], CrnStf_L1:int[18510], CrnStf_L2:int[24452], CrnStf_L3:int[29534], CrnStf_L4:int[33659], CrnStf_L5:int[35702], CrnStf_L6:int[42000], CrnStf_L7:int[45000], HubOffset:float[0], PressureRatio:float[0.7], RimInnerRadius:float[0.18], RimRadius:float[0.2], RubMyFct_T0:float[0.9], RubMyFct_T1:float[0.97], RubMyFct_T2:float[0.99], RubMyFct_T3:float[1], RubMyFct_T4:float[0.95], RubMyFct_T5:float[0.9], RubMyFct_T6:float[0.6], RubMyFct_T7:float[0.5], RubWear_T0:float[0.5], RubWear_T1:float[0.8], RubWear_T2:float[1], RubWear_T3:float[1.4], RubWear_T4:float[2], RubWear_T5:float[4], RubWear_T6:float[10], RubWear_T7:float[10], SFric_T0:float[1], SFric_T1:float[1], SFric_T2:float[0.7], SFric_T3:float[0.4], SFric_T4:float[0.85], SFric_T5:float[0.75], SideWallHeight:float[0.065], SlpPkCrn_L0:float[0.309923], SlpPkCrn_L1:float[0.322618], SlpPkCrn_L2:float[0.332085], SlpPkCrn_L3:float[0.343071], SlpPkCrn_L4:float[0.35399], SlpPkCrn_L5:float[0.369471], SlpPkCrn_L6:float[0.4], SlpPkCrn_L7:float[0.473576], SlpPkTrct_L0:float[0.25], SlpPkTrct_L1:float[0.18], SlpPkTrct_L2:float[0.16], SlpPkTrct_L3:float[0.15], SlpPkTrct_L4:float[0.14], SlpPkTrct_L5:float[0.135], SlpPkTrct_L6:float[0.128], SlpPkTrct_L7:float[0.11], StfFct_P0:float[1], StfFct_P1:float[1], StfFct_P2:float[1], StfFct_P3:float[1], TrctMyFct_L0:float[1.05], TrctMyFct_L1:float[1.05], TrctMyFct_L2:float[1.03], TrctMyFct_L3:float[1], TrctMyFct_L4:float[0.98], TrctMyFct_L5:float[0.96], TrctMyFct_L6:float[0.92], TrctMyFct_L7:float[0.8], TrctStf_L0:int[1000], TrctStf_L1:int[33150], TrctStf_L2:int[60000], TrctStf_L3:int[90450], TrctStf_L4:int[124090], TrctStf_L5:int[142920], TrctStf_L6:int[142920], TrctStf_L7:int[142920], TreadCoverage:float[0.6], TreadDepth:float[0.01], TreadElasticity:float[1.5e+06], TreadWidth:float[0.2], Type:int[3], WearFtr_T0:float[0.9], WearFtr_T1:float[0.9], WearFtr_T2:float[0.9], WearFtr_T3:float[0.9], WearFtr_T4:float[0.9], WearFtr_T5:float[0.9], WetSlope_T0:float[-0.1], WetSlope_T1:float[-0.1], WetSlope_T2:float[-0.2], WetSlope_T3:float[0], WetSlope_T4:float[-0.1], WetSlope_T5:float[-0.1], WetnessDesign:float[0], WetnessWearFtr:float[0.95], RubMyFct_T1:float[0.99], RubMyFct_T2:float[1], RubMyFct_T4:float[0.99], RubMyFct_T5:float[0.7], RubMyFct_T6:float[0.5], RubMyFct_T7:float[0.3], RubWear_T0:float[0.1], RubWear_T3:float[2], RubWear_T4:float[3], Type:int[4], WetnessDesign:float[0.5], WetnessWearFtr:float[0.9], Type:int[5], WetnessDesign:float[1], WetnessWearFtr:float[0.85], CarcassStiffnessLateral:float[302000], CarcassStiffnessTorsional:float[80000], CarcassStiffnessVertical:float[300000], CrnMyFct_L0:float[0.9], CrnMyFct_L1:float[0.89], CrnMyFct_L2:float[0.89], CrnMyFct_L3:float[0.88], CrnMyFct_L4:float[0.88], CrnMyFct_L5:float[0.88], CrnMyFct_L7:float[0.88], CrnStf_L0:int[16000], CrnStf_L1:int[28510], CrnStf_L2:int[34452], CrnStf_L3:int[39534], CrnStf_L4:int[43659], CrnStf_L5:int[45702], CrnStf_L6:int[52000], CrnStf_L7:int[55000], PressureRatio:float[0.8], RubMyFct_T0:float[1], RubMyFct_T1:float[1], RubMyFct_T4:float[1], RubMyFct_T5:float[1], RubMyFct_T6:float[1], RubMyFct_T7:float[1], SFric_T0:float[0.9], SFric_T1:float[0.8], SFric_T2:float[0.6], SFric_T3:float[1], SideWallHeight:float[0.09], SlpPkCrn_L0:float[0.259923], SlpPkCrn_L1:float[0.272618], SlpPkCrn_L2:float[0.282085], SlpPkCrn_L3:float[0.293071], SlpPkCrn_L4:float[0.30399], SlpPkCrn_L5:float[0.319471], SlpPkCrn_L6:float[0.35], SlpPkCrn_L7:float[0.403576], SlpPkTrct_L0:float[0.35], SlpPkTrct_L1:float[0.28], SlpPkTrct_L2:float[0.26], SlpPkTrct_L3:float[0.25], SlpPkTrct_L4:float[0.24], SlpPkTrct_L5:float[0.235], SlpPkTrct_L6:float[0.228], SlpPkTrct_L7:float[0.21], TrctMyFct_L0:float[0.98], TrctMyFct_L1:float[0.99], TrctMyFct_L2:float[0.995], TrctMyFct_L4:float[0.995], TrctMyFct_L5:float[0.99], TrctMyFct_L6:float[0.98], TrctMyFct_L7:float[0.92], TreadCoverage:float[0.5], TreadElasticity:float[5e+06], Type:int[6], WetSlope_T0:float[0], WetSlope_T1:float[0], WetSlope_T2:float[0], WetSlope_T4:float[0], WetSlope_T5:float[0], CarcassStiffnessLateral:float[410000], CarcassStiffnessVertical:float[250000], CarcassWidth:float[0.25], CrnMyFct_L0:float[1.1], CrnMyFct_L1:float[1.05], CrnMyFct_L2:float[1.01], CrnMyFct_L4:float[0.93], CrnMyFct_L5:float[0.9], CrnMyFct_L6:float[0.86], CrnMyFct_L7:float[0.75], CrnStf_L0:float[55000], CrnStf_L1:float[60623], CrnStf_L2:float[65787], CrnStf_L3:float[70692], CrnStf_L4:float[74037], CrnStf_L5:float[78459], CrnStf_L6:float[82000], CrnStf_L7:float[86000], RimInnerRadius:float[0.22], RimRadius:float[0.24], RubMyFct_T0:float[0.65], RubMyFct_T2:float[0.993], RubMyFct_T4:float[0.9], RubMyFct_T5:float[0.78], RubMyFct_T6:float[0.7], RubMyFct_T7:float[0.65], RubWear_T1:float[0.9], RubWear_T3:float[1.07], RubWear_T4:float[1.2], RubWear_T5:float[3], SFric_T0:float[0.8], SFric_T2:float[1], SFric_T3:float[0.2], SFric_T4:float[0.8], SFric_T5:float[0.7], SideWallHeight:float[0.045], SlpPkCrn_L0:float[0.221799], SlpPkCrn_L1:float[0.201657], SlpPkCrn_L2:float[0.188496], SlpPkCrn_L3:float[0.172788], SlpPkCrn_L4:float[0.156589], SlpPkCrn_L5:float[0.149862], SlpPkCrn_L6:float[0.144626], SlpPkCrn_L7:float[0.13439], SlpPkTrct_L0:float[0.16], SlpPkTrct_L1:float[0.15], SlpPkTrct_L2:float[0.145], SlpPkTrct_L3:float[0.14], SlpPkTrct_L4:float[0.135], SlpPkTrct_L5:float[0.125], SlpPkTrct_L6:float[0.11], SlpPkTrct_L7:float[0.09], StfFct_P0:float[0.5], StfFct_P1:float[0.9], StfFct_P2:float[1.1], StfFct_P3:float[1.5], TrctMyFct_L0:float[1.13], TrctMyFct_L2:float[1.04], TrctMyFct_L4:float[0.97], TrctMyFct_L5:float[0.95], TrctMyFct_L6:float[0.9], TrctStf_L0:float[70000], TrctStf_L1:float[80030], TrctStf_L2:float[85030], TrctStf_L3:float[90310], TrctStf_L4:float[95560], TrctStf_L5:float[103810], TrctStf_L6:float[108000], TrctStf_L7:float[115810], TreadCoverage:float[0.83], TreadElasticity:float[1e+06], TreadWidth:float[0.225], Type:int[0], WetSlope_T0:float[-0.3], WetSlope_T1:float[-0.2], WetSlope_T2:float[-0.3], WetSlope_T4:float[-0.3], WetSlope_T5:float[-0.4], WetnessWearFtr:float[1], RubMyFct_T0:float[0.8], RubMyFct_T1:float[0.965], RubMyFct_T2:float[0.995], RubMyFct_T4:float[0.88], RubMyFct_T5:float[0.76], RubMyFct_T6:float[0.69], RubMyFct_T7:float[0.6], RubWear_T1:float[1], RubWear_T2:float[1.2], RubWear_T3:float[1.5], RubWear_T4:float[6], RubWear_T5:float[9], Type:int[1], CarcassStiffnessLateral:float[300000], CarcassStiffnessTorsional:float[70000], CrnMyFct_L0:float[1.08], CrnMyFct_L1:float[1.04], CrnMyFct_L2:float[1], CrnMyFct_L3:float[0.95], CrnMyFct_L4:float[0.92], CrnStf_L0:float[43500], CrnStf_L1:float[47436], CrnStf_L2:float[51050], CrnStf_L3:float[54484], CrnStf_L4:float[56825], CrnStf_L5:float[59921], CrnStf_L6:float[62399], CrnStf_L7:float[65200], RubMyFct_T0:float[0.95], RubMyFct_T3:float[0.98], RubMyFct_T4:float[0.85], RubMyFct_T5:float[0.75], RubMyFct_T6:float[0.66], RubWear_T0:float[0.6], RubWear_T3:float[2.5], RubWear_T4:float[5], SFric_T0:float[0.87], SFric_T1:float[0.87], TrctStf_L0:float[60000], TrctStf_L1:float[70030], TrctStf_L2:float[75030], TrctStf_L3:float[80310], TrctStf_L4:float[85560], TrctStf_L5:float[93810], TrctStf_L6:float[98000], TrctStf_L7:float[105810], TreadCoverage:float[0.78], Type:int[2], WetSlope_T3:float[-0]
- `(null)`: TyreType00, TyreType01, TyreType02, TyreType03, TyreType04, TyreType05, TyreType06
### physics: 261 keys
- `PhysicsEngine/Car/CAR_ROOT/ControlUnit`: Gear0Downshift:float[0], Gear0Upshift:float[0], Gear1Downshift:float[0], Gear1Upshift:float[0], Gear2Downshift:float[0], Gear2Upshift:float[6200], Gear3Downshift:float[3000], Gear3Upshift:float[6200], Gear4Downshift:float[3200], Gear4Upshift:float[6200], Gear5Downshift:float[3400], Gear5Upshift:float[6200], Gear6Downshift:float[3400], Gear6Upshift:float[6200], Gear7Downshift:float[3800], Gear7Upshift:float[0]
- `PhysicsEngine/Car/CAR_ROOT/LightSystem`: BrakeLight0:int[1], BrakeLight1:int[1], HighBeam0:int[1], HighBeam1:int[1], LowBeam0:int[1], LowBeam1:int[1], RearLight0:int[1], RearLight1:int[1], ReverseLight0:int[1], ReverseLight1:int[1]
- `PhysicsEngine/Car/CAR_ROOT`: AntiRollBarStiffness0:int[30000], AntiRollBarStiffness1:int[20000], ControlUnit, DangerousAcceleration:float[1e+06], Drive, EffWheelRadius:float[0.315], InverseMass:float[0.000714], LightSystem, MaxFFTorque:float[135], SubTics:int[1], iCollisionMesh:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.00083..0.00169]
- `PhysicsEngine/Ground/GROUND`: Height:int[32768], Left:int[-16384], Top:int[-16384], Width:int[32768]
- `PhysicsEngine/Solid/BANNER_FENCE`: AngularSleepVelocity:float[6], InverseMass:float[0.01], LinearSleepVelocity:float[2], SleepDelay:float[1], Sleepable:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.01]
- `PhysicsEngine/Solid/GENERIC`: iCollisionMesh:int[3]
- `PhysicsEngine/Solid/LOG_POLE`: AngularSleepVelocity:float[6], InverseMass:float[0.001], LinearSleepVelocity:float[2], SleepDelay:float[1], Sleepable:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.01]
- `PhysicsEngine/Solid/LOG`: AngularSleepVelocity:float[6], InverseMass:float[0.05], LinearSleepVelocity:float[2], SleepDelay:float[0.5], Sleepable:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.1]
- `PhysicsEngine/Solid/PIG`: AngularSleepVelocity:float[9], InverseMass:float[0.00025], LinearSleepVelocity:float[2], SleepDelay:float[1], Sleepable:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.001]
- `PhysicsEngine/Solid/STREET_SIGN`: AngularSleepVelocity:float[1], InverseMass:float[0.2], LinearSleepVelocity:float[0.5], SleepDelay:float[0.75], Sleepable:int[1], VerySoft:int[1], vecLocalInverseInertiaDiagonal:floatx3[1]
- `PhysicsEngine/Solid/TRAFFIC_CONE`: AngularSleepVelocity:float[1], InverseMass:float[0.3], LinearSleepVelocity:float[0.5], SleepDelay:float[0.75], Sleepable:int[1], VerySoft:int[1], vecLocalInverseInertiaDiagonal:floatx3[2..3]
- `PhysicsEngine/Solid/WOODEN_FENCE_BAR`: AngularSleepVelocity:float[6], InverseMass:float[0.01], LinearSleepVelocity:float[2], SleepDelay:float[0.5], Sleepable:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.01]
- `PhysicsEngine/Solid/WOODEN_FENCE_POLE`: AngularSleepVelocity:float[3], InverseMass:float[0.001], LinearSleepVelocity:float[2], SleepDelay:float[1], Sleepable:int[1], vecLocalInverseInertiaDiagonal:floatx3[0.01]
- `PhysicsEngine/collision solver`: HyperRestitution:float[0], MinInvEffRedMass:float[1e-05], Restitution:float[0.1], nSteps:int[2]
- `PhysicsEngine/surface types/<material>`: BumpAmplitude:float[0..0.03], BumpFrequency:float[0..1.5], CollMyHigh:float[0.4..5], CollMyLow:float[0.2..5], Flags:flagx{2,3}, RollingResistance:float[0.05..0.5], SinkFactor:float[0..8e-07], SoilDensity:int[0..2000], SoilThickness:float[0..0.04], SolidGroundOffset:float[-0.03..0], Type:int[0..5], WaterFactor:float[0..1], eHigh:float[0.05..15000], eLow:float[0.01..600000], myKinetic:float[0.4..1.5], myKineticSoil:float[0..0.98], myStatic:float[0.65..1.54], myStaticSoil:float[0..1]
- `PhysicsEngine/surface types`: BR2_Gravel_Coarse_Deep_Damp, BR2_Gravel_Coarse_Deep_Dry, BR2_Gravel_Coarse_Deep_Wet, BR2_Gravel_Coarse_Deeper_Damp, BR2_Gravel_Coarse_Deeper_Dry, BR2_Gravel_Coarse_Deeper_Wet, BR2_Gravel_Coarse_Shallow_Damp, BR2_Gravel_Coarse_Shallow_Dry, BR2_Gravel_Coarse_Shallow_Wet, Bendable_Tree, Dense_Bush, Dirt_Hard_Damp, Dirt_Hard_Damp_Clone, Dirt_Hard_Dry, Dirt_Hard_Dry_Clone, Dirt_Hard_Wet, Dirt_Hard_Wet_Clone, Dirt_Medium_Damp, Dirt_Medium_Dry, Dirt_Medium_Wet, Dirt_Soft_Damp, Dirt_Soft_Dry, Dirt_Soft_Wet, Grass_Hard_Damp, Grass_Hard_Dry, Grass_Hard_Wet, Grass_Medium_Damp, Grass_Medium_Dry, Grass_Medium_Wet, Grass_Soft_Damp, Grass_Soft_Dry, Grass_Soft_Wet, Gravel_Coarse_Deep_Damp, Gravel_Coarse_Deep_Dry, Gravel_Coarse_Deep_Wet, Gravel_Coarse_Deeper_Damp, Gravel_Coarse_Deeper_Dry, Gravel_Coarse_Deeper_Wet, Gravel_Coarse_Shallow_Damp, Gravel_Coarse_Shallow_Dry, Gravel_Coarse_Shallow_Wet, Gravel_Fine_Deep_Damp, Gravel_Fine_Deep_Dry, Gravel_Fine_Deep_Wet, Gravel_Fine_Deeper_Damp, Gravel_Fine_Deeper_Dry, Gravel_Fine_Deeper_Wet, Gravel_Fine_Shallow_Damp, Gravel_Fine_Shallow_Damp_Clone, Gravel_Fine_Shallow_Dry, Gravel_Fine_Shallow_Dry_Clone, Gravel_Fine_Shallow_Wet, Gravel_Fine_Shallow_Wet_Clone, Gravel_Medium_Deep_Damp, Gravel_Medium_Deep_Dry, Gravel_Medium_Deep_Wet, Gravel_Medium_Deeper_Damp, Gravel_Medium_Deeper_Dry, Gravel_Medium_Deeper_Wet, Gravel_Medium_Shallow_Damp, Gravel_Medium_Shallow_Dry, Gravel_Medium_Shallow_Wet, Lov_Bush_Large, Lov_Bush_Medium, Lov_Bush_Small, Metal_Pole, Rock_Large, Rock_Medium, Rock_Small, Rough_Rough_Dry, Rough_Rough_Wet, Rough_VeryRough_Dry, Rough_VeryRough_Wet, SnowOnGravel_Deep, SnowOnGravel_Medium, SnowOnGravel_Shallow, SnowOnIce_Deep, SnowOnIce_Medium, SnowOnIce_Shallow, Snowwall, Snowwall_Bottom, Sparse_Bush, Spectator, Spret_Bush_Large, Spret_Bush_Medium, Spret_Bush_Small, Tarmac_Coarse_Clean_Damp, Tarmac_Coarse_Clean_Dry, Tarmac_Coarse_Clean_Wet, Tarmac_Coarse_Covered_Damp, Tarmac_Coarse_Covered_Dry, Tarmac_Coarse_Covered_Wet, Tarmac_Coarse_Sprinkled_Damp, Tarmac_Coarse_Sprinkled_Dry, Tarmac_Coarse_Sprinkled_Wet, Tarmac_Fine_Clean_Damp, Tarmac_Fine_Clean_Dry, Tarmac_Fine_Clean_Wet, Tarmac_Fine_Covered_Damp, Tarmac_Fine_Covered_Dry, Tarmac_Fine_Covered_Wet, Tarmac_Fine_Sprinkled_Damp, Tarmac_Fine_Sprinkled_Dry, Tarmac_Fine_Sprinkled_Wet, Tarmac_Medium_Clean_Damp, Tarmac_Medium_Clean_Dry, Tarmac_Medium_Clean_Wet, Tarmac_Medium_Covered_Damp, Tarmac_Medium_Covered_Dry, Tarmac_Medium_Covered_Wet, Tarmac_Medium_Sprinkled_Damp, Tarmac_Medium_Sprinkled_Dry, Tarmac_Medium_Sprinkled_Wet, Tree_Large, Tree_Medium, Tree_Small, Tree_Stump, Trunk_Large, Trunk_Medium, Trunk_Small, US2_Gravel_Fine_Deep_Damp, US2_Gravel_Fine_Deep_Dry, US2_Gravel_Fine_Deep_Wet, US2_Gravel_Fine_Deeper_Damp, US2_Gravel_Fine_Deeper_Dry, US2_Gravel_Fine_Deeper_Wet, US2_Gravel_Fine_Shallow_Damp, US2_Gravel_Fine_Shallow_Dry, US2_Gravel_Fine_Shallow_Wet, Undefined, Very_Dense_Bush, Very_Sparse_Bush, Water_Surface
- `PhysicsEngine`: BrakeWallBuffer:float[2], BrakeWallDamping:float[2], BrakeWallFactor:float[2], CollisionSolver, DbgFlags:flagx26, DefManager, Flags:flagx22, ForceMaterial:int[9], LogFlags:flagx9, MaterialManager, MaxTimeStep:float[0.007], MinTimeDelta:float[0.0002], vecGravity:int/floatx3[-9.81..0]
### localphysics: 3 keys
- `PhysicsEngine`: DbgFlags:flagx14, Flags:flagx19, LogFlags:flagx9
### misc: 7 keys
- `PhysicsEngine/Car/CAR_ROOT`: NULL_PPTOKEN
- `PhysicsEngine/_`: NULL_PPTOKEN
- `PhysicsEngine`: DefManager
- `PhysicsState/Peugeot 206/Rallybil`: vecParentRotation:floatx4[-0.1219..0.9925], vecRelativeAngularVelocity:floatx3[0], vecRelativeLinearVelocity:floatx3[0], vecWorldPosition:floatx3[-446..178]

Quirks observed in the survey:

- `NULL_PPTOKEN` is a bare atom in `misc/prime.lsp`. It precedes each car-part entry, `("Solid" ("CAR_*"))`
  and `("CAR_*" ("CAR_*"))`, 32 times in all. It looks like an unexpanded preprocessor token.
  Bare atoms without a value are skipped by the apply walk (see the exe parser section above), so
  it is treated as harmless. Unverified.
- `":-D"` is the instance name of every sub-block inside `CarSetup` in `setups/d_*.lsp`
  (`Drive`, `Engine`, `VehicleControlUnit`, `Wheel*`, `SpringDamper*`, `Tyre*`: 15 per file,
  405 times in all). Single-instance sub-blocks resolve by block name, so the instance name
  is never matched; the tools wrote a smiley placeholder. The same 4 bytes are the
  default tag that `E65_InitTag` (`0x582760`, `src/rbr/RBR_E65_MapA.cpp:145`) stores into a
  fresh map entry, confirming `:-D` is the codebase's conventional empty-name placeholder.
- `(null)` is the root/instance name in `r_*.lsp` and `tyres.lsp` (`"(null)"` at `0x739520`).
  It is the CRT `%s`-of-NULL fallback (`notes/formats/cars.md`): the exporter stringified a
  NULL name into the data files. Instance names of single-instance blocks are never
  matched, so it parses harmlessly.
- `collision solver` (with a space) is a quoted name under `PhysicsEngine` in `physics.lsp`;
  no exe string matches it (see the class table).

## Class and block mapping (exe evidence)

Each row ties a block or key name to an NTweak class. "String" is the NUL-terminated name in
the exe's `.rdata`. "Registry" is the 20-byte record `{name, ptr, 0, 0, factory}` in the
`.data` table at `0x7c7fa0`, with `factory` pointing at the function that creates the class. The
decompile addresses come from `out/decomp/RichardBurnsRally_SSE.exe.c`; our reconstruction
paths are under `src/rbr/`. Class names with no evidence are marked "inferred".

| `.lsp` block | Class | Evidence (address, source) |
|---|---|---|
| `PhysicsEngine` (file root) | `CPhysics` (inferred) | CBase `"Physics"`: `src/rbr/RBR_E46_CDef.cpp:20-21`. The string `PhysicsEngine` is in the exe's inline default text at `0x72eedb`, not as a standalone name. |
| `DefManager` | `CDefManager` | `CBaseCtor(self, "DefManager")`: `src/rbr/RBR_E39_MgrA.cpp:32`; string `0x739e90`; RTTI `.?AVCDefManager@NPhysics` at `0x7c945c`. |
| `MaterialManager` (contains `surface types`) | `CMaterialManager` | CBase `"MaterialManager"`: `src/rbr/RBR_E45_Material.cpp:149`; string `0x73d8b8`. The inline default text contains `MaterialManager ("surface types"` at `0x72ef74`. |
| `collision solver` | `CCollisionSolver` (name only) | CBase `"CCollisionSolver"`: `src/rbr/RBR_E38_SolverA.cpp:112`; string `0x739940`. The lsp name `collision solver` has no exe string, so the link is unverified. |
| `Ground/GROUND` | `CGround::CDef` | Registry `"Ground"` (`0x7c7fa0`) → `0x51baa0` = `CGround::CDef::New9`: `src/rbr/RBR_E43_GroundA.cpp:57`. |
| `Solid/<NAME>` | `CSolid` (`E47Solid`) | Registry `"Solid"` (`0x7c7fc8`) → `0x52b8b0` = `E47Solid::NewDef`: `src/rbr/RBR_E47_Def.cpp:86`. |
| `Car/CAR_ROOT` | `CCar::CDef` | Registry `"Car"` (`0x7c7fdc`) → `0x4e6f20` = `CCar::CDef::Create`: `src/rbr/RBR_E34_CarB.cpp:533`; ctor `src/rbr/RBR_E34_CarA.cpp:298`. |
| `CarBody/CAR_BODY` | `CCarBody` | Registry `"CarBody"` (`0x7c7ff0`) → `0x4eba80` (`CCarBody::NewDefD0`); `src/rbr/RBR_E35_BodyA.cpp:37`. |
| `HingedBody/CAR_*` | `CHingedRigidBody` | Registry `"HingedBody"` (`0x7c8004`) → `0x51f880` = `CHingedRigidBody::Vf50New`: `src/rbr/RBR_E44_Signal.cpp:150`. The function is a vf50 entry, not a `New`; the link is by name. |
| `MountedBody/CAR_*` | `CMountedRigidBody` | Registry `"MountedBody"` (`0x7c8018`) → `0x521b50` (`CMountedRigidBody::NewDef`). |
| `Wheel/CAR_WHEEL_*` | `CWheel` (`E51Wheel`) | Registry `"Wheel"` (`0x7c7fb4`) → `0x53b610` = `E51Wheel::M3B610`: `src/rbr/RBR_E51_WheelA.cpp:444`. |
| `…/SpringDamper` (under `Wheel`) | `CSpringDamper` | CBase `"SpringDamper"`: `src/rbr/RBR_E48_SpringDamper.cpp:117`; string `0x740174`. |
| `…/Tyre` (under `Wheel`) | `CTyre` | CBase `"Tyre"`: `src/rbr/RBR_E48_TyreMenu.cpp:182`; string `0x740b60`. |
| `Car/CAR_ROOT/ControlUnit` | `CVehicleControlUnit` | CBase `"ControlUnit"`: `src/rbr/RBR_E50_Vcu.cpp:68`; string `0x7380c8`; `CCar::CDef` holds it at `this+0x250` (`src/rbr/RBR_E34_CarA.cpp:365`). |
| `Car/CAR_ROOT/Drive` | `CDrive` | CBase `"Drive"`: `src/rbr/RBR_E40_Def.cpp:23-24`; string `0x7380c0`. |
| `Car/CAR_ROOT/Engine` | `CEngine` | CBase `"Engine"`: `src/rbr/RBR_E42_Defaults.cpp:14-18`; string `0x734d0c`. |
| `Car/CAR_ROOT/LightSystem` | `CCarLightSystem` | CBase `"LightSystem"`: `src/rbr/RBR_E36_Light.cpp:21-22`; string `0x7380b4`. |
| `r_*` roots: `CarOptions`, `DriveOptions`, `EngineOptions`, `ControlOptions` | `CCar`, `CDrive`, `CEngine`, `CVehicleControlUnit` `::CSetupOptions` | Strings `0x739ab0`…`0x739ae0`; ctors `src/rbr/RBR_E13_Application.cpp:908-968`. Name link by class name (inferred). |
| `r_*` roots: `SpringDamperOptions{LF,RF,LB,RB}`, `TyreOptions*`, `WheelOptions*` | `CSpringDamper`, `CTyre`, `CWheel` `::CSetupOptions` | Strings `0x739b48`…`0x739be0`; ctors `src/rbr/RBR_E13_Application.cpp:988-1028`. Name link inferred. |
| `setup` roots: `Car`, `Drive`, `Engine`, `VehicleControlUnit`, `Wheel*`, `Tyre*`, `SpringDamper*` | `…::CSetup` of the same classes | Ctors `src/rbr/RBR_E13_Application.cpp:768-869, 1130`. String `VehicleControlUnit` at `0x739a04`. Name link inferred. |
| `tyres` `TyreType00`…`06` | `CTyreTypes` (from `notes/exe-plan.md`) | Strings `0x739af4`…`0x739b3c`. |
| `(null)` roots | not a class | `"(null)"` at `0x739520` is the CRT printf fallback, so the name is a placeholder. |

Part instance names used as keys (`CAR_ROOT`, `CAR_BODY`, `CAR_BONNET`, `CAR_DOOR_L/R`,
`CAR_EXHAUSTPIPE`, `CAR_BUMPER_F/B`, `CAR_WING`, `CAR_WHEEL_LF/RF/LB/RB`, and also
`CAR_ANTENNA_F/M/B` and `CAR_INTAKE`, which appear only in `misc/prime.lsp`) form an
enumerated table in the exe. Its strings are at `0x7cba70`–`0x7cbaac` and `0x73f4a8`–`0x73f57c`.

Registry layout (`0x7c7fa0`, stride `0x14`): name pointer, a pointer, `0`, `0`, factory
function. Seven class records and a sentinel follow. A second table at `0x7c6530` (stride
`0x10`: name, pointer, size, `0`) gives `ControlUnit` (`0x48`), `Drive` (`0x64`), `Engine`
(`0x1d4`) and `LightSystem` (`0x274`). The third field is probably the object size; not
verified. A table at `0x7c7860` gives setup names (`TyreRB`, `CarOptions`, `DriveOptions`,
`ControlOptions`, `EngineOptions`, `SpringDamperOptionsLF`) with the same third-field pattern.

Not resolved: the `CarSetup` root name (no standalone string; only `LoadCarSetup`-style
format strings are present), and why `CCarLightSystem::CState::Vf20x` (`0x4f1980`)
returns the shared `g_e41StateTable` (`InletPressure`/`ExhaustPressure`) while its `Unk1C`
side owns the 1-record `Dummy` table at `0x7c6df0` (dict `008ef7c8`, found via the
`CCar::CState` `LightSystem` w1 link). No retail file has light-state keys either way.

## Key tables (resolved)

Every `.lsp` key maps to a class, a member byte offset, and a value reader:
`notes/formats/physics-lsp-fields.md` lists all 50 static key tables (1,094 records)
with table address, runtime dict, init site, parent links, and the C++ member type
where a header/ctor join proves it. The same data is a typed view in
`rust/rbr-formats/src/lsp_keys.rs` (`Class`/`key_offset`), covered by the retail test
`rust/rbr-formats/tests/lsp_keys.rs`, which checks every key of every retail `.lsp`
against the tables. Findings:

- Table shape is `{name, w1, w2, w3}` (16 bytes); `w2 * 4` is the member byte offset
  (verified against ctor placements and header offsets for `CCar::CDef`,
  `CSolid::CDef`, `CGround::CDef`, `CHingedRigidBody::CDef`, `CMaterial`,
  `CCollisionSolver::CDef`, `CPhysics::CDef`/`CState`, `CDrive::CSetup`). `w1` is 0 for
  values and the child-table address for sub-objects (12 w1 links equal the child
  class dict address exactly). Lookup is `CBase::Parse` (`0x52f0c0`) -> `Unk20` dict
  -> `CDictBase::Find` (`0x52ebf0`) -> member `Parse`; unknown keys are skipped
  silently, as are unknown `CBitField` names (`CBitField::Parse`, `0x52f930`).
- Values are stored raw: `atof`/`atoi` with no parse-time conversion (all six member
  readers verified in `src/rbr/RBR_E48_NTweak.cpp`). `Type` is `CMaterial::m_u13C`
  (`+0x13c`, `CUnsigned`; retail `0/1/4/5`) and `CTyre::CTyreTypes+0xc` (retail `0..6` =
  compound index); the consumers are untraced, so no conversion is claimed.
- `Flags`/`DbgFlags`/`LogFlags` are `CBitField`s with 20/27/20 names at
  `0x7cbc10`/`0x7cbcb0`/`0x7cbd90` (`CPhysics::CDef`) and 4 names at `0x7caf18`
  (`VALID`/`SOFT`/`HARD`/`BENDABLE` for `CMaterial`). Four retail bits have no table
  entry and are loader-skipped: `BLOCK_MAP_CULLING`, `BLOCK_MAP_CULLING_FIX`
  (`physics.lsp` `Flags`), `RENDER_TRINODE_COLLISION`, `INVALID_MATERIAL_ALERT`, plus
  `RENDER_MATERIAL_ID` (`localphysics.lsp` `DbgFlags`).
- Loader-skipped retail keys (no table entry anywhere): `Radius` in car-overlay
  `Wheel` blocks (unreferenced string at `0x738098`; radius comes from the `Tyre`
  child). The t-coroll-only `Pressure` in `d_*` `Tyre` blocks is not skipped: `TyreLF`'s
  `w1` links the dict at `0x165f200`, `CTyre::CSetup`'s (`Pressure` at `+0x1c`), and
  `TyreType00`'s the 104-key dict at `0x165f1ec`, `CTyreTypes`' (`src/` names the two
  dicts the other way round; see `physics-lsp-fields.md`).
- `Misc/state.lsp` is not loader input: its `PhysicsState` root and
  `vecWorldPosition` strings do not exist in the executable.

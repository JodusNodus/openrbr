# Physics coverage gaps: what the reference drive never reaches

Scope: the 929 non-exact physics functions (`RBR_E33`-`RBR_E51` minus the
profiler, `scripts/progress.py` physics set) absent from
`notes/progress/called.csv`. The reference drive (`tanner-corolla`,
`54 6 0 4`: Tanner/Japan gravel, Corolla 4WD, good weather, gravel
intermediate; the auto file has no damage number, and the plugin logs the game setting at stage start as damage 1, safe) reaches 792 of 2,651 physics functions.
Per-function assignments are in `notes/progress/unreached.csv`
(`addr,condition,evidence`).

## Ranked table

| Condition | Fns | Example functions | How to reach |
|---|---|---|---|
| stage load or teardown | 259 | E46Conn Init/Release, E39Conn CtorLoad/DtorUnload, CDefManager/EntityFactory, CDriveLine dtor, E13DictHelpers, Wipe7F60 | load-time count run, not a new drive |
| unknown (needs triage) | 246 | E38/E37 solver internals, E43 matrix walks, CDrive M-workers, CEngine Sims, wheel Unk* | see triage below; ~18 are fuzz candidates |
| unreachable in 1.02 | 168 | E33Vec2/E38Vec tiny math, E39 Nop/Small empties, E45 math self-tests, dead CDriveLine samplers, SampleBig labels | nothing (match for completeness only) |
| damage or a crash | 152 | NRepair Vf00/Vf04 (~40), CCarDamageHandler Dmg* (~25), Write/ReadBinary family, retire/respawn/assist, wall penetration, sub-body detach modes | new drive: damage-on + crashy |
| tweak/menu parsing | 62 | NTweak GetValueText/Parse/LoadBinary, TyreMenu UI classes, menu-TU tyre helpers, E36InitThunk | review only; never runs on a drive |
| car-specific (VCU/launch, drivetrain) | 13 | VCU CScratchSim/* (11), CDrive::M6840, E39_Step3870 | VCU: snapshot fuzz now; slots: 2WD drive |
| replay or ghost cars | 11 | AnimatedReplay Emits, Snap94D0/Sampler/Split, CDriveLine::Sample, DetachFromParent | replay/ghost/rescue session |
| surface (tarmac/snow/materials) | 10 | CGround kind virtuals (9), E44Entry8::Pick (verified, count-gap) | new drive: snow + tarmac stages |
| multiple cars or passengers | 6 | CPassenger P-smalls (Step itself runs) | trigger unclear; see below |
| weather | 0 | (none positively identified) | bad-weather drive for wet-variant data |

Two corrections to the "920 never run" premise, both documented per
function in the CSV:

- `called.csv` is a union of counted transplant runs, not a trace of
  everything that executes. 11 unreached functions provably run on the
  reference drive (evidence prefix `COUNT-GAP`): `verified`/`bad`
  harness status (Collide6000, Vf128x, Vf98x, Xform80B0, Snap9360,
  UpdateAndReset, E44Entry8::Pick, CBodyLink::DeleteThis, 3x
  E13DictHelper), or unconditional callees of reached functions
  (E39_Nop3860 from CCar::Vf24x, E46_RegWall from water Vf15CW,
  E39_Entf0C twice from E43 MF8B0). They need a count run, not a new
  drive. All 44 targets with reached callers were audited against the
  caller decompile; the rest are conditional (branch conditions quoted
  in each row's evidence).
- 168 functions have no callers anywhere in the binary (checked against
  the full Ghidra export: `// calls` lines plus `FUN_*` refs). Most are
  trivial helpers whose code runs inlined, empty no-ops, or dead
  duplicates (e.g. the unused CDriveLine samplers next to the used
  BlendB, the menu-TU tyre-curve duplicates of the reached TyreClamp).
  No drive or fuzzing will ever call the standalone copies.

Cheapest first: a load-time/full-transplant count run on
`tanner-corolla` reclaims the 259 teardown plus the 11 count-gap
functions with zero driving (section 5).

## How this was classified

Inputs: `gameplay-functions.csv` (status), `called.csv`, `progress.py`
physics set, reccmp names/scores (`build/rbr/reccmp.json`), src/rbr
annotations (file per function), the Ghidra decompile
(`out/decomp/RichardBurnsRally_SSE.exe.c`) parsed into a caller graph
(27,935 functions), per-function string literals, tweak-key tables,
`notes/physics/*.md`, and game-data checks on the vanilla install
(Cars.ini car order, Maps texture paths for the rally mapping).

Rules, first match wins (script: `/tmp/cov/classify.py`, kept out of
git; rerun inputs listed there): audited manual cases (the 44 reached
callers, serialization chains, VCU files) > harness status
(verified/bad = ran) > tweak-menu (NTweak/TyreMenu files and menu
accessors) > damage-crash (Damage/Repair files and names,
retire/respawn/assist/brake-wall, Write/ReadBinary callers, detach
modes) > replay-ghost (AnimatedReplay/ghost/rescue/detach names,
Snap/Sampler/Split, CDriveLine::Sample whose only callers are
replay/rescue/menu) > passenger (CPassenger; lifecycle splits to
teardown because Step runs) > car-specific (E49_VCU/E50_Vcu/E50_Sim
files, VCU-slip branches, per-slot drivetrain branch) > surface
(ground-kind virtuals, material-table selection) > lifecycle names
(ctor/dtor/DeleteThis/Create/Init/Demote/Unlink/...) >
unreachable-debug (math self-tests with "Checking..." strings, dumps)
> unreachable-dead (no callers at all + tiny/empty/or helper-shaped) >
unknown (with the caller evidence quoted).

Where evidence was weak the function is `unknown`, not guessed. The
`unknown` bucket splits as: 57 hang only off unreached callers (dead
subtrees or config chains), 33 are data/state-gated callees of reached
functions (count-run then fuzz candidates), 145 have no callers and are
non-trivial (dead code or an unparsed path — triage each before
matching effort). Status cross-check: `reviewed` means the lead
confirmed unreached-by-drive, consistent with dead/inlined; `verified`
or `bad` means it ran, so those rows are COUNT-GAP by construction.

## Condition notes

- damage-crash (152): two halves. (a) The damage system proper:
  ~40 NRepair effect appliers, ~25 CCarDamageHandler zone appliers,
  zone/stream helpers, all unreached because the reference auto file
  sets no damage number, so the game setting (1, safe) applies. (b) Crash-gated step branches
  audited in context: retire/respawn/assist Emits and Assist8DF0 fire
  from Update91F0/Update9AF0 only via Probe7730 stranded/terminal
  paths; brake-wall Emit/Impulse640 only on wall penetration
  (RaycastEntry itself runs); mounted/hinged detach modes (Vf6c,
  StepBig, SimFD00, ExclA060) only when sub-body mode words read 1/2.
  Damage serialization (Write/ReadBinary + 8 helpers) is called from
  gameflow 0x567930/0x567b90 at stage boundaries, so it needs a
  damage-on session, not mid-drive chaos.
- surface (10): thin because surface handling is mostly data through
  reached code (tyre step, material reads). The 9 CGround kind virtuals
  never dispatch on Tanner; E44Entry8::Pick (material-table index,
  verified) proves table-selection paths exist. Snow/tarmac drives
  cover the data variants; expect few newly-called functions.
- weather (0): no physics function could be tied to weather from
  strings, names, or callers. Wetness arrives as data (`*_Wet` tyre
  keys, `WetSlope_T*`, `@RainAmount`/`Damp` globals) through the shared
  tyre/ground code. A bad-weather drive is still recommended to cover
  the wet-variant selection paths, whatever they are.
- car-specific (13): 11 VCU/launch functions, off on the reference
  drive (`CScratchSim+0x40/+0x50`), reachable today via
  `harness/vcu_vectors.py --vcu-on/--fuzz`. Plus M6840 (VCU-slip-gated
  in reached UpdateD) and Step3870 (per-drivetrain-slot branch in
  reached M9BC0; other cars populate slots differently). No
  statically-identified 2WD-only function exists (driveline is
  data-driven loops), so the ZR drive is for data-driven branches.
- replay-ghost (11): needs a replay watch, ghost, or rescue-chopper
  session, not a drive. Snap9360 is verified (ran) but flag-gated in
  Hook (+0x540, emits the animated-replay signal).
- multi-car-passenger (6): CPassenger::Step runs every step; only six
  small helpers never run and their trigger is unknown (possibly dead).
  Low priority; confirm with the load-time count run.
- tweak-menu (62): NTweak text/parse/binary-load, TyreMenu UI, E13
  leftovers, menu-TU tyre-curve duplicates. Review-only.
- unreachable (168): nothing to reach. Includes the two SampleBig
  interior labels the gate already excludes, E44_BlockDtor (EH-unwind
  only), and ~140 dead/tiny standalones.

## Recommended next drives (1-3)

IDs: map = track number (rally groups from Maps textures: 10-15 Arctic
snow, 20-25 British, 31-36 Australia, 41-46 Mont Blanc tarmac, 51-56
Hokkaido/Japan incl. Tanner=54, 61-66 USA), car 0-7 from Cars.ini
(Xsara 0, Accent 1, ZR 2 = the FWD car, Lancer 3, 206 4, Impreza03 5,
Corolla 6, Impreza00 7), weather 0 = good. TyreType00-06 mapping and
the bad-weather id could not be pinned from the exe; confirm both from
the in-game menus before recording (reference: 4 = gravel
intermediate, the menu default).

1. `harness/record.sh tanner-damage "54 6 0 4 3"` then
   `drive-setup.sh`: same stage/car/tyre, realistic damage, driven
   dirty on purpose (wall hits, offs, a roll, use rescue, retire the
   car). Unlocks the 152 damage-crash functions with full attribution
   on a known-good stage.
2. `harness/record.sh arctic-snow "12 6 1 <snow-tyre> 3"`: Arctic snow
   stage, bad weather, snow tyre, realistic damage. Covers
   surface/material/snow data paths, wet variants, and damage in one
   run. If the snow-tyre id is elusive, run it on tyre 4 anyway: the
   surface code still runs on snow data.
3. `harness/record.sh tarmac-zr "43 2 0 <tarmac-tyre>"`: Mont Blanc
   tarmac with the FWD ZR on tarmac tyres (no damage flag, to isolate
   car + surface). Covers drivetrain-slot population and tarmac data.

Do these before any fuzzing campaign: each new drive also re-verifies
the already-verified functions under new data.

## Fuzz targets for a worker (no new drive)

Ready now: the 11 VCU functions + M6840/CheckSlip through the existing
`vcu_vectors.py --vcu-on/--fuzz` pattern (mutate flags, pedals, gear,
slip, thresholds at the call boundary, restore after).

Prototype next (same pattern, new mutation points at a Tanner
snapsh
...[truncated 2085 chars]
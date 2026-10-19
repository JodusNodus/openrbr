# Race options: damage level, driver aids, weather draw

How the quick race's menu options reach the physics and the game state (RaceOpts-1, 2026-10-17). Evidence is the
1.02 exe's asm and decompile (**loader**) unless noted. Rust: `rbr_gameplay::race_options`, `weather::random_draws`,
the boot (`rbr-standalone/src/boot/`).

## Where the options live

`E66MenuSettings` (0x16607f8, `src/rbr/RBR_E66_Settings.h`). Its quick-race block (`SQuickRaceSettings`, +0x0) and
gears block (`SGearSettings`, +0x698):

| Global | Block word | Option | Menu item | Default (`InitQR` 0x586f30, `InitGEAR` 0x588f00) |
| --- | --- | --- | --- | --- |
| 0x1660814 | QR +0x1c (per player) | automatic gearbox | 2-entry item (`Vf5C(8, 1, 3, &0x1660814)`) | 0 |
| 0x1660848 | QR +0x50 | weather 0 good, 1 random, 2 bad | 3-entry item | 0 |
| 0x166084c | QR +0x54 | damage selection | none | 1 |
| 0x1660850 | QR +0x58 | damage level 0-3 | `CARDAM` slider: `OFF`, `DIFF_SAFE`, `DIFF_REDUCE`, `DIFF_REAL` (list 0x165fa5c) | 1 |
| 0x1660ea0 | GEAR +0x10 | neutral lock | `CS_NEUTRALLOCK` off/on | 0 |
| 0x1660ea4 | GEAR +0x14 | clutch help | `CLUTCH_HLP` off/on | 1 |
| 0x1660ea8 | GEAR +0x18 | gear guard | `GEAR_PROT` off/on | 1 |

The English labels (`strings.rbz/bstrings.eng`): Car Damage (Off, Safe, Reduced, Realistic), Neutral Lock, Clutch
Help, Gear Protection. The profile keeps the damage level (`SQuickRaceSettings::Write`/`Read`, clamped to 3) and the
gears block (`GEAR` tag). The defaults are the reference drives' values.

**0x166084c is constant.** Only `InitQR`, the reset (`Vf04` 0x586770) and the snapshot restore write it. No
instruction stores to 0x166084c (immediate scan of `.text`); its two readers are `E60Flow::M5666b0` (0x5666b0) and
`InitQuickRaceDamageHandler`, which copies it to the host's +0x17854. So it is 1 in every game, not a menu option.

**There is no separate assistance-penalty option outside a rally.** `M576710` (0x576710) switches on 0x1660850, the
damage level: 0 and 1 give 10 s, 2 gives 20 s, 3 gives 35 s. In a rally (flow modes 3 and 5) the penalty comes from
the season's `*0x7d6128` +0x8e84 (0, 1, 2 → 10, 20, 35 s). That is the championship's difficulty, so it belongs to
the championship flow.

## How they reach the race

- **Damage level.** `InitQuickRaceDamageHandler` (0x567640) runs in the race load. In flow modes 1, 2, 4 and 5 it
  takes `v = 0x1660850`; mode 3 takes `0x1660c70`; mode 7 (school) takes 2. It stores the mode, `0x166084c` and the
  rally word at host +0x17850/+0x17854/+0x17858, then calls `CCarDamageHandler::Set3FB0(v)` (0x4ebfb0) on the
  player's handler. That writes the level to +0x3df0, the threshold to +0x3dec (0, 0.2, 0.7, 1.0) and counts the call
  at +0x3a14. It then copies the handler into the host's two other handlers (+0xfad0, +0x13980) and finally runs
  `srand(time(NULL))` and `E01SetSeed` with the same time. `UpdateA720`'s `Slot4` (0x4f1000) then derives the
  level's fractions. The Rust boot calls `damage::set_level(slot 0, level)` at the same point as before, with the
  option's level instead of the fixed 1. It does not do the two `CopyFrom`s; it never did at level 1 either, and
  the step does not read those copies on the checked drives.
- **Driver aids.** `CCarSettingsManager::LoadGlobal` (0x547080) stores the player's `0x1660814[player]`,
  `0x1660ea8` and `0x1660ea0` into the control unit's setup (`AutoGears`, `GearGuard`, `NeutralLock`; each
  `SetDirty` first). `InitializeCarSetting` (0x547530; quick race: case 1) loads the car setup, then sets
  `ClutchHelp` from `0x1660ea4` (`E36DefOp::Set0c`). The rally school (case 7) sets all four. The Rust boot
  already stored them at this point (`load::set_driver_aids`); now they come from the options.
- **Weather.** `IRBRGame::StartGame` (0x626a70) stores `eWeather` in 0x1660848 and runs the daemon's `Update`
  (0x4aab40). See `weather.md`. For random, `E74_FindBest` (0x5b53b0) counts the map's rows in the season table
  (0x13f rows), draws `rand()` (unsigned `% n`) and takes that row. After the daemon's setters, `Update` draws the
  wear as `(rand() % 10 > 6) + 1`. No call between the two draws uses `rand()` (`Refresh`, 0x5b5560 and 0x5b5590
  were checked). A map with no rows makes `FindBest` return 0, and `Update` reaches `int3`.
- **The `rand()` stream.** msvcr71 `rand()` (`holdrand * 214013 + 2531011`, bits 16-30), one per thread. The main
  thread seeds it with `srand(time(NULL))` in `CPC_Application::vf04` (0x44c890, after `startup.lua`), and again in
  each race's `InitQuickRaceDamageHandler`, after the weather draw. Between a seed and the next race's draw, the
  front end's sound and menus call `rand()` an unknown number of times (`sound.md`). So the game's draw cannot be
  reproduced, only its formula. Rust: `crt::CrtRand` and `weather::random_draws` give the formula exactly.
  `rbr-standalone --rand-seed N` draws from `srand(N)`. The launcher seeds once with the time at its start and
  takes one draw per Start (ours).

## What a non-default value changes in the boot image

Dumped with `--dump-image` on tanner-corolla's boot, then diffed against the default:

- **damage** (each level): `0x1660850`, and in the damage host's slot 0 (the car's handler): +0x3df0 (level),
  +0x3dec (threshold), and `Slot4`'s fractions at +0x3a8c, +0x3c38, +0x3c70, +0x3ca8, +0x3ce0, +0x3ce4, +0x3cec,
  +0x3cf4 and +0x3cfc. Realistic also sets +0x3ab0 (the level-3 flag) to 1. Off and reduced still step equal to
  tanner-corolla for the first step. Realistic diverges at step 0, as expected for a level-1 recording.
- **auto gears on**: `0x1660814`, the complete setup's `AutoGears` value (+0x6b8 of the 0x1700 block) and the car's
  control unit word car+0x890. The pure step then runs the automatic gear logic (`0x537f50`, called from `0x4e9930`;
  `rbr_physics::vcu::gear`, bound since AutoGears-1). No auto-gears recording exists to compare against.
- **gear guard off**: `0x1660ea8`, setup +0x6c8, car+0x894.
- **neutral lock on**: `0x1660ea0`, setup +0x6d8, car+0x898.
- **clutch help off**: `0x1660ea4`, setup +0x6e8, car+0x89c.
- **random weather**: the season row (stage variant, wetness → `.mat` table and wet meshes) and the wear, as for
  `--season-row`/`--wear` before (`weather.md`).

## Verified

- Level 3: the `.auto` files' fifth word is the damage level the harness writes to 0x1660850 (`RBRRecompHarness.cpp`
  `kDamage`), and the sixth is the transmission (0x1660814). The boot now takes both from the `.auto` when there is
  no flag. tanner-damage (`54 6 0 4 3 0`) boots and steps 12120 of 12120 steps equal to the end of the recording.
- Level 1 and the default aids: tanner-corolla 13000/13000.
- Not verified: levels 0 and 2, each aid's non-default value, and auto gears on (runs, no recording). See the drive list in
  `lead/reports/raceopts-1.md`.

## Championship options (ChampOpts-1, 2026-10-17)

The championship asks two options of its own before a season, in the menu settings' season block
(`SSeasonSettings`, `E66MenuSettings` +0x448 = 0x1660c40):

| Global | Block word | Option | Page | Default (`InitSEA` 0x588990) |
| --- | --- | --- | --- | --- |
| 0x1660c5c | SEA +0x1c | skill level: 0 `SROOKIE` Rookie, 1 `SPROFFS` Professional, 2 `SCHAMPION` Champion (label `SSKILL`, "Skill Level") | 0x24 (builder 0x5ab840) | 0 |
| 0x1660c70 | SEA +0x30 | damage level, the `CARDAM` slider's four values as the quick race's | 0x25 (builder 0x5ab5f0) | 3 (Realistic) |

- **Menus.** New season opens page 0x24; each skill item writes 0x1660c5c and goes to page 0x25. Each damage item
  writes 0x1660c70 and goes to page 0x18 (`TEAM_OFFERS`, 0x5ab0f0). The season block is not in the profile's menu
  settings (`E66MenuSettings::Write` 0x5862f0 does not call `SSeasonSettings::Write`), so every session starts on
  `InitSEA`'s values.
- **Start.** `E13MenuInitA` (0x47c7f0) calls `E60Flow::M566990(1, [0x1660c5c])`, which sets the flow to mode 3 and
  calls `E80Daemon::DaemonInit(1, skill, ...)` (0x5d34b0): `+0x8e84 = skill`. The hot-seat start `E13MenuInitC`
  (0x47c850) passes `[0x16616c0]` instead (multiplayer, out of scope).
- **The season save** (`CAISeason` write 0x5d3e40, read 0x5d5330; `rbr_formats::saves::season`): +0x8e84 is the
  first state word; 0x1660c5c is word 7 of the 13 menu globals (0x1660804, 0x1660808, 0x1660824, 0x1660c4c..0x1660c6c,
  0x1660814; 0x1660c70 is not among them); `Ser14b` (0x5d2e70) then writes the 14-word snapshot, refreshed from the
  globals first in mode 3 (`E80Snap14` 0x5d0c20), where 0x1660c5c is word 7 and 0x1660c70 word 12. The snapshot is
  restored into the globals before each stage (0x5d0cc0, from `E60Flow::M569fc0`) and taken when the season is left
  (`M566b40`). Rust: `Season::rally_level`, `damage_level`, `set_options`; `season::create` and `season::options`.
- **What reads them during a stage.**
  - `InitQuickRaceDamageHandler` (0x567640): in mode 3 the car's level is `[0x1660c70]` (modes 1, 2, 4, 5:
    0x1660850; 7: 2; others 1), and modes 3 and 5 copy +0x8e84 into the damage host's +0x17858.
    `race_options::damage_level`.
  - `M576710` (0x576710): in modes 3 and 5 the outside-assistance penalty is +0x8e84's 0, 1, 2 → 10, 20, 35 s (else
    0). `ChampionshipOptions::assist_settings`.
  - `E60Flow::M566640` / `M5665f0`: in mode 3 the assistance gate is shut when +0x8e84 is 2 or more (Champion). Then
    `CCar::Update9AF0` retires a stuck car (code 4) instead of helping it, and a script character's box gives
    `Assist8DF0(0, 30.0, 4.0)` and the rally incident. The physics reads the word itself (`flow_holds`).
  - `0x5d7480`: the AI's par per skill (`ai_season`, already ported).
- **Rust boot.** `boot::Race::in_championship`: flow word 3, the two menu globals, the car's level from
  `race_options::damage_level`, and a stand-in for the season daemon (ours): `[0x7d6128]` points to a zeroed block
  whose +0x8e84 is the skill; nothing else of `E80Daemon` is built. The quick race's 0x1660850 keeps its menu value,
  as in the game.

Checked: tanner-corolla in mode 3 at Rookie and Champion with damage Safe steps 13000/13000 equal (the gate is never
asked there). rcv-retire-impreza00-45 at Champion leaves the recording right after the box hit at step 2143
(car+0x15e4, the assist's other arguments), as the shut gate should; the quick race keeps it equal. The penalty per
skill is unit-tested. Not checked against the original: a championship drive at each skill (cachy).


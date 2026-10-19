# RBRControls HShifter (advanced H-pattern mode)

Behaviour reference for an H-pattern shifter with clutch, rev-matching and
auto-neutral. Source is not public; this comes from decompiling
`Plugins/RBRControls.dll` 5.3.7.0 (sha256 `5301866479458614b1442d25834c044243a8dc89f8f76c22040451a7d1974e47`)
from the RSF install. The DLL states MIT, by Steven Lee, mika-n, Towerbrah,
Detegr and RSWats; the advanced engagement logic is RSWats' 2025-2026 work.
The older RBRHshift 3.5 (`README.md` in this directory) is the simple version:
write the gear when clutch > bite point, else play a grind.

## Game state it uses

| Field | Meaning |
| --- | --- |
| car `[0x8EF660]` `+0x1100` | gear, read and written: 0 R, 1 N, 2-7 gears 1-6 |
| car `+0x86C` | clutch 0..1 |
| car `+0x85C` | throttle 0..1 |
| car info `+0x0C` / `+0x10` | speed (km/h) / engine rpm |

Gear ratios do not come from memory. It parses the NGP files at load:
`Gear`/`Pinion` per gear from `Physics/<car>/common.lsp` (fallback
`r_common.lsp`), rear final drive from the setup's `FinalDriveId`/`DropGearId`,
and wheel circumference from `CarcassRadius_NGP + TreadDepth_NGP`.

## Shift logic (`FUN_1002f200`)

Inputs: seven H-pattern buttons (1-6, R), clutch threshold, mode
(0 NGP, 1 advanced), engagement (synchro or dog, per car from `CarInfo.ini`),
dogleg, auto-neutral.

1. `clutchIn` = mode advanced ? `clutch > threshold` : always true.
2. Selected gear comes from the first button held, in the order gear 1 to 6,
   then R. Dogleg ignores the gear 1 button and maps the gear 2 to 6 buttons to
   1st to 5th (R stays). With no button held and
   auto-neutral on, gear := N immediately (no checks) and any grind stops.
3. Ignore if selected == last engaged and car is not in N, or selected is above
   the car's gear count.
4. `targetRpm = speed * 1000/60 / circumference * gearRatio * finalDrive / speedFactor`.
5. If clutch is in, engage at once. Otherwise compare actual rpm with
   `targetRpm`. The shift fails if rpm is outside the window, or (upshift only)
   throttle > 0.5:

   | | upshift | downshift |
   | --- | --- | --- |
   | synchro | 0.90 .. 1.05 x target | 0.98 .. 1.25 x target |
   | dog | 0.60 .. 1.60 x target | 0.80 .. 1.80 x target |

6. Timing (units are ticks of the plugin's frame counter, not yet confirmed as
   frames or ms):
   - synchro, matched: must stay matched for 5 ticks (synchronising, flagged as
     pending) before the gear is written. Mismatch: reset timer, grind.
   - dog, mismatched: a 10-tick grace window where the shift is pending and
     engages as soon as revs match; after that, grind.
7. Grind: start the looping, pitch-modulated `Shifter_Error.wav` and leave the
   gear unchanged. While a shift is pending, `FUN_1002fdf0` retries step 5
   every tick, so holding the lever and matching revs or pressing the clutch
   completes it. The grind's pitch is `0.65 + 1.2 * tanh((|rpm error| - 200) * 0.000667)`
   when the error is at least 200 rpm (`FUN_1013be70` is the CRT `tanh`); below that it passes 0.

## Speed factor calibration (`FUN_1002fdf0`)

Every tick, while in gear with the clutch fully released (`clutch == 0`):
`speedFactor = (rpm/60 * circumference / (gearRatio * finalDrive)) * 3.6 / speed`
(1.0 when stationary and rpm 0, 9999 when stationary with rpm). This absorbs
wheel slip and tyre radius error so step 4 matches the real rpm.

## Sequential mode (`FUN_1002fee0`)

Up: gear + 1 if below max. Down: gear - 1, except 1st -> N only when stopped
or a game flag allows it; there is an optional repeat-shift deadzone
(`DeadzoneByTimeMSecGearShift` from NGPCarMenu.ini) and an rpm guard that
blocks a downshift whose target gear would over-rev. Neutral button: N.

## Open questions

- Tick unit for the 5 / 10 / 11 counts.
- The `DAT_1019a4a8 + 0x1260ea0/ea8` flags in the sequential path.

Our port of the advanced mode is `rust/rbr-render/src/shifter.rs` (`notes/ngp.md`); it takes the target engine
speed from the physics' gear ratios instead of step 4, and assumes 60 ticks a second.

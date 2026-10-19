# Progress tracking

`python3 -I scripts/progress.py report [reccmp.txt]` prints the scorecard for the gameplay
reimplementation (`notes/rust-plan.md`). The lead updates these files from worker reports
and harness runs:

- `gameplay-functions.csv`: status per gameplay function (`progress.py mark STATUS EVIDENCE ADDR...`).
  Functions not listed are `untouched`; 100% reccmp matches count as `exact` automatically.
  - `verified`: ran on a reference drive and behaved like the original (transplant identical, or shadow with 0 mismatches)
  - `reviewed`: compared with the original, no behaviour difference found, not yet reached by a drive
  - `fixed`: behaviour fix not yet confirmed on the harness
  - `bad`: crashes or diverges on the harness
- `called.csv`: which functions each drive reaches (`progress.py called DRIVE CALLS.LOG`, from a run's `.calls.log`).
- `step-reached.csv`: which functions each drive's physics step reaches (`progress.py step DRIVE REACHED.txt`, from
  `harness/step_reached.py`); `progress.py report` counts their port status (pure step coverage), `progress.py
  steplist DRIVE` lists them.
- `drives.csv`: end-to-end runs with every physics function transplanted: identical steps per drive.
- `formats.csv`: data formats, parse status and retail files passing.

Gameplay functions are the exe physics files (`RBR_E33`-`RBR_E51`, minus the profiler) and the
loop, input, track loading, game flow and replay address ranges in `scripts/progress.py`.
- `systems.md`: every 1.02 gameplay system and flow (driving, stage flow, modes, setup, co-driver, sound, presentation,
  cameras, replays, input, options), its Rust status and the proposed order of the open work.

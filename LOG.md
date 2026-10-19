# Work log

Current state, open items and decisions in force, one line each. Keep it short; history is in git, and the log up to
the split is in rbr-recomp's `LOG.md` and `notes/archive/`. Per-system state: [notes/status.md](notes/status.md).

## Current state (2026-10-19)

- **Repository:** ReRBR, the Rust reimplementation only. The C++ reconstruction and its notes are in rbr-recomp
  (frozen).
- **Physics:** about 930 ported functions (`rust/rbr-physics/ports.csv`), about 660 bindings
  (`rust/rbr-hybrid/bound.csv`). The pure step runs the whole tanner-corolla stage equal to the recording.
- **Load path:** `rbr-standalone --boot` builds the physics state from the retail files; tanner-corolla boots
  13000/13000 equal.
- **Gameplay loop:** done on the recorded drives (control payload, stage states, timing, time card), plus the
  championship, service park and saves.
- **Gate matrix:** 4 hand-recorded drives and 35 playback-made drives cover every car at least twice, all 36
  main-rally map IDs, damage levels 1 and 3, weather 0-2; most are 3,000-step stretches, not whole stages.
- **NGP 7.4.778:** physics, load path, force feedback and the H-shifter in Rust ([notes/ngp.md](notes/ngp.md)).
- **Presentation:** renderer, sound, HUD, cameras and launcher in live play; leftovers in
  [notes/status.md](notes/status.md).

## Open items

- Extend the gate matrix to whole stages for every car and surface class, with damage and collisions.
- Runs of the original on cachy to compare: damage levels 0 and 2, each driver aid, a changed setup, the go's
  timing, a season golden, a retire, a record-beating drive.
- Community stages and cars: the full RSF sweep, FMOD leftovers, encrypted stages.
- Presentation leftovers: particle edges, door detach, windscreen cracks, captures of the original to compare.

## Decisions

- 2026-10-19 (user): the repository becomes ReRBR, Rust only; the C++ reconstruction moves to rbr-recomp.
  `data/rbr.recomp.csv` keeps its last source files and reccmp scores for the tools here.
- 2026-10-19: the documentation is cut to one note per area; detail as of the split stays in rbr-recomp under the
  same paths.
- 2026-10-17 (user): the goal is RBR with NGP 7 physics and its NGP cars, on the original and the modded stages, all
  in live play ([notes/rust-plan.md](notes/rust-plan.md)). NGP and force feedback are the user's code.
- 2026-10-17 (user): HUD modern and simple with every feature of the original; sound and visuals may be better than
  the original; macOS and Linux first, Windows later; English only.
- 2026-10-16 (user): particles aim to look good, not to match 1.02; the windscreen gets a modern water-on-glass
  system ([notes/renderer.md](notes/renderer.md)).
- 2026-10-16 (user): a new basic launcher starts the Rust game; it is not the 1.02 frontend.
- 2026-10-16: the headline metric is how far `rbr-standalone` runs pure and equal on every drive, and its stop list.
- 2026-10-15: the C++ physics is frozen; Rust is translated a group at a time and checked against the original
  directly. A group is bound whole and checked by one run; `--only` and bisect only on a divergence; fuzzed records
  only for unreached branches. Rust matches every word the gate compares.
- 2026-10-15: the gameplay loop is written directly in Rust and checked against recordings; 1.02 replays deferred.
- 2026-10-13: no matching or coverage work for UI, menus, rendering code, HedgeHog3D or LocoMotion.
- Decompile the official 1.02 binaries, not the RSF build. Never touch the user's RSF install on cachy
  (`~/Games/Richard Burns Rally`, `~/Games/prefixes/rbr`); runtime tests use `~/Games/rbr-vanilla-1.02`.
- Existing crates over hand-written parsers.
- Workers: one worktree each; reports go to `lead/reports/<tag>.md`.

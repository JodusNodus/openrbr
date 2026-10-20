# Agent guide

ReRBR is a Rust reimplementation of Richard Burns Rally 1.02 with NGP 7 physics: a standalone program, no original
code running, that plays the 1.02 stages, the NGP cars and the modded stages with physics and gameplay bit-exact
against recordings of the original. The original game is the oracle. The C++98 reconstruction, the behaviour
reference, lives in the separate, frozen rbr-recomp repository, which also keeps every note as of the split under the
same paths. UI, menus and the 1.02 renderer are not carried over.

## Start here

1. Run `git status --short` and preserve existing changes.
2. Read `notes/status.md` (current state, open items, decisions) and `docs/development.md` (how it fits together,
   crates, rules, checks).
3. Open the note for your area.

`README.md` is the user-facing overview. Keep this file an index. Put findings in the area's note, briefly: current
facts and rules with their evidence in one line, no run logs or narratives. Paths under `src/` in comments refer to
rbr-recomp.

## Notes

| Area | Note |
| --- | --- |
| Scope, "done means", milestones | `notes/rust-plan.md` |
| Current state, each system's state, open items, decisions | `notes/status.md` |
| Physics: step, x87 and float rules, store seam, subsystems, exe map | `notes/physics.md` |
| NGP 7.4.778, force feedback, H-shifter | `notes/ngp.md` |
| File formats | `notes/formats.md`; key tables `data/physics-lsp-fields.md`, `data/ngp-lsp-keys.md` |
| Renderer and visual systems | `notes/renderer.md` |
| Sound | `notes/sound.md` |
| Gameplay: control payload, race flow, timing, options, weather, season, saves | `notes/gameplay.md` |
| Cameras, HUD, launcher | `notes/cameras-hud-launcher.md` |
| Harness, recordings, hybrid check, vectors, masks | `notes/verification.md`, `harness/README.md` |
| Third-party references and licences | `notes/references/README.md` |

## Important paths

| Path | Purpose |
| --- | --- |
| `crates/rbr-formats` | Game file parsers, one module per format |
| `crates/rbr-physics` | Physics ports (1.02 and NGP); `ports.csv` lists them |
| `crates/rbr-hybrid` | Ports bound into the original's step in the emulator (`bound.csv`), and the pure step |
| `crates/rbr-gameplay` | Gameplay loop: input to car control, tick, stage events, timing, championship |
| `crates/rbr-standalone` | The physics program (`--boot`, `--live`) |
| `crates/rbr-render` | wgpu renderer and the driving window (`viewer`) |
| `crates/rbr-sound` | Gameplay sound from the SND1 record |
| `crates/rbr-launcher` | The launcher (binary `rerbr`) |
| `crates/rbr-recording` | Harness recording and live record readers |
| `harness/` | Plugin, recorder, playback runs, step replay, hybrid check, vector capture |
| `RBR_RECORDINGS`, `RBR_VECTOR_STORE` | Recorded drives (`drive-*.bin`) and vector records; not in git, published set via `scripts/fetch-test-data.sh` |
| `original/` | Original binaries and game data (gitignored, `SHA256SUMS` only) |
| `data/` | The exe's function bounds and library functions, the C++ reconstruction's last state, generator inputs |
| `scripts/` | `rust-check.sh`, `rust-vectors.sh`, `remote-rust.sh`, generators of Rust tables, NGP tools |

Treat `original/`, `build/` and the recordings as local data. Do not commit their contents.

## Rust rules

- Physics and gameplay stay bit-exact against the original's recordings.
- Presentation (rendering, sound, HUD, launcher) never changes the physics: it reads the physics memory after a step,
  or copies a payload at an existing host sink, without writing to it.
- One module or crate per system. Shared files (`renderer.rs`, `viewer.rs`, both `live.rs`, `lib.rs`, `Cargo.toml`,
  the README tables) get small hooks only. Data parsers go in `rbr-formats`, one module per format, with a test over
  every retail file it covers (`RBR_DATA`).
- Physics ports are pure functions over typed values; memory access (`Mem`, raw offsets) only in the subsystem's
  `layout.rs` or `layout/` directory.
- Never commit game files: no binaries, data, RSF content or anything extracted from them.

## Git hygiene

- Keep changes limited to the assigned system.
- Preserve unrelated dirty files in the main checkout.
- Commit small, logical batches on the worker branch. Do not push.
- Never use `git stash`: the stash is shared by every worktree, so another worker can pop your entry. Set work aside
  with a WIP commit or a patch file.
- Merge current `integration/handoff` into a worker branch before final verification when the branch has fallen
  behind.
- Report conflicts, failing checks and unresolved questions.

## Definition of done

A Rust batch includes:

- `scripts/rust-check.sh` from the repository root printing PASS, with `RBR_DATA`, `RBR_PATCH` and `RBR_RECORDINGS`
  set;
- for physics ports: the hybrid check (`harness/rust_step.py` with every binding over the drive's snapshots and
  recording) with every "run B vs run A" line equal, and fuzzed direct-call records for branches no drive reaches;
- for changes to `rbr-standalone` or `rbr-hybrid`: `rbr-standalone --boot --recording
  $RBR_RECORDINGS/drive-tanner-corolla.bin --stage track-54_E --steps 13000` giving 13000/13000 equal;
- for a format: a test that parses every retail file it covers;
- for visible or audible systems: before/after captures or a short audio render;
- short updates to the area's note for verified layouts and behaviour, and a report listing commits, checks and
  blockers.

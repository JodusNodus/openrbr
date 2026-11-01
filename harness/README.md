# Harness

Records and replays the original 1.02 game for the exactness checks. How it fits together, commands and masks:
[notes/verification.md](../notes/verification.md). Python tools run with `.venv/bin/python -I -B` (unicorn, numpy);
each script's docstring holds its usage. The replay, hybrid check and vector tools run on the published snapshots
and recordings (`scripts/fetch-test-data.sh DIR recordings vectors snapshots`); the ones that record or play the
original (`play.sh`, `record.sh`, `slots.sh`, `matrix.py`, `apitrace.sh`) need this PC's game installs (`dev.env`:
`RBR_GAME`/`RBR_PREFIX`, `RBR_NGP_GAME`/`RBR_NGP_PREFIX`). Harness Python runs locally with
`.venv/bin/python -I -B harness/...` after `cargo build --release -p rbr-hybrid`; the x87 oracle builds
`harness/replay-fpu.c` locally with gcc into `build/replay-fpu/`.

| Path | What |
| --- | --- |
| `plugin/` | `RBRRecompHarness.dll`: recorder, playback, snapshots, peeks, early stop, alloc log (`build.sh [--install]`) |
| `play.sh` | one unattended playback run in a free slot locally (`RBR_DRIVE`, `RBR_SNAPSHOT`, `RBR_PEEK`, ...); the transplant modes are rbr-recomp's |
| `record.sh` | record a live drive by hand |
| `v1-drive.sh` | make a drive from another drive's controls on another stage and car |
| `drive-trial.sh` | find the StartGame numbers of a menu-started drive |
| `slots.sh` | make the parallel game slots locally |
| `matrix.py` | stage × car pairs end to end (Rust autopilot, NGP original, Rust check) |
| `diverge.py` | first differing step between two recordings; `--mask` writes the noise mask |
| `session.py` | RSS1 session files of an RSF session (`scripts/rsf-session.sh`): reader and summary; the format is its docstring ([verification.md](../notes/verification.md#rsf-sessions)) |
| `replay.py` | one `StepForward` of the original in the emulator from a snapshot |
| `replay-approx.c`, `.bin`, `replay-fpu.c`, `.py` | the CPU's rsqrtss/rcpss tables and x87 oracle for the emulator |
| `rust_step.py` | the hybrid check: Rust bindings (run B) against the original (run A) |
| `long_run.py` | `rust_step.py --steps` over a drive's whole snapshot chain |
| `live.py` | the hybrid stepped live, harness-only (FRM1 out per control payload); the viewer drives `rbr-standalone --live` only |
| `vcu_vectors.py`, `record_entities_b4.py` | per-call vectors from a step, optionally fuzzed |
| `direct_calls.py` | fuzzed direct calls from a function's entry |
| `branch_cover.py` | which way each conditional jump went over a `direct_calls.py` run (no records) |
| `vector_specs/` | specs per vector set ([README](vector_specs/README.md)) |
| `subtree_tape.py` | the struts' calls over N steps (`tests/pure_struts.rs`) |
| `load_*_refs.py`, `lsp_tokens.py`, `ngp_data_refs.py` | load-path reference vectors cut from dumps |
| `compose_load.py` | write the Rust load's objects over a snapshot |
| `ngp_init_diff.py` | diffs an NGP `--boot` image against NGP reference vectors |
| `snapshot_reference.py` | snapshot view for the Rust parity test |
| `apitrace.sh` | D3D9 capture of a playback run |
| `ffb_trace.sh`, `.c`, `.py` | the wheel's force feedback at its evdev device locally (LD_PRELOAD, no root): RSF, Proton's dinput replaying NGP's calls (`probes/ffb_replay.c`) and ours (`rbr-render` example `ffb_play`); summary, level comparison, per-step pairing and timing |
| `probes/` | DirectInput test programs and peek configs |

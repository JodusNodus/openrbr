# Harness

Records and replays the original 1.02 game for the exactness checks. How it fits together, commands and masks:
[notes/verification.md](../notes/verification.md). Python tools run with `.venv/bin/python -I -B` (unicorn, numpy);
each script's docstring holds its usage. Most of them need the test machine or the snapshots it makes, which only
the maintainers have; `diverge.py` needs only recordings (`scripts/fetch-test-data.sh`).

| Path | What |
| --- | --- |
| `plugin/` | `RBRRecompHarness.dll`: recorder, playback, snapshots, peeks, early stop, alloc log (`build.sh [--install]`) |
| `play.sh` | one unattended playback run in a free slot on the test machine (`RBR_DRIVE`, `RBR_SNAPSHOT`, `RBR_PEEK`, ...); the transplant modes are rbr-recomp's |
| `record.sh` | record a live drive by hand |
| `v1-drive.sh` | make a drive from another drive's controls on another stage and car |
| `drive-trial.sh` | find the StartGame numbers of a menu-started drive |
| `slots.sh` | make the parallel game slots on the test machine |
| `matrix.py` | stage × car pairs end to end (Rust autopilot, original, Rust check); `--ngp` |
| `diverge.py` | first differing step between two recordings; `--mask` writes the noise mask |
| `explain.py` | name the object and field of a recording word offset |
| `replay.py` | one `StepForward` of the original in the emulator from a snapshot |
| `replay-approx.c`, `.bin`, `replay-fpu.c`, `.py` | the CPU's rsqrtss/rcpss tables and x87 oracle for the emulator |
| `rust_step.py` | the hybrid check: Rust bindings (run B) against the original (run A) |
| `long_run.py` | `rust_step.py --steps` over a drive's whole snapshot chain, then the worklist |
| `live.py` | the hybrid stepped live for the viewer (FRM1 out per control payload) |
| `worklist.py` | original functions a hybrid step still runs in the emulator |
| `pure_check.py` | host calls a pure Rust step would stop at |
| `step_reached.py` | functions the original step reaches (`scripts/progress.py step`) |
| `read_hook.py` | which code reads a memory range during a step |
| `vcu_vectors.py`, `record_entities_b4.py` | per-call vectors from a step, optionally fuzzed |
| `direct_calls.py` | fuzzed direct calls from a function's entry |
| `vector_specs/` | specs per vector set ([README](vector_specs/README.md)) |
| `subtree_tape.py` | the struts' calls over N steps (`tests/pure_struts.rs`) |
| `load_*_refs.py`, `lsp_tokens.py`, `ngp_data_refs.py` | load-path reference vectors cut from dumps |
| `compose_load.py` | write the Rust load's objects over a snapshot |
| `boot_diff.py`, `ngp_init_diff.py` | diff a `--boot` image against a snapshot or NGP reference vectors |
| `snapshot_reference.py` | snapshot view for the Rust parity test |
| `allocs.py` | print an allocation log |
| `remote.sh`, `remote-runner.sh` | run a harness script on the test machine (`RBR_REMOTE_HOST`; [verification.md](../notes/verification.md#test-machine)) |
| `apitrace.sh` | D3D9 capture of a playback run |
| `probes/` | DirectInput test programs and peek configs |

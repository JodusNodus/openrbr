# Work log

Keep this file short: current state, open items, and one line per decision. Trim it regularly. Reference detail belongs in `notes/`; history up to 2026-10-13 is in `notes/archive/log-2026-10.md`. The C++ reconstruction's matching log (its batches, merges and open C++ items) is in the rbr-recomp repository's LOG.md.

## Current state (2026-10-15)

- **Goal and method** (`notes/rust-plan.md`): a standalone Rust program, no original code running, that plays every
  1.02 stage and car with physics and the gameplay loop bit-exact against recordings of the original. Main track: the
  hybrid step (`rust/rbr-hybrid`, `harness/rust_step.py`) with every binding, run over long stretches and compared with
  the recording; its emulator worklist (`harness/worklist.py`) is driven to empty, then a Rust `Host` replaces the
  emulator.
- **Behaviour reference:** the original (recordings, snapshots). The C++98 reconstruction (the rbr-recomp
  repository) is frozen as a readable reference: exact end to end on tanner-corolla (`notes/progress/drives.csv`), not fixed further on other drives
  (open C++ culprits: `lead/reports/f5-new-drive-culprits.md`).
- **Rust:** 339 of 556 reached non-exact physics functions ported (`scripts/progress.py report`); 221 bindings in
  `rust/rbr-hybrid/bound.csv`; the strict 30-step chains on the 9 tanner-corolla snapshots are clean. First pure
  subtree done: the struts (B0, `rust/rbr-physics/tests/pure_struts.rs`). Live viewer runs the hybrid at ~170 steps/s.
- **Checks:** a ported group is bound whole and checked by one long run (for now the chains on the 9 snapshots); a
  byte may differ only if the gate masks it, it is a heap pointer, or the original never reads it. `--only` and
  bisect only to locate a divergence. Records of fuzzed direct calls only for branches no drive reaches.
- **Formats:** every stage and car file type parses on all retail files (`rust/rbr-formats`, `notes/formats/`).

## Open items

- The long-run driver and the ranked worklist (`lead/reports/pure-step.md`, being rewritten); then the worklist, group
  by group (engine-driveline, wheel, entities, collision, physics-core, car-body; the C++-exact accessors as one batch).
- New drives (tanner-damage, tarmac-lancer, arctic-snow) checked in Rust once the long-run driver exists.
- Load path step 2 (`notes/rust-plan.md` L); the Rust gameplay loop (G) after the worklist has shrunk.
- Particle system for the renderer: not started; handoff, data map and plan in `notes/particles.md`.

## Decisions

- 2026-10-19 (user): the repository becomes ReRBR, the Rust reimplementation only; the C++ reconstruction moves to its
  own repository, rbr-recomp. `data/rbr.recomp.csv` keeps its last source files and reccmp scores for the tools here.
- 2026-10-16 (user): particles aim to look good, not to match 1.02; the retail data and signals are the starting point
  (`notes/particles.md`).
- 2026-10-16 (user): the in-car windscreen gets a modern water-on-glass system (refraction, GPU drop simulation, wiper
  mask), not a port of 1.02's 2D effects (`notes/particles.md`, In-car glass effects).
- 2026-10-15 (shortcuts, `notes/rust-plan.md` Order): the C++ physics is frozen (no culprit fixes on new drives, no
  backports, no reccmp/renames for physics); Rust is translated from it and checked against the original directly.
- 2026-10-15: the driver is the all-bindings hybrid over long runs; headline metric is its emulator worklist; no subtree
  tapes. A group is bound whole and checked by one run; `--only`/bisect only on divergence. Fuzz records only for
  unreached branches. Rust matches every word the gate compares; the gate's masked lane-3 residue is allowed.
- 2026-10-15: the gameplay loop is written directly in Rust and checked against recordings; 1.02 replay deferred.
  Rust-only branches merge on `scripts/rust-check.sh` plus the group check; one LOG line per merge.
- Gameplay-only scope (2026-10-13, `notes/rust-plan.md`): no more matching or coverage work for UI, menus, rendering, sound, HedgeHog3D or LocoMotion. Their sources stay as frozen reference (rbr-recomp).
- Decompile the official 1.02 binaries, not the RSF build. Never touch the user's RSF install on cachy (`~/Games/Richard Burns Rally`, `~/Games/prefixes/rbr`); runtime tests use `~/Games/rbr-vanilla-1.02`.
- Existing crates over hand-written parsers.
- Workers: one worktree each; reports go to `lead/reports/<tag>.md` (worker_message bodies do not reliably arrive).
- R1: `rust/rbr-recording`, a bounded reader for the harness FRM1 recordings (frames, control, car, wheels; offset-precise errors) plus `rec_info`; 300-frame sample of ref1 parses (first 4 frames are the 40-substep load settle).
- C1 (car formats): `notes/formats/cars.md` (per-car files, Cars.ini index -> physics dir -> tyre set, physics.rbz listing, damage block = `HGMD` v3 inside replays, read by CCarDamageHandler::ReadBinary 0x4eee00), `.lsp` key tree per file role with NTweak class mapping from the registry at 0x7c7fa0 (lead re-checked the table), `rbr_formats::{ini,cars}` with a retail test.
- C1b: CHELALIC archive reader (`rbr_formats::dat`: physics.dat, Textures/*.dat; loader E15 0x483580) and `repair.ini` reader (`rbr_formats::repair`, loader 0x5b16f0); physics.dat = the physics.rbz tree with 4 differing files (MaxTimeStep, tyre RubMyFct_T1, one p_206 hinge sign).
- Lead: 3 parallel harness slots on cachy (harness/slots.sh; play.sh takes the first free slot); two parallel baseline runs identical to ref1 to the end of the controls (only the known heap-pointer control word after the last control).
- F2 (stage collision .col, notes/formats/col.md): full structure from the loader (FUN_004fe570 and helpers): header, root (type 4/3, tree, mesh lists), cells (mode 1, graft to root placeholders), tree nodes, 18-byte triangles; every retail file (86, 5.9 M triangles) tiles exactly. `rbr_formats::col`.
- F3 (stage geometry .lbs, notes/formats/lbs.md): loader chain exe CPC_TrackState::LoadTrack 0x466f10 -> HH CHH_TrackLoader::LoadTrack 0x10052220; container framing (same as .trk) tiles all 88 files; geometry (1,0) and object lanes (1,1) fully parsed (vertex streams per slot stride, render chunks = DrawIndexedPrimitive ranges, texture indices); track-10_O exports to OBJ. Other chunks (sky, water, visibility, ...) raw. Found: src/hedgehog3d HH_TrackLoaderLoad never opens the file (takes the path from the wrong argument).
- G1 (control path + game loop, notes/gameplay-loop.md): the 8 control words with producer/consumer addresses (word 6 is an integer command slot written only with flag bit 2), input sampled before CApplication::Tic, fixed step = PhysicsEngine.MaxTimeStep from physics.rbz (0.007 s per substep). Fixes: TicPhysicsRecording reads the definition at host +0x24 (typed member compiled at +0x14), Tic passes frame dt to EmitForce, StepForward flags/collision gate/scaled step. Open: our StepForward copy crashes on transplant (+0x202); E13Host9B70 typed members sit 0x10 too low (vtable not counted) in other E13/E21 users.
- F4 (car models .sgc, notes/formats/sgc.md): HedgeHog3D loader 0x1001f2a0; banner + 12-byte-header records tile all 8 cars; 718 static + 287 damageable meshes (bounds, pos+normal, UV, u16 triangles, faces = <car>.ini FaceBlock materials, morph block on damageable). Found: src/hedgehog3d HH_H7LoadGeomCar reads an extra int before the 24-byte bounds.
- F2b (.col consumers, notes/formats/col.md): brake-wall definition parsed (76 walls); triangle readers traced (CCollisionSystem::Vf00, ray casts, wheel ground probe): +0x06/+0x08 are per-vertex 5-bit attrs, +0x0a/+0x0b index the .mat run table (E44MatTable::Pick 0x520240) giving a surface id into CMaterialManager (Tag520940). Open: surface id -> physics.lsp surface type mapping (needs the CMaterial population code or a harness probe of CWheel::Unk9900).
- F3b (.lbs): stage texture resolution through TextureFilenameMap.dat (every referenced texture of the 86 stages resolves; 556 via a sibling variant's archive), object item texture/kind/vertex layout (shaders 18/19), sky dome (1,3) parsed; water/reflection layout documented only.
- F1 (stage formats): every retail file of .mat (zero-run materials are legal: FUN_0046a290), .fnc (the 72-byte tail is a texture table, FUN_006058c0), .trk (all chunks: (3,0x14) driveline, (3,0x16) collision meshes), .dls (FUN_00479020: AnimData tables, replay stream v6) parses; all retail tests pass (15). No split/checkpoint/start/finish data in .trk or .dls.
- F2c (surface ids, notes/formats/col.md): the .mat cell byte is the surface id; exe table at 0x7caf38 (134 records {name, value = 88*id + 3}; lead re-checked) maps it to the physics.lsp surface key (lsp file order and `; id` comments are not the key). track-10_O maps to SnowOnGravel/SnowOnIce/Snowwall. Enable bit is entry+0x158 bit 0 (VALID). Not yet confirmed in game.
- F5 (stage markers, notes/formats/stage-markers.md): start (tag 0x15), splits (0x17, max 16), finish (0x16), end (0x18) and pace notes are keys of the .dls `Driveline` set, on the driveline-distance axis (metres, inferred); read by CTimeKeeper::Load 0x4a01f0 and dispatched by the E12 tic 0x4783b0. `rbr_formats::markers`, all 86 .dls.
- H1 (harness peek probes): RBR_PEEK=<file> logs pointer-chain values every physics step (plugin RBRRecompPeek.txt, PEK1 trace, rbr-recording peek reader + peek_info). First use confirms the surface mapping in game: on Tanner the wheels' tags are Gravel_Medium_{Deeper,Deep,Shallow}_Damp and Grass_{Medium,Soft}_Damp (harness/probes/surface-tags.peek).
- Lead (took over H1): the plugin keeps the game running unfocused (74383f20: subclassed window proc swallows WM_ACTIVATEAPP(FALSE)/WM_ACTIVATE(WA_INACTIVE)/WM_KILLFOCUS before the game's pause gate). 3 parallel baselines x2 identical to ref1 over 12,084 steps; play.sh back to 3 slots, and every run installs its plugin (RBR_HARNESS_PLUGIN or ~/rbr-harness-plugin) so no run inherits another's build.
- G1b (live probe confirms MaxTimeStep = 0.007 s on all 12,084 steps): CPhysics::StepForward dereferenced its +0x10 scratch object as a pointer (the +0x202 crash), walked the +0x480 list wrongly and used the scaled step for Work5500; E13Host9B70 counted no vfptr (typed members 0x10 low: fixed in the class, TicPhysicsRecording's raw-offset workaround dropped); EndRace signal payload is 20 bytes with state at +0x10. StepForward, TicPhysicsRecording, TimeKeeper Update/Check verified by transplant runs over the whole drive. Control payload table in notes/gameplay-loop.md (brake, throttle, handbrake, clutch, steering, flags, gear selection, aux bit). RBR exact 9544 -> 9546; accepted E61_M0056e8a0 56.5 -> 48.4 (corrected payload).

## 2026-10-13 diverge.py: step-9,729 flake masked
- The "diverged at step 9,729 (2 words)" seen by P2W, P3, P3X and in five sweep groups: words 0x678/0x694 (car+0x658 bit 1, car+0x674 1.0 -> 0.0) change at a frame-driven event and shift by whole fast-forward frames when transplant overhead packs steps differently (8 or 112 steps late), then agree again. Same class as car+0x824. Masked in harness/diverge.py and cachy /tmp/lead-diverge.py; e2e119/265/479 now identical. No single function ever diverged this way, so no false culprits.

## 2026-10-14 Harness early stop
- The plugin compares each recorded step with ~/rbr-harness-runs/ref1.bin (RBRRecompEarlyStop.txt) and quits at the first divergence, ignoring ref1.bin.mask (`diverge.py --mask ref1.bin ref2.bin`) as diverge.py does; play.sh enables it for plain transplant runs (RBR_EARLYSTOP=0 to run to the end). diverge.py reads REF.mask too, so a short run reports the same step.
- Tests: a step-1389 divergence (g3-30) stopped at step 1389, same word 0xb0c, in 28 s instead of ~56 s; a verified batch ran all 12,084 steps identical. diverge.py on the last 40 recordings: same results (only words-compared counts differ on short runs). Installed as the shared plugin; /tmp/lead-diverge.py on cachy updated.
- Shadow census of the gate set (250 transplantable): 3 shadow-eligible; 230 call other functions, 72 write globals, 38 have EH frames. Allowing calls to shadow-eligible leaves would add ~34.

## 2026-10-14 Rust pilot merged (parked)
- rust/physics-pilot: harness capture mode (RBR_TRACE=N: the first N calls of each shadow-eligible function, registers, args, regions before/after, x87 control word; rust/rbr-recording trace reader + trace_info), and rust/rbr-physics (fpu::round24, spring_damper, tyre leaves; 15 damper + tyre vector tests bit-exact on a capture), parked as a test oracle. User: the pilot proved bit-exact Rust works; its code is not the shipped design. Runtime x87 precision is 24-bit.
- Plugin tests before install: plain transplant identical over 12,084 steps; shadow run identical, 0 mismatches (DamperCombine 27,867 calls). Installed. cargo test (208) and clippy clean; the peek.rs fmt diff predates this.
- P3R A2 bisect: 0x502870, 0x506e60 (19,016), 0x506f20 marked verified; culprits 0x4ff070, 0x4ff170, 0x502780, 0x5029c0.

## 2026-10-14 Comparison fix: the first-step mask hid real differences
- Step replay (agent-1224) found BlendB 0x50a650 wrong: car+0x17c 5.0869 vs 5.2217 from the first record (post 40) to the end. diverge.py and the plugin's early stop ignored every word that differed at the first common step, so any change a transplant makes during the load settle and keeps was never reported; BlendB had been "verified".
- Why that mask existed: any harness mode (count runs too, where our code never runs) shifts the heap, and ~172 words (uninitialised memory, pointers) differ from ref1 for the whole drive. Fix: those words now come from count runs (refnoise-count1..3.bin on cachy, kept by the pruning) in ref1.bin.mask (1005 words), and the first-step rule is gone from diverge.py and the plugin. Checks: ref2 and 3 count runs identical; every BlendB run now diverges at step 40 (0x19c); other verified runs checked stay identical. Plugin reinstalled.
- 0x50a650 marked bad. Every "verified" function has to be re-verified under the new rule: gate sweep 3 stopped; sweep 4 runs the whole reached non-exact physics set (564, verified included).

## 2026-10-14 Step replay merged (designer agent-1224)
- harness/replay.py (unicorn): replays one StepForward call from a plugin snapshot (RBR_SNAPSHOT="40 572 1295"), run A all original, run B with our functions patched in like ApplyTransplant, and reports the first differing write. Run A bit-exact at 40->80 (40 substeps), 572->573 and 1295->1296 (frame and every written byte). Emulator fixes: rsqrtss from the CPU's 2048-entry tables (replay-approx.bin), x87 transcendentals from cachy's CPU (replay-fpu.c, cached); PC=24 and x87 arithmetic match. 0.2-0.4 s per step.
- Plugin: the snapshot path made StepForwardHook's frame bigger, which moved the step's esp and changed uninitialised stack the step stores (car+0x8ec, the 5 "extra" words in snapT2); the hook now compiles to main's frame and snapshots go through a naked stand-in at the same esp.
- --each over the 475 verified functions at 40/572/1295: 178/149/179 identical, 286/316/286 not reached, 11/10/10 differ. Real value differences, marked bad: 0x4fd480 M_D480 (wheel0+0xcb0), 0x50ed70 SimED70 (car+0x1330, then a null+0x188 fault), 0x52c8f0 VfC8 (car+0x17c = 0), 0x532f10 FillOutputs (wheel0+0x1344), 0x53d0d0 MultiTire::Emit (calls into HedgeHog3D where the original doesn't), 0x4ea300 Vf6cx (garbage local stored at car+0x8ec; a drive diverges). Left verified, flagged: 0x51bd30 CollideInit (pool pointer in global 0x16588c4 differs: allocation, not physics, unless shown otherwise), and stack-garbage-only differences in 0x4eb6a0, 0x5273a0, 0x533080, 0x539a20.
- Plugin with the replay snapshot mode installed after plain/shadow/count tests, all identical over 12,084 steps (rp-plain, rp-shadow 0 mismatches, rp-count).

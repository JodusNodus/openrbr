# Gameplay sound

How 1.02 makes its in-race sound and how `crates/rbr-sound` reproduces it. Addresses are `RichardBurnsRally_SSE.exe`
1.02. File layouts (`audio.dat`, audio inis, `.eng`, FMOD banks): [formats.md](formats.md). The code's doc comments
carry formulas and constants. Detail: rbr-recomp `notes/sound.md`, `notes/marks-sound-ghost.md` section 2.

## Architecture

Layers in `lib.rs` (models, mixer, `device`, `Player`), the mixer's rules in `mixer/mod.rs` (queue, limit, master
gain, limiter, ours-divergences), distances in `world.rs`. The model is NGP 7.4.778's (`Inputs::ngp`); FMOD banks are
modelled only for cars that have one (`fmod`). The pipeline never writes physics or gameplay state.

- Sound limit evidence: tanner in the car plays 1431 of 1607 gravel hits at 32 against 604 at 16 (not in code).
- The 30 ms sound thread never runs (not in code); the in-car flag is the viewer's bonnet and cockpit views.

## Faithful and ours

Every source is faithful except the divergences documented as ours in each module (`mixer/mod.rs` lists the mixer's;
`engine.rs`, `car.rs`, `tyre.rs`, `impact.rs`, `menu.rs` their own). Decisions kept here:

- Frame clock, resampling, stereo balance, fades, pitch/gain variation, cabin low-pass: `mixer`.
- Realism (rally car): hotter in-car engine EQ, dog-box whine with coast whine, anti-lag crackle, sound limit 32.
- Engine grains, whine smoothing, blow-off once per arming, software tyre path, gear-click edge, wipers per blade
  change, impact spacing, hard reflections, late-join countdown skip, retail fallbacks, own bird timer, siren at the
  sphere, H-shifter grind: the source's module.
- No music (user decision).

## Engine

Mechanics in `engine.rs` (inputs, bank, slew, crossfade, grains, `MixScale`, output filter), reflections in
`reflections.rs`, whine/turbo/blow-off in `car.rs`.

- All 8 cars use `subaru.eng`; four of the other `.eng` files cannot load in 1.02 (not in code).

## Exhaust and car effects

Mechanics in `exhaust.rs` (pops, bangs, runs, engine fire, anti-lag) and `effects.rs` (ignition, dog rings, cooler,
wipers).

- Only the wipers fire in the drives tried (check coverage, not in code).

## Tyres and surfaces

Mechanics in `tyre.rs` (loops, rumble, gravel hits, rattles, software path, low-pass) and `tables.rs` (surface
table); NGP's road list in `rbr_formats::ngp`.

- The splash is 0 in every drive tried, so no wetness voice is checked. Lancer leaving the road at 69.9 s, in the
  car: -22 to -17 dB over 200 ms now (evidence for the gain law, not in code).

## Suspension and driveshaft

Mechanics in `suspension.rs` (squeaks, knocks, bottoming, damage call 7, frame folding) and `driveshaft.rs` (clunks).

## Impacts

Mechanics in `impact.rs` (payload change rule, ranged set, scream).

- A scrape changes the payload every step: Lancer 75-90 s, 371 impacts to 22 (evidence for the spacing rule, not in
  code).

## Co-driver

Mechanics in `codriver.rs` (phrase, queue, damage calls, countdown). Word ids: `rbr_gameplay::markers`.

## Custom co-driver

Mechanics in `custom_codriver.rs` (window, spoken call, sticky rules, preview in `preview.rs`, HUD integration);
pack layout in `rbr_formats::rsf`.

- RSF install (2026-10-26): all 17 styles and both range sets load the same packages, so the voice decides the words
  (`Janne Laahanen EnNu A`: Descriptive ids on numbered takes, `six_left` for EASYLEFT); the launcher hides the style
  and range rows when the sets say the same (data observation, not in code).
- Owned copies: Janne V3 with Tanner LuppisV3 loads 1,154 samples without RSF configured; a 40 s constant-speed
  preview gives the same 47 calls and byte-identical WAV as the source pack (local copied-pack review, 2026-10-22).

## Countdown

Mechanics in `codriver.rs` (HUD codes, `CSoundFX`, late-join skip). No finish sound.

## World and ambient

Mechanics in `world.rs` (wind, birds, siren) and `mixer/rain.rs`. Birds, wind and siren play only in
`rbr-sound --camera trackside` (the binary has no replay cameras).

## Scripted and service park

Mechanics in `scripted.rs` (helicopter, bird hit, incident) and `menu.rs` (service park, music).

## FMOD cars (ours)

Mechanics in `fmod.rs` (block, parameters, pauses, bank levels, `LOUDNESS`), `mixer/fmod_mix.rs`, `mixer/dsp.rs`; bank
layout in `rbr_formats::fmod`. Cars without a bank use slot 5's retail sound (Impreza 2003); the rest of the sounds
come from the retail `audio.dat` (`audio.dat.disabled` in RSF installs).

- NGP's FMOD output sits 6.8 dB (car) and 8.8 dB (outside) above ours (not in code).
- Checked: gvrc's C2 S1600 bank loads (not in code).

## Open

- Which mixer (DX, Prologic, EAX) runs; which resampler mode 2 is.
- Whether 1.02 on current Windows takes the tyres' software path.
- The sound limit's count: 9 against the live list's 11.
- HUD countdown against the gameplay start; why a `--boot` drive starts below 0.
- What writes the splash (`+0x98`); ignition, dog rings, cooler and `EngineNoise p[5]`/`p[7]` never seen live.
- `E31Speech +0x95` as the idle flag (inferred).
- Wine's constant-power 3D panning against 2D speech.
- FMOD: region end rule, async and loop flags, parameter defaults, 3D attributes, an A/B against NGP.

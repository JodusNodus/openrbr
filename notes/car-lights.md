# Car lights

How the 1.02 game switches and draws the car lights, written for implementing them in `rbr-render`.
Rendering is out of scope for matching, so this records behaviour, not match work.

There are three parts:

1. The exe's light system (`NPhysics::CCarLightSystem`) decides which lights are on and emits a
   `CSigCarLight` signal per lamp.
2. HedgeHog3D's `CHH_World::OnCarLight` switches the lit and unlit lamp meshes and starts the flare fade.
3. `CHH_Car::RenderLights` draws an additive billboard sprite ("flare") at each light point.

The exe looks up a rain/snow headlight particle emitter, but no retail `PS_*.ini` defines one (see the last section).

## Light kinds

Each kind has up to four lamps, numbered by index `i`. The `Id` in `<car>_settings.ini` is `kind * 4 + i`,
which is the key HedgeHog3D uses to look up a light record (see `OnCarLight` below).

| Kind | Lamps | Mesh switch box | Settings `Id` | Driven by | State at start |
|---:|---|---|---|---|---|
| 1 | front headlights | `light_lf`, `light_rf` | 4, 5 | nothing (static) | on |
| 2 | — | — | — | nothing | off; HedgeHog3D ignores it |
| 3 | tail lights | `light_lb`, `light_rb` | 12, 13 | nothing (static) | on |
| 4 | brake lights | `brake_lb`, `brake_rb`, `brake_m` (206 only) | 16, 17, 18 | brake pedal > 0.1 | off |
| 5 | reverse lights | `reverse_lb`, `reverse_rb` | 20, 21 | drive gear index == 0 | off |
| 6 | — | — | — | nothing | HedgeHog3D ignores it |

All eight retail cars define kinds 1, 3, 4 and 5 with two lamps each. The Peugeot 206 also has the centre brake light
(`Id = 18`, `Point_light_brake_mb`, mesh `e_light_brake_m[_lit]`).

## Exe: `CCarLightSystem` (E36, `src/rbr/RBR_E36_Light.cpp`)

`CCarLightSystem` is 0x18 bytes at `CCar+0x838`. In the header, `m_unk04` is the `CDef` (`CCar+0x4210`), `m_def` is
the `CState` (`CCar::CState+0x9d0`; the header name is wrong), and `m_flags` is the light block inside
`CCarDamageHandler` (`+0x3b24`).

- **`CDef`** ("LightSystem" tweak): six `CUnsigned[4]` arrays at `+0x7c + 0x40*(kind-1)`. Lamp `i` of a kind exists
  when that array's element `i` is non-zero (`EmitLights` skips lamps that don't exist).
- **`CState`**: one `CUnsigned` per kind at `+0x20 + 0x10*(kind-1)` (value at `+0xc`), meaning the kind is
  switched on. The ctor (0x4f1840) sets kinds 1 and 3 to 1 and kinds 2, 4 and 5 to 0, and leaves kind 6 unset.
- **Light block** (`E36LightBlock`, 0xe0 bytes, in the damage handler, saved with it): dword 0 is a "refresh all"
  flag. Dwords 1-6 are per-kind dirty flags. Each kind has a group of 8 dwords at `+0x1c + 0x20*(kind-1)`: lamp `i`
  uses the pair `[2i]` = lamp works and `[2i+1]` = glass intact. The tail word at `+0xdc` is a master switch that
  multiplies every kind's state. `Reset` (0x4f19b0) sets everything to 1. Which damage events clear these words has
  not been traced yet.

Dirty flags and setters (each setter compares, writes the `CState` value and raises the dirty flag only on change):

| Kind | Setter | Dirty dword | Caller |
|---:|---|---:|---|
| 1 | `Set1f00` 0x4f1f00 | 1 | none |
| 2 | `Set1f30` 0x4f1f30 | 2 | none |
| 3 | `Set1e70` 0x4f1e70 | 6 | none |
| 4 | `Set1e40` 0x4f1e40 | 3 | VCU `Step`/`VcuStepBasic`/`UpdateFA` (0x536fa0, 0x537f50, 0x5380a0) |
| 5 | `Set1ed0` 0x4f1ed0 | 4 | VCU `Step`/`VcuStepBasic` |
| 6 | `Set1ea0` 0x4f1ea0 | 5 | none |

"None" means there's no direct call or pointer to the setter anywhere in the exe, so headlights and tail lights have
no in-game switch. The physics doesn't care about time of day here.

- **Brake (kind 4):** on when the value sent to the drive's brake (`Set30`) is `> 0.1`. Normally that's
  `m_brake` (`+0x10`). When the VCU pedal swap is active (`vcu_enabled && b+0x58`), it's `m_throttle`
  (`+0x0c`), so the lights follow whichever pedal brakes (`src/rbr/RBR_E50_Sim.cpp`).
- **Reverse (kind 5):** on when `drive+0x54c == 0`. This supports reading (b) in `notes/physics/engine-driveline.md`
  (0 = reverse), but that question is still listed as open there.

`CCar` calls `Update` (0x4f2150) every step from 0x4ea05c. For each kind whose dirty flag or the refresh flag is
set, it calls `EmitLights(kind, …, scaled = state[kind] * block.tail)` and then clears the flags. `Attach`
(0x4f1e00) raises every flag, so the first update emits every lamp.

`EmitLights` (0x4f20c0) emits one `CSigCarLight` per existing lamp through signal manager `+0x9af0`:

| Offset | Value |
|---:|---|
| `+0x00` | 1 |
| `+0x04` | lamp index `i` |
| `+0x08` | main: 1 = lit if `scaled != 0` and lamp works, else 2 |
| `+0x0c` | glass: 1 if glass intact, else 2 |
| `+0x10` | kind |

## HedgeHog3D: `CHH_World::OnCarLight` (0x10068e30, `src/hedgehog3d/HH_World_H25_A.cpp`)

1. Kinds 2 and 6 are ignored, and so is any index `>= 3`. For every kind except 1, the suffix index is `i + 2`.
2. Name = `prefix[kind] + suffix[index]`, where prefix = `{"", "light", "", "light", "brake", "reverse"}` (0x101da280)
   and suffix = `{"_lf", "_rf", "_lb", "_rb", "_m"}` (0x101da26c). That gives `light_lf/rf` for kind 1,
   `light_lb/rb` for kind 3, `brake_lb/rb/m` for kind 4 and `reverse_lb/rb` for kind 5. Glass name =
   `"lightglass" + suffix[index]`.
3. For each switch object in `CHH_World+0x270` whose name matches, it selects `"on"` if main == 1 and `"off"`
   otherwise. That shows the `<car>.ini` parts with `SwitchBox = <name>` and the matching `Predicate`
   (`e_light_lf_lit` is `on`, `e_light_lf` is `off`). It then finds the car's light record with key
   `kind * 4 + i` (0x100182c0), sets record `+0x04` to `main == 1`, and sets record `+0x20` to the car clock
   (`CHH_Car+0x14`). Our source writes byte `+0x01` instead (`((bool*)car)[1]`); the original writes `+0x04`.
4. The glass object gets `"on"` if glass == 1 and `"off"` otherwise. In XSARA, `e_lightglass_lf/rf` are switched
   (`Predicate = on`, so they disappear when the glass breaks), while `e_lightglass_lb/rb` have `Switch = false`.

At load, `CHH_World` vf14 (0x100690a0) sets every `light`/`brake`/`reverse` box to `off`. `rbr-render` already draws
that start state (`shown_at_start` in `rust/rbr-render/src/car/parts.rs`). Then the exe's first `Update` turns kinds 1
and 3 on, so in play the headlight and tail-light `_lit` meshes are shown from the first frame.

## Flares: `CHH_Car::InitLights` (0x10016fd0) and `RenderLights` (0x100158b0)

`InitLights` reads `<car>_settings.ini` sections `Light0`, `Light1`, … until one has no `Id` key (`NLB_HasEntry`: an
empty `Id =` still counts, and reads as 0). Each section becomes a
0x38-byte record in `CHH_Car+0x04`:

| Offset | Source | Default |
|---:|---|---|
| `+0x00` | `Id` | 0 |
| `+0x04` | `Active` (byte; later the on flag from `OnCarLight`) | false |
| `+0x08` | `Pos` of the `[Car Part]` section in `<car>.ini` | — |
| `+0x14` | `Normal` of the same section | — |
| `+0x20` | time of the last switch | 0.0 |
| `+0x24` | `Fade In Time` | 1.0 |
| `+0x28` | `Fade Out Time` | 1.0 |
| `+0x2c` | texture index of `Texture` (from `<car dir>\Textures\`) | `Light.tga` |
| `+0x30` | colour `0x00RRGGBB` from `Red`/`Green`/`Blue` | 255 each |
| `+0x34` | `Size` | 1.0 |

`InitLights` doesn't read the `Use Lens Flare*` keys, and none of the retail cars enables them anyway.

Render state: additive blending (`SRCALPHA`, `ONE`), no z-write, texture modulated by vertex colour, FVF `0x142`
(xyz, diffuse, uv; 24 bytes). All lights share one vertex buffer (`+0x1c`, 4 vertices and 2 triangles per light) and
one draw. The texture is set per light in the loop, so the last light's texture applies to the whole batch (every
retail light uses `Light.dds`).

For each record, with `t = carClock - record[+0x20]`:

- **Fade:** if on, `fade = min(t / fadeIn, 1)`. If off and `t < fadeOut`, `fade = 1 - t / fadeOut`. If off and
  `t >= fadeOut`, the light is skipped.
- Position and normal are transformed by the car's world matrix.
- **Visibility:** two dot-product tests involving the camera must both be negative. The first uses the light normal
  against the camera direction. That dot, `facing = -dot`, is kept. The exact vectors in the second test haven't
  been pinned down.
- **Sprite:** a quad centred on `pos - 0.2 * toCamera` (constant 0x101b41e8). Its half-extents are the camera
  right and up axes (view matrix rows, `SShaderManagerImpl+0x440`) scaled by `Size * facing`. UVs are
  (0,0), (1,0), (0,1), (1,1).
- **Colour:** RGB from the record. Alpha = `(int)(facing * fade * 114.75)` (0x101b4398, 0.45 * 255), so a lamp
  pointing straight at the camera peaks at alpha 114.

The front lights are `Active = true` and have no fade keys, so they fade in over the default 1 s. Brake lights fade
in over 0.025 s and out over 0.05 s.

`RenderLightGlass` (0x100160c0) draws a second batch from the `+0x164` buffer with `Textures\reflection_flare.tga`,
but only when `CHH_Car+0x16c` is non-zero. `RenderLights` resets that counter to 0 and never increments it, so this
pass appears unused in 1.02 (not checked at runtime).


Which records switch. `OnCarLight` updates a record only while it walks the car's switch boxes: when a box is named
after the lamp (step 3 above), it looks up the record with `0x100182c0`, which returns the **first** record with the
key. The boxes are made only for `Switch = true` parts (`0x1001fde0` registers a `CGeomObjectSwitch` when `0x1001fc70`
reads `Switch` as true). So a record follows its lamp only when it is the first with its `Id` and a `Switch = true`
part has the lamp's box; every other record keeps its `Active` value and the switch time 0 from the load. Examples:
- the retail Lancer Evo VII has no `light_lf`/`light_rf` box, so its headlight flares stay on from `Active = true`
  even when the lamps break;
- the RSF cars' extra lamps (light pods `Point_light_bon1-4`, bumper lamps) reuse a headlight's `Id` (4 or 5) and
  are lit or dark by their `Active` alone; the Alpine A110 has no reverse boxes, so its reverse flares never light.
## Weather particles

On rain stages, `RBR_E21_Track.cpp:308` sets `g_rain_headlights_emitter`; on snow stages, `:323` sets
`g_snow_headlights_emitter`. The particle definition parser flags any emitter whose name contains `headlights`
(`RBR_E83_D.cpp:178`, `+0x104 = 1`). What that flag changes in emission hasn't been traced. No retail
`PS_*.ini` (12 files) or `Misc/track_w.ini` names a `headlights` emitter; only the dev tweak file
`Misc/particletweak.ini` lists `SNOWHEADLIGHTS`. So in 1.02 both lookups find nothing (`weather.md`).

## Implementation checklist for `rbr-render`

1. Parse `<car>_settings.ini` into the record above, using the defaults (`rbr_formats::ini` already reads it).
2. Keep a lamp state per `(kind, i)`: kinds 1 and 3 on from the start, kind 4 = brake input > 0.1, kind 5 =
   gear index 0, and all lamps off if the damage master or the lamp's own flag says so.
3. On every change, swap the `SwitchBox` part between `Predicate = off` and `on`, then restart the fade clock.
4. Draw the additive billboards after the car's shader lists. `CHH_Car::Render` (0x10015530) draws them in mode 1
   when `+0x1fc` is clear. Use the fade, facing and alpha rules above.

## Rust status (`rbr-render`, `lights.rs`)

- Live play (fx-1): the standalone sends the light system's words as PEV1 kind 6 (`live/picture.rs` `lights_event`):
  `car+0x838` -> `+8` is the state, kind k's switch at `+0x2c + 0x10(k-1)`; `+0x14` is the light block, lamp i's
  works and glass words at `+0x1c + 0x20(k-1) + 8i` and `+4`, the master word at `+0xdc`. The VCU setters run in
  the physics step, so the switches already carry the pedal swap and reverse. A lamp is lit when its kind's switch,
  the master word and its works bit are set (`lights::Lamps::lit`); its `lightglass` part and flare go once its
  glass bit clears. On tanner-damage the front lamp-1 words drop at step 3312 and lamps 0 and 1 at 5836, so the
  standalone's damage writes them; the writer was not found by an offset grep. `EngineGate` 0x4ef680 clears the
  master word and `CElectricalDamage` restores it.
- Playback without live data: front and tail lights always on; brake lights when the brake given to the drive
  (`CDrive+0x3c`, written by `Set30` 0x504450, = `CCar+0x4ec`, in the recorded car block) is above 0.1. All five
  `Set30` calls are on the VCU paths before the `Set1e40` gate, so this is the game's lamp value, pedal swap
  included (fx-2). It equals the control payload's brake word except where the drive brakes on its own: on
  `drive-v6-gravel-206-64` from frame 56021 the drive's brake is 1.0 with the pedal at 0. Recordings older than the
  car block fall back to word 1. No reverse and no lamp damage: the gear (`dri+0x54c`) and the lamp block are on
  the heap, outside the recording (`CCar+0x658` holds the gear's teeth from `M8480` 0x508480, not the gear index).
- Playback of a recording (fx-2, leftovers-1): the PEV1 kind-6 lights events of its replay set the lamps, reverse
  and lamp damage included (`Playback::set_recorded_lamps`). The viewer makes that sidecar itself, once per
  recording, with `rbr-standalone --boot --recording X --events-out FILE` (the recording's controls through the live
  boot, bit-exact: the run checks each step's car pose against the recording) and caches it outside the recordings
  (`rbr_render::particles::sidecar`; `RBR_PARTICLE_EVENTS` overrides it). No harness or format change is needed.
  Seen: `drive-tarmac-lancer` (41_M) reverses at steps 9917-10037 (kind-5 switch, white flares), and
  `drive-tanner-damage` breaks the front lamp 1 (kinds 1 and 2, works and glass) at step 3148. The alternative, carrying it in the recordings, is the harness recording
  the gear `dri+0x54c` and the light block (`CCar+0x838` → `+8` switches, the `+0x14` block's lamp words and
  master).
- Switch parts (`Switch = true` with a lamp's `SwitchBox`) are drawn in the state of their predicate, through the
  playback's per-part visibility. Start state: `Car::at_rest` (front and tail on, brake and reverse off).
- Flares: each `Light` record with a lamp kind (read as `InitLights` reads them) becomes a quad drawn additively
  (`DrawState::additive`) with the car's `Textures\Light.dds`. A record that never switches (above) keeps its
  `Active` value from the drive's start (`Light::fixed`; the game's clock starts at the load). Fades use the record's
  fade times and the time since the lamp last switched (looked back over the frames). Facing: `-dot(normal, forward) > 0` and the camera in front of the lamp face. Position
  `pos - 0.2 * forward` (our reading of `toCamera`).
- Not done: the second visibility vector of `RenderLights`; a lamp the physics `LightSystem` def lacks (`EmitLights`
  skips it, so its record keeps `Active`) still follows its kind here. Headlight light on the stage is the user's night mode
  (`--enhance night`), outside this path.

## Open

- Which function writes the per-lamp works/glass words on damage (they change in the standalone; see above).
- The second visibility vector in `RenderLights`.
- What the `headlights` flag does in the particle system.
- Where the exe uses the `"Headlights Force: %f Deviation: %f"` debug string (no code reference found).

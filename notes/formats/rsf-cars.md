# RallySimFans (RSF) NGP cars: what the game loads

How an RSF install turns a chosen car into the files RBR 1.02 + NGP 7.4.778 loads, from the sample install
(`rsf-samples`: `Cars/Cars.ini`, `Cars/evo_IX`, `rsfdata/cars` with 42 packs, `rsfdata/cache`, `Physics`). Nothing
from it is committed. Rust: `rbr_formats::rsf`, `rbr-standalone --rsf-car`, `viewer --rsf-car`, the launcher's RSF
field.

## The trace

1. RSF's cache lists the cars: `rsfdata/cache/cars.json` (102 cars: `id`, `name`, `path` = the pack folder,
   `carmodel_id`, `base_group_id`, `rev`, `ngp` = 6, `hash`, `custom_setups`, `audio`), `carmodels.json` (248 models:
   `id`, `name`, `path` = the `Cars/` folder, `filename`, `hash`, `fallback`), `cargroups.json` (group `id`, `name`)
   and `car_group_map.json` (car to group, many to many).
2. The chosen car's physics pack `rsfdata/cars/<path>/` (also kept as `<path>.zip`) is copied into one slot's physics
   folder. In the sample, car 24 (Mitsubishi Lancer Evo IX N4) is in slot 5: `Physics/s_i2003/` is byte-identical to
   `rsfdata/cars/Mitsubishi_Lancer_Evo_IX_N4_ngp6/` (`rsfdata/Physics/` is a copy of `Physics/`). The other slot
   folders hold the retail cars' files (the retail sizes; `physics.rbz` unpacked). `Physics/physics.lsp` is NGP's.
3. The slot's `[Car05]` in `Cars/Cars.ini` is rewritten to the car's model folder, with three keys of RSF's own:
   `CarName = Mitsubishi Lancer Evo IX N4`, `RSFCarID = 24`, `RSFCarPhysics = "rsfdata\cars\<pack>"`. The five retail
   keys follow the retail pattern from the model's `path` and `filename`:
   `FileName "Cars\evo_IX\evo_IX.sgc"`, `IniFile ...\evo_IX.ini`, `ShaderFile ...\evo_IX_shaders.ini`,
   `ShaderSettings ...\evo_IX_shader_settings`, `TexturePath "Cars\evo_IX\Textures\"`. The other seven slots stay
   retail.
4. The game loads slot 5 as a retail car: `g_carNames[5]` = `Physics/s_i2003/` for the physics files, Pirelli for the
   tyre brand (`LoadTyreBrand`: cars 5 and 7), `Cars.ini [Car05]` for the model. NGP reads the pack's `*_NGP` keys
   through its extended key tables (`notes/ngp-load-path.md`) and runs its per-car init on them.

Slot 5 is RSF's slot in the sample; whether RSF ever uses another slot is not known (one sample). NGP's own test
install (the user's) also races its cars in slot 5 (`ngp-load11c`: the Fiesta Rally3 in `s_i2003`).

`fallback` (ours, from the values): below 8 a retail slot whose model stands in when the model folder is missing
(Evo IX: 3, the Lancer Evo VII; BMW M3 E30: 5), else another model's id (Ford Focus 2006: 16). Not traced in RSF's
plugin.

## Which part is whose

| File | Format | Notes |
| --- | --- | --- |
| pack `common.lsp`, `gravel/tarmac/snow.lsp` | retail `.lsp` grammar, NGP keys | `CarDef`/`DriveDef`/`EngineDef`/... with `*_NGP` keys (`notes/formats/ngp-lsp.md`); parse with `lsp::parse` |
| pack `r_<surface>.lsp`, `setups/d_<surface>.lsp` | retail grammar, NGP keys | setup options and default setups (renamed stock keys, `notes/ngp-load-path.md`) |
| pack `repair.ini` | retail (`rbr_formats::repair`) | 76 parts, as seven retail cars |
| pack `<Car Name>` (no extension) | RSF / pack author | name, `revision N / date`, spec date, 3D model credit; first line read as the title |
| `Cars/<model>/<file>.sgc` | retail HedgeHog3D model, two community variants | damageable meshes end with a zero word after the morph lists; the node table has a duplicate id (`d_bonnet`); textures in DXT5 |
| `Cars/<model>/<file>.ini`, `_shaders.ini`, `_shader_settings_{clear,overcast}.ini` | retail | parts, cameras, materials |
| `Cars/<model>/<file>_settings.ini` | retail (`CHH_Car::InitLights`: `[LightN]` with `Id`, `Car Part`, `Active`, fades, colour; the `Use Lens Flare*` keys are never read) | read (`rbr_render::lights::flares`, see "Option folders") |
| `Cars/<model>/<file>_personal.ini` | RSF (the player's camera overrides, `[Cam_*]`) | not read |
| `Cars/<model>/carparts/<option>/` | RSF car-parts picker: `metadata.ini` (`[rsf_info]` `name` = category, `state`), `preview.jpg`, partial `<file>.ini` / `<file>_settings.ini` | not applied (see "Option folders") |
| `Cars/<model>/{light_on, light_off, Extralights, No Extralights, light_ramp_*, ini/settingschange/*}/` | the modder's option sets: whole `<file>.ini` / `_settings.ini` copies, or a `.txt` of sections to paste | not applied |
| `Cars/<model>/.rsfconfig` | RSF launcher (`[skin]`: `name`, `folder`, the selected skin) | read ([`rsf::selected_skin`]) |
| `Cars/<model>/skins/<skin>/` | RSF skin: `textures/*.dds` (retail DDS: DXT1/3/5), some `<model>_shader_settings_*.ini`, `metadata.ini` (`[rsf_info]`), `preview/`, `readme.txt` | the selected one is drawn over the model (see "Skins") |
| `Cars.ini` `CarName`, `RSFCarID`, `RSFCarPhysics` | RSF | ignored by the retail parser |
| `rsfdata/cache/*.json` | RSF | car list for the launcher |
| `Physics/physics.lsp` | NGP (= NGP 7.4.778's `physics.rbz` entry) | |

The 42 packs are byte-identical to `original/ngp/cars-rsf/` (the cars `notes/formats/ngp-lsp.md` inventoried). Two
are `_ngp7` (`Ford_Fiesta_Rally2_ngp7`, `Ford_Fiesta_Rally3_Evo_ngp7`); their zips use `\` separators.

### The model variants

- **Trailing word.** Every damageable mesh (subtype 3) of `evo_IX.sgc` ends with 4 zero bytes after the morph lists
  (72 of 72); retail has none. GeomCar (HH `0x1000acf0`) reads the record through a cursor and never checks its end,
  so the game ignores it; `sgc::parse` skips one zero word there.
- **Duplicate node id.** The LocoMotion node table (record 18/0) holds `d_bonnet` twice under id 2804. `LoadNodes`
  puts each node under `LM_FindNodeByUnk48(parent)` (LM `0x1002a4a0`): the first node with that id in a pop-last
  walk from the root. `rbr-standalone`'s handles do the same walk (retail ids are unique, so nothing changes there).
- **DXT5.** Four of the Evo IX's textures are DXT5 (BC3); retail only has DXT1 and DXT3. `dds` reads it and the
  renderer uploads it as BC3.
- **One mip level more than the header counts.** Some textures (all 2048x2048 ones of `Alpine_A110_2020`, the BMW
  M3 E30's `int3`, `lights`, `lights_lit`, `racingmirror`) hold the full chain but their header's mip count is one
  short. D3DX reads the counted levels and ignores the rest; `dds::parse` now does the same instead of rejecting the
  file (it drew the Alpine magenta).
- **An empty texture set header.** `Fiesta_rally3_shaders.ini` has `[TSDefault]` twice, the first empty.
  `car_parts` takes the set's entries from every section of that name (ours: the game's INI reader is not traced
  for duplicate sections; the car is drawn untextured otherwise).
- **Camera `Up`.** The BMW M3 E30's and Alpine's `[Cam_internal]` have `Up = 0 0.049 0.999` (retail `0 1 0`).
  `E09_BuildCamMatrix` (0x4663f0) never reads `Up`, so the viewer's fixed views are upright now (it rolled the BMW's
  cockpit on its side).
- **Hidden deforming panels.** The BMW M3 E30's deforming panels (`e_bonnet`, `e_bumper_f`, `e_door_b`, `e_wing`,
  ...) are `Switch = true` parts, hidden at the start; it draws rigid copies. So its body does not dent or scrape,
  in the game too (assumed: the switch rule is retail's); only window cracks show.
- **Not read:** `_personal.ini` cameras.

### Option folders and lights

**file** (samples): the game loads only the model folder's top-level `<file>.ini` and `<file>_settings.ini`; every
option folder is an alternative to them.
- `carparts/<option>/` (Alpine 14 options, BMW 32, Evo IX 22, Fiesta 5): each option's `.ini` files hold only the
  sections the option changes. Options of one category replace the same sections (rims: the wheel parts; headlights:
  the `e_light_*` parts and `Light0`-`Light8`), and the categories use disjoint section names (the Evo IX's
  extralights are `Light9`-`Light12`), so a selection reads as all chosen options' sections laid over the top-level
  files. That merge is our reading; RSF's picker is not traced, and where RSF keeps the selection is not known
  (`.rsfconfig` holds only `[skin]`). In the samples the top-level lights match none of the options.
- Lights that the options switch are `Light` records: an extra lamp (light pod, bumper lamps) reuses a headlight's
  `Id` and is on or off by its `Active` (`notes/car-lights.md`, "Which records switch"). "Lights off" options drop the
  headlight records or set the `e_light_*_lit` parts' `Predicate`.
- In the samples as installed: the Evo IX and BMW M3 E30 have no headlight records (no headlight flares; the Evo IX
  has no `light_lf`/`light_rf` box either), the Fiesta Rally3 has its four, and the Alpine has its four plus six
  unmounted extra lamps (`Active = false`, dark) and reverse records without a box (never lit).

### Skins

**file** (samples): the Fiesta Rally3 has three skins (`default`, `Pajari_Monte`, `Zasada_Rally3`) and a
`.rsfconfig` selecting `Pajari_Monte`; the BMW M3 E30 has `skins/default` and no `.rsfconfig`. Every skin file is a
plain retail DDS or INI (none RSF-encrypted).

- The model's `Textures\` is not a copy of any skin. The Fiesta's 18 textures are a fourth set (`Fiesta_rally3_external.dds`
  4096² DXT5, one level; `default`'s is 2048² DXT1, the two others' 4096² DXT3). Only `external_dynamic` equals the
  two named skins'. So RSF does not copy the selected skin into `Textures\`.
- `Pajari_Monte` holds 6 textures, `Zasada_Rally3` 6 and two shader settings files, `default` all 18 plus
  `settings/` (the model's `.ini` files). A skin of 6 textures needs the model's other 12.
- **Ours (inferred, RSF's plugin is not traced):** the selected skin's files replace the model folder's file of the
  same path below it (`textures/<name>.dds` for `Textures\<name>.dds`, `<model>_shader_settings_clear.ini` beside
  the model's): `Data::with_skin`, set by `rsf::select` from `.rsfconfig`. Without a `.rsfconfig` the model's own
  `Textures\` are drawn. `default/settings/` is not applied (it is a different layout; its use is not known).
- Not done: a skin picker (the launcher shows no skins; which skin a car without `.rsfconfig` shows in RSF is not
  known: `Textures\` or `skins/default`).


Sample model folders (`rsf-samples/Cars`): `evo_IX`, `Alpine_A110_2020`, `bmw_m3_e30` (lower case; paths resolve
ignoring case) and `Fiesta_rally3`; none holds an RSF-encrypted file.

## RSF-protected files

RSF encrypts some of its content (lead survey: 162 of 443 stage folders have files starting `01 a0 00 00`, `.ini`s
`01 a1 00 00`, high entropy). We never decrypt them. `rsf::encrypted` tells them by those four bytes: a pack file read
through `rsf::Pack::read` fails with "RSF-encrypted", and a model folder holding any such file makes the car
unloadable (`Install::model_entry`); the launcher lists those cars with the reason. In the sample install no pack and
no model file is encrypted (42 packs, folders and zips, and `Cars/evo_IX`).

## How we load it

Nothing is written into the game folders. `rbr_formats::rsf::select(data, key, slot)`:

- finds the car (a pack path, or an id or pack name in the install `RBR_RSF`),
- opens its pack (the folder, else the zip) as `rsf::Pack`,
- picks its model: its own folder if installed, else the `fallback` chain down to a retail slot,
- returns the `Data` with that model's entry in the slot (`Data::with_car`: `Data::car_slots` replaces the slot's
  `Cars.ini` entry; `Cars\` paths the install lacks resolve under the RSF root).

The standalone boot then reads the slot's physics files from the pack instead of `physics.rbz`
(`boot::pack_file`: names under the slot's folder), everything else as before, under `--ngp-physics`. The viewer
takes the same `Data`, so the model, its interior and its textures load through the existing `Cars.ini` paths.
